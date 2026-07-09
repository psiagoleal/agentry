// Caminho relativo: crates/core/src/tools/lsp.rs
//! Tools `lsp_hover`/`lsp_definition` (MT-24, ADR-0013): expõem operações de
//! **leitura** (hover, *go-to-definition*, referências) do cliente LSP
//! (MT-23) ao agent loop. Nenhuma operação de escrita/refatoração via LSP
//! nesta v0.1 (ADR-0013).
//!
//! Roda sob o mesmo `ToolRegistry`/gate de permissão de qualquer outra tool
//! — nenhuma lógica de permissão própria aqui (mesma disciplina do MT-12/21).
//! Ausência do *language server* no ambiente (`LspClient::start` falhando)
//! é reportada como [`ToolOutput::error`], nunca trava o agent loop
//! (ADR-0013).
//!
//! [`LspSession`] inicia o *language server* sob demanda (na primeira
//! chamada de qualquer uma das duas tools) e reaproveita o mesmo processo
//! entre chamadas subsequentes — as duas tools compartilham uma única
//! sessão (via `Arc`), nunca spawnam um *language server* cada uma.
//!
//! Saída como JSON bruto da resposta do *language server* — formatação
//! legível para humano fica para quando houver demanda real; o essencial
//! desta v0.1 é o mecanismo (permissão, flag, ausência tratada), não a
//! apresentação.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lsp_types::{
    GotoDefinitionParams, Hover, HoverParams, Location, PartialResultParams, Position,
    ReferenceContext, ReferenceParams, TextDocumentIdentifier, TextDocumentPositionParams, Uri,
    WorkDoneProgressParams,
};

use crate::context::lsp::client::{LspClient, LspError};
use crate::provider::BoxFuture;
use crate::tools::{Tool, ToolOutput, ToolRegistry};

/// Sessão LSP compartilhada entre `lsp_hover`/`lsp_definition` (MT-24):
/// inicia o *language server* sob demanda, na primeira chamada, e
/// reaproveita o mesmo processo entre chamadas subsequentes.
pub struct LspSession {
    command: String,
    args: Vec<String>,
    root: PathBuf,
    client: Mutex<Option<LspClient>>,
}

impl LspSession {
    /// Cria a sessão — `command`/`args` identificam o *language server* a
    /// iniciar (ex.: `"rust-analyzer"`, `&[]`); `root` é a raiz do
    /// workspace, usada como *workspace folder* no `initialize`.
    #[must_use]
    pub fn new(command: impl Into<String>, args: Vec<String>, root: impl Into<PathBuf>) -> Self {
        Self {
            command: command.into(),
            args,
            root: root.into(),
            client: Mutex::new(None),
        }
    }

    fn file_uri(&self, caminho_relativo: &str) -> Result<Uri, String> {
        let absoluto = self.root.join(caminho_relativo);
        format!("file://{}", absoluto.display())
            .parse::<Uri>()
            .map_err(|_| format!("caminho não pôde ser convertido em URI: '{caminho_relativo}'"))
    }

    /// Garante que o cliente está iniciado e inicializado (na primeira
    /// chamada) e roda `f` com acesso exclusivo a ele.
    fn com_cliente<T>(
        &self,
        f: impl FnOnce(&mut LspClient) -> Result<T, LspError>,
    ) -> Result<T, LspError> {
        let mut guard = self.client.lock().expect("mutex não deve envenenar");
        if guard.is_none() {
            let args_refs: Vec<&str> = self.args.iter().map(String::as_str).collect();
            let mut client = LspClient::start(&self.command, &args_refs)?;
            let root_uri = format!("file://{}", self.root.display())
                .parse::<Uri>()
                .ok();
            client.initialize(root_uri)?;
            *guard = Some(client);
        }
        let client = guard.as_mut().expect("acabamos de garantir Some acima");
        f(client)
    }
}

fn parse_position(arguments: &serde_json::Value) -> Result<(String, Position), String> {
    let path = arguments
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("argumento 'path' ausente ou inválido")?
        .to_string();
    let line = arguments
        .get("line")
        .and_then(serde_json::Value::as_u64)
        .ok_or("argumento 'line' ausente ou inválido")?;
    let character = arguments
        .get("character")
        .and_then(serde_json::Value::as_u64)
        .ok_or("argumento 'character' ausente ou inválido")?;
    Ok((
        path,
        Position {
            line: line as u32,
            character: character as u32,
        },
    ))
}

fn posicao_schema() -> serde_json::Value {
    serde_json::json!({
        "path": { "type": "string", "description": "Caminho relativo à raiz do workspace." },
        "line": { "type": "integer", "description": "Linha, 0-indexada (convenção do LSP)." },
        "character": { "type": "integer", "description": "Coluna, 0-indexada (convenção do LSP)." }
    })
}

/// Tool `lsp_hover`: consulta tipo/documentação de um símbolo via *language server*.
pub struct LspHoverTool {
    session: Arc<LspSession>,
}

impl LspHoverTool {
    #[must_use]
    pub fn new(session: Arc<LspSession>) -> Self {
        Self { session }
    }
}

impl Tool for LspHoverTool {
    fn name(&self) -> &str {
        "lsp_hover"
    }

    fn description(&self) -> &str {
        "Consulta tipo/documentação (hover) de um símbolo via language server, na posição dada de um arquivo."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": posicao_schema(),
            "required": ["path", "line", "character"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let (caminho, position) = match parse_position(&arguments) {
                Ok(v) => v,
                Err(e) => return ToolOutput::error(e),
            };
            let uri = match self.session.file_uri(&caminho) {
                Ok(u) => u,
                Err(e) => return ToolOutput::error(e),
            };

            let resultado = self.session.com_cliente(|client| {
                let params = HoverParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier { uri },
                        position,
                    },
                    work_done_progress_params: WorkDoneProgressParams::default(),
                };
                client.request::<HoverParams, Option<Hover>>("textDocument/hover", params)
            });

            match resultado {
                Ok(Some(hover)) => match serde_json::to_string_pretty(&hover) {
                    Ok(texto) => ToolOutput::ok(texto),
                    Err(e) => ToolOutput::error(format!("falha ao formatar a resposta: {e}")),
                },
                Ok(None) => ToolOutput::ok("nenhuma informação de hover disponível nessa posição"),
                Err(e) => ToolOutput::error(format!("falha ao consultar o language server: {e}")),
            }
        })
    }
}

/// Tool `lsp_definition`: localiza a definição de um símbolo via *language
/// server* e, se `include_references` for `true`, também suas referências.
pub struct LspDefinitionTool {
    session: Arc<LspSession>,
}

impl LspDefinitionTool {
    #[must_use]
    pub fn new(session: Arc<LspSession>) -> Self {
        Self { session }
    }
}

impl Tool for LspDefinitionTool {
    fn name(&self) -> &str {
        "lsp_definition"
    }

    fn description(&self) -> &str {
        "Localiza a definição (e, opcionalmente, as referências) de um símbolo via language \
         server, na posição dada de um arquivo."
    }

    fn input_schema(&self) -> serde_json::Value {
        let mut properties = posicao_schema();
        properties["include_references"] = serde_json::json!({
            "type": "boolean",
            "description": "Também busca todas as referências ao símbolo (default: false)."
        });
        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": ["path", "line", "character"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let (caminho, position) = match parse_position(&arguments) {
                Ok(v) => v,
                Err(e) => return ToolOutput::error(e),
            };
            let uri = match self.session.file_uri(&caminho) {
                Ok(u) => u,
                Err(e) => return ToolOutput::error(e),
            };
            let include_references = arguments
                .get("include_references")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);

            let resultado = self.session.com_cliente(|client| {
                let text_document_position_params = TextDocumentPositionParams {
                    text_document: TextDocumentIdentifier { uri: uri.clone() },
                    position,
                };

                let definicao = client
                    .request::<GotoDefinitionParams, Option<lsp_types::GotoDefinitionResponse>>(
                        "textDocument/definition",
                        GotoDefinitionParams {
                            text_document_position_params: text_document_position_params.clone(),
                            work_done_progress_params: WorkDoneProgressParams::default(),
                            partial_result_params: PartialResultParams::default(),
                        },
                    )?;

                let referencias = if include_references {
                    client.request::<ReferenceParams, Option<Vec<Location>>>(
                        "textDocument/references",
                        ReferenceParams {
                            text_document_position: text_document_position_params,
                            work_done_progress_params: WorkDoneProgressParams::default(),
                            partial_result_params: PartialResultParams::default(),
                            context: ReferenceContext {
                                include_declaration: true,
                            },
                        },
                    )?
                } else {
                    None
                };

                Ok((definicao, referencias))
            });

            match resultado {
                Ok((definicao, referencias)) => {
                    let corpo = serde_json::json!({
                        "definition": definicao,
                        "references": referencias,
                    });
                    match serde_json::to_string_pretty(&corpo) {
                        Ok(texto) => ToolOutput::ok(texto),
                        Err(e) => ToolOutput::error(format!("falha ao formatar a resposta: {e}")),
                    }
                }
                Err(e) => ToolOutput::error(format!("falha ao consultar o language server: {e}")),
            }
        })
    }
}

/// Registra `lsp_hover`/`lsp_definition` em `registry`, respeitando a flag
/// `context.lsp_grounding.enabled` (ADR-0013, *default* `true`) — desligada,
/// nenhuma das duas é registrada. Fiação real com o `settings-schema` fica
/// fora de escopo deste ticket (mesmo padrão do MT-21).
pub fn register_lsp_tools(registry: &mut ToolRegistry, enabled: bool, session: Arc<LspSession>) {
    if enabled {
        registry.register(Arc::new(LspHoverTool::new(Arc::clone(&session))));
        registry.register(Arc::new(LspDefinitionTool::new(session)));
    }
}

// O round-trip de sucesso contra um language server de verdade
// (`lsp_hover_consulta_o_language_server_com_sucesso`) mora em
// `crates/core/tests/lsp_tools.rs`: `CARGO_BIN_EXE_fake_lsp_server` só é
// definida pelo Cargo para *testes de integração* do pacote, não para
// testes unitários dentro de `--lib` (mesma observação do MT-23). Os testes
// abaixo nunca chegam a spawnar um processo de verdade — `deny` barra antes
// de `execute()` rodar, e as duas checagens de flag só inspecionam
// `ToolRegistry::specs()` — então um comando inexistente serve perfeitamente
// como sessão de teste.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;
    use crate::model::ToolCall;
    use crate::tools::permission::PermissionGate;
    use crate::tools::ExecutionOutcome;

    fn call(name: &str, arguments: serde_json::Value) -> ToolCall {
        ToolCall {
            id: "call-1".into(),
            name: name.into(),
            arguments,
        }
    }

    fn sessao_dummy() -> Arc<LspSession> {
        Arc::new(LspSession::new(
            "este-comando-nao-existe-agentry-teste",
            Vec::new(),
            std::env::temp_dir(),
        ))
    }

    #[tokio::test]
    async fn lsp_hover_com_language_server_ausente_e_erro_tratado_sem_travar() {
        let tool = LspHoverTool::new(sessao_dummy());

        let saida = tool
            .execute(serde_json::json!({ "path": "a.rs", "line": 0, "character": 0 }))
            .await;

        assert!(
            saida.is_error,
            "ausência do language server deve ser reportada como erro tratado"
        );
    }

    #[tokio::test]
    async fn respeita_gate_de_permissao_do_mt11() {
        let gate = PermissionGate::new(Permissions {
            deny: vec!["lsp_hover".into()],
            ask: vec![],
        });
        let mut registry = ToolRegistry::new(gate);
        registry.register(Arc::new(LspHoverTool::new(sessao_dummy())));

        let outcome = registry
            .execute(&call(
                "lsp_hover",
                serde_json::json!({ "path": "a.rs", "line": 0, "character": 0 }),
            ))
            .await;

        assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
    }

    #[test]
    fn flag_desligada_nao_registra_nenhuma_tool() {
        let gate = PermissionGate::new(Permissions::default());
        let mut registry = ToolRegistry::new(gate);

        register_lsp_tools(&mut registry, false, sessao_dummy());

        assert!(
            registry.specs().is_empty(),
            "flag desligada não deveria registrar nenhuma tool"
        );
    }

    #[test]
    fn flag_ligada_registra_as_duas_tools() {
        let gate = PermissionGate::new(Permissions::default());
        let mut registry = ToolRegistry::new(gate);

        register_lsp_tools(&mut registry, true, sessao_dummy());

        let specs = registry.specs();
        let nomes: Vec<&str> = specs.iter().map(|s| s.name.as_str()).collect();
        assert!(nomes.contains(&"lsp_hover"));
        assert!(nomes.contains(&"lsp_definition"));
    }
}
