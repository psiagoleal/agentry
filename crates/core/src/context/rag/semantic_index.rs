// Caminho relativo: crates/core/src/context/rag/semantic_index.rs
//! Índice semântico (embeddings) sobre os chunks do RAG (MT-27, ADR-0011).
//!
//! Usa `lancedb` — embutido/*in-process* sobre o formato Lance (Arrow),
//! sem servidor externo nem ponte FFI (ADR-0011) — para encontrar código
//! semanticamente parecido cujo nome/localização o modelo não conhece de
//! antemão, o caso em que o índice lexical ([`super::lexical_index`],
//! MT-26) erra. Vetores gerados via `LlmProvider::embeddings` (MT-03) já
//! existente — **nenhum adapter novo de embeddings** (proibido pelo
//! ADR-0011). Complementa, não substitui, o índice lexical; a combinação
//! dos dois (busca híbrida + *reranking*) é o MT-28.

use std::sync::Arc;

use arrow_array::types::Float32Type;
use arrow_array::{
    Array, FixedSizeListArray, RecordBatch, RecordBatchIterator, RecordBatchReader, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};

use super::chunk::Chunk;
use super::{kind_from_str, kind_to_str};
use crate::provider::{EmbeddingsRequest, LlmProvider, ProviderError};

const NOME_TABELA: &str = "chunks";

/// Erros do índice semântico — indicam falha do provider de embeddings,
/// resposta inconsistente dele, ou falha interna do `lancedb`; nenhum
/// indica um problema nos chunks dados pelo chamador em uso normal.
#[derive(Debug)]
pub enum SemanticIndexError {
    /// O provider de embeddings falhou.
    Provider(ProviderError),
    /// O provider devolveu uma quantidade de vetores diferente da
    /// quantidade de chunks enviados — resposta incompatível com o
    /// contrato de [`LlmProvider::embeddings`].
    ContagemDeVetoresInconsistente { esperado: usize, recebido: usize },
    /// Os vetores devolvidos não têm todos a mesma dimensão.
    DimensaoInconsistente,
    /// Falha interna do `lancedb` (schema/consulta malformados, escrita
    /// na tabela).
    LanceDb(String),
}

impl std::fmt::Display for SemanticIndexError {
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
            Self::LanceDb(msg) => write!(f, "erro no índice semântico (lancedb): {msg}"),
        }
    }
}

impl std::error::Error for SemanticIndexError {}

fn schema_com_dimensao(dimensao: i32) -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("file", DataType::Utf8, false),
        Field::new("symbol", DataType::Utf8, false),
        Field::new("kind", DataType::Utf8, false),
        Field::new("range_start", DataType::UInt64, false),
        Field::new("range_end", DataType::UInt64, false),
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
    chunks: &[Chunk],
    vetores: &[Vec<f32>],
    dimensao: usize,
) -> Result<RecordBatch, SemanticIndexError> {
    let schema = schema_com_dimensao(dimensao as i32);

    let file = StringArray::from_iter_values(chunks.iter().map(|c| c.file.as_str()));
    let symbol = StringArray::from_iter_values(chunks.iter().map(|c| c.symbol.as_str()));
    let kind = StringArray::from_iter_values(chunks.iter().map(|c| kind_to_str(c.kind)));
    let range_start =
        arrow_array::UInt64Array::from_iter_values(chunks.iter().map(|c| c.range.start as u64));
    let range_end =
        arrow_array::UInt64Array::from_iter_values(chunks.iter().map(|c| c.range.end as u64));
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
            Arc::new(file),
            Arc::new(symbol),
            Arc::new(kind),
            Arc::new(range_start),
            Arc::new(range_end),
            Arc::new(text),
            Arc::new(vector),
        ],
    )
    .map_err(|e| SemanticIndexError::LanceDb(e.to_string()))
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

fn record_batch_para_chunks(batch: &RecordBatch) -> Vec<Chunk> {
    let file = coluna_texto(batch, "file");
    let symbol = coluna_texto(batch, "symbol");
    let kind = coluna_texto(batch, "kind");
    let range_start = coluna_u64(batch, "range_start");
    let range_end = coluna_u64(batch, "range_end");
    let text = coluna_texto(batch, "text");

    (0..batch.num_rows())
        .map(|i| Chunk {
            file: file[i].to_string(),
            symbol: symbol[i].to_string(),
            kind: kind_from_str(kind[i]),
            range: (range_start[i] as usize)..(range_end[i] as usize),
            text: text[i].to_string(),
        })
        .collect()
}

/// Índice semântico (embeddings) sobre um conjunto de chunks (MT-25).
///
/// Embutido/*in-process* (`lancedb` sobre `memory://`, sem servidor
/// externo) — construído uma vez a partir de `Vec<Chunk>` via
/// [`SemanticIndex::build`], consultado via [`SemanticIndex::search`].
#[derive(Debug)]
pub struct SemanticIndex {
    tabela: Option<lancedb::Table>,
}

impl SemanticIndex {
    /// Gera embeddings para o texto de cada chunk via `provider`
    /// (`LlmProvider::embeddings`, MT-03) e indexa os vetores resultantes.
    ///
    /// `chunks` vazio não é erro — devolve um índice sem tabela por trás,
    /// cujas buscas sempre respondem lista vazia (não há dimensão de
    /// vetor conhecida sem ao menos um embedding real).
    ///
    /// # Errors
    ///
    /// Devolve [`SemanticIndexError::Provider`] se a chamada de
    /// embeddings falhar; [`SemanticIndexError::ContagemDeVetoresInconsistente`]
    /// ou [`SemanticIndexError::DimensaoInconsistente`] se a resposta do
    /// provider não bater com o número/formato esperado de vetores —
    /// ambos indicam um provider mal implementado, não um problema nos
    /// chunks; [`SemanticIndexError::LanceDb`] se a escrita na tabela
    /// falhar internamente.
    pub async fn build(
        chunks: Vec<Chunk>,
        provider: &dyn LlmProvider,
        embedding_model: &str,
    ) -> Result<Self, SemanticIndexError> {
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
            .map_err(SemanticIndexError::Provider)?;

        if resposta.vectors.len() != chunks.len() {
            return Err(SemanticIndexError::ContagemDeVetoresInconsistente {
                esperado: chunks.len(),
                recebido: resposta.vectors.len(),
            });
        }

        let dimensao = resposta.vectors[0].len();
        if resposta.vectors.iter().any(|v| v.len() != dimensao) {
            return Err(SemanticIndexError::DimensaoInconsistente);
        }

        let batch = construir_record_batch(&chunks, &resposta.vectors, dimensao)?;
        let schema = batch.schema();
        let batches: Box<dyn RecordBatchReader + Send> =
            Box::new(RecordBatchIterator::new(vec![Ok(batch)], schema));

        let conexao = lancedb::connect("memory://")
            .execute()
            .await
            .map_err(|e| SemanticIndexError::LanceDb(e.to_string()))?;
        let tabela = conexao
            .create_table(NOME_TABELA, batches)
            .execute()
            .await
            .map_err(|e| SemanticIndexError::LanceDb(e.to_string()))?;

        Ok(Self {
            tabela: Some(tabela),
        })
    }

    /// Consulta o índice pelo vetor dado (busca por vizinho mais próximo),
    /// devolvendo até `limite` chunks reconstruídos a partir das colunas
    /// armazenadas, do mais para o menos próximo.
    ///
    /// # Errors
    ///
    /// Devolve [`SemanticIndexError::LanceDb`] se a busca falhar
    /// internamente (ex.: `vetor` com dimensão diferente da indexada).
    pub async fn search(
        &self,
        vetor: &[f32],
        limite: usize,
    ) -> Result<Vec<Chunk>, SemanticIndexError> {
        let Some(tabela) = &self.tabela else {
            return Ok(Vec::new());
        };

        let batches: Vec<RecordBatch> = tabela
            .query()
            .limit(limite)
            .nearest_to(vetor)
            .map_err(|e| SemanticIndexError::LanceDb(e.to_string()))?
            .execute()
            .await
            .map_err(|e| SemanticIndexError::LanceDb(e.to_string()))?
            .try_collect()
            .await
            .map_err(|e| SemanticIndexError::LanceDb(e.to_string()))?;

        Ok(batches.iter().flat_map(record_batch_para_chunks).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ast::{Language, SymbolKind};
    use crate::context::rag::chunk::chunk_file;
    use crate::model::Usage;
    use crate::provider::mock::MockProvider;
    use crate::provider::EmbeddingsResponse;

    fn chunks_de_exemplo() -> Vec<Chunk> {
        let source = "\
fn soma(a: i32, b: i32) -> i32 {
    a + b
}

fn multiplica(a: i32, b: i32) -> i32 {
    a * b
}
";
        chunk_file("src/lib.rs", source, Language::Rust).expect("deve parsear")
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
        // "soma" fica próximo de [1.0, 0.0]; "multiplica" de [0.0, 1.0].
        let mock = mock_com_vetores(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);

        let indice = SemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect("deve construir o índice");

        let resultados = indice
            .search(&[0.0, 1.0], 5)
            .await
            .expect("busca deve funcionar");

        assert!(!resultados.is_empty());
        assert_eq!(resultados[0].symbol, "multiplica");
    }

    #[tokio::test]
    async fn limite_restringe_a_quantidade_de_resultados() {
        let chunks = chunks_de_exemplo();
        let mock = mock_com_vetores(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);

        let indice = SemanticIndex::build(chunks, &mock, "embed-x")
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

        let indice = SemanticIndex::build(Vec::new(), &mock, "embed-x")
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

        let erro = SemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect_err("contagem inconsistente deve ser erro");

        assert!(matches!(
            erro,
            SemanticIndexError::ContagemDeVetoresInconsistente {
                esperado: 2,
                recebido: 1
            }
        ));
    }

    #[tokio::test]
    async fn chunk_reconstruido_preserva_todos_os_metadados() {
        let chunks = chunks_de_exemplo();
        let mock = mock_com_vetores(vec![vec![1.0, 0.0], vec![0.0, 1.0]]);

        let indice = SemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect("deve construir o índice");

        let resultados = indice
            .search(&[1.0, 0.0], 5)
            .await
            .expect("busca deve funcionar");
        let soma = resultados
            .into_iter()
            .find(|c| c.symbol == "soma")
            .expect("soma deve ter sido indexado e encontrado");

        assert_eq!(soma.file, "src/lib.rs");
        assert_eq!(soma.kind, SymbolKind::Function);
        assert!(soma.text.contains("a + b"));
        assert_eq!(soma.range.end - soma.range.start, soma.text.len());
    }
}
