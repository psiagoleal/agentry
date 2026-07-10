// Caminho relativo: crates/core/src/context/rag/hybrid_search.rs
//! Busca híbrida + *reranking* sobre os índices lexical e semântico (MT-28, ADR-0011).
//!
//! Combina o índice lexical (`tantivy`/BM25, MT-26) e o semântico
//! (embeddings/`lancedb`, MT-27) via *reciprocal rank fusion* (RRF) —
//! soma, por chunk, o inverso da posição em cada lista, dando mais peso a
//! quem aparece bem posicionado em ambas, sem exigir normalizar escalas de
//! score tão diferentes quanto BM25 e distância vetorial. Em seguida
//! reordena o top-K com um *reranker* cross-encoder — servido localmente
//! via a mesma `trait LlmProvider` (MT-03), **sem API nova** (ADR-0011): o
//! "cross-encoder" aqui é uma chamada de chat pedindo ao modelo para
//! ordenar os candidatos por relevância.

use crate::model::Message;
use crate::provider::{ChatRequest, LlmProvider, ProviderError};

use super::chunk::Chunk;
use super::lexical_index::{LexicalIndex, LexicalIndexError};
use super::semantic_index::{SemanticIndex, SemanticIndexError};

/// Constante de suavização do RRF — mesmo valor (60) usual na literatura/
/// prática (Cormack et al.), reduz a sensibilidade da fusão a pequenas
/// variações de posição no topo da lista.
const RRF_K: f64 = 60.0;

/// Erros da busca híbrida — indicam falha de um dos índices ou do provider
/// de *reranking*; nenhum indica um problema na consulta dada pelo
/// chamador em uso normal.
#[derive(Debug)]
pub enum HybridSearchError {
    Lexical(LexicalIndexError),
    Semantic(SemanticIndexError),
    Provider(ProviderError),
    /// A resposta do *reranker* não é um array JSON de exatamente
    /// `chunks.len()` índices válidos e sem repetição.
    RerankParse(String),
}

impl std::fmt::Display for HybridSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lexical(e) => write!(f, "índice lexical falhou: {e}"),
            Self::Semantic(e) => write!(f, "índice semântico falhou: {e}"),
            Self::Provider(e) => write!(f, "provider de reranking falhou: {e}"),
            Self::RerankParse(msg) => write!(f, "resposta do reranker inválida: {msg}"),
        }
    }
}

impl std::error::Error for HybridSearchError {}

/// Combina `lexical` e `semantic` via *reciprocal rank fusion*, devolvendo
/// até `limite` chunks. Um chunk que aparece em ambas as listas acumula a
/// contribuição das duas posições — reflete tanto *match* lexical exato
/// quanto proximidade semântica, mesmo quando nenhuma das duas listas
/// isoladamente o colocaria no topo.
#[must_use]
pub fn fuse(lexical: &[Chunk], semantic: &[Chunk], limite: usize) -> Vec<Chunk> {
    let mut pontuados: Vec<(Chunk, f64)> = Vec::new();

    for lista in [lexical, semantic] {
        for (posicao, chunk) in lista.iter().enumerate() {
            let contribuicao = 1.0 / (RRF_K + (posicao + 1) as f64);
            if let Some(existente) = pontuados.iter_mut().find(|(c, _)| c == chunk) {
                existente.1 += contribuicao;
            } else {
                pontuados.push((chunk.clone(), contribuicao));
            }
        }
    }

    pontuados.sort_by(|a, b| b.1.total_cmp(&a.1));
    pontuados
        .into_iter()
        .take(limite)
        .map(|(chunk, _)| chunk)
        .collect()
}

fn prompt_de_reranking(query: &str, chunks: &[Chunk]) -> String {
    let candidatos = chunks
        .iter()
        .enumerate()
        .map(|(indice, chunk)| {
            format!(
                "{indice}: [{}] {}\n{}",
                chunk.symbol, chunk.file, chunk.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n---\n");

    format!(
        "Ordene os candidatos abaixo por relevância para a consulta, do mais para o \
         menos relevante.\n\nConsulta: \"{query}\"\n\n{candidatos}\n\nResponda apenas \
         com um array JSON dos índices numéricos, do mais para o menos relevante \
         (ex.: [2, 0, 1]). Nenhum texto além do array."
    )
}

fn parse_ordem(resposta: &str, total: usize) -> Result<Vec<usize>, HybridSearchError> {
    let indices: Vec<usize> = serde_json::from_str(resposta.trim())
        .map_err(|e| HybridSearchError::RerankParse(format!("JSON inválido: {e}")))?;

    if indices.len() != total {
        return Err(HybridSearchError::RerankParse(format!(
            "esperava {total} índice(s), recebeu {}",
            indices.len()
        )));
    }

    let mut vistos = vec![false; total];
    for &i in &indices {
        if i >= total || vistos[i] {
            return Err(HybridSearchError::RerankParse(format!(
                "índice inválido ou repetido: {i}"
            )));
        }
        vistos[i] = true;
    }

    Ok(indices)
}

/// Reordena `chunks` por relevância à `query`, via uma chamada de chat ao
/// `provider` (mesma `trait LlmProvider`, sem API nova — ADR-0011) pedindo
/// a ordem de relevância como array JSON de índices. `chunks` com 0 ou 1
/// elemento não chama o provider — não há o que reordenar.
///
/// # Errors
///
/// Devolve [`HybridSearchError::Provider`] se a chamada de chat falhar;
/// [`HybridSearchError::RerankParse`] se a resposta não for um array JSON
/// de exatamente `chunks.len()` índices válidos e sem repetição — nesses
/// casos a ordem original (da fusão) não é confiável o bastante para ser
/// usada silenciosamente, então o erro é reportado em vez de mascarado.
pub async fn rerank(
    chunks: Vec<Chunk>,
    query: &str,
    provider: &dyn LlmProvider,
    model: &str,
) -> Result<Vec<Chunk>, HybridSearchError> {
    if chunks.len() <= 1 {
        return Ok(chunks);
    }

    let prompt = prompt_de_reranking(query, &chunks);
    let request = ChatRequest::new(model.to_string(), vec![Message::user(prompt)]);
    let resposta = provider
        .chat(request)
        .await
        .map_err(HybridSearchError::Provider)?;

    let ordem = parse_ordem(&resposta.message.text_content(), chunks.len())?;
    Ok(ordem.into_iter().map(|i| chunks[i].clone()).collect())
}

/// Pipeline completo: consulta os dois índices, funde via RRF ([`fuse`]) e
/// reordena o resultado com [`rerank`].
///
/// # Errors
///
/// Propaga falhas de qualquer uma das três etapas — ver
/// [`LexicalIndex::search`], [`SemanticIndex::search`] e [`rerank`].
pub async fn hybrid_search(
    lexical: &LexicalIndex,
    semantic: &SemanticIndex,
    query: &str,
    query_vector: &[f32],
    limite: usize,
    reranker_provider: &dyn LlmProvider,
    reranker_model: &str,
) -> Result<Vec<Chunk>, HybridSearchError> {
    let candidatos_lexical = lexical
        .search(query, limite)
        .map_err(HybridSearchError::Lexical)?;
    let candidatos_semantico = semantic
        .search(query_vector, limite)
        .await
        .map_err(HybridSearchError::Semantic)?;

    let fundidos = fuse(&candidatos_lexical, &candidatos_semantico, limite);
    rerank(fundidos, query, reranker_provider, reranker_model).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ast::Language;
    use crate::context::rag::chunk::chunk_file;
    use crate::model::Usage;
    use crate::provider::mock::MockProvider;
    use crate::provider::EmbeddingsResponse;

    fn chunk_de(symbol: &str) -> Chunk {
        Chunk {
            file: "src/lib.rs".to_string(),
            symbol: symbol.to_string(),
            kind: crate::context::ast::SymbolKind::Function,
            range: 0..1,
            text: format!("fn {symbol}() {{}}"),
        }
    }

    #[test]
    fn fusao_reflete_tanto_match_lexical_quanto_semantico() {
        let a = chunk_de("a"); // só no lexical, rank 1
        let b = chunk_de("b"); // rank 2 no lexical, rank 1 no semântico
        let c = chunk_de("c"); // só no semântico, rank 2

        let lexical = vec![a.clone(), b.clone()];
        let semantico = vec![b.clone(), c.clone()];

        let resultado = fuse(&lexical, &semantico, 3);

        // b aparece nas duas listas (rank 2 + rank 1) e supera a (só rank 1
        // no lexical) — a fusão reflete os dois sinais, não só o melhor
        // rank isolado de uma única lista.
        assert_eq!(resultado, vec![b, a, c]);
    }

    #[test]
    fn fuse_respeita_o_limite() {
        let a = chunk_de("a");
        let b = chunk_de("b");

        let resultado = fuse(&[a.clone(), b.clone()], &[], 1);

        assert_eq!(resultado, vec![a]);
    }

    fn mock_com_resposta_de_chat(texto: &str) -> MockProvider {
        let mock = MockProvider::new("mock-rerank");
        mock.enqueue_chat(Ok(crate::provider::ChatResponse {
            message: Message::assistant(texto),
            usage: Usage::default(),
        }));
        mock
    }

    #[tokio::test]
    async fn rerank_reordena_um_caso_conhecido() {
        let a = chunk_de("a");
        let b = chunk_de("b");
        let c = chunk_de("c");
        let mock = mock_com_resposta_de_chat("[2, 0, 1]");

        let resultado = rerank(
            vec![a.clone(), b.clone(), c.clone()],
            "consulta",
            &mock,
            "m",
        )
        .await
        .expect("reranking deve funcionar");

        assert_eq!(resultado, vec![c, a, b]);
    }

    #[tokio::test]
    async fn rerank_com_resposta_malformada_e_erro_tratado() {
        let a = chunk_de("a");
        let b = chunk_de("b");
        let mock = mock_com_resposta_de_chat("não sei ordenar isso");

        let erro = rerank(vec![a, b], "consulta", &mock, "m")
            .await
            .expect_err("resposta não-JSON deve ser erro");

        assert!(matches!(erro, HybridSearchError::RerankParse(_)));
    }

    #[tokio::test]
    async fn rerank_com_zero_ou_um_chunk_nao_chama_o_provider() {
        let mock = MockProvider::new("mock-sem-fila"); // sem resposta enfileirada

        let vazio = rerank(Vec::new(), "consulta", &mock, "m")
            .await
            .expect("lista vazia não deve chamar o provider");
        assert!(vazio.is_empty());

        let um = chunk_de("a");
        let resultado = rerank(vec![um.clone()], "consulta", &mock, "m")
            .await
            .expect("um único chunk não deve chamar o provider");
        assert_eq!(resultado, vec![um]);
    }

    #[tokio::test]
    async fn hybrid_search_pipeline_completo_funde_e_reordena() {
        let source = "\
fn soma(a: i32, b: i32) -> i32 {
    a + b
}

fn multiplica(a: i32, b: i32) -> i32 {
    a * b
}
";
        let chunks = chunk_file("src/lib.rs", source, Language::Rust).expect("deve parsear");

        let mock = MockProvider::new("mock-hybrid");
        mock.enqueue_embeddings(Ok(EmbeddingsResponse {
            vectors: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
            usage: Usage::default(),
        }));

        let lexical = LexicalIndex::build(chunks.clone()).expect("índice lexical");
        let semantico = SemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect("índice semântico");

        // Tanto a busca lexical (identificador exato) quanto a semântica
        // (vetor [1.0, 0.0]) apontam "soma" como mais relevante — a fusão
        // deve concordar. O reranker (mock) inverte essa ordem, provando
        // que o resultado final reflete o reranking, não só a fusão.
        mock.enqueue_chat(Ok(crate::provider::ChatResponse {
            message: Message::assistant("[1, 0]"),
            usage: Usage::default(),
        }));

        let resultado = hybrid_search(&lexical, &semantico, "soma", &[1.0, 0.0], 2, &mock, "m")
            .await
            .expect("pipeline híbrido deve funcionar");

        assert_eq!(resultado.len(), 2);
        assert_eq!(resultado[0].symbol, "multiplica");
        assert_eq!(resultado[1].symbol, "soma");
    }
}
