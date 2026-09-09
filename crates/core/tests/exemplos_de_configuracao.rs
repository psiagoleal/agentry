// Caminho relativo: crates/core/tests/exemplos_de_configuracao.rs
//! Guarda dos exemplos de configuração versionados em `usage-test/exemplos/`.
//!
//! Os exemplos existem para serem **copiados**. Isso os coloca numa posição
//! incomum: são documentação que precisa executar. Duas coisas podem dar
//! errado em silêncio, e nenhuma delas é pega por revisão de PR:
//!
//! 1. **O exemplo para de carregar.** Um campo renomeado, uma chave que passou
//!    a ser obrigatória, e o exemplo vira uma armadilha — quem copiar recebe
//!    um erro de configuração e conclui que a ferramenta está quebrada. Um
//!    exemplo que não carrega é pior que exemplo nenhum, porque custa
//!    confiança além do tempo.
//! 2. **Um endereço real entra no repositório.** O caminho natural para o
//!    exemplo ficar "útil" é alguém colar ali o gateway que usa no dia a dia.
//!    Este repositório é aberto: endereço interno commitado é topologia
//!    publicada, e não se despublica. O `gitleaks` do CI pega credencial;
//!    **não** pega um `baseUrl` interno, que não parece segredo nenhum.

use agentry_core::config::{Config, Settings};
use std::path::{Path, PathBuf};

/// Hosts que um exemplo pode citar: *loopback* e o TLD reservado `.exemplo`
/// (contraparte de `.example`, RFC 2606 — existe justamente para nunca
/// resolver para nada real).
const HOSTS_PERMITIDOS: &[&str] = &["127.0.0.1", "localhost", "[::1]", ".exemplo"];

fn raiz_do_workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR deve ter dois ancestrais até a raiz")
        .to_path_buf()
}

fn exemplos() -> Vec<PathBuf> {
    let dir = raiz_do_workspace().join("usage-test/exemplos");
    let mut achados: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("usage-test/exemplos/ deve existir ({e})"))
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    achados.sort();
    achados
}

#[test]
fn todo_exemplo_versionado_carrega_como_configuracao_valida() {
    let exemplos = exemplos();
    // Guarda da guarda: uma varredura que não varreu nada passa vacuamente.
    assert!(
        exemplos.len() >= 4,
        "só {} exemplo(s) encontrado(s) — a pasta mudou de lugar ou de formato, e a guarda \
         deixou de cobrir o que deveria",
        exemplos.len()
    );

    for caminho in &exemplos {
        let json = std::fs::read_to_string(caminho).expect("exemplo deve ser legível");
        let camada = Settings::from_json_str(&json).unwrap_or_else(|erro| {
            panic!(
                "{} não carrega como configuração: {erro}\n\nUm exemplo que não carrega é pior \
                 que exemplo nenhum: quem copiar conclui que a ferramenta está quebrada.",
                caminho.display()
            )
        });
        // `resolve` é o que a CLI de fato roda; `from_json_str` sozinho não
        // exercita a validação que acontece na resolução das camadas.
        let _cfg = Config::resolve(vec![camada]);
    }
}

#[test]
fn nenhum_exemplo_aponta_para_um_host_real() {
    for caminho in &exemplos() {
        let json = std::fs::read_to_string(caminho).expect("exemplo deve ser legível");
        let valor: serde_json::Value =
            serde_json::from_str(&json).expect("exemplo deve ser JSON válido");

        for (campo, url) in urls_declaradas(&valor) {
            let permitido = HOSTS_PERMITIDOS.iter().any(|h| url.contains(h));
            assert!(
                permitido,
                "{}: {campo} aponta para {url:?}, que não é loopback nem espaço reservado.\n\n\
                 Endereço de serviço real não entra em exemplo versionado — este repositório é \
                 aberto, e endereço interno commitado é topologia publicada. Coloque o valor \
                 real em usage-test/local/, que não é versionado. Hosts aceitos aqui: {:?}",
                caminho.display(),
                HOSTS_PERMITIDOS
            );
        }
    }
}

/// Colhe todo par `(campo, valor)` cujo valor pareça uma URL, em qualquer
/// profundidade. Varre pelo **formato do valor**, não por uma lista de nomes
/// de campo: um campo novo que aceite URL passaria despercebido por uma lista
/// fixa, e é justamente o caso novo que ninguém lembra de conferir.
fn urls_declaradas(valor: &serde_json::Value) -> Vec<(String, String)> {
    let mut achados = Vec::new();
    colher(valor, "", &mut achados);
    achados
}

fn colher(valor: &serde_json::Value, caminho: &str, saida: &mut Vec<(String, String)>) {
    match valor {
        serde_json::Value::Object(mapa) => {
            for (chave, filho) in mapa {
                // Comentários explicam formato e podem citar URL de
                // documentação; não são configuração.
                if chave.starts_with('_') || chave == "$schema" {
                    continue;
                }
                let sub = if caminho.is_empty() {
                    chave.clone()
                } else {
                    format!("{caminho}.{chave}")
                };
                colher(filho, &sub, saida);
            }
        }
        serde_json::Value::Array(itens) => {
            for (i, item) in itens.iter().enumerate() {
                colher(item, &format!("{caminho}[{i}]"), saida);
            }
        }
        serde_json::Value::String(texto)
            if texto.starts_with("http://") || texto.starts_with("https://") =>
        {
            saida.push((caminho.to_string(), texto.clone()));
        }
        _ => {}
    }
}
