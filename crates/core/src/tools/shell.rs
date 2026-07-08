// Caminho relativo: crates/core/src/tools/shell.rs
//! Tool de shell sob permissão (MT-13): execução de comando com **deny por
//! padrão** e ganchos de sandbox.
//!
//! Diferente do gate genérico de tools (MT-11/[`crate::tools::permission`],
//! onde um nome fora das listas `deny`/`ask` é `Allow` por padrão), a
//! [`ShellPolicy`] deste módulo inverte a semântica: **ausência de padrão
//! explícito é `Deny`** — nenhum comando roda a menos que esteja listado em
//! `allow`. Isso é uma segunda camada, interna à tool, **além** do gate
//! genérico do `ToolRegistry`: mesmo que o registro permita chamar
//! `shell_exec` como tool, a `ShellPolicy` ainda decide, comando a comando,
//! se aquela linha específica pode rodar.
//!
//! A execução de fato passa pelo trait [`CommandRunner`] — o "gancho de
//! sandbox" que este ticket entrega (fora de escopo: sandbox completo de SO,
//! como namespaces/seccomp/limites de recurso; um executor assim poderia
//! implementar o mesmo trait no futuro, sem mudar a política aqui). O
//! executor *default* ([`SystemCommandRunner`]) só invoca o interpretador do
//! SO (`sh -c` em Unix, `cmd /C` no Windows — portabilidade, ADR-0005), sem
//! sandbox real.

use std::sync::Arc;

use crate::provider::BoxFuture;
use crate::tools::{Tool, ToolOutput};

/// Política de comandos do shell: **default-deny**. Só comandos que casam
/// com algum padrão de `allow` rodam; qualquer outro — incluindo o que não
/// casa com nada — é bloqueado.
#[derive(Debug, Clone, Default)]
pub struct ShellPolicy {
    /// Padrões (prefixo do comando, após aparar espaços) explicitamente
    /// permitidos.
    pub allow: Vec<String>,
}

impl ShellPolicy {
    /// Cria uma política a partir dos padrões de `allow` dados.
    #[must_use]
    pub fn new(allow: Vec<String>) -> Self {
        Self { allow }
    }

    /// Decide se `command` pode rodar: precisa casar com algum padrão de
    /// `allow` (comparação por prefixo); ausência de padrão que case é
    /// **sempre** bloqueio, nunca um "talvez".
    #[must_use]
    pub fn is_allowed(&self, command: &str) -> bool {
        let comando_aparado = command.trim_start();
        self.allow
            .iter()
            .any(|padrao| comando_aparado.starts_with(padrao.as_str()))
    }
}

/// Resultado bruto da execução de um comando.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    /// Saída padrão do processo.
    pub stdout: String,
    /// Saída de erro do processo.
    pub stderr: String,
    /// Código de saída, se o processo terminou normalmente.
    pub exit_code: Option<i32>,
}

/// Executa um comando de shell de fato.
///
/// Abstrai a execução para permitir, no futuro, um executor com sandbox de
/// SO real (namespaces, seccomp, limites de recurso, contêiner) sem mudar a
/// lógica de política deste módulo — implementar esse sandbox está fora do
/// escopo do MT-13, que só define o gancho. Dyn-compatible via [`BoxFuture`],
/// mesmo padrão de [`crate::provider::LlmProvider`]/[`Tool`], sem
/// `async-trait`.
pub trait CommandRunner: Send + Sync {
    /// Executa `command` e devolve o resultado bruto.
    fn run(&self, command: &str) -> BoxFuture<'_, CommandOutput>;
}

/// Executor *default*: roda o comando através do interpretador do SO, sem
/// nenhum sandbox real — `sh -c` em Unix, `cmd /C` no Windows (ADR-0005).
pub struct SystemCommandRunner;

impl CommandRunner for SystemCommandRunner {
    fn run(&self, command: &str) -> BoxFuture<'_, CommandOutput> {
        let command = command.to_string();
        Box::pin(async move {
            let mut cmd = if cfg!(target_os = "windows") {
                let mut c = tokio::process::Command::new("cmd");
                c.args(["/C", &command]);
                c
            } else {
                let mut c = tokio::process::Command::new("sh");
                c.args(["-c", &command]);
                c
            };
            match cmd.output().await {
                Ok(output) => CommandOutput {
                    stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
                    stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
                    exit_code: output.status.code(),
                },
                Err(e) => CommandOutput {
                    stdout: String::new(),
                    stderr: format!("falha ao iniciar o processo: {e}"),
                    exit_code: None,
                },
            }
        })
    }
}

/// Tool de execução de shell (`shell_exec`), sob [`ShellPolicy`] e
/// [`CommandRunner`].
pub struct ShellTool {
    policy: ShellPolicy,
    runner: Arc<dyn CommandRunner>,
}

impl ShellTool {
    /// Cria a tool com a política dada, usando o executor *default*
    /// ([`SystemCommandRunner`]).
    #[must_use]
    pub fn new(policy: ShellPolicy) -> Self {
        Self {
            policy,
            runner: Arc::new(SystemCommandRunner),
        }
    }

    /// Cria a tool com um executor customizado — gancho de sandbox futuro,
    /// ou dublê de teste.
    #[must_use]
    pub fn with_runner(policy: ShellPolicy, runner: Arc<dyn CommandRunner>) -> Self {
        Self { policy, runner }
    }
}

impl Tool for ShellTool {
    fn name(&self) -> &str {
        "shell_exec"
    }

    fn description(&self) -> &str {
        "Executa um comando de shell. Bloqueado por padrão — só comandos explicitamente \
         permitidos pela política rodam."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "Linha de comando a executar." }
            },
            "required": ["command"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(command) = arguments.get("command").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'command' ausente ou inválido");
            };

            if !self.policy.is_allowed(command) {
                return ToolOutput::error(format!(
                    "comando bloqueado por política (default-deny, MT-13): '{command}'"
                ));
            }

            let resultado = self.runner.run(command).await;
            let texto = format!(
                "exit_code={}\n--- stdout ---\n{}--- stderr ---\n{}",
                resultado
                    .exit_code
                    .map_or_else(|| "desconhecido".to_string(), |c| c.to_string()),
                resultado.stdout,
                resultado.stderr
            );

            if resultado.exit_code == Some(0) {
                ToolOutput::ok(texto)
            } else {
                ToolOutput::error(texto)
            }
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
    use std::sync::Mutex;

    /// Executor de teste: nunca toca o SO de verdade; grava os comandos
    /// recebidos e devolve uma saída fixa — prova que "bloqueado" significa
    /// que o executor **nunca foi chamado**, não só que o resultado veio com erro.
    #[derive(Default)]
    struct RecordingRunner {
        chamadas: Mutex<Vec<String>>,
    }

    impl CommandRunner for RecordingRunner {
        fn run(&self, command: &str) -> BoxFuture<'_, CommandOutput> {
            self.chamadas
                .lock()
                .expect("mutex do runner de teste não deve envenenar")
                .push(command.to_string());
            Box::pin(async move {
                CommandOutput {
                    stdout: "ok".into(),
                    stderr: String::new(),
                    exit_code: Some(0),
                }
            })
        }
    }

    fn call(command: &str) -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: "shell_exec".into(),
            arguments: serde_json::json!({ "command": command }),
        }
    }

    #[tokio::test]
    async fn comando_bloqueado_por_padrao_nunca_executa() {
        let runner = Arc::new(RecordingRunner::default());
        let tool = ShellTool::with_runner(ShellPolicy::default(), runner.clone());

        let saida = tool
            .execute(serde_json::json!({ "command": "echo qualquer-coisa" }))
            .await;

        assert!(saida.is_error);
        assert!(
            runner.chamadas.lock().unwrap().is_empty(),
            "default-deny nunca deve chegar a chamar o executor"
        );
    }

    #[tokio::test]
    async fn comando_explicitamente_permitido_executa() {
        let runner = Arc::new(RecordingRunner::default());
        let policy = ShellPolicy::new(vec!["echo".into()]);
        let tool = ShellTool::with_runner(policy, runner.clone());

        let saida = tool
            .execute(serde_json::json!({ "command": "echo oi" }))
            .await;

        assert!(!saida.is_error);
        assert!(saida.content.contains("ok"));
        assert_eq!(
            *runner.chamadas.lock().unwrap(),
            vec!["echo oi".to_string()]
        );
    }

    #[tokio::test]
    async fn comando_fora_do_padrao_permitido_e_bloqueado() {
        let runner = Arc::new(RecordingRunner::default());
        let policy = ShellPolicy::new(vec!["git status".into()]);
        let tool = ShellTool::with_runner(policy, runner.clone());

        let saida = tool
            .execute(serde_json::json!({ "command": "git push" }))
            .await;

        assert!(saida.is_error);
        assert!(runner.chamadas.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn deny_no_gate_generico_do_mt11_bloqueia_antes_da_shell_policy() {
        // Mesmo com uma ShellPolicy que permitiria tudo, o gate do
        // ToolRegistry (MT-11) barra "shell_exec" no nível do nome da tool.
        let runner = Arc::new(RecordingRunner::default());
        let policy = ShellPolicy::new(vec!["qualquer".into()]);
        let gate = PermissionGate::new(Permissions {
            deny: vec!["shell_exec".into()],
            ask: vec![],
        });
        let mut registry = ToolRegistry::new(gate);
        registry.register(Arc::new(ShellTool::with_runner(policy, runner.clone())));

        let outcome = registry.execute(&call("qualquer coisa")).await;

        assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
        assert!(
            runner.chamadas.lock().unwrap().is_empty(),
            "deny no gate genérico não deve nem chegar ao execute() da tool"
        );
    }

    #[tokio::test]
    async fn system_command_runner_executa_de_verdade() {
        let policy = ShellPolicy::new(vec!["echo".into()]);
        let tool = ShellTool::new(policy);

        let saida = tool
            .execute(serde_json::json!({ "command": "echo agentry-mt13" }))
            .await;

        assert!(!saida.is_error);
        assert!(saida.content.contains("agentry-mt13"));
    }
}
