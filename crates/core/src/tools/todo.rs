// Caminho relativo: crates/core/src/tools/todo.rs
//! Tool `todo_write` (MT-105, ADR-0034): o agente declara/atualiza a lista
//! de passos da tarefa atual — mesmo padrão do `TodoWrite`/`Task*` deste
//! próprio ambiente. **Semântica de substituição total:** cada chamada
//! carrega a lista inteira e atual, nunca um *diff* incremental — os
//! próprios argumentos da chamada **são** o estado; não existe nenhum
//! armazenamento paralelo aqui nem em [`crate::session`] para sincronizar
//! entre chamadas ou entre turnos (ver ADR-0034 §Decisão).
//!
//! `execute()` só valida estruturalmente os argumentos e devolve uma
//! confirmação textual curta — nunca falha por *conteúdo* da lista em si
//! (uma lista vazia, por exemplo, é válida), só por JSON malformado (mesmo
//! padrão de erro tratado das demais *tools*).
//!
//! Renderização de um *checklist* de verdade fica só do lado da TUI
//! (`crates/cli/src/tui/chat.rs`, MT-107) — este módulo não sabe nada sobre
//! interface, mesma separação núcleo/apresentação de qualquer outra *tool*.

use crate::provider::BoxFuture;
use crate::tools::{Tool, ToolOutput};

/// Tool `todo_write`: sem efeito colateral (não toca sistema de arquivos,
/// rede, nem estado do processo).
pub struct TodoWriteTool;

impl TodoWriteTool {
    /// Cria a tool — sem estado nenhum, então não há nada a configurar.
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

impl Default for TodoWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

/// Valores aceitos para `status` de um item — qualquer outra string é erro
/// tratado (ADR-0034: `execute()` só falha por JSON malformado, e um
/// `status` fora deste conjunto conta como isso).
const STATUS_VALIDOS: [&str; 3] = ["pending", "in_progress", "completed"];

impl Tool for TodoWriteTool {
    fn name(&self) -> &str {
        "todo_write"
    }

    fn description(&self) -> &str {
        "Declara ou atualiza a lista de passos da tarefa atual -- cada chamada substitui a \
         lista inteira (não é incremental), então sempre inclua todos os itens, mesmo os já \
         concluídos. Ajuda a manter o raciocínio organizado em tarefas com vários passos e \
         deixa o progresso visível para o usuário. Use com moderação: só para tarefas \
         genuinamente multi-etapa, não para pedidos triviais de um passo só."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "description": "Lista completa e atual dos passos da tarefa.",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": {
                                "type": "string",
                                "description": "Descrição curta do passo."
                            },
                            "status": {
                                "type": "string",
                                "enum": ["pending", "in_progress", "completed"],
                                "description": "Estado atual do passo."
                            }
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["items"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(items) = arguments.get("items").and_then(|v| v.as_array()) else {
                return ToolOutput::error("argumento 'items' obrigatório e deve ser um array");
            };

            for (indice, item) in items.iter().enumerate() {
                let Some(conteudo) = item.get("content").and_then(|v| v.as_str()) else {
                    return ToolOutput::error(format!(
                        "item {indice}: 'content' obrigatório e deve ser string"
                    ));
                };
                if conteudo.trim().is_empty() {
                    return ToolOutput::error(format!("item {indice}: 'content' vazio"));
                }
                let Some(status) = item.get("status").and_then(|v| v.as_str()) else {
                    return ToolOutput::error(format!(
                        "item {indice}: 'status' obrigatório e deve ser string"
                    ));
                };
                if !STATUS_VALIDOS.contains(&status) {
                    return ToolOutput::error(format!(
                        "item {indice}: status '{status}' desconhecido (válidos: {})",
                        STATUS_VALIDOS.join(", ")
                    ));
                }
            }

            ToolOutput::ok(format!(
                "lista de tarefas atualizada ({} itens)",
                items.len()
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;
    use crate::model::ToolCall;
    use crate::tools::permission::PermissionGate;
    use crate::tools::{ExecutionOutcome, ToolRegistry};
    use serde_json::json;
    use std::sync::Arc;

    #[tokio::test]
    async fn lista_valida_confirma_com_a_contagem_de_itens() {
        let tool = TodoWriteTool::new();

        let saida = tool
            .execute(json!({
                "items": [
                    {"content": "ler o arquivo", "status": "completed"},
                    {"content": "editar o arquivo", "status": "in_progress"},
                    {"content": "rodar os testes", "status": "pending"}
                ]
            }))
            .await;

        assert!(!saida.is_error);
        assert_eq!(saida.content, "lista de tarefas atualizada (3 itens)");
    }

    #[tokio::test]
    async fn lista_vazia_e_valida() {
        let tool = TodoWriteTool::new();

        let saida = tool.execute(json!({ "items": [] })).await;

        assert!(!saida.is_error);
        assert_eq!(saida.content, "lista de tarefas atualizada (0 itens)");
    }

    #[tokio::test]
    async fn items_ausente_e_erro_tratado_sem_panic() {
        let tool = TodoWriteTool::new();

        let saida = tool.execute(json!({})).await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn status_desconhecido_e_erro_tratado() {
        let tool = TodoWriteTool::new();

        let saida = tool
            .execute(json!({ "items": [{"content": "x", "status": "feito"}] }))
            .await;

        assert!(saida.is_error);
        assert!(
            saida.content.contains("feito"),
            "mensagem: {}",
            saida.content
        );
    }

    #[tokio::test]
    async fn content_vazio_e_erro_tratado() {
        let tool = TodoWriteTool::new();

        let saida = tool
            .execute(json!({ "items": [{"content": "", "status": "pending"}] }))
            .await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn content_ausente_e_erro_tratado() {
        let tool = TodoWriteTool::new();

        let saida = tool
            .execute(json!({ "items": [{"status": "pending"}] }))
            .await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn tool_respeita_deny_do_permission_gate_como_qualquer_outra() {
        let mut permissions = Permissions::default();
        permissions.deny.push("todo_write".to_string());
        let mut registry = ToolRegistry::new(PermissionGate::new(permissions));
        registry.register(Arc::new(TodoWriteTool::new()));

        let call = ToolCall {
            id: "1".into(),
            name: "todo_write".into(),
            arguments: json!({ "items": [] }),
        };
        let outcome = registry.execute(&call).await;

        assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
    }
}
