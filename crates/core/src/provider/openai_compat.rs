// Caminho relativo: crates/core/src/provider/openai_compat.rs
//! Adapter OpenAI-compatible (MT-15): cobre vLLM, OpenRouter e gateways
//! LiteLLM, que expõem a mesma API `POST {base_url}/chat/completions` sobre
//! o Transporte único (MT-07) — este módulo nunca importa `reqwest`
//! (ADR-0002); toda chamada passa pela allowlist e pelo audit log já
//! existentes, então respeita a classe de egresso ativa da sessão sem lógica
//! adicional aqui.
//!
//! Autenticação via chave de API (`Authorization: Bearer`, necessária para
//! OpenRouter/gateways LiteLLM em nuvem) é responsabilidade do
//! [`crate::transport::Transport`] injetado (`Transport::with_header`) —
//! este adapter não guarda nem manuseia a chave.
//!
//! **ADR-0006 (LiteLLM):** um endpoint de proxy/gateway só é alcançável se
//! declarado na allowlist do [`Transport`] com uma classe de egresso
//! explícita; a ausência de declaração é fail-closed (bloqueado), nunca
//! inferida do host — este módulo não trata `localhost` como caso especial.
//!
//! O formato de fio da API OpenAI (`OpenAiMessage`, `OpenAiToolCall` etc.) é
//! interno a este módulo — os tipos de domínio (`crate::model`) nunca vazam
//! o formato de um provider específico (MT-02).

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::model::{ContentBlock, Message, Role, StreamEvent, ToolCall, Usage};
use crate::provider::{
    BoxFuture, ChatRequest, ChatResponse, ChatStream, EmbeddingsRequest, EmbeddingsResponse,
    LlmProvider, ProviderError, ToolSpec,
};
use crate::transport::{Transport, TransportError};

/// Adapter para a API de chat OpenAI-compatible.
pub struct OpenAiCompatProvider {
    transport: Arc<Transport>,
    base_url: String,
    /// Nome deste provider no Router (MT-09) — cada instância aponta para um
    /// endpoint diferente (vLLM local, OpenRouter, gateway LiteLLM), então o
    /// nome não é fixo como no `OllamaProvider`.
    name: String,
}

impl OpenAiCompatProvider {
    /// Cria um adapter apontando para `base_url` (ex.:
    /// `https://openrouter.ai/api/v1`, `http://localhost:8000/v1`), com
    /// `name` identificando esta instância no Router.
    #[must_use]
    pub fn new(
        transport: Arc<Transport>,
        base_url: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            transport,
            base_url: base_url.into(),
            name: name.into(),
        }
    }

    fn chat_url(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }
}

// ---- Formato de fio da API OpenAI (interno; não confundir com `crate::model`) ----

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    messages: Vec<OpenAiMessage>,
    stream: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream_options: Option<StreamOptions>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<OpenAiTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
}

/// Sem isto, a API OpenAI (e gateways que a espelham, ex.: LiteLLM) **não**
/// inclui `usage` em nenhum chunk do streaming — só `include_usage: true`
/// pede o chunk final extra com o total. Omitido em requisições
/// não-streaming, onde `usage` já vem sempre na resposta única.
#[derive(Serialize)]
struct StreamOptions {
    include_usage: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default)]
struct OpenAiMessage {
    role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tool_calls: Vec<OpenAiToolCall>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type", default = "default_function_kind")]
    kind: String,
    function: OpenAiFunctionCall,
}

fn default_function_kind() -> String {
    "function".to_string()
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct OpenAiFunctionCall {
    name: String,
    /// A API OpenAI representa argumentos como uma **string** contendo JSON
    /// (diferente do Ollama, que usa um objeto JSON aninhado).
    arguments: String,
}

#[derive(Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    kind: &'static str,
    function: OpenAiToolFunction,
}

#[derive(Serialize)]
struct OpenAiToolFunction {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

#[derive(Deserialize, Debug, Default)]
struct OpenAiChatResponse {
    #[serde(default)]
    choices: Vec<OpenAiChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize, Debug, Default)]
struct OpenAiChoice {
    #[serde(default)]
    message: OpenAiMessage,
}

#[derive(Deserialize, Debug, Default, Clone, Copy)]
struct OpenAiUsage {
    #[serde(default)]
    prompt_tokens: u64,
    #[serde(default)]
    completion_tokens: u64,
}

impl From<OpenAiUsage> for Usage {
    fn from(u: OpenAiUsage) -> Self {
        Self {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
        }
    }
}

// ---- Formato de fio do streaming SSE (`data: {...}` por linha) ----

#[derive(Deserialize, Debug, Default)]
struct OpenAiStreamChunk {
    #[serde(default)]
    choices: Vec<OpenAiStreamChoice>,
    #[serde(default)]
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize, Debug, Default)]
struct OpenAiStreamChoice {
    #[serde(default)]
    delta: OpenAiStreamDelta,
}

#[derive(Deserialize, Debug, Default)]
struct OpenAiStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiToolCallDelta>,
}

#[derive(Deserialize, Debug, Default)]
struct OpenAiToolCallDelta {
    index: usize,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    function: Option<OpenAiFunctionCallDelta>,
}

#[derive(Deserialize, Debug, Default)]
struct OpenAiFunctionCallDelta {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    arguments: Option<String>,
}

// ---- Conversões de/para os tipos de domínio (`crate::model`) ----

fn role_to_openai(role: Role) -> &'static str {
    match role {
        Role::System => "system",
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::Tool => "tool",
    }
}

/// Converte uma [`Message`] de domínio em uma ou mais [`OpenAiMessage`].
///
/// Papel `Tool`: cada [`ContentBlock::ToolResult`] vira **uma mensagem
/// própria** (a API OpenAI exige `tool_call_id` por mensagem — diferente do
/// Ollama, que aceita concatenar tudo numa só). Demais papéis: texto
/// concatenado em `content`, chamadas de tool coletadas em `tool_calls`
/// (só relevante para `Assistant`).
fn message_to_openai(message: &Message) -> Vec<OpenAiMessage> {
    if message.role == Role::Tool {
        return message
            .content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::ToolResult(resultado) => Some(OpenAiMessage {
                    role: "tool".to_string(),
                    content: Some(resultado.content.clone()),
                    tool_calls: Vec::new(),
                    tool_call_id: Some(resultado.call_id.clone()),
                }),
                _ => None,
            })
            .collect();
    }

    let mut texto = String::new();
    let mut tool_calls = Vec::new();
    for block in &message.content {
        match block {
            ContentBlock::Text { text } => {
                if !texto.is_empty() {
                    texto.push('\n');
                }
                texto.push_str(text);
            }
            ContentBlock::ToolCall(chamada) => tool_calls.push(OpenAiToolCall {
                id: chamada.id.clone(),
                kind: default_function_kind(),
                function: OpenAiFunctionCall {
                    name: chamada.name.clone(),
                    arguments: chamada.arguments.to_string(),
                },
            }),
            ContentBlock::ToolResult(_) => {
                // Não deveria ocorrer fora de `Role::Tool`; nada a converter.
            }
        }
    }

    vec![OpenAiMessage {
        role: role_to_openai(message.role).to_string(),
        content: if texto.is_empty() && !tool_calls.is_empty() {
            None
        } else {
            Some(texto)
        },
        tool_calls,
        tool_call_id: None,
    }]
}

fn tool_spec_to_openai(spec: &ToolSpec) -> OpenAiTool {
    OpenAiTool {
        kind: "function",
        function: OpenAiToolFunction {
            name: spec.name.clone(),
            description: spec.description.clone(),
            parameters: spec.input_schema.clone(),
        },
    }
}

fn build_request<'a>(request: &'a ChatRequest, stream: bool) -> OpenAiRequest<'a> {
    OpenAiRequest {
        model: &request.model,
        messages: request
            .messages
            .iter()
            .flat_map(message_to_openai)
            .collect(),
        stream,
        stream_options: stream.then_some(StreamOptions {
            include_usage: true,
        }),
        tools: request.tools.iter().map(tool_spec_to_openai).collect(),
        max_tokens: request.max_tokens,
        temperature: request.temperature,
        top_p: request.top_p,
    }
}

/// Converte a mensagem final da resposta OpenAI em [`Message`] de domínio.
///
/// # Errors
///
/// Devolve [`ProviderError::InvalidResponse`] se `arguments` de algum
/// `tool_call` não for JSON válido.
fn openai_message_to_domain(msg: &OpenAiMessage) -> Result<Message, ProviderError> {
    let mut blocks = Vec::new();
    if let Some(texto) = &msg.content {
        if !texto.is_empty() {
            blocks.push(ContentBlock::Text {
                text: texto.clone(),
            });
        }
    }
    for chamada in &msg.tool_calls {
        let arguments = serde_json::from_str(&chamada.function.arguments).map_err(|e| {
            ProviderError::InvalidResponse(format!("tool_call.arguments inválido: {e}"))
        })?;
        blocks.push(ContentBlock::ToolCall(ToolCall {
            id: chamada.id.clone(),
            name: chamada.function.name.clone(),
            arguments,
        }));
    }
    Ok(Message {
        role: Role::Assistant,
        content: blocks,
    })
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

impl LlmProvider for OpenAiCompatProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn chat(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatResponse, ProviderError>> {
        Box::pin(async move {
            let body = serde_json::to_value(build_request(&request, false))
                .expect("OpenAiRequest sempre serializável");
            let resposta = self
                .transport
                .post_json(&self.chat_url(), "chat", &body, None)
                .await
                .map_err(map_transport_error)?;

            let parsed: OpenAiChatResponse = serde_json::from_value(resposta)
                .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;
            let choice = parsed
                .choices
                .into_iter()
                .next()
                .ok_or_else(|| ProviderError::InvalidResponse("resposta sem choices".into()))?;

            Ok(ChatResponse {
                message: openai_message_to_domain(&choice.message)?,
                usage: parsed.usage.map(Usage::from).unwrap_or_default(),
            })
        })
    }

    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxFuture<'_, Result<ChatStream, ProviderError>> {
        Box::pin(async move {
            let body = serde_json::to_value(build_request(&request, true))
                .expect("OpenAiRequest sempre serializável");
            let mut linhas = self
                .transport
                // Sem timeout adaptativo: o sinal de troca de modelo do ADR-0009/MT-17
                // é escopo só do provider local (Ollama) — endpoints gerenciados
                // não sofrem com cold load de modelo.
                .post_json_lines(&self.chat_url(), "chat_stream", &body, None)
                .await
                .map_err(map_transport_error)?;

            let (tx, rx) = tokio::sync::mpsc::channel(16);
            tokio::spawn(async move {
                if tx.send(Ok(StreamEvent::MessageStart)).await.is_err() {
                    return;
                }

                let mut indice_para_id: HashMap<usize, String> = HashMap::new();
                let mut ultimo_uso = Usage::default();

                while let Some(linha) = linhas.recv().await {
                    let linha = match linha {
                        Ok(l) => l,
                        Err(e) => {
                            let _ = tx.send(Err(map_transport_error(e))).await;
                            return;
                        }
                    };

                    let Some(dados) = linha.strip_prefix("data:") else {
                        continue; // outros campos de SSE (ex.: `event:`) são ignorados
                    };
                    let dados = dados.trim();
                    if dados == "[DONE]" {
                        break;
                    }

                    let chunk: OpenAiStreamChunk = match serde_json::from_str(dados) {
                        Ok(c) => c,
                        Err(e) => {
                            let _ = tx
                                .send(Err(ProviderError::InvalidResponse(e.to_string())))
                                .await;
                            return;
                        }
                    };

                    if let Some(uso) = chunk.usage {
                        ultimo_uso = uso.into();
                    }

                    for choice in &chunk.choices {
                        if let Some(texto) = &choice.delta.content {
                            if !texto.is_empty()
                                && tx
                                    .send(Ok(StreamEvent::TextDelta {
                                        text: texto.clone(),
                                    }))
                                    .await
                                    .is_err()
                            {
                                return;
                            }
                        }

                        for delta in &choice.delta.tool_calls {
                            let id = if let Some(id) = &delta.id {
                                let nome = delta
                                    .function
                                    .as_ref()
                                    .and_then(|f| f.name.clone())
                                    .unwrap_or_default();
                                indice_para_id.insert(delta.index, id.clone());
                                if tx
                                    .send(Ok(StreamEvent::ToolCallStart {
                                        id: id.clone(),
                                        name: nome,
                                    }))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                                id.clone()
                            } else if let Some(id) = indice_para_id.get(&delta.index) {
                                id.clone()
                            } else {
                                continue; // fragmento sem id conhecido; nada a fazer
                            };

                            if let Some(argumentos) =
                                delta.function.as_ref().and_then(|f| f.arguments.clone())
                            {
                                if !argumentos.is_empty()
                                    && tx
                                        .send(Ok(StreamEvent::ToolCallDelta {
                                            id,
                                            delta: argumentos,
                                        }))
                                        .await
                                        .is_err()
                                {
                                    return;
                                }
                            }
                        }
                    }
                }

                let _ = tx
                    .send(Ok(StreamEvent::MessageEnd { usage: ultimo_uso }))
                    .await;
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
                "OpenAiCompatProvider ainda não implementa /embeddings (fora do escopo do MT-15)"
                    .into(),
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

    /// Sink de teste que descarta as entradas — auditoria já coberta em
    /// `transport::tests`.
    struct NoopSink;
    impl AuditSink for NoopSink {
        fn record(&self, _entry: AuditEntry) {}
    }

    /// Mesma técnica de mock HTTP mínimo do MT-07/MT-08 (só `tokio::net`,
    /// sem lib de mock nova): sobe um servidor que sempre devolve o corpo
    /// fixo dado.
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

    /// Como [`start_mock_server`], mas também captura os bytes brutos da
    /// primeira requisição recebida — mesma técnica de
    /// `transport::tests::start_mock_server_capturando`, usada aqui para
    /// provar o corpo da requisição (`stream_options`), não um header.
    async fn start_mock_server_capturando(
        response_body: &'static str,
    ) -> (std::net::SocketAddr, Arc<std::sync::Mutex<Vec<u8>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");
        let capturado = Arc::new(std::sync::Mutex::new(Vec::new()));
        let alvo = Arc::clone(&capturado);

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                if let Ok(n) = socket.read(&mut buf).await {
                    alvo.lock()
                        .expect("mutex de captura não deve envenenar")
                        .extend_from_slice(&buf[..n]);
                }
                let resposta = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = socket.write_all(resposta.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        (addr, capturado)
    }

    fn transport_com_classe(addr: std::net::SocketAddr, classe: EgressClass) -> Arc<Transport> {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(addr.ip().to_string(), classe)]);
        Arc::new(Transport::new(
            allowlist,
            classe,
            Some("pessoal".into()),
            Arc::new(NoopSink),
        ))
    }

    #[tokio::test]
    async fn chat_via_transporte_retorna_mensagem_e_uso() {
        let (addr, _conexoes) = start_mock_server(
            r#"{"choices":[{"message":{"role":"assistant","content":"Olá!"}}],"usage":{"prompt_tokens":5,"completion_tokens":7}}"#,
        )
        .await;
        let provider = OpenAiCompatProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
            "vllm-local",
        );

        let resposta = provider
            .chat(ChatRequest::new("qwen2.5-coder", vec![Message::user("oi")]))
            .await
            .expect("chat deve funcionar via Transporte");

        assert_eq!(resposta.message, Message::assistant("Olá!"));
        assert_eq!(resposta.usage.input_tokens, 5);
        assert_eq!(resposta.usage.output_tokens, 7);
    }

    #[tokio::test]
    async fn chat_com_tool_call_preserva_id_e_decodifica_argumentos() {
        let (addr, _conexoes) = start_mock_server(
            r#"{"choices":[{"message":{"role":"assistant","content":null,"tool_calls":[{"id":"call_abc","type":"function","function":{"name":"fs_read","arguments":"{\"path\":\"Cargo.toml\"}"}}]}}]}"#,
        )
        .await;
        let provider = OpenAiCompatProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
            "vllm-local",
        );

        let resposta = provider
            .chat(ChatRequest::new(
                "qwen2.5-coder",
                vec![Message::user("leia o manifesto")],
            ))
            .await
            .expect("chat deve funcionar via Transporte");

        assert_eq!(
            resposta.message.content,
            vec![ContentBlock::ToolCall(ToolCall {
                id: "call_abc".into(),
                name: "fs_read".into(),
                arguments: serde_json::json!({"path": "Cargo.toml"}),
            })]
        );
    }

    #[tokio::test]
    async fn chat_stream_via_transporte_entrega_texto_e_tool_call_em_ordem() {
        let corpo = "\
            data: {\"choices\":[{\"delta\":{\"content\":\"ola\"}}]}\n\
            data: {\"choices\":[{\"delta\":{\"content\":\" mundo\"}}]}\n\
            data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"fs_read\",\"arguments\":\"\"}}]}}]}\n\
            data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"path\\\"\"}}]}}]}\n\
            data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\":\\\"a.txt\\\"}\"}}]}}]}\n\
            data: {\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4},\"choices\":[]}\n\
            data: [DONE]\n";
        let (addr, _conexoes) = start_mock_server(corpo).await;
        let provider = OpenAiCompatProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
            "vllm-local",
        );

        let mut stream = provider
            .chat_stream(ChatRequest::new("qwen2.5-coder", vec![Message::user("oi")]))
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
                StreamEvent::ToolCallStart {
                    id: "call_1".into(),
                    name: "fs_read".into(),
                },
                StreamEvent::ToolCallDelta {
                    id: "call_1".into(),
                    delta: "{\"path\"".into(),
                },
                StreamEvent::ToolCallDelta {
                    id: "call_1".into(),
                    delta: ":\"a.txt\"}".into(),
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
    async fn chat_stream_pede_stream_options_include_usage() {
        // Sem `stream_options: {include_usage: true}` no corpo, um gateway
        // OpenAI-compatible de verdade (LiteLLM, vLLM) nunca manda `usage`
        // em nenhum chunk do streaming — o mock deste teste nem chega a
        // devolver `usage` (só MessageStart/[DONE]); o que importa é o
        // corpo que o provider realmente mandou.
        let (addr, capturado) =
            start_mock_server_capturando("data: {\"choices\":[]}\ndata: [DONE]\n").await;
        let provider = OpenAiCompatProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
            "vllm-local",
        );

        let mut stream = provider
            .chat_stream(ChatRequest::new("qwen2.5-coder", vec![Message::user("oi")]))
            .await
            .expect("stream deve funcionar via Transporte");
        while stream.recv().await.is_some() {}

        let requisicao_bruta =
            String::from_utf8_lossy(&capturado.lock().expect("mutex não deve envenenar"))
                .into_owned();
        assert!(
            requisicao_bruta.contains(r#""stream_options":{"include_usage":true}"#),
            "esperava stream_options:{{include_usage:true}} no corpo da requisição \
             de streaming; recebido:\n{requisicao_bruta}"
        );
    }

    #[tokio::test]
    async fn chat_nao_stream_nao_manda_stream_options() {
        // Requisição não-streaming já recebe `usage` sempre, sem precisar
        // pedir nada extra — `stream_options` deve ficar de fora do corpo.
        let (addr, capturado) = start_mock_server_capturando(
            r#"{"choices":[{"message":{"role":"assistant","content":"oi"}}]}"#,
        )
        .await;
        let provider = OpenAiCompatProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
            "vllm-local",
        );

        provider
            .chat(ChatRequest::new("qwen2.5-coder", vec![Message::user("oi")]))
            .await
            .expect("chat deve funcionar via Transporte");

        let requisicao_bruta =
            String::from_utf8_lossy(&capturado.lock().expect("mutex não deve envenenar"))
                .into_owned();
        assert!(
            !requisicao_bruta.contains("stream_options"),
            "requisição não-streaming não deveria mandar stream_options; recebido:\n{requisicao_bruta}"
        );
    }

    #[tokio::test]
    async fn chat_respeita_cloud_opt_out_e_bloqueia_sem_tocar_a_rede() {
        let (addr, conexoes) = start_mock_server(r#"{"choices":[]}"#).await;
        // Allowlist cadastra o host, mas exigindo uma classe que a sessão
        // ativa (`cloud-opt-out`) não cobre.
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            addr.ip().to_string(),
            EgressClass::CloudOk,
        )]);
        let transport = Arc::new(Transport::new(
            allowlist,
            EgressClass::CloudOptOut,
            Some("externo-confidencial".into()),
            Arc::new(NoopSink),
        ));
        let provider = OpenAiCompatProvider::new(transport, format!("http://{addr}"), "openrouter");

        let erro = provider
            .chat(ChatRequest::new("qwen2.5-coder", vec![Message::user("oi")]))
            .await
            .expect_err("sessão cloud-opt-out não deve alcançar host cloud-ok");

        assert!(matches!(erro, ProviderError::Network(_)));
        assert_eq!(
            conexoes.load(Ordering::SeqCst),
            0,
            "nenhuma conexão deveria ter sido aberta"
        );
    }

    /// ADR-0006: endpoint de proxy/gateway (ex.: LiteLLM) com classe
    /// **declarada** funciona sob a classe correspondente.
    #[tokio::test]
    async fn endpoint_litellm_com_classe_declarada_funciona() {
        let (addr, _conexoes) =
            start_mock_server(r#"{"choices":[{"message":{"role":"assistant","content":"ok"}}]}"#)
                .await;
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            addr.ip().to_string(),
            EgressClass::CloudOk,
        )]);
        let transport = Arc::new(Transport::new(
            allowlist,
            EgressClass::CloudOk,
            Some("pessoal".into()),
            Arc::new(NoopSink),
        ));
        let provider =
            OpenAiCompatProvider::new(transport, format!("http://{addr}"), "litellm-gateway");

        let resposta = provider
            .chat(ChatRequest::new(
                "qualquer-modelo",
                vec![Message::user("oi")],
            ))
            .await
            .expect("endpoint com classe declarada deve funcionar");

        assert_eq!(resposta.message, Message::assistant("ok"));
    }

    /// ADR-0006: endpoint de proxy/gateway **sem** classe declarada (fora da
    /// allowlist) é bloqueado em perfil restritivo — fail-closed, mesmo que
    /// o host seja tecnicamente local.
    #[tokio::test]
    async fn endpoint_litellm_sem_classe_declarada_e_bloqueado_em_perfil_restritivo() {
        let (addr, conexoes) = start_mock_server(r#"{"choices":[]}"#).await;
        // Allowlist vazia: nenhuma classe foi declarada para este host.
        let transport = Arc::new(Transport::new(
            Allowlist::new(vec![]),
            EgressClass::LocalOnly,
            Some("empresa".into()),
            Arc::new(NoopSink),
        ));
        let provider =
            OpenAiCompatProvider::new(transport, format!("http://{addr}"), "litellm-gateway");

        let erro = provider
            .chat(ChatRequest::new(
                "qualquer-modelo",
                vec![Message::user("oi")],
            ))
            .await
            .expect_err("endpoint sem classe declarada deve ser bloqueado (fail-closed)");

        assert!(matches!(erro, ProviderError::Network(_)));
        assert_eq!(
            conexoes.load(Ordering::SeqCst),
            0,
            "nenhuma conexão deveria ter sido aberta para endpoint não declarado"
        );
    }

    #[test]
    fn build_request_inclui_temperature_top_p_e_max_tokens_quando_definidos() {
        let mut request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        request.temperature = Some(0.3);
        request.top_p = Some(0.8);
        request.max_tokens = Some(100);

        let openai_request = build_request(&request, false);
        let json = serde_json::to_value(&openai_request).expect("deve serializar");

        assert_eq!(json["temperature"], serde_json::json!(0.3_f32));
        assert_eq!(json["top_p"], serde_json::json!(0.8_f32));
        assert_eq!(json["max_tokens"], 100);
    }

    #[test]
    fn build_request_omite_parametros_ausentes() {
        let request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        let openai_request = build_request(&request, false);
        let json = serde_json::to_value(&openai_request).expect("deve serializar");

        assert!(json.get("temperature").is_none());
        assert!(json.get("top_p").is_none());
        assert!(json.get("max_tokens").is_none());
    }

    #[test]
    fn message_to_openai_expande_multiplos_tool_results_em_mensagens_separadas() {
        let mensagem = Message {
            role: Role::Tool,
            content: vec![
                ContentBlock::ToolResult(crate::model::ToolResult {
                    call_id: "call_1".into(),
                    content: "resultado 1".into(),
                    is_error: false,
                }),
                ContentBlock::ToolResult(crate::model::ToolResult {
                    call_id: "call_2".into(),
                    content: "resultado 2".into(),
                    is_error: false,
                }),
            ],
        };

        let convertidas = message_to_openai(&mensagem);

        assert_eq!(convertidas.len(), 2);
        assert_eq!(convertidas[0].tool_call_id.as_deref(), Some("call_1"));
        assert_eq!(convertidas[0].content.as_deref(), Some("resultado 1"));
        assert_eq!(convertidas[1].tool_call_id.as_deref(), Some("call_2"));
        assert_eq!(convertidas[1].content.as_deref(), Some("resultado 2"));
    }
}
