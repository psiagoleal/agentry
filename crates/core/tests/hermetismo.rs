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

/// Único ponto do workspace autorizado a encerrar o processo (MT-157).
const ENCERRAMENTO_AUTORIZADO: &str = "async fn main()";

#[test]
fn nenhum_modulo_fora_da_main_encerra_o_processo() {
    // MT-157: `std::process::exit` **não roda destrutores**. Enquanto cada
    // caminho de erro chamava `exit` direto, todo `Drop` responsável por
    // limpeza virava condicional ao caminho feliz — o caso concreto foi a
    // ponte MCP do `claudeCli`, que deixava `agentry-mcp-<pid>.json` no
    // diretório temporário a cada execução que terminasse em erro.
    //
    // Guarda estática, e não de comportamento, pela mesma razão do teste
    // acima: um `exit` novo introduzido em qualquer caminho de erro continua
    // compilando e deixa a suíte **verde** — só um teste que exercitasse
    // exatamente aquele caminho perceberia. Aqui a propriedade é do
    // código-fonte, então é verificada nele.
    //
    // A varredura libera o arquivo **inteiro** que contém a `main`; quem
    // garante que lá dentro existe uma saída só é
    // `a_main_autorizada_de_fato_encerra_o_processo`, logo abaixo. A divisão
    // é intencional: aqui a pergunta é "que arquivos podem encerrar", lá é
    // "quantas vezes".
    let fontes = fontes_do_workspace();
    assert!(fontes.len() > 20, "varredura vazia — ver o teste acima");

    let mut violacoes = Vec::new();
    for caminho in &fontes {
        let conteudo = std::fs::read_to_string(caminho).expect("fonte deve ser legível");
        // `fake_provider` é fixture de teste, não entra no binário instalado.
        if caminho.to_string_lossy().contains("bin/fake_provider.rs") {
            continue;
        }
        let autorizado = conteudo.contains(ENCERRAMENTO_AUTORIZADO);
        for (n, linha) in conteudo.lines().enumerate() {
            if e_comentario(linha) || !linha.contains("process::exit") {
                continue;
            }
            if autorizado {
                continue;
            }
            violacoes.push(format!("{}:{}", caminho.display(), n + 1));
        }
    }

    assert!(
        violacoes.is_empty(),
        "só a `main` da CLI pode encerrar o processo (MT-157): `std::process::exit` pula \
         destrutores, então um `exit` fora dela transforma qualquer limpeza por `Drop` em \
         limpeza condicional ao caminho feliz. Devolva um erro até a `main`. Violações:\n  {}",
        violacoes.join("\n  ")
    );
}

#[test]
fn a_main_autorizada_de_fato_encerra_o_processo() {
    // Contraparte: se a `main` deixar de concentrar a saída (ou o marcador
    // mudar), a varredura acima passaria a não achar nada e ficaria verde
    // sem estar guardando coisa alguma.
    let caminho = raiz_do_workspace().join("crates/cli/src/main.rs");
    let conteudo = std::fs::read_to_string(&caminho).expect("main.rs deve existir");
    assert!(
        conteudo.contains(ENCERRAMENTO_AUTORIZADO),
        "o marcador {ENCERRAMENTO_AUTORIZADO:?} sumiu de main.rs — a guarda do MT-157 precisa \
         ser atualizada junto"
    );
    // Só linhas de código: a documentação do MT-157 cita `std::process::exit`
    // várias vezes justamente para explicar por que ele é proibido.
    let chamadas: Vec<&str> = conteudo
        .lines()
        .filter(|l| !e_comentario(l) && l.contains("std::process::exit"))
        .collect();
    assert_eq!(
        chamadas.len(),
        1,
        "a saída do processo precisa continuar num ponto só, depois de `executar` ter \
         devolvido e derrubado tudo que criou (MT-157); achadas: {chamadas:?}"
    );
}

#[test]
fn so_a_montagem_de_config_le_o_ambiente_do_processo() {
    // Irmão da guarda de `HOME` acima, e da mesma família de falha: um teste
    // que lê o ambiente **real** fica verde enquanto a máquina de quem roda
    // estiver limpa, e passa a falhar — ou pior, a passar por motivo errado —
    // no dia em que alguém exportar a variável no `.zshrc`.
    //
    // Aconteceu de verdade em 2026-09-10: dois testes de precedência de
    // camada de configuração quebraram quando `AGENTRY_MODEL` foi exportada,
    // porque a camada de ambiente é a última de `Config::resolve` e vencia o
    // que o próprio teste tinha acabado de declarar. Eles não estavam
    // verificando precedência; estavam verificando o `.zshrc` do mantenedor.
    //
    // A correção é injeção: quem monta a configuração lê o ambiente **uma
    // vez**, na fronteira, e passa a camada adiante. Esta guarda mantém essa
    // fronteira em um lugar só.
    let fontes = fontes_do_workspace();
    assert!(fontes.len() > 20, "varredura vazia — ver o primeiro teste");

    let mut violacoes = Vec::new();
    for caminho in &fontes {
        let conteudo = std::fs::read_to_string(caminho).expect("fonte deve ser legível");
        // `config/mod.rs` **define** `from_process_env`; `main.rs` é o único
        // que pode chamá-la, e só na montagem da configuração.
        let e_definicao = caminho.to_string_lossy().contains("config/mod.rs");
        let e_montagem = conteudo.contains("fn build_config_com_camadas");
        if e_definicao || e_montagem {
            continue;
        }
        for (n, linha) in conteudo.lines().enumerate() {
            if !e_comentario(linha) && linha.contains("from_process_env") {
                violacoes.push(format!("{}:{}", caminho.display(), n + 1));
            }
        }
    }

    assert!(
        violacoes.is_empty(),
        "só a montagem da configuração pode ler o ambiente do processo: `from_process_env` \
         espalhado torna o comportamento dependente do shell de quem roda, e o teste que \
         depender dele fica verde por acidente. Receba a camada por parâmetro. Violações:\n  {}",
        violacoes.join("\n  ")
    );
}

#[test]
fn a_presenca_de_binario_externo_nao_e_sondada_fora_da_montagem_da_cli() {
    // Terceiro membro da mesma família (`HOME`, `AGENTRY_*`, e agora `PATH`):
    // código que pergunta ao sistema "isto está instalado?" torna o resultado
    // do teste dependente da estação de trabalho de quem roda.
    //
    // Aconteceu no primeiro CI da vida do projeto (2026-09-12): três testes
    // de montagem do provider `claude-cli` reprovaram nos **três** SOs da
    // matriz, porque o runner não tem `claude` no `PATH`. Eles estavam
    // verdes havia meses só porque toda máquina de desenvolvimento do
    // projeto tem o Claude Code instalado — não testavam a montagem do
    // provider, testavam a máquina.
    //
    // A correção é a de sempre: sondar na fronteira e injetar o resultado.
    let fontes = fontes_do_workspace();
    assert!(fontes.len() > 20, "varredura vazia — ver o primeiro teste");

    let mut violacoes = Vec::new();
    for caminho in &fontes {
        let conteudo = std::fs::read_to_string(caminho).expect("fonte deve ser legível");
        // `main.rs` **define** a sondagem e a usa uma vez, na montagem.
        if conteudo.contains("fn binario_no_path") {
            continue;
        }
        for (n, linha) in conteudo.lines().enumerate() {
            if !e_comentario(linha) && linha.contains("binario_no_path") {
                violacoes.push(format!("{}:{}", caminho.display(), n + 1));
            }
        }
    }

    assert!(
        violacoes.is_empty(),
        "sondar o `PATH` fora da montagem da CLI amarra o comportamento à máquina de quem \
         roda. Injete o resultado por parâmetro. Violações:\n  {}",
        violacoes.join("\n  ")
    );
}
