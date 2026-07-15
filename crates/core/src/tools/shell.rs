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

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

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

/// Teto de tamanho do buffer de saída acumulada de um processo em segundo
/// plano (MT-68, ADR-0026) — um `watch`/`dev server` que nunca é
/// consultado não pode crescer sem limite na memória do processo
/// `agentry`. Ao exceder, o conteúdo mais **antigo** é descartado (mantém
/// o mais recente, mais útil para diagnóstico de um processo vivo).
const MAX_BUFFER_CHARS: usize = 50_000;

/// Descarta o conteúdo mais antigo de `buffer` até caber em `max_chars` —
/// função pura, extraída de [`acumula_stream`] para ser testável sem
/// depender de I/O assíncrona real.
fn aplica_teto(buffer: &mut String, max_chars: usize) {
    let excesso = buffer.chars().count().saturating_sub(max_chars);
    if excesso > 0 {
        *buffer = buffer.chars().skip(excesso).collect();
    }
}

/// Monta o comando via o interpretador do SO — mesma lógica de
/// [`SystemCommandRunner`] (`sh -c` em Unix, `cmd /C` no Windows,
/// ADR-0005); duplicada aqui (não reaproveita [`CommandRunner`]) porque a
/// execução em segundo plano precisa configurar `stdout`/`stderr` como
/// *pipes* e nunca chamar `.output()` (que esperaria o processo terminar)
/// — um contrato diferente o suficiente para não valer a pena forçar na
/// mesma abstração.
fn monta_comando(command: &str) -> tokio::process::Command {
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = tokio::process::Command::new("cmd");
        c.args(["/C", command]);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.args(["-c", command]);
        c
    };
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    // Rede de segurança (mesmo espírito do `Drop` do `LspClient`, MT-23):
    // se o processo `agentry` terminar sem que `stop` tenha sido chamado
    // para este processo em segundo plano, o tokio manda o sinal de morte
    // quando o `Child` é derrubado — best-effort, não cobre um
    // `std::process::exit` que pule os destrutores.
    cmd.kill_on_drop(true);
    cmd
}

/// Lê `leitor` continuamente, acumulando em `buffer` (aplicando o teto de
/// tamanho a cada leitura) até o fluxo fechar (processo terminou) ou falhar.
async fn acumula_stream<R>(mut leitor: R, buffer: Arc<Mutex<String>>)
where
    R: tokio::io::AsyncRead + Unpin,
{
    use tokio::io::AsyncReadExt;
    let mut chunk = [0u8; 4096];
    loop {
        let lidos = match leitor.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        let texto = String::from_utf8_lossy(&chunk[..lidos]).into_owned();
        let mut guarda = buffer
            .lock()
            .expect("mutex do buffer de background não deve envenenar");
        guarda.push_str(&texto);
        aplica_teto(&mut guarda, MAX_BUFFER_CHARS);
    }
}

/// Um processo em segundo plano rastreado — `child` só é usado para
/// `stop` (matar); a leitura de saída nunca toca `child` diretamente, só o
/// `buffer` compartilhado (preenchido por tarefas separadas via
/// [`acumula_stream`]).
struct ProcessoEmBackground {
    child: tokio::process::Child,
    buffer: Arc<Mutex<String>>,
}

/// Tool `shell_background` (MT-68, ADR-0026): shell de longa duração (`dev
/// server`/`watch`) — `start` spawna sem esperar terminar; `output` lê o
/// que foi acumulado desde a última consulta, sem bloquear esperando o
/// processo terminar; `stop` finaliza.
///
/// Sob a **mesma** [`ShellPolicy`] de `shell_exec` (MT-13) — rodar em
/// segundo plano nunca contorna a política *default-deny* de comando;
/// não é uma política paralela mais permissiva.
pub struct ShellBackgroundTool {
    policy: ShellPolicy,
    processos: Arc<Mutex<HashMap<String, ProcessoEmBackground>>>,
    proximo_id: AtomicU64,
}

impl ShellBackgroundTool {
    /// Cria a tool com a política dada (mesmo tipo de `ShellTool`).
    #[must_use]
    pub fn new(policy: ShellPolicy) -> Self {
        Self {
            policy,
            processos: Arc::new(Mutex::new(HashMap::new())),
            proximo_id: AtomicU64::new(1),
        }
    }

    async fn iniciar(&self, command: &str) -> ToolOutput {
        if !self.policy.is_allowed(command) {
            return ToolOutput::error(format!(
                "comando bloqueado por política (default-deny, MT-13): '{command}'"
            ));
        }

        let mut child = match monta_comando(command).spawn() {
            Ok(child) => child,
            Err(erro) => {
                return ToolOutput::error(format!(
                    "falha ao iniciar processo em segundo plano: {erro}"
                ))
            }
        };
        let pid = child.id().unwrap_or(0);

        let buffer = Arc::new(Mutex::new(String::new()));
        if let Some(stdout) = child.stdout.take() {
            tokio::spawn(acumula_stream(stdout, Arc::clone(&buffer)));
        }
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(acumula_stream(stderr, Arc::clone(&buffer)));
        }

        let id = format!("bg-{}", self.proximo_id.fetch_add(1, Ordering::SeqCst));
        self.processos
            .lock()
            .expect("mutex de processos em segundo plano não deve envenenar")
            .insert(id.clone(), ProcessoEmBackground { child, buffer });

        ToolOutput::ok(format!(
            "processo em segundo plano iniciado, id={id}, pid={pid}"
        ))
    }

    fn ler_saida(&self, id: &str) -> ToolOutput {
        let processos = self
            .processos
            .lock()
            .expect("mutex de processos em segundo plano não deve envenenar");
        let Some(processo) = processos.get(id) else {
            return ToolOutput::error(format!("nenhum processo em segundo plano com id='{id}'"));
        };
        let mut guarda = processo
            .buffer
            .lock()
            .expect("mutex do buffer de background não deve envenenar");
        let texto = std::mem::take(&mut *guarda);
        drop(guarda);
        if texto.is_empty() {
            ToolOutput::ok("(sem saída nova desde a última consulta)".to_string())
        } else {
            ToolOutput::ok(texto)
        }
    }

    async fn parar(&self, id: &str) -> ToolOutput {
        let processo = self
            .processos
            .lock()
            .expect("mutex de processos em segundo plano não deve envenenar")
            .remove(id);
        let Some(mut processo) = processo else {
            return ToolOutput::error(format!("nenhum processo em segundo plano com id='{id}'"));
        };
        match processo.child.kill().await {
            Ok(()) => ToolOutput::ok(format!("processo '{id}' finalizado")),
            Err(erro) => ToolOutput::error(format!("erro ao finalizar processo '{id}': {erro}")),
        }
    }
}

impl Tool for ShellBackgroundTool {
    fn name(&self) -> &str {
        "shell_background"
    }

    fn description(&self) -> &str {
        "Roda um comando de shell em segundo plano (ex.: dev server/watch), sem bloquear o \
         agente. action='start' inicia e devolve um id; action='output' (com esse id) lê a \
         saída acumulada desde a última consulta, sem esperar o processo terminar; \
         action='stop' finaliza o processo. Sob a mesma política default-deny de shell_exec."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["start", "output", "stop"],
                    "description": "Ação a executar."
                },
                "command": {
                    "type": "string",
                    "description": "Comando a rodar (obrigatório só para action='start')."
                },
                "id": {
                    "type": "string",
                    "description": "Identificador do processo (obrigatório para action='output'/'stop')."
                }
            },
            "required": ["action"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(action) = arguments.get("action").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'action' obrigatório e deve ser string");
            };
            match action {
                "start" => {
                    let Some(command) = arguments.get("command").and_then(|v| v.as_str()) else {
                        return ToolOutput::error("action='start' exige o argumento 'command'");
                    };
                    self.iniciar(command).await
                }
                "output" => {
                    let Some(id) = arguments.get("id").and_then(|v| v.as_str()) else {
                        return ToolOutput::error("action='output' exige o argumento 'id'");
                    };
                    self.ler_saida(id)
                }
                "stop" => {
                    let Some(id) = arguments.get("id").and_then(|v| v.as_str()) else {
                        return ToolOutput::error("action='stop' exige o argumento 'id'");
                    };
                    self.parar(id).await
                }
                outro => ToolOutput::error(format!(
                    "action desconhecida: '{outro}' (use start/output/stop)"
                )),
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

    // --- MT-68: tool shell_background (ADR-0026) ---

    fn extrai_campo<'a>(texto: &'a str, chave: &str) -> &'a str {
        texto
            .split(", ")
            .find_map(|parte| parte.split_once('=').filter(|(k, _)| *k == chave))
            .map(|(_, v)| v)
            .unwrap_or_else(|| panic!("campo '{chave}' não encontrado em '{texto}'"))
    }

    fn processo_existe(pid: u32) -> bool {
        #[cfg(unix)]
        {
            // `kill -0` só testa existência do processo, não mata nada;
            // stderr silenciado — "No such process" é o resultado esperado
            // depois do `stop`. Mesmo padrão de `crates/core/tests/lsp_client.rs`
            // (MT-23) para provar que um `stop`/`Drop` matou o processo de
            // verdade, não só devolveu sucesso.
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
            false
        }
    }

    #[test]
    fn aplica_teto_descarta_o_conteudo_mais_antigo_quando_excede() {
        let mut buffer = "x".repeat(MAX_BUFFER_CHARS + 100);

        aplica_teto(&mut buffer, MAX_BUFFER_CHARS);

        assert_eq!(buffer.chars().count(), MAX_BUFFER_CHARS);
    }

    #[test]
    fn aplica_teto_nao_mexe_quando_dentro_do_limite() {
        let mut buffer = "abc".to_string();

        aplica_teto(&mut buffer, MAX_BUFFER_CHARS);

        assert_eq!(buffer, "abc");
    }

    #[tokio::test]
    async fn start_bloqueado_por_shell_policy_nunca_spawna_nem_avanca_o_contador() {
        let policy = ShellPolicy::new(vec!["echo".into()]);
        let tool = ShellBackgroundTool::new(policy);

        let bloqueado = tool
            .execute(serde_json::json!({ "action": "start", "command": "sleep 30" }))
            .await;
        assert!(bloqueado.is_error);

        let permitido = tool
            .execute(serde_json::json!({ "action": "start", "command": "echo oi" }))
            .await;
        assert!(!permitido.is_error);
        assert_eq!(
            extrai_campo(&permitido.content, "id"),
            "bg-1",
            "a tentativa bloqueada não deve ter avançado o contador nem inserido nada no registro"
        );

        let id = extrai_campo(&permitido.content, "id").to_string();
        let _ = tool
            .execute(serde_json::json!({ "action": "stop", "id": id }))
            .await;
    }

    #[tokio::test]
    async fn start_permitido_devolve_um_id() {
        let policy = ShellPolicy::new(vec!["echo".into()]);
        let tool = ShellBackgroundTool::new(policy);

        let saida = tool
            .execute(serde_json::json!({ "action": "start", "command": "echo oi" }))
            .await;

        assert!(!saida.is_error);
        assert!(saida.content.contains("id=bg-"));

        let id = extrai_campo(&saida.content, "id").to_string();
        let _ = tool
            .execute(serde_json::json!({ "action": "stop", "id": id }))
            .await;
    }

    #[tokio::test]
    async fn start_nao_bloqueia_ate_o_processo_terminar() {
        let policy = ShellPolicy::new(vec!["sleep".into()]);
        let tool = ShellBackgroundTool::new(policy);

        let inicio = std::time::Instant::now();
        let saida = tool
            .execute(serde_json::json!({ "action": "start", "command": "sleep 5" }))
            .await;
        let decorrido = inicio.elapsed();

        assert!(!saida.is_error);
        assert!(
            decorrido < std::time::Duration::from_secs(2),
            "start deveria devolver quase imediatamente, sem esperar o comando terminar \
             (levou {decorrido:?})"
        );

        let id = extrai_campo(&saida.content, "id").to_string();
        let _ = tool
            .execute(serde_json::json!({ "action": "stop", "id": id }))
            .await;
    }

    #[tokio::test]
    async fn output_devolve_a_saida_acumulada_e_drena_a_cada_consulta() {
        let policy = ShellPolicy::new(vec!["echo".into()]);
        let tool = ShellBackgroundTool::new(policy);

        let inicio = tool
            .execute(serde_json::json!({ "action": "start", "command": "echo agentry-mt68" }))
            .await;
        assert!(!inicio.is_error);
        let id = extrai_campo(&inicio.content, "id").to_string();

        // Dá tempo à tarefa de leitura em segundo plano capturar a saída —
        // `ler_saida` em si nunca espera o processo terminar, só lê o
        // buffer já preenchido por `acumula_stream`.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        let saida1 = tool
            .execute(serde_json::json!({ "action": "output", "id": id.clone() }))
            .await;
        assert!(!saida1.is_error);
        assert!(saida1.content.contains("agentry-mt68"));

        let saida2 = tool
            .execute(serde_json::json!({ "action": "output", "id": id.clone() }))
            .await;
        assert!(
            saida2.content.contains("sem saída nova"),
            "segunda consulta deve vir vazia -- a primeira já drenou o buffer"
        );

        let _ = tool
            .execute(serde_json::json!({ "action": "stop", "id": id }))
            .await;
    }

    #[tokio::test]
    async fn stop_finaliza_o_processo_de_fato() {
        let policy = ShellPolicy::new(vec!["sleep".into()]);
        let tool = ShellBackgroundTool::new(policy);

        let inicio = tool
            .execute(serde_json::json!({ "action": "start", "command": "sleep 30" }))
            .await;
        assert!(!inicio.is_error);
        let pid: u32 = extrai_campo(&inicio.content, "pid").parse().unwrap();
        let id = extrai_campo(&inicio.content, "id").to_string();

        let parado = tool
            .execute(serde_json::json!({ "action": "stop", "id": id }))
            .await;
        assert!(!parado.is_error);

        // Dá tempo ao SO reaproveitar o PID antes de checar.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert!(
            !processo_existe(pid),
            "processo deveria ter sido finalizado de fato pelo stop"
        );
    }

    #[tokio::test]
    async fn id_desconhecido_em_output_ou_stop_e_erro_tratado_sem_panic() {
        let tool = ShellBackgroundTool::new(ShellPolicy::default());

        let saida_output = tool
            .execute(serde_json::json!({ "action": "output", "id": "bg-999" }))
            .await;
        assert!(saida_output.is_error);

        let saida_stop = tool
            .execute(serde_json::json!({ "action": "stop", "id": "bg-999" }))
            .await;
        assert!(saida_stop.is_error);
    }

    #[tokio::test]
    async fn action_desconhecida_e_erro_tratado() {
        let tool = ShellBackgroundTool::new(ShellPolicy::default());

        let saida = tool.execute(serde_json::json!({ "action": "voar" })).await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn action_start_sem_command_e_erro_tratado() {
        let tool = ShellBackgroundTool::new(ShellPolicy::default());

        let saida = tool.execute(serde_json::json!({ "action": "start" })).await;

        assert!(saida.is_error);
    }
}
