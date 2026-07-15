// Caminho relativo: crates/cli/src/tool_executor.rs
//! Ponte entre o `ToolRegistry` (`agentry_core::tools`, MT-11) e o
//! `ToolExecutor` que o agent loop consome (`agentry_core::session`, MT-10).
//!
//! O `ToolRegistry` decide `allow`/`ask`/`deny` mas nunca bloqueia esperando
//! um humano — devolve `NeedsConfirmation` e quem chama decide (MT-11). Esta
//! CLI é quem interage com o usuário: pergunta via [`Confirmer`] e, se
//! aprovado, roda a tool via `ToolRegistry::execute_confirmed` (sem
//! reconsultar o gate, que já respondeu `ask`).

use std::io::Write;
use std::sync::Arc;

use agentry_core::model::{ToolCall, ToolResult};
use agentry_core::provider::BoxFuture;
use agentry_core::session::ToolExecutor;
use agentry_core::tools::ask_user::Prompter;
use agentry_core::tools::{ExecutionOutcome, ToolRegistry};

/// Pergunta ao usuário se uma chamada de tool sob `ask` pode rodar.
///
/// Dyn-compatible via [`BoxFuture`], mesmo padrão das demais traits do
/// projeto (sem `async-trait`) — permite trocar por um dublê nos testes.
pub trait Confirmer: Send + Sync {
    /// Devolve `true` se o usuário aprovou a execução de `call`.
    fn confirm(&self, call: &ToolCall) -> BoxFuture<'_, bool>;
}

/// Confirmador interativo: imprime a chamada pendente e lê a resposta
/// (`s`/`n`) da entrada padrão.
pub struct InteractiveConfirmer;

impl Confirmer for InteractiveConfirmer {
    fn confirm(&self, call: &ToolCall) -> BoxFuture<'_, bool> {
        let nome = call.name.clone();
        let argumentos = call.arguments.clone();
        Box::pin(async move {
            print!("Permitir execução de '{nome}' com argumentos {argumentos}? [s/N] ");
            let _ = std::io::stdout().flush();

            let mut linha = String::new();
            if std::io::stdin().read_line(&mut linha).is_err() {
                return false;
            }
            matches!(
                linha.trim().to_lowercase().as_str(),
                "s" | "sim" | "y" | "yes"
            )
        })
    }
}

/// Formata a pergunta e, se houver, as sugestões numeradas — extraída para
/// ser testável sem depender de `stdin`/`stdout` reais (MT-64).
fn formata_pergunta(question: &str, options: &[String]) -> String {
    if options.is_empty() {
        format!("{question} ")
    } else {
        let lista = options
            .iter()
            .enumerate()
            .map(|(indice, opcao)| format!("  {}. {opcao}", indice + 1))
            .collect::<Vec<_>>()
            .join("\n");
        format!("{question}\n{lista}\nResposta: ")
    }
}

/// Implementação real de [`Prompter`] (`agentry_core::tools::ask_user`,
/// MT-63/ADR-0024): imprime a pergunta (e sugestões numeradas, se houver) e
/// lê uma linha de `stdin` — mesmo padrão síncrono de
/// [`InteractiveConfirmer`], sem *parsing*/validação da resposta. Funciona
/// tanto no modo *one-shot* quanto no REPL, sem distinção — mesma raiz de
/// código dos dois modos.
pub struct InteractivePrompter;

impl Prompter for InteractivePrompter {
    fn ask(&self, question: &str, options: &[String]) -> BoxFuture<'_, String> {
        let prompt = formata_pergunta(question, options);
        Box::pin(async move {
            print!("{prompt}");
            let _ = std::io::stdout().flush();

            let mut linha = String::new();
            if std::io::stdin().read_line(&mut linha).is_err() {
                return String::new();
            }
            linha.trim().to_string()
        })
    }
}

/// Adapta um [`ToolRegistry`] + [`Confirmer`] para o [`ToolExecutor`] que o
/// agent loop consome.
pub struct RegistryToolExecutor {
    registry: ToolRegistry,
    confirmer: Arc<dyn Confirmer>,
}

impl RegistryToolExecutor {
    /// Cria o adapter.
    #[must_use]
    pub fn new(registry: ToolRegistry, confirmer: Arc<dyn Confirmer>) -> Self {
        Self {
            registry,
            confirmer,
        }
    }
}

impl ToolExecutor for RegistryToolExecutor {
    fn execute(&self, call: &ToolCall) -> BoxFuture<'_, ToolResult> {
        let call = call.clone();
        Box::pin(async move {
            match self.registry.execute(&call).await {
                ExecutionOutcome::Executed(result) | ExecutionOutcome::Denied(result) => result,
                ExecutionOutcome::NeedsConfirmation(pendente) => {
                    if self.confirmer.confirm(&pendente).await {
                        self.registry.execute_confirmed(&pendente).await
                    } else {
                        ToolResult {
                            call_id: pendente.id.clone(),
                            content: format!("usuário recusou a execução de '{}'", pendente.name),
                            is_error: true,
                        }
                    }
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::config::Permissions;
    use agentry_core::tools::permission::PermissionGate;

    struct DummyTool;
    impl agentry_core::tools::Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }
        fn description(&self) -> &str {
            "tool de teste"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({ "type": "object" })
        }
        fn execute(
            &self,
            _arguments: serde_json::Value,
        ) -> BoxFuture<'_, agentry_core::tools::ToolOutput> {
            Box::pin(async move { agentry_core::tools::ToolOutput::ok("executado de verdade") })
        }
    }

    struct FixedConfirmer(bool);
    impl Confirmer for FixedConfirmer {
        fn confirm(&self, _call: &ToolCall) -> BoxFuture<'_, bool> {
            let resposta = self.0;
            Box::pin(async move { resposta })
        }
    }

    fn call(name: &str) -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn confirmacao_aprovada_executa_a_tool_de_fato() {
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions {
            deny: vec![],
            ask: vec!["dummy".into()],
        }));
        registry.register(Arc::new(DummyTool));
        let executor = RegistryToolExecutor::new(registry, Arc::new(FixedConfirmer(true)));

        let resultado = executor.execute(&call("dummy")).await;

        assert!(!resultado.is_error);
        assert_eq!(resultado.content, "executado de verdade");
    }

    #[tokio::test]
    async fn confirmacao_recusada_nao_executa() {
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions {
            deny: vec![],
            ask: vec!["dummy".into()],
        }));
        registry.register(Arc::new(DummyTool));
        let executor = RegistryToolExecutor::new(registry, Arc::new(FixedConfirmer(false)));

        let resultado = executor.execute(&call("dummy")).await;

        assert!(resultado.is_error);
        assert!(resultado.content.contains("recusou"));
    }

    #[tokio::test]
    async fn deny_bloqueia_sem_perguntar_nada() {
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions {
            deny: vec!["dummy".into()],
            ask: vec![],
        }));
        registry.register(Arc::new(DummyTool));
        // Confirmer que sempre aprovaria — não deve nem ser consultado.
        let executor = RegistryToolExecutor::new(registry, Arc::new(FixedConfirmer(true)));

        let resultado = executor.execute(&call("dummy")).await;

        assert!(resultado.is_error);
    }

    // --- MT-64: formatação da pergunta/sugestões do InteractivePrompter ---

    #[test]
    fn formata_pergunta_sem_opcoes_e_so_a_pergunta() {
        let saida = formata_pergunta("qual cor?", &[]);

        assert_eq!(saida, "qual cor? ");
    }

    #[test]
    fn formata_pergunta_com_opcoes_numera_cada_uma() {
        let saida = formata_pergunta(
            "qual cor?",
            &[
                "azul".to_string(),
                "verde".to_string(),
                "vermelho".to_string(),
            ],
        );

        assert_eq!(
            saida,
            "qual cor?\n  1. azul\n  2. verde\n  3. vermelho\nResposta: "
        );
    }
}
