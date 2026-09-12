// Caminho relativo: crates/core/tests/fake_provider.rs
//! Teste de aceite do `fake_provider` (MT-142, ADR-0045) — a fixture que os
//! casos ponta a ponta vão usar precisa ela própria estar correta, senão um
//! caso vermelho não distingue bug do `agentry` de bug da fixture.
//!
//! Exercita o binário de verdade (`CARGO_BIN_EXE_fake_provider`), pelo mesmo
//! motivo de `mcp_client.rs`: a variável só existe para alvos de teste de
//! integração do pacote.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

/// Diretório temporário removido ao sair de escopo (mesma disciplina de
/// `config::tests::TempDir`/`state_dir::tests::TempDir`, MT-38 — o projeto não
/// tem a crate `tempfile` e não a introduz por causa de teste, ADR-0004).
struct TempDir(std::path::PathBuf);

impl TempDir {
    fn new() -> Self {
        let unico = format!(
            "agentry-fake-provider-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("relógio do sistema não deve estar antes de 1970")
                .as_nanos(),
            {
                // Unicidade construída, não observada: ver `hermetismo.rs`.
                static SEQUENCIA: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                SEQUENCIA.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            }
        );
        let path = std::env::temp_dir().join(unico);
        std::fs::create_dir_all(&path).expect("deve criar diretório temporário de teste");
        Self(path)
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Processo filho morto ao sair de escopo — sem isso, um teste que falha no
/// meio deixa o `fake_provider` rodando e segurando a porta.
struct Servidor {
    filho: Child,
    porta: u16,
}

impl Drop for Servidor {
    fn drop(&mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

/// Sobe o `fake_provider` com o roteiro dado e devolve a porta que ele anunciou.
fn subir(roteiro: &str, dir: &TempDir, registro: Option<&std::path::Path>) -> Servidor {
    let caminho_roteiro = dir.path().join("roteiro.json");
    std::fs::write(&caminho_roteiro, roteiro).expect("grava roteiro");

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_fake_provider"));
    cmd.arg(&caminho_roteiro);
    if let Some(registro) = registro {
        cmd.arg(registro);
    }
    let mut filho = cmd
        .stdout(Stdio::piped())
        .spawn()
        .expect("fake_provider deve iniciar");

    let stdout = filho.stdout.take().expect("stdout capturado");
    let mut linha = String::new();
    BufReader::new(stdout)
        .read_line(&mut linha)
        .expect("fake_provider deve anunciar a porta");
    let porta: u16 = linha
        .trim()
        .parse()
        .unwrap_or_else(|e| panic!("porta anunciada inválida {linha:?}: {e}"));

    Servidor { filho, porta }
}

#[tokio::test]
async fn responde_o_corpo_roteirizado_e_registra_o_que_recebeu() {
    let dir = TempDir::new();
    let registro = dir.path().join("registro.jsonl");
    let servidor = subir(
        r#"{"respostas":[{"corpo":{"choices":[{"message":{"role":"assistant","content":"Olá!"}}]}}]}"#,
        &dir,
        Some(&registro),
    );

    let resposta = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{}/chat/completions",
            servidor.porta
        ))
        .json(&serde_json::json!({ "model": "m", "messages": [] }))
        .send()
        .await
        .expect("requisição deve completar")
        .json::<serde_json::Value>()
        .await
        .expect("resposta deve ser JSON");

    assert_eq!(resposta["choices"][0]["message"]["content"], "Olá!");

    // O ponto do registro: afirmar o que o cliente **enviou**. Foi uma tool a
    // mais no pedido que passou despercebida pela suíte inteira (ADR-0045).
    let linhas = std::fs::read_to_string(&registro).expect("registro deve existir");
    let registrada: serde_json::Value =
        serde_json::from_str(linhas.lines().next().expect("uma linha registrada"))
            .expect("linha do registro deve ser JSON");
    assert_eq!(registrada["caminho"], "/chat/completions");
    assert_eq!(registrada["corpo"]["model"], "m");
}

#[tokio::test]
async fn responde_stream_sse_encerrado_por_done() {
    let dir = TempDir::new();
    let servidor = subir(
        r#"{"respostas":[{"sse":["{\"choices\":[{\"delta\":{\"content\":\"ol\"}}]}","{\"choices\":[{\"delta\":{\"content\":\"á\"}}]}"]}]}"#,
        &dir,
        None,
    );

    let corpo = reqwest::Client::new()
        .post(format!(
            "http://127.0.0.1:{}/chat/completions",
            servidor.porta
        ))
        .json(&serde_json::json!({ "model": "m", "messages": [], "stream": true }))
        .send()
        .await
        .expect("requisição deve completar")
        .text()
        .await
        .expect("corpo deve ser texto");

    assert!(corpo.contains(r#"data: {"choices":[{"delta":{"content":"ol"}}]}"#));
    assert!(corpo.contains(r#"data: {"choices":[{"delta":{"content":"á"}}]}"#));
    assert!(
        corpo.trim_end().ends_with("data: [DONE]"),
        "stream deve encerrar com [DONE], que o parser do provider trata \
         explicitamente; veio: {corpo:?}"
    );
}

#[tokio::test]
async fn roteiro_esgotado_falha_alto_em_vez_de_repetir_a_ultima() {
    let dir = TempDir::new();
    let servidor = subir(r#"{"respostas":[{"corpo":{"choices":[]}}]}"#, &dir, None);
    let url = format!("http://127.0.0.1:{}/chat/completions", servidor.porta);
    let cliente = reqwest::Client::new();

    let primeira = cliente
        .post(&url)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("primeira requisição");
    assert!(primeira.status().is_success());

    // A segunda não tem resposta roteirizada. Devolver de novo a última seria
    // o pior comportamento possível: o caso passaria por acidente.
    let segunda = cliente
        .post(&url)
        .json(&serde_json::json!({}))
        .send()
        .await
        .expect("segunda requisição");
    assert_eq!(segunda.status().as_u16(), 500);
    let corpo: serde_json::Value = segunda.json().await.expect("erro em JSON");
    assert!(
        corpo["error"]["message"]
            .as_str()
            .is_some_and(|m| m.contains("roteiro esgotado")),
        "mensagem de erro deve dizer o que houve; veio: {corpo}"
    );
}
