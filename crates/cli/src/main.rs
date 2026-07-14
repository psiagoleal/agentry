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
//!
//! A flag `--init` (e o comando `/init` do REPL) materializam
//! `.agentry/agentry.settings.json`: sem `--profile`, o exemplo genérico do
//! schema mínimo (ADR-0018 §5), sem nenhuma chamada de rede (ADR-0019 §1,
//! MT-41); com `--profile <nome>`, busca o artefato real daquele perfil no
//! repositório irmão `ai-coding-agent-profiles` (ver [`init`], MT-42).

mod init;
mod repl;
mod streaming;
mod tool_executor;

use std::io;
use std::sync::Arc;

use clap::Parser;

use agentry_core::config::{Config, Settings};
use agentry_core::egress::allowlist::{Allowlist, AllowlistEntry};
use agentry_core::egress::audit::AuditEntry;
use agentry_core::guardrail::{GuardrailAuditEntry, GuardrailAuditSink};
use agentry_core::provider::ollama::OllamaProvider;
use agentry_core::provider::LlmProvider;
use agentry_core::router::{CallPreset, Router, RuntimeOverride};
use agentry_core::session::{Session, TokenBudget, ToolExecutor};
use agentry_core::state_dir;
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
/// Exemplo genérico do schema mínimo (ADR-0018 §5) — conteúdo exato gravado
/// por `--init`/`/init` quando nenhum `--profile` é informado (ADR-0019 §1,
/// MT-41): todas as flags de contexto/provider em `true`, permissões
/// vazias. Busca de valores diferenciados por perfil fica para o MT-42.
const GENERIC_SETTINGS_EXAMPLE: &str = r#"{
  "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
  "schemaVersion": 1,
  "permissions": {
    "deny": [],
    "ask": []
  },
  "context": {
    "repoMap": { "enabled": true },
    "semanticRag": { "enabled": true },
    "lspGrounding": { "enabled": true }
  },
  "providers": {
    "ollama": { "structuredOutput": true }
  }
}
"#;
/// Comando manual equivalente, sempre exibido por `--init`/`/init` (ADR-0019
/// §5) — para quem preferir os valores diferenciados por perfil
/// (`empresa`/`externo-confidencial`/`pessoal`) do `ai-coding-agent-profiles`
/// em vez do exemplo genérico, inspecionando/rodando por conta própria.
const MANUAL_SETUP_HINT: &str = "dica: para valores diferenciados por perfil (empresa/externo-confidencial/pessoal), rode o scripts/setup-profile.sh de https://github.com/psiagoleal/ai-coding-agent-profiles";

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

    /// Cria `.agentry/agentry.settings.json` (bootstrap, ADR-0019) e sai —
    /// sem `--profile`, usa o exemplo genérico do schema mínimo (ADR-0018
    /// §5), sem nenhuma chamada de rede.
    #[arg(long, conflicts_with = "tarefa")]
    init: bool,

    /// Com `--init`: busca o `agentry.settings.json` real deste perfil
    /// (`empresa`/`externo-confidencial`/`pessoal`) no `ai-coding-agent-profiles`
    /// público, numa referência fixa (MT-42, ADR-0019 §2-4) — em vez do
    /// exemplo genérico.
    #[arg(long, requires = "init")]
    profile: Option<String>,
}

/// Resultado de [`run_init_local`] — usado tanto por `--init` quanto por
/// `/init` (MT-41) para formatar a mesma mensagem sem duplicar a decisão.
enum InitOutcome {
    /// Arquivo criado agora, no caminho dado.
    Created(std::path::PathBuf),
    /// Arquivo já existia — não sobrescrito (mesma idempotência de
    /// `state_dir::ensure_state_dir` para o `.gitignore`, MT-38).
    AlreadyExists(std::path::PathBuf),
}

/// Grava `conteudo` em `.agentry/agentry.settings.json` — reaproveita
/// `state_dir::ensure_state_dir` (cria o diretório de estado e seu
/// `.gitignore`, MT-38) e `state_dir::agentry_settings_path` (MT-39) para
/// resolver o caminho final; nunca sobrescreve um arquivo já existente.
/// Compartilhada entre o bootstrap local (MT-41) e o via rede com
/// `--profile` (MT-42) — a diferença entre os dois é só de onde `conteudo`
/// vem, nunca de como é gravado.
///
/// # Errors
///
/// Devolve o `io::Error` de criar o diretório de estado ou escrever o
/// arquivo, sem tratamento especial.
fn write_settings_if_absent(
    workspace_root: &std::path::Path,
    conteudo: &str,
) -> io::Result<InitOutcome> {
    state_dir::ensure_state_dir(workspace_root)?;
    let caminho = state_dir::agentry_settings_path(workspace_root);
    if caminho.exists() {
        return Ok(InitOutcome::AlreadyExists(caminho));
    }
    std::fs::write(&caminho, conteudo)?;
    Ok(InitOutcome::Created(caminho))
}

/// Materializa `.agentry/agentry.settings.json` com o exemplo genérico do
/// schema mínimo (ADR-0018 §5) — bootstrap local, sem `--profile` (ADR-0019
/// §1), zero rede.
///
/// # Errors
///
/// Ver [`write_settings_if_absent`].
fn run_init_local(workspace_root: &std::path::Path) -> io::Result<InitOutcome> {
    write_settings_if_absent(workspace_root, GENERIC_SETTINGS_EXAMPLE)
}

/// Escreve o resultado de [`run_init_local`] em `output`, sempre seguido do
/// comando manual equivalente (ADR-0019 §5) — usado tanto por `--init`
/// quanto por `/init`, para não duplicar a mensagem entre CLI e REPL.
fn escrever_resultado_init(outcome: &InitOutcome, output: &mut impl io::Write) -> io::Result<()> {
    match outcome {
        InitOutcome::Created(caminho) => {
            writeln!(output, "criado: {}", caminho.display())?;
        }
        InitOutcome::AlreadyExists(caminho) => {
            writeln!(output, "já existe, não sobrescrito: {}", caminho.display())?;
        }
    }
    writeln!(output, "{MANUAL_SETUP_HINT}")
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

/// Emite cada [`GuardrailAuditEntry`] em stderr (MT-46) — `Display` já
/// compacto, uma linha por entrada, mesma disciplina do `impl AuditSink`
/// acima (nunca o `Debug` dump).
impl GuardrailAuditSink for StderrAuditSink {
    fn record(&self, entry: GuardrailAuditEntry) {
        eprintln!("{entry}");
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

/// Resolve a `Config` final a partir das duas camadas reais do binário:
/// `.agentry/agentry.settings.json` (`Settings::from_file`, MT-39) e as
/// variáveis de ambiente (`Settings::from_process_env`) — nesta ordem, para
/// que o ambiente sobrescreva o arquivo (mesma precedência documentada em
/// `Settings::from_file`). Extraída de `main()` para ser testável sem rodar
/// o binário inteiro (MT-46, mesmo padrão do MT-40/`register_context_tools`).
///
/// **Achado do MT-46:** até este ticket, `main()` só montava a `Config` a
/// partir de `Settings::from_process_env()` — a camada do arquivo nunca
/// entrava na chamada, apesar do MT-39/40 estarem fechados como "consumo
/// real"; as 4 flags de contexto/provider só funcionavam de fato via
/// variável de ambiente. Corrigido aqui porque o critério de aceite do
/// MT-46 depende diretamente disso (regra de guardrail no arquivo precisa
/// chegar à `Session` real).
///
/// # Errors
///
/// Propaga o `ConfigError` de qualquer uma das duas camadas (arquivo
/// malformado/`schemaVersion` divergente, ou variável de ambiente numérica
/// inválida) — nunca *panic*.
fn build_config(
    workspace_root: &std::path::Path,
) -> Result<Config, agentry_core::config::ConfigError> {
    Ok(Config::resolve(vec![
        Settings::from_file(workspace_root)?,
        Settings::from_process_env()?,
    ]))
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

    if args.init {
        let workspace_root = std::env::current_dir().unwrap_or_else(|erro| {
            eprintln!("erro ao ler diretório de trabalho: {erro}");
            std::process::exit(1)
        });
        let resultado = match &args.profile {
            Some(perfil) => {
                let sink: Arc<dyn AuditSink> = Arc::new(StderrAuditSink);
                match init::fetch_profile_settings(perfil, sink).await {
                    Ok(conteudo) => write_settings_if_absent(&workspace_root, &conteudo),
                    Err(erro) => {
                        eprintln!("erro ao buscar configuração do perfil: {erro}");
                        std::process::exit(1)
                    }
                }
            }
            None => run_init_local(&workspace_root),
        };
        match resultado {
            Ok(outcome) => {
                escrever_resultado_init(&outcome, &mut io::stdout()).unwrap_or_else(|erro| {
                    eprintln!("erro: {erro}");
                    std::process::exit(1)
                });
            }
            Err(erro) => {
                eprintln!("erro ao inicializar configuração: {erro}");
                std::process::exit(1)
            }
        }
        return;
    }

    let overrides = overrides_from_args(&args).unwrap_or_else(|erro| {
        eprintln!("erro: {erro}");
        std::process::exit(2)
    });

    let workspace_root = std::env::current_dir().unwrap_or_else(|erro| {
        eprintln!("erro ao ler diretório de trabalho: {erro}");
        std::process::exit(1)
    });

    let cfg = build_config(&workspace_root).unwrap_or_else(|erro| {
        eprintln!("erro de configuração: {erro}");
        std::process::exit(2)
    });

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
    let mut session = Session::new(rota, executor, TokenBudget::new(budget))
        .with_guardrails(Arc::new(cfg.guardrails), Arc::new(StderrAuditSink));

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
            &workspace_root,
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

    #[test]
    fn run_init_local_cria_o_arquivo_ausente_com_o_exemplo_exato_da_adr_0018() {
        let dir = TempDir::new();

        let outcome = run_init_local(dir.path()).expect("deve criar o arquivo");
        let caminho = match outcome {
            InitOutcome::Created(caminho) => caminho,
            InitOutcome::AlreadyExists(_) => panic!("arquivo não deveria existir ainda"),
        };

        let conteudo = std::fs::read_to_string(&caminho).expect("arquivo deve existir");
        assert_eq!(conteudo, GENERIC_SETTINGS_EXAMPLE);
    }

    #[test]
    fn run_init_local_nao_sobrescreve_arquivo_ja_existente() {
        let dir = TempDir::new();
        run_init_local(dir.path()).expect("primeira chamada deve criar");
        let caminho = state_dir::agentry_settings_path(dir.path());
        std::fs::write(&caminho, r#"{"customizado": true}"#)
            .expect("simula customização do usuário");

        let outcome = run_init_local(dir.path()).expect("segunda chamada não deve falhar");

        assert!(matches!(outcome, InitOutcome::AlreadyExists(_)));
        let conteudo = std::fs::read_to_string(&caminho).expect("arquivo deve continuar existindo");
        assert_eq!(
            conteudo, r#"{"customizado": true}"#,
            "customização do usuário não pode ser sobrescrita"
        );
    }

    #[test]
    fn escrever_resultado_init_sempre_inclui_o_comando_manual() {
        let dir = TempDir::new();

        let criado = run_init_local(dir.path()).expect("deve criar");
        let mut saida_criado = Vec::new();
        escrever_resultado_init(&criado, &mut saida_criado).expect("deve escrever");
        let texto_criado = String::from_utf8(saida_criado).unwrap();
        assert!(texto_criado.contains("criado:"));
        assert!(texto_criado.contains(MANUAL_SETUP_HINT));

        let ja_existente = run_init_local(dir.path()).expect("segunda chamada");
        let mut saida_existente = Vec::new();
        escrever_resultado_init(&ja_existente, &mut saida_existente).expect("deve escrever");
        let texto_existente = String::from_utf8(saida_existente).unwrap();
        assert!(texto_existente.contains("já existe"));
        assert!(texto_existente.contains(MANUAL_SETUP_HINT));
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
            guardrails: agentry_core::guardrail::GuardrailGate::default(),
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

    // --- MT-46: Guardrail Gate consumido de ponta a ponta na CLI real ---

    use agentry_core::router::ResolvedRoute;
    use agentry_core::session::StopReason;

    /// Monta uma `Session` real como `main()` faria (registry vazio,
    /// `RegistryToolExecutor` real, `MockProvider` no lugar do Ollama) — o
    /// suficiente para provar a fiação de `with_guardrails`, sem repetir o
    /// resto do `main()` que não é específico deste ticket.
    fn sessao_de_teste(cfg: &Config, mock: Arc<MockProvider>) -> Session {
        let route = ResolvedRoute::new(mock, "modelo-teste", CallPreset::default());
        let registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let executor: Arc<dyn ToolExecutor> = Arc::new(RegistryToolExecutor::new(
            registry,
            Arc::new(InteractiveConfirmer),
        ));
        Session::new(route, executor, TokenBudget::new(10_000))
            .with_guardrails(Arc::new(cfg.guardrails.clone()), Arc::new(StderrAuditSink))
    }

    fn router_vazio() -> agentry_core::router::Router {
        agentry_core::router::Router::new(EgressClass::LocalOnly)
    }

    fn escreve_settings(dir: &std::path::Path, conteudo: &str) {
        state_dir::ensure_state_dir(dir).expect("cria .agentry de teste");
        std::fs::write(state_dir::agentry_settings_path(dir), conteudo)
            .expect("grava agentry.settings.json de teste");
    }

    #[test]
    fn build_config_le_regras_de_guardrail_do_arquivo_real() {
        let dir = TempDir::new();
        escreve_settings(
            dir.path(),
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "guardrails": {
                "input": [{"id": "bloqueia-senha", "match": "senha:", "action": "block"}],
                "output": [{"id": "mascara-segredo", "match": "segredo-abc", "action": "redact"}]
              }
            }"#,
        );

        let cfg = build_config(dir.path()).expect("arquivo válido deve resolver");

        assert_eq!(cfg.guardrails.input.len(), 1);
        assert_eq!(cfg.guardrails.input[0].id, "bloqueia-senha");
        assert_eq!(cfg.guardrails.output.len(), 1);
        assert_eq!(cfg.guardrails.output[0].id, "mascara-segredo");
    }

    #[test]
    fn ausencia_do_arquivo_de_settings_preserva_guardrails_vazio() {
        let dir = TempDir::new();

        let cfg = build_config(dir.path()).expect("ausência do arquivo não é erro");

        assert!(cfg.guardrails.input.is_empty());
        assert!(cfg.guardrails.output.is_empty());
    }

    #[tokio::test]
    async fn agentry_settings_json_com_regra_de_entrada_block_bloqueia_de_ponta_a_ponta_via_sessao_real(
    ) {
        let dir = TempDir::new();
        escreve_settings(
            dir.path(),
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "guardrails": {
                "input": [{"id": "bloqueia-senha", "match": "senha:", "action": "block"}],
                "output": []
              }
            }"#,
        );
        let cfg = build_config(dir.path()).expect("arquivo válido deve resolver");
        let mock = Arc::new(MockProvider::new("mock"));
        // Nenhuma resposta enfileirada de propósito: se o provider fosse
        // chamado, o mock devolveria erro de fila vazia.
        let mut session = sessao_de_teste(&cfg, mock.clone());
        session.push_user_message("minha senha: 12345");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("bloqueio de entrada não deve ser erro");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(mock.chat_requests().len(), 0, "o provider nunca é chamado");
        assert_eq!(outcome.guardrail_hits.len(), 1);
    }

    #[tokio::test]
    async fn agentry_settings_json_com_regra_de_saida_redact_mascara_a_resposta_de_ponta_a_ponta() {
        let dir = TempDir::new();
        escreve_settings(
            dir.path(),
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "guardrails": {
                "input": [],
                "output": [{"id": "mascara-segredo", "match": "segredo-abc", "action": "redact"}]
              }
            }"#,
        );
        let cfg = build_config(dir.path()).expect("arquivo válido deve resolver");
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(agentry_core::provider::ChatResponse {
            message: agentry_core::model::Message::assistant("o valor é segredo-abc, guarde bem"),
            usage: agentry_core::model::Usage::default(),
        }));
        let mut session = sessao_de_teste(&cfg, mock.clone());
        session.push_user_message("qual o valor?");

        let outcome = session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        let ultima = session
            .messages()
            .last()
            .expect("deve ter uma resposta")
            .text_content();
        assert!(!ultima.contains("segredo-abc"));
        assert!(ultima.contains(agentry_core::egress::redact::REDACTED_PLACEHOLDER));
        assert_eq!(outcome.guardrail_hits.len(), 1);
    }
}
