// Caminho relativo: crates/core/src/lib.rs
//! Núcleo do `agentry` — o motor de execução do CLI agêntico.
//!
//! Módulos implementados até aqui: [`model`] (tipos de domínio de mensagens/LLM,
//! MT-02), [`provider`] (`trait LlmProvider` + mock, MT-03) e [`config`]
//! (configuração em camadas + classe de privacidade, MT-04). Os demais —
//! transporte/egresso, router, tools, context manager — entram nos micro-tickets
//! seguintes do roadmap (`docs/roadmap-v0.1.md`).

pub mod config;
pub mod model;
pub mod provider;

/// Nome do produto.
pub const NAME: &str = "agentry";

/// Versão do crate, propagada do `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Retorna um banner curto com nome e versão (placeholder de bootstrap).
#[must_use]
pub fn banner() -> String {
    format!("{NAME} {VERSION}")
}

#[cfg(test)]
mod tests {
    use super::{banner, VERSION};

    #[test]
    fn banner_inclui_nome_e_versao() {
        let b = banner();
        assert!(b.starts_with("agentry "), "banner deve começar com o nome");
        assert!(b.contains(VERSION), "banner deve conter a versão");
    }
}
