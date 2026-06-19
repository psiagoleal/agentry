// Caminho relativo: crates/cli/src/main.rs
//! Ponto de entrada da CLI `agentry`.
//!
//! Bootstrap (MT-01): imprime o banner de versão. O parsing de argumentos (clap), o
//! loop agêntico e o streaming entram nos micro-tickets seguintes do roadmap.

fn main() {
    println!("{}", agentry_core::banner());
}
