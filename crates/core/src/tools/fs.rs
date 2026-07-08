// Caminho relativo: crates/core/src/tools/fs.rs
//! Tools de filesystem (MT-12): leitura, escrita/edição e busca de arquivos.
//!
//! Implementam [`Tool`] (MT-11) e rodam sob o mesmo `ToolRegistry`/gate de
//! permissão de qualquer outra tool — nenhuma lógica de permissão própria
//! aqui. Todo caminho é resolvido dentro de uma raiz fixa (workspace) e nunca
//! escapa dela: caminho absoluto ou com `..` é rejeitado antes de qualquer
//! I/O. Arquivos/diretórios cobertos por `.claudeignore` (mesma semântica de
//! `.gitignore`, via crate `ignore` — não reimplementada na mão) ficam
//! inacessíveis a estas tools.

use std::fs;
use std::path::{Component, Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use ignore::WalkBuilder;

use crate::provider::BoxFuture;
use crate::tools::{Tool, ToolOutput};

/// Nome do arquivo de ignore reconhecido por estas tools.
const IGNORE_FILE_NAME: &str = ".claudeignore";

/// Resolve `relative` dentro de `root`, rejeitando caminho absoluto ou que
/// contenha `..` — lógica pura, sem tocar o filesystem.
fn resolve_within_root(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let rel_path = Path::new(relative);
    if rel_path.is_absolute() {
        return Err(format!("caminho absoluto não permitido: '{relative}'"));
    }
    if rel_path
        .components()
        .any(|c| matches!(c, Component::ParentDir))
    {
        return Err(format!("caminho não pode conter '..': '{relative}'"));
    }
    Ok(root.join(rel_path))
}

/// Carrega o `.claudeignore` da raiz — ausência do arquivo é normal e trata
/// como "nada ignorado" (nunca erro).
fn load_ignore(root: &Path) -> Gitignore {
    let mut builder = GitignoreBuilder::new(root);
    let _ = builder.add(root.join(IGNORE_FILE_NAME));
    builder.build().unwrap_or_else(|_| Gitignore::empty())
}

/// Contexto compartilhado pelas tools de filesystem: raiz do workspace +
/// matcher de `.claudeignore` já carregado.
struct FsContext {
    root: PathBuf,
    ignore: Gitignore,
}

impl FsContext {
    fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let ignore = load_ignore(&root);
        Self { root, ignore }
    }

    /// Resolve e valida `relative`: dentro da raiz e não coberto por
    /// `.claudeignore`.
    fn resolve(&self, relative: &str) -> Result<PathBuf, String> {
        let path = resolve_within_root(&self.root, relative)?;
        if self.ignore.matched(&path, path.is_dir()).is_ignore() {
            return Err(format!("'{relative}' está coberto por .claudeignore"));
        }
        Ok(path)
    }
}

/// Tool de leitura de arquivo (`fs_read`).
pub struct FsReadTool {
    ctx: FsContext,
}

impl FsReadTool {
    /// Cria a tool com `root` como raiz do workspace.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            ctx: FsContext::new(root),
        }
    }
}

impl Tool for FsReadTool {
    fn name(&self) -> &str {
        "fs_read"
    }

    fn description(&self) -> &str {
        "Lê o conteúdo de um arquivo de texto dentro do workspace."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Caminho relativo à raiz do workspace." }
            },
            "required": ["path"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(path_arg) = arguments.get("path").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'path' ausente ou inválido");
            };
            let path = match self.ctx.resolve(path_arg) {
                Ok(p) => p,
                Err(e) => return ToolOutput::error(e),
            };
            match fs::read_to_string(&path) {
                Ok(content) => ToolOutput::ok(content),
                Err(e) => ToolOutput::error(format!("falha ao ler '{path_arg}': {e}")),
            }
        })
    }
}

/// Tool de escrita de arquivo (`fs_write`): cria ou sobrescreve por inteiro.
pub struct FsWriteTool {
    ctx: FsContext,
}

impl FsWriteTool {
    /// Cria a tool com `root` como raiz do workspace.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            ctx: FsContext::new(root),
        }
    }
}

impl Tool for FsWriteTool {
    fn name(&self) -> &str {
        "fs_write"
    }

    fn description(&self) -> &str {
        "Cria ou sobrescreve por inteiro um arquivo de texto dentro do workspace."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Caminho relativo à raiz do workspace." },
                "content": { "type": "string", "description": "Conteúdo completo do arquivo." }
            },
            "required": ["path", "content"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(path_arg) = arguments.get("path").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'path' ausente ou inválido");
            };
            let Some(content) = arguments.get("content").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'content' ausente ou inválido");
            };
            let path = match self.ctx.resolve(path_arg) {
                Ok(p) => p,
                Err(e) => return ToolOutput::error(e),
            };
            if let Some(parent) = path.parent() {
                if let Err(e) = fs::create_dir_all(parent) {
                    return ToolOutput::error(format!(
                        "falha ao criar diretório para '{path_arg}': {e}"
                    ));
                }
            }
            match fs::write(&path, content) {
                Ok(()) => ToolOutput::ok(format!("'{path_arg}' escrito ({} bytes)", content.len())),
                Err(e) => ToolOutput::error(format!("falha ao escrever '{path_arg}': {e}")),
            }
        })
    }
}

/// Tool de edição de arquivo (`fs_edit`): substitui uma ocorrência **única**
/// de `old_string` por `new_string` — mesma disciplina de unicidade de
/// ferramentas de edição já conhecidas (evita ambiguidade sobre qual trecho
/// o modelo queria alterar).
pub struct FsEditTool {
    ctx: FsContext,
}

impl FsEditTool {
    /// Cria a tool com `root` como raiz do workspace.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            ctx: FsContext::new(root),
        }
    }
}

impl Tool for FsEditTool {
    fn name(&self) -> &str {
        "fs_edit"
    }

    fn description(&self) -> &str {
        "Substitui uma ocorrência única de old_string por new_string num arquivo existente do workspace."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Caminho relativo à raiz do workspace." },
                "old_string": { "type": "string", "description": "Trecho a substituir; deve ser único no arquivo." },
                "new_string": { "type": "string", "description": "Trecho de substituição." }
            },
            "required": ["path", "old_string", "new_string"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(path_arg) = arguments.get("path").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'path' ausente ou inválido");
            };
            let Some(old_string) = arguments.get("old_string").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'old_string' ausente ou inválido");
            };
            let Some(new_string) = arguments.get("new_string").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'new_string' ausente ou inválido");
            };
            let path = match self.ctx.resolve(path_arg) {
                Ok(p) => p,
                Err(e) => return ToolOutput::error(e),
            };
            let content = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => return ToolOutput::error(format!("falha ao ler '{path_arg}': {e}")),
            };
            let ocorrencias = content.matches(old_string).count();
            if ocorrencias == 0 {
                return ToolOutput::error(format!("'old_string' não encontrado em '{path_arg}'"));
            }
            if ocorrencias > 1 {
                return ToolOutput::error(format!(
                    "'old_string' aparece {ocorrencias} vezes em '{path_arg}'; deve ser único"
                ));
            }
            let atualizado = content.replacen(old_string, new_string, 1);
            match fs::write(&path, atualizado) {
                Ok(()) => ToolOutput::ok(format!("'{path_arg}' editado")),
                Err(e) => ToolOutput::error(format!("falha ao escrever '{path_arg}': {e}")),
            }
        })
    }
}

/// Tool de busca (`fs_search`): substring literal (sem regex — mesma
/// disciplina de dependências mínimas do MT-06) em arquivos de texto do
/// workspace, respeitando **apenas** `.claudeignore` (os filtros padrão de
/// `.gitignore`/`.git/info/exclude` ficam desligados — escopo do MT-12 é
/// só `.claudeignore`).
pub struct FsSearchTool {
    ctx: FsContext,
}

impl FsSearchTool {
    /// Cria a tool com `root` como raiz do workspace.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            ctx: FsContext::new(root),
        }
    }
}

impl Tool for FsSearchTool {
    fn name(&self) -> &str {
        "fs_search"
    }

    fn description(&self) -> &str {
        "Busca uma substring literal em arquivos de texto do workspace, respeitando .claudeignore."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Substring literal a procurar." },
                "path": { "type": "string", "description": "Subdiretório onde buscar (default: raiz do workspace)." }
            },
            "required": ["pattern"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(pattern) = arguments.get("pattern").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'pattern' ausente ou inválido");
            };
            let subdir = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let start = match self.ctx.resolve(subdir) {
                Ok(p) => p,
                Err(e) => return ToolOutput::error(e),
            };

            let mut resultados = Vec::new();
            let walker = WalkBuilder::new(&start)
                .standard_filters(false)
                .add_custom_ignore_filename(IGNORE_FILE_NAME)
                .build();
            for entrada in walker {
                let Ok(entrada) = entrada else { continue };
                if entrada.file_type().is_some_and(|ft| !ft.is_file()) {
                    continue;
                }
                let caminho = entrada.path();
                let Ok(conteudo) = fs::read_to_string(caminho) else {
                    continue;
                };
                let relativo = caminho.strip_prefix(&self.ctx.root).unwrap_or(caminho);
                for (numero, linha) in conteudo.lines().enumerate() {
                    if linha.contains(pattern) {
                        resultados.push(format!(
                            "{}:{}: {}",
                            relativo.display(),
                            numero + 1,
                            linha.trim()
                        ));
                    }
                }
            }

            if resultados.is_empty() {
                ToolOutput::ok("nenhuma ocorrência encontrada")
            } else {
                ToolOutput::ok(resultados.join("\n"))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;
    use crate::tools::permission::PermissionGate;
    use crate::tools::ToolRegistry;
    use std::sync::Arc;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo — sem depender de uma crate de teste nova (mesma disciplina do
    /// mock HTTP do MT-07).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-fs-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio do sistema não deve estar antes de 1970")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unico);
            fs::create_dir_all(&path).expect("deve criar diretório temporário de teste");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn call(name: &str, arguments: serde_json::Value) -> crate::model::ToolCall {
        crate::model::ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments,
        }
    }

    #[tokio::test]
    async fn fs_read_le_arquivo_existente() {
        let dir = TempDir::new();
        fs::write(dir.path().join("a.txt"), "conteúdo original").unwrap();

        let tool = FsReadTool::new(dir.path());
        let saida = tool.execute(serde_json::json!({ "path": "a.txt" })).await;

        assert!(!saida.is_error);
        assert_eq!(saida.content, "conteúdo original");
    }

    #[tokio::test]
    async fn fs_read_arquivo_inexistente_e_erro() {
        let dir = TempDir::new();
        let tool = FsReadTool::new(dir.path());

        let saida = tool
            .execute(serde_json::json!({ "path": "nao-existe.txt" }))
            .await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn fs_read_rejeita_path_traversal() {
        let dir = TempDir::new();
        let tool = FsReadTool::new(dir.path());

        for tentativa in ["../fora.txt", "/etc/passwd"] {
            let saida = tool.execute(serde_json::json!({ "path": tentativa })).await;
            assert!(saida.is_error, "{tentativa} deveria ser rejeitado");
        }
    }

    #[tokio::test]
    async fn fs_write_cria_arquivo_novo() {
        let dir = TempDir::new();
        let tool = FsWriteTool::new(dir.path());

        let saida = tool
            .execute(serde_json::json!({ "path": "novo.txt", "content": "olá" }))
            .await;

        assert!(!saida.is_error);
        assert_eq!(
            fs::read_to_string(dir.path().join("novo.txt")).unwrap(),
            "olá"
        );
    }

    #[tokio::test]
    async fn fs_write_sobrescreve_arquivo_existente() {
        let dir = TempDir::new();
        fs::write(dir.path().join("a.txt"), "antigo").unwrap();
        let tool = FsWriteTool::new(dir.path());

        tool.execute(serde_json::json!({ "path": "a.txt", "content": "novo" }))
            .await;

        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "novo"
        );
    }

    #[tokio::test]
    async fn fs_edit_substitui_ocorrencia_unica() {
        let dir = TempDir::new();
        fs::write(dir.path().join("a.txt"), "fn foo() {}\nfn bar() {}\n").unwrap();
        let tool = FsEditTool::new(dir.path());

        let saida = tool
            .execute(serde_json::json!({
                "path": "a.txt",
                "old_string": "fn foo() {}",
                "new_string": "fn foo_renomeada() {}"
            }))
            .await;

        assert!(!saida.is_error);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "fn foo_renomeada() {}\nfn bar() {}\n"
        );
    }

    #[tokio::test]
    async fn fs_edit_erro_se_old_string_nao_encontrado() {
        let dir = TempDir::new();
        fs::write(dir.path().join("a.txt"), "conteúdo").unwrap();
        let tool = FsEditTool::new(dir.path());

        let saida = tool
            .execute(serde_json::json!({
                "path": "a.txt",
                "old_string": "não existe",
                "new_string": "x"
            }))
            .await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn fs_edit_erro_se_old_string_ambiguo() {
        let dir = TempDir::new();
        fs::write(dir.path().join("a.txt"), "x x x").unwrap();
        let tool = FsEditTool::new(dir.path());

        let saida = tool
            .execute(serde_json::json!({ "path": "a.txt", "old_string": "x", "new_string": "y" }))
            .await;

        assert!(saida.is_error);
        // Arquivo não deve ter sido alterado quando a substituição é ambígua.
        assert_eq!(
            fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "x x x"
        );
    }

    #[tokio::test]
    async fn fs_search_encontra_ocorrencias_em_arquivos() {
        let dir = TempDir::new();
        fs::write(dir.path().join("a.txt"), "linha 1\nalvo aqui\nlinha 3\n").unwrap();
        fs::write(dir.path().join("b.txt"), "nada relevante\n").unwrap();
        let tool = FsSearchTool::new(dir.path());

        let saida = tool.execute(serde_json::json!({ "pattern": "alvo" })).await;

        assert!(!saida.is_error);
        assert!(saida.content.contains("a.txt:2: alvo aqui"));
        assert!(!saida.content.contains("b.txt"));
    }

    #[tokio::test]
    async fn fs_search_sem_ocorrencias_nao_e_erro() {
        let dir = TempDir::new();
        fs::write(dir.path().join("a.txt"), "nada aqui\n").unwrap();
        let tool = FsSearchTool::new(dir.path());

        let saida = tool
            .execute(serde_json::json!({ "pattern": "inexistente" }))
            .await;

        assert!(!saida.is_error);
    }

    #[tokio::test]
    async fn respeita_claudeignore_no_read_e_no_search() {
        let dir = TempDir::new();
        fs::write(dir.path().join(".claudeignore"), "segredo.txt\n").unwrap();
        fs::write(dir.path().join("segredo.txt"), "alvo confidencial").unwrap();
        fs::write(dir.path().join("normal.txt"), "alvo normal").unwrap();

        let read_tool = FsReadTool::new(dir.path());
        let saida_leitura = read_tool
            .execute(serde_json::json!({ "path": "segredo.txt" }))
            .await;
        assert!(
            saida_leitura.is_error,
            "arquivo ignorado não deveria ser lido"
        );

        let search_tool = FsSearchTool::new(dir.path());
        let saida_busca = search_tool
            .execute(serde_json::json!({ "pattern": "alvo" }))
            .await;
        assert!(saida_busca.content.contains("normal.txt"));
        assert!(
            !saida_busca.content.contains("segredo.txt"),
            "arquivo ignorado não deveria aparecer na busca"
        );
    }

    #[tokio::test]
    async fn respeita_gate_de_permissao_do_mt11() {
        let dir = TempDir::new();
        let gate = PermissionGate::new(Permissions {
            deny: vec!["fs_write".into()],
            ask: vec![],
        });
        let mut registry = ToolRegistry::new(gate);
        registry.register(Arc::new(FsWriteTool::new(dir.path())));

        let outcome = registry
            .execute(&call(
                "fs_write",
                serde_json::json!({ "path": "novo.txt", "content": "não deveria escrever" }),
            ))
            .await;

        assert!(matches!(outcome, crate::tools::ExecutionOutcome::Denied(_)));
        assert!(
            !dir.path().join("novo.txt").exists(),
            "deny deve impedir a escrita de fato, não só sinalizar"
        );
    }
}
