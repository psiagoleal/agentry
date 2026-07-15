// Caminho relativo: crates/core/src/tools/web_fetch.rs
//! Tool `web_fetch` (MT-65, ADR-0025): busca o conteúdo de uma URL
//! arbitrária, via o `Transport` único (nunca `reqwest` direto). Diferente
//! de todo outro provider/tool que já usa `Transport` (Ollama/LiteLLM/
//! Anthropic/SearXNG), o alvo aqui **não é um host fixo pré-cadastrado** —
//! é qualquer URL que o modelo peça. Por isso esta tool só funciona com um
//! `Transport` cuja `Allowlist` tenha o coringa
//! [`crate::egress::allowlist::ANY_HOST`], liberado só sob
//! `EgressClass::CloudOk` — quem monta esse `Transport` (a CLI) decide
//! isso, não esta tool.

use std::sync::Arc;
use std::time::Duration;

use crate::provider::BoxFuture;
use crate::tools::{Tool, ToolOutput};
use crate::transport::Transport;

/// `User-Agent` genérico fixo para tools de web (ADR-0025 — requisito de
/// anonimato): nunca o *default* do `reqwest`, nunca nada que identifique
/// usuário/máquina/versão do SO.
pub const WEB_TOOL_USER_AGENT: &str = "agentry-web-tool/1";

/// Teto de tamanho do corpo devolvido — evita que uma página gigante
/// consuma todo o orçamento de contexto de uma sessão.
const MAX_BODY_CHARS: usize = 20_000;

/// Timeout de rede para uma busca — página lenta/travada não deve travar o
/// agent loop indefinidamente.
const FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// Trunca `texto` a `max_chars` caracteres (não bytes — nunca corta um
/// caractere UTF-8 ao meio), anexando um aviso explícito quando corta.
fn trunca(texto: &str, max_chars: usize) -> String {
    if texto.chars().count() <= max_chars {
        return texto.to_string();
    }
    let truncado: String = texto.chars().take(max_chars).collect();
    format!("{truncado}\n\n[... conteúdo truncado em {max_chars} caracteres ...]")
}

/// Tool `web_fetch`: busca o conteúdo de uma URL via [`Transport`] e
/// devolve como texto puro, truncado — **sem** conversão HTML→Markdown
/// (fora de escopo da ADR-0025, exigiria um *parser* de HTML).
pub struct WebFetchTool {
    transport: Arc<Transport>,
}

impl WebFetchTool {
    /// Cria a tool sobre um `Transport` já configurado — tipicamente um
    /// `Transport` dedicado com o coringa `ANY_HOST`, montado pela CLI só
    /// quando a sessão está sob `EgressClass::CloudOk`.
    #[must_use]
    pub fn new(transport: Arc<Transport>) -> Self {
        Self { transport }
    }
}

impl Tool for WebFetchTool {
    fn name(&self) -> &str {
        "web_fetch"
    }

    fn description(&self) -> &str {
        "Busca o conteúdo de uma URL e devolve como texto (sem conversão para Markdown). Só \
         disponível quando o perfil ativo permite acesso amplo à internet."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": {
                    "type": "string",
                    "description": "URL completa a buscar (ex.: https://exemplo.com/pagina)."
                }
            },
            "required": ["url"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(url) = arguments.get("url").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'url' obrigatório e deve ser string");
            };

            match self
                .transport
                .get_text(url, "web_fetch", Some(FETCH_TIMEOUT))
                .await
            {
                Ok(corpo) => ToolOutput::ok(trunca(&corpo, MAX_BODY_CHARS)),
                Err(erro) => ToolOutput::error(format!("erro ao buscar '{url}': {erro}")),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::privacy::EgressClass;
    use crate::egress::allowlist::{Allowlist, AllowlistEntry, ANY_HOST};
    use crate::egress::audit::AuditEntry;
    use crate::transport::AuditSink;
    use serde_json::json;
    use std::sync::Mutex;

    struct NoopSink;
    impl AuditSink for NoopSink {
        fn record(&self, _entry: AuditEntry) {}
    }

    /// Mesma técnica de mock HTTP mínimo já usada em `transport::tests`/
    /// `provider::ollama::tests` (só `tokio::net`, sem lib de mock nova).
    async fn start_mock_server(response_body: &'static str) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                let _ = socket.read(&mut buf).await;
                let resposta = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = socket.write_all(resposta.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        addr
    }

    /// Como [`start_mock_server`], mas captura os bytes brutos da
    /// requisição — usado para provar que o `User-Agent` genérico foi
    /// realmente enviado.
    async fn start_mock_server_capturando(
        response_body: &'static str,
    ) -> (std::net::SocketAddr, Arc<Mutex<Vec<u8>>>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");
        let capturado = Arc::new(Mutex::new(Vec::new()));
        let alvo = Arc::clone(&capturado);

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 4096];
                if let Ok(n) = socket.read(&mut buf).await {
                    alvo.lock()
                        .expect("mutex de captura não deve envenenar")
                        .extend_from_slice(&buf[..n]);
                }
                let resposta = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\n\
                     Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response_body.len(),
                    response_body
                );
                let _ = socket.write_all(resposta.as_bytes()).await;
                let _ = socket.shutdown().await;
            }
        });

        (addr, capturado)
    }

    fn transport_coringa_cloud_ok() -> Arc<Transport> {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(ANY_HOST, EgressClass::CloudOk)]);
        Arc::new(
            Transport::new(allowlist, EgressClass::CloudOk, None, Arc::new(NoopSink))
                .with_header("User-Agent", WEB_TOOL_USER_AGENT),
        )
    }

    #[tokio::test]
    async fn busca_com_sucesso_devolve_o_corpo() {
        let addr = start_mock_server("conteúdo da página").await;
        let tool = WebFetchTool::new(transport_coringa_cloud_ok());

        let saida = tool
            .execute(json!({ "url": format!("http://{addr}/") }))
            .await;

        assert!(!saida.is_error);
        assert_eq!(saida.content, "conteúdo da página");
    }

    #[tokio::test]
    async fn corpo_maior_que_o_teto_e_truncado() {
        let corpo_grande: &'static str =
            Box::leak(vec!['a'; MAX_BODY_CHARS + 500].into_iter().collect());
        let addr = start_mock_server(corpo_grande).await;
        let tool = WebFetchTool::new(transport_coringa_cloud_ok());

        let saida = tool
            .execute(json!({ "url": format!("http://{addr}/") }))
            .await;

        assert!(!saida.is_error);
        assert!(saida.content.contains("truncado"));
        assert!(saida.content.len() < corpo_grande.len());
    }

    #[tokio::test]
    async fn user_agent_generico_e_enviado_de_fato() {
        let (addr, capturado) = start_mock_server_capturando("ok").await;
        let tool = WebFetchTool::new(transport_coringa_cloud_ok());

        tool.execute(json!({ "url": format!("http://{addr}/") }))
            .await;

        let requisicao_bruta =
            String::from_utf8_lossy(&capturado.lock().expect("mutex não deve envenenar"))
                .to_string();
        assert!(
            requisicao_bruta.contains(&format!("user-agent: {WEB_TOOL_USER_AGENT}"))
                || requisicao_bruta.contains(&format!("User-Agent: {WEB_TOOL_USER_AGENT}")),
            "requisição deveria carregar o User-Agent genérico fixo, corpo bruto: {requisicao_bruta}"
        );
    }

    #[tokio::test]
    async fn url_ausente_e_erro_tratado_sem_panic() {
        let tool = WebFetchTool::new(transport_coringa_cloud_ok());

        let saida = tool.execute(json!({})).await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn classe_de_egresso_insuficiente_e_erro_tratado_sem_panic() {
        // Allowlist com o coringa exigindo CloudOk, mas sessão só
        // LocalOnly — prova que o fail-closed do Transport (MT-05)
        // continua valendo mesmo com o coringa presente: o Transport
        // recusa antes de abrir qualquer conexão real.
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(ANY_HOST, EgressClass::CloudOk)]);
        let transport = Arc::new(Transport::new(
            allowlist,
            EgressClass::LocalOnly,
            None,
            Arc::new(NoopSink),
        ));
        let tool = WebFetchTool::new(transport);

        let saida = tool
            .execute(json!({ "url": "http://qualquer-host.invalido/" }))
            .await;

        assert!(saida.is_error);
    }
}
