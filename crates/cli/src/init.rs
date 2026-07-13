// Caminho relativo: crates/cli/src/init.rs
//! Bootstrap via rede de `.agentry/agentry.settings.json` com `--profile`
//! (MT-42, ADR-0019) — busca só o **dado** (um GET do JSON), nunca um script
//! para executar.
//!
//! A busca passa pelo mesmo [`agentry_core::transport::Transport`] central
//! (ADR-0002), numa instância dedicada ao bootstrap: `Allowlist` restrita a
//! um único host fixo ([`RAW_GITHUB_HOST`]) e [`EgressClass::CloudOk`] — nem
//! a classe de egresso do perfil-alvo, nem herdada de nenhuma sessão (ainda
//! não existe nenhuma resolvida neste ponto do processo). A referência
//! ([`PROFILES_REPO_REF`]) é fixa, gravada aqui como constante — nunca
//! resolvida contra "latest" (ADR-0019 §4).
//!
//! O artefato obtido é validado com [`Settings::from_json_str`]
//! (`schemaVersion`) antes de ser aceito — nome de perfil desconhecido é
//! erro tratado **antes** de qualquer chamada de rede; falha de rede ou
//! schema incompatível nunca cai silenciosamente no exemplo genérico do
//! MT-41 (isso é responsabilidade de quem chama esta função, não desta).

use std::sync::Arc;

use agentry_core::config::privacy::EgressClass;
use agentry_core::config::{ConfigError, Settings};
use agentry_core::egress::allowlist::{Allowlist, AllowlistEntry};
use agentry_core::transport::{AuditSink, Transport, TransportError};

/// Host oficial de conteúdo bruto do GitHub — único host permitido na
/// `Allowlist` dedicada ao bootstrap via rede (ADR-0019 §3).
const RAW_GITHUB_HOST: &str = "raw.githubusercontent.com";
/// Referência (commit) fixa do `ai-coding-agent-profiles` usada pelo
/// bootstrap via `--profile` — atualizada manualmente a cada *bump*
/// deliberado (ADR-0019 §4), nunca resolvida contra "latest".
const PROFILES_REPO_REF: &str = "d3ed413fbfcbb83da268bef540b924c26e2c3a2f";
/// Perfis reconhecidos pelo `ai-coding-agent-profiles` — mesma lista usada
/// pela taxonomia de privacidade (`config::privacy::Profile`, ADR-0002).
const PERFIS_CONHECIDOS: [&str; 3] = ["empresa", "externo-confidencial", "pessoal"];

/// Erros do bootstrap via rede.
#[derive(Debug)]
pub enum InitError {
    /// `--profile` recebeu um nome fora de [`PERFIS_CONHECIDOS`] — nenhuma
    /// chamada de rede é feita neste caso.
    PerfilDesconhecido(String),
    /// Falha na chamada de rede em si (allowlist, HTTP, timeout).
    Rede(TransportError),
    /// O artefato obtido não é JSON válido ou declara `schemaVersion`
    /// incompatível — nunca gravado em disco.
    Schema(ConfigError),
}

impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PerfilDesconhecido(nome) => write!(
                f,
                "perfil desconhecido: '{nome}' (esperado um de {PERFIS_CONHECIDOS:?})"
            ),
            Self::Rede(erro) => write!(f, "falha ao buscar o perfil: {erro}"),
            Self::Schema(erro) => write!(f, "artefato do perfil inválido: {erro}"),
        }
    }
}

impl std::error::Error for InitError {}

/// Busca `.agentry/agentry.settings.json` de `perfil` no
/// `ai-coding-agent-profiles` público, na referência pinada
/// ([`PROFILES_REPO_REF`]). Devolve o texto **bruto** do JSON (não
/// reserializado) — já validado (`schemaVersion`), mas preservando
/// exatamente o que o `profiles` publicou.
///
/// # Errors
///
/// Ver [`InitError`].
pub(crate) async fn fetch_profile_settings(
    perfil: &str,
    sink: Arc<dyn AuditSink>,
) -> Result<String, InitError> {
    let base_url = format!(
        "https://{RAW_GITHUB_HOST}/psiagoleal/ai-coding-agent-profiles/{PROFILES_REPO_REF}"
    );
    fetch_profile_settings_de(perfil, &base_url, RAW_GITHUB_HOST, sink).await
}

/// Núcleo testável de [`fetch_profile_settings`]: `base_url`/`host_permitido`
/// são injetados para que os testes apontem para um servidor local, em vez
/// do GitHub real.
async fn fetch_profile_settings_de(
    perfil: &str,
    base_url: &str,
    host_permitido: &str,
    sink: Arc<dyn AuditSink>,
) -> Result<String, InitError> {
    if !PERFIS_CONHECIDOS.contains(&perfil) {
        return Err(InitError::PerfilDesconhecido(perfil.to_string()));
    }

    let url = format!("{base_url}/profiles/{perfil}/.agentry/agentry.settings.json");
    let allowlist = Allowlist::new(vec![AllowlistEntry::new(
        host_permitido,
        EgressClass::CloudOk,
    )]);
    let transport = Transport::new(
        allowlist,
        EgressClass::CloudOk,
        Some("init".to_string()),
        sink,
    );

    let texto = transport
        .get_text(&url, "init --profile", None)
        .await
        .map_err(InitError::Rede)?;

    Settings::from_json_str(&texto).map_err(InitError::Schema)?;

    Ok(texto)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::egress::audit::AuditEntry;
    use std::sync::Mutex;

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

    /// Sobe um servidor HTTP mínimo respondendo sempre `200 OK` com o corpo
    /// dado — mesma técnica (só `tokio::net`) já usada em
    /// `agentry_core::transport::tests`, para não introduzir dependência de
    /// mock HTTP nova.
    async fn start_mock_server(response_body: &'static str) -> std::net::SocketAddr {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");

        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    break;
                };
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

        addr
    }

    /// Como [`start_mock_server`], mas nunca aceita conexão (porta fechada
    /// logo em seguida) — usado para simular host inalcançável.
    async fn endereco_inalcancavel() -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind em porta efêmera deve funcionar");
        let addr = listener
            .local_addr()
            .expect("socket deve ter endereço local");
        drop(listener); // fecha a porta: conexões subsequentes são recusadas
        addr
    }

    #[tokio::test]
    async fn perfil_desconhecido_e_erro_tratado_antes_de_qualquer_rede() {
        let sink = Arc::new(AuditCollector::default());

        let erro =
            fetch_profile_settings_de("producao", "http://127.0.0.1:9", "127.0.0.1", sink.clone())
                .await
                .expect_err("perfil desconhecido deve ser rejeitado");

        assert!(matches!(erro, InitError::PerfilDesconhecido(_)));
        assert!(
            sink.0.lock().unwrap().is_empty(),
            "nenhuma tentativa de rede deve ter sido registrada"
        );
    }

    #[tokio::test]
    async fn servidor_respondendo_json_valido_e_aceito() {
        let json_valido = r#"{"schemaVersion":1,"permissions":{"deny":["shell_exec"],"ask":[]}}"#;
        let addr = start_mock_server(json_valido).await;
        let base_url = format!("http://{addr}");
        let sink = Arc::new(AuditCollector::default());

        let texto =
            fetch_profile_settings_de("empresa", &base_url, addr.ip().to_string().as_str(), sink)
                .await
                .expect("JSON válido deve ser aceito");

        assert_eq!(texto, json_valido);
    }

    #[tokio::test]
    async fn servidor_respondendo_schema_incompativel_e_erro_tratado() {
        let json_incompativel = r#"{"schemaVersion":2}"#;
        let addr = start_mock_server(json_incompativel).await;
        let base_url = format!("http://{addr}");
        let sink = Arc::new(AuditCollector::default());

        let erro =
            fetch_profile_settings_de("empresa", &base_url, addr.ip().to_string().as_str(), sink)
                .await
                .expect_err("schemaVersion incompatível deve ser rejeitado");

        assert!(matches!(erro, InitError::Schema(_)));
    }

    #[tokio::test]
    async fn host_inalcancavel_e_erro_tratado_nunca_cai_no_exemplo_generico() {
        let addr = endereco_inalcancavel().await;
        let base_url = format!("http://{addr}");
        let sink = Arc::new(AuditCollector::default());

        let erro =
            fetch_profile_settings_de("pessoal", &base_url, addr.ip().to_string().as_str(), sink)
                .await
                .expect_err("host inalcançável deve ser erro tratado, nunca sucesso silencioso");

        assert!(matches!(erro, InitError::Rede(_)));
    }

    #[tokio::test]
    async fn host_fora_da_allowlist_dedicada_aborta_sem_tocar_a_rede() {
        // Mesma prova do teste-guarda do MT-07 (`Transport` é o único ponto
        // de rede): host fora da allowlist dedicada ao bootstrap nunca
        // alcança o servidor, mesmo que o servidor esteja de pé e correto.
        let json_valido = r#"{"schemaVersion":1}"#;
        let addr = start_mock_server(json_valido).await;
        let base_url = format!("http://{addr}");
        let sink = Arc::new(AuditCollector::default());

        let erro =
            fetch_profile_settings_de("pessoal", &base_url, "host-nao-cadastrado.invalido", sink)
                .await
                .expect_err("host fora da allowlist dedicada deve abortar");

        assert!(matches!(erro, InitError::Rede(TransportError::Blocked(_))));
    }
}
