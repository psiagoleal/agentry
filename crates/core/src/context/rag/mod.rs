// Caminho relativo: crates/core/src/context/rag/mod.rs
//! RAG semântico local para código (Fase 6, ADR-0011): chunking AST-aware
//! ([`chunk`], MT-25), índice lexical (MT-26), índice semântico (MT-27) e
//! busca híbrida com *reranking* (MT-28) chegam nos micro-tickets seguintes
//! — complementa (não substitui) o repo-map do ADR-0010.

pub mod chunk;
