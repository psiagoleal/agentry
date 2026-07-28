// Caminho relativo: crates/cli/src/mcp_server.rs
//! Modo servidor MCP (`agentry --mcp-server`, ADR-0042): expõe o
//! [`ToolRegistry`] já montado pela configuração do projeto como um servidor
//! MCP falando JSON-RPC sobre *stdio*.
//!
//! **Para que serve.** É o transporte que faltava para a assinatura Claude
//! Pro/Max ser o agente principal com tool-calling real (ADR-0040 registrou a
//! ausência; ADR-0041 entregou a política). O `claude -p` roda com as
//! ferramentas embutidas desligadas (`--tools ""`) e recebe, via
//! `--mcp-config`/`--strict-mcp-config`, **apenas** as tools que este servidor
//! anuncia — então a sessão de nuvem tem exatamente o que o `agentry` expõe,
//! e nada mais.
//!
//! **A contenção é da política, não deste módulo.** Toda chamada passa por
//! `ToolRegistry::execute`, portanto pelo mesmo `PermissionGate` do modo
//! normal — incluindo `readAllow`/`subagentPermissions` (ADR-0041). Este
//! módulo nunca executa uma tool por fora do gate: é a diretriz de
//! conformidade da ADR-0042, e o que impede o servidor de virar porta lateral.
//!
//! **`ask` vira negação.** Um servidor MCP falando por *stdio* não tem como
//! pedir confirmação ao humano — o *stdio* já é o canal do protocolo. Uma tool
//! sob `ask` é recusada com explicação, nunca aprovada em silêncio (que seria
//! transformar `ask` em `allow` sem o usuário saber).
//!
//! **Stdout é do protocolo.** Nada além do JSON-RPC pode ser escrito lá; todo
//! diagnóstico vai para `stderr`, senão o handshake quebra.

use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, ContentBlock, ErrorData as McpError, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{RoleServer, ServiceExt};

use agentry_core::model::ToolCall;
use agentry_core::tools::{ExecutionOutcome, ToolRegistry};

/// Nome anunciado no handshake — aparece prefixando cada tool do lado do
/// cliente (`mcp__<servidor>__<tool>`), então mudar isto quebra qualquer
/// `--allowedTools` já configurado.
pub const NOME_DO_SERVIDOR: &str = "agentry";

/// Servidor MCP que empresta o `ToolRegistry` da configuração corrente.
pub struct ServidorDeTools {
    registry: Arc<ToolRegistry>,
}

impl ServidorDeTools {
    /// Cria o servidor sobre um registry já montado (com o `PermissionGate`
    /// que a configuração resolveu — ver a doc do módulo).
    #[must_use]
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }
}

/// Converte um `ToolSpec` do `agentry` no `Tool` do protocolo MCP.
///
/// O `input_schema` do MCP é um objeto JSON, não um `Value` qualquer — um
/// schema que não seja objeto é substituído por um objeto vazio em vez de
/// derrubar a listagem inteira por causa de uma tool malformada.
fn spec_para_tool(spec: &agentry_core::provider::ToolSpec) -> Tool {
    let schema = spec
        .input_schema
        .as_object()
        .cloned()
        .unwrap_or_else(serde_json::Map::new);
    Tool::new(
        spec.name.clone(),
        spec.description.clone(),
        Arc::new(schema),
    )
}

impl ServerHandler for ServidorDeTools {
    fn get_info(&self) -> ServerInfo {
        let mut info = ServerInfo::default();
        info.capabilities = ServerCapabilities::builder().enable_tools().build();
        info.server_info.name = NOME_DO_SERVIDOR.into();
        info.server_info.version = env!("CARGO_PKG_VERSION").into();
        info.instructions = Some(
            "Ferramentas do agentry, sujeitas à política de permissões do projeto \
             (permissions/readAllow). Leituras fora do escopo permitido são recusadas — \
             use a tool `subagent` para delegar essas leituras a um modelo local, que \
             devolve apenas o resultado."
                .into(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        // Tool negada por **nome** nunca vai rodar: anunciá-la só faria o
        // modelo gastar turnos (e tokens da assinatura) tentando usar algo que
        // sempre falha. Filtrar aqui é conveniência, não segurança — a decisão
        // real continua em `call_tool`, que reconsulta o gate.
        //
        // O filtro é só por nome: `readAllow` depende do caminho pedido, então
        // uma tool de arquivo continua listada e é decidida por chamada.
        let visiveis = self
            .registry
            .specs()
            .iter()
            .filter(|spec| self.registry.permite_pelo_nome(&spec.name))
            .map(spec_para_tool)
            .collect();
        Ok(ListToolsResult::with_all_items(visiveis))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, McpError> {
        let chamada = ToolCall {
            // O `id` só existe para correlacionar tool-call e resultado dentro
            // de uma `Session` do agentry; aqui quem correlaciona é o próprio
            // JSON-RPC, então um valor fixo basta e nada o observa.
            id: "mcp".to_string(),
            name: request.name.to_string(),
            arguments: request
                .arguments
                .map_or_else(|| serde_json::json!({}), serde_json::Value::Object),
        };

        let resultado = match self.registry.execute(&chamada).await {
            ExecutionOutcome::Executed(r) | ExecutionOutcome::Denied(r) => r,
            // Sem canal para perguntar ao humano: `ask` é recusado com o
            // motivo, nunca auto-aprovado (ver doc do módulo).
            ExecutionOutcome::NeedsConfirmation(pendente) => {
                return Ok(CallToolResult::error(vec![ContentBlock::text(format!(
                    "tool '{}' exige confirmação interativa e não pode ser executada por \
                     esta via (servidor MCP). Rode-a pelo `agentry` diretamente, ou mova-a \
                     de `permissions.ask` para `allow` se a aprovação prévia for aceitável.",
                    pendente.name
                ))]));
            }
        };

        let conteudo = vec![ContentBlock::text(resultado.content)];
        Ok(if resultado.is_error {
            CallToolResult::error(conteudo)
        } else {
            CallToolResult::success(conteudo)
        })
    }
}

/// Sobe o servidor sobre *stdio* e bloqueia até o cliente encerrar.
///
/// # Errors
///
/// Devolve erro se o handshake falhar ou o transporte cair de forma anormal —
/// nunca `panic`, e nunca em silêncio (diretriz de conformidade da ADR-0042).
pub async fn servir(registry: Arc<ToolRegistry>) -> Result<(), String> {
    let servico = ServidorDeTools::new(registry)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|erro| format!("falha ao iniciar o servidor MCP: {erro}"))?;

    servico
        .waiting()
        .await
        .map_err(|erro| format!("servidor MCP encerrou com erro: {erro}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::config::Permissions;
    use agentry_core::tools::permission::PermissionGate;

    fn registry_de_teste(permissions: Permissions, raiz: &std::path::Path) -> Arc<ToolRegistry> {
        let mut registry =
            ToolRegistry::new(PermissionGate::new(permissions).with_workspace_root(raiz));
        registry.register(Arc::new(agentry_core::tools::fs::FsReadTool::new(
            raiz, false,
        )));
        Arc::new(registry)
    }

    #[test]
    fn spec_vira_tool_do_protocolo_preservando_nome_descricao_e_schema() {
        let spec = agentry_core::provider::ToolSpec {
            name: "fs_read".into(),
            description: "Lê um arquivo".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": { "path": { "type": "string" } }
            }),
        };

        let tool = spec_para_tool(&spec);

        assert_eq!(tool.name, "fs_read");
        assert_eq!(tool.description.as_deref(), Some("Lê um arquivo"));
        assert_eq!(tool.input_schema.get("type").unwrap(), "object");
    }

    /// Schema malformado não pode derrubar a listagem inteira — o cliente
    /// ficaria sem nenhuma tool por causa de uma só.
    #[test]
    fn schema_que_nao_e_objeto_vira_objeto_vazio_em_vez_de_quebrar() {
        let spec = agentry_core::provider::ToolSpec {
            name: "estranha".into(),
            description: "".into(),
            input_schema: serde_json::json!("isto não é um objeto"),
        };

        assert!(spec_para_tool(&spec).input_schema.is_empty());
    }

    /// A invariante central da ADR-0042: a chamada via MCP passa pelo mesmo
    /// `PermissionGate` do modo normal, então `readAllow` continua valendo.
    #[tokio::test]
    async fn chamada_via_mcp_respeita_read_allow() {
        let dir = std::env::temp_dir().join(format!("agentry-mcp-teste-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("criar dir");
        std::fs::write(dir.join("README.md"), "publico").expect("escrever");
        std::fs::write(dir.join("segredo.csv"), "cpf,nome").expect("escrever");

        let registry = registry_de_teste(
            Permissions {
                deny: vec![],
                ask: vec![],
                read_allow: vec!["README.md".into()],
            },
            &dir,
        );
        let servidor = ServidorDeTools::new(registry);

        let chamar = |path: &str| ToolCall {
            id: "mcp".into(),
            name: "fs_read".into(),
            arguments: serde_json::json!({ "path": path }),
        };

        let permitido = servidor.registry.execute(&chamar("README.md")).await;
        assert!(
            matches!(permitido, ExecutionOutcome::Executed(ref r) if !r.is_error),
            "README.md está em readAllow e deve ser lido"
        );

        let negado = servidor.registry.execute(&chamar("segredo.csv")).await;
        assert!(
            matches!(negado, ExecutionOutcome::Denied(_)),
            "o servidor MCP não pode ser porta lateral em volta da ADR-0041"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `ask` sem canal para perguntar: recusa explicada, nunca auto-aprovação.
    #[tokio::test]
    async fn tool_sob_ask_e_recusada_com_explicacao() {
        let dir = std::env::temp_dir().join(format!("agentry-mcp-ask-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("criar dir");
        std::fs::write(dir.join("a.txt"), "x").expect("escrever");

        let registry = registry_de_teste(
            Permissions {
                deny: vec![],
                ask: vec!["fs_read".into()],
                read_allow: vec![],
            },
            &dir,
        );

        let resultado = ServidorDeTools::new(registry)
            .call_tool_direto("fs_read", serde_json::json!({ "path": "a.txt" }))
            .await;

        assert_eq!(resultado.is_error, Some(true));
        let texto = format!("{:?}", resultado.content);
        assert!(
            texto.contains("confirmação interativa"),
            "a recusa precisa dizer por que não rodou: {texto}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    impl ServidorDeTools {
        /// Atalho de teste para exercitar a lógica de [`ServerHandler::call_tool`]
        /// sem montar um `RequestContext` completo (que exige um transporte real).
        async fn call_tool_direto(
            &self,
            nome: &str,
            argumentos: serde_json::Value,
        ) -> CallToolResult {
            let chamada = ToolCall {
                id: "mcp".into(),
                name: nome.into(),
                arguments: argumentos,
            };
            match self.registry.execute(&chamada).await {
                ExecutionOutcome::Executed(r) | ExecutionOutcome::Denied(r) => {
                    let c = vec![ContentBlock::text(r.content)];
                    if r.is_error {
                        CallToolResult::error(c)
                    } else {
                        CallToolResult::success(c)
                    }
                }
                ExecutionOutcome::NeedsConfirmation(p) => {
                    CallToolResult::error(vec![ContentBlock::text(format!(
                        "tool '{}' exige confirmação interativa e não pode ser executada por \
                         esta via (servidor MCP).",
                        p.name
                    ))])
                }
            }
        }
    }
}
