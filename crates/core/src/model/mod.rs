// Caminho relativo: crates/core/src/model/mod.rs
//! Tipos de domínio de mensagens/LLM (MT-02).
//!
//! Representação **agnóstica de provider** de uma conversa: mensagens, blocos de
//! conteúdo, chamadas de tool, uso de tokens e eventos de streaming. Os adapters
//! (Ollama, OpenAI-compatible, Anthropic) traduzem estes tipos de/para o formato
//! de cada API — nenhum formato específico de provider vaza para cá (ADR-0001).
//!
//! Todos os tipos são serializáveis com `serde`; o formato JSON destes tipos é o
//! formato *interno* do `agentry` (sessões, audit log), não o de nenhuma API.

use serde::{Deserialize, Serialize};

/// Papel do autor de uma [`Message`] na conversa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Instruções de sistema (prompt de sistema).
    System,
    /// Mensagem do usuário humano.
    User,
    /// Resposta do modelo.
    Assistant,
    /// Resultado de tool devolvido ao modelo.
    Tool,
}

/// Chamada de tool solicitada pelo modelo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolCall {
    /// Identificador único da chamada (correlaciona com [`ToolResult::call_id`]).
    pub id: String,
    /// Nome da tool a executar.
    pub name: String,
    /// Argumentos da chamada, como JSON arbitrário.
    pub arguments: serde_json::Value,
}

/// Resultado da execução de uma tool, devolvido ao modelo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    /// Identificador da [`ToolCall`] correspondente.
    pub call_id: String,
    /// Conteúdo textual do resultado (saída da tool ou descrição do erro).
    pub content: String,
    /// Indica se a execução falhou.
    #[serde(default)]
    pub is_error: bool,
}

/// Bloco de conteúdo de uma [`Message`].
///
/// Serializado com tag interna `type` (ex.: `{"type": "text", "text": "..."}`),
/// mantendo o formato interno estável e autodescritivo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    /// Texto simples.
    Text {
        /// Conteúdo textual.
        text: String,
    },
    /// Solicitação de execução de tool feita pelo modelo.
    ToolCall(ToolCall),
    /// Resultado de tool devolvido ao modelo.
    ToolResult(ToolResult),
}

/// Mensagem de uma conversa: um [`Role`] e uma sequência de [`ContentBlock`]s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    /// Autor da mensagem.
    pub role: Role,
    /// Blocos de conteúdo, em ordem.
    pub content: Vec<ContentBlock>,
}

impl Message {
    /// Cria uma mensagem de texto com o papel dado.
    #[must_use]
    pub fn text(role: Role, text: impl Into<String>) -> Self {
        Self {
            role,
            content: vec![ContentBlock::Text { text: text.into() }],
        }
    }

    /// Cria uma mensagem de sistema com texto simples.
    #[must_use]
    pub fn system(text: impl Into<String>) -> Self {
        Self::text(Role::System, text)
    }

    /// Cria uma mensagem de usuário com texto simples.
    #[must_use]
    pub fn user(text: impl Into<String>) -> Self {
        Self::text(Role::User, text)
    }

    /// Cria uma mensagem do assistente com texto simples.
    #[must_use]
    pub fn assistant(text: impl Into<String>) -> Self {
        Self::text(Role::Assistant, text)
    }

    /// Concatena os blocos [`ContentBlock::Text`] da mensagem, ignorando
    /// tool-calls/tool-results — usado sempre que se precisa só do texto
    /// puro de uma resposta (resumo de compactação, MT-36; resposta de
    /// reranking, MT-28).
    #[must_use]
    pub fn text_content(&self) -> String {
        self.content
            .iter()
            .filter_map(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Contagem de tokens consumidos em uma interação.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Usage {
    /// Tokens de entrada (prompt).
    #[serde(default)]
    pub input_tokens: u64,
    /// Tokens de saída (resposta gerada).
    #[serde(default)]
    pub output_tokens: u64,
}

impl Usage {
    /// Total de tokens (entrada + saída), saturando em `u64::MAX`.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

/// Evento de uma resposta em streaming, agnóstico de provider.
///
/// Serializado com tag interna `event` (ex.: `{"event": "text_delta", ...}`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum StreamEvent {
    /// Início da resposta do modelo.
    MessageStart,
    /// Fragmento incremental de texto.
    TextDelta {
        /// Trecho de texto recebido.
        text: String,
    },
    /// Início de uma chamada de tool (nome já conhecido; argumentos virão em deltas).
    ToolCallStart {
        /// Identificador único da chamada.
        id: String,
        /// Nome da tool.
        name: String,
    },
    /// Fragmento incremental dos argumentos (JSON parcial) de uma chamada de tool.
    ToolCallDelta {
        /// Identificador da chamada correspondente.
        id: String,
        /// Trecho do JSON de argumentos.
        delta: String,
    },
    /// Fim da resposta, com o consumo total de tokens.
    MessageEnd {
        /// Uso de tokens da interação.
        usage: Usage,
    },
    /// Resultado de uma chamada de tool já executada (ADR-0035, MT-114) —
    /// emitido por [`crate::session::Session::run_streaming`] logo depois
    /// de `Session::after_response` executar a tool, correlacionado ao
    /// `ToolCallStart`/`ToolCallDelta` correspondente pelo mesmo `id`. O
    /// nome da tool não viaja de novo aqui — quem consome já viu o nome no
    /// `ToolCallStart`. Sem equivalente em [`crate::session::Session::run`]
    /// (não-*streaming*): a execução de tools é a mesma em ambos os modos,
    /// só não há `on_event` em `run` para emitir isso.
    ToolCallResult {
        /// Identificador da chamada correspondente (mesmo `id` do
        /// `ToolCallStart`/`ToolCallDelta`, e de [`ToolResult::call_id`]).
        id: String,
        /// Conteúdo textual do resultado (saída da tool ou descrição do erro).
        content: String,
        /// Indica se a execução falhou.
        is_error: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializa e desserializa, exigindo igualdade (round-trip serde do MT-02).
    fn round_trip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de> + PartialEq + core::fmt::Debug,
    {
        let json = serde_json::to_string(value).expect("serialização deve funcionar");
        let de: T = serde_json::from_str(&json).expect("desserialização deve funcionar");
        assert_eq!(value, &de, "round-trip deve preservar o valor: {json}");
    }

    #[test]
    fn round_trip_de_todos_os_papeis() {
        for role in [Role::System, Role::User, Role::Assistant, Role::Tool] {
            round_trip(&role);
        }
    }

    #[test]
    fn round_trip_de_mensagem_com_todos_os_blocos() {
        let msg = Message {
            role: Role::Assistant,
            content: vec![
                ContentBlock::Text {
                    text: "vou ler o arquivo".into(),
                },
                ContentBlock::ToolCall(ToolCall {
                    id: "call-1".into(),
                    name: "fs_read".into(),
                    arguments: serde_json::json!({ "path": "Cargo.toml", "linhas": 10 }),
                }),
                ContentBlock::ToolResult(ToolResult {
                    call_id: "call-1".into(),
                    content: "[workspace]".into(),
                    is_error: false,
                }),
            ],
        };
        round_trip(&msg);
    }

    #[test]
    fn round_trip_dos_eventos_de_stream() {
        let eventos = [
            StreamEvent::MessageStart,
            StreamEvent::TextDelta {
                text: "olá".into()
            },
            StreamEvent::ToolCallStart {
                id: "call-1".into(),
                name: "shell".into(),
            },
            StreamEvent::ToolCallDelta {
                id: "call-1".into(),
                delta: "{\"cmd\":".into(),
            },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 12,
                    output_tokens: 34,
                },
            },
        ];
        for evento in &eventos {
            round_trip(evento);
        }
    }

    #[test]
    fn formato_interno_estavel() {
        // Trava o formato de fio interno: tag `type` em blocos e `event` em stream.
        let bloco = ContentBlock::Text { text: "oi".into() };
        assert_eq!(
            serde_json::to_value(&bloco).unwrap(),
            serde_json::json!({ "type": "text", "text": "oi" })
        );

        let evento = StreamEvent::TextDelta { text: "oi".into() };
        assert_eq!(
            serde_json::to_value(&evento).unwrap(),
            serde_json::json!({ "event": "text_delta", "text": "oi" })
        );

        let papel = serde_json::to_value(Role::Assistant).unwrap();
        assert_eq!(papel, serde_json::json!("assistant"));
    }

    #[test]
    fn tool_result_sem_is_error_assume_false() {
        // `is_error` tem `#[serde(default)]`: entradas antigas/mínimas continuam válidas.
        let de: ToolResult =
            serde_json::from_str(r#"{ "call_id": "c1", "content": "ok" }"#).unwrap();
        assert!(!de.is_error);
    }

    #[test]
    fn usage_total_satura_sem_overflow() {
        let u = Usage {
            input_tokens: u64::MAX,
            output_tokens: 1,
        };
        assert_eq!(u.total(), u64::MAX);
        assert_eq!(Usage::default().total(), 0);
    }

    #[test]
    fn construtores_de_mensagem() {
        let m = Message::user("oi");
        assert_eq!(m.role, Role::User);
        assert_eq!(m.content, vec![ContentBlock::Text { text: "oi".into() }]);
        assert_eq!(Message::system("s").role, Role::System);
        assert_eq!(Message::assistant("a").role, Role::Assistant);
    }
}
