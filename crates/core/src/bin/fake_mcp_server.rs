// Caminho relativo: crates/core/src/bin/fake_mcp_server.rs
//! Fixture de teste (MT-78, ADR-0028): um servidor MCP mínimo, só o
//! suficiente para os testes de ciclo de vida do `McpClient` (`start` →
//! `list_tools` → `shutdown`) exercitarem *handshake* + descoberta de
//! tools reais sobre `stdio`, sem depender de nenhum servidor MCP de
//! verdade instalado no ambiente de CI. Não é parte do produto — só um
//! binário auxiliar de teste, spawnado via
//! `env!("CARGO_BIN_EXE_fake_mcp_server")` a partir dos testes (mesmo
//! padrão do `fake_lsp_server`, MT-23).
//!
//! **Implementação própria e mínima do protocolo MCP** (JSON-RPC 2.0
//! delimitado por linha sobre `stdio` — confirmado no código-fonte do
//! próprio `rmcp`, `transport/async_rw.rs::JsonRpcMessageCodec`: é
//! *newline-delimited*, ao contrário do LSP, que usa cabeçalhos
//! `Content-Length`) em vez de habilitar a *feature* `server` do `rmcp` —
//! decisão registrada em `docs/decisoes-autonomas.md`: um alvo `[[bin]]`
//! de `crates/core` (como este, igual ao `fake_lsp_server`) só recebe as
//! *features* de `[dependencies]`, nunca as de `[dev-dependencies]`
//! (Cargo só estende `dev-dependencies` para `tests`/`examples`, não para
//! `[[bin]]`) — habilitar `server` exigiria torná-la parte da dependência
//! de **produção**, o que a própria ADR-0028 proíbe. Responder
//! manualmente usando os tipos de `rmcp::model` (módulo sem *feature
//! gate* — disponível só com `client`, já a dependência de produção) é
//! mais simples e não exige nenhuma mudança de escopo de dependência.
//!
//! Responde `initialize`/`tools/list`/`tools/call` com sucesso trivial
//! (fixo, independente dos parâmetros pedidos), ignora a notificação
//! `notifications/initialized` e encerra ao `stdin` fechar — mesma técnica
//! do `fake_lsp_server`, só com o formato de mensagem do MCP em vez do
//! LSP. Qualquer outro **método com `id`** (uma requisição de verdade,
//! diferente de uma notificação) recebe um erro JSON-RPC `-32601` ("Method
//! not found") em vez de ser ignorado em silêncio — descoberto durante o
//! MT-79: um método sem resposta nenhuma deixa o cliente `rmcp` real
//! esperando para sempre (sem *timeout* próprio), travando o teste em vez
//! de falhar de forma tratada.

use std::io::{BufRead, Write};

use rmcp::model::{
    CallToolResult, ContentBlock, InitializeResult, JsonObject, ListToolsResult,
    ServerCapabilities, Tool, ToolsCapability,
};

/// `ServerCapabilities::builder()` exige a *feature* `server`/`macros` do
/// `rmcp` (ver doc do módulo) — construída à mão via `Default` + acesso
/// direto ao campo `tools` (público mesmo com a *struct* `#[non_exhaustive]`
/// — essa marcação só proíbe sintaxe de literal `Struct { .. }` fora da
/// *crate*, não acesso/mutação de campo já `pub`).
fn capacidades_com_tools() -> ServerCapabilities {
    let mut capacidades = ServerCapabilities::default();
    capacidades.tools = Some(ToolsCapability::default());
    capacidades
}

fn tool_ping() -> Tool {
    Tool::new(
        "ping",
        "Devolve 'pong' — tool de teste do fake_mcp_server",
        JsonObject::new(),
    )
}

fn escreve_resposta(saida: &mut impl Write, id: serde_json::Value, resultado: serde_json::Value) {
    let envelope = serde_json::json!({ "jsonrpc": "2.0", "id": id, "result": resultado });
    let _ = writeln!(saida, "{envelope}");
    let _ = saida.flush();
}

fn escreve_erro_metodo_desconhecido(saida: &mut impl Write, id: serde_json::Value, metodo: &str) {
    let envelope = serde_json::json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": -32601, "message": format!("Method not found: {metodo}") }
    });
    let _ = writeln!(saida, "{envelope}");
    let _ = saida.flush();
}

fn main() {
    let entrada = std::io::stdin();
    let mut saida = std::io::stdout();

    for linha in entrada.lock().lines() {
        let Ok(linha) = linha else { break };
        if linha.trim().is_empty() {
            continue;
        }
        let Ok(mensagem) = serde_json::from_str::<serde_json::Value>(&linha) else {
            continue;
        };
        let Some(metodo) = mensagem.get("method").and_then(|m| m.as_str()) else {
            continue;
        };
        let Some(id) = mensagem.get("id").cloned() else {
            continue; // notificação (sem id): nada a responder
        };

        match metodo {
            "initialize" => {
                let resultado = InitializeResult::new(capacidades_com_tools());
                escreve_resposta(
                    &mut saida,
                    id,
                    serde_json::to_value(resultado).unwrap_or_default(),
                );
            }
            "tools/list" => {
                let resultado = ListToolsResult::with_all_items(vec![tool_ping()]);
                escreve_resposta(
                    &mut saida,
                    id,
                    serde_json::to_value(resultado).unwrap_or_default(),
                );
            }
            "tools/call" => {
                let resultado = CallToolResult::success(vec![ContentBlock::text("pong")]);
                escreve_resposta(
                    &mut saida,
                    id,
                    serde_json::to_value(resultado).unwrap_or_default(),
                );
            }
            outro => escreve_erro_metodo_desconhecido(&mut saida, id, outro),
        }
    }
}
