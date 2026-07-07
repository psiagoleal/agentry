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

use std::sync::Arc;

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
}

impl Transport {
    /// Cria um transporte com a política de egresso e o coletor de
    /// auditoria dados.
    #[must_use]
    pub fn new(
        client: reqwest::Client,
        allowlist: Allowlist,
        egress_class: EgressClass,
        profile: Option<String>,
        sink: Arc<dyn AuditSink>,
    ) -> Self {
        Self {
            client,
            allowlist,
            egress_class,
            profile,
            sink,
        }
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
    /// rede falhar ou devolver status de erro.
    pub async fn post_json(
        &self,
        url: &str,
        task: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, TransportError> {
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

        let resposta = self
            .client
            .post(parsed)
            .json(body)
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
            reqwest::Client::new(),
            allowlist,
            EgressClass::CloudOk,
            Some("pessoal".into()),
            sink.clone(),
        );

        let url = format!("http://{addr}/chat");
        let resposta = transport
            .post_json(&url, "chat de teste", &serde_json::json!({"oi": "mundo"}))
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
            reqwest::Client::new(),
            allowlist,
            EgressClass::LocalOnly,
            Some("empresa".into()),
            sink.clone(),
        );

        let url = format!("http://{addr}/chat");
        let erro = transport
            .post_json(&url, "chat de teste", &serde_json::json!({"oi": "mundo"}))
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
        let transport = Transport::new(
            reqwest::Client::new(),
            allowlist,
            EgressClass::CloudOk,
            None,
            sink,
        );

        let erro = transport
            .post_json("http://127.0.0.1:9/chat", "tarefa", &serde_json::json!({}))
            .await
            .expect_err("host não cadastrado deve abortar");
        assert!(matches!(
            erro,
            TransportError::Blocked(EgressError::NotAllowlisted { .. })
        ));
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
