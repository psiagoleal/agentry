// Caminho relativo: crates/core/tests/mcp_tool.rs
//! Teste de integração da tool MCP (`McpTool`, MT-79, ADR-0028) — cliente
//! MCP real (`fake_mcp_server`) + `ToolRegistry`/`PermissionGate` reais.
//! Precisa ser teste de integração pelo mesmo motivo de `mcp_client.rs`:
//! `CARGO_BIN_EXE_fake_mcp_server` só é definida para alvos de teste de
//! integração do pacote, não para testes unitários dentro de `--lib`.

use std::sync::Arc;

use agentry_core::config::Permissions;
use agentry_core::mcp::McpClient;
use agentry_core::model::ToolCall;
use agentry_core::tools::mcp::McpTool;
use agentry_core::tools::permission::PermissionGate;
use agentry_core::tools::{ExecutionOutcome, Tool, ToolRegistry};

const FAKE_MCP_SERVER: &str = env!("CARGO_BIN_EXE_fake_mcp_server");

/// Conecta ao `fake_mcp_server` de verdade, descobre a tool `ping` e monta
/// a `McpTool` correspondente, registrada sob `nome_servidor`.
async fn cliente_e_tool_ping(nome_servidor: &str) -> (Arc<McpClient>, McpTool) {
    let cliente = Arc::new(
        McpClient::start(FAKE_MCP_SERVER, &[])
            .await
            .expect("fake_mcp_server deve iniciar"),
    );
    let tools = cliente
        .list_tools()
        .await
        .expect("list_tools deve completar com sucesso");
    let tool_bruta = tools
        .into_iter()
        .find(|t| t.name == "ping")
        .expect("fake_mcp_server expõe a tool 'ping'");
    let mcp_tool = McpTool::new(Arc::clone(&cliente), nome_servidor, &tool_bruta);
    (cliente, mcp_tool)
}

#[tokio::test]
async fn tool_registrada_aparece_em_specs_com_nome_prefixado() {
    let (_cliente, mcp_tool) = cliente_e_tool_ping("meu-servidor").await;
    let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
    registry.register(Arc::new(mcp_tool));

    let nomes: Vec<String> = registry.specs().into_iter().map(|s| s.name).collect();

    assert!(nomes.contains(&"meu-servidor__ping".to_string()));
}

#[tokio::test]
async fn execucao_via_registry_chega_ao_servidor_real_e_devolve_pong() {
    let (_cliente, mcp_tool) = cliente_e_tool_ping("meu-servidor").await;
    let nome = mcp_tool.name().to_string();
    let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
    registry.register(Arc::new(mcp_tool));

    let call = ToolCall {
        id: "1".into(),
        name: nome,
        arguments: serde_json::json!({}),
    };
    let outcome = registry.execute(&call).await;

    let ExecutionOutcome::Executed(resultado) = outcome else {
        panic!("esperava ExecutionOutcome::Executed, veio {outcome:?}");
    };
    assert!(!resultado.is_error);
    assert_eq!(resultado.content, "pong");
}

#[tokio::test]
async fn tool_mcp_respeita_deny_do_permission_gate_como_qualquer_outra() {
    let (_cliente, mcp_tool) = cliente_e_tool_ping("meu-servidor").await;
    let nome = mcp_tool.name().to_string();
    let mut permissions = Permissions::default();
    permissions.deny.push(nome.clone());
    let mut registry = ToolRegistry::new(PermissionGate::new(permissions));
    registry.register(Arc::new(mcp_tool));

    let call = ToolCall {
        id: "1".into(),
        name: nome,
        arguments: serde_json::json!({}),
    };
    let outcome = registry.execute(&call).await;

    assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
}

#[tokio::test]
async fn tool_mcp_sob_ask_sinaliza_sem_executar() {
    let (_cliente, mcp_tool) = cliente_e_tool_ping("meu-servidor").await;
    let nome = mcp_tool.name().to_string();
    let mut permissions = Permissions::default();
    permissions.ask.push(nome.clone());
    let mut registry = ToolRegistry::new(PermissionGate::new(permissions));
    registry.register(Arc::new(mcp_tool));

    let call = ToolCall {
        id: "1".into(),
        name: nome,
        arguments: serde_json::json!({}),
    };
    let outcome = registry.execute(&call).await;

    assert!(matches!(outcome, ExecutionOutcome::NeedsConfirmation(_)));
}

#[tokio::test]
async fn duas_tools_de_mesmo_nome_em_servidores_diferentes_nao_colidem_no_registro() {
    let (_cliente_a, tool_a) = cliente_e_tool_ping("servidor-a").await;
    let (_cliente_b, tool_b) = cliente_e_tool_ping("servidor-b").await;
    let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
    registry.register(Arc::new(tool_a));
    registry.register(Arc::new(tool_b));

    let nomes: Vec<String> = registry.specs().into_iter().map(|s| s.name).collect();

    assert!(nomes.contains(&"servidor-a__ping".to_string()));
    assert!(nomes.contains(&"servidor-b__ping".to_string()));
    assert_eq!(
        nomes.len(),
        2,
        "as duas tools devem coexistir, nenhuma sobrescreve a outra"
    );
}
