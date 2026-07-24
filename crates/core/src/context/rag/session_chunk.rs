// Caminho relativo: crates/core/src/context/rag/session_chunk.rs
//! Chunking por mensagem de sessões salvas (MT-131, ADR-0039).
//!
//! Análogo a [`super::chunk`] (código), mas para o corpus de
//! `.agentry/session/*.md` (ADR-0036): um chunk por mensagem de
//! `Role::User`/`Role::Assistant` — nunca a sessão inteira como um chunk
//! só (perderia granularidade de busca), nunca por tamanho fixo de token
//! (quebraria uma mensagem no meio). `Role::System` (prompt de sistema não
//! é conteúdo de conversa buscável) e `Role::Tool` (resultado de tool é
//! JSON, sem valor semântico de busca) nunca viram chunk. Uma mensagem de
//! `Role::Assistant` cujo único conteúdo é uma chamada de tool (sem texto)
//! também não vira chunk — não há texto para indexar.
//!
//! Puro e sem I/O, mesmo escopo de [`super::chunk::chunk_file`]: recebe as
//! mensagens já desserializadas (via
//! [`crate::session::persist::desserializar_de_markdown`], MT-120), não lê
//! nenhum arquivo sozinho — a orquestração de ler `.agentry/session/*.md`
//! fica para quem chama (indexação incremental, MT-135; tool
//! `session_search`, MT-136).

use crate::model::{Message, Role};

/// Um chunk pronto para indexação: o texto de uma única mensagem de uma
/// sessão salva, com metadados suficientes para correlacionar de volta à
/// sessão/mensagem de origem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionChunk {
    /// `id` da sessão de origem (mesmo `id` usado em
    /// `.agentry/session/<id>.md`, ADR-0036).
    pub session_id: String,
    /// Índice da mensagem dentro do `Vec<Message>` original da sessão —
    /// preserva a posição real (mensagens ignoradas, ex. `Role::System`,
    /// não deslocam os índices das seguintes).
    pub message_index: usize,
    /// Autor da mensagem — sempre `Role::User` ou `Role::Assistant`
    /// (nunca `System`/`Tool`, filtrados antes de chegar aqui).
    pub role: Role,
    /// Texto da mensagem — sempre [`Message::text_content`] completo,
    /// nunca truncado.
    pub text: String,
}

/// Gera os chunks de uma sessão (`session_id`, `mensagens`) — um por
/// mensagem de `Role::User`/`Role::Assistant` com texto não vazio. A ordem
/// segue a ordem de `mensagens` (mesma ordem cronológica da conversa).
#[must_use]
pub fn chunk_session(session_id: &str, mensagens: &[Message]) -> Vec<SessionChunk> {
    mensagens
        .iter()
        .enumerate()
        .filter(|(_, mensagem)| matches!(mensagem.role, Role::User | Role::Assistant))
        .filter_map(|(indice, mensagem)| {
            let texto = mensagem.text_content();
            if texto.is_empty() {
                return None;
            }
            Some(SessionChunk {
                session_id: session_id.to_string(),
                message_index: indice,
                role: mensagem.role,
                text: texto,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, ToolCall};

    #[test]
    fn chunk_session_gera_um_chunk_por_mensagem_de_usuario_e_agente() {
        let mensagens = vec![
            Message::system("prompt de sistema"),
            Message::user("qual a capital da frança"),
            Message::assistant("paris"),
        ];

        let chunks = chunk_session("20260724-000000", &mensagens);

        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0].session_id, "20260724-000000");
        assert_eq!(chunks[0].message_index, 1);
        assert_eq!(chunks[0].role, Role::User);
        assert_eq!(chunks[0].text, "qual a capital da frança");
        assert_eq!(chunks[1].message_index, 2);
        assert_eq!(chunks[1].role, Role::Assistant);
        assert_eq!(chunks[1].text, "paris");
    }

    #[test]
    fn chunk_session_ignora_mensagem_de_sistema_e_de_tool() {
        let mensagens = vec![
            Message::system("prompt de sistema"),
            Message::user("crie um arquivo"),
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: "call_1".into(),
                    name: "fs_write".into(),
                    arguments: serde_json::json!({}),
                })],
            },
            Message {
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(crate::model::ToolResult {
                    call_id: "call_1".into(),
                    content: "arquivo criado".into(),
                    is_error: false,
                })],
            },
        ];

        let chunks = chunk_session("id", &mensagens);

        // Só a mensagem de usuário vira chunk: a de sistema é sempre
        // ignorada, a de tool não tem role User/Assistant, e a de
        // assistente só tem um bloco de tool call (sem texto).
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].role, Role::User);
        assert_eq!(chunks[0].text, "crie um arquivo");
    }

    #[test]
    fn chunk_session_preserva_o_indice_original_apesar_de_mensagens_ignoradas() {
        let mensagens = vec![
            Message::system("prompt"),      // índice 0, ignorada
            Message::user("pergunta"),      // índice 1
            Message::assistant("resposta"), // índice 2
        ];

        let chunks = chunk_session("id", &mensagens);

        assert_eq!(chunks[0].message_index, 1);
        assert_eq!(chunks[1].message_index, 2);
    }

    #[test]
    fn sessao_sem_mensagem_de_usuario_ou_agente_nao_gera_chunk() {
        let mensagens = vec![Message::system("só sistema")];

        let chunks = chunk_session("id", &mensagens);

        assert!(chunks.is_empty());
    }

    #[test]
    fn sessao_vazia_nao_gera_chunk() {
        assert_eq!(chunk_session("id", &[]), Vec::new());
    }
}
