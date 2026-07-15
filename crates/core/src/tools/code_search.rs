// Caminho relativo: crates/core/src/tools/code_search.rs
//! Tool `code_search` (MT-30, ADR-0011): expõe a busca híbrida (MT-28) —
//! índice lexical (MT-26) + semântico (MT-27), reordenados por *reranking*
//! — como [`Tool`] (MT-11), fechando a trilha do RAG semântico local
//! (MT-25..30).
//!
//! Roda sob o mesmo `ToolRegistry`/gate de permissão de qualquer outra
//! tool — nenhuma lógica de permissão própria (mesma disciplina do
//! MT-12/21/24). Lê arquivos-fonte sob uma raiz fixa, respeitando
//! `.agentryignore` (mesma técnica de `crate::tools::repo_map`, MT-21;
//! `.claudeignore` continua funcionando como *fallback* de compatibilidade,
//! ADR-0020/MT-52) — a duplicação do laço de `WalkBuilder` é deliberada: as
//! duas tools produzem formatos de saída diferentes o bastante —
//! `SourceFile` emprestado ali, `ArquivoFonte` possuído aqui — que
//! compartilhar uma função só complicaria as assinaturas sem remover risco
//! real).
//!
//! [`CodeSearchSession`] mantém os índices lexical/semântico em cache
//! (`tokio::sync::Mutex`, para poder segurar o *lock* através de um
//! `.await`) entre chamadas — reconstruídos só quando a indexação
//! incremental (MT-29) reporta que **algum** arquivo mudou desde a
//! chamada anterior; sem mudança nenhuma, a chamada reaproveita os
//! índices já prontos e nem chama `LlmProvider::embeddings` de novo para
//! o corpo do índice (só para a consulta em si, sempre nova). **Limitação
//! conhecida:** quando algo muda, o índice semântico reembeda **todos**
//! os chunks atuais, não só os do(s) arquivo(s) alterado(s) — o MT-29
//! só evita reprocessar o *chunking* AST-aware de arquivos inalterados;
//! `SemanticIndex::build` (MT-27) não tem uma API para adicionar vetores
//! incrementalmente a um índice existente. Otimizar isso fica para quando
//! houver demanda real, não é o critério de aceite deste ticket.
//!
//! Ativada por padrão (ADR-0011, flag `context.semantic_rag.enabled`) —
//! [`register_code_search_tool`] decide, a partir da flag, se a tool é
//! registrada; fiação real com o `settings-schema` fica fora de escopo
//! (mesmo padrão do MT-21/24).

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ignore::WalkBuilder;
use tokio::sync::Mutex;

use crate::context::ast::Language;
use crate::context::rag::chunk::Chunk;
use crate::context::rag::hybrid_search::{self, HybridSearchError};
use crate::context::rag::incremental::{ArquivoFonte, IncrementalError, IncrementalIndexer};
use crate::context::rag::lexical_index::{LexicalIndex, LexicalIndexError};
use crate::context::rag::semantic_index::{SemanticIndex, SemanticIndexError};
use crate::provider::{BoxFuture, EmbeddingsRequest, LlmProvider, ProviderError};
use crate::state_dir;
use crate::tools::{resolve_ignore_file_name, Tool, ToolOutput, ToolRegistry};

const LIMITE_PADRAO: usize = 5;

fn linguagem_por_extensao(caminho: &Path) -> Option<Language> {
    match caminho.extension().and_then(|ext| ext.to_str()) {
        Some("rs") => Some(Language::Rust),
        Some("py") => Some(Language::Python),
        _ => None,
    }
}

fn ler_arquivos(root: &Path, respect_gitignore: bool) -> Vec<ArquivoFonte> {
    let mut arquivos = Vec::new();
    let walker = WalkBuilder::new(root)
        .standard_filters(false)
        .add_custom_ignore_filename(resolve_ignore_file_name(root))
        .git_ignore(respect_gitignore)
        // Ver comentário equivalente em repo_map.rs: `.gitignore` deve
        // valer mesmo fora de um repo git de verdade.
        .require_git(false)
        .build();
    for entrada in walker {
        let Ok(entrada) = entrada else { continue };
        if entrada.file_type().is_some_and(|ft| !ft.is_file()) {
            continue;
        }
        let caminho = entrada.path();
        let Some(language) = linguagem_por_extensao(caminho) else {
            continue;
        };
        let Ok(fonte) = fs::read_to_string(caminho) else {
            continue;
        };
        let relativo = caminho
            .strip_prefix(root)
            .unwrap_or(caminho)
            .to_string_lossy()
            .into_owned();
        arquivos.push(ArquivoFonte {
            caminho: relativo,
            fonte,
            language,
        });
    }
    arquivos
}

fn formatar_resultados(chunks: &[Chunk]) -> String {
    chunks
        .iter()
        .enumerate()
        .map(|(indice, chunk)| {
            format!(
                "{}. {} :: {} ({:?})\n{}",
                indice + 1,
                chunk.file,
                chunk.symbol,
                chunk.kind,
                chunk.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n")
}

/// Erros da tool `code_search` — cada variante indica falha de uma etapa
/// do pipeline (indexação incremental, um dos dois índices, o provider de
/// embeddings da consulta, ou a busca híbrida/*reranking*), nunca um
/// problema na consulta dada pelo chamador em uso normal.
#[derive(Debug)]
pub enum CodeSearchError {
    Incremental(IncrementalError),
    Lexical(LexicalIndexError),
    Semantic(SemanticIndexError),
    Provider(ProviderError),
    Hybrid(HybridSearchError),
}

impl std::fmt::Display for CodeSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Incremental(e) => write!(f, "falha na indexação incremental: {e}"),
            Self::Lexical(e) => write!(f, "falha no índice lexical: {e}"),
            Self::Semantic(e) => write!(f, "falha no índice semântico: {e}"),
            Self::Provider(e) => write!(f, "falha no provider (embeddings da consulta): {e}"),
            Self::Hybrid(e) => write!(f, "falha na busca híbrida: {e}"),
        }
    }
}

impl std::error::Error for CodeSearchError {}

/// Sessão compartilhada da tool `code_search`: mantém os índices lexical/
/// semântico em cache entre chamadas, reconstruídos só quando a indexação
/// incremental (MT-29) reporta mudança em algum arquivo.
pub struct CodeSearchSession {
    root: PathBuf,
    provider: Arc<dyn LlmProvider>,
    embedding_model: String,
    reranker_model: String,
    indexer: IncrementalIndexer,
    indices: Mutex<Option<(LexicalIndex, SemanticIndex)>>,
    /// `context.gitignore.enabled` resolvido (ADR-0020 §3, MT-53).
    respect_gitignore: bool,
}

impl CodeSearchSession {
    /// `root` é a raiz do workspace a indexar/buscar; `provider` serve
    /// tanto os embeddings (chunks e consulta) quanto o *reranking*
    /// (`LlmProvider::chat`) — mesma trait, sem API nova (ADR-0011).
    #[must_use]
    pub fn new(
        root: impl Into<PathBuf>,
        provider: Arc<dyn LlmProvider>,
        embedding_model: impl Into<String>,
        reranker_model: impl Into<String>,
        respect_gitignore: bool,
    ) -> Self {
        let root = root.into();
        let estado = state_dir::ensure_state_dir(&root).unwrap_or_else(|_| root.join(".agentry"));
        Self {
            indexer: IncrementalIndexer::new(&estado),
            root,
            provider,
            embedding_model: embedding_model.into(),
            reranker_model: reranker_model.into(),
            indices: Mutex::new(None),
            respect_gitignore,
        }
    }

    async fn buscar(&self, query: &str, limite: usize) -> Result<Vec<Chunk>, CodeSearchError> {
        let arquivos = ler_arquivos(&self.root, self.respect_gitignore);
        let resultado_reindex = self
            .indexer
            .reindex(&arquivos)
            .map_err(CodeSearchError::Incremental)?;

        let mut guard = self.indices.lock().await;
        let precisa_reconstruir =
            guard.is_none() || !resultado_reindex.arquivos_reprocessados.is_empty();

        if precisa_reconstruir {
            let lexical = LexicalIndex::build(resultado_reindex.chunks.clone())
                .map_err(CodeSearchError::Lexical)?;
            let semantic = SemanticIndex::build(
                resultado_reindex.chunks,
                self.provider.as_ref(),
                &self.embedding_model,
            )
            .await
            .map_err(CodeSearchError::Semantic)?;
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
            .map_err(CodeSearchError::Provider)?;
        let vetor_consulta = resposta_embeddings
            .vectors
            .first()
            .cloned()
            .unwrap_or_default();

        hybrid_search::hybrid_search(
            lexical,
            semantic,
            query,
            &vetor_consulta,
            limite,
            self.provider.as_ref(),
            &self.reranker_model,
        )
        .await
        .map_err(CodeSearchError::Hybrid)
    }
}

/// Tool `code_search`: busca híbrida (lexical + semântica + *reranking*)
/// sobre o código do workspace.
pub struct CodeSearchTool {
    session: Arc<CodeSearchSession>,
}

impl CodeSearchTool {
    #[must_use]
    pub fn new(session: Arc<CodeSearchSession>) -> Self {
        Self { session }
    }
}

impl Tool for CodeSearchTool {
    fn name(&self) -> &str {
        "code_search"
    }

    fn description(&self) -> &str {
        "Busca híbrida (lexical + semântica, com reranking) no código do workspace; devolve os \
         trechos mais relevantes para a consulta dada."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Consulta em linguagem natural ou termos/identificadores de código."
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

/// Registra `code_search` em `registry`, respeitando a flag
/// `context.semantic_rag.enabled` (ADR-0011, *default* `true`) — desligada,
/// a tool não é registrada (e, como consequência direta, nunca é chamada
/// — nenhuma indexação roda). Fiação real com o `settings-schema` fica
/// fora de escopo deste ticket, mesmo padrão do MT-21/24.
pub fn register_code_search_tool(
    registry: &mut ToolRegistry,
    enabled: bool,
    session: Arc<CodeSearchSession>,
) {
    if enabled {
        registry.register(Arc::new(CodeSearchTool::new(session)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;
    use crate::model::{Message, ToolCall, Usage};
    use crate::provider::mock::MockProvider;
    use crate::provider::{ChatResponse, EmbeddingsResponse};
    use crate::tools::permission::PermissionGate;
    use crate::tools::ExecutionOutcome;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo — mesma técnica do MT-12/21/38.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-code-search-test-{}-{}",
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

    fn sessao(dir: &TempDir, provider: Arc<MockProvider>) -> Arc<CodeSearchSession> {
        Arc::new(CodeSearchSession::new(
            dir.path(),
            provider,
            "embed-x",
            "rerank-x",
            false,
        ))
    }

    fn enfileirar_embeddings(mock: &MockProvider, vetores: Vec<Vec<f32>>) {
        mock.enqueue_embeddings(Ok(EmbeddingsResponse {
            vectors: vetores,
            usage: Usage::default(),
        }));
    }

    fn enfileirar_rerank(mock: &MockProvider, ordem: &str) {
        mock.enqueue_chat(Ok(ChatResponse {
            message: Message::assistant(ordem),
            usage: Usage::default(),
        }));
    }

    #[tokio::test]
    async fn respeita_gate_de_permissao_do_mt11() {
        let dir = TempDir::new();
        let gate = PermissionGate::new(Permissions {
            deny: vec!["code_search".into()],
            ask: vec![],
        });
        let mut registry = ToolRegistry::new(gate);
        registry.register(Arc::new(CodeSearchTool::new(sessao(
            &dir,
            Arc::new(MockProvider::new("mock")),
        ))));

        let outcome = registry
            .execute(&call("code_search", serde_json::json!({ "query": "algo" })))
            .await;

        assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
    }

    #[test]
    fn flag_desligada_nao_registra_a_tool() {
        let dir = TempDir::new();
        let gate = PermissionGate::new(Permissions::default());
        let mut registry = ToolRegistry::new(gate);

        register_code_search_tool(
            &mut registry,
            false,
            sessao(&dir, Arc::new(MockProvider::new("mock"))),
        );

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

        register_code_search_tool(
            &mut registry,
            true,
            sessao(&dir, Arc::new(MockProvider::new("mock"))),
        );

        assert!(registry
            .specs()
            .iter()
            .any(|spec| spec.name == "code_search"));
    }

    #[tokio::test]
    async fn busca_devolve_resultados_formatados_e_reordenados() {
        let dir = TempDir::new();
        fs::write(
            dir.path().join("a.rs"),
            "fn soma(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("b.rs"),
            "fn multiplica(a: i32, b: i32) -> i32 {\n    a * b\n}\n",
        )
        .unwrap();

        let mock = Arc::new(MockProvider::new("mock"));
        enfileirar_embeddings(&mock, vec![vec![1.0, 0.0], vec![0.0, 1.0]]); // build do índice
        enfileirar_embeddings(&mock, vec![vec![1.0, 0.0]]); // consulta
        enfileirar_rerank(&mock, "[1, 0]"); // inverte a ordem da fusão

        let tool = CodeSearchTool::new(sessao(&dir, mock));
        let saida = tool
            .execute(serde_json::json!({ "query": "soma", "limit": 2 }))
            .await;

        assert!(!saida.is_error, "saída: {}", saida.content);
        assert!(saida.content.contains("multiplica"));
        assert!(saida.content.contains("soma"));
        let posicao_multiplica = saida.content.find("multiplica").unwrap();
        let posicao_soma = saida.content.find("soma").unwrap();
        assert!(
            posicao_multiplica < posicao_soma,
            "reranking deveria ter invertido a ordem da fusão; saída: {}",
            saida.content
        );
    }

    #[tokio::test]
    async fn segunda_chamada_sem_mudancas_nao_reconstroi_os_indices() {
        let dir = TempDir::new();
        fs::write(
            dir.path().join("a.rs"),
            "fn soma(a: i32, b: i32) -> i32 {\n    a + b\n}\n",
        )
        .unwrap();

        let mock = Arc::new(MockProvider::new("mock"));
        // Primeira chamada: build do índice (1 vetor) + consulta (1 vetor) + rerank.
        enfileirar_embeddings(&mock, vec![vec![1.0, 0.0]]);
        enfileirar_embeddings(&mock, vec![vec![1.0, 0.0]]);
        enfileirar_rerank(&mock, "[0]");
        // Segunda chamada: nada mudou, então só a consulta + rerank — se o
        // código reconstruísse os índices à toa, a fila de embeddings
        // esgotaria aqui e a chamada falharia.
        enfileirar_embeddings(&mock, vec![vec![1.0, 0.0]]);
        enfileirar_rerank(&mock, "[0]");

        let tool = CodeSearchTool::new(sessao(&dir, mock));

        let primeira = tool.execute(serde_json::json!({ "query": "soma" })).await;
        assert!(!primeira.is_error, "saída: {}", primeira.content);

        let segunda = tool.execute(serde_json::json!({ "query": "soma" })).await;
        assert!(
            !segunda.is_error,
            "segunda chamada não deveria precisar de mais embeddings de build; saída: {}",
            segunda.content
        );
    }

    #[tokio::test]
    async fn query_vazia_e_erro_tratado() {
        let dir = TempDir::new();
        let tool = CodeSearchTool::new(sessao(&dir, Arc::new(MockProvider::new("mock"))));

        let saida = tool.execute(serde_json::json!({ "query": "" })).await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn sem_arquivos_suportados_nao_e_erro() {
        let dir = TempDir::new();
        fs::write(dir.path().join("nota.md"), "só markdown por aqui\n").unwrap();

        let mock = Arc::new(MockProvider::new("mock"));
        enfileirar_embeddings(&mock, vec![]); // build do índice (vazio)
        enfileirar_embeddings(&mock, vec![vec![1.0, 0.0]]); // consulta

        let tool = CodeSearchTool::new(sessao(&dir, mock));
        let saida = tool.execute(serde_json::json!({ "query": "algo" })).await;

        assert!(!saida.is_error, "saída: {}", saida.content);
        assert!(saida.content.contains("nenhum resultado"));
    }
}
