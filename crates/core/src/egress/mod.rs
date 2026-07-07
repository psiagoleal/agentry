// Caminho relativo: crates/core/src/egress/mod.rs
//! Fronteira de egresso de rede (ADR-0002): decide, audita e higieniza todo
//! tráfego de saída do `agentry`.
//!
//! MT-05 traz apenas a [`allowlist`] — decisão em memória, sem I/O — sobre se
//! um destino é alcançável para a classe de egresso ativa. MT-06 acrescenta
//! audit log e redação de segredos; MT-07 integra tudo no transporte único
//! sobre `reqwest`, o único ponto do crate autorizado a fazer rede.

pub mod allowlist;
