// Caminho relativo: crates/core/src/tools/permission.rs
//! Gate de permissão de *ação* sobre tools (MT-11): decide, para um nome de
//! tool, se a execução é `allow`/`ask`/`deny`, a partir das listas já
//! definidas em [`crate::config::Permissions`] (MT-04) — não inventa um novo
//! formato de política. Correspondência por nome exato, lógica pura, sem I/O.
//!
//! Este gate cobre permissão de **ação** ("posso executar esta tool?"). Não
//! confundir com o Guardrail Gate de **conteúdo** do ADR-0007 (ainda não
//! implementado): são mecanismos distintos, sobre dimensões diferentes.

use crate::config::Permissions;

/// Decisão de permissão para uma tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Executa sem confirmação.
    Allow,
    /// Requer confirmação explícita antes de executar.
    Ask,
    /// Nunca executa.
    Deny,
}

/// Decide a permissão de uma tool a partir das listas `deny`/`ask`.
#[derive(Debug, Clone)]
pub struct PermissionGate {
    permissions: Permissions,
}

impl PermissionGate {
    /// Cria o gate a partir das permissões já resolvidas (MT-04).
    #[must_use]
    pub fn new(permissions: Permissions) -> Self {
        Self { permissions }
    }

    /// Decide a permissão de `tool_name`.
    ///
    /// **Precedência fail-closed:** `deny` é checado antes de `ask` — se um
    /// nome aparecer (por erro de configuração) em ambas as listas, prevalece
    /// o mais restritivo. Nomes fora de ambas as listas são `Allow` por
    /// padrão — mesma convenção de [`Permissions`] (MT-04): as listas são
    /// exceções sobre um padrão permissivo, não um allowlist fechado.
    #[must_use]
    pub fn decide(&self, tool_name: &str) -> Permission {
        if self
            .permissions
            .deny
            .iter()
            .any(|padrao| padrao == tool_name)
        {
            Permission::Deny
        } else if self
            .permissions
            .ask
            .iter()
            .any(|padrao| padrao == tool_name)
        {
            Permission::Ask
        } else {
            Permission::Allow
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(deny: &[&str], ask: &[&str]) -> PermissionGate {
        PermissionGate::new(Permissions {
            deny: deny.iter().map(|s| (*s).to_string()).collect(),
            ask: ask.iter().map(|s| (*s).to_string()).collect(),
        })
    }

    #[test]
    fn nome_fora_das_listas_e_allow_por_padrao() {
        assert_eq!(gate(&[], &[]).decide("fs_read"), Permission::Allow);
    }

    #[test]
    fn nome_na_lista_deny_e_deny() {
        assert_eq!(
            gate(&["shell_exec"], &[]).decide("shell_exec"),
            Permission::Deny
        );
    }

    #[test]
    fn nome_na_lista_ask_e_ask() {
        assert_eq!(gate(&[], &["git_push"]).decide("git_push"), Permission::Ask);
    }

    #[test]
    fn deny_prevalece_sobre_ask_no_mesmo_nome() {
        assert_eq!(
            gate(&["shell_exec"], &["shell_exec"]).decide("shell_exec"),
            Permission::Deny
        );
    }

    #[test]
    fn nomes_nao_relacionados_nao_se_afetam() {
        let g = gate(&["shell_exec"], &["git_push"]);
        assert_eq!(g.decide("fs_read"), Permission::Allow);
        assert_eq!(g.decide("shell_exec"), Permission::Deny);
        assert_eq!(g.decide("git_push"), Permission::Ask);
    }
}
