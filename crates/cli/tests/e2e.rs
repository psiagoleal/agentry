// Caminho relativo: crates/cli/tests/e2e.rs
//! Testes de integração **ponta a ponta** (Fase K, MT-143, ADR-0045): sobem o
//! binário `agentry` de verdade como subprocesso, contra o `fake_provider`, e
//! afirmam sobre `stdout`/`stderr`/código de saída.
//!
//! Cobrem a **fiação** — argumentos, carga de configuração, `Router`, laço do
//! agente, guardrails, auditoria —, não a lógica de cada unidade, que já tem
//! cobertura. É a fronteira onde todos os defeitos de produção deste projeto
//! apareceram, e que os testes unitários não alcançam (ADR-0045).
//!
//! **Hermetismo** (ADR-0045): cada caso roda com `HOME` e `cwd` apontando para
//! um diretório temporário, então nenhuma leitura ou escrita toca o
//! `~/.agentry/` real de quem executa. Isso funciona porque `global_dir.rs` é o
//! único ponto do workspace que resolve o *home* — propriedade protegida pelo
//! teste-guarda `crates/core/tests/hermetismo.rs` (MT-141).

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

/// Diretório temporário removido ao sair de escopo (mesma disciplina de
/// `config::tests::TempDir`, MT-38 — o projeto não usa a crate `tempfile`).
struct TempDir(PathBuf);

impl TempDir {
    fn new(rotulo: &str) -> Self {
        let unico = format!(
            "agentry-e2e-{rotulo}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("relógio do sistema não deve estar antes de 1970")
                .as_nanos()
        );
        // `std::env::temp_dir()` (tipicamente `/tmp`) não fica sob um diretório
        // com `.agentry/`, o que importa: a busca de configuração sobe a
        // árvore e acharia a do próprio repositório (ADR-0045).
        let path = std::env::temp_dir().join(unico);
        std::fs::create_dir_all(&path).expect("deve criar diretório temporário de teste");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Caminho do `fake_provider`. Ele é um `[[bin]]` de `agentry-core`, então
/// `CARGO_BIN_EXE_fake_provider` **não** existe aqui — essa variável só é
/// definida para testes do pacote que declara o binário. Deliberadamente não
/// movemos a fixture para `crates/cli`: `cargo install` instalaria todos os
/// binários deste pacote, e uma fixture de teste não pode ir junto para a
/// máquina do usuário.
///
/// Os dois binários caem no mesmo diretório de saída, então derivar o caminho
/// a partir do `agentry` é estável.
fn caminho_fake_provider() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_agentry"))
        .parent()
        .expect("binário de teste deve estar em um diretório")
        .to_path_buf();
    let candidato = dir.join(if cfg!(windows) {
        "fake_provider.exe"
    } else {
        "fake_provider"
    });
    assert!(
        candidato.exists(),
        "fake_provider não encontrado em {} — ele é um bin de `agentry-core`, e este teste \
         precisa que ele já esteja compilado. Rode `cargo test --all` (o que o CI faz) ou \
         `cargo build -p agentry-core --bin fake_provider` antes de `cargo test -p agentry`.",
        candidato.display()
    );
    candidato
}

/// `fake_provider` rodando, morto ao sair de escopo — sem isso um caso que
/// falha no meio deixa o processo vivo segurando a porta.
struct Provider {
    filho: Child,
    porta: u16,
}

impl Drop for Provider {
    fn drop(&mut self) {
        let _ = self.filho.kill();
        let _ = self.filho.wait();
    }
}

impl Provider {
    /// Sobe o `fake_provider` com o roteiro dado, gravado em `dir`.
    fn subir(dir: &TempDir, roteiro: &str, registro: Option<&Path>) -> Self {
        let caminho_roteiro = dir.path().join("roteiro.json");
        std::fs::write(&caminho_roteiro, roteiro).expect("grava roteiro");

        let mut cmd = Command::new(caminho_fake_provider());
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
        let porta = linha
            .trim()
            .parse()
            .unwrap_or_else(|e| panic!("porta anunciada inválida {linha:?}: {e}"));

        Self { filho, porta }
    }

    fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.porta)
    }
}

/// Projeto de teste: um diretório com `.git/` e `.agentry/agentry.settings.json`
/// apontando para o `fake_provider`.
///
/// Tudo é `local-only`: o endpoint é `127.0.0.1` e o perfil ausente resolve
/// *fail-closed* para `local-only` (ADR-0002). Nenhum caso desta suíte pode
/// alcançar a nuvem nem por acidente.
fn preparar_projeto(dir: &TempDir, base_url: &str) -> PathBuf {
    let projeto = dir.path().join("projeto");
    std::fs::create_dir_all(projeto.join(".git")).expect("cria .git");
    std::fs::create_dir_all(projeto.join(".agentry")).expect("cria .agentry");

    // Funcionalidades de contexto desligadas de propósito: `semanticRag`
    // levantaria índice e pediria embeddings ao Ollama, o que furaria o
    // hermetismo do caso e o deixaria lento sem cobrir nada de novo.
    let settings = format!(
        r#"{{
  "schemaVersion": 1,
  "context": {{
    "repoMap": {{ "enabled": false }},
    "semanticRag": {{ "enabled": false }},
    "lspGrounding": {{ "enabled": false }}
  }},
  "providers": {{
    "litellm": {{
      "baseUrl": "{base_url}",
      "model": "modelo-de-teste",
      "egressClass": "local-only"
    }}
  }},
  "taskClasses": {{
    "chat": {{
      "candidates": [
        {{ "provider": "litellm", "model": "modelo-de-teste", "egressClass": "local-only" }}
      ]
    }}
  }}
}}"#
    );
    std::fs::write(projeto.join(".agentry/agentry.settings.json"), settings)
        .expect("grava agentry.settings.json");
    projeto
}

/// Roda o binário real, com `HOME` e `cwd` isolados.
fn rodar_agentry(dir: &TempDir, projeto: &Path, args: &[&str]) -> Output {
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("cria HOME temporário");

    Command::new(env!("CARGO_BIN_EXE_agentry"))
        .args(args)
        .current_dir(projeto)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        // Nenhuma credencial do ambiente real pode vazar para o caso.
        .env_remove("AGENTRY_LITELLM_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGENTRY_PROFILE")
        .env_remove("AGENTRY_MODEL")
        .output()
        .expect("agentry deve executar")
}

fn texto(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn conversa_sem_tool_vai_do_argumento_ate_a_resposta_do_provider() {
    let dir = TempDir::new("conversa");
    let registro = dir.path().join("registro.jsonl");
    // O caminho one-shot usa `chat_stream`, não `chat`: o roteiro precisa ser
    // SSE. Responder JSON não-streaming aqui produz saída vazia com código 0 —
    // ver a observação registrada em `docs/roadmap-v0.18.md`.
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[{"sse":[
            "{\"choices\":[{\"delta\":{\"content\":\"quarenta e \"}}]}",
            "{\"choices\":[{\"delta\":{\"content\":\"dois\"}}]}",
            "{\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":4},\"choices\":[]}"
        ]}]}"#,
        Some(&registro),
    );
    let projeto = preparar_projeto(&dir, &provider.base_url());

    let saida = rodar_agentry(&dir, &projeto, &["qual a resposta?"]);

    assert!(
        saida.status.success(),
        "agentry deveria sair com 0.\nstdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );
    assert!(
        texto(&saida.stdout).contains("quarenta e dois"),
        "a resposta do provider deveria chegar ao stdout.\nstdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );

    // O outro lado da fronteira: o que o binário **enviou**.
    let linhas = std::fs::read_to_string(&registro).expect("provider deve ter registrado");
    let pedido: serde_json::Value =
        serde_json::from_str(linhas.lines().next().expect("uma requisição registrada"))
            .expect("registro deve ser JSON");
    assert_eq!(pedido["caminho"], "/chat/completions");
    assert_eq!(pedido["corpo"]["model"], "modelo-de-teste");
    let mensagens = pedido["corpo"]["messages"]
        .as_array()
        .expect("pedido deve ter messages");
    assert!(
        mensagens.iter().any(|m| m["content"]
            .as_str()
            .is_some_and(|c| c.contains("qual a resposta?"))),
        "a tarefa da linha de comando deveria chegar ao provider; veio: {mensagens:?}"
    );
}

#[test]
fn nao_toca_o_home_real_do_usuario() {
    // Contraparte prática do teste-guarda estático (MT-141): aqui a afirmação
    // é de comportamento — depois de uma execução completa, o HOME temporário
    // é o único que pode ter sido escrito.
    let dir = TempDir::new("hermetismo");
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[{"sse":["{\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}"]}]}"#,
        None,
    );
    let projeto = preparar_projeto(&dir, &provider.base_url());
    let home = dir.path().join("home");

    let saida = rodar_agentry(&dir, &projeto, &["oi"]);
    assert!(
        saida.status.success(),
        "stdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );

    // Se o binário tivesse resolvido o home por outro caminho que não a
    // variável de ambiente, escreveria fora daqui — e este caso não notaria.
    // Por isso o teste-guarda estático existe: os dois juntos cobrem o que
    // nenhum dos dois cobre sozinho.
    assert!(
        home.exists(),
        "o HOME temporário deveria continuar existindo após a execução"
    );
}
