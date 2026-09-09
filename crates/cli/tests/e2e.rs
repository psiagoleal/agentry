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
    preparar_projeto_com(dir, base_url, "local-only", "local-only", "[]")
}

/// Variante com as classes de egresso do **endpoint** e do **candidato de
/// rota** separadas, mais as regras de guardrail de entrada (MT-145).
///
/// As duas classes existem separadas de propósito: o *fail-closed* tem duas
/// camadas, e elas se comportam de forma diferente. Candidato mais permissivo
/// que a sessão faz o `Router` recusar a rota **antes** de qualquer chamada;
/// candidato permitido com endpoint mais permissivo faz o `Transport` barrar,
/// e é só nesse caso que existe entrada de auditoria.
fn preparar_projeto_com(
    dir: &TempDir,
    base_url: &str,
    egress_class: &str,
    classe_candidato: &str,
    guardrails_input: &str,
) -> PathBuf {
    let projeto = dir.path().join("projeto");
    std::fs::create_dir_all(projeto.join(".git")).expect("cria .git");
    std::fs::create_dir_all(projeto.join(".agentry")).expect("cria .agentry");

    // **Todas** as funcionalidades de contexto desligadas, e não só as caras:
    // `semanticRag`/`sessionSearch` levantariam índice e pediriam embeddings ao
    // Ollama, furando o hermetismo. Desligar o conjunto inteiro também torna o
    // set de tools anunciado determinístico — se dependesse dos *defaults*
    // (`semanticRag` e `sessionSearch` são `true` quando ausentes), a asserção
    // do conjunto quebraria por mudança de default, não por regressão real.
    let settings = format!(
        r#"{{
  "schemaVersion": 1,
  "context": {{
    "repoMap": {{ "enabled": false }},
    "semanticRag": {{ "enabled": false }},
    "lspGrounding": {{ "enabled": false }},
    "sessionSearch": {{ "enabled": false }}
  }},
  "providers": {{
    "litellm": {{
      "baseUrl": "{base_url}",
      "model": "modelo-de-teste",
      "egressClass": "{egress_class}"
    }}
  }},
  "guardrails": {{ "input": {guardrails_input}, "output": [] }},
  "taskClasses": {{
    "chat": {{
      "candidates": [
        {{ "provider": "litellm", "model": "modelo-de-teste", "egressClass": "{classe_candidato}" }}
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

// ---------------------------------------------------------------------------
// MT-144 — tool-calling ponta a ponta
// ---------------------------------------------------------------------------

/// Nomes das tools anunciadas ao modelo na primeira requisição.
fn tools_anunciadas(registro: &Path) -> Vec<String> {
    let linhas = std::fs::read_to_string(registro).expect("provider deve ter registrado");
    let pedido: serde_json::Value =
        serde_json::from_str(linhas.lines().next().expect("uma requisição registrada"))
            .expect("registro deve ser JSON");
    let mut nomes: Vec<String> = pedido["corpo"]["tools"]
        .as_array()
        .map(|ts| {
            ts.iter()
                .filter_map(|t| t["function"]["name"].as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default();
    nomes.sort();
    nomes
}

#[test]
fn tool_calling_executa_a_tool_e_devolve_o_resultado_ao_modelo() {
    let dir = TempDir::new("toolcall");
    let registro = dir.path().join("registro.jsonl");
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[
            {"sse":[
                "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"fs_read\",\"arguments\":\"{\\\"path\\\":\\\"alvo.txt\\\"}\"}}]}}]}"
            ]},
            {"sse":[
                "{\"choices\":[{\"delta\":{\"content\":\"o arquivo diz batata\"}}]}"
            ]}
        ]}"#,
        Some(&registro),
    );
    let projeto = preparar_projeto(&dir, &provider.base_url());
    std::fs::write(projeto.join("alvo.txt"), "batata").expect("cria alvo.txt");

    let saida = rodar_agentry(&dir, &projeto, &["leia o alvo.txt"]);

    assert!(
        saida.status.success(),
        "stdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );
    assert!(
        texto(&saida.stdout).contains("o arquivo diz batata"),
        "a resposta final deveria chegar ao stdout.\nstdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );

    // O ponto do caso: a tool foi mesmo **executada** e o resultado voltou ao
    // modelo. Sem isso, o bug do Ollama (tool-call devolvida como texto, nada
    // executando) passaria de novo — foi o defeito mais grave já encontrado.
    let linhas = std::fs::read_to_string(&registro).expect("registro deve existir");
    assert_eq!(
        linhas.lines().count(),
        2,
        "deveria haver duas idas ao provider: a que pediu a tool e a que recebeu o resultado"
    );
    let segunda: serde_json::Value =
        serde_json::from_str(linhas.lines().nth(1).expect("segunda requisição"))
            .expect("segunda requisição deve ser JSON");
    let mensagens = segunda["corpo"]["messages"]
        .as_array()
        .expect("segunda requisição deve ter messages");
    assert!(
        mensagens
            .iter()
            .any(|m| m["content"].as_str().is_some_and(|c| c.contains("batata"))),
        "o conteúdo lido pela tool deveria voltar ao modelo; veio: {mensagens:?}"
    );
}

#[test]
fn conjunto_de_tools_anunciado_ao_modelo_e_exatamente_o_esperado() {
    let dir = TempDir::new("tools");
    let registro = dir.path().join("registro.jsonl");
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[{"sse":["{\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}"]}]}"#,
        Some(&registro),
    );
    let projeto = preparar_projeto(&dir, &provider.base_url());

    let saida = rodar_agentry(&dir, &projeto, &["oi"]);
    assert!(
        saida.status.success(),
        "stdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );

    // Afirmação sobre o conjunto **inteiro**, não sobre presença individual: o
    // defeito real foi uma tool a **mais** no pedido (`shell_background`), que
    // nenhuma asserção de presença teria pego (ADR-0045).
    let esperado = vec![
        "ask_user",
        "fs_edit",
        "fs_read",
        "fs_search",
        "fs_write",
        "glob",
        "shell_background",
        "shell_exec",
        "skill",
        "subagent",
        "todo_write",
    ];
    assert_eq!(
        tools_anunciadas(&registro),
        esperado,
        "o conjunto de tools mudou; se foi de propósito, atualize esta lista — ela existe \
         justamente para uma tool nova não entrar no pedido sem ninguém notar"
    );
}

// ---------------------------------------------------------------------------
// MT-145 — egresso, auditoria e guardrail de PII
// ---------------------------------------------------------------------------

/// Requisições que chegaram ao provider. Arquivo ausente ⇒ nenhuma chegou,
/// que é a afirmação central dos casos de bloqueio.
fn requisicoes(registro: &Path) -> Vec<serde_json::Value> {
    std::fs::read_to_string(registro)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).expect("linha do registro deve ser JSON"))
        .collect()
}

fn audit_log(projeto: &Path) -> String {
    std::fs::read_to_string(projeto.join(".agentry").join("audit.log")).unwrap_or_default()
}

#[test]
fn audit_log_registra_destino_e_classe_de_egresso() {
    let dir = TempDir::new("audit");
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[{"sse":["{\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}"]}]}"#,
        None,
    );
    let projeto = preparar_projeto(&dir, &provider.base_url());

    let saida = rodar_agentry(&dir, &projeto, &["oi"]);
    assert!(
        saida.status.success(),
        "stdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );

    // ADR-0037: a trilha é persistente, não só uma linha no terminal — quem
    // audita lê o arquivo depois do fato.
    let log = audit_log(&projeto);
    assert!(
        log.contains(&provider.base_url()),
        "audit.log deveria registrar o destino; veio: {log}"
    );
    assert!(
        log.contains("local-only"),
        "audit.log deveria registrar a classe de egresso resolvida; veio: {log}"
    );
}

#[test]
fn rota_de_nuvem_e_recusada_sob_perfil_local_only() {
    let dir = TempDir::new("failclosed-rota");
    let registro = dir.path().join("registro.jsonl");
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[{"sse":["{\"choices\":[{\"delta\":{\"content\":\"nao deveria chegar\"}}]}"]}]}"#,
        Some(&registro),
    );
    // Candidato de rota declarado `cloud-ok`; o perfil ausente resolve
    // *fail-closed* para `local-only` (ADR-0002).
    let projeto = preparar_projeto_com(&dir, &provider.base_url(), "cloud-ok", "cloud-ok", "[]");

    let saida = rodar_agentry(&dir, &projeto, &["oi"]);

    assert!(
        requisicoes(&registro).is_empty(),
        "nenhuma requisição deveria ter alcançado o provider.\nstderr: {}",
        texto(&saida.stderr)
    );
    // Amarra o caso ao mecanismo: sem isto, ele passaria por qualquer motivo
    // que impedisse a chamada, inclusive um erro de configuração alheio.
    assert_eq!(
        saida.status.code(),
        Some(1),
        "recusa de rota deve sair com código 1.\nstderr: {}",
        texto(&saida.stderr)
    );
    assert!(
        texto(&saida.stderr).contains("nenhuma rota disponível"),
        "a recusa deveria ser explícita ao usuário; veio: {}",
        texto(&saida.stderr)
    );
}

#[test]
fn transport_barra_endpoint_mais_permissivo_que_a_sessao_e_audita() {
    let dir = TempDir::new("failclosed-transport");
    let registro = dir.path().join("registro.jsonl");
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[{"sse":["{\"choices\":[{\"delta\":{\"content\":\"nao deveria chegar\"}}]}"]}]}"#,
        Some(&registro),
    );
    // Candidato `local-only` (a rota é aceita) mas endpoint declarado
    // `cloud-ok`: a barreira cai no `Transport`, que é a camada que audita.
    let projeto = preparar_projeto_com(&dir, &provider.base_url(), "cloud-ok", "local-only", "[]");

    let saida = rodar_agentry(&dir, &projeto, &["oi"]);

    assert!(
        requisicoes(&registro).is_empty(),
        "o pacote não deveria ter saído.\nstdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );
    let log = audit_log(&projeto);
    assert!(
        log.contains("blocked"),
        "o bloqueio no Transport deve deixar trilha persistente (ADR-0002/0037); veio: {log:?}\nstderr: {}",
        texto(&saida.stderr)
    );
}

#[test]
fn cpf_no_prompt_e_bloqueado_e_o_log_nao_guarda_o_valor() {
    const CPF: &str = "529.982.247-25";
    let dir = TempDir::new("pii");
    let registro = dir.path().join("registro.jsonl");
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[{"sse":["{\"choices\":[{\"delta\":{\"content\":\"nao deveria chegar\"}}]}"]}]}"#,
        Some(&registro),
    );
    let projeto = preparar_projeto_com(
        &dir,
        &provider.base_url(),
        "local-only",
        "local-only",
        r#"[{ "id": "bloqueia-cpf", "detect": "cpf", "action": "block" }]"#,
    );

    let saida = rodar_agentry(&dir, &projeto, &[&format!("meu documento é {CPF}")]);

    assert!(
        requisicoes(&registro).is_empty(),
        "o CPF não deveria ter saído da máquina.\nstdout: {}\nstderr: {}",
        texto(&saida.stdout),
        texto(&saida.stderr)
    );

    // O ponto da ADR-0043: o log torna o vazamento detectável **sem** virar
    // ele próprio um repositório de dado sensível. Afirmar a ausência do valor
    // é tão importante quanto a presença do nome do detector — e é a metade
    // que uma asserção ingênua esqueceria.
    let log = audit_log(&projeto);
    assert!(
        log.contains("cpf"),
        "audit.log deveria nomear o detector; veio: {log}"
    );
    for forma in [CPF, "52998224725"] {
        assert!(
            !log.contains(forma),
            "audit.log NUNCA pode conter o valor detectado ({forma}); veio: {log}"
        );
    }
    assert!(
        !texto(&saida.stdout).contains("nao deveria chegar"),
        "stdout: {}",
        texto(&saida.stdout)
    );

    // Mesma disciplina do caso de egresso: amarrar ao mecanismo. O id da regra
    // é o que aparece no lugar do valor casado (ADR-0043).
    let relato = format!("{}{}", texto(&saida.stdout), texto(&saida.stderr));
    assert!(
        relato.contains("bloqueia-cpf"),
        "o bloqueio deveria ser reportado pelo id da regra; veio: {relato}"
    );
}

/// Acrescenta um bloco `mcpServers` ao `agentry.settings.json` já gravado por
/// [`preparar_projeto`] — usado só pelo caso de diagnóstico abaixo.
fn declarar_servidor_mcp(projeto: &Path, nome: &str, comando: &str) {
    let caminho = projeto.join(".agentry/agentry.settings.json");
    let bruto = std::fs::read_to_string(&caminho).expect("settings deve existir");
    let mut settings: serde_json::Value =
        serde_json::from_str(&bruto).expect("settings deve ser JSON válido");
    settings["mcpServers"] = serde_json::json!({
        nome: { "command": comando, "args": [], "egressClass": "local-only" }
    });
    std::fs::write(
        &caminho,
        serde_json::to_string_pretty(&settings).expect("serializa settings"),
    )
    .expect("regrava settings");
}

/// MT-152. O aviso de servidor MCP que não conectou deixou de ser um
/// `eprintln!` no ponto de origem e passou por um coletor
/// (`DiagnosticosDeInicializacao`) para poder aparecer **dentro** da TUI.
/// O risco dessa mudança é exatamente o oposto do problema que ela resolve:
/// um coletor que nunca é escoado engole o aviso em todos os modos, e a
/// sessão segue em silêncio como se o servidor tivesse conectado.
///
/// Este caso prende a ponta de texto (a única observável de fora: a TUI
/// exige TTY). O binário roda de verdade, com um comando que não existe no
/// `PATH`, e o aviso tem de estar em `stderr` — sem derrubar a execução,
/// que é a outra metade do contrato.
#[test]
fn servidor_mcp_que_nao_conecta_avisa_em_stderr_sem_derrubar_a_sessao() {
    let dir = TempDir::new("mcp-diagnostico");
    let provider = Provider::subir(
        &dir,
        r#"{"respostas":[{"sse":[
            "{\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}"
        ]}]}"#,
        None,
    );
    let projeto = preparar_projeto(&dir, &provider.base_url());
    declarar_servidor_mcp(&projeto, "inexistente", "agentry-comando-que-nao-existe");

    let saida = rodar_agentry(&dir, &projeto, &["oi"]);
    let stderr = texto(&saida.stderr);

    assert!(
        stderr.contains("erro ao conectar ao servidor MCP 'inexistente'"),
        "o aviso de MCP sumiu de stderr — o coletor do MT-152 precisa ser escoado nos modos \
         de texto, senão a falha de conexão vira silêncio. stderr: {stderr}"
    );
    assert!(
        saida.status.success(),
        "servidor MCP indisponível não pode derrubar a sessão; status: {:?}, stderr: {stderr}",
        saida.status.code()
    );
}

/// MT-157. A ponte MCP do `claudeCli` (ADR-0042) escreve
/// `agentry-mcp-<pid>.json` no diretório temporário e conta com o `Drop` de
/// `PonteMcp` para removê-lo. Enquanto cada caminho de erro de `main` chamava
/// `std::process::exit`, esse `Drop` **não rodava** — `exit` evapora o
/// processo com o desempilhamento pela metade — e o arquivo sobrava a cada
/// execução malsucedida. Achado real: cinco arquivos órfãos em `/tmp`, todos
/// de PIDs mortos.
///
/// O caso é hermético de duas formas que valem registro:
///
/// - **não depende do `claude` de verdade.** `build_claude_cli_provider` só
///   checa se o binário existe no `PATH`; um *script* vazio com esse nome
///   basta para a ponte ser montada, e é o que mantém o caso rodando em CI.
/// - **não depende de `/tmp`.** `TMPDIR` aponta para o diretório do caso, o
///   que permite afirmar sobre o **conteúdo inteiro** do diretório em vez de
///   caçar um padrão de nome numa pasta compartilhada com o resto da máquina.
///
/// A execução é forçada a falhar (rota de nuvem sob `local-only`) porque é
/// exatamente o caminho que vazava; um caso feliz já passava antes da
/// correção e não teria detectado nada.
#[test]
fn saida_por_erro_nao_deixa_a_config_temporaria_da_ponte_mcp_para_tras() {
    let dir = TempDir::new("ponte-mcp");
    let provider = Provider::subir(&dir, r#"{"respostas":[]}"#, None);
    // Candidato `cloud-ok` sob sessão `local-only`: o `Router` recusa a rota
    // e `executar` termina com código 1.
    let projeto = preparar_projeto_com(&dir, &provider.base_url(), "local-only", "cloud-ok", "[]");
    declarar_claude_cli_com_ponte(&projeto);

    let tmp = dir.path().join("tmp");
    std::fs::create_dir_all(&tmp).expect("cria TMPDIR do caso");
    let falso_path = criar_claude_falso(&dir);

    let home = dir.path().join("home");
    std::fs::create_dir_all(&home).expect("cria HOME temporário");
    let saida = Command::new(env!("CARGO_BIN_EXE_agentry"))
        .arg("oi")
        .current_dir(&projeto)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("TMPDIR", &tmp)
        .env("PATH", falso_path)
        .env_remove("AGENTRY_LITELLM_API_KEY")
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("AGENTRY_PROFILE")
        .env_remove("AGENTRY_MODEL")
        .output()
        .expect("agentry deve executar");

    // Sem isto o caso passaria por vacuidade: se a execução tivesse sido
    // bem-sucedida, o `Drop` rodaria mesmo antes da correção.
    assert!(
        !saida.status.success(),
        "o caso precisa exercitar um caminho de ERRO; saída: {:?}, stderr: {}",
        saida.status.code(),
        texto(&saida.stderr)
    );

    let restantes: Vec<String> = std::fs::read_dir(&tmp)
        .expect("TMPDIR deve existir")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        restantes.is_empty(),
        "saída por erro deixou lixo no diretório temporário: {restantes:?} — `Drop` só limpa \
         se o processo desempilhar, então nenhum caminho pode chamar std::process::exit \
         (MT-157)"
    );
}

/// Acrescenta `providers.claudeCli` com `mcpTools` ligado — é a combinação
/// que faz `main` montar a ponte e, portanto, escrever o arquivo temporário.
fn declarar_claude_cli_com_ponte(projeto: &Path) {
    let caminho = projeto.join(".agentry/agentry.settings.json");
    let bruto = std::fs::read_to_string(&caminho).expect("settings deve existir");
    let mut settings: serde_json::Value =
        serde_json::from_str(&bruto).expect("settings deve ser JSON válido");
    settings["providers"]["claudeCli"] = serde_json::json!({
        "model": "opus",
        "mcpTools": true
    });
    std::fs::write(
        &caminho,
        serde_json::to_string_pretty(&settings).expect("serializa settings"),
    )
    .expect("regrava settings");
}

/// Cria um `claude` inerte e devolve o `PATH` que o encontra. `agentry` só
/// verifica a **presença** do binário para registrar o provider; ele não é
/// executado neste caso, que falha antes de qualquer chamada ao modelo.
fn criar_claude_falso(dir: &TempDir) -> String {
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("cria bin/ do caso");
    let falso = bin.join("claude");
    std::fs::write(&falso, "#!/bin/sh\nexit 0\n").expect("escreve claude falso");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&falso, std::fs::Permissions::from_mode(0o755))
            .expect("torna executável");
    }
    bin.to_string_lossy().into_owned()
}
