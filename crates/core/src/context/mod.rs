// Caminho relativo: crates/core/src/context/mod.rs
//! Extração de contexto do repositório (Fase 6): repo-map estilo Aider via
//! `tree-sitter` (ADR-0010) e *grounding* via LSP (ADR-0013) — ambos
//! complementam, não substituem, o RAG semântico do ADR-0011.
//!
//! [`ast`] traz a extração de símbolos (MT-18); [`repo_map`] o grafo de
//! referências (MT-19), o ranking estilo PageRank (MT-20) e — em
//! `crates/core/src/tools/repo_map.rs` — a tool exposta ao agent loop
//! (MT-21). [`lsp`] traz o cliente LSP mínimo (MT-23); a tool de leitura
//! (hover/definição/referências) chega no MT-24.

pub mod ast;
pub mod lsp;
pub mod repo_map;
