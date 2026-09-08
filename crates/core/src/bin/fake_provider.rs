// Caminho relativo: crates/core/src/bin/fake_provider.rs
//! Fixture de teste (MT-142, ADR-0045): provider HTTP falso que fala o
//! protocolo OpenAI-compatible (`POST {base}/chat/completions`), com respostas
//! **roteirizadas** — a sequência vem de um arquivo JSON, não de um modelo.
//! Não é parte do produto; é spawnado via `env!("CARGO_BIN_EXE_fake_provider")`
//! a partir dos testes, no mesmo padrão de `fake_mcp_server`/`fake_lsp_server`.
//!
//! Existe porque teste ponta a ponta não pode depender de rede, credencial nem
//! cota de provider de terceiro (ADR-0045): a matriz de três SOs (ADR-0005)
//! multiplicaria por três qualquer indisponibilidade alheia, e teste que falha
//! por motivo externo ensina a ignorar CI vermelho.
//!
//! **Por que não o mock in-process que já existe.** Os testes unitários do
//! `openai_compat` sobem um servidor HTTP mínimo em `tokio` dentro do próprio
//! processo de teste (MT-07). Ele não serve aqui: o caso ponta a ponta roda o
//! `agentry` como **outro processo**, que só alcança o mock por uma porta real.
//! Os dois coexistem por terem alvos diferentes — unidade do provider ali,
//! fiação do binário aqui —, não por duplicação.
//!
//! **Servidor HTTP sobre `std::net::TcpListener`, com parsing mínimo.** Adotar
//! `axum`/`hyper` como dependência direta seria parada dura por ADR-0004, e um
//! servidor de teste não justifica isso. Vale também a restrição registrada no
//! cabeçalho do `fake_mcp_server`: um alvo `[[bin]]` de `crates/core` só recebe
//! as *features* de `[dependencies]`, nunca as de `[dev-dependencies]` — então
//! aqui só entra o que já é dependência de produção (`serde_json`) e `std`.
//!
//! # Uso
//!
//! ```text
//! fake_provider <roteiro.json> [registro.jsonl]
//! ```
//!
//! Escreve **uma linha em `stdout` com a porta** (atribuída pelo SO, nunca
//! fixa — porta fixa é intermitente em CI paralelo) e serve indefinidamente.
//! Com `registro.jsonl`, grava cada requisição recebida em JSON Lines, para o
//! teste poder afirmar **o que o `agentry` enviou**, e não só o que aceitou de
//! volta — foi exatamente uma tool a mais no pedido (`shell_background`) que
//! passou despercebida pela suíte inteira.
//!
//! # Roteiro
//!
//! ```json
//! {
//!   "respostas": [
//!     { "corpo": { "choices": [ ... ] } },
//!     { "sse": ["{\"choices\":[{\"delta\":{\"content\":\"olá\"}}]}"] }
//!   ]
//! }
//! ```
//!
//! Uma entrada por requisição, na ordem. Requisição além do roteiro devolve
//! **500 com mensagem explícita**: o teste tem de falhar alto, e não receber
//! silenciosamente a última resposta de novo.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde_json::Value;

/// Índice da próxima resposta do roteiro. O servidor atende uma conexão por
/// vez (determinismo importa mais que concorrência numa fixture), mas o
/// contador é atômico para o dia em que isso mudar.
static PROXIMA: AtomicUsize = AtomicUsize::new(0);

fn main() {
    let mut args = std::env::args().skip(1);
    let roteiro_path = args.next().unwrap_or_else(|| {
        eprintln!("uso: fake_provider <roteiro.json> [registro.jsonl]");
        std::process::exit(2);
    });
    let registro_path = args.next();

    let roteiro = carregar_roteiro(Path::new(&roteiro_path));

    let listener = TcpListener::bind("127.0.0.1:0").expect("bind em porta efêmera");
    let porta = listener.local_addr().expect("endereço local").port();

    // O teste lê esta linha para descobrir a porta; sem o flush, ela pode ficar
    // no buffer enquanto o teste espera, e o caso trava até o timeout.
    println!("{porta}");
    std::io::stdout().flush().expect("flush da porta");

    for conexao in listener.incoming() {
        match conexao {
            Ok(stream) => atender(stream, &roteiro, registro_path.as_deref()),
            Err(e) => eprintln!("fake_provider: conexão falhou: {e}"),
        }
    }
}

/// Uma resposta roteirizada: corpo JSON único ou sequência de eventos SSE.
enum Resposta {
    Json(String),
    Sse(Vec<String>),
}

fn carregar_roteiro(caminho: &Path) -> Vec<Resposta> {
    let bruto = std::fs::read_to_string(caminho)
        .unwrap_or_else(|e| panic!("roteiro {} ilegível: {e}", caminho.display()));
    let valor: Value = serde_json::from_str(&bruto)
        .unwrap_or_else(|e| panic!("roteiro {} não é JSON válido: {e}", caminho.display()));

    let entradas = valor
        .get("respostas")
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("roteiro deve ter a chave \"respostas\" com um array"));

    entradas
        .iter()
        .map(|entrada| {
            if let Some(eventos) = entrada.get("sse").and_then(Value::as_array) {
                Resposta::Sse(
                    eventos
                        .iter()
                        .map(|e| match e {
                            Value::String(s) => s.clone(),
                            outro => outro.to_string(),
                        })
                        .collect(),
                )
            } else if let Some(corpo) = entrada.get("corpo") {
                Resposta::Json(corpo.to_string())
            } else {
                panic!("entrada de roteiro precisa de \"corpo\" ou \"sse\": {entrada}")
            }
        })
        .collect()
}

fn atender(mut stream: TcpStream, roteiro: &[Resposta], registro: Option<&str>) {
    let Some((caminho, corpo)) = ler_requisicao(&mut stream) else {
        return;
    };

    if let Some(arquivo) = registro {
        registrar(arquivo, &caminho, &corpo);
    }

    let indice = PROXIMA.fetch_add(1, Ordering::SeqCst);
    match roteiro.get(indice) {
        Some(Resposta::Json(corpo)) => responder_json(&mut stream, corpo),
        Some(Resposta::Sse(eventos)) => responder_sse(&mut stream, eventos),
        None => responder_erro(
            &mut stream,
            &format!(
                "roteiro esgotado: chegou a requisição {} e o roteiro tem {} resposta(s)",
                indice + 1,
                roteiro.len()
            ),
        ),
    }
}

/// Lê a requisição HTTP: linha inicial, cabeçalhos até a linha em branco e
/// exatamente `Content-Length` bytes de corpo. Devolve `(caminho, corpo)`.
fn ler_requisicao(stream: &mut TcpStream) -> Option<(String, String)> {
    let mut leitor = BufReader::new(stream.try_clone().expect("clone do socket"));

    let mut inicial = String::new();
    if leitor.read_line(&mut inicial).ok()? == 0 {
        return None; // conexão fechada sem requisição
    }
    // "POST /chat/completions HTTP/1.1"
    let caminho = inicial.split_whitespace().nth(1).unwrap_or("/").to_string();

    let mut tamanho = 0usize;
    loop {
        let mut linha = String::new();
        if leitor.read_line(&mut linha).ok()? == 0 {
            break;
        }
        let limpa = linha.trim_end();
        if limpa.is_empty() {
            break;
        }
        if let Some(valor) = limpa
            .split_once(':')
            .filter(|(nome, _)| nome.eq_ignore_ascii_case("content-length"))
            .map(|(_, valor)| valor.trim())
        {
            tamanho = valor.parse().unwrap_or(0);
        }
    }

    let mut corpo = vec![0u8; tamanho];
    if tamanho > 0 {
        leitor.read_exact(&mut corpo).ok()?;
    }
    Some((caminho, String::from_utf8_lossy(&corpo).into_owned()))
}

/// Uma linha JSON por requisição — caminho e corpo já interpretado, para o
/// teste poder navegar o pedido sem reparsear string.
fn registrar(arquivo: &str, caminho: &str, corpo: &str) {
    let corpo_json: Value = serde_json::from_str(corpo).unwrap_or(Value::Null);
    let linha = serde_json::json!({ "caminho": caminho, "corpo": corpo_json });
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(arquivo)
        .unwrap_or_else(|e| panic!("registro {arquivo} não pôde ser aberto: {e}"));
    writeln!(f, "{linha}").expect("escrita no registro");
}

fn responder_json(stream: &mut TcpStream, corpo: &str) {
    let cabecalho = format!(
        "HTTP/1.1 200 OK\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        corpo.len()
    );
    let _ = stream.write_all(cabecalho.as_bytes());
    let _ = stream.write_all(corpo.as_bytes());
    let _ = stream.flush();
}

fn responder_sse(stream: &mut TcpStream, eventos: &[String]) {
    let cabecalho = "HTTP/1.1 200 OK\r\n\
         Content-Type: text/event-stream\r\n\
         Cache-Control: no-cache\r\n\
         Connection: close\r\n\r\n";
    let _ = stream.write_all(cabecalho.as_bytes());
    for evento in eventos {
        let _ = stream.write_all(format!("data: {evento}\n\n").as_bytes());
        let _ = stream.flush();
    }
    // O parser do provider trata `[DONE]` explicitamente
    // (`openai_compat.rs`); sem ele o stream termina por EOF, o que também
    // funciona, mas emitir é o que um provider real faz.
    let _ = stream.write_all(b"data: [DONE]\n\n");
    let _ = stream.flush();
}

fn responder_erro(stream: &mut TcpStream, mensagem: &str) {
    let corpo = serde_json::json!({ "error": { "message": mensagem } }).to_string();
    let cabecalho = format!(
        "HTTP/1.1 500 Internal Server Error\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\r\n",
        corpo.len()
    );
    let _ = stream.write_all(cabecalho.as_bytes());
    let _ = stream.write_all(corpo.as_bytes());
    let _ = stream.flush();
    eprintln!("fake_provider: {mensagem}");
}
