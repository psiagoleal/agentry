// Caminho relativo: crates/core/src/mcp/mod.rs
//! Cliente MCP mínimo (Fase 16, ADR-0028): spawna um servidor MCP local
//! como subprocesso via `rmcp` (`client`+`transport-child-process`, nunca
//! `server`/transporte HTTP em produção), completa o *handshake* MCP e
//! lista as tools disponíveis.
//!
//! Mesmo modelo de confiança de [`crate::context::lsp::client::LspClient`]
//! (ADR-0013): subprocesso local, IPC via `pipe` — nunca uma chamada de
//! rede mediada pelo `agentry`. Só servidores MCP locais são suportados
//! nesta fase; transportes remotos (HTTP/SSE) ficam fora de escopo
//! (ADR-0028).
//!
//! Registro das tools descobertas no `ToolRegistry` fica em
//! [`crate::tools::mcp`] (MT-79).

use rmcp::model::{CallToolRequestParams, CallToolResult, Tool};
use rmcp::service::{RoleClient, RunningService};
use rmcp::transport::TokioChildProcess;
use rmcp::ServiceExt;
use tokio::process::Command;

use crate::config::privacy::EgressClass;
use crate::config::McpServerSettings;

/// Erros do ciclo de vida do cliente MCP.
///
/// Mesma forma de [`crate::context::lsp::client::LspError`] — ausência do
/// servidor no ambiente é [`McpError::Spawn`], erro tratado, nunca pânico.
#[derive(Debug)]
pub enum McpError {
    /// Falha ao iniciar o processo do servidor MCP.
    Spawn(String),
    /// Falha no *handshake*/protocolo MCP (inclui o servidor encerrar
    /// antes de responder).
    Protocol(String),
    /// `egressClass` declarada é diferente de `local-only` (ADR-0028) —
    /// devolvido **antes** de qualquer subprocesso ser spawnado. Defesa em
    /// profundidade (MT-80): `Settings::from_json_str` (MT-77) já rejeita
    /// esse caso no *parsing* do arquivo, mas este erro garante que nenhum
    /// caminho de código — inclusive um `McpServerSettings` montado direto
    /// em Rust, sem passar pelo parser — chega a conectar um servidor
    /// remoto.
    EgressNotSupported(EgressClass),
}

impl core::fmt::Display for McpError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Spawn(msg) => write!(f, "falha ao iniciar o servidor MCP: {msg}"),
            Self::Protocol(msg) => write!(f, "servidor MCP respondeu com erro: {msg}"),
            Self::EgressNotSupported(classe) => write!(
                f,
                "servidor MCP declara egressClass '{classe:?}', mas esta versão só suporta \
                 servidores MCP locais (egressClass 'local-only'); servidores remotos ainda \
                 não são suportados (ADR-0028)"
            ),
        }
    }
}

impl std::error::Error for McpError {}

/// Cliente MCP: inicia um servidor MCP local como subprocesso e completa o
/// *handshake*. `RunningService` (do `rmcp`) já encapsula a *task* de I/O
/// de fundo e o processo internamente — o próprio `TokioChildProcess` mata
/// o subprocesso quando descartado (`ChildWithCleanup::drop`, dentro do
/// `rmcp`), então nenhum `Drop` manual é necessário aqui: descartar
/// `McpClient` sem [`Self::shutdown`] explícito já não deixa o processo
/// órfão, mesma garantia observável de `LspClient` (validado pelo teste de
/// integração — `crates/core/tests/mcp_client.rs`).
#[derive(Debug)]
pub struct McpClient {
    servico: RunningService<RoleClient, ()>,
    pid: Option<u32>,
}

impl McpClient {
    /// Inicia `command` (com `args`) como subprocesso MCP local (`stdio`)
    /// e completa o *handshake*.
    ///
    /// # Errors
    ///
    /// Devolve [`McpError::Spawn`] se o processo não puder ser iniciado —
    /// caso mais comum: `command` não encontrado no `PATH`.
    /// [`McpError::Protocol`] se o *handshake* MCP falhar.
    pub async fn start(command: &str, args: &[String]) -> Result<Self, McpError> {
        let mut comando = Command::new(command);
        comando.args(args);
        let transporte =
            TokioChildProcess::new(comando).map_err(|e| McpError::Spawn(e.to_string()))?;
        let pid = transporte.id();
        let servico = ().serve(transporte).await.map_err(|e| McpError::Protocol(e.to_string()))?;
        Ok(Self { servico, pid })
    }

    /// Igual a [`Self::start`], mas a partir de um [`McpServerSettings`]
    /// completo — checa `egress_class` **antes** de tocar em
    /// [`Command`]/[`TokioChildProcess`] (MT-80, defesa em profundidade
    /// além do que `Settings::from_json_str`, MT-77, já rejeita no
    /// *parsing*). É o ponto de entrada usado pela wiring de produção
    /// (`register_mcp_tools`, `crates/cli/src/main.rs`) — `Self::start`
    /// continua existindo à parte só porque a suíte de testes/*fixtures*
    /// (`fake_mcp_server`) não passa por `McpServerSettings`.
    ///
    /// # Errors
    ///
    /// Devolve [`McpError::EgressNotSupported`] se `settings.egress_class`
    /// não for [`EgressClass::LocalOnly`] — nenhum subprocesso é spawnado
    /// nesse caso. Do contrário, os mesmos erros de [`Self::start`].
    pub async fn start_from_settings(settings: &McpServerSettings) -> Result<Self, McpError> {
        if settings.egress_class != EgressClass::LocalOnly {
            return Err(McpError::EgressNotSupported(settings.egress_class));
        }
        Self::start(&settings.command, &settings.args).await
    }

    /// PID do subprocesso, se disponível — útil para diagnóstico/log; a
    /// suíte de testes usa para confirmar que o processo não fica órfão.
    #[must_use]
    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Lista todas as tools expostas pelo servidor — paginação resolvida
    /// automaticamente pelo `rmcp` (`Peer::list_all_tools`).
    ///
    /// # Errors
    ///
    /// Devolve [`McpError::Protocol`] se a chamada falhar.
    pub async fn list_tools(&self) -> Result<Vec<Tool>, McpError> {
        self.servico
            .list_all_tools()
            .await
            .map_err(|e| McpError::Protocol(e.to_string()))
    }

    /// Chama uma tool pelo nome **original** (como o servidor a conhece,
    /// sem o prefixo de servidor que [`crate::tools::mcp::McpTool`] usa só
    /// no nome de *registro*).
    ///
    /// # Errors
    ///
    /// Devolve [`McpError::Protocol`] se a chamada falhar.
    pub async fn call_tool(
        &self,
        params: CallToolRequestParams,
    ) -> Result<CallToolResult, McpError> {
        self.servico
            .call_tool(params)
            .await
            .map_err(|e| McpError::Protocol(e.to_string()))
    }

    /// Encerra a sessão de forma limpa. Consome `self`: depois de chamado,
    /// o cliente não pode mais ser usado.
    ///
    /// # Errors
    ///
    /// Devolve [`McpError::Protocol`] se o encerramento falhar.
    pub async fn shutdown(self) -> Result<(), McpError> {
        self.servico
            .cancel()
            .await
            .map_err(|e| McpError::Protocol(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // O ciclo de vida completo (start → list_tools → shutdown, e
    // Drop sem shutdown explícito não deixa processo órfão) mora em
    // `crates/core/tests/mcp_client.rs`: `CARGO_BIN_EXE_fake_mcp_server`
    // só é definida pelo Cargo para testes de integração do pacote, não
    // para testes unitários dentro de `--lib` (mesmo motivo do
    // `fake_lsp_server`, MT-23).

    #[tokio::test]
    async fn start_com_comando_inexistente_e_erro_tratado() {
        let erro = McpClient::start("este-comando-nao-existe-agentry-teste", &[])
            .await
            .expect_err("comando inexistente deve falhar ao spawnar, não travar");
        assert!(matches!(erro, McpError::Spawn(_)));
    }

    #[tokio::test]
    async fn start_from_settings_com_egress_class_remoto_e_erro_tratado_sem_spawnar() {
        let settings = McpServerSettings {
            command: "este-comando-nunca-deve-ser-executado".to_string(),
            args: vec![],
            egress_class: EgressClass::CloudOk,
        };

        let erro = McpClient::start_from_settings(&settings)
            .await
            .expect_err("egressClass diferente de local-only deve ser rejeitada antes de spawnar");

        assert!(matches!(
            erro,
            McpError::EgressNotSupported(EgressClass::CloudOk)
        ));
    }

    #[tokio::test]
    async fn start_from_settings_com_local_only_e_comando_inexistente_ainda_falha_ao_spawnar() {
        let settings = McpServerSettings {
            command: "este-comando-nao-existe-agentry-teste".to_string(),
            args: vec![],
            egress_class: EgressClass::LocalOnly,
        };

        let erro = McpClient::start_from_settings(&settings)
            .await
            .expect_err("comando inexistente deve falhar ao spawnar, não travar");

        assert!(matches!(erro, McpError::Spawn(_)));
    }
}
