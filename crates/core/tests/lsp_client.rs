// Caminho relativo: crates/core/tests/lsp_client.rs
//! Teste de integração do ciclo de vida completo do `LspClient` (MT-23,
//! ADR-0013) — precisa ser um teste de *integração* (não `#[cfg(test)]`
//! dentro de `--lib`) porque `CARGO_BIN_EXE_fake_lsp_server` só é definida
//! pelo Cargo para alvos de teste de integração do pacote, não para testes
//! unitários embutidos na lib.

use agentry_core::context::lsp::client::{LspClient, LspError};

/// Caminho do binário de teste `fake_lsp_server` (`crates/core/src/bin/fake_lsp_server.rs`).
const FAKE_LSP_SERVER: &str = env!("CARGO_BIN_EXE_fake_lsp_server");

fn processo_existe(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // `kill -0` só testa existência do processo, não mata nada;
        // stderr silenciado — "No such process" é o resultado esperado no
        // caso comum deste teste (processo já encerrado).
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
    #[cfg(not(unix))]
    {
        // Verificação best-effort fora do Unix; o teste de ciclo de vida
        // completo (start → initialize → shutdown) já cobre o caminho
        // principal em todas as plataformas via `Child::wait`.
        let _ = pid;
        false
    }
}

#[test]
fn ciclo_de_vida_completo_start_initialize_shutdown() {
    let mut client = LspClient::start(FAKE_LSP_SERVER, &[]).expect("fake_lsp_server deve iniciar");
    let pid = client.pid();

    // Chegar até aqui sem erro já prova o handshake completo: a resposta
    // do fake_lsp_server desserializou como `InitializeResult` válido e a
    // notificação `initialized` foi enviada com sucesso.
    let _resultado = client
        .initialize(None)
        .expect("initialize deve completar com sucesso");

    client
        .shutdown()
        .expect("shutdown deve completar com sucesso");

    assert!(
        !processo_existe(pid),
        "processo do language server não deveria existir após shutdown"
    );
}

#[test]
fn drop_sem_shutdown_explicito_nao_deixa_processo_orfao() {
    let pid;
    {
        let client = LspClient::start(FAKE_LSP_SERVER, &[]).expect("fake_lsp_server deve iniciar");
        pid = client.pid();
        // `client` sai de escopo aqui sem `shutdown()` explícito.
    }

    assert!(
        !processo_existe(pid),
        "Drop deveria ter encerrado o processo mesmo sem shutdown() explícito"
    );
}

#[test]
fn start_com_comando_inexistente_e_erro_tratado() {
    let erro = LspClient::start("este-comando-nao-existe-agentry-teste", &[])
        .expect_err("comando inexistente deve falhar ao spawnar, não travar");
    assert!(matches!(erro, LspError::Spawn(_)));
}
