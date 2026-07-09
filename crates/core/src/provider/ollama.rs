// Caminho relativo: crates/core/src/provider/ollama.rs
//! Adapter Ollama (MT-08): primeiro provider real, local, sobre o Transporte
//! único (MT-07).
//!
//! Fala com a API REST do Ollama (`POST {base_url}/api/chat`) exclusivamente
//! através de [`Transport`] — este módulo nunca importa `reqwest` (ADR-0002);
//! toda chamada passa pela allowlist e pelo audit log já existentes, então
//! respeita a classe de egresso ativa (tipicamente `local-only`) sem lógica
//! adicional aqui.
//!
//! O formato de fio do Ollama (`OllamaMessage`, `OllamaToolCall` etc.) é
//! interno a este módulo — os tipos de domínio (`crate::model`) nunca vazam
//! o formato de um provider específico (MT-02).
//!
//! **Timeout adaptativo + `keep_alive` (MT-17, ADR-0009):** `ChatRequest::is_model_switch`
//! (propagado pelo Router via `Session`) decide entre [`DEFAULT_TIMEOUT_COLD`] (troca
//! de modelo — o Ollama precisa fazer um *cold load*, que pode ser bem mais lento que
//! uma inferência já aquecida) e [`DEFAULT_TIMEOUT_WARM`] (mesmo modelo); o
//! `keep_alive` (mais generoso que o *default* do próprio Ollama) é enviado em toda
//! chamada para reduzir descarregamento desnecessário do modelo entre trocas
//! frequentes de `task-class`. Ambos são constantes documentadas nesta v0.1 — exposição
//! via `settings-schema` fica para quando houver demanda real de ajuste por perfil
//! (mesmo padrão de outros defaults de provider, ex. `DEFAULT_MAX_TOKENS` do MT-16).
//!
//! **Saída estruturada para tool-calling (MT-22, ADR-0012):** quando `tools` não
//! está vazio e [`OllamaProvider::structured_output`] está ativo (*default*: `true`,
//! ajustável via [`OllamaProvider::with_structured_output`]), o campo `format` da API
//! do Ollama recebe um JSON Schema combinado das `tools` — um `oneOf` de
//! `{name: <const>, arguments: <input_schema da tool>}`, restringindo a geração da
//! porção de tool-call ao formato esperado. Fiação da flag com o `settings-schema`
//! (`providers.ollama.structured_output`) fica para quando o restante do
//! `settings-schema` de providers existir — mesmo adiamento já aplicado ao MT-17/MT-16.

use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::{ContentBlock, Message, Role, StreamEvent, ToolCall, Usage};
use crate::provider::{
    BoxFuture, ChatRequest, ChatResponse, ChatStream, EmbeddingsRequest, EmbeddingsResponse,
    LlmProvider, ProviderError, ToolSpec,
};
use crate::transport::{Transport, TransportError};

/// Timeout para chamadas ao mesmo modelo já carregado (MT-17, ADR-0009) —
/// generoso o bastante para inferência local, mas falha rápido numa conexão
/// genuinamente travada.
const DEFAULT_TIMEOUT_WARM: Duration = Duration::from_secs(30);
/// Timeout para chamadas que implicam troca de modelo — carregar um modelo
/// de 8B–30B do disco pode levar bem mais que uma inferência já aquecida
/// (MT-17, ADR-0009).
const DEFAULT_TIMEOUT_COLD: Duration = Duration::from_secs(300);
/// `keep_alive` enviado em toda chamada (MT-17, ADR-0009) — mais generoso
/// que o *default* do próprio Ollama (5m), para reduzir descarregamento
/// desnecessário do modelo entre trocas frequentes de `task-class`.
const DEFAULT_KEEP_ALIVE: &str = "30m";

/// Adapter para a API de chat do Ollama.
pub struct OllamaProvider {
    transport: Arc<Transport>,
    base_url: String,
    /// Saída estruturada (MT-22, ADR-0012) — *default* `true`.
    structured_output: bool,
}

impl OllamaProvider {
    /// Cria um adapter apontando para `base_url` (ex.: `http://localhost:11434`),
    /// com saída estruturada ativada por padrão (ver [`Self::with_structured_output`]).
    #[must_use]
    pub fn new(transport: Arc<Transport>, base_url: impl Into<String>) -> Self {
        Self {
            transport,
            base_url: base_url.into(),
            structured_output: true,
        }
    }

    /// Ativa/desativa a saída estruturada (`format`) para tool-calling
    /// (MT-22, ADR-0012) — desligar pode ser necessário para depuração ou
    /// modelos/versões do Ollama que não suportem bem o recurso.
    #[must_use]
    pub fn with_structured_output(mut self, structured_output: bool) -> Self {
        self.structured_output = structured_output;
        self
    }

    fn chat_url(&self) -> String {
        format!("{}/api/chat", self.base_url.trim_end_matches('/'))
    }
}

/// Timeout adaptativo para a chamada (MT-17, ADR-0009): [`DEFAULT_TIMEOUT_COLD`]
/// se a resolução implica troca de modelo, [`DEFAULT_TIMEOUT_WARM`] caso contrário.
fn timeout_for(request: &ChatRequest) -> Duration {
    if request.is_model_switch {
        DEFAULT_TIMEOUT_COLD
    } else {
        DEFAULT_TIMEOUT_WARM
    }
}

// ---- Formato de fio da API Ollama (interno; não confundir com `crate::model`) ----

#[derive(Serialize)]
struct OllamaRequest<'a> {
    model: &'a str,
    messages: Vec<OllamaMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OllamaTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    options: Option<OllamaOptions>,
    /// Ativa/desativa raciocínio estendido (MT-32, ADR-0014) — campo de
    /// nível superior na API do Ollama, fora de `options`.
    #[serde(skip_serializing_if = "Option::is_none")]
    think: Option<bool>,
    /// Enviado em toda chamada (MT-17, ADR-0009) — nunca omitido.
    keep_alive: &'static str,
    /// JSON Schema combinado das `tools` (MT-22, ADR-0012) — presente só
    /// quando há `tools` **e** a saída estruturada está ativa.
    #[serde(skip_serializing_if = "Option::is_none")]
    format: Option<serde_json::Value>,
}

#[derive(Serialize, Default)]
struct OllamaOptions {
    #[serde(skip_serializing_if = "Option::is_none")]
    num_predict: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

impl OllamaOptions {
    /// Constrói as opções a partir do `ChatRequest`, ou `None` se nenhum
    /// parâmetro (MT-31, ADR-0008) estiver definido.
    fn from_request(request: &ChatRequest) -> Option<Self> {
        if request.max_tokens.is_none() && request.temperature.is_none() && request.top_p.is_none()
        {
            return None;
        }
        Some(Self {
            num_predict: request.max_tokens,
            temperature: request.temperature,
            top_p: request.top_p,
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct OllamaMessage {
    role: String,
    #[serde(default)]
    content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OllamaToolCall>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OllamaToolCall {
    function: OllamaFunctionCall,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OllamaFunctionCall {
    name: String,
    arguments: serde_json::Value,
}

#[derive(Serialize)]
struct OllamaTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OllamaToolFunction,
}

#[derive(Serialize)]
struct OllamaToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// Um "chunk" de `/api/chat`: idêntico em formato para resposta completa
/// (`done: true` já na primeira e única mensagem) ou para cada linha NDJSON
/// de uma resposta em streaming.
#[derive(Deserialize, Debug, Default)]
struct OllamaChatChunk {
    #[serde(default)]
    message: OllamaMessage,
    #[serde(default)]
    done: bool,
    #[serde(default)]
    prompt_eval_count: u64,
    #[serde(default)]
    eval_count: u64,
}

// ---- Conversões de/para os tipos de domínio (`crate::model`) ----

fn role_to_ollama(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

/// Achata os blocos de uma [`Message`] no formato de mensagem única do
/// Ollama: texto e resultado de tool viram `content` (concatenados por
/// `\n`); chamadas de tool viram `tool_calls`.
fn message_to_ollama(message: &Message) -> OllamaMessage {
    let mut content = String::new();
    let mut tool_calls = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text } => {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(text);
            }
            ContentBlock::ToolCall(chamada) => tool_calls.push(OllamaToolCall {
                function: OllamaFunctionCall {
                    name: chamada.name.clone(),
                    arguments: chamada.arguments.clone(),
                },
            }),
            ContentBlock::ToolResult(resultado) => {
                if !content.is_empty() {
                    content.push('\n');
                }
                content.push_str(&resultado.content);
            }
        }
    }
    OllamaMessage {
        role: role_to_ollama(message.role).to_string(),
        content,
        tool_calls,
    }
}

fn tool_spec_to_ollama(spec: &ToolSpec) -> OllamaTool {
    OllamaTool {
        kind: "function",
        function: OllamaToolFunction {
            name: spec.name.clone(),
            description: spec.description.clone(),
            parameters: spec.input_schema.clone(),
        },
    }
}

/// Constrói o JSON Schema combinado das `tools` para o campo `format`
/// (MT-22, ADR-0012): um `oneOf` de objetos `{name: <const>, arguments:
/// <input_schema da tool>}` — representa "uma destas chamadas de tool",
/// restringindo a geração da porção de tool-call ao formato esperado.
fn combined_tools_format(tools: &[ToolSpec]) -> serde_json::Value {
    let alternativas: Vec<serde_json::Value> = tools
        .iter()
        .map(|spec| {
            serde_json::json!({
                "type": "object",
                "properties": {
                    "name": { "const": spec.name },
                    "arguments": spec.input_schema,
                },
                "required": ["name", "arguments"],
            })
        })
        .collect();

    serde_json::json!({ "oneOf": alternativas })
}

/// Converte a mensagem final do Ollama em [`Message`] de domínio,
/// sintetizando um `id` sequencial para cada `tool_call` — o Ollama não
/// devolve identificador próprio para chamadas de tool.
fn ollama_message_to_domain(msg: &OllamaMessage) -> Message {
    let mut blocks = Vec::new();
    if !msg.content.is_empty() {
        blocks.push(ContentBlock::Text {
            text: msg.content.clone(),
        });
    }
    for (indice, chamada) in msg.tool_calls.iter().enumerate() {
        blocks.push(ContentBlock::ToolCall(ToolCall {
            id: format!("ollama-call-{indice}"),
            name: chamada.function.name.clone(),
            arguments: chamada.function.arguments.clone(),
        }));
    }
    Message {
        role: Role::Assistant,
        content: blocks,
    }
}

fn build_request<'a>(
    request: &'a ChatRequest,
    stream: bool,
    structured_output: bool,
) -> OllamaRequest<'a> {
    let format = if structured_output && !request.tools.is_empty() {
        Some(combined_tools_format(&request.tools))
    } else {
        None
    };
    OllamaRequest {
        model: &request.model,
        messages: request.messages.iter().map(message_to_ollama).collect(),
        stream,
        tools: request.tools.iter().map(tool_spec_to_ollama).collect(),
        options: OllamaOptions::from_request(request),
        think: request.reasoning,
        keep_alive: DEFAULT_KEEP_ALIVE,
        format,
    }
}

fn map_transport_error(err: TransportError) -> ProviderError {
    match err {
        TransportError::InvalidUrl(msg) => ProviderError::InvalidResponse(msg),
        TransportError::Blocked(egress_err) => {
            ProviderError::Network(format!("egresso bloqueado: {egress_err}"))
        }
        TransportError::Http(msg) => ProviderError::Network(msg),
    }
}

impl LlmProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn chat(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatResponse, ProviderError>> {
        Box::pin(async move {
            let timeout = timeout_for(&request);
            let body = serde_json::to_value(build_request(&request, false, self.structured_output))
                .expect("OllamaRequest sempre serializável");
            let resposta = self
                .transport
                .post_json(&self.chat_url(), "chat", &body, Some(timeout))
                .await
                .map_err(map_transport_error)?;

            let chunk: OllamaChatChunk = serde_json::from_value(resposta)
                .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

            Ok(ChatResponse {
                message: ollama_message_to_domain(&chunk.message),
                usage: Usage {
                    input_tokens: chunk.prompt_eval_count,
                    output_tokens: chunk.eval_count,
                },
            })
        })
    }

    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxFuture<'_, Result<ChatStream, ProviderError>> {
        Box::pin(async move {
            let timeout = timeout_for(&request);
            let body = serde_json::to_value(build_request(&request, true, self.structured_output))
                .expect("OllamaRequest sempre serializável");
            let mut linhas = self
                .transport
                .post_json_lines(&self.chat_url(), "chat_stream", &body, Some(timeout))
                .await
                .map_err(map_transport_error)?;

            let (tx, rx) = tokio::sync::mpsc::channel(16);
            tokio::spawn(async move {
                if tx.send(Ok(StreamEvent::MessageStart)).await.is_err() {
                    return;
                }

                let mut proximo_id = 0usize;
                while let Some(linha) = linhas.recv().await {
                    let linha = match linha {
                        Ok(l) => l,
                        Err(e) => {
                            let _ = tx.send(Err(map_transport_error(e))).await;
                            return;
                        }
                    };
                    let chunk: OllamaChatChunk = match serde_json::from_str(&linha) {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = tx
                                .send(Err(ProviderError::InvalidResponse(e.to_string())))
                                .await;
                            return;
                        }
                    };

                    if !chunk.message.content.is_empty() {
                        let evento = StreamEvent::TextDelta {
                            text: chunk.message.content.clone(),
                        };
                        if tx.send(Ok(evento)).await.is_err() {
                            return;
                        }
                    }

                    for chamada in &chunk.message.tool_calls {
                        let id = format!("ollama-call-{proximo_id}");
                        proximo_id += 1;
                        let inicio = StreamEvent::ToolCallStart {
                            id: id.clone(),
                            name: chamada.function.name.clone(),
                        };
                        if tx.send(Ok(inicio)).await.is_err() {
                            return;
                        }
                        let delta = StreamEvent::ToolCallDelta {
                            id,
                            delta: chamada.function.arguments.to_string(),
                        };
                        if tx.send(Ok(delta)).await.is_err() {
                            return;
                        }
                    }

                    if chunk.done {
                        let fim = StreamEvent::MessageEnd {
                            usage: Usage {
                                input_tokens: chunk.prompt_eval_count,
                                output_tokens: chunk.eval_count,
                            },
                        };
                        let _ = tx.send(Ok(fim)).await;
                        return;
                    }
                }
            });

            Ok(rx)
        })
    }

    fn embeddings(
        &self,
        _request: EmbeddingsRequest,
    ) -> BoxFuture<'_, Result<EmbeddingsResponse, ProviderError>> {
        Box::pin(async move {
            Err(ProviderError::Unsupported(
                "OllamaProvider ainda não implementa /api/embed (fora do escopo do MT-08)".into(),
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::config::privacy::EgressClass;
    use crate::egress::allowlist::{Allowlist, AllowlistEntry};
    use crate::egress::audit::AuditEntry;
    use crate::transport::AuditSink;

    /// Sink de teste que descarta as entradas — os testes deste módulo já
    /// são cobertos quanto a auditoria em `transport::tests`.
    struct NoopSink;
    impl AuditSink for NoopSink {
        fn record(&self, _entry: AuditEntry) {}
    }

    /// Mesma técnica de mock HTTP mínimo do MT-07 (só `tokio::net`, sem lib
    /// de mock nova): sobe um servidor que sempre devolve o corpo fixo dado.
    async fn start_mock_server(
        response_body: &'static str,
    ) -> (std::net::SocketAddr, Arc<AtomicUsize>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");
        let conexoes = Arc::new(AtomicUsize::new(0));
        let contador = Arc::clone(&conexoes);

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                contador.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = socket.read(&mut buf).await;
                    let resposta = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = socket.write_all(resposta.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        (addr, conexoes)
    }

    fn transport_local_only(addr: std::net::SocketAddr) -> Arc<Transport> {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            addr.ip().to_string(),
            EgressClass::LocalOnly,
        )]);
        Arc::new(Transport::new(
            allowlist,
            EgressClass::LocalOnly,
            Some("empresa".into()),
            Arc::new(NoopSink),
        ))
    }

    #[tokio::test]
    async fn chat_via_transporte_retorna_mensagem_e_uso() {
        let (addr, _conexoes) = start_mock_server(
            r#"{"message":{"role":"assistant","content":"Olá!"},"done":true,"prompt_eval_count":5,"eval_count":7}"#,
        )
        .await;
        let provider = OllamaProvider::new(transport_local_only(addr), format!("http://{addr}"));

        let resposta = provider
            .chat(ChatRequest::new("llama3.1:8b", vec![Message::user("oi")]))
            .await
            .expect("chat deve funcionar via Transporte");

        assert_eq!(resposta.message, Message::assistant("Olá!"));
        assert_eq!(resposta.usage.input_tokens, 5);
        assert_eq!(resposta.usage.output_tokens, 7);
    }

    #[tokio::test]
    async fn chat_com_tool_call_sintetiza_id() {
        let (addr, _conexoes) = start_mock_server(
            r#"{"message":{"role":"assistant","content":"","tool_calls":[{"function":{"name":"fs_read","arguments":{"path":"Cargo.toml"}}}]},"done":true}"#,
        )
        .await;
        let provider = OllamaProvider::new(transport_local_only(addr), format!("http://{addr}"));

        let resposta = provider
            .chat(ChatRequest::new(
                "llama3.1:8b",
                vec![Message::user("leia o manifesto")],
            ))
            .await
            .expect("chat deve funcionar via Transporte");

        assert_eq!(
            resposta.message.content,
            vec![ContentBlock::ToolCall(ToolCall {
                id: "ollama-call-0".into(),
                name: "fs_read".into(),
                arguments: serde_json::json!({"path": "Cargo.toml"}),
            })]
        );
    }

    #[tokio::test]
    async fn chat_stream_via_transporte_entrega_eventos_em_ordem() {
        let corpo = "\
            {\"message\":{\"role\":\"assistant\",\"content\":\"ola\"},\"done\":false}\n\
            {\"message\":{\"role\":\"assistant\",\"content\":\" mundo\"},\"done\":false}\n\
            {\"message\":{\"role\":\"assistant\",\"content\":\"\"},\"done\":true,\"prompt_eval_count\":3,\"eval_count\":4}\n";
        let (addr, _conexoes) = start_mock_server(corpo).await;
        let provider = OllamaProvider::new(transport_local_only(addr), format!("http://{addr}"));

        let mut stream = provider
            .chat_stream(ChatRequest::new("llama3.1:8b", vec![Message::user("oi")]))
            .await
            .expect("stream deve funcionar via Transporte");

        let mut eventos = Vec::new();
        while let Some(evento) = stream.recv().await {
            eventos.push(evento.expect("mock não produz erro"));
        }

        assert_eq!(
            eventos,
            vec![
                StreamEvent::MessageStart,
                StreamEvent::TextDelta { text: "ola".into() },
                StreamEvent::TextDelta {
                    text: " mundo".into()
                },
                StreamEvent::MessageEnd {
                    usage: Usage {
                        input_tokens: 3,
                        output_tokens: 4
                    }
                },
            ]
        );
    }

    #[tokio::test]
    async fn chat_respeita_local_only_e_bloqueia_sem_tocar_a_rede() {
        let (addr, conexoes) = start_mock_server(r#"{"done":true}"#).await;
        // Allowlist cadastra o host, mas exigindo uma classe de nuvem — a
        // sessão local-only não deve conseguir alcançá-lo.
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            addr.ip().to_string(),
            EgressClass::CloudOk,
        )]);
        let transport = Arc::new(Transport::new(
            allowlist,
            EgressClass::LocalOnly,
            Some("empresa".into()),
            Arc::new(NoopSink),
        ));
        let provider = OllamaProvider::new(transport, format!("http://{addr}"));

        let erro = provider
            .chat(ChatRequest::new("llama3.1:8b", vec![Message::user("oi")]))
            .await
            .expect_err("sessão local-only não deve alcançar host cloud-ok");

        assert!(matches!(erro, ProviderError::Network(_)));
        assert_eq!(
            conexoes.load(Ordering::SeqCst),
            0,
            "nenhuma conexão deveria ter sido aberta"
        );
    }

    #[test]
    fn build_request_inclui_temperature_e_top_p_quando_definidos() {
        let mut request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        request.temperature = Some(0.3);
        request.top_p = Some(0.8);
        request.max_tokens = Some(100);

        let ollama_request = build_request(&request, false, true);
        let json = serde_json::to_value(&ollama_request).expect("deve serializar");

        // Compara contra o mesmo caminho f32→f64 do serde_json, evitando
        // falso-negativo por imprecisão de largura de ponto flutuante.
        assert_eq!(json["options"]["temperature"], serde_json::json!(0.3_f32));
        assert_eq!(json["options"]["top_p"], serde_json::json!(0.8_f32));
        assert_eq!(json["options"]["num_predict"], 100);
    }

    #[test]
    fn build_request_omite_options_sem_nenhum_parametro() {
        let request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        let ollama_request = build_request(&request, false, true);
        let json = serde_json::to_value(&ollama_request).expect("deve serializar");

        assert!(
            json.get("options").is_none(),
            "options não deveria aparecer sem nenhum parâmetro definido"
        );
    }

    #[test]
    fn build_request_inclui_think_quando_reasoning_definido() {
        let mut request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        request.reasoning = Some(true);

        let ollama_request = build_request(&request, false, true);
        let json = serde_json::to_value(&ollama_request).expect("deve serializar");

        assert_eq!(json["think"], true);
    }

    #[test]
    fn build_request_omite_think_sem_reasoning_definido() {
        let request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        let ollama_request = build_request(&request, false, true);
        let json = serde_json::to_value(&ollama_request).expect("deve serializar");

        assert!(
            json.get("think").is_none(),
            "think não deveria aparecer sem reasoning definido"
        );
    }

    #[test]
    fn build_request_sempre_envia_keep_alive() {
        let request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        let ollama_request = build_request(&request, false, true);
        let json = serde_json::to_value(&ollama_request).expect("deve serializar");

        assert_eq!(json["keep_alive"], DEFAULT_KEEP_ALIVE);
    }

    fn tool_spec_de_teste() -> ToolSpec {
        ToolSpec {
            name: "fs_read".into(),
            description: "lê um arquivo".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        }
    }

    #[test]
    fn build_request_inclui_format_quando_ha_tools_e_saida_estruturada_ativa() {
        let mut request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        request.tools = vec![tool_spec_de_teste()];

        let ollama_request = build_request(&request, false, true);
        let json = serde_json::to_value(&ollama_request).expect("deve serializar");

        let alternativas = json["format"]["oneOf"]
            .as_array()
            .expect("format.oneOf deve ser um array");
        assert_eq!(alternativas.len(), 1);
        assert_eq!(alternativas[0]["properties"]["name"]["const"], "fs_read");
        assert_eq!(
            alternativas[0]["properties"]["arguments"]["required"][0],
            "path"
        );
    }

    #[test]
    fn build_request_omite_format_sem_tools() {
        let request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        let ollama_request = build_request(&request, false, true);
        let json = serde_json::to_value(&ollama_request).expect("deve serializar");

        assert!(
            json.get("format").is_none(),
            "format não deveria aparecer sem tools, mesmo com saída estruturada ativa"
        );
    }

    #[test]
    fn build_request_omite_format_quando_saida_estruturada_desativada() {
        let mut request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        request.tools = vec![tool_spec_de_teste()];

        let ollama_request = build_request(&request, false, false);
        let json = serde_json::to_value(&ollama_request).expect("deve serializar");

        assert!(
            json.get("format").is_none(),
            "format não deveria aparecer com a flag desativada, mesmo havendo tools"
        );
    }

    #[tokio::test]
    async fn chat_com_tools_envia_format_via_transporte_respeitando_a_flag() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        async fn requisicao_capturada(structured_output: bool) -> String {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .expect("bind em porta efêmera deve funcionar");
            let addr = listener
                .local_addr()
                .expect("socket deve ter endereço local");
            let corpo_capturado = Arc::new(std::sync::Mutex::new(String::new()));
            let alvo = Arc::clone(&corpo_capturado);

            tokio::spawn(async move {
                if let Ok((mut socket, _)) = listener.accept().await {
                    let mut buf = [0u8; 4096];
                    if let Ok(n) = socket.read(&mut buf).await {
                        *alvo.lock().expect("mutex não deve envenenar") =
                            String::from_utf8_lossy(&buf[..n]).into_owned();
                    }
                    let resposta_json =
                        r#"{"message":{"role":"assistant","content":"ok"},"done":true}"#;
                    let resposta = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                        resposta_json.len(),
                        resposta_json
                    );
                    let _ = socket.write_all(resposta.as_bytes()).await;
                    let _ = socket.shutdown().await;
                }
            });

            let provider =
                OllamaProvider::new(transport_local_only(addr), format!("http://{addr}"))
                    .with_structured_output(structured_output);

            let mut request = ChatRequest::new("llama3.1:8b", vec![Message::user("oi")]);
            request.tools = vec![tool_spec_de_teste()];
            provider
                .chat(request)
                .await
                .expect("chat deve funcionar via Transporte");

            let resultado = corpo_capturado
                .lock()
                .expect("mutex não deve envenenar")
                .clone();
            resultado
        }

        let com_estruturada = requisicao_capturada(true).await;
        assert!(
            com_estruturada.contains("\"format\":{\"oneOf\""),
            "esperava format no corpo com saída estruturada ativa; recebido:\n{com_estruturada}"
        );

        let sem_estruturada = requisicao_capturada(false).await;
        assert!(
            !sem_estruturada.contains("\"format\""),
            "não esperava format no corpo com a flag desativada; recebido:\n{sem_estruturada}"
        );
    }

    #[test]
    fn timeout_for_escolhe_quente_sem_troca_de_modelo() {
        let request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        assert_eq!(timeout_for(&request), DEFAULT_TIMEOUT_WARM);
    }

    #[test]
    fn timeout_for_escolhe_frio_com_troca_de_modelo() {
        let mut request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        request.is_model_switch = true;
        assert_eq!(timeout_for(&request), DEFAULT_TIMEOUT_COLD);
    }

    #[tokio::test]
    async fn chat_com_troca_de_modelo_envia_keep_alive_via_transporte() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");
        let corpo_capturado = Arc::new(std::sync::Mutex::new(String::new()));
        let alvo = Arc::clone(&corpo_capturado);

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                if let Ok(n) = socket.read(&mut buf).await {
                    *alvo.lock().expect("mutex não deve envenenar") =
                        String::from_utf8_lossy(&buf[..n]).into_owned();
                }
                let resposta_json =
                    r#"{"message":{"role":"assistant","content":"ok"},"done":true}"#;
                let resposta = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    resposta_json.len(),
                    resposta_json
                );
                let _ = socket.write_all(resposta.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        struct NoopSink;
        impl crate::transport::AuditSink for NoopSink {
            fn record(&self, _entry: crate::egress::audit::AuditEntry) {}
        }
        let allowlist = crate::egress::allowlist::Allowlist::new(vec![
            crate::egress::allowlist::AllowlistEntry::new(
                addr.ip().to_string(),
                crate::config::privacy::EgressClass::LocalOnly,
            ),
        ]);
        let transport = Arc::new(Transport::new(
            allowlist,
            crate::config::privacy::EgressClass::LocalOnly,
            None,
            Arc::new(NoopSink),
        ));
        let provider = OllamaProvider::new(transport, format!("http://{addr}"));

        let mut request = ChatRequest::new("llama3.1:8b", vec![Message::user("oi")]);
        request.is_model_switch = true;
        provider
            .chat(request)
            .await
            .expect("chat deve funcionar via Transporte");

        let requisicao_bruta = corpo_capturado
            .lock()
            .expect("mutex não deve envenenar")
            .clone();
        assert!(
            requisicao_bruta.contains(&format!("\"keep_alive\":\"{DEFAULT_KEEP_ALIVE}\"")),
            "esperava keep_alive no corpo da requisição; recebido:\n{requisicao_bruta}"
        );
    }
}
