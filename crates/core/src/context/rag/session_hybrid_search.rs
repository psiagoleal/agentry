// Caminho relativo: crates/core/src/context/rag/session_hybrid_search.rs
//! Busca híbrida + *reranking* sobre os índices de sessão (MT-134, ADR-0039).
//!
//! Análogo a [`super::hybrid_search`] (código): combina o índice lexical
//! ([`super::session_lexical_index`], MT-132) e o semântico
//! ([`super::session_semantic_index`], MT-133) via *reciprocal rank
//! fusion* (RRF, mesma constante de suavização `RRF_K = 60.0`) e reordena
//! o top-K com um *reranker* via `LlmProvider::chat` — **sempre** o
//! provider Ollama local (ADR-0039 §2), nunca a task-class de chat
//! configurada; a garantia vem de quem chama (MT-136), não deste módulo.

use crate::model::Message;
use crate::provider::{ChatRequest, LlmProvider, ProviderError};

use super::session_chunk::SessionChunk;
use super::session_lexical_index::{SessionLexicalIndex, SessionLexicalIndexError};
use super::session_semantic_index::{SessionSemanticIndex, SessionSemanticIndexError};

const RRF_K: f64 = 60.0;

/// Erros da busca híbrida de sessão — mesma semântica de
/// [`super::hybrid_search::HybridSearchError`].
#[derive(Debug)]
pub enum SessionHybridSearchError {
    Lexical(SessionLexicalIndexError),
    Semantic(SessionSemanticIndexError),
    Provider(ProviderError),
    RerankParse(String),
}

impl std::fmt::Display for SessionHybridSearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Lexical(e) => write!(f, "índice lexical de sessão falhou: {e}"),
            Self::Semantic(e) => write!(f, "índice semântico de sessão falhou: {e}"),
            Self::Provider(e) => write!(f, "provider de reranking falhou: {e}"),
            Self::RerankParse(msg) => write!(f, "resposta do reranker inválida: {msg}"),
        }
    }
}

impl std::error::Error for SessionHybridSearchError {}

/// Combina `lexical` e `semantic` via *reciprocal rank fusion*, devolvendo
/// até `limite` chunks — mesmo algoritmo de [`super::hybrid_search::fuse`].
#[must_use]
pub fn fuse(
    lexical: &[SessionChunk],
    semantic: &[SessionChunk],
    limite: usize,
) -> Vec<SessionChunk> {
    let mut pontuados: Vec<(SessionChunk, f64)> = Vec::new();

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

fn prompt_de_reranking(query: &str, chunks: &[SessionChunk]) -> String {
    let candidatos = chunks
        .iter()
        .enumerate()
        .map(|(indice, chunk)| {
            format!(
                "{indice}: [sessão {}, mensagem {}] {}",
                chunk.session_id, chunk.message_index, chunk.text
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

fn parse_ordem(resposta: &str, total: usize) -> Result<Vec<usize>, SessionHybridSearchError> {
    let indices: Vec<usize> = serde_json::from_str(resposta.trim())
        .map_err(|e| SessionHybridSearchError::RerankParse(format!("JSON inválido: {e}")))?;

    if indices.len() != total {
        return Err(SessionHybridSearchError::RerankParse(format!(
            "esperava {total} índice(s), recebeu {}",
            indices.len()
        )));
    }

    let mut vistos = vec![false; total];
    for &i in &indices {
        if i >= total || vistos[i] {
            return Err(SessionHybridSearchError::RerankParse(format!(
                "índice inválido ou repetido: {i}"
            )));
        }
        vistos[i] = true;
    }

    Ok(indices)
}

/// Reordena `chunks` por relevância à `query`, via uma chamada de chat ao
/// `provider`. `chunks` com 0 ou 1 elemento não chama o provider.
///
/// # Errors
///
/// Devolve [`SessionHybridSearchError::Provider`] se a chamada de chat
/// falhar; [`SessionHybridSearchError::RerankParse`] se a resposta não for
/// um array JSON de exatamente `chunks.len()` índices válidos e sem
/// repetição.
pub async fn rerank(
    chunks: Vec<SessionChunk>,
    query: &str,
    provider: &dyn LlmProvider,
    model: &str,
) -> Result<Vec<SessionChunk>, SessionHybridSearchError> {
    if chunks.len() <= 1 {
        return Ok(chunks);
    }

    let prompt = prompt_de_reranking(query, &chunks);
    let request = ChatRequest::new(model.to_string(), vec![Message::user(prompt)]);
    let resposta = provider
        .chat(request)
        .await
        .map_err(SessionHybridSearchError::Provider)?;

    let ordem = parse_ordem(&resposta.message.text_content(), chunks.len())?;
    Ok(ordem.into_iter().map(|i| chunks[i].clone()).collect())
}

/// Pipeline completo: consulta os dois índices, funde via RRF ([`fuse`]) e
/// reordena o resultado com [`rerank`].
///
/// # Errors
///
/// Propaga falhas de qualquer uma das três etapas.
pub async fn hybrid_search(
    lexical: &SessionLexicalIndex,
    semantic: &SessionSemanticIndex,
    query: &str,
    query_vector: &[f32],
    limite: usize,
    reranker_provider: &dyn LlmProvider,
    reranker_model: &str,
) -> Result<Vec<SessionChunk>, SessionHybridSearchError> {
    let candidatos_lexical = lexical
        .search(query, limite)
        .map_err(SessionHybridSearchError::Lexical)?;
    let candidatos_semantico = semantic
        .search(query_vector, limite)
        .await
        .map_err(SessionHybridSearchError::Semantic)?;

    let fundidos = fuse(&candidatos_lexical, &candidatos_semantico, limite);
    rerank(fundidos, query, reranker_provider, reranker_model).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::rag::session_chunk::chunk_session;
    use crate::model::{Message as ModelMessage, Usage};
    use crate::provider::mock::MockProvider;
    use crate::provider::EmbeddingsResponse;

    fn chunk_de(session_id: &str, message_index: usize, texto: &str) -> SessionChunk {
        SessionChunk {
            session_id: session_id.to_string(),
            message_index,
            role: crate::model::Role::User,
            text: texto.to_string(),
        }
    }

    #[test]
    fn fusao_reflete_tanto_match_lexical_quanto_semantico() {
        let a = chunk_de("s1", 0, "a");
        let b = chunk_de("s1", 1, "b");
        let c = chunk_de("s1", 2, "c");

        let lexical = vec![a.clone(), b.clone()];
        let semantico = vec![b.clone(), c.clone()];

        let resultado = fuse(&lexical, &semantico, 3);

        assert_eq!(resultado, vec![b, a, c]);
    }

    #[test]
    fn fuse_respeita_o_limite() {
        let a = chunk_de("s1", 0, "a");
        let b = chunk_de("s1", 1, "b");

        let resultado = fuse(&[a.clone(), b.clone()], &[], 1);

        assert_eq!(resultado, vec![a]);
    }

    fn mock_com_resposta_de_chat(texto: &str) -> MockProvider {
        let mock = MockProvider::new("mock-rerank");
        mock.enqueue_chat(Ok(crate::provider::ChatResponse {
            message: ModelMessage::assistant(texto),
            usage: Usage::default(),
        }));
        mock
    }

    #[tokio::test]
    async fn rerank_reordena_um_caso_conhecido() {
        let a = chunk_de("s1", 0, "a");
        let b = chunk_de("s1", 1, "b");
        let c = chunk_de("s1", 2, "c");
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
        let a = chunk_de("s1", 0, "a");
        let b = chunk_de("s1", 1, "b");
        let mock = mock_com_resposta_de_chat("não sei ordenar isso");

        let erro = rerank(vec![a, b], "consulta", &mock, "m")
            .await
            .expect_err("resposta não-JSON deve ser erro");

        assert!(matches!(erro, SessionHybridSearchError::RerankParse(_)));
    }

    #[tokio::test]
    async fn rerank_com_zero_ou_um_chunk_nao_chama_o_provider() {
        let mock = MockProvider::new("mock-sem-fila"); // sem resposta enfileirada

        let vazio = rerank(Vec::new(), "consulta", &mock, "m")
            .await
            .expect("lista vazia não deve chamar o provider");
        assert!(vazio.is_empty());

        let um = chunk_de("s1", 0, "a");
        let resultado = rerank(vec![um.clone()], "consulta", &mock, "m")
            .await
            .expect("um único chunk não deve chamar o provider");
        assert_eq!(resultado, vec![um]);
    }

    #[tokio::test]
    async fn hybrid_search_pipeline_completo_funde_e_reordena() {
        let mensagens = vec![
            ModelMessage::user("qual a capital da frança"),
            ModelMessage::assistant("qual a capital da alemanha"),
        ];
        let chunks = chunk_session("s1", &mensagens);

        let mock = MockProvider::new("mock-hybrid");
        mock.enqueue_embeddings(Ok(EmbeddingsResponse {
            vectors: vec![vec![1.0, 0.0], vec![0.0, 1.0]],
            usage: Usage::default(),
        }));

        let lexical = SessionLexicalIndex::build(chunks.clone()).expect("índice lexical");
        let semantico = SessionSemanticIndex::build(chunks, &mock, "embed-x")
            .await
            .expect("índice semântico");

        mock.enqueue_chat(Ok(crate::provider::ChatResponse {
            message: ModelMessage::assistant("[1, 0]"),
            usage: Usage::default(),
        }));

        let resultado = hybrid_search(&lexical, &semantico, "frança", &[1.0, 0.0], 2, &mock, "m")
            .await
            .expect("pipeline híbrido deve funcionar");

        assert_eq!(resultado.len(), 2);
        // O reranker (mock) inverte a ordem da fusão -- prova que o
        // resultado final reflete o reranking, não só a fusão.
        assert_eq!(resultado[0].message_index, 1);
        assert_eq!(resultado[1].message_index, 0);
    }
}
