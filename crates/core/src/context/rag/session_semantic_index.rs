// Caminho relativo: crates/core/src/context/rag/session_semantic_index.rs
//! Índice semântico (embeddings) sobre os chunks de sessão (MT-133, ADR-0039).
//!
//! Análogo a [`super::semantic_index`] (código), mesma técnica (`lancedb`
//! embutido sobre `memory://`, vetores via `LlmProvider::embeddings`) sobre
//! um *schema* próprio de [`super::session_chunk::SessionChunk`].
//!
//! **Egresso (ADR-0039 §2):** quem chama [`SessionSemanticIndex::build`]/
//! [`SessionSemanticIndex::search`] deve sempre passar o provider Ollama
//! local — nunca a task-class/provider de chat configurada — mesma
//! disciplina já em vigor para `code_search` (`ollama_provider` fixo em
//! `register_context_tools`, `crates/cli/src/main.rs`). Este módulo não
//! impõe isso em tempo de compilação (mesma limitação de
//! `semantic_index.rs`, que recebe `&dyn LlmProvider` genérico) — a
//! garantia vem de quem instancia o índice (MT-136), não de uma
//! verificação aqui.

use std::sync::Arc;

use arrow_array::types::Float32Type;
use arrow_array::{
    Array, FixedSizeListArray, RecordBatch, RecordBatchIterator, RecordBatchReader, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};

use super::session_chunk::SessionChunk;
use crate::model::Role;
use crate::provider::{EmbeddingsRequest, LlmProvider, ProviderError};

const NOME_TABELA: &str = "session_chunks";

/// Erros do índice semântico de sessão — mesma semântica de
/// [`super::semantic_index::SemanticIndexError`].
#[derive(Debug)]
pub enum SessionSemanticIndexError {
    Provider(ProviderError),
    ContagemDeVetoresInconsistente { esperado: usize, recebido: usize },
    DimensaoInconsistente,
    LanceDb(String),
}

impl std::fmt::Display for SessionSemanticIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Provider(e) => write!(f, "provider de embeddings falhou: {e}"),
            Self::ContagemDeVetoresInconsistente { esperado, recebido } => write!(
                f,
                "provider devolveu {recebido} vetor(es) para {esperado} chunk(s)"
            ),
            Self::DimensaoInconsistente => {
                write!(f, "vetores de embeddings com dimensões diferentes entre si")
            }
            Self::LanceDb(msg) => write!(f, "erro no índice semântico de sessão (lancedb): {msg}"),
        }
    }
}

impl std::error::Error for SessionSemanticIndexError {}

fn role_to_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

fn role_from_str(role: &str) -> Role {
    match role {
        "assistant" => Role::Assistant,
        "system" => Role::System,
        "tool" => Role::Tool,
        _ => Role::User,
    }
}

fn schema_com_dimensao(dimensao: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("session_id", DataType::Utf8, false),
        Field::new("message_index", DataType::UInt64, false),
        Field::new("role", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                dimensao,
            ),
            false,
        ),
    ]))
}

fn construir_record_batch(
    chunks: &[SessionChunk],
    vetores: &[Vec<f32>],
    dimensao: usize,
) -> Result<RecordBatch, SessionSemanticIndexError> {
    let schema = schema_com_dimensao(dimensao as i32);

    let session_id = StringArray::from_iter_values(chunks.iter().map(|c| c.session_id.as_str()));
    let message_index =
        arrow_array::UInt64Array::from_iter_values(chunks.iter().map(|c| c.message_index as u64));
    let role = StringArray::from_iter_values(chunks.iter().map(|c| role_to_str(c.role)));
    let text = StringArray::from_iter_values(chunks.iter().map(|c| c.text.as_str()));
    let vector = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
        vetores
            .iter()
            .map(|v| Some(v.iter().map(|x| Some(*x)).collect::<Vec<_>>())),
        dimensao as i32,
    );

    RecordBatch::try_new(
        schema,
        vec![
            Arc::new(session_id),
            Arc::new(message_index),
            Arc::new(role),
            Arc::new(text),
            Arc::new(vector),
        ],
    )
    .map_err(|e| SessionSemanticIndexError::LanceDb(e.to_string()))
}

fn coluna_texto<'a>(batch: &'a RecordBatch, nome: &str) -> Vec<&'a str> {
    batch
        .column_by_name(nome)
        .and_then(|col| col.as_any().downcast_ref::<StringArray>())
        .map(|arr| (0..arr.len()).map(|i| arr.value(i)).collect())
        .unwrap_or_default()
}

fn coluna_u64(batch: &RecordBatch, nome: &str) -> Vec<u64> {
    batch
        .column_by_name(nome)
        .and_then(|col| col.as_any().downcast_ref::<arrow_array::UInt64Array>())
        .map(|arr| (0..arr.len()).map(|i| arr.value(i)).collect())
        .unwrap_or_default()
}

fn record_batch_para_chunks(batch: &RecordBatch) -> Vec<SessionChunk> {
    let session_id = coluna_texto(batch, "session_id");
    let message_index = coluna_u64(batch, "message_index");
    let role = coluna_texto(batch, "role");
    let text = coluna_texto(batch, "text");

    (0..batch.num_rows())
        .map(|i| SessionChunk {
            session_id: session_id[i].to_string(),
            message_index: message_index[i] as usize,
            role: role_from_str(role[i]),
            text: text[i].to_string(),
        })
        .collect()
}

/// Índice semântico (embeddings) sobre um conjunto de [`SessionChunk`]s.
///
/// Embutido/*in-process* (`lancedb` sobre `memory://`) — construído uma
/// vez via [`SessionSemanticIndex::build`], consultado via
/// [`SessionSemanticIndex::search`].
#[derive(Debug)]
pub struct SessionSemanticIndex {
    tabela: Option<lancedb::Table>,
}

impl SessionSemanticIndex {
    /// Gera embeddings para o texto de cada chunk via `provider` e indexa
    /// os vetores resultantes. `chunks` vazio não é erro — devolve um
    /// índice sem tabela por trás, cujas buscas sempre respondem lista
    /// vazia.
    ///
    /// # Errors
    ///
    /// Mesma semântica de [`super::semantic_index::SemanticIndex::build`]:
    /// [`SessionSemanticIndexError::Provider`] se a chamada de embeddings
    /// falhar; [`SessionSemanticIndexError::ContagemDeVetoresInconsistente`]/
    /// [`SessionSemanticIndexError::DimensaoInconsistente`] se a resposta
    /// não bater com o esperado; [`SessionSemanticIndexError::LanceDb`] se
    /// a escrita na tabela falhar internamente.
    pub async fn build(
        chunks: Vec<SessionChunk>,
        provider: &dyn LlmProvider,
        embedding_model: &str,
    ) -> Result<Self, SessionSemanticIndexError> {
        if chunks.is_empty() {
            return Ok(Self { tabela: None });
        }

        let textos: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
        let resposta = provider
            .embeddings(EmbeddingsRequest {
                model: embedding_model.to_string(),
                input: textos,
            })
            .await
            .map_err(SessionSemanticIndexError::Provider)?;

        if resposta.vectors.len() != chunks.len() {
            return Err(SessionSemanticIndexError::ContagemDeVetoresInconsistente {
                esperado: chunks.len(),
                recebido: resposta.vectors.len(),
            });
        }

        let dimensao = resposta.vectors[0].len();
        if resposta.vectors.iter().any(|v| v.len() != dimensao) {
            return Err(SessionSemanticIndexError::DimensaoInconsistente);
        }

        let batch = construir_record_batch(&chunks, &resposta.vectors, dimensao)?;
        let schema = batch.schema();
        let batches: Box<dyn RecordBatchReader + Send> =
            Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema));

        let conexao = lancedb::connect("memory://")
            .execute()
            .await
            .map_err(|e| SessionSemanticIndexError::LanceDb(e.to_string()))?;
        let tabela = conexao
            .create_table(NOME_TABELA, batches)
            .execute()
            .await
            .map_err(|e| SessionSemanticIndexError::LanceDb(e.to_string()))?;

        Ok(Self {
            tabela: Some(tabela),
        })
    }

    /// Consulta o índice pelo vetor dado (busca por vizinho mais próximo),
    /// devolvendo até `limite` chunks reconstruídos, do mais para o menos
    /// próximo.
    ///
    /// # Errors
    ///
    /// Devolve [`SessionSemanticIndexError::LanceDb`] se a busca falhar
    /// internamente (ex.: `vetor` com dimensão diferente da indexada).
    pub async fn search(
        &self,
        vetor: &[f32],
        limite: usize,
    ) -> Result<Vec<SessionChunk>, SessionSemanticIndexError> {
        let Some(tabela) = &self.tabela else {
            return Ok(Vec::new());
        };

        let batches: Vec<RecordBatch> = tabela
            .query()
            .limit(limite)
            .nearest_to(vetor)
            .map_err(|e| SessionSemanticIndexError::LanceDb(e.to_string()))?
            .execute()
            .await
            .map_err(|e| SessionSemanticIndexError::LanceDb(e.to_string()))?
            .try_collect()
            .await
            .map_err(|e| SessionSemanticIndexError::LanceDb(e.to_string()))?;

        Ok(batches.iter().flat_map(record_batch_para_chunks).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::rag::session_chunk::chunk_session;
    use crate::model::{Message, Usage};
    use crate::provider::mock::MockProvider;
    use crate::provider::EmbeddingsResponse;

    fn chunks_de_exemplo() -> Vec<SessionChunk> {
        let mensagens = vec![
            Message::user("qual a capital da frança"),
            Message::assistant("paris é a capital da frança"),
        ];
        chunk_session("20260724-000000", &mensagens)
    }

    fn mock_com_vetores(vetores: Vec<Vec<f32>>) -> MockProvider {
        let mock = MockProvider::new("mock-embeddings");
        mock.enqueue_embeddings(Ok(EmbeddingsResponse {
            vectors: vetores,
            usage: Usage::default(),
        }));
        mock
    }

    #[tokio::test]
    async fn consulta_por_vetor_devolve_o_chunk_mais_proximo_no_topo() {
        let chunks = chunks_de_exemplo();
        // A mensagem do usuário (índice 0) fica próxima de [1.0, 0.0]; a
        // do agente (índice 1) de [0.0, 1.0].
        let mock = mock_com_vetores(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);

        let indice = SessionSemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect("deve construir o índice");

        let resultados = indice
            .search(&[0.0, 1.0], 5)
            .await
            .expect("busca deve funcionar");

        assert!(!resultados.is_empty());
        assert_eq!(resultados[0].role, Role::Assistant);
    }

    #[tokio::test]
    async fn limite_restringe_a_quantidade_de_resultados() {
        let chunks = chunks_de_exemplo();
        let mock = mock_com_vetores(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);

        let indice = SessionSemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect("deve construir o índice");

        let resultados = indice
            .search(&[0.5, 0.5], 1)
            .await
            .expect("busca deve funcionar");

        assert_eq!(resultados.len(), 1);
    }

    #[tokio::test]
    async fn chunks_vazio_nao_e_erro_e_busca_devolve_lista_vazia() {
        let mock = MockProvider::new("mock-embeddings");

        let indice = SessionSemanticIndex::build(Vec::new(), &mock, "embed-x")
            .await
            .expect("chunks vazio não deve ser erro");

        let resultados = indice
            .search(&[1.0, 0.0], 5)
            .await
            .expect("busca sobre índice vazio não deve ser erro");

        assert!(resultados.is_empty());
    }

    #[tokio::test]
    async fn contagem_de_vetores_diferente_da_de_chunks_e_erro() {
        let chunks = chunks_de_exemplo();
        let mock = mock_com_vetores(vec![vec![1.0, 0.0]]); // só 1 vetor para 2 chunks

        let erro = SessionSemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect_err("contagem inconsistente deve ser erro");

        assert!(matches!(
            erro,
            SessionSemanticIndexError::ContagemDeVetoresInconsistente {
                esperado: 2,
                recebido: 1
            }
        ));
    }

    #[tokio::test]
    async fn chunk_reconstruido_preserva_todos_os_metadados() {
        let chunks = chunks_de_exemplo();
        let mock = mock_com_vetores(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);

        let indice = SessionSemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect("deve construir o índice");

        let resultados = indice
            .search(&[1.0, 0.0], 5)
            .await
            .expect("busca deve funcionar");
        let usuario = resultados
            .into_iter()
            .find(|c| c.role == Role::User)
            .expect("mensagem de usuário deve ter sido indexada e encontrada");

        assert_eq!(usuario.session_id, "20260724-000000");
        assert_eq!(usuario.message_index, 0);
        assert_eq!(usuario.text, "qual a capital da frança");
    }
}
