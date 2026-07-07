// Caminho relativo: crates/core/src/egress/allowlist.rs
//! Allowlist de endpoints por classe de egresso (MT-05, ADR-0002).
//!
//! Decide, **sem tocar em rede**, se um host pode ser alcançado sob a classe
//! de egresso ativa de uma sessão: só hosts explicitamente cadastrados aqui
//! são alcançáveis, e mesmo cadastrados só o são se a classe ativa cobrir a
//! classe mínima que o host exige. Toda ambiguidade resolve para o lado mais
//! restritivo — **fail-closed** nunca degrada confidencialidade em silêncio.

use crate::config::privacy::EgressClass;

/// Uma entrada da allowlist: host aprovado e a classe mínima que ele exige.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AllowlistEntry {
    /// Host exato (`api.exemplo.com`) ou padrão de subdomínio (`*.exemplo.com`).
    pub host: String,
    /// Classe de egresso mínima que a sessão precisa ter para alcançar este host.
    pub required_class: EgressClass,
}

impl AllowlistEntry {
    /// Cria uma entrada de allowlist.
    #[must_use]
    pub fn new(host: impl Into<String>, required_class: EgressClass) -> Self {
        Self {
            host: host.into(),
            required_class,
        }
    }

    /// Indica se esta entrada casa com `host` (match exato, ou padrão
    /// `*.sufixo` casando qualquer subdomínio — nunca o domínio nu).
    fn matches(&self, host: &str) -> bool {
        match self.host.strip_prefix("*.") {
            Some(sufixo) => {
                host.len() > sufixo.len()
                    && host.ends_with(sufixo)
                    && host.as_bytes()[host.len() - sufixo.len() - 1] == b'.'
            }
            None => self.host == host,
        }
    }
}

/// Allowlist de endpoints: decisão em memória sobre alcançabilidade de hosts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowlist(Vec<AllowlistEntry>);

impl Allowlist {
    /// Cria uma allowlist a partir das entradas dadas.
    #[must_use]
    pub fn new(entries: Vec<AllowlistEntry>) -> Self {
        Self(entries)
    }

    /// Decide se `host` pode ser alcançado sob `active_class`.
    ///
    /// Quando várias entradas casam com o mesmo host exigindo classes
    /// diferentes, prevalece a **mais restritiva** (maior [`EgressClass::rank`]):
    /// a decisão nunca é mais permissiva do que a entrada mais exigente
    /// cadastrada para aquele host.
    ///
    /// # Errors
    ///
    /// Devolve [`EgressError::NotAllowlisted`] se nenhuma entrada casar com o
    /// host, e [`EgressError::ClassInsufficient`] se a classe ativa não cobrir
    /// a classe mínima exigida.
    pub fn check(&self, active_class: EgressClass, host: &str) -> Result<(), EgressError> {
        let required = self
            .0
            .iter()
            .filter(|entry| entry.matches(host))
            .map(|entry| entry.required_class)
            .max_by_key(|class| class.rank());

        let Some(required) = required else {
            return Err(EgressError::NotAllowlisted { host: host.into() });
        };

        if active_class.permits(required) {
            Ok(())
        } else {
            Err(EgressError::ClassInsufficient {
                host: host.into(),
                active: active_class,
                required,
            })
        }
    }
}

/// Erro de decisão de egresso — sempre fail-closed (nunca "talvez permitido").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EgressError {
    /// Host não cadastrado na allowlist.
    NotAllowlisted {
        /// Host solicitado.
        host: String,
    },
    /// Host cadastrado, mas a classe ativa não é suficiente.
    ClassInsufficient {
        /// Host solicitado.
        host: String,
        /// Classe ativa da sessão.
        active: EgressClass,
        /// Classe mínima exigida pela entrada mais restritiva que casa com o host.
        required: EgressClass,
    },
}

impl core::fmt::Display for EgressError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::NotAllowlisted { host } => {
                write!(f, "host '{host}' fora da allowlist; egresso bloqueado")
            }
            Self::ClassInsufficient {
                host,
                active,
                required,
            } => write!(
                f,
                "host '{host}' exige classe {required:?}, mas a sessão está \
                 em {active:?}; egresso bloqueado"
            ),
        }
    }
}

impl std::error::Error for EgressError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_fora_da_allowlist_e_erro() {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            "ollama.local",
            EgressClass::LocalOnly,
        )]);

        let erro = allowlist
            .check(EgressClass::CloudOk, "api.anthropic.com")
            .expect_err("host não cadastrado deve ser rejeitado");
        assert_eq!(
            erro,
            EgressError::NotAllowlisted {
                host: "api.anthropic.com".into()
            }
        );
    }

    #[test]
    fn local_only_rejeita_host_de_nuvem() {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            "api.anthropic.com",
            EgressClass::CloudOk,
        )]);

        let erro = allowlist
            .check(EgressClass::LocalOnly, "api.anthropic.com")
            .expect_err("local-only não pode alcançar host cloud-ok");
        assert_eq!(
            erro,
            EgressError::ClassInsufficient {
                host: "api.anthropic.com".into(),
                active: EgressClass::LocalOnly,
                required: EgressClass::CloudOk,
            }
        );
    }

    #[test]
    fn local_only_aceita_host_on_premise() {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            "ollama.local",
            EgressClass::LocalOnly,
        )]);

        allowlist
            .check(EgressClass::LocalOnly, "ollama.local")
            .expect("host local-only deve ser alcançável em sessão local-only");
    }

    #[test]
    fn cloud_ok_alcanca_hosts_de_qualquer_classe_cadastrada() {
        let allowlist = Allowlist::new(vec![
            AllowlistEntry::new("ollama.local", EgressClass::LocalOnly),
            AllowlistEntry::new("gateway.exemplo.com", EgressClass::CloudOptOut),
            AllowlistEntry::new("api.anthropic.com", EgressClass::CloudOk),
        ]);

        for host in ["ollama.local", "gateway.exemplo.com", "api.anthropic.com"] {
            allowlist
                .check(EgressClass::CloudOk, host)
                .unwrap_or_else(|_| panic!("cloud-ok deveria alcançar {host}"));
        }
    }

    #[test]
    fn cloud_opt_out_rejeita_host_que_exige_cloud_ok() {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            "api.anthropic.com",
            EgressClass::CloudOk,
        )]);

        let erro = allowlist
            .check(EgressClass::CloudOptOut, "api.anthropic.com")
            .expect_err("cloud-opt-out não cobre exigência cloud-ok");
        assert!(matches!(erro, EgressError::ClassInsufficient { .. }));
    }

    #[test]
    fn entradas_conflitantes_para_o_mesmo_host_falham_fechado_na_mais_restritiva() {
        // Mesmo host cadastrado duas vezes com classes diferentes: a decisão
        // nunca deve ser mais branda do que a entrada mais exigente.
        let allowlist = Allowlist::new(vec![
            AllowlistEntry::new("gateway.interno", EgressClass::LocalOnly),
            AllowlistEntry::new("gateway.interno", EgressClass::CloudOk),
        ]);

        allowlist
            .check(EgressClass::CloudOk, "gateway.interno")
            .expect("cloud-ok cobre a entrada mais restritiva");

        let erro = allowlist
            .check(EgressClass::LocalOnly, "gateway.interno")
            .expect_err("local-only não cobre a entrada mais restritiva cadastrada");
        assert_eq!(
            erro,
            EgressError::ClassInsufficient {
                host: "gateway.interno".into(),
                active: EgressClass::LocalOnly,
                required: EgressClass::CloudOk,
            }
        );
    }

    #[test]
    fn wildcard_de_subdominio_nao_casa_dominio_nu_nem_host_nao_relacionado() {
        let allowlist = Allowlist::new(vec![AllowlistEntry::new(
            "*.exemplo.com",
            EgressClass::CloudOk,
        )]);

        allowlist
            .check(EgressClass::CloudOk, "api.exemplo.com")
            .expect("subdomínio deve casar com o padrão");

        assert_eq!(
            allowlist.check(EgressClass::CloudOk, "exemplo.com"),
            Err(EgressError::NotAllowlisted {
                host: "exemplo.com".into()
            }),
            "domínio nu não deve casar com *.exemplo.com"
        );
        assert_eq!(
            allowlist.check(EgressClass::CloudOk, "outroexemplo.com"),
            Err(EgressError::NotAllowlisted {
                host: "outroexemplo.com".into()
            }),
            "sufixo textual sem o ponto de subdomínio não deve casar"
        );
    }
}
