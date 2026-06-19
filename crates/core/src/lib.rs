// Caminho relativo: crates/core/src/lib.rs
//! Núcleo do `agentry` — o motor de execução do CLI agêntico.
//!
//! Nesta fase de bootstrap (MT-01) o crate expõe apenas um banner de versão. Os
//! módulos de domínio — providers, transporte/egresso, router, tools, context
//! manager — entram nos micro-tickets seguintes do roadmap (`docs/roadmap-v0.1.md`).

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
