// Caminho relativo: crates/core/src/context/mod.rs
//! Extração de contexto do repositório (Fase 6): repo-map estilo Aider via
//! `tree-sitter` (ADR-0010), *grounding* via LSP (ADR-0013) e RAG semântico
//! local (ADR-0011) — os três se complementam, nenhum substitui os outros.
//!
//! [`ast`] traz a extração de símbolos (MT-18); [`repo_map`] o grafo de
//! referências (MT-19), o ranking estilo PageRank (MT-20) e — em
//! `crates/core/src/tools/repo_map.rs` — a tool exposta ao agent loop
//! (MT-21). [`lsp`] traz o cliente LSP mínimo (MT-23) e — em
//! `crates/core/src/tools/lsp.rs` — as tools de leitura (MT-24). [`rag`]
//! traz o chunking AST-aware (MT-25), o índice lexical BM25 (MT-26), o
//! índice semântico via embeddings (MT-27), a busca híbrida com
//! *reranking* (MT-28) e a indexação incremental (MT-29).

pub mod ast;
pub mod lsp;
pub mod rag;
pub mod repo_map;
