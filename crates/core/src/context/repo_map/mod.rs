// Caminho relativo: crates/core/src/context/repo_map/mod.rs
//! Repo map estilo Aider (Fase 6, ADR-0010): grafo de referências ([`graph`],
//! MT-19), ranking de relevância estilo PageRank ([`rank`], MT-20) e, no
//! micro-ticket seguinte, a tool `repo_map` exposta ao agent loop (MT-21).

pub mod graph;
pub mod rank;
