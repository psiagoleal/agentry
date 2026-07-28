// Caminho relativo: crates/core/src/tools/permission.rs
//! Gate de permissão de *ação* sobre tools (MT-11): decide, para um nome de
//! tool, se a execução é `allow`/`ask`/`deny`, a partir das listas já
//! definidas em [`crate::config::Permissions`] (MT-04) — não inventa um novo
//! formato de política. Correspondência por nome exato, lógica pura, sem I/O.
//!
//! Este gate cobre permissão de **ação** ("posso executar esta tool?"). Não
//! confundir com o Guardrail Gate de **conteúdo** do ADR-0007 (ainda não
//! implementado): são mecanismos distintos, sobre dimensões diferentes.

use std::path::{Path, PathBuf};

use ignore::gitignore::{Gitignore, GitignoreBuilder};

use crate::config::Permissions;
use crate::model::ToolCall;

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

/// Decide a permissão de uma tool a partir das listas `deny`/`ask` e, quando
/// declarado, do escopo de caminho `readAllow` (ADR-0041).
#[derive(Debug, Clone)]
pub struct PermissionGate {
    permissions: Permissions,
    /// Matcher de `readAllow` já compilado — `None` quando a lista está vazia
    /// (sem escopo de caminho, comportamento anterior à ADR-0041).
    read_allow: Option<Gitignore>,
    /// Raiz do workspace, para normalizar o argumento `path` pela mesma regra
    /// que a tool aplicará. `None` ⇒ escopo de caminho inativo.
    root: Option<PathBuf>,
}

impl PermissionGate {
    /// Cria o gate a partir das permissões já resolvidas (MT-04), **sem**
    /// escopo de caminho — `readAllow` exige a raiz do workspace, que este
    /// construtor não recebe (ver [`Self::with_workspace_root`]).
    #[must_use]
    pub fn new(permissions: Permissions) -> Self {
        Self {
            permissions,
            read_allow: None,
            root: None,
        }
    }

    /// Ativa o escopo de caminho de `readAllow` (ADR-0041) relativo a `root`.
    ///
    /// Sem raiz não há como normalizar o argumento `path` da mesma forma que a
    /// tool o normalizará, e comparar a string crua abriria evasão por `../`.
    /// Por isso o escopo só existe quando este construtor é usado; `readAllow`
    /// vazio continua significando "sem escopo".
    #[must_use]
    pub fn with_workspace_root(mut self, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        if !self.permissions.read_allow.is_empty() {
            let mut builder = GitignoreBuilder::new(&root);
            for padrao in &self.permissions.read_allow {
                // A sintaxe do `.gitignore` marca o que **casa**; aqui um
                // padrão que casa significa "permitido", não "ignorado" — a
                // inversão de sentido é só de interpretação, o matcher é o
                // mesmo já usado por `.agentryignore` (nenhuma dependência
                // nova, e a sintaxe que o usuário já conhece).
                let _ = builder.add_line(None, padrao);
            }
            self.read_allow = builder.build().ok();
        }
        self.root = Some(root);
        self
    }

    /// Decide a permissão de `call`.
    ///
    /// **Precedência fail-closed:** `deny` é checado antes de `ask` — se um
    /// nome aparecer (por erro de configuração) em ambas as listas, prevalece
    /// o mais restritivo. Nomes fora de ambas as listas são `Allow` por
    /// padrão — mesma convenção de [`Permissions`] (MT-04): as listas são
    /// exceções sobre um padrão permissivo, não um allowlist fechado.
    ///
    /// Quando `readAllow` está ativo (ADR-0041) e a chamada traz um argumento
    /// `path`, o caminho é normalizado e conferido **depois** de `deny` (que
    /// nunca é afrouxado) e **antes** de `ask`: um caminho fora do escopo é
    /// `Deny`, não uma confirmação que o usuário poderia aceitar por engano.
    #[must_use]
    pub fn decide(&self, call: &ToolCall) -> Permission {
        if self.permissions.deny.iter().any(|p| p == &call.name) {
            return Permission::Deny;
        }
        if self.caminho_fora_do_escopo(call) {
            return Permission::Deny;
        }
        if self.permissions.ask.iter().any(|p| p == &call.name) {
            Permission::Ask
        } else {
            Permission::Allow
        }
    }

    /// `true` se `readAllow` está ativo, a chamada tem argumento `path` e esse
    /// caminho **não** está no escopo permitido.
    ///
    /// Chamada sem argumento `path` nunca é barrada aqui — `shell_exec`/`glob`/
    /// `fs_search` leem por outras vias e são deliberadamente **não cobertos**
    /// (ADR-0041): quem quer contê-los precisa colocá-los em `deny`.
    fn caminho_fora_do_escopo(&self, call: &ToolCall) -> bool {
        let (Some(matcher), Some(root)) = (self.read_allow.as_ref(), self.root.as_ref()) else {
            return false;
        };
        let Some(path_arg) = call.arguments.get("path").and_then(|v| v.as_str()) else {
            return false;
        };
        // Mesma normalização que a tool aplicará (`fs::resolve_within_root`):
        // caminho absoluto ou com `..` é recusado antes de qualquer
        // comparação — nunca se compara a string crua.
        let Ok(absoluto) = crate::tools::fs::resolve_within_root(root, path_arg) else {
            return true;
        };
        !caminho_permitido(matcher, root, &absoluto)
    }
}

/// Decide se `absoluto` casa algum padrão de `readAllow`.
///
/// Um diretório permitido libera o que está sob ele: o matcher é consultado
/// também para cada ancestral até a raiz, senão `docs/**` não cobriria
/// `docs/a/b.md` em todas as grafias de padrão que o usuário pode escrever
/// (`docs/`, `docs`, `docs/**` têm comportamentos sutilmente distintos no
/// `.gitignore`).
fn caminho_permitido(matcher: &Gitignore, root: &Path, absoluto: &Path) -> bool {
    if matcher.matched(absoluto, absoluto.is_dir()).is_ignore() {
        return true;
    }
    let mut atual = absoluto.parent();
    while let Some(dir) = atual {
        if dir == root {
            break;
        }
        if matcher.matched(dir, true).is_ignore() {
            return true;
        }
        atual = dir.parent();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(deny: &[&str], ask: &[&str]) -> PermissionGate {
        PermissionGate::new(Permissions {
            deny: deny.iter().map(|s| (*s).to_string()).collect(),
            ask: ask.iter().map(|s| (*s).to_string()).collect(),
            read_allow: vec![],
        })
    }

    /// Chamada sem argumentos — para os testes que só exercitam nome de tool.
    fn chamada(nome: &str) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            name: nome.into(),
            arguments: serde_json::json!({}),
        }
    }

    /// Chamada com argumento `path`, o único que o escopo da ADR-0041 lê.
    fn chamada_com_path(nome: &str, path: &str) -> ToolCall {
        ToolCall {
            id: "c1".into(),
            name: nome.into(),
            arguments: serde_json::json!({ "path": path }),
        }
    }

    #[test]
    fn nome_fora_das_listas_e_allow_por_padrao() {
        assert_eq!(
            gate(&[], &[]).decide(&chamada("fs_read")),
            Permission::Allow
        );
    }

    #[test]
    fn nome_na_lista_deny_e_deny() {
        assert_eq!(
            gate(&["shell_exec"], &[]).decide(&chamada("shell_exec")),
            Permission::Deny
        );
    }

    #[test]
    fn nome_na_lista_ask_e_ask() {
        assert_eq!(
            gate(&[], &["git_push"]).decide(&chamada("git_push")),
            Permission::Ask
        );
    }

    #[test]
    fn deny_prevalece_sobre_ask_no_mesmo_nome() {
        assert_eq!(
            gate(&["shell_exec"], &["shell_exec"]).decide(&chamada("shell_exec")),
            Permission::Deny
        );
    }

    #[test]
    fn nomes_nao_relacionados_nao_se_afetam() {
        let g = gate(&["shell_exec"], &["git_push"]);
        assert_eq!(g.decide(&chamada("fs_read")), Permission::Allow);
        assert_eq!(g.decide(&chamada("shell_exec")), Permission::Deny);
        assert_eq!(g.decide(&chamada("git_push")), Permission::Ask);
    }

    // ---- readAllow: escopo de leitura por caminho (ADR-0041) ----

    fn gate_com_escopo(read_allow: &[&str], deny: &[&str]) -> PermissionGate {
        PermissionGate::new(Permissions {
            deny: deny.iter().map(|s| (*s).to_string()).collect(),
            ask: vec![],
            read_allow: read_allow.iter().map(|s| (*s).to_string()).collect(),
        })
        .with_workspace_root("/workspace")
    }

    #[test]
    fn read_allow_vazio_nao_restringe_nada() {
        let g = gate(&[], &[]).with_workspace_root("/workspace");
        assert_eq!(
            g.decide(&chamada_com_path("fs_read", "segredos/clientes.csv")),
            Permission::Allow,
            "sem readAllow declarado o comportamento anterior à ADR-0041 é preservado"
        );
    }

    #[test]
    fn arquivo_listado_e_permitido() {
        let g = gate_com_escopo(&["README.md", "AGENTS.md"], &[]);
        assert_eq!(
            g.decide(&chamada_com_path("fs_read", "README.md")),
            Permission::Allow
        );
    }

    /// O caso que motiva a ADR: o agente principal lê a documentação, mas não
    /// o arquivo confidencial.
    #[test]
    fn arquivo_fora_do_escopo_e_deny() {
        let g = gate_com_escopo(&["README.md", "docs/**"], &[]);
        assert_eq!(
            g.decide(&chamada_com_path("fs_read", "clientes.csv")),
            Permission::Deny
        );
    }

    #[test]
    fn diretorio_listado_libera_o_que_esta_sob_ele() {
        let g = gate_com_escopo(&["docs/**", "skills/"], &[]);
        for caminho in ["docs/a.md", "docs/sub/b.md", "skills/x/SKILL.md"] {
            assert_eq!(
                g.decide(&chamada_com_path("fs_read", caminho)),
                Permission::Allow,
                "'{caminho}' deveria estar coberto"
            );
        }
    }

    /// Evasão por travessia: sem normalizar, `docs/../clientes.csv` casaria o
    /// prefixo `docs/` e passaria.
    #[test]
    fn travessia_com_dot_dot_e_deny_mesmo_partindo_de_diretorio_liberado() {
        let g = gate_com_escopo(&["docs/**"], &[]);
        assert_eq!(
            g.decide(&chamada_com_path("fs_read", "docs/../clientes.csv")),
            Permission::Deny
        );
    }

    #[test]
    fn caminho_absoluto_e_deny() {
        let g = gate_com_escopo(&["docs/**"], &[]);
        assert_eq!(
            g.decide(&chamada_com_path("fs_read", "/etc/passwd")),
            Permission::Deny
        );
    }

    /// `readAllow` restringe leitura por caminho, mas não substitui `deny`:
    /// uma tool negada continua negada mesmo pedindo um caminho liberado.
    #[test]
    fn deny_de_tool_vence_readallow_que_liberaria_o_caminho() {
        let g = gate_com_escopo(&["README.md"], &["fs_write"]);
        assert_eq!(
            g.decide(&chamada_com_path("fs_write", "README.md")),
            Permission::Deny
        );
    }

    /// Cobertura parcial declarada na ADR-0041: sem argumento `path`, o
    /// escopo não tem o que checar — conter essas tools é papel do `deny`.
    #[test]
    fn tool_sem_argumento_path_nao_e_barrada_pelo_escopo() {
        let g = gate_com_escopo(&["README.md"], &[]);
        assert_eq!(
            g.decide(&chamada("shell_exec")),
            Permission::Allow,
            "shell_exec não é coberta por readAllow — precisa entrar em deny"
        );
    }
}
