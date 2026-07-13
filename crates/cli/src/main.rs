// Caminho relativo: crates/cli/src/main.rs
//! Ponto de entrada da CLI `agentry` (MT-14).
//!
//! Monta configuração (MT-04), transporte+allowlist (MT-05/07), o `Router`
//! com o provider Ollama (MT-08/09), o `ToolRegistry` com as tools de fs
//! (MT-12) e shell (MT-13), e despacha para um dos dois modos:
//!
//! - **One-shot** (`agentry "<tarefa>"`): roda um único turno (com o loop de
//!   tool-calls interno de [`agentry_core::session::Session::run_streaming`])
//!   e sai.
//! - **REPL** (sem tarefa na invocação): entra em [`repl::run_repl`], que
//!   aceita mensagens e comandos de barra até `/exit`/`/quit`/EOF.
//!
//! Em ambos os modos, as flags de override (`--model`, `--temperature`,
//! `--top-p`, `--max-tokens`, `--system`, `--reasoning`) montam o
//! [`RuntimeOverride`] inicial (ADR-0014/MT-33): no one-shot, vale só para
//! aquela invocação (o processo roda uma vez e sai); no REPL, vira o estado
//! de sessão inicial, que os comandos de barra atualizam a partir daí.

mod repl;
mod streaming;
mod tool_executor;

use std::io;
use std::sync::Arc;

use clap::Parser;

use agentry_core::config::{Config, Settings};
use agentry_core::egress::allowlist::{Allowlist, AllowlistEntry};
use agentry_core::egress::audit::AuditEntry;
use agentry_core::provider::ollama::OllamaProvider;
use agentry_core::provider::LlmProvider;
use agentry_core::router::{CallPreset, Router, RuntimeOverride};
use agentry_core::session::{Session, TokenBudget, ToolExecutor};
use agentry_core::tools::code_search::{register_code_search_tool, CodeSearchSession};
use agentry_core::tools::fs::{FsEditTool, FsReadTool, FsSearchTool, FsWriteTool};
use agentry_core::tools::lsp::{register_lsp_tools, LspSession};
use agentry_core::tools::permission::PermissionGate;
use agentry_core::tools::repo_map::{register_repo_map_tool, RepoMapTool};
use agentry_core::tools::shell::{ShellPolicy, ShellTool};
use agentry_core::tools::ToolRegistry;
use agentry_core::transport::{AuditSink, Transport};

use repl::parse_bool_toggle;
use tool_executor::{InteractiveConfirmer, RegistryToolExecutor};

/// Modelo Ollama usado quando `--model` não é informado.
const DEFAULT_MODEL: &str = "llama3.1:8b";
/// Host:porta padrão do servidor Ollama local.
const DEFAULT_OLLAMA_HOST: &str = "127.0.0.1:11434";
/// Orçamento de tokens usado quando `max_tokens` não está definido em
/// nenhuma camada de configuração.
const DEFAULT_TOKEN_BUDGET: u64 = 100_000;
/// *Language server* usado por `lsp_hover`/`lsp_definition` (MT-24, ADR-0013)
/// quando `context.lspGrounding.enabled` está ativo. Seleção por linguagem
/// (detectar o projeto e escolher o LS certo) fica para um ticket futuro —
/// fora de escopo do MT-40 ("UI/CLI de configuração"); `rust-analyzer` é o
/// default razoável hoje porque o próprio `agentry` é um workspace Rust.
/// Ausência do binário no `PATH` já é erro tratado pelo `LspClient` (MT-23),
/// nunca *panic*.
const DEFAULT_LSP_COMMAND: &str = "rust-analyzer";

/// CLI agêntica de codificação (multi-provedor, roteamento por classe de
/// privacidade) — v0.1 fala só com um servidor Ollama local.
#[derive(Parser, Debug)]
#[command(name = "agentry", version, about)]
struct Args {
    /// Tarefa a rodar em modo one-shot; ausente inicia o REPL interativo.
    tarefa: Option<String>,

    /// Modelo a usar nesta invocação (sobrescreve o padrão).
    #[arg(long, short = 'm')]
    model: Option<String>,

    /// Temperatura de amostragem desta invocação.
    #[arg(long)]
    temperature: Option<f32>,

    /// *Top-p* (*nucleus sampling*) desta invocação.
    #[arg(long = "top-p")]
    top_p: Option<f32>,

    /// Limite de tokens de saída desta invocação.
    #[arg(long = "max-tokens")]
    max_tokens: Option<u32>,

    /// *System prompt* desta invocação.
    #[arg(long)]
    system: Option<String>,

    /// Raciocínio estendido (`on`/`off`), se o modelo suportar.
    #[arg(long)]
    reasoning: Option<String>,

    /// Host:porta do servidor Ollama local.
    #[arg(long = "ollama-host", default_value = DEFAULT_OLLAMA_HOST)]
    ollama_host: String,
}

/// Emite cada [`AuditEntry`] de egresso em stderr — suficiente para a v0.1;
/// persistência estruturada (arquivo/serviço) fica para quando houver
/// demanda concreta. Usa o `Display` de `AuditEntry` (uma linha compacta),
/// não `{:?}` — o *dump* de `Debug` poluía o stderr (achado real do teste
/// de usabilidade, `scripts/usability-test.sh`).
struct StderrAuditSink;

impl AuditSink for StderrAuditSink {
    fn record(&self, entry: AuditEntry) {
        eprintln!("[audit] {entry}");
    }
}

/// Registra as 3 tools de contexto (`repo_map`, `code_search`,
/// `lsp_hover`/`lsp_definition`) segundo as 3 flags booleanas resolvidas por
/// `Config` (MT-39/ADR-0018) — extraído de `main()` para ser testável sem
/// rodar o binário inteiro (parsing de argv, rede real etc., MT-40).
/// `ollama_provider` é reaproveitado do provider Ollama já registrado no
/// `Router` (embeddings/reranking do RAG semântico, ADR-0011), não um
/// segundo cliente.
fn register_context_tools(
    registry: &mut ToolRegistry,
    cfg: &Config,
    workspace_root: &std::path::Path,
    ollama_provider: Arc<dyn LlmProvider>,
    modelo: &str,
) {
    register_repo_map_tool(
        registry,
        cfg.repo_map_enabled,
        RepoMapTool::new(workspace_root.to_path_buf()),
    );
    register_code_search_tool(
        registry,
        cfg.semantic_rag_enabled,
        Arc::new(CodeSearchSession::new(
            workspace_root.to_path_buf(),
            ollama_provider,
            modelo,
            modelo,
        )),
    );
    register_lsp_tools(
        registry,
        cfg.lsp_grounding_enabled,
        Arc::new(LspSession::new(
            DEFAULT_LSP_COMMAND,
            vec![],
            workspace_root.to_path_buf(),
        )),
    );
}

/// Monta o `RuntimeOverride` inicial a partir das flags de invocação.
///
/// # Errors
///
/// Devolve erro se `--reasoning` vier com um valor que não seja `on`/`off`
/// (e variantes) — falha explícita em vez de ignorar a flag em silêncio.
fn overrides_from_args(args: &Args) -> Result<RuntimeOverride, String> {
    let reasoning = args
        .reasoning
        .as_deref()
        .map(parse_bool_toggle)
        .transpose()?;
    Ok(RuntimeOverride {
        provider: None,
        model: args.model.clone(),
        temperature: args.temperature,
        top_p: args.top_p,
        system_prompt: args.system.clone(),
        max_tokens: args.max_tokens,
        reasoning,
    })
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let overrides = overrides_from_args(&args).unwrap_or_else(|erro| {
        eprintln!("erro: {erro}");
        std::process::exit(2)
    });

    let cfg = Config::resolve(vec![Settings::from_process_env().unwrap_or_else(|erro| {
        eprintln!("erro de configuração: {erro}");
        std::process::exit(2);
    })]);

    let ollama_ip = args
        .ollama_host
        .split(':')
        .next()
        .unwrap_or(&args.ollama_host)
        .to_string();
    let allowlist = Allowlist::new(vec![AllowlistEntry::new(
        ollama_ip,
        agentry_core::config::privacy::EgressClass::LocalOnly,
    )]);
    let transport = Arc::new(Transport::new(
        allowlist,
        cfg.egress_class,
        cfg.profile.map(|p| format!("{p:?}")),
        Arc::new(StderrAuditSink),
    ));
    let ollama = Arc::new(
        OllamaProvider::new(transport, format!("http://{}", args.ollama_host))
            .with_structured_output(cfg.ollama_structured_output),
    );
    // Clonado antes de `register_provider` consumir o Arc — o repo_map/RAG
    // semântico reaproveita o mesmo provider Ollama para embeddings/reranking
    // (ADR-0011), não um segundo cliente.
    let ollama_provider: Arc<dyn LlmProvider> = ollama.clone();

    let mut router = Router::new(cfg.egress_class);
    router.register_provider(ollama);
    let modelo_inicial = args
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    repl::set_chat_route(&mut router, &modelo_inicial, &CallPreset::default());

    let rota = router
        .resolve_with_override(repl::TASK_CLASS, &overrides)
        .unwrap_or_else(|erro| {
            eprintln!("erro ao resolver rota: {erro}");
            std::process::exit(1)
        });

    let workspace_root = std::env::current_dir().unwrap_or_else(|erro| {
        eprintln!("erro ao ler diretório de trabalho: {erro}");
        std::process::exit(1)
    });

    let mut registry = ToolRegistry::new(PermissionGate::new(cfg.permissions.clone()));
    registry.register(Arc::new(FsReadTool::new(workspace_root.clone())));
    registry.register(Arc::new(FsWriteTool::new(workspace_root.clone())));
    registry.register(Arc::new(FsEditTool::new(workspace_root.clone())));
    registry.register(Arc::new(FsSearchTool::new(workspace_root.clone())));
    // Sem padrões de `allow` configuráveis ainda (fora de escopo do MT-14):
    // shell fica bloqueado por padrão (default-deny da `ShellPolicy`, MT-13).
    registry.register(Arc::new(ShellTool::new(ShellPolicy::new(vec![]))));

    register_context_tools(
        &mut registry,
        &cfg,
        &workspace_root,
        ollama_provider,
        &modelo_inicial,
    );

    let executor: Arc<dyn ToolExecutor> = Arc::new(RegistryToolExecutor::new(
        registry,
        Arc::new(InteractiveConfirmer),
    ));

    let budget = cfg
        .max_tokens
        .map(u64::from)
        .unwrap_or(DEFAULT_TOKEN_BUDGET);
    let mut session = Session::new(rota, executor, TokenBudget::new(budget));

    if let Some(tarefa) = args.tarefa {
        session.push_user_message(tarefa);
        streaming::stream_to_writer(&mut session, io::stdout(), &router)
            .await
            .unwrap_or_else(|erro| {
                eprintln!("erro: {erro}");
                std::process::exit(1)
            });
    } else {
        let stdin = io::stdin();
        repl::run_repl(
            stdin.lock(),
            io::stdout(),
            &mut session,
            &mut router,
            &CallPreset::default(),
            overrides,
        )
        .await
        .unwrap_or_else(|erro| {
            eprintln!("erro: {erro}");
            std::process::exit(1);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::config::privacy::EgressClass;
    use agentry_core::config::Permissions;
    use agentry_core::provider::mock::MockProvider;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `state_dir`/`config`/`tools::*`, MT-38/39).
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-cli-main-test-{}-{}",
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

    fn cfg_com_flags(repo_map: bool, semantic_rag: bool, lsp_grounding: bool) -> Config {
        Config {
            profile: None,
            egress_class: EgressClass::LocalOnly,
            model: None,
            max_tokens: None,
            permissions: Permissions::default(),
            repo_map_enabled: repo_map,
            semantic_rag_enabled: semantic_rag,
            lsp_grounding_enabled: lsp_grounding,
            ollama_structured_output: true,
        }
    }

    fn nomes_registrados(registry: &ToolRegistry) -> Vec<String> {
        registry.specs().into_iter().map(|s| s.name).collect()
    }

    #[test]
    fn flags_true_registra_as_3_tools_de_contexto() {
        let dir = TempDir::new();
        let cfg = cfg_com_flags(true, true, true);
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("mock"));

        register_context_tools(&mut registry, &cfg, dir.path(), provider, "modelo-teste");

        let nomes = nomes_registrados(&registry);
        assert!(nomes.contains(&"repo_map".to_string()));
        assert!(nomes.contains(&"code_search".to_string()));
        assert!(nomes.contains(&"lsp_hover".to_string()));
        assert!(nomes.contains(&"lsp_definition".to_string()));
    }

    #[test]
    fn flags_false_nao_registra_nenhuma_das_3_tools_de_contexto() {
        let dir = TempDir::new();
        let cfg = cfg_com_flags(false, false, false);
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("mock"));

        register_context_tools(&mut registry, &cfg, dir.path(), provider, "modelo-teste");

        let nomes = nomes_registrados(&registry);
        assert!(!nomes.contains(&"repo_map".to_string()));
        assert!(!nomes.contains(&"code_search".to_string()));
        assert!(!nomes.contains(&"lsp_hover".to_string()));
        assert!(!nomes.contains(&"lsp_definition".to_string()));
    }

    #[test]
    fn ausencia_do_arquivo_preserva_o_comportamento_anterior_todas_true() {
        // Mesmo critério de aceite do MT-39: sem `.agentry/agentry.settings.json`,
        // `Config::resolve` cai nos defaults do ADR-0018 (todas `true`) — e,
        // por extensão, as 3 tools de contexto continuam registradas.
        let cfg = Config::resolve(vec![Settings::default()]);
        let dir = TempDir::new();
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("mock"));

        register_context_tools(&mut registry, &cfg, dir.path(), provider, "modelo-teste");

        let nomes = nomes_registrados(&registry);
        assert!(nomes.contains(&"repo_map".to_string()));
        assert!(nomes.contains(&"code_search".to_string()));
        assert!(nomes.contains(&"lsp_hover".to_string()));
        assert!(nomes.contains(&"lsp_definition".to_string()));
    }
}
