// Caminho relativo: crates/core/src/transport/mod.rs
//! Transporte HTTP único sobre `reqwest` (MT-07, ADR-0002).
//!
//! Este é o **único** ponto do crate autorizado a fazer chamadas de rede
//! reais. Toda requisição passa, nesta ordem, por:
//!
//! 1. **Allowlist** (MT-05, [`crate::egress::allowlist`]) — decide se o host
//!    de destino é alcançável sob a classe de egresso ativa da sessão.
//! 2. **Audit log** (MT-06, [`crate::egress::audit`]) — toda tentativa,
//!    permitida ou bloqueada, gera uma [`AuditEntry`] entregue ao [`AuditSink`]
//!    configurado; a entrada nunca contém segredos porque `AuditEntry::new` já
//!    redige (MT-06, [`crate::egress::redact`]).
//!
//! Uma tentativa bloqueada **aborta antes de tocar a rede**: nenhuma conexão
//! TCP é aberta. Nenhum outro módulo deste crate deve importar `reqwest`
//! diretamente — o teste `reqwest_e_usado_somente_no_modulo_de_transporte`,
//! abaixo, garante isso lendo o próprio código-fonte do crate.
//!
//! Além do POST não-streaming ([`Transport::post_json`]), o transporte expõe
//! [`Transport::post_json_lines`] para adapters com streaming (ex.: Ollama,
//! MT-08): devolve o corpo da resposta como um fluxo de **linhas de texto
//! bruto**, sem conhecer o formato de nenhum provider — a interpretação de
//! cada linha (NDJSON, SSE etc.) é responsabilidade do adapter, mantendo o
//! transporte agnóstico de provider (fora de escopo do MT-07). [`Transport::get_text`]
//! (MT-42, ADR-0019) cobre o caso GET simples — busca de um artefato estático
//! (não uma API de chat/tool-calling) —, devolvendo o corpo como texto bruto
//! pela mesma razão de agnosticismo de formato.
//!
//! Ambos os métodos aceitam um **timeout por chamada** (MT-17, ADR-0009),
//! via a API nativa do `reqwest` (`.timeout()` no builder da requisição);
//! `None` cai no timeout *default* do `Client` construído internamente. O
//! caso de uso motivador é a troca de modelo em provider local (Ollama): o
//! Router (MT-09) sabe, antes de qualquer chamada, se a resolução atual
//! implica um *cold load* — só o Transporte pode aplicar o timeout, e só o
//! Router sabe qual usar, então o valor sempre entra de fora.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::config::privacy::EgressClass;
use crate::egress::allowlist::{Allowlist, EgressError};
use crate::egress::audit::AuditEntry;

/// Recebe cada [`AuditEntry`] produzida pelo transporte.
///
/// A persistência/transmissão do log fica a critério de quem implementa esta
/// trait (arquivo, stdout, coletor de teste); o transporte só garante que
/// **toda** tentativa de egresso gera uma chamada a [`AuditSink::record`] —
/// não há caminho silencioso.
pub trait AuditSink: Send + Sync {
    /// Registra uma entrada de auditoria.
    fn record(&self, entry: AuditEntry);
}

/// Erros do transporte único.
#[derive(Debug, PartialEq)]
pub enum TransportError {
    /// URL malformada ou sem host.
    InvalidUrl(String),
    /// Egresso bloqueado pela política de allowlist/classe (ADR-0002).
    Blocked(EgressError),
    /// Falha na chamada HTTP em si (rede, status, corpo).
    Http(String),
}

impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidUrl(msg) => write!(f, "URL inválida: {msg}"),
            Self::Blocked(err) => write!(f, "egresso bloqueado: {err}"),
            Self::Http(msg) => write!(f, "falha HTTP: {msg}"),
        }
    }
}

impl std::error::Error for TransportError {}

/// O transporte único: integra allowlist + audit log sobre um `reqwest::Client`.
pub struct Transport {
    client: reqwest::Client,
    allowlist: Allowlist,
    egress_class: EgressClass,
    profile: Option<String>,
    sink: Arc<dyn AuditSink>,
    default_headers: Vec<(String, String)>,
}

impl Transport {
    /// Cria um transporte com a política de egresso e o coletor de auditoria
    /// dados, construindo seu próprio `reqwest::Client` internamente.
    ///
    /// O tipo `reqwest::Client` deliberadamente **não** aparece na assinatura
    /// pública: se aparecesse, qualquer módulo que construísse um
    /// [`Transport`] (adapters, testes) precisaria importar `reqwest`
    /// também, furando o invariante que este módulo existe para garantir.
    #[must_use]
    pub fn new(
        allowlist: Allowlist,
        egress_class: EgressClass,
        profile: Option<String>,
        sink: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            client: reqwest::Client::new(),
            allowlist,
            egress_class,
            profile,
            sink,
            default_headers: Vec::new(),
        }
    }

    /// Anexa um header (`name`/`value`) a toda requisição deste transporte
    /// (MT-15/16) — mecanismo genérico de autenticação por provider: o
    /// padrão OpenAI-compatible usa `Authorization: Bearer <key>`
    /// (OpenRouter/gateways LiteLLM em nuvem), a Anthropic usa `x-api-key` +
    /// `anthropic-version`. O transporte não assume nenhum esquema
    /// específico — quem monta o [`Transport`] para um provider decide quais
    /// headers ele precisa. Builder encadeável em vez de parâmetro extra em
    /// [`Self::new`], para não quebrar os chamadores existentes (ex.:
    /// Ollama) que não precisam de nenhum header extra.
    #[must_use]
    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.default_headers.push((name.into(), value.into()));
        self
    }

    /// Verifica a allowlist para `url` e emite o [`AuditEntry`] correspondente
    /// (permitido ou bloqueado). Se permitido, devolve a URL já interpretada,
    /// pronta para a chamada real — nenhuma conexão é aberta antes disto.
    fn authorize(&self, url: &str, task: &str) -> Result<reqwest::Url, TransportError> {
        let parsed =
            reqwest::Url::parse(url).map_err(|e| TransportError::InvalidUrl(e.to_string()))?;
        let host = parsed
            .host_str()
            .ok_or_else(|| TransportError::InvalidUrl(format!("'{url}' não tem host")))?
            .to_string();

        if let Err(egress_err) = self.allowlist.check(self.egress_class, &host) {
            self.sink.record(AuditEntry::blocked(
                url,
                self.profile.clone(),
                self.egress_class,
                task,
                egress_err.to_string(),
            ));
            return Err(TransportError::Blocked(egress_err));
        }

        self.sink.record(AuditEntry::allowed(
            url,
            self.profile.clone(),
            self.egress_class,
            task,
        ));

        Ok(parsed)
    }

    /// Envia um POST com corpo JSON para `url`, sob a política de egresso ativa.
    ///
    /// Decide primeiro se o host de `url` é alcançável (allowlist); se não
    /// for, devolve [`TransportError::Blocked`] **sem abrir conexão alguma**.
    /// Toda tentativa — permitida ou bloqueada — gera uma [`AuditEntry`]
    /// através do `sink` configurado.
    ///
    /// # Errors
    ///
    /// Devolve [`TransportError::InvalidUrl`] se `url` não puder ser
    /// interpretada ou não tiver host; [`TransportError::Blocked`] se a
    /// allowlist recusar o destino; [`TransportError::Http`] se a chamada de
    /// rede falhar, estourar `timeout` (quando `Some`) ou devolver status de
    /// erro.
    pub async fn post_json(
        &self,
        url: &str,
        task: &str,
        body: &serde_json::Value,
        timeout: Option<Duration>,
    ) -> Result<serde_json::Value, TransportError> {
        let parsed = self.authorize(url, task)?;

        let mut requisicao = self.client.post(parsed).json(body);
        for (nome, valor) in &self.default_headers {
            requisicao = requisicao.header(nome, valor);
        }
        if let Some(timeout) = timeout {
            requisicao = requisicao.timeout(timeout);
        }
        let resposta = requisicao
            .send()
            .await
            .map_err(|e| TransportError::Http(e.to_string()))?;

        if !resposta.status().is_success() {
            return Err(TransportError::Http(format!(
                "status HTTP {}",
                resposta.status()
            )));
        }

        resposta
            .json::<serde_json::Value>()
            .await
            .map_err(|e| TransportError::Http(e.to_string()))
    }

    /// Envia um GET para `url`, sob a mesma política de egresso de
    /// [`Self::post_json`] — decide primeiro se o host é alcançável
    /// (allowlist); aborta antes de qualquer conexão se não for. Devolve o
    /// corpo da resposta como texto bruto, sem assumir nenhum formato (JSON,
    /// texto simples etc.) — a interpretação fica a cargo de quem chama (ex.:
    /// `Settings::from_json_str` no bootstrap de configuração via rede,
    /// MT-42/ADR-0019).
    ///
    /// # Errors
    ///
    /// Mesmos casos de [`Self::post_json`].
    pub async fn get_text(
        &self,
        url: &str,
        task: &str,
        timeout: Option<Duration>,
    ) -> Result<String, TransportError> {
        let parsed = self.authorize(url, task)?;

        let mut requisicao = self.client.get(parsed);
        for (nome, valor) in &self.default_headers {
            requisicao = requisicao.header(nome, valor);
        }
        if let Some(timeout) = timeout {
            requisicao = requisicao.timeout(timeout);
        }
        let resposta = requisicao
            .send()
            .await
            .map_err(|e| TransportError::Http(e.to_string()))?;

        if !resposta.status().is_success() {
            return Err(TransportError::Http(format!(
                "status HTTP {}",
                resposta.status()
            )));
        }

        resposta
            .text()
            .await
            .map_err(|e| TransportError::Http(e.to_string()))
    }

    /// Envia um POST com corpo JSON para `url` e devolve o corpo da resposta
    /// como um fluxo de linhas de texto não vazias, sob a mesma política de
    /// egresso de [`Self::post_json`].
    ///
    /// Cada linha do canal é um trecho do corpo bruto da resposta (ex.: uma
    /// linha NDJSON); o transporte não interpreta o conteúdo — isso é
    /// responsabilidade do adapter que consome o fluxo. Uma tentativa
    /// bloqueada aborta antes de abrir conexão, como em [`Self::post_json`].
    ///
    /// # Errors
    ///
    /// Mesmos casos de [`Self::post_json`]; erros que ocorrem depois de a
    /// conexão ser aberta chegam como um item `Err` no canal devolvido, em
    /// vez de no `Result` externo. `timeout`, quando `Some`, cobre a
    /// requisição inteira — conexão **e** leitura do corpo em streaming, não
    /// só o tempo até o primeiro byte; quem escolhe o valor (Router, MT-17)
    /// deve considerar isso ao decidir o timeout "frio" de troca de modelo.
    pub async fn post_json_lines(
        &self,
        url: &str,
        task: &str,
        body: &serde_json::Value,
        timeout: Option<Duration>,
    ) -> Result<mpsc::Receiver<Result<String, TransportError>>, TransportError> {
        let parsed = self.authorize(url, task)?;

        let mut requisicao = self.client.post(parsed).json(body);
        for (nome, valor) in &self.default_headers {
            requisicao = requisicao.header(nome, valor);
        }
        if let Some(timeout) = timeout {
            requisicao = requisicao.timeout(timeout);
        }
        let mut resposta = requisicao
            .send()
            .await
            .map_err(|e| TransportError::Http(e.to_string()))?;

        if !resposta.status().is_success() {
            return Err(TransportError::Http(format!(
                "status HTTP {}",
                resposta.status()
            )));
        }

        let (tx, rx) = mpsc::channel(16);
        tokio::spawn(async move {
            let mut buffer = String::new();
            loop {
                match resposta.chunk().await {
                    Ok(Some(bytes)) => {
                        buffer.push_str(&String::from_utf8_lossy(&bytes));
                        while let Some(pos) = buffer.find('\n') {
                            let linha = buffer[..pos].trim().to_string();
                            buffer.drain(..=pos);
                            if !linha.is_empty() && tx.send(Ok(linha)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Ok(None) => {
                        let restante = buffer.trim().to_string();
                        if !restante.is_empty() {
                            let _ = tx.send(Ok(restante)).await;
                        }
                        return;
                    }
                    Err(e) => {
                        let _ = tx.send(Err(TransportError::Http(e.to_string()))).await;
                        return;
                    }
                }
            }
        });

        Ok(rx)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use super::*;
    use crate::egress::allowlist::AllowlistEntry;
    use crate::egress::audit::AuditOutcome;

    /// Coletor de auditoria de teste: só acumula as entradas em memória.
    #[derive(Default)]
    struct AuditCollector(Mutex<Vec<AuditEntry>>);

    impl AuditSink for AuditCollector {
        fn record(&self, entry: AuditEntry) {
            self.0
                .lock()
                .expect("mutex do coletor não deve envenenar")
                .push(entry);
        }
    }

    impl AuditCollector {
        fn entries(&self) -> Vec<AuditEntry> {
            self.0
                .lock()
                .expect("mutex do coletor não deve envenenar")
                .clone()
        }
    }

    /// Sobe um servidor HTTP mínimo em `127.0.0.1` (porta efêmera) que sempre
    /// responde `200 OK` com o corpo dado. Implementado só com `tokio::net`
    /// (já dependência do crate) para não introduzir uma lib de mock HTTP
    /// nova — e a verificação de maturidade/licença que isso exigiria pelo
    /// ADR-0004 — só para devolver uma resposta fixa em teste.
    async fn start_mock_server(
        response_body: &'static str,
    ) -> (std::net::SocketAddr, Arc<AtomicUsize>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");
        let conexoes = Arc::new(AtomicUsize::new(0));
        let contador = Arc::clone(&conexoes);

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
                contador.fetch_add(1, Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut buf = [0u8; 1024];
                    let _ = socket.read(&mut buf).await;
                    let resposta = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\
                         Content-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    );
                    let _ = socket.write_all(resposta.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });

        (addr, conexoes)
    }

    #[tokio::test]
    async fn chamada_permitida_alcanca_o_servidor_e_devolve_o_corpo() {
        let (addr, conexoes) = start_mock_server(r#"{"ok":true}"#).await;
        let host = addr.ip().to_string();
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            host.as_str(),
            EgressClass::CloudOk,
        )]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(
            allowlist,
            EgressClass::CloudOk,
            Some("pessoal".into()),
            sink.clone(),
        );

        let url = format!("http://{addr}/chat");
        let resposta = transport
            .post_json(
                &url,
                "chat de teste",
                &serde_json::json!({"oi": "mundo"}),
                None,
            )
            .await
            .expect("chamada permitida deve passar");

        assert_eq!(resposta, serde_json::json!({"ok": true}));
        assert_eq!(
            conexoes.load(Ordering::SeqCst),
            1,
            "o mock deve ter recebido a conexão"
        );

        let entradas = sink.entries();
        assert_eq!(entradas.len(), 1);
        assert_eq!(entradas[0].outcome, AuditOutcome::Allowed);
    }

    #[tokio::test]
    async fn chamada_bloqueada_por_classe_insuficiente_aborta_sem_tocar_a_rede() {
        let (addr, conexoes) = start_mock_server(r#"{"ok":true}"#).await;
        let host = addr.ip().to_string();
        // A allowlist cadastra o host, mas exige uma classe que a sessão não tem.
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            host.as_str(),
            EgressClass::CloudOk,
        )]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(
            allowlist,
            EgressClass::LocalOnly,
            Some("empresa".into()),
            sink.clone(),
        );

        let url = format!("http://{addr}/chat");
        let erro = transport
            .post_json(
                &url,
                "chat de teste",
                &serde_json::json!({"oi": "mundo"}),
                None,
            )
            .await
            .expect_err("chamada bloqueada deve abortar");

        assert!(matches!(erro, TransportError::Blocked(_)));
        assert_eq!(
            conexoes.load(Ordering::SeqCst),
            0,
            "nenhuma conexão deve ter sido aberta para um destino bloqueado"
        );

        let entradas = sink.entries();
        assert_eq!(entradas.len(), 1);
        assert_eq!(entradas[0].outcome, AuditOutcome::Blocked);
    }

    #[tokio::test]
    async fn host_fora_da_allowlist_tambem_aborta_sem_tocar_a_rede() {
        let allowlist = Allowlist::new(vec![]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(allowlist, EgressClass::CloudOk, None, sink);

        let erro = transport
            .post_json(
                "http://127.0.0.1:9/chat",
                "tarefa",
                &serde_json::json!({}),
                None,
            )
            .await
            .expect_err("host não cadastrado deve abortar");
        assert!(matches!(
            erro,
            TransportError::Blocked(EgressError::NotAllowlisted { .. })
        ));
    }

    #[tokio::test]
    async fn get_text_alcanca_o_servidor_e_devolve_o_corpo_bruto() {
        let (addr, conexoes) = start_mock_server(r#"{"schemaVersion":1}"#).await;
        let host = addr.ip().to_string();
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            host.as_str(),
            EgressClass::CloudOk,
        )]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(
            allowlist,
            EgressClass::CloudOk,
            Some("init".into()),
            sink.clone(),
        );

        let url = format!("http://{addr}/perfil.json");
        let corpo = transport
            .get_text(&url, "init de teste", None)
            .await
            .expect("GET permitido deve passar");

        assert_eq!(corpo, r#"{"schemaVersion":1}"#);
        assert_eq!(conexoes.load(Ordering::SeqCst), 1);

        let entradas = sink.entries();
        assert_eq!(entradas.len(), 1);
        assert_eq!(entradas[0].outcome, AuditOutcome::Allowed);
    }

    #[tokio::test]
    async fn get_text_bloqueado_por_classe_insuficiente_aborta_sem_tocar_a_rede() {
        let (addr, conexoes) = start_mock_server(r#"{"schemaVersion":1}"#).await;
        let host = addr.ip().to_string();
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            host.as_str(),
            EgressClass::CloudOk,
        )]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(allowlist, EgressClass::LocalOnly, None, sink.clone());

        let url = format!("http://{addr}/perfil.json");
        let erro = transport
            .get_text(&url, "init de teste", None)
            .await
            .expect_err("classe insuficiente deve abortar");

        assert!(matches!(erro, TransportError::Blocked(_)));
        assert_eq!(
            conexoes.load(Ordering::SeqCst),
            0,
            "nenhuma conexão deve ter sido aberta"
        );
    }

    #[tokio::test]
    async fn post_json_lines_entrega_linhas_nao_vazias_em_ordem() {
        let corpo = "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n";
        let (addr, _conexoes) = start_mock_server(corpo).await;
        let host = addr.ip().to_string();
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            host.as_str(),
            EgressClass::CloudOk,
        )]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(allowlist, EgressClass::CloudOk, None, sink);

        let url = format!("http://{addr}/chat");
        let mut linhas = transport
            .post_json_lines(&url, "stream de teste", &serde_json::json!({}), None)
            .await
            .expect("stream permitido deve abrir");

        let mut recebidas = Vec::new();
        while let Some(linha) = linhas.recv().await {
            recebidas.push(linha.expect("mock não produz erro de transporte"));
        }
        assert_eq!(recebidas, vec!["{\"a\":1}", "{\"a\":2}", "{\"a\":3}"]);
    }

    /// Como [`start_mock_server`], mas só responde depois de `atraso` — usado
    /// para provar que o timeout por chamada (MT-17, ADR-0009) é aplicado de
    /// verdade.
    async fn start_mock_server_lento(
        response_body: &'static str,
        atraso: std::time::Duration,
    ) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");

        tokio::spawn(async move {
            if let Ok((mut socket, _)) = listener.accept().await {
                let mut buf = [0u8; 1024];
                let _ = socket.read(&mut buf).await;
                tokio::time::sleep(atraso).await;
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

    #[tokio::test]
    async fn timeout_curto_aborta_chamada_lenta() {
        let addr = start_mock_server_lento(r#"{"ok":true}"#, Duration::from_millis(300)).await;
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            addr.ip().to_string(),
            EgressClass::CloudOk,
        )]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(allowlist, EgressClass::CloudOk, None, sink);

        let url = format!("http://{addr}/chat");
        let erro = transport
            .post_json(
                &url,
                "chat de teste",
                &serde_json::json!({}),
                Some(Duration::from_millis(30)),
            )
            .await
            .expect_err("timeout curto deve abortar a chamada lenta");

        assert!(matches!(erro, TransportError::Http(_)));
    }

    #[tokio::test]
    async fn timeout_longo_permite_chamada_lenta_terminar() {
        let addr = start_mock_server_lento(r#"{"ok":true}"#, Duration::from_millis(50)).await;
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            addr.ip().to_string(),
            EgressClass::CloudOk,
        )]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(allowlist, EgressClass::CloudOk, None, sink);

        let url = format!("http://{addr}/chat");
        let resposta = transport
            .post_json(
                &url,
                "chat de teste",
                &serde_json::json!({}),
                Some(Duration::from_secs(5)),
            )
            .await
            .expect("timeout longo deve permitir a chamada terminar");

        assert_eq!(resposta, serde_json::json!({"ok": true}));
    }

    /// Como [`start_mock_server`], mas também captura os bytes brutos da
    /// primeira requisição recebida — usado para provar que um header foi
    /// realmente enviado (MT-15/16, `with_header`).
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

    #[tokio::test]
    async fn with_header_anexa_o_header_dado_a_toda_requisicao() {
        let (addr, capturado) = start_mock_server_capturando(r#"{"ok":true}"#).await;
        let host = addr.ip().to_string();
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            host.as_str(),
            EgressClass::CloudOk,
        )]);
        let sink = Arc::new(AuditCollector::default());
        let transport = Transport::new(allowlist, EgressClass::CloudOk, None, sink)
            .with_header("Authorization", "Bearer chave-secreta")
            .with_header("x-api-key", "outra-chave")
            .with_header("anthropic-version", "2023-06-01");

        let url = format!("http://{addr}/chat");
        transport
            .post_json(&url, "chat de teste", &serde_json::json!({}), None)
            .await
            .expect("chamada permitida deve passar");

        let requisicao_bruta =
            String::from_utf8_lossy(&capturado.lock().expect("mutex não deve envenenar"))
                .into_owned();
        let em_minusculas = requisicao_bruta.to_lowercase();
        assert!(
            em_minusculas.contains("authorization: bearer chave-secreta"),
            "esperava header Authorization: Bearer na requisição; recebido:\n{requisicao_bruta}"
        );
        assert!(
            em_minusculas.contains("x-api-key: outra-chave"),
            "esperava header x-api-key na requisição; recebido:\n{requisicao_bruta}"
        );
        assert!(
            em_minusculas.contains("anthropic-version: 2023-06-01"),
            "esperava header anthropic-version na requisição; recebido:\n{requisicao_bruta}"
        );
    }

    #[test]
    fn reqwest_e_usado_somente_no_modulo_de_transporte() {
        let src_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut arquivos_com_reqwest = Vec::new();
        visitar_arquivos_rs(&src_dir, &mut |caminho, conteudo| {
            if conteudo.contains("reqwest::") {
                arquivos_com_reqwest.push(caminho.to_path_buf());
            }
        });

        let permitido = src_dir.join("transport").join("mod.rs");
        assert!(
            !arquivos_com_reqwest.is_empty(),
            "esperava encontrar uso de reqwest:: em {}",
            permitido.display()
        );
        assert!(
            arquivos_com_reqwest
                .iter()
                .all(|caminho| caminho == &permitido),
            "reqwest:: só pode ser usado em {}; também encontrado em: {:?}",
            permitido.display(),
            arquivos_com_reqwest
        );
    }

    fn visitar_arquivos_rs(
        dir: &std::path::Path,
        visitante: &mut dyn FnMut(&std::path::Path, &str),
    ) {
        for entrada in std::fs::read_dir(dir).expect("diretório src deve existir") {
            let entrada = entrada.expect("entrada de diretório legível");
            let caminho = entrada.path();
            if caminho.is_dir() {
                visitar_arquivos_rs(&caminho, visitante);
            } else if caminho.extension().is_some_and(|ext| ext == "rs") {
                let conteudo =
                    std::fs::read_to_string(&caminho).expect("arquivo .rs deve ser UTF-8 legível");
                visitante(&caminho, &conteudo);
            }
        }
    }
}
