// Caminho relativo: crates/core/src/context/mod.rs
//! Extração de contexto do repositório (Fase 6, ADR-0010): repo-map estilo
//! Aider via `tree-sitter`, sem vector DB — complementa (não substitui) o
//! RAG semântico do ADR-0011.
//!
//! [`ast`] traz a extração de símbolos (MT-18); [`repo_map::graph`] o grafo
//! de referências entre arquivos (MT-19). Ranking de relevância (MT-20) e a
//! tool `repo_map` exposta ao agent loop (MT-21) chegam nos micro-tickets
//! seguintes.

pub mod ast;
pub mod repo_map;
