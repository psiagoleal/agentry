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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::{mpsc, oneshot};

use agentry_core::model::{ToolCall, ToolResult};
use agentry_core::provider::BoxFuture;
use agentry_core::session::ToolExecutor;
use agentry_core::tools::ask_user::Prompter;
use agentry_core::tools::{ExecutionOutcome, ToolRegistry};

use crate::tui::diff::{diff_linhas, LinhaDiff};

/// Pergunta ao usuário se uma chamada de tool sob `ask` pode rodar.
///
/// Dyn-compatible via [`BoxFuture`], mesmo padrão das demais traits do
/// projeto (sem `async-trait`) — permite trocar por um dublê nos testes.
pub trait Confirmer: Send + Sync {
    /// Devolve `true` se o usuário aprovou a execução de `call`.
    fn confirm(&self, call: &ToolCall) -> BoxFuture<'_, bool>;
}

/// Pedido de interação humana enviado pelo [`TuiConfirmer`]/
/// `crates/cli/src/tui/ask_user.rs::TuiPrompter` ao laço de eventos da TUI
/// (MT-74/ADR-0027). O `Confirmer`/`Prompter` são chamados de dentro da
/// *task* de streaming (`Session::run_streaming`, MT-72) — não o laço que
/// possui o terminal — então a resposta atravessa um canal `oneshot`
/// dedicado a cada pedido, nunca `stdin`/`stdout` diretamente (que já
/// brigam com o modo bruto do terminal, achado do MT-72).
pub enum PedidoHumano {
    /// Confirmação de uma tool-call pendente sob `ask` ([`Confirmer`]).
    Confirmacao {
        call: ToolCall,
        /// Diff linha a linha (MT-75) para `fs_write`/`fs_edit` — `None`
        /// para qualquer outra tool (nenhum diff faz sentido para ela; o
        /// laço de eventos volta a mostrar os argumentos brutos).
        diff: Option<Vec<LinhaDiff>>,
        responder: oneshot::Sender<bool>,
    },
    /// Pergunta de texto livre (`Prompter`/`AskUserTool`, ADR-0024).
    Pergunta {
        question: String,
        options: Vec<String>,
        responder: oneshot::Sender<String>,
    },
}

/// Confirmador para o modo TUI (MT-74/ADR-0027): em vez de `print!`/
/// `read_line` (brigam com o modo bruto do terminal), envia um
/// [`PedidoHumano::Confirmacao`] pelo canal ao laço de eventos da TUI, que
/// desenha o modal e devolve a resposta pelo `oneshot` do pedido.
///
/// *Toggle* `auto`/`normal` (`auto`, um `AtomicBool` compartilhado com o
/// laço de eventos via `Arc`, alternado pelo atalho de teclado dedicado):
/// no modo `auto`, aprova **sem mostrar o modal nem passar pelo canal** —
/// mas só é consultado quando o `PermissionGate` já decidiu `ask`
/// (`RegistryToolExecutor::execute`, abaixo, nem chama `Confirmer::confirm`
/// para uma chamada negada — a invariante "`auto` nunca aprova sob `deny`"
/// é estrutural, garantida pelo `match` de `ExecutionOutcome`, não uma
/// checagem redundante aqui).
pub struct TuiConfirmer {
    tx: mpsc::UnboundedSender<PedidoHumano>,
    auto: Arc<AtomicBool>,
    /// Raiz do workspace — usada só para montar o diff de `fs_write`/
    /// `fs_edit` (MT-75), lendo o conteúdo atual do arquivo via
    /// `fs::read_to_string`. Nenhuma mudança em `FsWriteTool`/`FsEditTool`
    /// nem na checagem de segurança que elas fazem ao executar de fato —
    /// esta leitura é só para a prévia mostrada antes da aprovação.
    workspace_root: PathBuf,
}

impl TuiConfirmer {
    /// Cria o confirmador — `tx` é o remetente compartilhado com
    /// `TuiPrompter` (mesmo canal, mesmo laço de eventos do lado
    /// receptor); `auto` é compartilhado com o laço de eventos, que
    /// alterna o valor quando o usuário aperta o atalho do *toggle*.
    #[must_use]
    pub fn new(
        tx: mpsc::UnboundedSender<PedidoHumano>,
        auto: Arc<AtomicBool>,
        workspace_root: PathBuf,
    ) -> Self {
        Self {
            tx,
            auto,
            workspace_root,
        }
    }
}

impl Confirmer for TuiConfirmer {
    fn confirm(&self, call: &ToolCall) -> BoxFuture<'_, bool> {
        if self.auto.load(Ordering::Relaxed) {
            return Box::pin(async { true });
        }
        let diff = montar_diff_se_aplicavel(call, &self.workspace_root);
        let (responder, receptor) = oneshot::channel();
        let pedido = PedidoHumano::Confirmacao {
            call: call.clone(),
            diff,
            responder,
        };
        let tx = self.tx.clone();
        Box::pin(async move {
            if tx.send(pedido).is_err() {
                // Laço de eventos encerrado — nega por segurança, nunca
                // aprova um pedido que ninguém pôde responder.
                return false;
            }
            receptor.await.unwrap_or(false)
        })
    }
}

/// Monta o diff de [`LinhaDiff`] para `fs_write`/`fs_edit` — `None` para
/// qualquer outra tool, ou se os argumentos esperados estiverem ausentes/
/// malformados (a confirmação segue com o resumo genérico nesse caso, sem
/// travar por causa de uma prévia que não pôde ser montada). Lê o conteúdo
/// atual do arquivo direto do disco (`fs::read_to_string`) — arquivo
/// ausente conta como conteúdo antigo vazio (`fs_write` cobre esse caso;
/// `fs_edit` sempre exige um arquivo existente, então a leitura falhando
/// aqui já significa que a própria tool também vai falhar ao rodar).
fn montar_diff_se_aplicavel(call: &ToolCall, workspace_root: &Path) -> Option<Vec<LinhaDiff>> {
    match call.name.as_str() {
        "fs_write" => {
            let path = call.arguments.get("path")?.as_str()?;
            let novo = call.arguments.get("content")?.as_str()?;
            let antigo = std::fs::read_to_string(workspace_root.join(path)).unwrap_or_default();
            Some(diff_linhas(&antigo, novo))
        }
        "fs_edit" => {
            let path = call.arguments.get("path")?.as_str()?;
            let old_string = call.arguments.get("old_string")?.as_str()?;
            let new_string = call.arguments.get("new_string")?.as_str()?;
            let antigo = std::fs::read_to_string(workspace_root.join(path)).ok()?;
            let novo = antigo.replacen(old_string, new_string, 1);
            Some(diff_linhas(&antigo, &novo))
        }
        _ => None,
    }
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

    // --- MT-74: TuiConfirmer ---

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo — mesmo padrão já usado em `crates/cli/src/main.rs`.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-tool-executor-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio do sistema não deve estar antes de 1970")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unico);
            std::fs::create_dir_all(&path).expect("deve criar diretório temporário de teste");
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn workspace_root_de_teste() -> PathBuf {
        PathBuf::from(".")
    }

    #[tokio::test]
    async fn tui_confirmer_em_auto_aprova_sem_passar_pelo_canal() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let auto = Arc::new(AtomicBool::new(true));
        let confirmer = TuiConfirmer::new(tx, auto, workspace_root_de_teste());

        let aprovado = confirmer.confirm(&call("dummy")).await;

        assert!(aprovado);
        assert!(
            rx.try_recv().is_err(),
            "modo auto não deve nem enviar o pedido pelo canal"
        );
    }

    #[tokio::test]
    async fn tui_confirmer_em_normal_envia_pedido_e_aguarda_a_resposta_do_canal() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let auto = Arc::new(AtomicBool::new(false));
        let confirmer = TuiConfirmer::new(tx, auto, workspace_root_de_teste());

        // Dublê do laço de eventos da TUI: recebe o pedido e responde.
        let responder_tarefa = tokio::spawn(async move {
            let pedido = rx.recv().await.expect("deve chegar um pedido");
            match pedido {
                PedidoHumano::Confirmacao {
                    call,
                    diff,
                    responder,
                } => {
                    assert_eq!(call.name, "dummy");
                    assert_eq!(diff, None, "'dummy' não é fs_write/fs_edit");
                    responder.send(true).expect("laço ainda deve estar vivo");
                }
                PedidoHumano::Pergunta { .. } => panic!("esperava um pedido de confirmação"),
            }
        });

        let aprovado = confirmer.confirm(&call("dummy")).await;

        assert!(aprovado);
        responder_tarefa
            .await
            .expect("dublê não deve entrar em pânico");
    }

    #[tokio::test]
    async fn tui_confirmer_sem_ninguem_do_outro_lado_do_canal_nega_por_seguranca() {
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx); // simula o laço de eventos já encerrado
        let confirmer = TuiConfirmer::new(
            tx,
            Arc::new(AtomicBool::new(false)),
            workspace_root_de_teste(),
        );

        let aprovado = confirmer.confirm(&call("dummy")).await;

        assert!(!aprovado);
    }

    /// Invariante de segurança central do MT-74/ADR-0027: o *toggle* `auto`
    /// só acelera a aprovação de uma chamada sob `ask` — nunca contorna um
    /// `deny` do `PermissionGate`. Estrutural, não incidental:
    /// `RegistryToolExecutor::execute` nem chama `Confirmer::confirm` para
    /// uma chamada negada (ver o `match` acima), então nenhum `TuiConfirmer`
    /// — em `auto` ou não — jamais participa dessa decisão.
    #[tokio::test]
    async fn modo_auto_do_tui_confirmer_nunca_aprova_uma_tool_sob_deny() {
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions {
            deny: vec!["dummy".into()],
            ask: vec![],
        }));
        registry.register(Arc::new(DummyTool));
        let (tx, mut rx) = mpsc::unbounded_channel();
        let confirmer = TuiConfirmer::new(
            tx,
            Arc::new(AtomicBool::new(true)),
            workspace_root_de_teste(),
        );
        let executor = RegistryToolExecutor::new(registry, Arc::new(confirmer));

        let resultado = executor.execute(&call("dummy")).await;

        assert!(
            resultado.is_error,
            "deny deve bloquear mesmo com o toggle auto ligado"
        );
        assert!(
            rx.try_recv().is_err(),
            "o Confirmer nem deveria ser consultado para uma chamada negada"
        );
    }

    // --- MT-75: diff de fs_write/fs_edit ---

    fn tool_call(name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments,
        }
    }

    #[test]
    fn montar_diff_fs_write_para_arquivo_novo_marca_tudo_como_adicionado() {
        let dir = TempDir::new();
        let call = tool_call(
            "fs_write",
            serde_json::json!({ "path": "novo.txt", "content": "linha 1\nlinha 2" }),
        );

        let diff = montar_diff_se_aplicavel(&call, dir.path()).expect("deve montar diff");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Adicionada("linha 1".to_string()),
                LinhaDiff::Adicionada("linha 2".to_string()),
            ]
        );
    }

    #[test]
    fn montar_diff_fs_write_sobrescrevendo_arquivo_existente_mostra_removidas_e_adicionadas() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("existe.txt"), "antigo").expect("grava arquivo de teste");
        let call = tool_call(
            "fs_write",
            serde_json::json!({ "path": "existe.txt", "content": "novo" }),
        );

        let diff = montar_diff_se_aplicavel(&call, dir.path()).expect("deve montar diff");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Removida("antigo".to_string()),
                LinhaDiff::Adicionada("novo".to_string()),
            ]
        );
    }

    #[test]
    fn montar_diff_fs_edit_reflete_a_substituicao() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("existe.txt"), "a\nb\nc").expect("grava arquivo de teste");
        let call = tool_call(
            "fs_edit",
            serde_json::json!({ "path": "existe.txt", "old_string": "b", "new_string": "x" }),
        );

        let diff = montar_diff_se_aplicavel(&call, dir.path()).expect("deve montar diff");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Inalterada("a".to_string()),
                LinhaDiff::Removida("b".to_string()),
                LinhaDiff::Adicionada("x".to_string()),
                LinhaDiff::Inalterada("c".to_string()),
            ]
        );
    }

    #[test]
    fn montar_diff_fs_edit_sem_arquivo_existente_e_none() {
        let dir = TempDir::new();
        let call = tool_call(
            "fs_edit",
            serde_json::json!({ "path": "nao-existe.txt", "old_string": "a", "new_string": "b" }),
        );

        assert_eq!(montar_diff_se_aplicavel(&call, dir.path()), None);
    }

    #[test]
    fn montar_diff_para_outra_tool_e_none() {
        let dir = TempDir::new();
        let call = tool_call("shell", serde_json::json!({ "command": "ls" }));

        assert_eq!(montar_diff_se_aplicavel(&call, dir.path()), None);
    }
}
