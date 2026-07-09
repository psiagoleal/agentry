// Caminho relativo: crates/core/src/context/lsp/mod.rs
//! *Grounding* via LSP (ADR-0013): reduz alucinação de assinatura/tipo
//! consultando um *Language Server* já instalado no ambiente do usuário —
//! o `agentry` não empacota nem instala nenhum.
//!
//! [`client`] traz o cliente mínimo (spawn + ciclo de vida, MT-23); a tool
//! `lsp_hover`/`lsp_definition` exposta ao agent loop chega no MT-24.

pub mod client;
