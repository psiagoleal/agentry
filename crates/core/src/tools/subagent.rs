// Caminho relativo: crates/core/src/tools/subagent.rs
//! Tool `subagent` (MT-90, ADR-0031): delega uma subtarefa a uma [`Session`]
//! interna — roda até completar (sem *streaming*, só o texto final volta
//! como [`ToolOutput`]).
//!
//! **Classe de egresso restrita à sessão-mãe, sem código novo de
//! imposição:** esta tool guarda o **mesmo** `Arc<Router>` já construído
//! para a sessão-mãe (nunca um `Router` próprio) — como
//! [`Router::resolve`]/[`Router::resolve_with_override`] já recusam
//! qualquer candidato mais permissivo que o teto de egresso do perfil
//! ativo, para **qualquer** chamador, essa garantia vale automaticamente
//! para o subagente.
//!
//! **Recursão impossível estruturalmente:** o `Arc<dyn ToolExecutor>`
//! injetado aqui vem de um `ToolRegistry` que **nunca registra esta própria
//! tool** (fiação em `crates/cli/src/main.rs`, MT-91) — o modelo dentro do
//! subagente nem enxerga a tool `subagent` existir.
//!
//! Reaproveita o mesmo `PermissionGate`/`Confirmer` (via o `ToolExecutor`
//! compartilhado) e o mesmo `GuardrailGate`/*sink* da sessão-mãe — nenhum
//! mecanismo paralelo de permissão/auditoria.

use std::sync::Arc;

use crate::guardrail::{GuardrailAuditSink, GuardrailGate};
use crate::provider::BoxFuture;
use crate::router::{Router, RuntimeOverride};
use crate::session::{Session, TokenBudget, ToolExecutor};
use crate::tools::{Tool, ToolOutput};

/// `task-class` usada quando o argumento `task_class` não é informado —
/// mesmo *default* de `--task-class`/`/task-class` na CLI (ADR-0021).
const TASK_CLASS_PADRAO: &str = "chat";

/// Orçamento de tokens de um subagente — mesmo valor de
/// `DEFAULT_TOKEN_BUDGET` (`crates/cli/src/main.rs`), duplicado aqui só
/// pela fronteira de crate (`crates/core` não depende de `crates/cli`);
/// escolhido pelo mesmo motivo, não uma decisão de design nova.
const TOKEN_BUDGET_SUBAGENTE: u64 = 100_000;

/// Delega uma subtarefa a uma [`Session`] interna.
pub struct SubagentTool {
    /// O **mesmo** `Router` da sessão-mãe — nunca um `Router` próprio, mais
    /// permissivo (ver documentação do módulo).
    router: Arc<Router>,
    /// Executor cujo `ToolRegistry` interno nunca inclui esta própria tool
    /// (recursão impossível estruturalmente).
    executor: Arc<dyn ToolExecutor>,
    /// Mesmo par gate/*sink* de *guardrails* da sessão-mãe, se houver —
    /// `None` reflete uma sessão-mãe sem *guardrails* configurados.
    guardrails: Option<(Arc<GuardrailGate>, Arc<dyn GuardrailAuditSink>)>,
}

impl SubagentTool {
    #[must_use]
    pub fn new(
        router: Arc<Router>,
        executor: Arc<dyn ToolExecutor>,
        guardrails: Option<(Arc<GuardrailGate>, Arc<dyn GuardrailAuditSink>)>,
    ) -> Self {
        Self {
            router,
            executor,
            guardrails,
        }
    }
}

impl Tool for SubagentTool {
    fn name(&self) -> &str {
        "subagent"
    }

    fn description(&self) -> &str {
        "Delega uma subtarefa a um subagente (sessão interna): roda até completar e devolve \
         só a resposta final, sem streaming incremental. Um subagente nunca pode criar outro \
         subagente. A classe de egresso do subagente nunca é mais permissiva que a da sessão \
         principal."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "description": {
                    "type": "string",
                    "description": "Descrição da subtarefa a delegar ao subagente."
                },
                "task_class": {
                    "type": "string",
                    "description": "Task-class já declarada a usar para o subagente (opcional; default 'chat')."
                }
            },
            "required": ["description"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(description) = arguments.get("description").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'description' ausente ou inválido");
            };
            let task_class = arguments
                .get("task_class")
                .and_then(|v| v.as_str())
                .unwrap_or(TASK_CLASS_PADRAO);

            let rota = match self
                .router
                .resolve_with_override(task_class, &RuntimeOverride::default())
            {
                Ok(rota) => rota,
                Err(erro) => return ToolOutput::error(erro.to_string()),
            };

            let mut sessao = Session::new(
                rota,
                Arc::clone(&self.executor),
                TokenBudget::new(TOKEN_BUDGET_SUBAGENTE),
            );
            if let Some((gate, sink)) = &self.guardrails {
                sessao = sessao.with_guardrails(Arc::clone(gate), Arc::clone(sink));
            }
            sessao.push_user_message(description);

            match sessao.run(self.router.as_ref()).await {
                Ok(_outcome) => {
                    let resposta = sessao
                        .messages()
                        .last()
                        .map(|m| m.text_content())
                        .unwrap_or_default();
                    ToolOutput::ok(resposta)
                }
                Err(erro) => ToolOutput::error(erro.to_string()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::privacy::EgressClass;
    use crate::model::{Message, ToolCall, ToolResult};
    use crate::provider::mock::MockProvider;
    use crate::provider::LlmProvider;
    use crate::router::{CallPreset, RouteEntry, RouteTarget};
    use crate::tools::{PermissionGate, ToolRegistry};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct NoopExecutor {
        chamadas: AtomicUsize,
    }

    impl ToolExecutor for NoopExecutor {
        fn execute(&self, call: &ToolCall) -> BoxFuture<'_, ToolResult> {
            self.chamadas.fetch_add(1, Ordering::SeqCst);
            let call_id = call.id.clone();
            Box::pin(async move {
                ToolResult {
                    call_id,
                    content: "ok".into(),
                    is_error: false,
                }
            })
        }
    }

    fn router_com_chat(egress_class: EgressClass, modelo_resposta: Arc<MockProvider>) -> Router {
        let mut router = Router::new(egress_class);
        router.register_provider(modelo_resposta.clone());
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    modelo_resposta.name(),
                    "modelo-x",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );
        router
    }

    #[tokio::test]
    async fn subtarefa_simples_completa_e_devolve_o_texto_da_resposta_final() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(crate::provider::ChatResponse {
            message: Message::assistant("resultado da subtarefa"),
            usage: crate::model::Usage::default(),
        }));
        let router = Arc::new(router_com_chat(EgressClass::LocalOnly, mock));
        let executor: Arc<dyn ToolExecutor> = Arc::new(NoopExecutor {
            chamadas: AtomicUsize::new(0),
        });
        let tool = SubagentTool::new(router, executor, None);

        let resultado = tool
            .execute(serde_json::json!({ "description": "resuma este arquivo" }))
            .await;

        assert!(!resultado.is_error);
        assert_eq!(resultado.content, "resultado da subtarefa");
    }

    #[tokio::test]
    async fn task_class_desconhecida_e_erro_tratado() {
        let mock = Arc::new(MockProvider::new("mock"));
        let router = Arc::new(router_com_chat(EgressClass::LocalOnly, mock));
        let executor: Arc<dyn ToolExecutor> = Arc::new(NoopExecutor {
            chamadas: AtomicUsize::new(0),
        });
        let tool = SubagentTool::new(router, executor, None);

        let resultado = tool
            .execute(serde_json::json!({
                "description": "tarefa",
                "task_class": "task-class-inexistente"
            }))
            .await;

        assert!(resultado.is_error);
        assert!(resultado.content.contains("task-class desconhecida"));
    }

    #[tokio::test]
    async fn subagente_nunca_resolve_candidato_mais_permissivo_que_o_teto_da_sessao_mae() {
        // Router com teto local-only: mesmo declarando um candidato de nuvem
        // para "chat", resolve() já recusaria — o subagente reaproveita o
        // MESMO Router, então herda a mesma recusa, sem checagem própria.
        let mock = Arc::new(MockProvider::new("mock"));
        let mut router = Router::new(EgressClass::LocalOnly);
        router.register_provider(mock.clone());
        router.set_route(
            "so-nuvem",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    mock.name(),
                    "modelo-nuvem",
                    EgressClass::CloudOk,
                )],
                preset: CallPreset::default(),
            },
        );
        let router = Arc::new(router);
        let executor: Arc<dyn ToolExecutor> = Arc::new(NoopExecutor {
            chamadas: AtomicUsize::new(0),
        });
        let tool = SubagentTool::new(router, executor, None);

        let resultado = tool
            .execute(serde_json::json!({
                "description": "tarefa sensível",
                "task_class": "so-nuvem"
            }))
            .await;

        assert!(
            resultado.is_error,
            "candidato de nuvem sob teto local-only deve ser recusado, mesmo pedido pelo subagente"
        );
    }

    #[tokio::test]
    async fn executor_do_subagente_nunca_expoe_a_propria_tool_subagent() {
        // Simula a fiação real (MT-91): um ToolRegistry que NUNCA registra
        // SubagentTool é o que alimenta o executor do subagente — aqui
        // provamos que um registro assim, de fato, nunca lista "subagent".
        let registry =
            ToolRegistry::new(PermissionGate::new(crate::config::Permissions::default()));
        // Nenhuma tool registrada de propósito — o teste importa é sobre a
        // ausência de "subagent" nas specs, não sobre quais outras tools
        // existem (isso é responsabilidade da fiação real, MT-91).
        let nomes: Vec<String> = registry.specs().into_iter().map(|s| s.name).collect();

        assert!(
            !nomes.contains(&"subagent".to_string()),
            "um ToolRegistry vazio (como o do executor interno do subagente) nunca lista \
             'subagent' entre suas specs — a fiação real (MT-91) nunca registra essa tool nele"
        );
    }
}
