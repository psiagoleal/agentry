// Caminho relativo: crates/core/src/tools/repo_map.rs
//! Tool `repo_map` (MT-21, ADR-0010): expõe o repo-map estilo Aider — grafo
//! de referências entre arquivos (MT-19) + ranking estilo PageRank (MT-20)
//! — como [`Tool`] (MT-11). Dada uma lista de arquivos "semente", devolve os
//! arquivos do workspace mais relevantes para continuar a partir deles.
//!
//! Roda sob o mesmo `ToolRegistry`/gate de permissão de qualquer outra tool
//! — nenhuma lógica de permissão própria aqui (mesma disciplina do MT-12).
//! Lê arquivos-fonte sob uma raiz fixa, respeitando `.agentryignore` (mesma
//! técnica de `crate::tools::fs`, via `ignore::WalkBuilder`; `.claudeignore`
//! continua funcionando como *fallback* de compatibilidade, ADR-0020/MT-52);
//! só extensões reconhecidas (`.rs`, `.py` — mesmas linguagens do MT-18)
//! entram no grafo, as demais são ignoradas em silêncio.
//!
//! Ativada por padrão (ADR-0010, flag `context.repo_map.enabled`) —
//! [`register_repo_map_tool`] decide, a partir da flag, se a tool é
//! registrada; fiação real com o `settings-schema` (leitura da flag a
//! partir de `crate::config::Config`) fica fora de escopo deste ticket
//! (UI/CLI de configuração) — aqui só o mecanismo de habilitar/desabilitar.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::WalkBuilder;

use crate::context::ast::Language;
use crate::context::repo_map::graph::{build_reference_graph, SourceFile};
use crate::context::repo_map::rank::rank;
use crate::provider::BoxFuture;
use crate::tools::{resolve_ignore_file_name, Tool, ToolOutput, ToolRegistry};

/// Número máximo de arquivos devolvidos no ranking — evita uma resposta
/// gigante em repositórios grandes.
const MAX_RESULTADOS: usize = 20;

/// Detecta a linguagem suportada pela extensão do arquivo, ou `None` se não
/// for reconhecida (arquivo simplesmente não entra no grafo).
fn linguagem_por_extensao(caminho: &Path) -> Option<Language> {
    match caminho.extension().and_then(|ext| ext.to_str()) {
        Some("rs") => Some(Language::Rust),
        Some("py") => Some(Language::Python),
        _ => None,
    }
}

/// Tool `repo_map`: repo-map estilo Aider (MT-19/20) exposta ao agent loop.
pub struct RepoMapTool {
    root: PathBuf,
}

impl RepoMapTool {
    /// Cria a tool com `root` como raiz do workspace.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Lê o conteúdo de cada arquivo de linguagem suportada sob a raiz,
    /// respeitando o arquivo de ignore ativo (`.agentryignore`, ou
    /// `.claudeignore` como *fallback*). Arquivos ilegíveis (não-UTF-8,
    /// etc.) são pulados em silêncio — o repo-map é best-effort, não deve
    /// falhar a tarefa inteira por causa de um arquivo problemático.
    fn ler_arquivos(&self) -> Vec<(String, String, Language)> {
        let mut arquivos = Vec::new();
        let walker = WalkBuilder::new(&self.root)
            .standard_filters(false)
            .add_custom_ignore_filename(resolve_ignore_file_name(&self.root))
            .build();
        for entrada in walker {
            let Ok(entrada) = entrada else { continue };
            if entrada.file_type().is_some_and(|ft| !ft.is_file()) {
                continue;
            }
            let caminho = entrada.path();
            let Some(linguagem) = linguagem_por_extensao(caminho) else {
                continue;
            };
            let Ok(conteudo) = fs::read_to_string(caminho) else {
                continue;
            };
            let relativo = caminho
                .strip_prefix(&self.root)
                .unwrap_or(caminho)
                .to_string_lossy()
                .into_owned();
            arquivos.push((relativo, conteudo, linguagem));
        }
        arquivos
    }
}

impl Tool for RepoMapTool {
    fn name(&self) -> &str {
        "repo_map"
    }

    fn description(&self) -> &str {
        "Devolve os arquivos do workspace mais relevantes para uma tarefa, a partir de um \
         grafo de referências entre símbolos (repo-map estilo Aider)."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "seeds": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Arquivos-semente (caminhos relativos à raiz do workspace) de onde partir a relevância; vazio usa relevância global do repositório."
                }
            }
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let seeds: Vec<String> = arguments
                .get("seeds")
                .and_then(|v| v.as_array())
                .map(|itens| {
                    itens
                        .iter()
                        .filter_map(|v| v.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let arquivos = self.ler_arquivos();
            if arquivos.is_empty() {
                return ToolOutput::ok("nenhum arquivo suportado encontrado no workspace");
            }

            let source_files: Vec<SourceFile<'_>> = arquivos
                .iter()
                .map(|(caminho, conteudo, linguagem)| SourceFile {
                    path: caminho.as_str(),
                    source: conteudo.as_str(),
                    language: *linguagem,
                })
                .collect();

            let grafo = build_reference_graph(&source_files);
            let nodes: Vec<&str> = source_files.iter().map(|f| f.path).collect();
            let seeds_refs: Vec<&str> = seeds.iter().map(String::as_str).collect();

            let ranking = rank(&grafo, &nodes, &seeds_refs);
            if ranking.is_empty() {
                return ToolOutput::ok("nenhum arquivo relevante encontrado além da semente");
            }

            let texto = ranking
                .iter()
                .take(MAX_RESULTADOS)
                .enumerate()
                .map(|(indice, (caminho, pontuacao))| {
                    format!("{}. {caminho} ({pontuacao:.4})", indice + 1)
                })
                .collect::<Vec<_>>()
                .join("\n");

            ToolOutput::ok(texto)
        })
    }
}

/// Registra a tool `repo_map` em `registry`, respeitando a flag
/// `context.repo_map.enabled` (ADR-0010, *default* `true`) — desligada, a
/// tool simplesmente não é registrada: não aparece em [`ToolRegistry::specs`],
/// não pode ser chamada. Fiação real com o `settings-schema` fica fora de
/// escopo deste ticket — aqui só o mecanismo de habilitar/desabilitar em si.
pub fn register_repo_map_tool(registry: &mut ToolRegistry, enabled: bool, tool: RepoMapTool) {
    if enabled {
        registry.register(Arc::new(tool));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;
    use crate::model::ToolCall;
    use crate::tools::permission::PermissionGate;
    use crate::tools::ExecutionOutcome;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo — mesma técnica do MT-12 (`crate::tools::fs`).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-repo-map-test-{}-{}",
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

    fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments,
        }
    }

    #[tokio::test]
    async fn tool_produz_ranking_a_partir_da_semente() {
        let dir = TempDir::new();
        fs::write(
            dir.path().join("seed.rs"),
            "fn desde_seed() {\n    popular();\n    popular();\n    obscuro();\n}\n",
        )
        .unwrap();
        fs::write(dir.path().join("popular.rs"), "fn popular() {}\n").unwrap();
        fs::write(dir.path().join("obscuro.rs"), "fn obscuro() {}\n").unwrap();
        fs::write(dir.path().join("isolado.rs"), "fn isolado() {}\n").unwrap();

        let tool = RepoMapTool::new(dir.path());
        let saida = tool
            .execute(serde_json::json!({ "seeds": ["seed.rs"] }))
            .await;

        assert!(!saida.is_error);
        let posicao_popular = saida
            .content
            .find("popular.rs")
            .expect("popular.rs deve aparecer");
        let posicao_obscuro = saida
            .content
            .find("obscuro.rs")
            .expect("obscuro.rs deve aparecer");
        assert!(
            posicao_popular < posicao_obscuro,
            "popular.rs (referenciado 2x pela semente) deve vir antes de obscuro.rs (1x); \
             saída: {}",
            saida.content
        );
        assert!(
            !saida.content.contains("seed.rs"),
            "o próprio arquivo semente não deve aparecer no ranking dos 'demais'"
        );
    }

    #[tokio::test]
    async fn tool_ignora_arquivos_cobertos_por_claudeignore_legado() {
        // Só .claudeignore (sem .agentryignore) — fallback de compatibilidade
        // (MT-52, ADR-0020 §2).
        let dir = TempDir::new();
        fs::write(dir.path().join(".claudeignore"), "secreto.rs\n").unwrap();
        fs::write(
            dir.path().join("seed.rs"),
            "fn desde_seed() {\n    popular();\n    de_secreto();\n}\n",
        )
        .unwrap();
        fs::write(dir.path().join("popular.rs"), "fn popular() {}\n").unwrap();
        fs::write(dir.path().join("secreto.rs"), "fn de_secreto() {}\n").unwrap();

        let tool = RepoMapTool::new(dir.path());
        let saida = tool
            .execute(serde_json::json!({ "seeds": ["seed.rs"] }))
            .await;

        assert!(!saida.is_error);
        assert!(
            !saida.content.contains("secreto.rs"),
            "arquivo coberto por .claudeignore não deveria aparecer no ranking"
        );
    }

    #[tokio::test]
    async fn tool_ignora_arquivos_cobertos_por_agentryignore() {
        let dir = TempDir::new();
        fs::write(dir.path().join(".agentryignore"), "secreto.rs\n").unwrap();
        fs::write(
            dir.path().join("seed.rs"),
            "fn desde_seed() {\n    popular();\n    de_secreto();\n}\n",
        )
        .unwrap();
        fs::write(dir.path().join("popular.rs"), "fn popular() {}\n").unwrap();
        fs::write(dir.path().join("secreto.rs"), "fn de_secreto() {}\n").unwrap();

        let tool = RepoMapTool::new(dir.path());
        let saida = tool
            .execute(serde_json::json!({ "seeds": ["seed.rs"] }))
            .await;

        assert!(!saida.is_error);
        assert!(
            !saida.content.contains("secreto.rs"),
            "arquivo coberto por .agentryignore não deveria aparecer no ranking"
        );
    }

    #[tokio::test]
    async fn tool_sem_arquivos_suportados_nao_e_erro() {
        let dir = TempDir::new();
        fs::write(dir.path().join("nota.md"), "só markdown por aqui\n").unwrap();

        let tool = RepoMapTool::new(dir.path());
        let saida = tool.execute(serde_json::json!({})).await;

        assert!(!saida.is_error);
    }

    #[tokio::test]
    async fn respeita_gate_de_permissao_do_mt11() {
        let dir = TempDir::new();
        let gate = PermissionGate::new(Permissions {
            deny: vec!["repo_map".into()],
            ask: vec![],
        });
        let mut registry = ToolRegistry::new(gate);
        registry.register(Arc::new(RepoMapTool::new(dir.path())));

        let outcome = registry
            .execute(&call("repo_map", serde_json::json!({})))
            .await;

        assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
    }

    #[test]
    fn flag_desligada_nao_registra_a_tool() {
        let dir = TempDir::new();
        let gate = PermissionGate::new(Permissions::default());
        let mut registry = ToolRegistry::new(gate);

        register_repo_map_tool(&mut registry, false, RepoMapTool::new(dir.path()));

        assert!(
            registry.specs().is_empty(),
            "flag desligada não deveria registrar nenhuma tool"
        );
    }

    #[test]
    fn flag_ligada_registra_a_tool() {
        let dir = TempDir::new();
        let gate = PermissionGate::new(Permissions::default());
        let mut registry = ToolRegistry::new(gate);

        register_repo_map_tool(&mut registry, true, RepoMapTool::new(dir.path()));

        assert!(registry.specs().iter().any(|spec| spec.name == "repo_map"));
    }
}
