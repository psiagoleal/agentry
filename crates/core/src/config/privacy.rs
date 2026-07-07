// Caminho relativo: crates/core/src/config/privacy.rs
//! Resolução perfil → classe de egresso (`privacy-taxonomy:1`, ADR-0002).
//!
//! O mapa perfil→classe é **ratificado** pelo contrato de interop v1 (SPEC §2.1,
//! canônico no repo `ai-coding-agent-profiles`) e não pode ser alterado sem novo
//! ADR + entrada no exchange-log. Regra central: **fail-closed** — perfil
//! ausente, desconhecido ou ambíguo resolve para [`EgressClass::LocalOnly`].

use serde::{Deserialize, Serialize};

/// Perfil de trabalho definido pelo `ai-coding-agent-profiles`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Profile {
    /// Trabalho corporativo: dados confidenciais por padrão.
    Empresa,
    /// Trabalho externo com confidencialidade contratual.
    ExternoConfidencial,
    /// Projetos pessoais/open-source.
    Pessoal,
}

impl Profile {
    /// Interpreta o identificador textual do perfil (`empresa`,
    /// `externo-confidencial`, `pessoal`).
    ///
    /// Reconhecimento **estrito** (apenas espaços nas bordas são tolerados):
    /// qualquer outra grafia é tratada como perfil desconhecido (`None`), o que
    /// leva o chamador ao fail-closed.
    #[must_use]
    pub fn parse(texto: &str) -> Option<Self> {
        match texto.trim() {
            "empresa" => Some(Self::Empresa),
            "externo-confidencial" => Some(Self::ExternoConfidencial),
            "pessoal" => Some(Self::Pessoal),
            _ => None,
        }
    }

    /// Classe de egresso ratificada para este perfil (SPEC §2.1).
    #[must_use]
    pub fn egress_class(self) -> EgressClass {
        match self {
            Self::Empresa => EgressClass::LocalOnly,
            Self::ExternoConfidencial => EgressClass::CloudOptOut,
            Self::Pessoal => EgressClass::CloudOk,
        }
    }
}

/// Classe de egresso de rede (ADR-0002): o que o transporte pode alcançar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EgressClass {
    /// Egresso para nuvem proibido; só endpoints on-premise/aprovados.
    LocalOnly,
    /// Nuvem só com opt-out de retenção comprovado + allowlist.
    CloudOptOut,
    /// APIs de nuvem livres (bom senso de custo).
    CloudOk,
}

impl EgressClass {
    /// Resolve a classe a partir do identificador de perfil, **fail-closed**:
    /// `None` ou perfil desconhecido ⇒ [`EgressClass::LocalOnly`].
    #[must_use]
    pub fn resolve(profile: Option<&str>) -> Self {
        profile
            .and_then(Profile::parse)
            .map_or(Self::LocalOnly, Profile::egress_class)
    }

    /// Posição desta classe na ordem de **permissividade** crescente
    /// (`local-only` < `cloud-opt-out` < `cloud-ok`).
    ///
    /// Esta ordenação é uma interpretação interna do `agentry` sobre a
    /// taxonomia do SPEC §2.1 (não é, em si, parte do contrato de interop) e
    /// serve só para decidir, em memória, se a classe ativa de uma sessão
    /// cobre a classe mínima exigida por um destino (ver [`Self::permits`]).
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Self::LocalOnly => 0,
            Self::CloudOptOut => 1,
            Self::CloudOk => 2,
        }
    }

    /// Indica se esta classe (a classe ativa de uma sessão) é permissiva o
    /// bastante para alcançar um destino que exige, no mínimo, `required`.
    #[must_use]
    pub fn permits(self, required: Self) -> bool {
        self.rank() >= required.rank()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empresa_resolve_para_local_only() {
        assert_eq!(
            EgressClass::resolve(Some("empresa")),
            EgressClass::LocalOnly
        );
    }

    #[test]
    fn externo_confidencial_resolve_para_cloud_opt_out() {
        assert_eq!(
            EgressClass::resolve(Some("externo-confidencial")),
            EgressClass::CloudOptOut
        );
    }

    #[test]
    fn pessoal_resolve_para_cloud_ok() {
        assert_eq!(EgressClass::resolve(Some("pessoal")), EgressClass::CloudOk);
    }

    #[test]
    fn perfil_ausente_falha_fechado_em_local_only() {
        assert_eq!(EgressClass::resolve(None), EgressClass::LocalOnly);
    }

    #[test]
    fn perfil_desconhecido_ou_ambiguo_falha_fechado_em_local_only() {
        for ambiguo in ["corporativo", "EMPRESA", "pessoal-e-empresa", "", "  "] {
            assert_eq!(
                EgressClass::resolve(Some(ambiguo)),
                EgressClass::LocalOnly,
                "perfil {ambiguo:?} deveria falhar fechado"
            );
        }
    }

    #[test]
    fn parse_tolera_apenas_espacos_nas_bordas() {
        assert_eq!(Profile::parse("  pessoal "), Some(Profile::Pessoal));
        assert_eq!(Profile::parse("pes soal"), None);
    }

    #[test]
    fn permits_reflete_a_ordem_de_permissividade() {
        assert!(EgressClass::LocalOnly.permits(EgressClass::LocalOnly));
        assert!(!EgressClass::LocalOnly.permits(EgressClass::CloudOptOut));
        assert!(!EgressClass::LocalOnly.permits(EgressClass::CloudOk));

        assert!(EgressClass::CloudOptOut.permits(EgressClass::LocalOnly));
        assert!(EgressClass::CloudOptOut.permits(EgressClass::CloudOptOut));
        assert!(!EgressClass::CloudOptOut.permits(EgressClass::CloudOk));

        assert!(EgressClass::CloudOk.permits(EgressClass::LocalOnly));
        assert!(EgressClass::CloudOk.permits(EgressClass::CloudOptOut));
        assert!(EgressClass::CloudOk.permits(EgressClass::CloudOk));
    }

    #[test]
    fn formato_serde_em_kebab_case() {
        assert_eq!(
            serde_json::to_value(Profile::ExternoConfidencial).unwrap(),
            serde_json::json!("externo-confidencial")
        );
        assert_eq!(
            serde_json::to_value(EgressClass::LocalOnly).unwrap(),
            serde_json::json!("local-only")
        );
    }
}
