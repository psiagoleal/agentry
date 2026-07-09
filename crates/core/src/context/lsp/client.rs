// Caminho relativo: crates/core/src/context/lsp/client.rs
//! Cliente LSP mínimo (MT-23, ADR-0013): spawn + JSON-RPC sobre stdio.
//!
//! Fala com o *Language Server* já instalado no ambiente do usuário para a
//! linguagem do projeto (`rust-analyzer`, `pyright`, `gopls` etc.) — o
//! `agentry` **não** empacota nem instala nenhum *language server*
//! (ADR-0013), só fala o protocolo com o que já está disponível.
//!
//! Usa o *framing* de mensagens do `lsp-server` (`Message::read`/`write`,
//! genéricos sobre `BufRead`/`Write` — a mesma técnica que o `rust-analyzer`
//! usa do lado servidor, reaproveitada aqui do lado cliente) e os tipos de
//! protocolo do `lsp-types`. Cobre só o ciclo de vida (`start` →
//! `initialize` → `shutdown`) e `didOpen`; operações de leitura
//! (hover/definição/referências) chegam no MT-24 como `Tool` (MT-11).

use std::io::BufReader;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use lsp_server::{Message, Notification, Request, RequestId, Response};
use lsp_types::{
    DidOpenTextDocumentParams, InitializeParams, InitializeResult, InitializedParams,
    TextDocumentItem, Uri,
};

/// Erros do ciclo de vida do cliente LSP.
///
/// Ausência do *language server* no ambiente é [`LspError::Spawn`] — erro
/// tratado, não pânico (ADR-0013: quem consome este cliente, ex. a tool do
/// MT-24, decide como reportar isso sem travar o agent loop).
#[derive(Debug)]
pub enum LspError {
    /// Falha ao iniciar o processo do *language server*.
    Spawn(String),
    /// Falha de I/O ao ler/escrever uma mensagem.
    Io(String),
    /// O *language server* respondeu com um erro de protocolo.
    Protocol(String),
    /// O processo encerrou (ou fechou stdout) antes da resposta esperada.
    Closed,
}

impl core::fmt::Display for LspError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Spawn(msg) => write!(f, "falha ao iniciar o language server: {msg}"),
            Self::Io(msg) => write!(f, "falha de I/O com o language server: {msg}"),
            Self::Protocol(msg) => write!(f, "language server respondeu com erro: {msg}"),
            Self::Closed => write!(f, "language server encerrou antes da resposta esperada"),
        }
    }
}

impl std::error::Error for LspError {}

/// Cliente LSP mínimo: inicia um *language server* como subprocesso e fala
/// JSON-RPC sobre seu `stdin`/`stdout`. `stderr` é herdado (útil para
/// depuração manual; nunca interpretado pelo cliente).
#[derive(Debug)]
pub struct LspClient {
    processo: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    proximo_id: i32,
}

impl LspClient {
    /// Inicia `command` (com `args`) como subprocesso, com `stdin`/`stdout`
    /// conectados por *pipe*.
    ///
    /// # Errors
    ///
    /// Devolve [`LspError::Spawn`] se o processo não puder ser iniciado —
    /// caso mais comum: `command` não encontrado no `PATH` (o *language
    /// server* da linguagem não está instalado no ambiente).
    pub fn start(command: &str, args: &[&str]) -> Result<Self, LspError> {
        let mut processo = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| LspError::Spawn(e.to_string()))?;
        let stdin = processo
            .stdin
            .take()
            .expect("stdin configurado como piped em Self::start");
        let stdout = processo
            .stdout
            .take()
            .expect("stdout configurado como piped em Self::start");
        Ok(Self {
            processo,
            stdin,
            stdout: BufReader::new(stdout),
            proximo_id: 0,
        })
    }

    /// PID do processo do *language server* — útil para diagnóstico/log; a
    /// suíte de testes usa para confirmar que o processo não fica órfão.
    #[must_use]
    pub fn pid(&self) -> u32 {
        self.processo.id()
    }

    fn proximo_request_id(&mut self) -> RequestId {
        let id = self.proximo_id;
        self.proximo_id += 1;
        RequestId::from(id)
    }

    fn enviar(&mut self, mensagem: Message) -> Result<(), LspError> {
        mensagem
            .write(&mut self.stdin)
            .map_err(|e| LspError::Io(e.to_string()))
    }

    fn receber(&mut self) -> Result<Message, LspError> {
        Message::read(&mut self.stdout)
            .map_err(|e| LspError::Io(e.to_string()))?
            .ok_or(LspError::Closed)
    }

    /// Espera pela [`Response`] de `id`, descartando qualquer outra
    /// mensagem recebida antes dela.
    fn esperar_resposta(&mut self, id: &RequestId) -> Result<Response, LspError> {
        loop {
            if let Message::Response(resposta) = self.receber()? {
                if &resposta.id == id {
                    return Ok(resposta);
                }
            }
        }
    }

    /// Envia uma requisição arbitrária `method` com `params` e devolve a
    /// resposta desserializada como `R` — primitivo genérico usado pelas
    /// tools de leitura do MT-24 (`textDocument/hover`/`definition`/
    /// `references`); `initialize`/`shutdown` continuam com métodos
    /// dedicados por já precisarem de passos adicionais no *handshake*.
    ///
    /// # Errors
    ///
    /// Devolve [`LspError::Protocol`] se o servidor responder com erro;
    /// [`LspError::Io`] se a comunicação falhar ou a resposta não
    /// desserializar como `R`.
    pub fn request<P, R>(&mut self, method: &str, params: P) -> Result<R, LspError>
    where
        P: serde::Serialize,
        R: serde::de::DeserializeOwned,
    {
        let id = self.proximo_request_id();
        self.enviar(Message::Request(Request::new(
            id.clone(),
            method.to_string(),
            params,
        )))?;
        let resposta = self.esperar_resposta(&id)?;
        if let Some(erro) = resposta.error {
            return Err(LspError::Protocol(erro.message));
        }
        serde_json::from_value(resposta.result.unwrap_or_default())
            .map_err(|e| LspError::Io(e.to_string()))
    }

    /// Envia `initialize`, espera a resposta e envia a notificação
    /// `initialized` — *handshake* completo do LSP.
    ///
    /// `root_uri` é traduzido para `workspace_folders` (`root_uri` é campo
    /// depreciado na API do `lsp-types`, em favor de `workspace_folders`).
    ///
    /// # Errors
    ///
    /// Devolve [`LspError::Protocol`] se o servidor responder com erro;
    /// [`LspError::Io`]/[`LspError::Closed`] em falha de comunicação.
    pub fn initialize(&mut self, root_uri: Option<Uri>) -> Result<InitializeResult, LspError> {
        let workspace_folders = root_uri.map(|uri| {
            vec![lsp_types::WorkspaceFolder {
                uri,
                name: "workspace".to_string(),
            }]
        });
        let params = InitializeParams {
            workspace_folders,
            ..Default::default()
        };
        let resultado: InitializeResult = self.request("initialize", params)?;

        self.enviar(Message::Notification(Notification::new(
            "initialized".to_string(),
            InitializedParams {},
        )))?;

        Ok(resultado)
    }

    /// Notifica o servidor de que `uri` foi "aberto" com o conteúdo `text`
    /// — necessário antes de operações de leitura sobre o documento (MT-24).
    ///
    /// # Errors
    ///
    /// Devolve erro se a notificação não puder ser enviada.
    pub fn did_open(&mut self, uri: Uri, language_id: &str, text: String) -> Result<(), LspError> {
        let params = DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri,
                language_id: language_id.to_string(),
                version: 0,
                text,
            },
        };
        self.enviar(Message::Notification(Notification::new(
            "textDocument/didOpen".to_string(),
            params,
        )))
    }

    /// Encerra o ciclo de vida de forma limpa: `shutdown` (espera resposta),
    /// `exit`, e espera o processo terminar de fato — garante que nenhum
    /// processo fique órfão (ADR-0013). Consome `self`: depois de chamado,
    /// o cliente não pode mais ser usado.
    ///
    /// # Errors
    ///
    /// Devolve [`LspError::Protocol`] se o servidor responder com erro ao
    /// `shutdown`; [`LspError::Io`] se a troca de mensagens ou a espera
    /// pelo processo falhar.
    pub fn shutdown(mut self) -> Result<(), LspError> {
        let _resultado: serde_json::Value = self.request("shutdown", serde_json::Value::Null)?;

        self.enviar(Message::Notification(Notification::new(
            "exit".to_string(),
            serde_json::Value::Null,
        )))?;

        self.processo
            .wait()
            .map_err(|e| LspError::Io(e.to_string()))?;
        Ok(())
    }
}

impl Drop for LspClient {
    /// Rede de segurança: se [`Self::shutdown`] nunca foi chamado (ex.:
    /// erro no meio do ciclo de vida), garante que o processo não fique
    /// órfão de qualquer forma — mata e espera.
    fn drop(&mut self) {
        let _ = self.processo.kill();
        let _ = self.processo.wait();
    }
}

// O ciclo de vida completo (start → initialize → shutdown, contra o
// `fake_lsp_server` de teste) mora em `crates/core/tests/lsp_client.rs`:
// `CARGO_BIN_EXE_fake_lsp_server` só é definida pelo Cargo para *testes de
// integração* do pacote, não para testes unitários dentro de `--lib`.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_com_comando_inexistente_e_erro_tratado() {
        let erro = LspClient::start("este-comando-nao-existe-agentry-teste", &[])
            .expect_err("comando inexistente deve falhar ao spawnar, não travar");
        assert!(matches!(erro, LspError::Spawn(_)));
    }
}
