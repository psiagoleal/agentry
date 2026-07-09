// Caminho relativo: crates/core/tests/lsp_tools.rs
//! Teste de integração do round-trip de sucesso da tool `lsp_hover` (MT-24,
//! ADR-0013) contra o `fake_lsp_server` de teste (MT-23) — precisa ser um
//! teste de *integração* (não `#[cfg(test)]` dentro de `--lib`) porque
//! `CARGO_BIN_EXE_fake_lsp_server` só é definida pelo Cargo para alvos de
//! teste de integração do pacote.

use std::sync::Arc;

use agentry_core::tools::lsp::{LspHoverTool, LspSession};
use agentry_core::tools::Tool;

/// Caminho do binário de teste `fake_lsp_server` (`crates/core/src/bin/fake_lsp_server.rs`).
const FAKE_LSP_SERVER: &str = env!("CARGO_BIN_EXE_fake_lsp_server");

#[tokio::test]
async fn lsp_hover_consulta_o_language_server_com_sucesso() {
    let session = Arc::new(LspSession::new(
        FAKE_LSP_SERVER,
        Vec::new(),
        std::env::temp_dir(),
    ));
    let tool = LspHoverTool::new(session);

    let saida = tool
        .execute(serde_json::json!({ "path": "a.rs", "line": 0, "character": 0 }))
        .await;

    assert!(!saida.is_error, "saída: {}", saida.content);
    assert!(saida.content.contains("fake hover"));
}
