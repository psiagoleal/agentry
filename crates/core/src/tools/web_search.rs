// Caminho relativo: crates/core/src/tools/web_search.rs
//! Tool `web_search` (MT-66, ADR-0025): pesquisa via uma instância SearXNG
//! configurada pelo usuário (`tools.webSearch.searxngUrl`) — **nunca** uma
//! instância pública *hardcoded*. Diferente de `web_fetch` (MT-65, host
//! arbitrário via coringa), o endpoint SearXNG é **um host único e
//! conhecido**, cabendo no mesmo modelo de `Allowlist` já usado pelo
//! LiteLLM (ADR-0006) — sem o coringa `ANY_HOST`.

use std::sync::Arc;
use std::time::Duration;

use serde::Deserialize;

use crate::provider::BoxFuture;
use crate::tools::{Tool, ToolOutput};
use crate::transport::{build_searxng_search_url, Transport};

/// Número máximo de resultados formatados — evita uma resposta gigante.
const MAX_RESULTS: usize = 8;
/// Timeout de rede para uma busca.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
struct SearxngResponse {
    #[serde(default)]
    results: Vec<SearxngResult>,
}

#[derive(Debug, Deserialize)]
struct SearxngResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    content: String,
}

/// Formata os resultados como lista numerada (título/URL/resumo), capada a
/// [`MAX_RESULTS`] — devolvidos na ordem em que o SearXNG já os devolve,
/// sem *ranking*/reordenação própria (fora de escopo desta ADR).
fn formata_resultados(resultados: &[SearxngResult]) -> String {
    if resultados.is_empty() {
        return "nenhum resultado encontrado".to_string();
    }
    resultados
        .iter()
        .take(MAX_RESULTS)
        .enumerate()
        .map(|(indice, resultado)| {
            format!(
                "{}. {}\n   {}\n   {}",
                indice + 1,
                resultado.title,
                resultado.url,
                resultado.content
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Tool `web_search`: consulta a API JSON de uma instância SearXNG via
/// [`Transport`] (host único, sem coringa) e devolve os resultados
/// formatados.
pub struct WebSearchTool {
    transport: Arc<Transport>,
    searxng_url: String,
}

impl WebSearchTool {
    /// Cria a tool sobre um `Transport` já configurado com a `Allowlist` do
    /// host de `searxng_url` — tipicamente um `Transport` dedicado, mesmo
    /// padrão de `providers.litellm` (MT-49).
    #[must_use]
    pub fn new(transport: Arc<Transport>, searxng_url: impl Into<String>) -> Self {
        Self {
            transport,
            searxng_url: searxng_url.into(),
        }
    }
}

impl Tool for WebSearchTool {
    fn name(&self) -> &str {
        "web_search"
    }

    fn description(&self) -> &str {
        "Pesquisa um termo na web via a instância SearXNG configurada e devolve os principais \
         resultados (título, URL, resumo)."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Termo a pesquisar."
                }
            },
            "required": ["query"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(query) = arguments.get("query").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'query' obrigatório e deve ser string");
            };

            let url = match build_searxng_search_url(&self.searxng_url, query) {
                Ok(url) => url,
                Err(erro) => return ToolOutput::error(format!("URL do SearXNG inválida: {erro}")),
            };

            let corpo = match self
                .transport
                .get_text(&url, "web_search", Some(SEARCH_TIMEOUT))
                .await
            {
                Ok(corpo) => corpo,
                Err(erro) => {
                    return ToolOutput::error(format!("erro ao pesquisar '{query}': {erro}"))
                }
            };

            match serde_json::from_str::<SearxngResponse>(&corpo) {
                Ok(resposta) => ToolOutput::ok(formata_resultados(&resposta.results)),
                Err(erro) => ToolOutput::error(format!("resposta do SearXNG malformada: {erro}")),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::privacy::EgressClass;
    use crate::egress::allowlist::{Allowlist, AllowlistEntry};
    use crate::egress::audit::AuditEntry;
    use crate::transport::AuditSink;
    use serde_json::json;
    use std::sync::Mutex;

    struct NoopSink;
    impl AuditSink for NoopSink {
        fn record(&self, _entry: AuditEntry) {}
    }

    /// Mesma técnica de mock HTTP mínimo já usada em `tools::web_fetch`/
    /// `transport::tests` (só `tokio::net`, sem lib de mock nova).
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
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
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
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
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

    fn transport_para(addr: std::net::SocketAddr) -> Arc<Transport> {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            addr.ip().to_string(),
            EgressClass::CloudOk,
        )]);
        Arc::new(
            Transport::new(allowlist, EgressClass::CloudOk, None, Arc::new(NoopSink))
                .with_header("User-Agent", "agentry-web-tool/1"),
        )
    }

    const RESPOSTA_SEARXNG_VALIDA: &str = r#"{
        "results": [
            { "title": "Rust — linguagem", "url": "https://rust-lang.org", "content": "Uma linguagem de sistemas." },
            { "title": "Tokio", "url": "https://tokio.rs", "content": "Runtime assíncrono." }
        ]
    }"#;

    #[tokio::test]
    async fn resposta_json_valida_e_formatada_com_titulo_url_resumo() {
        let addr = start_mock_server(RESPOSTA_SEARXNG_VALIDA).await;
        let tool = WebSearchTool::new(transport_para(addr), format!("http://{addr}"));

        let saida = tool.execute(json!({ "query": "rust async" })).await;

        assert!(!saida.is_error);
        assert!(saida.content.contains("Rust — linguagem"));
        assert!(saida.content.contains("https://rust-lang.org"));
        assert!(saida.content.contains("Uma linguagem de sistemas."));
        assert!(saida.content.contains("Tokio"));
    }

    #[tokio::test]
    async fn sem_resultados_devolve_mensagem_tratada_nao_vazia() {
        let addr = start_mock_server(r#"{"results": []}"#).await;
        let tool = WebSearchTool::new(transport_para(addr), format!("http://{addr}"));

        let saida = tool.execute(json!({ "query": "algo bem obscuro" })).await;

        assert!(!saida.is_error);
        assert_eq!(saida.content, "nenhum resultado encontrado");
    }

    #[tokio::test]
    async fn resposta_malformada_e_erro_tratado_sem_panic() {
        let addr = start_mock_server("isso não é JSON válido {{{").await;
        let tool = WebSearchTool::new(transport_para(addr), format!("http://{addr}"));

        let saida = tool.execute(json!({ "query": "qualquer coisa" })).await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn query_ausente_e_erro_tratado() {
        let addr = start_mock_server(RESPOSTA_SEARXNG_VALIDA).await;
        let tool = WebSearchTool::new(transport_para(addr), format!("http://{addr}"));

        let saida = tool.execute(json!({})).await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn user_agent_generico_e_enviado_de_fato() {
        let (addr, capturado) = start_mock_server_capturando(r#"{"results": []}"#).await;
        let tool = WebSearchTool::new(transport_para(addr), format!("http://{addr}"));

        tool.execute(json!({ "query": "teste" })).await;

        let requisicao_bruta =
            String::from_utf8_lossy(&capturado.lock().expect("mutex não deve envenenar"))
                .to_string();
        assert!(
            requisicao_bruta.to_lowercase().contains("user-agent: agentry-web-tool/1"),
            "requisição deveria carregar o User-Agent genérico fixo, corpo bruto: {requisicao_bruta}"
        );
        assert!(
            requisicao_bruta.contains("GET /search?q=teste&format=json"),
            "requisição deveria mirar /search com q e format, corpo bruto: {requisicao_bruta}"
        );
    }
}
