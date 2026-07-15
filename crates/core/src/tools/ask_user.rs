// Caminho relativo: crates/core/src/tools/ask_user.rs
//! Tool `ask_user` (MT-63, ADR-0024): canal para o agente perguntar/
//! confirmar algo com o usuário no meio de uma tarefa — o único canal
//! humano→agente que existia até aqui (`Confirmer`,
//! `crates/cli/src/tool_executor.rs`) só aprova/recusa uma `ToolCall`
//! pendente, sem espaço para o modelo pedir uma informação ou esclarecer
//! uma ambiguidade.
//!
//! O canal de interação (`Prompter`) é definido aqui, no `core` — mesmo
//! padrão de `AuditSink`/`GuardrailAuditSink` (interface no `core`,
//! implementação concreta fornecida por quem consome, ex.: a CLI) — e não
//! o padrão do `Confirmer` (tipo só da CLI): `AskUserTool` implementa
//! `Tool` como qualquer outra (MT-11), e toda `Tool` vive em
//! `agentry_core::tools`.

use std::sync::Arc;

use crate::provider::BoxFuture;
use crate::tools::{Tool, ToolOutput};

/// Canal de interação humano↔agente. Dyn-compatible via [`BoxFuture`],
/// mesmo padrão das demais traits do projeto (sem `async-trait`) — permite
/// trocar por um dublê nos testes, ou por um widget de TUI no futuro (Fase
/// 15), sem tocar em [`AskUserTool`].
pub trait Prompter: Send + Sync {
    /// Pergunta `question` ao usuário; `options`, se não vazio, são
    /// sugestões — o usuário ainda pode responder livremente. Devolve a
    /// resposta como texto, sem *parsing*/validação: quem decide o que
    /// fazer com ela é o próprio modelo, no próximo turno.
    fn ask(&self, question: &str, options: &[String]) -> BoxFuture<'_, String>;
}

/// Tool `ask_user`: pergunta/confirma algo com o usuário via um
/// [`Prompter`] injetado.
pub struct AskUserTool {
    prompter: Arc<dyn Prompter>,
}

impl AskUserTool {
    /// Cria a tool sobre o canal de interação dado.
    #[must_use]
    pub fn new(prompter: Arc<dyn Prompter>) -> Self {
        Self { prompter }
    }
}

impl Tool for AskUserTool {
    fn name(&self) -> &str {
        "ask_user"
    }

    fn description(&self) -> &str {
        "Pergunta algo ao usuário e devolve a resposta em texto — use para esclarecer uma \
         ambiguidade genuína ou confirmar uma decisão que só o usuário pode tomar. Evite \
         perguntar o que já pode ser descoberto lendo o código/config do projeto."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "question": {
                    "type": "string",
                    "description": "A pergunta a fazer ao usuário."
                },
                "options": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Sugestões de resposta (opcional) — o usuário ainda pode responder livremente."
                }
            },
            "required": ["question"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(question) = arguments.get("question").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'question' obrigatório e deve ser string");
            };
            let options: Vec<String> = arguments
                .get("options")
                .and_then(|v| v.as_array())
                .map(|itens| {
                    itens
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let resposta = self.prompter.ask(question, &options).await;
            ToolOutput::ok(resposta)
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

    struct PrompterFixo(String);
    impl Prompter for PrompterFixo {
        fn ask(&self, _question: &str, _options: &[String]) -> BoxFuture<'_, String> {
            let resposta = self.0.clone();
            Box::pin(async move { resposta })
        }
    }

    /// Prova que `question`/`options` de fato chegam ao `Prompter` — a
    /// resposta fixa sozinha não provaria isso; aqui devolvemos o que
    /// recebemos, ecoado.
    struct PrompterQueEcoa;
    impl Prompter for PrompterQueEcoa {
        fn ask(&self, question: &str, options: &[String]) -> BoxFuture<'_, String> {
            let eco = format!("pergunta='{question}' opcoes={options:?}");
            Box::pin(async move { eco })
        }
    }

    #[tokio::test]
    async fn resposta_do_prompter_vira_o_conteudo_da_saida() {
        let tool = AskUserTool::new(Arc::new(PrompterFixo("azul".into())));

        let saida = tool.execute(json!({ "question": "qual cor?" })).await;

        assert!(!saida.is_error);
        assert_eq!(saida.content, "azul");
    }

    #[tokio::test]
    async fn question_e_options_chegam_intactos_ao_prompter() {
        let tool = AskUserTool::new(Arc::new(PrompterQueEcoa));

        let saida = tool
            .execute(json!({ "question": "qual?", "options": ["a", "b"] }))
            .await;

        assert_eq!(saida.content, "pergunta='qual?' opcoes=[\"a\", \"b\"]");
    }

    #[tokio::test]
    async fn options_ausente_funciona_lista_vazia() {
        let tool = AskUserTool::new(Arc::new(PrompterQueEcoa));

        let saida = tool.execute(json!({ "question": "qual?" })).await;

        assert_eq!(saida.content, "pergunta='qual?' opcoes=[]");
    }

    #[tokio::test]
    async fn question_ausente_e_erro_tratado_sem_panic() {
        let tool = AskUserTool::new(Arc::new(PrompterFixo("nunca chamado".into())));

        let saida = tool.execute(json!({})).await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn tool_respeita_deny_do_permission_gate_como_qualquer_outra() {
        let mut permissions = Permissions::default();
        permissions.deny.push("ask_user".to_string());
        let mut registry = ToolRegistry::new(PermissionGate::new(permissions));
        registry.register(Arc::new(AskUserTool::new(Arc::new(PrompterFixo(
            "nunca chamado".into(),
        )))));

        let call = ToolCall {
            id: "1".into(),
            name: "ask_user".into(),
            arguments: json!({ "question": "qual?" }),
        };
        let outcome = registry.execute(&call).await;

        assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
    }
}
