// Caminho relativo: crates/core/tests/mcp_client.rs
//! Teste de integração do ciclo de vida completo do `McpClient` (MT-78,
//! ADR-0028) — precisa ser um teste de *integração* (não `#[cfg(test)]`
//! dentro de `--lib`) porque `CARGO_BIN_EXE_fake_mcp_server` só é definida
//! pelo Cargo para alvos de teste de integração do pacote, não para testes
//! unitários embutidos na lib (mesmo motivo do `lsp_client.rs`, MT-23).

use agentry_core::mcp::{McpClient, McpError};

/// Caminho do binário de teste `fake_mcp_server` (`crates/core/src/bin/fake_mcp_server.rs`).
const FAKE_MCP_SERVER: &str = env!("CARGO_BIN_EXE_fake_mcp_server");

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
        // completo (start → list_tools → shutdown) já cobre o caminho
        // principal em todas as plataformas.
        let _ = pid;
        false
    }
}

#[tokio::test]
async fn ciclo_de_vida_completo_start_list_tools_shutdown() {
    let client = McpClient::start(FAKE_MCP_SERVER, &[])
        .await
        .expect("fake_mcp_server deve iniciar");
    let pid = client.pid().expect("processo real deve ter PID");

    // Chegar até aqui sem erro já prova o handshake completo.
    let tools = client
        .list_tools()
        .await
        .expect("list_tools deve completar com sucesso");
    assert_eq!(tools.len(), 1, "fake_mcp_server expõe uma única tool");
    assert_eq!(tools[0].name, "ping");

    client
        .shutdown()
        .await
        .expect("shutdown deve completar com sucesso");

    // `shutdown`/`cancel` não espera o processo terminar de fato (só
    // fecha a sessão MCP); dá um instante para o `Drop` assíncrono do
    // `rmcp` (`ChildWithCleanup`) matar o subprocesso de verdade.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        !processo_existe(pid),
        "processo do servidor MCP não deveria existir após shutdown"
    );
}

#[tokio::test]
async fn drop_sem_shutdown_explicito_nao_deixa_processo_orfao() {
    let pid;
    {
        let client = McpClient::start(FAKE_MCP_SERVER, &[])
            .await
            .expect("fake_mcp_server deve iniciar");
        pid = client.pid().expect("processo real deve ter PID");
        // `client` sai de escopo aqui sem `shutdown()` explícito.
    }

    // O `Drop` do `TokioChildProcess` dentro do `rmcp` mata o subprocesso
    // de forma assíncrona (`tokio::spawn` dentro do próprio `drop`) — dá
    // um instante para essa task rodar antes de checar.
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    assert!(
        !processo_existe(pid),
        "Drop deveria ter encerrado o processo mesmo sem shutdown() explícito"
    );
}

#[tokio::test]
async fn start_com_comando_inexistente_e_erro_tratado() {
    let erro = McpClient::start("este-comando-nao-existe-agentry-teste", &[])
        .await
        .expect_err("comando inexistente deve falhar ao spawnar, não travar");
    assert!(matches!(erro, McpError::Spawn(_)));
}
