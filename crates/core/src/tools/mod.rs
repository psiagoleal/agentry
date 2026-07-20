// Caminho relativo: crates/core/src/tools/mod.rs
//! Tool Registry (MT-11): `trait Tool`, registro e decisão de execução sob o
//! gate de permissão ([`permission`]). [`fs`] traz as tools de filesystem
//! (MT-12); [`shell`] traz a tool de shell sob permissão (MT-13, com sua
//! própria política *default-deny*, mais restritiva que o gate genérico);
//! [`repo_map`] expõe o repo-map estilo Aider (MT-19/20) como tool (MT-21);
//! [`lsp`] expõe *grounding* via LSP (hover/definição, MT-23) como tool
//! (MT-24); [`code_search`] expõe a busca híbrida do RAG semântico
//! (MT-26..28) como tool, com indexação incremental (MT-29) por trás
//! (MT-30). **`ask` nunca bloqueia esperando um humano** — sinaliza
//! devolvendo a [`ToolCall`] pendente; quem interage com o usuário (a
//! CLI, MT-14) decide o que fazer com esse sinal.

pub mod ask_user;
pub mod checkpoint;
pub mod code_search;
pub mod fs;
pub mod glob;
pub mod lsp;
pub mod mcp;
pub mod permission;
pub mod repo_map;
pub mod shell;
pub mod skill;
pub mod subagent;
pub mod todo;
pub mod web_fetch;
pub mod web_search;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use permission::{Permission, PermissionGate};

use crate::model::{ToolCall, ToolResult};
use crate::provider::{BoxFuture, ToolSpec};

/// Nome canônico do arquivo de ignore do `agentry` (ADR-0020) — artefato
/// próprio do `agentry`, não mais do contrato de interop v1 (ver ADR-0003
/// emendada). Checado **antes** do legado.
pub(crate) const IGNORE_FILE_NAME: &str = ".agentryignore";
/// Nome legado (`.claudeignore`, herdado do contrato de interop pré-ADR-0020)
/// — mantido só como *fallback* de compatibilidade quando
/// [`IGNORE_FILE_NAME`] está ausente (MT-52).
pub(crate) const LEGACY_IGNORE_FILE_NAME: &str = ".claudeignore";

/// Decide qual arquivo de ignore usar em `root`: `.agentryignore` se
/// presente, senão `.claudeignore` (*fallback* de compatibilidade). Se os
/// dois existirem, `.agentryignore` vence **sozinho** — nunca um merge dos
/// dois padrões (ADR-0020 §2). Compartilhada por `fs`/`repo_map`/
/// `code_search` para não triplicar a mesma decisão.
pub(crate) fn resolve_ignore_file_name(root: &Path) -> &'static str {
    if root.join(IGNORE_FILE_NAME).is_file() {
        IGNORE_FILE_NAME
    } else {
        LEGACY_IGNORE_FILE_NAME
    }
}

/// Resultado bruto da execução de uma tool — sem `call_id`, que pertence à
/// chamada ([`ToolCall`], MT-02), não à tool em si.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolOutput {
    /// Conteúdo textual do resultado.
    pub content: String,
    /// Indica se a execução falhou.
    pub is_error: bool,
}

impl ToolOutput {
    /// Cria uma saída de sucesso.
    #[must_use]
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    /// Cria uma saída de erro.
    #[must_use]
    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}

/// Uma tool executável pelo agent loop.
///
/// Dyn-compatible via [`BoxFuture`], no mesmo padrão de `LlmProvider` (MT-03)
/// e `ToolExecutor` (MT-10) — sem `async-trait`.
pub trait Tool: Send + Sync {
    /// Nome único da tool (chave no registro e no gate de permissão).
    fn name(&self) -> &str;
    /// Descrição da tool, para o [`ToolSpec`] oferecido ao modelo.
    fn description(&self) -> &str;
    /// JSON Schema dos argumentos aceitos.
    fn input_schema(&self) -> serde_json::Value;
    /// Executa a tool com os argumentos dados.
    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput>;
}

/// Decisão de execução de uma [`ToolCall`] pelo registro.
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionOutcome {
    /// A tool rodou (`allow`); resultado observável pelo agent loop.
    Executed(ToolResult),
    /// A tool está sob `ask`: não rodou; quem chama decide se confirma.
    NeedsConfirmation(ToolCall),
    /// A tool está sob `deny`, ou não está registrada: nunca roda.
    Denied(ToolResult),
}

/// Registro de tools + gate de permissão.
pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    gate: PermissionGate,
}

impl ToolRegistry {
    /// Cria um registro vazio sob o gate de permissão dado.
    #[must_use]
    pub fn new(gate: PermissionGate) -> Self {
        Self {
            tools: HashMap::new(),
            gate,
        }
    }

    /// Registra uma tool (chave: [`Tool::name`]).
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    /// As tools registradas — usado para **reaproveitar** as mesmas
    /// instâncias `Arc<dyn Tool>` num segundo `ToolRegistry` (MT-91/
    /// ADR-0031: o executor interno de `SubagentTool` precisa das mesmas
    /// tools, exceto a própria `subagent`, sem reconstruir nenhuma).
    pub fn tools(&self) -> impl Iterator<Item = &Arc<dyn Tool>> {
        self.tools.values()
    }

    /// Especificações das tools registradas, prontas para `ChatRequest::tools`.
    #[must_use]
    pub fn specs(&self) -> Vec<ToolSpec> {
        self.tools
            .values()
            .map(|tool| ToolSpec {
                name: tool.name().to_string(),
                description: tool.description().to_string(),
                input_schema: tool.input_schema(),
            })
            .collect()
    }

    /// Decide a permissão de `call` e, se `allow`, executa.
    ///
    /// `deny` (explícito ou por ausência de registro) devolve um
    /// [`ToolResult`] de erro sem executar nada; `ask` devolve a
    /// [`ToolCall`] pendente sem executar — a confirmação é responsabilidade
    /// de quem chama.
    pub async fn execute(&self, call: &ToolCall) -> ExecutionOutcome {
        match self.gate.decide(&call.name) {
            Permission::Deny => ExecutionOutcome::Denied(ToolResult {
                call_id: call.id.clone(),
                content: format!("tool '{}' bloqueada por política (deny)", call.name),
                is_error: true,
            }),
            Permission::Ask => ExecutionOutcome::NeedsConfirmation(call.clone()),
            Permission::Allow => self.run_tool(call).await,
        }
    }

    /// Executa `call` diretamente, **sem** consultar o gate de permissão.
    ///
    /// Uso previsto: depois que quem chama já obteve, por fora (ex.: prompt
    /// interativo na CLI, MT-14), a confirmação humana que um
    /// [`ExecutionOutcome::NeedsConfirmation`] pedia — reconsultar o gate
    /// aqui devolveria `NeedsConfirmation` de novo, em loop. `deny` não se
    /// aplica: se a tool não estava registrada nem sob `allow`, este método
    /// só deve ser chamado em resposta a um `NeedsConfirmation` genuíno.
    pub async fn execute_confirmed(&self, call: &ToolCall) -> ToolResult {
        match self.run_tool(call).await {
            ExecutionOutcome::Executed(result) | ExecutionOutcome::Denied(result) => result,
            ExecutionOutcome::NeedsConfirmation(_) => unreachable!(
                "run_tool nunca devolve NeedsConfirmation — só decide entre executar e negar"
            ),
        }
    }

    /// Executa a tool de `call` se registrada, sem checar o gate.
    async fn run_tool(&self, call: &ToolCall) -> ExecutionOutcome {
        match self.tools.get(&call.name) {
            Some(tool) => {
                let output = tool.execute(call.arguments.clone()).await;
                ExecutionOutcome::Executed(ToolResult {
                    call_id: call.id.clone(),
                    content: output.content,
                    is_error: output.is_error,
                })
            }
            None => ExecutionOutcome::Denied(ToolResult {
                call_id: call.id.clone(),
                content: format!("tool '{}' não registrada", call.name),
                is_error: true,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;

    struct DummyTool;

    impl Tool for DummyTool {
        fn name(&self) -> &str {
            "dummy"
        }

        fn description(&self) -> &str {
            "tool de teste"
        }

        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({ "type": "object" })
        }

        fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
            Box::pin(async move { ToolOutput::ok(format!("executado com {arguments}")) })
        }
    }

    fn call(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: id.into(),
            name: name.into(),
            arguments: serde_json::json!({ "x": 1 }),
        }
    }

    fn gate(deny: &[&str], ask: &[&str]) -> PermissionGate {
        PermissionGate::new(Permissions {
            deny: deny.iter().map(|s| (*s).to_string()).collect(),
            ask: ask.iter().map(|s| (*s).to_string()).collect(),
        })
    }

    fn registry_with_dummy(deny: &[&str], ask: &[&str]) -> ToolRegistry {
        let mut registry = ToolRegistry::new(gate(deny, ask));
        registry.register(Arc::new(DummyTool));
        registry
    }

    #[tokio::test]
    async fn allow_executa_a_tool() {
        let registry = registry_with_dummy(&[], &[]);
        let outcome = registry.execute(&call("call-1", "dummy")).await;
        match outcome {
            ExecutionOutcome::Executed(result) => {
                assert_eq!(result.call_id, "call-1");
                assert!(!result.is_error);
                assert!(result.content.contains("executado com"));
            }
            other => panic!("esperava Executed, veio {other:?}"),
        }
    }

    #[tokio::test]
    async fn deny_bloqueia_sem_executar() {
        let registry = registry_with_dummy(&["dummy"], &[]);
        let outcome = registry.execute(&call("call-1", "dummy")).await;
        match outcome {
            ExecutionOutcome::Denied(result) => {
                assert_eq!(result.call_id, "call-1");
                assert!(result.is_error);
            }
            other => panic!("esperava Denied, veio {other:?}"),
        }
    }

    #[tokio::test]
    async fn ask_sinaliza_sem_executar() {
        let registry = registry_with_dummy(&[], &["dummy"]);
        let chamada = call("call-1", "dummy");
        let outcome = registry.execute(&chamada).await;
        assert_eq!(outcome, ExecutionOutcome::NeedsConfirmation(chamada));
    }

    #[tokio::test]
    async fn tool_nao_registrada_sob_allow_e_denied() {
        // Gate vazio (allow por padrão), mas nenhuma tool registrada.
        let registry = ToolRegistry::new(gate(&[], &[]));
        let outcome = registry.execute(&call("call-1", "nao-existe")).await;
        assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
    }

    #[tokio::test]
    async fn execute_confirmed_roda_a_tool_mesmo_sob_ask() {
        // ask nunca deveria rodar via execute(); execute_confirmed() é o
        // caminho de quem já obteve a confirmação humana por fora (CLI, MT-14).
        let registry = registry_with_dummy(&[], &["dummy"]);
        let resultado = registry.execute_confirmed(&call("call-1", "dummy")).await;
        assert!(!resultado.is_error);
        assert!(resultado.content.contains("executado com"));
    }

    #[tokio::test]
    async fn execute_confirmed_de_tool_nao_registrada_e_erro_sem_panico() {
        let registry = ToolRegistry::new(gate(&[], &[]));
        let resultado = registry
            .execute_confirmed(&call("call-1", "nao-existe"))
            .await;
        assert!(resultado.is_error);
    }

    #[test]
    fn specs_refletem_as_tools_registradas() {
        let registry = registry_with_dummy(&[], &[]);
        let specs = registry.specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(specs[0].name, "dummy");
        assert_eq!(specs[0].description, "tool de teste");
    }

    // --- MT-52: resolve_ignore_file_name (ADR-0020) ---

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-tools-mod-test-{}-{}",
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

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolve_ignore_file_name_sem_nenhum_arquivo_cai_no_legado() {
        let dir = TempDir::new();
        assert_eq!(
            resolve_ignore_file_name(dir.path()),
            LEGACY_IGNORE_FILE_NAME
        );
    }

    #[test]
    fn resolve_ignore_file_name_so_com_agentryignore() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(IGNORE_FILE_NAME), "").unwrap();
        assert_eq!(resolve_ignore_file_name(dir.path()), IGNORE_FILE_NAME);
    }

    #[test]
    fn resolve_ignore_file_name_so_com_claudeignore_legado() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(LEGACY_IGNORE_FILE_NAME), "").unwrap();
        assert_eq!(
            resolve_ignore_file_name(dir.path()),
            LEGACY_IGNORE_FILE_NAME
        );
    }

    #[test]
    fn resolve_ignore_file_name_com_os_dois_vence_agentryignore_sozinho() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join(IGNORE_FILE_NAME), "").unwrap();
        std::fs::write(dir.path().join(LEGACY_IGNORE_FILE_NAME), "").unwrap();
        assert_eq!(
            resolve_ignore_file_name(dir.path()),
            IGNORE_FILE_NAME,
            "ADR-0020 §2: nunca faz merge dos dois, .agentryignore vence sozinho"
        );
    }
}
