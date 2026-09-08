// Caminho relativo: crates/core/tests/hermetismo.rs
//! Teste-guarda do hermetismo dos testes ponta a ponta (MT-141, ADR-0045).
//!
//! O isolamento da Fase K não vem de uma variável de ambiente própria do
//! projeto: vem de `crates/core/src/global_dir.rs` ser o **único** ponto do
//! workspace que resolve o diretório *home*. Definir `HOME` no processo filho
//! redireciona `~/.agentry/` por inteiro justamente porque não há um segundo
//! resolvedor.
//!
//! Essa propriedade é frágil de um jeito perigoso: um `dirs::home_dir()`
//! introduzido em qualquer módulo continuaria compilando, e os testes ponta a
//! ponta continuariam **verdes** — só que escrevendo no `~/.agentry/` real da
//! máquina de quem rodou. Falha silenciosa não é detectável por teste de
//! comportamento; por isso a guarda é estática, sobre o código-fonte.
//!
//! Não muta `std::env`: a suíte roda em paralelo no mesmo processo, e variável
//! de ambiente é global ao processo — a mesma razão pela qual `global_dir`
//! expõe um núcleo testável que recebe a busca de variável como parâmetro.

use std::path::{Path, PathBuf};

/// Chamadas que resolvem o *home* do usuário. São procuradas como **chamada de
/// API**, não como texto solto: `credentials.rs` menciona `$HOME` e
/// `%USERPROFILE%` em documentação e numa mensagem de erro, o que é legítimo e
/// não pode acusar.
///
/// **Limitação conhecida:** a varredura é textual e só descarta linhas de
/// comentário, então um destes padrões dentro de uma *string literal* em linha
/// de código acusaria sem haver violação. Hoje não acontece em nenhum arquivo
/// do workspace, e escrever um parser de Rust para fechar essa fresta custaria
/// muito mais do que o falso positivo — que, se surgir, aparece como falha
/// nomeando arquivo e linha, e não em silêncio.
const RESOLVEDORES_PROIBIDOS: &[&str] = &[
    "var_os(\"HOME\")",
    "var(\"HOME\")",
    "var_os(\"USERPROFILE\")",
    "var(\"USERPROFILE\")",
    "dirs::home_dir",
    "directories::",
    "home::home_dir",
];

/// Único arquivo autorizado a resolver o *home* (ADR-0045).
const ARQUIVO_AUTORIZADO: &str = "global_dir.rs";

fn raiz_do_workspace() -> PathBuf {
    // CARGO_MANIFEST_DIR é `<raiz>/crates/core`.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("CARGO_MANIFEST_DIR deve ter dois ancestrais até a raiz")
        .to_path_buf()
}

/// Coleta todo `.rs` sob `crates/*/src/`, recursivamente.
fn fontes_do_workspace() -> Vec<PathBuf> {
    let mut encontrados = Vec::new();
    let crates = raiz_do_workspace().join("crates");
    let pacotes = std::fs::read_dir(&crates).expect("crates/ deve existir");
    for pacote in pacotes.flatten() {
        colher_rs(&pacote.path().join("src"), &mut encontrados);
    }
    encontrados
}

fn colher_rs(dir: &Path, saida: &mut Vec<PathBuf>) {
    let Ok(entradas) = std::fs::read_dir(dir) else {
        return;
    };
    for entrada in entradas.flatten() {
        let caminho = entrada.path();
        if caminho.is_dir() {
            colher_rs(&caminho, saida);
        } else if caminho.extension().is_some_and(|e| e == "rs") {
            saida.push(caminho);
        }
    }
}

/// Linha de comentário — `//`, `///` e `//!`. Documentação pode falar de
/// `$HOME` à vontade; o que a guarda proíbe é **chamar** o resolvedor.
fn e_comentario(linha: &str) -> bool {
    linha.trim_start().starts_with("//")
}

#[test]
fn home_e_resolvido_unicamente_no_global_dir() {
    let fontes = fontes_do_workspace();

    // Guarda da guarda: um teste de varredura que não varreu nada passa
    // vacuamente e dá falsa confiança. Se a estrutura de diretórios mudar,
    // este teste tem de falhar, não silenciar.
    assert!(
        fontes.len() > 20,
        "varredura encontrou só {} arquivos .rs — estrutura do workspace mudou e a guarda \
         deixou de cobrir o que deveria",
        fontes.len()
    );

    let mut violacoes = Vec::new();
    for caminho in &fontes {
        if caminho.file_name().is_some_and(|n| n == ARQUIVO_AUTORIZADO) {
            continue;
        }
        let conteudo = std::fs::read_to_string(caminho).expect("fonte deve ser legível");
        for (n, linha) in conteudo.lines().enumerate() {
            if e_comentario(linha) {
                continue;
            }
            for padrao in RESOLVEDORES_PROIBIDOS {
                if linha.contains(padrao) {
                    violacoes.push(format!("{}:{} — `{padrao}`", caminho.display(), n + 1));
                }
            }
        }
    }

    assert!(
        violacoes.is_empty(),
        "o diretório home só pode ser resolvido em {ARQUIVO_AUTORIZADO} (ADR-0045): mover essa \
         resolução para outro módulo quebra o isolamento dos testes ponta a ponta em silêncio \
         — eles seguiriam verdes escrevendo no ~/.agentry/ real. Violações:\n  {}",
        violacoes.join("\n  ")
    );
}

#[test]
fn o_arquivo_autorizado_de_fato_resolve_o_home() {
    // Contraparte do teste acima: se `global_dir.rs` for renomeado ou perder a
    // resolução, a varredura passaria a não achar nada e o teste anterior
    // continuaria verde sem estar guardando coisa alguma.
    let caminho = raiz_do_workspace()
        .join("crates/core/src")
        .join(ARQUIVO_AUTORIZADO);
    let conteudo = std::fs::read_to_string(&caminho).unwrap_or_else(|e| {
        panic!(
            "{ARQUIVO_AUTORIZADO} deve existir em {} ({e}) — se foi movido, a guarda de \
             hermetismo precisa ser atualizada junto",
            caminho.display()
        )
    });
    // O resolvedor é indireto de propósito: `home_dir_de` recebe a busca de
    // variável como parâmetro para os testes não precisarem mutar `std::env`.
    // Logo, não existe a string `var_os("HOME")` no arquivo — o que existe é a
    // chamada genérica e os nomes das variáveis. Afirmar sobre a forma errada
    // faria esta guarda falhar sem que nada estivesse quebrado.
    assert!(
        conteudo.contains("std::env::var_os"),
        "{ARQUIVO_AUTORIZADO} deixou de ler variável de ambiente para resolver o home — o \
         isolamento dos testes ponta a ponta depende disso (ADR-0045)"
    );
    for variavel in ["\"HOME\"", "\"USERPROFILE\""] {
        assert!(
            conteudo.contains(variavel),
            "{ARQUIVO_AUTORIZADO} não menciona mais {variavel}; o redirecionamento usado pelos \
             testes ponta a ponta deixaria de funcionar (ADR-0045)"
        );
    }
}
