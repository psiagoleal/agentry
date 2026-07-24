// Caminho relativo: crates/core/src/tools/session_search.rs
//! Tool `session_search` (MT-136, ADR-0039): expõe a busca híbrida sobre
//! sessões salvas (`.agentry/session/*.md`, ADR-0036) — índice lexical
//! (MT-132) + semântico (MT-133), reordenados por *reranking* (MT-134) —
//! como [`Tool`] (MT-11), reaproveitando a mesma técnica do `code_search`
//! (ADR-0011) sobre um corpus diferente (mesmos módulos paralelos,
//! MT-131..135).
//!
//! Roda sob o mesmo `ToolRegistry`/gate de permissão de qualquer outra
//! tool — nenhuma lógica de permissão própria, mesma disciplina do
//! `code_search` (ADR-0039 §3: exposição dupla ao usuário/agente sob o
//! mesmo `PermissionGate`, sem exceção).
//!
//! **Egresso (ADR-0039 §2):** `provider` é sempre o cliente Ollama local —
//! nunca a task-class/provider de chat configurada. Quem instancia
//! [`SessionSearchSession`] (`register_session_search_tool`,
//! `crates/cli/src/main.rs`) é responsável por passar o Ollama fixo, mesmo
//! padrão já em vigor para `CodeSearchSession`.
//!
//! Ativada por padrão (`context.sessionSearch.enabled`, ADR-0039) —
//! [`register_session_search_tool`] decide, a partir da flag, se a tool é
//! registrada.

use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

use crate::context::rag::session_chunk::SessionChunk;
use crate::context::rag::session_hybrid_search::{self, SessionHybridSearchError};
use crate::context::rag::session_incremental::{
    SessaoFonte, SessionIncrementalError, SessionIncrementalIndexer,
};
use crate::context::rag::session_lexical_index::{SessionLexicalIndex, SessionLexicalIndexError};
use crate::context::rag::session_semantic_index::{
    SessionSemanticIndex, SessionSemanticIndexError,
};
use crate::provider::{BoxFuture, EmbeddingsRequest, LlmProvider, ProviderError};
use crate::session::persist;
use crate::state_dir;
use crate::tools::{Tool, ToolOutput, ToolRegistry};

const LIMITE_PADRAO: usize = 5;

/// Lê cada `.agentry/session/*.md` do workspace e desserializa (MT-120) —
/// um arquivo ilegível/corrompido é **ignorado**, não aborta a busca
/// inteira (mesmo espírito de `sessao::listar_sessoes`, MT-123: uma sessão
/// quebrada não deveria esconder as demais). Sem `.agentry/session/`
/// ainda criado, devolve lista vazia (não erro).
fn ler_sessoes(root: &std::path::Path) -> Vec<SessaoFonte> {
    let Ok(estado) = state_dir::ensure_state_dir(root) else {
        return Vec::new();
    };
    let diretorio = estado.join("session");
    let Ok(entradas) = std::fs::read_dir(&diretorio) else {
        return Vec::new();
    };

    let mut sessoes = Vec::new();
    for entrada in entradas {
        let Ok(entrada) = entrada else { continue };
        let caminho = entrada.path();
        if !caminho.extension().is_some_and(|ext| ext == "md") {
            continue;
        }
        let Ok(texto) = std::fs::read_to_string(&caminho) else {
            continue;
        };
        let Ok((metadados, mensagens)) = persist::desserializar_de_markdown(&texto) else {
            continue;
        };
        sessoes.push(SessaoFonte {
            session_id: metadados.id,
            mensagens,
        });
    }
    sessoes
}

/// Formata os chunks encontrados como texto legível — fonte única entre a
/// tool (`execute`, abaixo) e o comando `/recall` (MT-137, REPL/TUI), sem
/// duas versões divergentes do mesmo texto (mesmo padrão de
/// `sessao::formatar_lista_de_sessoes`).
#[must_use]
pub fn formatar_resultados(chunks: &[SessionChunk]) -> String {
    chunks
        .iter()
        .enumerate()
        .map(|(indice, chunk)| {
            format!(
                "{}. sessão {}, mensagem {} ({:?})\n{}",
                indice + 1,
                chunk.session_id,
                chunk.message_index,
                chunk.role,
                chunk.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n")
}

/// Erros da tool `session_search` — mesma semântica de `CodeSearchError`:
/// cada variante indica falha de uma etapa do pipeline, nunca um problema
/// na consulta dada pelo chamador em uso normal.
#[derive(Debug)]
pub enum SessionSearchError {
    Incremental(SessionIncrementalError),
    Lexical(SessionLexicalIndexError),
    Semantic(SessionSemanticIndexError),
    Provider(ProviderError),
    Hybrid(SessionHybridSearchError),
}

impl std::fmt::Display for SessionSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incremental(e) => write!(f, "falha na indexação incremental de sessão: {e}"),
            Self::Lexical(e) => write!(f, "falha no índice lexical de sessão: {e}"),
            Self::Semantic(e) => write!(f, "falha no índice semântico de sessão: {e}"),
            Self::Provider(e) => write!(f, "falha no provider (embeddings da consulta): {e}"),
            Self::Hybrid(e) => write!(f, "falha na busca híbrida de sessão: {e}"),
        }
    }
}

impl std::error::Error for SessionSearchError {}

/// Sessão compartilhada da tool `session_search` (e do comando `/recall`,
/// MT-137, que reaproveita [`Self::buscar`] em vez de duplicar o
/// *pipeline*): mantém os índices lexical/semântico em cache entre
/// chamadas, reconstruídos só quando a indexação incremental (MT-135)
/// reporta sessão nova.
pub struct SessionSearchSession {
    root: PathBuf,
    provider: Arc<dyn LlmProvider>,
    embedding_model: String,
    reranker_model: String,
    indexer: SessionIncrementalIndexer,
    indices: Mutex<Option<(SessionLexicalIndex, SessionSemanticIndex)>>,
}

impl SessionSearchSession {
    /// `root` é a raiz do workspace (onde `.agentry/session/` é
    /// resolvido); `provider` serve tanto os embeddings quanto o
    /// *reranking* — **sempre** o cliente Ollama local (ADR-0039 §2),
    /// nunca a task-class de chat configurada.
    #[must_use]
    pub fn new(
        root: impl Into<PathBuf>,
        provider: Arc<dyn LlmProvider>,
        embedding_model: impl Into<String>,
        reranker_model: impl Into<String>,
    ) -> Self {
        let root = root.into();
        let estado = state_dir::ensure_state_dir(&root).unwrap_or_else(|_| root.join(".agentry"));
        Self {
            indexer: SessionIncrementalIndexer::new(&estado),
            root,
            provider,
            embedding_model: embedding_model.into(),
            reranker_model: reranker_model.into(),
            indices: Mutex::new(None),
        }
    }

    /// Executa a busca híbrida completa: lê/reindexa as sessões salvas,
    /// reconstrói os índices só se algo mudou, gera o embedding da
    /// consulta e devolve os chunks reordenados. Reaproveitada tanto pela
    /// tool (`execute`, abaixo) quanto pelo comando `/recall` (MT-137).
    ///
    /// # Errors
    ///
    /// Propaga a falha da etapa correspondente do *pipeline* — ver
    /// [`SessionSearchError`].
    pub async fn buscar(
        &self,
        query: &str,
        limite: usize,
    ) -> Result<Vec<SessionChunk>, SessionSearchError> {
        let sessoes = ler_sessoes(&self.root);
        let resultado_reindex = self
            .indexer
            .reindex(&sessoes)
            .map_err(SessionSearchError::Incremental)?;

        let mut guard = self.indices.lock().await;
        let precisa_reconstruir =
            guard.is_none() || !resultado_reindex.sessoes_reprocessadas.is_empty();

        if precisa_reconstruir {
            let lexical = SessionLexicalIndex::build(resultado_reindex.chunks.clone())
                .map_err(SessionSearchError::Lexical)?;
            let semantic = SessionSemanticIndex::build(
                resultado_reindex.chunks,
                self.provider.as_ref(),
                &self.embedding_model,
            )
            .await
            .map_err(SessionSearchError::Semantic)?;
            *guard = Some((lexical, semantic));
        }

        let (lexical, semantic) = guard.as_ref().expect("acabamos de garantir Some acima");

        let resposta_embeddings = self
            .provider
            .embeddings(EmbeddingsRequest {
                model: self.embedding_model.clone(),
                input: vec![query.to_string()],
            })
            .await
            .map_err(SessionSearchError::Provider)?;
        let vetor_consulta = resposta_embeddings
            .vectors
            .first()
            .cloned()
            .unwrap_or_default();

        session_hybrid_search::hybrid_search(
            lexical,
            semantic,
            query,
            &vetor_consulta,
            limite,
            self.provider.as_ref(),
            &self.reranker_model,
        )
        .await
        .map_err(SessionSearchError::Hybrid)
    }
}

/// Tool `session_search`: busca híbrida (lexical + semântica + *reranking*)
/// sobre sessões salvas do workspace.
pub struct SessionSearchTool {
    session: Arc<SessionSearchSession>,
}

impl SessionSearchTool {
    #[must_use]
    pub fn new(session: Arc<SessionSearchSession>) -> Self {
        Self { session }
    }
}

impl Tool for SessionSearchTool {
    fn name(&self) -> &str {
        "session_search"
    }

    fn description(&self) -> &str {
        "Busca híbrida (lexical + semântica, com reranking) em sessões salvas anteriormente \
         (.agentry/session/*.md); devolve os trechos de conversa mais relevantes para a \
         consulta dada."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Consulta em linguagem natural sobre o que foi discutido em conversas anteriores."
                },
                "limit": {
                    "type": "integer",
                    "description": "Número máximo de trechos devolvidos (default 5)."
                }
            },
            "required": ["query"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let query = match arguments.get("query").and_then(|v| v.as_str()) {
                Some(q) if !q.is_empty() => q.to_string(),
                _ => return ToolOutput::error("argumento 'query' ausente ou inválido"),
            };
            let limite = arguments
                .get("limit")
                .and_then(serde_json::Value::as_u64)
                .map(|v| v as usize)
                .unwrap_or(LIMITE_PADRAO);

            match self.session.buscar(&query, limite).await {
                Ok(chunks) if chunks.is_empty() => {
                    ToolOutput::ok("nenhum resultado encontrado para a consulta")
                }
                Ok(chunks) => ToolOutput::ok(formatar_resultados(&chunks)),
                Err(e) => ToolOutput::error(e.to_string()),
            }
        })
    }
}

/// Registra `session_search` em `registry`, respeitando a flag
/// `context.sessionSearch.enabled` (ADR-0039, *default* `true`) —
/// desligada, a tool não é registrada (e, como consequência direta, nunca
/// é chamada — nenhuma indexação de sessão roda). Mesmo padrão do MT-30
/// (`register_code_search_tool`).
pub fn register_session_search_tool(
    registry: &mut ToolRegistry,
    enabled: bool,
    session: Arc<SessionSearchSession>,
) {
    if enabled {
        registry.register(Arc::new(SessionSearchTool::new(session)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;
    use crate::model::{Message, Usage};
    use crate::provider::mock::MockProvider;
    use crate::provider::{ChatResponse, EmbeddingsResponse};
    use crate::tools::permission::PermissionGate;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new() -> Self {
            let caminho = std::env::temp_dir().join(format!(
                "agentry-session-search-teste-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio não deve estar antes de 1970")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&caminho).expect("deve criar o diretório temporário");
            Self(caminho)
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

    fn escrever_sessao_de_teste(root: &std::path::Path, id: &str, mensagens: &[Message]) {
        let metadados = persist::MetadadosDeSessao {
            id: id.to_string(),
            criado_em: "2026-07-24T00:00:00Z".to_string(),
            provider: "ollama".to_string(),
            model: "modelo-x".to_string(),
            task_class: "chat".to_string(),
            usage_input_tokens: 0,
            usage_output_tokens: 0,
        };
        let markdown = persist::serializar_para_markdown(&metadados, mensagens);
        let estado = state_dir::ensure_state_dir(root).expect("deve criar .agentry");
        let diretorio = estado.join("session");
        std::fs::create_dir_all(&diretorio).expect("deve criar .agentry/session");
        std::fs::write(diretorio.join(format!("{id}.md")), markdown)
            .expect("deve escrever a sessão de teste");
    }

    #[test]
    fn tool_registrada_quando_habilitada() {
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let mock: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("mock"));
        let session = Arc::new(SessionSearchSession::new(
            std::env::temp_dir(),
            mock,
            "embed-x",
            "rerank-x",
        ));

        register_session_search_tool(&mut registry, true, session);

        assert!(registry.specs().iter().any(|s| s.name == "session_search"));
    }

    #[test]
    fn tool_ausente_quando_desabilitada() {
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let mock: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("mock"));
        let session = Arc::new(SessionSearchSession::new(
            std::env::temp_dir(),
            mock,
            "embed-x",
            "rerank-x",
        ));

        register_session_search_tool(&mut registry, false, session);

        assert!(!registry.specs().iter().any(|s| s.name == "session_search"));
    }

    #[tokio::test]
    async fn busca_sem_nenhuma_sessao_salva_devolve_lista_vazia() {
        let dir = TempDir::new();
        let mock = MockProvider::new("mock");
        // Sem sessão nenhuma, `SessionSemanticIndex::build` não chama o
        // provider (chunks vazio, mesmo atalho de `semantic_index.rs`) --
        // mas `buscar` ainda gera o embedding da própria consulta
        // incondicionalmente (mesmo comportamento de `code_search.rs`).
        mock.enqueue_embeddings(Ok(EmbeddingsResponse {
            vectors: vec![vec![1.0, 0.0]],
            usage: Usage::default(),
        }));
        let session = SessionSearchSession::new(dir.path(), Arc::new(mock), "embed-x", "rerank-x");

        let resultado = session
            .buscar("qualquer coisa", 5)
            .await
            .expect("busca sem sessões não deve ser erro");

        assert!(resultado.is_empty());
    }

    #[tokio::test]
    async fn busca_real_sobre_sessoes_de_teste_devolve_os_chunks_esperados() {
        let dir = TempDir::new();
        escrever_sessao_de_teste(
            dir.path(),
            "20260724-000000",
            &[
                Message::user("qual a capital da frança"),
                Message::assistant("paris é a capital da frança"),
            ],
        );

        let mock = MockProvider::new("mock");
        mock.enqueue_embeddings(Ok(EmbeddingsResponse {
            vectors: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
            usage: Usage::default(),
        }));
        mock.enqueue_embeddings(Ok(EmbeddingsResponse {
            vectors: vec![vec![1.0, 0.0]],
            usage: Usage::default(),
        }));
        mock.enqueue_chat(Ok(ChatResponse {
            message: Message::assistant("[0, 1]"),
            usage: Usage::default(),
        }));

        let session = SessionSearchSession::new(dir.path(), Arc::new(mock), "embed-x", "rerank-x");
        let resultado = session
            .buscar("frança", 5)
            .await
            .expect("busca deve funcionar");

        assert!(!resultado.is_empty());
        assert_eq!(resultado[0].session_id, "20260724-000000");
    }
}
