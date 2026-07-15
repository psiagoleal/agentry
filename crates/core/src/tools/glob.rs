// Caminho relativo: crates/core/src/tools/glob.rs
//! Tool `glob` (MT-67, ADR-0026): busca arquivos por **padrão de nome/
//! caminho** (`"**/*.rs"`), sem olhar conteúdo — distinta de `fs_search`
//! (MT-12, busca *conteúdo*) e `repo_map` (MT-21, *ranking* de relevância).
//!
//! Reaproveita `ignore::overrides::OverrideBuilder` + `ignore::WalkBuilder`
//! (mesma *crate* já usada por `tools::fs`/`repo_map`/`code_search` —
//! **nenhuma dependência nova**; `OverrideBuilder` é o mesmo mecanismo que o
//! `ripgrep` usa para a própria *flag* `--glob`). Roda sob o mesmo
//! `ToolRegistry`/gate de permissão de qualquer outra tool — sem
//! *default-deny* especial (leitura de metadados de caminho, sem conteúdo,
//! mesma categoria de `fs_read`).

use std::path::PathBuf;

use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;

use crate::provider::BoxFuture;
use crate::tools::{resolve_ignore_file_name, Tool, ToolOutput};

/// Teto de resultados devolvidos — evita uma lista gigante num repositório
/// grande (mesmo espírito de `MAX_RESULTADOS`, `repo_map.rs`, MT-21).
const MAX_RESULTADOS: usize = 200;

/// Tool `glob`: busca por padrão de nome/caminho de arquivo.
pub struct GlobTool {
    root: PathBuf,
    /// `context.gitignore.enabled` resolvido (ADR-0020 §3) — mesmo campo já
    /// usado por `FsSearchTool`/`RepoMapTool`/`CodeSearchSession`.
    respect_gitignore: bool,
}

impl GlobTool {
    /// Cria a tool com `root` como raiz do workspace.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, respect_gitignore: bool) -> Self {
        Self {
            root: root.into(),
            respect_gitignore,
        }
    }
}

impl Tool for GlobTool {
    fn name(&self) -> &str {
        "glob"
    }

    fn description(&self) -> &str {
        "Busca arquivos cujo caminho casa com um padrão glob (ex.: '**/*.rs'), sem olhar \
         conteúdo — devolve caminhos relativos à raiz do workspace. Para buscar por conteúdo, \
         use fs_search."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Padrão glob (ex.: '**/*.rs', 'src/**/*.test.ts')."
                }
            },
            "required": ["pattern"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(pattern) = arguments.get("pattern").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'pattern' obrigatório e deve ser string");
            };

            let mut builder = OverrideBuilder::new(&self.root);
            if let Err(erro) = builder.add(pattern) {
                return ToolOutput::error(format!("padrão glob inválido '{pattern}': {erro}"));
            }
            let overrides = match builder.build() {
                Ok(overrides) => overrides,
                Err(erro) => {
                    return ToolOutput::error(format!("erro ao montar o padrão glob: {erro}"))
                }
            };

            let walker = WalkBuilder::new(&self.root)
                .standard_filters(false)
                .add_custom_ignore_filename(resolve_ignore_file_name(&self.root))
                .git_ignore(self.respect_gitignore)
                // Ver comentário equivalente em fs.rs/repo_map.rs: `.gitignore`
                // deve valer mesmo fora de um repo git de verdade.
                .require_git(false)
                .overrides(overrides)
                .build();

            let mut caminhos = Vec::new();
            for entrada in walker {
                let Ok(entrada) = entrada else { continue };
                if entrada.file_type().is_some_and(|ft| !ft.is_file()) {
                    continue;
                }
                let relativo = entrada
                    .path()
                    .strip_prefix(&self.root)
                    .unwrap_or(entrada.path())
                    .to_string_lossy()
                    .into_owned();
                caminhos.push(relativo);
                if caminhos.len() >= MAX_RESULTADOS {
                    break;
                }
            }

            if caminhos.is_empty() {
                return ToolOutput::ok("nenhum arquivo casou com o padrão");
            }
            ToolOutput::ok(caminhos.join("\n"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `tools::fs`/`repo_map`, MT-12/21).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-glob-test-{}-{}",
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

    fn escreve(dir: &Path, relativo: &str, conteudo: &str) {
        let caminho = dir.join(relativo);
        if let Some(pai) = caminho.parent() {
            std::fs::create_dir_all(pai).unwrap();
        }
        std::fs::write(caminho, conteudo).unwrap();
    }

    #[tokio::test]
    async fn padrao_casa_exatamente_os_arquivos_esperados() {
        let dir = TempDir::new();
        escreve(dir.path(), "src/main.rs", "");
        escreve(dir.path(), "src/lib.rs", "");
        escreve(dir.path(), "src/util.py", "");
        escreve(dir.path(), "README.md", "");
        let tool = GlobTool::new(dir.path(), false);

        let saida = tool
            .execute(serde_json::json!({ "pattern": "**/*.rs" }))
            .await;

        assert!(!saida.is_error);
        let mut linhas: Vec<&str> = saida.content.lines().collect();
        linhas.sort_unstable();
        assert_eq!(linhas, vec!["src/lib.rs", "src/main.rs"]);
    }

    #[tokio::test]
    async fn arquivo_coberto_por_agentryignore_nunca_aparece() {
        let dir = TempDir::new();
        escreve(dir.path(), ".agentryignore", "segredos/\n");
        escreve(dir.path(), "segredos/chave.rs", "");
        escreve(dir.path(), "src/main.rs", "");
        let tool = GlobTool::new(dir.path(), false);

        let saida = tool
            .execute(serde_json::json!({ "pattern": "**/*.rs" }))
            .await;

        assert!(!saida.content.contains("segredos/chave.rs"));
        assert!(saida.content.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn padrao_sem_correspondencia_nao_e_erro() {
        let dir = TempDir::new();
        escreve(dir.path(), "src/main.rs", "");
        let tool = GlobTool::new(dir.path(), false);

        let saida = tool
            .execute(serde_json::json!({ "pattern": "**/*.nonexistent-ext" }))
            .await;

        assert!(!saida.is_error);
        assert_eq!(saida.content, "nenhum arquivo casou com o padrão");
    }

    #[tokio::test]
    async fn resultado_e_capado_ao_teto_configurado() {
        let dir = TempDir::new();
        for i in 0..(MAX_RESULTADOS + 20) {
            escreve(dir.path(), &format!("arquivo-{i}.txt"), "");
        }
        let tool = GlobTool::new(dir.path(), false);

        let saida = tool
            .execute(serde_json::json!({ "pattern": "*.txt" }))
            .await;

        assert_eq!(saida.content.lines().count(), MAX_RESULTADOS);
    }

    #[tokio::test]
    async fn pattern_ausente_e_erro_tratado_sem_panic() {
        let dir = TempDir::new();
        let tool = GlobTool::new(dir.path(), false);

        let saida = tool.execute(serde_json::json!({})).await;

        assert!(saida.is_error);
    }
}
