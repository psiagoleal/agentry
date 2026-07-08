// Caminho relativo: crates/core/src/provider/anthropic.rs
//! Adapter Anthropic (MT-16): Messages API (`POST {base_url}/v1/messages`),
//! *tool use* e streaming SSE, sobre o Transporte único (MT-07) — este
//! módulo nunca importa `reqwest` (ADR-0002); toda chamada passa pela
//! allowlist e pelo audit log já existentes, então respeita a classe de
//! egresso ativa da sessão (tipicamente `cloud-ok`) sem lógica adicional
//! aqui.
//!
//! Autenticação é responsabilidade do [`crate::transport::Transport`]
//! injetado: a Messages API exige os headers `x-api-key: <chave>` e
//! `anthropic-version: <versão>` em toda requisição (diferente do padrão
//! `Authorization: Bearer` do MT-15) — anexados via
//! `Transport::with_header` por quem monta o transporte; este adapter não
//! guarda nem manuseia a chave.
//!
//! **Diferença estrutural do Ollama/OpenAI-compatible:** a Messages API não
//! tem papel `system` nem `tool` na lista de mensagens — o prompt de sistema
//! é um campo `system` de nível superior (mensagens `Role::System` são
//! extraídas do histórico e concatenadas nele) e resultado de tool é um
//! bloco `tool_result` **dentro** de uma mensagem de papel `user` (não uma
//! mensagem própria) — ao contrário do OpenAI, aqui múltiplos
//! `ContentBlock::ToolResult` no mesmo `Message` de domínio cabem em uma
//! única `AnthropicMessage` (blocos, não mensagens, carregam a correlação).
//!
//! `max_tokens` é campo obrigatório da API; na ausência de um valor no
//! `ChatRequest`, usa-se [`DEFAULT_MAX_TOKENS`]. Raciocínio estendido
//! (MT-32/ADR-0014, `reasoning: Some(true)`) traduz para o campo `thinking`
//! nativo (`{"type":"enabled","budget_tokens":..}`); blocos de resposta que
//! esse modo produz (`thinking`/`redacted_thinking`) são reconhecidos mas
//! descartados na conversão para o tipo de domínio — o `StreamEvent` do
//! MT-02 não tem uma variante de raciocínio para carregá-los.
//!
//! O formato de fio da API Anthropic (`AnthropicMessage`, `AnthropicContentBlock`
//! etc.) é interno a este módulo — os tipos de domínio (`crate::model`) nunca
//! vazam o formato de um provider específico (MT-02).

use std::collections::HashMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::model::{ContentBlock, Message, Role, StreamEvent, ToolCall, Usage};
use crate::provider::{
    BoxFuture, ChatRequest, ChatResponse, ChatStream, EmbeddingsRequest, EmbeddingsResponse,
    LlmProvider, ProviderError, ToolSpec,
};
use crate::transport::{Transport, TransportError};

/// `max_tokens` é obrigatório na Messages API; usado quando o `ChatRequest`
/// não define um valor.
const DEFAULT_MAX_TOKENS: u32 = 4096;
/// Orçamento de tokens de raciocínio quando `reasoning` está ativo e nenhum
/// controle mais fino é exposto (fora de escopo do MT-16) — valor mínimo
/// documentado pela API.
const DEFAULT_THINKING_BUDGET_TOKENS: u32 = 1024;

/// Adapter para a Messages API da Anthropic.
pub struct AnthropicProvider {
    transport: Arc<Transport>,
    base_url: String,
}

impl AnthropicProvider {
    /// Cria um adapter apontando para `base_url` (ex.:
    /// `https://api.anthropic.com`).
    #[must_use]
    pub fn new(transport: Arc<Transport>, base_url: impl Into<String>) -> Self {
        Self {
            transport,
            base_url: base_url.into(),
        }
    }

    fn messages_url(&self) -> String {
        format!("{}/v1/messages", self.base_url.trim_end_matches('/'))
    }
}

// ---- Formato de fio da Messages API (interno; não confundir com `crate::model`) ----

#[derive(Serialize)]
struct AnthropicRequest<'a> {
    model: &'a str,
    max_tokens: u32,
    messages: Vec<AnthropicMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    stream: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tools: Vec<AnthropicTool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    top_p: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thinking: Option<AnthropicThinking>,
}

#[derive(Serialize)]
struct AnthropicThinking {
    #[serde(rename = "type")]
    kind: &'static str,
    budget_tokens: u32,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct AnthropicMessage {
    role: String,
    content: Vec<AnthropicContentBlock>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// Bloco de conteúdo, compartilhado entre requisição e resposta.
///
/// `ToolResult` só aparece do lado da requisição (construído por este
/// adapter); `Other` captura tipos que a resposta pode trazer e que este
/// adapter não precisa interpretar (`thinking`, `redacted_thinking`) — sem
/// isso, uma resposta com raciocínio estendido falharia ao desserializar.
#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
        #[serde(default, skip_serializing_if = "is_false")]
        is_error: bool,
    },
    #[serde(other)]
    Other,
}

#[derive(Serialize)]
struct AnthropicTool {
    name: String,
    description: String,
    input_schema: serde_json::Value,
}

#[derive(Deserialize, Debug, Default)]
struct AnthropicChatResponse {
    #[serde(default)]
    content: Vec<AnthropicContentBlock>,
    #[serde(default)]
    usage: AnthropicUsage,
}

#[derive(Deserialize, Debug, Default, Clone, Copy)]
struct AnthropicUsage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

impl From<AnthropicUsage> for Usage {
    fn from(u: AnthropicUsage) -> Self {
        Self {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
        }
    }
}

// ---- Formato de fio do streaming SSE (`data: {...}` por linha) ----

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamEvent {
    MessageStart {
        message: AnthropicStreamMessageStart,
    },
    ContentBlockStart {
        index: usize,
        content_block: AnthropicStreamContentBlockStart,
    },
    ContentBlockDelta {
        index: usize,
        delta: AnthropicStreamDelta,
    },
    ContentBlockStop {
        #[allow(dead_code)]
        index: usize,
    },
    MessageDelta {
        #[serde(default)]
        usage: AnthropicUsage,
    },
    MessageStop,
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug, Default)]
struct AnthropicStreamMessageStart {
    #[serde(default)]
    usage: AnthropicUsage,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamContentBlockStart {
    Text {
        #[serde(default)]
        #[allow(dead_code)]
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
    },
    #[serde(other)]
    Other,
}

#[derive(Deserialize, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AnthropicStreamDelta {
    TextDelta {
        text: String,
    },
    InputJsonDelta {
        partial_json: String,
    },
    #[serde(other)]
    Other,
}

// ---- Conversões de/para os tipos de domínio (`crate::model`) ----

fn tool_spec_to_anthropic(spec: &ToolSpec) -> AnthropicTool {
    AnthropicTool {
        name: spec.name.clone(),
        description: spec.description.clone(),
        input_schema: spec.input_schema.clone(),
    }
}

/// Converte o histórico de domínio no par (`system`, `messages`) que a
/// Messages API espera: mensagens `Role::System` são extraídas do
/// histórico e concatenadas em `system` (nunca aparecem em `messages`).
fn convert_messages(messages: &[Message]) -> (Option<String>, Vec<AnthropicMessage>) {
    let mut system = String::new();
    let mut convertidas = Vec::new();

    for message in messages {
        if message.role == Role::System {
            for block in &message.content {
                if let ContentBlock::Text { text } = block {
                    if !system.is_empty() {
                        system.push('\n');
                    }
                    system.push_str(text);
                }
            }
            continue;
        }
        convertidas.push(message_to_anthropic(message));
    }

    let system = if system.is_empty() {
        None
    } else {
        Some(system)
    };
    (system, convertidas)
}

/// Converte uma [`Message`] de domínio (papel `User`/`Assistant`/`Tool`) em
/// uma [`AnthropicMessage`]. `Role::Tool` vira papel `user` — a Messages API
/// não tem papel de tool próprio, resultado de tool é um bloco `tool_result`
/// dentro de uma mensagem `user`.
fn message_to_anthropic(message: &Message) -> AnthropicMessage {
    let role = match message.role {
        Role::User | Role::Tool => "user",
        Role::Assistant => "assistant",
        Role::System => unreachable!("mensagens de sistema são filtradas em convert_messages"),
    };

    let content = message
        .content
        .iter()
        .map(|block| match block {
            ContentBlock::Text { text } => AnthropicContentBlock::Text { text: text.clone() },
            ContentBlock::ToolCall(chamada) => AnthropicContentBlock::ToolUse {
                id: chamada.id.clone(),
                name: chamada.name.clone(),
                input: chamada.arguments.clone(),
            },
            ContentBlock::ToolResult(resultado) => AnthropicContentBlock::ToolResult {
                tool_use_id: resultado.call_id.clone(),
                content: resultado.content.clone(),
                is_error: resultado.is_error,
            },
        })
        .collect();

    AnthropicMessage {
        role: role.to_string(),
        content,
    }
}

fn build_request<'a>(request: &'a ChatRequest, stream: bool) -> AnthropicRequest<'a> {
    let (system, messages) = convert_messages(&request.messages);
    AnthropicRequest {
        model: &request.model,
        max_tokens: request.max_tokens.unwrap_or(DEFAULT_MAX_TOKENS),
        messages,
        system,
        stream,
        tools: request.tools.iter().map(tool_spec_to_anthropic).collect(),
        temperature: request.temperature,
        top_p: request.top_p,
        thinking: request.reasoning.and_then(|ligado| {
            ligado.then_some(AnthropicThinking {
                kind: "enabled",
                budget_tokens: DEFAULT_THINKING_BUDGET_TOKENS,
            })
        }),
    }
}

/// Converte os blocos de conteúdo de uma resposta em [`Message`] de domínio.
///
/// # Errors
///
/// Devolve [`ProviderError::InvalidResponse`] se `arguments` de algum
/// `tool_use` não puder ser reaproveitado como JSON (na prática, sempre é —
/// o campo já chega desserializado como `serde_json::Value`; o `Result`
/// existe para simetria com o adapter OpenAI-compatible e para acomodar
/// validações futuras sem mudar a assinatura).
fn anthropic_content_to_domain(blocks: &[AnthropicContentBlock]) -> Message {
    let mut domain_blocks = Vec::new();
    for block in blocks {
        match block {
            AnthropicContentBlock::Text { text } => {
                domain_blocks.push(ContentBlock::Text { text: text.clone() });
            }
            AnthropicContentBlock::ToolUse { id, name, input } => {
                domain_blocks.push(ContentBlock::ToolCall(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments: input.clone(),
                }));
            }
            AnthropicContentBlock::ToolResult { .. } | AnthropicContentBlock::Other => {
                // `ToolResult` nunca vem do modelo; `Other` cobre blocos de
                // raciocínio estendido (`thinking`/`redacted_thinking`), sem
                // representação no `StreamEvent`/`ContentBlock` do MT-02.
            }
        }
    }
    Message {
        role: Role::Assistant,
        content: domain_blocks,
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

impl LlmProvider for AnthropicProvider {
    fn name(&self) -> &str {
        "anthropic"
    }

    fn chat(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatResponse, ProviderError>> {
        Box::pin(async move {
            let body = serde_json::to_value(build_request(&request, false))
                .expect("AnthropicRequest sempre serializável");
            let resposta = self
                .transport
                .post_json(&self.messages_url(), "chat", &body)
                .await
                .map_err(map_transport_error)?;

            let parsed: AnthropicChatResponse = serde_json::from_value(resposta)
                .map_err(|e| ProviderError::InvalidResponse(e.to_string()))?;

            Ok(ChatResponse {
                message: anthropic_content_to_domain(&parsed.content),
                usage: parsed.usage.into(),
            })
        })
    }

    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxFuture<'_, Result<ChatStream, ProviderError>> {
        Box::pin(async move {
            let body = serde_json::to_value(build_request(&request, true))
                .expect("AnthropicRequest sempre serializável");
            let mut linhas = self
                .transport
                .post_json_lines(&self.messages_url(), "chat_stream", &body)
                .await
                .map_err(map_transport_error)?;

            let (tx, rx) = tokio::sync::mpsc::channel(16);
            tokio::spawn(async move {
                if tx.send(Ok(StreamEvent::MessageStart)).await.is_err() {
                    return;
                }

                let mut indice_para_id: HashMap<usize, String> = HashMap::new();
                let mut usage = Usage::default();

                while let Some(linha) = linhas.recv().await {
                    let linha = match linha {
                        Ok(l) => l,
                        Err(e) => {
                            let _ = tx.send(Err(map_transport_error(e))).await;
                            return;
                        }
                    };

                    let Some(dados) = linha.strip_prefix("data:") else {
                        continue; // linhas `event: ...` são ignoradas; o `type` já vem no payload
                    };
                    let dados = dados.trim();

                    let evento: AnthropicStreamEvent = match serde_json::from_str(dados) {
                        Ok(e) => e,
                        Err(e) => {
                            let _ = tx
                                .send(Err(ProviderError::InvalidResponse(e.to_string())))
                                .await;
                            return;
                        }
                    };

                    match evento {
                        AnthropicStreamEvent::MessageStart { message } => {
                            usage.input_tokens = message.usage.input_tokens;
                        }
                        AnthropicStreamEvent::ContentBlockStart {
                            index,
                            content_block,
                        } => {
                            if let AnthropicStreamContentBlockStart::ToolUse { id, name } =
                                content_block
                            {
                                indice_para_id.insert(index, id.clone());
                                if tx
                                    .send(Ok(StreamEvent::ToolCallStart { id, name }))
                                    .await
                                    .is_err()
                                {
                                    return;
                                }
                            }
                        }
                        AnthropicStreamEvent::ContentBlockDelta { index, delta } => match delta {
                            AnthropicStreamDelta::TextDelta { text } => {
                                if !text.is_empty()
                                    && tx.send(Ok(StreamEvent::TextDelta { text })).await.is_err()
                                {
                                    return;
                                }
                            }
                            AnthropicStreamDelta::InputJsonDelta { partial_json } => {
                                if let Some(id) = indice_para_id.get(&index) {
                                    if !partial_json.is_empty()
                                        && tx
                                            .send(Ok(StreamEvent::ToolCallDelta {
                                                id: id.clone(),
                                                delta: partial_json,
                                            }))
                                            .await
                                            .is_err()
                                    {
                                        return;
                                    }
                                }
                            }
                            AnthropicStreamDelta::Other => {}
                        },
                        AnthropicStreamEvent::ContentBlockStop { .. } => {}
                        AnthropicStreamEvent::MessageDelta { usage: delta_usage } => {
                            usage.output_tokens = delta_usage.output_tokens;
                        }
                        AnthropicStreamEvent::MessageStop => {
                            let _ = tx.send(Ok(StreamEvent::MessageEnd { usage })).await;
                            return;
                        }
                        AnthropicStreamEvent::Other => {} // ex.: `ping`
                    }
                }

                let _ = tx.send(Ok(StreamEvent::MessageEnd { usage })).await;
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
                "AnthropicProvider não implementa embeddings — a Anthropic não oferece essa API"
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
    use crate::model::ToolResult;
    use crate::transport::AuditSink;

    /// Sink de teste que descarta as entradas — auditoria já coberta em
    /// `transport::tests`.
    struct NoopSink;
    impl AuditSink for NoopSink {
        fn record(&self, _entry: AuditEntry) {}
    }

    /// Mesma técnica de mock HTTP mínimo do MT-07/08/15 (só `tokio::net`,
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

    fn transport_com_classe(addr: std::net::SocketAddr, classe: EgressClass) -> Arc<Transport> {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(addr.ip().to_string(), classe)]);
        Arc::new(
            Transport::new(
                allowlist,
                classe,
                Some("pessoal".into()),
                Arc::new(NoopSink),
            )
            .with_header("x-api-key", "chave-de-teste")
            .with_header("anthropic-version", "2023-06-01"),
        )
    }

    #[tokio::test]
    async fn chat_via_transporte_retorna_mensagem_e_uso() {
        let (addr, _conexoes) = start_mock_server(
            r#"{"content":[{"type":"text","text":"Olá!"}],"usage":{"input_tokens":5,"output_tokens":7}}"#,
        )
        .await;
        let provider = AnthropicProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
        );

        let resposta = provider
            .chat(ChatRequest::new("claude-sonnet", vec![Message::user("oi")]))
            .await
            .expect("chat deve funcionar via Transporte");

        assert_eq!(resposta.message, Message::assistant("Olá!"));
        assert_eq!(resposta.usage.input_tokens, 5);
        assert_eq!(resposta.usage.output_tokens, 7);
    }

    #[tokio::test]
    async fn chat_com_tool_use_preserva_id_e_input() {
        let (addr, _conexoes) = start_mock_server(
            r#"{"content":[{"type":"tool_use","id":"toolu_01","name":"fs_read","input":{"path":"Cargo.toml"}}],"usage":{"input_tokens":1,"output_tokens":1}}"#,
        )
        .await;
        let provider = AnthropicProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
        );

        let resposta = provider
            .chat(ChatRequest::new(
                "claude-sonnet",
                vec![Message::user("leia o manifesto")],
            ))
            .await
            .expect("chat deve funcionar via Transporte");

        assert_eq!(
            resposta.message.content,
            vec![ContentBlock::ToolCall(ToolCall {
                id: "toolu_01".into(),
                name: "fs_read".into(),
                arguments: serde_json::json!({"path": "Cargo.toml"}),
            })]
        );
    }

    #[tokio::test]
    async fn chat_ignora_blocos_de_raciocinio_estendido_na_resposta() {
        let (addr, _conexoes) = start_mock_server(
            r#"{"content":[{"type":"thinking","thinking":"deixa eu pensar...","signature":"abc"},{"type":"text","text":"pronto"}],"usage":{"input_tokens":1,"output_tokens":1}}"#,
        )
        .await;
        let provider = AnthropicProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
        );

        let resposta = provider
            .chat(ChatRequest::new("claude-sonnet", vec![Message::user("oi")]))
            .await
            .expect("resposta com bloco de raciocínio não deve falhar ao desserializar");

        assert_eq!(resposta.message, Message::assistant("pronto"));
    }

    #[tokio::test]
    async fn chat_stream_via_transporte_entrega_texto_e_tool_use_em_ordem() {
        let corpo = "\
            data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":3,\"output_tokens\":0}}}\n\
            data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\
            data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ola\"}}\n\
            data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" mundo\"}}\n\
            data: {\"type\":\"content_block_stop\",\"index\":0}\n\
            data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"fs_read\",\"input\":{}}}\n\
            data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"path\\\"\"}}\n\
            data: {\"type\":\"content_block_delta\",\"index\":1,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\":\\\"a.txt\\\"}\"}}\n\
            data: {\"type\":\"content_block_stop\",\"index\":1}\n\
            data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":4}}\n\
            data: {\"type\":\"message_stop\"}\n";
        let (addr, _conexoes) = start_mock_server(corpo).await;
        let provider = AnthropicProvider::new(
            transport_com_classe(addr, EgressClass::CloudOk),
            format!("http://{addr}"),
        );

        let mut stream = provider
            .chat_stream(ChatRequest::new("claude-sonnet", vec![Message::user("oi")]))
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
                    id: "toolu_1".into(),
                    name: "fs_read".into(),
                },
                StreamEvent::ToolCallDelta {
                    id: "toolu_1".into(),
                    delta: "{\"path\"".into(),
                },
                StreamEvent::ToolCallDelta {
                    id: "toolu_1".into(),
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
    async fn chat_respeita_classe_de_egresso_e_bloqueia_sem_tocar_a_rede() {
        let (addr, conexoes) = start_mock_server(r#"{"content":[]}"#).await;
        // Allowlist cadastra o host, mas exigindo uma classe de nuvem que a
        // sessão ativa (`cloud-opt-out`) não cobre.
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            addr.ip().to_string(),
            EgressClass::CloudOk,
        )]);
        let transport = Arc::new(
            Transport::new(
                allowlist,
                EgressClass::CloudOptOut,
                Some("externo-confidencial".into()),
                Arc::new(NoopSink),
            )
            .with_header("x-api-key", "chave-de-teste")
            .with_header("anthropic-version", "2023-06-01"),
        );
        let provider = AnthropicProvider::new(transport, format!("http://{addr}"));

        let erro = provider
            .chat(ChatRequest::new("claude-sonnet", vec![Message::user("oi")]))
            .await
            .expect_err("sessão cloud-opt-out não deve alcançar host cloud-ok");

        assert!(matches!(erro, ProviderError::Network(_)));
        assert_eq!(
            conexoes.load(Ordering::SeqCst),
            0,
            "nenhuma conexão deveria ter sido aberta"
        );
    }

    #[test]
    fn build_request_extrai_mensagens_de_sistema_para_o_campo_system() {
        let request = ChatRequest::new(
            "modelo-x",
            vec![Message::system("seja conciso"), Message::user("oi")],
        );

        let anthropic_request = build_request(&request, false);
        let json = serde_json::to_value(&anthropic_request).expect("deve serializar");

        assert_eq!(json["system"], "seja conciso");
        assert_eq!(
            json["messages"].as_array().expect("deve ser array").len(),
            1
        );
        assert_eq!(json["messages"][0]["role"], "user");
    }

    #[test]
    fn build_request_omite_thinking_sem_reasoning_definido() {
        let request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        let anthropic_request = build_request(&request, false);
        let json = serde_json::to_value(&anthropic_request).expect("deve serializar");

        assert!(json.get("thinking").is_none());
    }

    #[test]
    fn build_request_inclui_thinking_habilitado_quando_reasoning_e_true() {
        let mut request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        request.reasoning = Some(true);

        let anthropic_request = build_request(&request, false);
        let json = serde_json::to_value(&anthropic_request).expect("deve serializar");

        assert_eq!(json["thinking"]["type"], "enabled");
        assert_eq!(
            json["thinking"]["budget_tokens"],
            DEFAULT_THINKING_BUDGET_TOKENS
        );
    }

    #[test]
    fn build_request_omite_thinking_quando_reasoning_e_false() {
        let mut request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        request.reasoning = Some(false);

        let anthropic_request = build_request(&request, false);
        let json = serde_json::to_value(&anthropic_request).expect("deve serializar");

        assert!(json.get("thinking").is_none());
    }

    #[test]
    fn build_request_usa_max_tokens_default_quando_ausente() {
        let request = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        let anthropic_request = build_request(&request, false);
        let json = serde_json::to_value(&anthropic_request).expect("deve serializar");

        assert_eq!(json["max_tokens"], DEFAULT_MAX_TOKENS);
    }

    #[test]
    fn message_to_anthropic_agrupa_multiplos_tool_results_num_so_bloco_de_mensagem() {
        let mensagem = Message {
            role: Role::Tool,
            content: vec![
                ContentBlock::ToolResult(ToolResult {
                    call_id: "call_1".into(),
                    content: "resultado 1".into(),
                    is_error: false,
                }),
                ContentBlock::ToolResult(ToolResult {
                    call_id: "call_2".into(),
                    content: "resultado 2".into(),
                    is_error: false,
                }),
            ],
        };

        let convertida = message_to_anthropic(&mensagem);

        assert_eq!(convertida.role, "user");
        assert_eq!(convertida.content.len(), 2);
    }
}
