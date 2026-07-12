// Caminho relativo: crates/core/src/egress/audit.rs
//! Entrada de auditoria estruturada de egresso (MT-06, ADR-0002).
//!
//! Cada tentativa de egresso — permitida ou bloqueada — deve produzir uma
//! [`AuditEntry`] com os campos exigidos pelo ADR-0002: destino, perfil,
//! classe de egresso e tarefa. Este módulo define só a estrutura e a
//! emissão (serialização); persistência/transmissão do log ficam para o
//! MT-07, que integra tudo no transporte único.
//!
//! Todo campo textual passa por [`redact_text`] em [`AuditEntry::new`]:
//! nenhum chamador precisa lembrar de redigir manualmente antes de logar.

use serde::Serialize;

use super::redact::redact_text;
use crate::config::privacy::EgressClass;

/// Resultado de uma decisão de egresso, para o audit trail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    /// O egresso foi permitido e prosseguiu.
    Allowed,
    /// O egresso foi bloqueado (fora da allowlist ou classe insuficiente).
    Blocked,
}

/// Entrada de auditoria estruturada de uma tentativa de egresso.
///
/// Campos exigidos pelo ADR-0002: `destination` (destino), `profile`
/// (perfil), `egress_class` (classe) e `task` (tarefa) — sempre presentes,
/// mesmo quando o perfil é `None` (sessão sem perfil resolvido).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditEntry {
    /// Destino do egresso (host ou endpoint), já redigido.
    pub destination: String,
    /// Identificador do perfil ativo, se houver.
    pub profile: Option<String>,
    /// Classe de egresso resolvida para a sessão.
    pub egress_class: EgressClass,
    /// Descrição da tarefa que originou a tentativa de egresso, já redigida.
    pub task: String,
    /// Resultado da decisão.
    pub outcome: AuditOutcome,
    /// Motivo do bloqueio, se houver (já redigido).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl AuditEntry {
    /// Cria uma entrada de auditoria, redigindo automaticamente `destination`,
    /// `task` e `reason` antes de armazená-los.
    #[must_use]
    pub fn new(
        destination: impl Into<String>,
        profile: Option<String>,
        egress_class: EgressClass,
        task: impl Into<String>,
        outcome: AuditOutcome,
        reason: Option<String>,
    ) -> Self {
        Self {
            destination: redact_text(&destination.into()),
            profile,
            egress_class,
            task: redact_text(&task.into()),
            outcome,
            reason: reason.map(|r| redact_text(&r)),
        }
    }

    /// Cria uma entrada para um egresso **permitido**.
    #[must_use]
    pub fn allowed(
        destination: impl Into<String>,
        profile: Option<String>,
        egress_class: EgressClass,
        task: impl Into<String>,
    ) -> Self {
        Self::new(
            destination,
            profile,
            egress_class,
            task,
            AuditOutcome::Allowed,
            None,
        )
    }

    /// Cria uma entrada para um egresso **bloqueado**, com o motivo.
    #[must_use]
    pub fn blocked(
        destination: impl Into<String>,
        profile: Option<String>,
        egress_class: EgressClass,
        task: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self::new(
            destination,
            profile,
            egress_class,
            task,
            AuditOutcome::Blocked,
            Some(reason.into()),
        )
    }

    /// Serializa a entrada em JSON (formato de linha do audit log).
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("AuditEntry é sempre serializável")
    }
}

impl std::fmt::Display for AuditEntry {
    /// Uma linha compacta e legível — não o *dump* de `Debug` (`{entry:?}`),
    /// que reimprime nomes de campo e chega a `2-3` linhas por chamada de
    /// egresso, poluindo o stderr de quem só quer ver a resposta/erro da
    /// tarefa (achado real do teste de usabilidade,
    /// `scripts/usability-test.sh`). Continua obrigatório pelo ADR-0002
    /// (audit trail de todo egresso) — só o formato de impressão muda.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} -> {} ({}",
            self.task, self.destination, self.egress_class
        )?;
        match self.outcome {
            AuditOutcome::Allowed => write!(f, ", allowed)"),
            AuditOutcome::Blocked => {
                write!(f, ", blocked")?;
                if let Some(reason) = &self.reason {
                    write!(f, ": {reason}")?;
                }
                write!(f, ")")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entrada_contem_os_campos_exigidos_pelo_adr_0002() {
        let entrada = AuditEntry::allowed(
            "api.anthropic.com",
            Some("pessoal".into()),
            EgressClass::CloudOk,
            "resumir arquivo",
        );
        let json: serde_json::Value = serde_json::from_str(&entrada.to_json()).unwrap();
        for campo in ["destination", "profile", "egress_class", "task", "outcome"] {
            assert!(
                json.get(campo).is_some(),
                "campo obrigatório ausente: {campo}"
            );
        }
    }

    #[test]
    fn entrada_funciona_sem_perfil_resolvido() {
        let entrada = AuditEntry::blocked(
            "endpoint.desconhecido",
            None,
            EgressClass::LocalOnly,
            "tarefa qualquer",
            "host fora da allowlist",
        );
        assert_eq!(entrada.profile, None);
        assert_eq!(entrada.outcome, AuditOutcome::Blocked);
        assert!(entrada.reason.is_some());
    }

    #[test]
    fn segredo_na_tarefa_nunca_aparece_na_entrada_nem_no_json() {
        let segredo = "sk-proj-super-secreta-123456";
        let entrada = AuditEntry::allowed(
            "api.anthropic.com",
            Some("empresa".into()),
            EgressClass::LocalOnly,
            format!("processar prompt com chave={segredo}"),
        );

        assert!(!entrada.task.contains(segredo));
        assert!(!entrada.to_json().contains(segredo));
    }

    #[test]
    fn segredo_no_motivo_de_bloqueio_nunca_aparece() {
        let segredo = "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQ";
        let entrada = AuditEntry::blocked(
            "gateway.interno",
            Some("empresa".into()),
            EgressClass::LocalOnly,
            "chamar gateway",
            format!("cabeçalho recebido: {segredo}"),
        );

        let reason = entrada.reason.as_deref().unwrap_or_default();
        assert!(!reason.contains("eyJhbGciOiJIUzI1NiJ9"));
        assert!(!entrada.to_json().contains("eyJhbGciOiJIUzI1NiJ9"));
    }

    #[test]
    fn segredo_no_destino_tambem_e_redigido() {
        // Ex.: destino com token na query string.
        let entrada = AuditEntry::allowed(
            "gateway.interno/chat?token=ghp_1234567890abcdefghij",
            Some("pessoal".into()),
            EgressClass::CloudOk,
            "chat",
        );
        assert!(!entrada.destination.contains("ghp_1234567890abcdefghij"));
    }

    #[test]
    fn outcome_serializa_em_snake_case() {
        assert_eq!(
            serde_json::to_value(AuditOutcome::Blocked).unwrap(),
            serde_json::json!("blocked")
        );
    }

    #[test]
    fn display_e_uma_linha_compacta_nao_o_dump_de_debug() {
        let entrada = AuditEntry::allowed(
            "http://127.0.0.1:11434/api/chat",
            None,
            EgressClass::LocalOnly,
            "chat_stream",
        );

        assert_eq!(
            entrada.to_string(),
            "chat_stream -> http://127.0.0.1:11434/api/chat (local-only, allowed)"
        );
    }

    #[test]
    fn display_de_entrada_bloqueada_inclui_o_motivo() {
        let entrada = AuditEntry::blocked(
            "endpoint.desconhecido",
            None,
            EgressClass::LocalOnly,
            "chat",
            "host fora da allowlist",
        );

        assert_eq!(
            entrada.to_string(),
            "chat -> endpoint.desconhecido (local-only, blocked: host fora da allowlist)"
        );
    }
}
