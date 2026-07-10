// Caminho relativo: crates/core/src/context/rag/mod.rs
//! RAG semântico local para código (Fase 6, ADR-0011): chunking AST-aware
//! ([`chunk`], MT-25), índice lexical BM25 ([`lexical_index`], MT-26),
//! índice semântico via embeddings ([`semantic_index`], MT-27) e busca
//! híbrida com *reranking* ([`hybrid_search`], MT-28) — complementa (não
//! substitui) o repo-map do ADR-0010.

use crate::context::ast::SymbolKind;

pub mod chunk;
pub mod hybrid_search;
pub mod lexical_index;
pub mod semantic_index;

/// Serialização textual de [`SymbolKind`] — reaproveitada pelos dois
/// índices ([`lexical_index`] e [`semantic_index`]) para armazenar/
/// reconstruir o campo `kind` do [`chunk::Chunk`] em formatos que só
/// aceitam campos escalares (documento do `tantivy`, coluna Arrow do
/// `lancedb`). `SymbolKind` não ganha `Display`/`serde` só para isto — a
/// conversão fica local ao RAG, não no módulo `ast` (MT-18).
pub(super) fn kind_to_str(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Class => "class",
    }
}

pub(super) fn kind_from_str(kind: &str) -> SymbolKind {
    match kind {
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        _ => SymbolKind::Function,
    }
}
