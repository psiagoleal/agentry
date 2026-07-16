// Caminho relativo: crates/core/src/tools/mcp.rs
//! Tool MCP (Fase 16/ADR-0028, MT-79): expõe uma tool descoberta por
//! [`crate::mcp::McpClient`] (MT-78) como [`Tool`] (MT-11) — sob o mesmo
//! `ToolRegistry`/`PermissionGate` de qualquer outra tool, nenhum
//! mecanismo paralelo de confirmação/bloqueio.
//!
//! O nome de **registro** (`ToolRegistry`/`PermissionGate`, e o que o
//! modelo vê) é sempre prefixado pelo nome do servidor
//! (`"<servidor>__<tool>"`) para nunca colidir entre dois servidores que
//! exponham uma tool de mesmo nome — a chamada de verdade ao servidor
//! (`Peer::call_tool`) usa o nome **original**, sem prefixo, que o
//! servidor de fato conhece.

use std::sync::Arc;

use rmcp::model::{CallToolRequestParams, CallToolResult, ContentBlock};

use crate::mcp::McpClient;
use crate::provider::BoxFuture;
use crate::tools::{Tool, ToolOutput};

/// Separador entre nome de servidor e nome de tool no nome de registro
/// (`"<servidor>__<tool>"`, ADR-0028) — única constante que conhece o
/// formato exato do prefixo, para nunca haver duas convenções divergentes.
pub const SERVER_TOOL_SEPARATOR: &str = "__";

/// Monta o nome de registro de uma tool MCP a partir do nome do servidor e
/// do nome original da tool.
#[must_use]
pub fn nome_registrado(servidor: &str, tool: &str) -> String {
    format!("{servidor}{SERVER_TOOL_SEPARATOR}{tool}")
}

/// Adapta uma tool MCP descoberta (`McpClient::list_tools`) como [`Tool`]
/// do `ToolRegistry`.
pub struct McpTool {
    cliente: Arc<McpClient>,
    nome_registrado: String,
    /// Nome como o servidor MCP o conhece (sem o prefixo de servidor) —
    /// usado só na chamada de verdade (`Peer::call_tool`), nunca exposto
    /// como nome de registro.
    nome_original: String,
    descricao: String,
    input_schema: serde_json::Value,
}

impl McpTool {
    /// Cria a tool a partir de uma [`rmcp::model::Tool`] descoberta em
    /// `nome_servidor` — o nome de registro (`nome_registrado`) já sai
    /// prefixado.
    #[must_use]
    pub fn new(cliente: Arc<McpClient>, nome_servidor: &str, tool: &rmcp::model::Tool) -> Self {
        Self {
            cliente,
            nome_registrado: nome_registrado(nome_servidor, &tool.name),
            nome_original: tool.name.to_string(),
            descricao: tool.description.as_deref().unwrap_or_default().to_string(),
            input_schema: serde_json::Value::Object((*tool.input_schema).clone()),
        }
    }
}

impl Tool for McpTool {
    fn name(&self) -> &str {
        &self.nome_registrado
    }

    fn description(&self) -> &str {
        &self.descricao
    }

    fn input_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let params = match arguments.as_object().cloned() {
                Some(objeto) if !objeto.is_empty() => {
                    CallToolRequestParams::new(self.nome_original.clone()).with_arguments(objeto)
                }
                _ => CallToolRequestParams::new(self.nome_original.clone()),
            };
            match self.cliente.call_tool(params).await {
                Ok(resultado) => converte_resultado(resultado),
                Err(erro) => ToolOutput::error(erro.to_string()),
            }
        })
    }
}

/// Converte um [`CallToolResult`] do `rmcp` para o [`ToolOutput`] genérico
/// do `agentry` — concatena os blocos de texto (`ContentBlock::Text`);
/// blocos de imagem/áudio/recurso ficam fora de escopo desta ticket
/// (`ToolOutput` só representa texto hoje, mesma limitação de toda outra
/// tool do projeto).
fn converte_resultado(resultado: CallToolResult) -> ToolOutput {
    let texto = resultado
        .content
        .iter()
        .filter_map(|bloco| match bloco {
            ContentBlock::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    if resultado.is_error.unwrap_or(false) {
        ToolOutput::error(texto)
    } else {
        ToolOutput::ok(texto)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rmcp::model::TextContent;

    // O ciclo de vida completo com um cliente MCP real (start → registro
    // no ToolRegistry sob o gate de permissão → execute() de ponta a
    // ponta) mora em `crates/core/tests/mcp_tool.rs`: precisa do
    // `fake_mcp_server` (`CARGO_BIN_EXE_fake_mcp_server`, só definida para
    // testes de integração, mesmo motivo do `mcp_client.rs`).

    #[test]
    fn nome_registrado_prefixa_o_servidor_e_evita_colisao_entre_servidores() {
        let de_a = nome_registrado("servidor-a", "buscar");
        let de_b = nome_registrado("servidor-b", "buscar");

        assert_eq!(de_a, "servidor-a__buscar");
        assert_eq!(de_b, "servidor-b__buscar");
        assert_ne!(
            de_a, de_b,
            "mesma tool ('buscar') em servidores diferentes nunca deve colidir"
        );
    }

    #[test]
    fn converte_resultado_concatena_blocos_de_texto() {
        let resultado = CallToolResult::success(vec![
            ContentBlock::Text(TextContent::new("linha 1")),
            ContentBlock::Text(TextContent::new("linha 2")),
        ]);

        let saida = converte_resultado(resultado);

        assert!(!saida.is_error);
        assert_eq!(saida.content, "linha 1\nlinha 2");
    }

    #[test]
    fn converte_resultado_com_is_error_vira_tool_output_de_erro() {
        let resultado = CallToolResult::error(vec![ContentBlock::Text(TextContent::new(
            "algo deu errado",
        ))]);

        let saida = converte_resultado(resultado);

        assert!(saida.is_error);
        assert_eq!(saida.content, "algo deu errado");
    }
}
