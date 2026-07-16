// Caminho relativo: crates/core/src/tools/checkpoint.rs
//! Decorador de checkpoint (MT-87, ADR-0030): envolve `fs_write`/`fs_edit`
//! (`crates/core/src/tools/fs.rs`), gravando um [`CheckpointStore`] antes de
//! delegar a chamada de verdade — só quando o resultado delegado **não** é
//! erro (nada mudou de fato numa chamada que falhou, não há o que desfazer).
//!
//! Genérico sobre `Arc<dyn Tool>` — funciona para qualquer tool cujo schema
//! de argumentos tenha uma chave `path` (hoje só `fs_write`/`fs_edit` são
//! envolvidas na fiação de produção, `crates/cli/src/main.rs`, mas o
//! decorador em si não conhece nomes de tool específicos).

use std::path::PathBuf;
use std::sync::Arc;

use crate::checkpoint::CheckpointStore;
use crate::provider::BoxFuture;
use crate::tools::fs::resolve_within_root;
use crate::tools::{Tool, ToolOutput};

/// Decora uma tool que escreve arquivo, gravando um checkpoint (via
/// [`CheckpointStore`]) antes de cada chamada bem-sucedida.
pub struct CheckpointingTool {
    inner: Arc<dyn Tool>,
    store: Arc<CheckpointStore>,
    root: PathBuf,
}

impl CheckpointingTool {
    /// Envolve `inner` — `root` é a raiz do workspace (mesma usada por
    /// `inner` para resolver `path`, ADR-0017/MT-12), `store` grava os
    /// checkpoints.
    #[must_use]
    pub fn new(
        inner: Arc<dyn Tool>,
        root: impl Into<PathBuf>,
        store: Arc<CheckpointStore>,
    ) -> Self {
        Self {
            inner,
            store,
            root: root.into(),
        }
    }
}

impl Tool for CheckpointingTool {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn description(&self) -> &str {
        self.inner.description()
    }

    fn input_schema(&self) -> serde_json::Value {
        self.inner.input_schema()
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let path_arg = arguments
                .get("path")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            // Lê o conteúdo "antes" já aqui, antes de delegar — se o
            // caminho não resolver (mesma validação de segurança da tool
            // real, `resolve_within_root`), a chamada delegada também vai
            // falhar com o mesmo erro, e o checkpoint nunca chega a ser
            // gravado (gate abaixo: só grava se `!resultado.is_error`).
            let conteudo_antes = path_arg.as_deref().and_then(|p| {
                resolve_within_root(&self.root, p)
                    .ok()
                    .map(|abs| std::fs::read_to_string(&abs).ok())
            });

            let resultado = self.inner.execute(arguments).await;

            if !resultado.is_error {
                if let (Some(path_arg), Some(conteudo_antes)) = (path_arg, conteudo_antes) {
                    let _ = self.store.record(path_arg, conteudo_antes);
                }
            }

            resultado
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::BoxFuture as ToolBoxFuture;

    /// Diretório temporário de teste (mesma disciplina de
    /// `checkpoint/mod.rs`/`tools/fs.rs`, sem crate de teste nova).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-checkpointing-tool-test-{}-{}",
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

    /// Tool de teste: escreve `arguments["conteudo"]` em `arguments["path"]`
    /// (relativo à raiz dada), ou devolve erro se `arguments["falhar"]` for
    /// `true` — sem escrever nada, simula uma `fs_write`/`fs_edit` real que
    /// falha (ex.: caminho coberto por `.agentryignore`).
    struct ToolDeEscritaFake {
        root: PathBuf,
    }

    impl Tool for ToolDeEscritaFake {
        fn name(&self) -> &str {
            "fake_write"
        }
        fn description(&self) -> &str {
            "tool de teste"
        }
        fn input_schema(&self) -> serde_json::Value {
            serde_json::json!({})
        }
        fn execute(&self, arguments: serde_json::Value) -> ToolBoxFuture<'_, ToolOutput> {
            Box::pin(async move {
                if arguments.get("falhar").and_then(|v| v.as_bool()) == Some(true) {
                    return ToolOutput::error("falha simulada");
                }
                let path = arguments.get("path").and_then(|v| v.as_str()).unwrap();
                let conteudo = arguments.get("conteudo").and_then(|v| v.as_str()).unwrap();
                std::fs::write(self.root.join(path), conteudo).unwrap();
                ToolOutput::ok("escrito")
            })
        }
    }

    #[tokio::test]
    async fn chamada_bem_sucedida_grava_checkpoint_com_conteudo_anterior() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("a.txt"), "original").unwrap();
        let store = Arc::new(CheckpointStore::new(dir.path()));
        let inner = Arc::new(ToolDeEscritaFake {
            root: dir.path().to_path_buf(),
        });
        let tool = CheckpointingTool::new(inner, dir.path(), Arc::clone(&store));

        let resultado = tool
            .execute(serde_json::json!({ "path": "a.txt", "conteudo": "novo" }))
            .await;

        assert!(!resultado.is_error);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "novo"
        );

        let outcome = store
            .undo()
            .expect("undo deve encontrar o checkpoint gravado");
        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "original",
            "undo deve restaurar o conteúdo de antes da chamada"
        );
        assert_eq!(outcome.path, "a.txt");
    }

    #[tokio::test]
    async fn chamada_com_erro_nao_grava_nenhum_checkpoint() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("a.txt"), "original").unwrap();
        let store = Arc::new(CheckpointStore::new(dir.path()));
        let inner = Arc::new(ToolDeEscritaFake {
            root: dir.path().to_path_buf(),
        });
        let tool = CheckpointingTool::new(inner, dir.path(), Arc::clone(&store));

        let resultado = tool
            .execute(serde_json::json!({ "path": "a.txt", "falhar": true }))
            .await;

        assert!(resultado.is_error);
        let erro = store
            .undo()
            .expect_err("chamada com erro não deve ter gravado checkpoint");
        assert!(matches!(erro, crate::checkpoint::CheckpointError::Vazio));
    }

    #[tokio::test]
    async fn checkpoint_de_arquivo_novo_marca_ausencia_e_undo_remove() {
        let dir = TempDir::new();
        // "novo.txt" não existe antes da chamada.
        let store = Arc::new(CheckpointStore::new(dir.path()));
        let inner = Arc::new(ToolDeEscritaFake {
            root: dir.path().to_path_buf(),
        });
        let tool = CheckpointingTool::new(inner, dir.path(), Arc::clone(&store));

        let resultado = tool
            .execute(serde_json::json!({ "path": "novo.txt", "conteudo": "criado agora" }))
            .await;

        assert!(!resultado.is_error);
        assert!(dir.path().join("novo.txt").exists());

        store.undo().expect("undo deve funcionar");
        assert!(
            !dir.path().join("novo.txt").exists(),
            "undo de um arquivo criado deve removê-lo"
        );
    }

    #[test]
    fn name_description_input_schema_delegam_para_a_tool_interna() {
        let dir = TempDir::new();
        let store = Arc::new(CheckpointStore::new(dir.path()));
        let inner = Arc::new(ToolDeEscritaFake {
            root: dir.path().to_path_buf(),
        });
        let tool = CheckpointingTool::new(inner, dir.path(), store);

        assert_eq!(tool.name(), "fake_write");
        assert_eq!(tool.description(), "tool de teste");
        assert_eq!(tool.input_schema(), serde_json::json!({}));
    }
}
