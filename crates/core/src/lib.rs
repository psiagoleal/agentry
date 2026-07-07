// Caminho relativo: crates/core/src/lib.rs
//! Núcleo do `agentry` — o motor de execução do CLI agêntico.
//!
//! Módulos implementados até aqui: [`model`] (tipos de domínio de mensagens/LLM,
//! MT-02), [`provider`] (`trait LlmProvider` + mock + adapter Ollama, MT-03/08),
//! [`config`] (configuração em camadas + classe de privacidade, MT-04),
//! [`egress`] (allowlist, audit log e redação de segredos, MT-05/06),
//! [`transport`] (transporte HTTP único sobre `reqwest`, MT-07), [`router`]
//! (Router / Policy Engine, MT-09), [`session`] (agent loop ReAct mínimo,
//! MT-10) e [`tools`] (Tool Registry + gate de permissão `allow`/`ask`/`deny`,
//! MT-11). Os demais — implementações reais de fs/shell, context manager —
//! entram nos micro-tickets seguintes do roadmap (`docs/roadmap-v0.1.md`).

pub mod config;
pub mod egress;
pub mod model;
pub mod provider;
pub mod router;
pub mod session;
pub mod tools;
pub mod transport;

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
