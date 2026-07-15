// Caminho relativo: crates/core/src/tools/skill.rs
//! Tool `skill` (MT-61, ADR-0023): carrega o corpo completo de uma skill
//! descoberta por [`crate::skills::discover_skills`] (MT-60), sob demanda —
//! fecha o mecanismo de *progressive disclosure*: até aqui, o modelo só via
//! `name`+`description` de cada skill na mensagem de sistema (MT-60);
//! chamar esta tool é como ele obtém as instruções completas de uma delas.
//!
//! Roda sob o mesmo `ToolRegistry`/gate de permissão de qualquer outra tool
//! (MT-11) — sem *default-deny* especial (diferente da tool de shell,
//! MT-13): é leitura local de um arquivo já descoberto, sem efeito
//! colateral, mesma categoria de `fs_read`/`repo_map`.

use std::path::Path;

use crate::provider::BoxFuture;
use crate::skills::SkillDescriptor;
use crate::tools::{Tool, ToolOutput};

/// Devolve só o corpo de um `SKILL.md` (tudo após o `---` de fechamento do
/// frontmatter) — nunca os metadados. Arquivo sem o par de delimitadores
/// `---`/`---` é devolvido por inteiro (não deveria acontecer na prática —
/// `discover_skills`, MT-60, já validou o frontmatter antes de criar o
/// `SkillDescriptor` — mas tratado sem *panic* caso o arquivo tenha mudado
/// entre a descoberta e esta chamada).
fn corpo_sem_frontmatter(conteudo: &str) -> String {
    let linhas: Vec<&str> = conteudo.lines().collect();
    if linhas.first() != Some(&"---") {
        return conteudo.to_string();
    }
    match linhas.iter().skip(1).position(|linha| *linha == "---") {
        Some(posicao_apos_abertura) => {
            let inicio_corpo = posicao_apos_abertura + 2;
            linhas
                .get(inicio_corpo..)
                .unwrap_or(&[])
                .join("\n")
                .trim_start()
                .to_string()
        }
        None => conteudo.to_string(),
    }
}

/// Tool `skill`: carrega o corpo de uma skill descoberta (MT-60) pelo nome.
pub struct SkillTool {
    skills: Vec<SkillDescriptor>,
}

impl SkillTool {
    /// Cria a tool a partir das skills já descobertas
    /// (`skills::discover_skills`, MT-60).
    #[must_use]
    pub fn new(skills: Vec<SkillDescriptor>) -> Self {
        Self { skills }
    }

    fn encontra(&self, nome: &str) -> Option<&SkillDescriptor> {
        self.skills.iter().find(|skill| skill.name == nome)
    }
}

impl Tool for SkillTool {
    fn name(&self) -> &str {
        "skill"
    }

    fn description(&self) -> &str {
        "Carrega as instruções completas de uma skill já listada no system prompt, pelo nome."
    }

    fn input_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Nome da skill a carregar, exatamente como listada no system prompt."
                }
            },
            "required": ["name"]
        })
    }

    fn execute(&self, arguments: serde_json::Value) -> BoxFuture<'_, ToolOutput> {
        Box::pin(async move {
            let Some(nome) = arguments.get("name").and_then(|v| v.as_str()) else {
                return ToolOutput::error("argumento 'name' obrigatório e deve ser string");
            };
            let Some(skill) = self.encontra(nome) else {
                return ToolOutput::error(format!("skill '{nome}' não encontrada"));
            };
            ler_corpo(&skill.path).map_or_else(
                |erro| ToolOutput::error(format!("erro ao ler skill '{nome}': {erro}")),
                ToolOutput::ok,
            )
        })
    }
}

fn ler_corpo(caminho: &Path) -> std::io::Result<String> {
    let conteudo = std::fs::read_to_string(caminho)?;
    Ok(corpo_sem_frontmatter(&conteudo))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Permissions;
    use crate::model::ToolCall;
    use crate::tools::permission::PermissionGate;
    use crate::tools::{ExecutionOutcome, ToolRegistry};
    use serde_json::json;
    use std::path::PathBuf;
    use std::sync::Arc;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `skills`/`project_instructions`).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-skill-tool-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio do sistema não deve estar antes de 1970")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unico);
            std::fs::create_dir_all(&path).expect("deve criar diretório temporário de teste");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn escreve_skill_md(dir: &Path, conteudo: &str) -> PathBuf {
        let caminho = dir.join("SKILL.md");
        std::fs::write(&caminho, conteudo).unwrap();
        caminho
    }

    const SKILL_MD_EXEMPLO: &str = r#"---
name: minha-skill
description: uma skill de teste
---

# Corpo da skill

Instruções completas aqui.
"#;

    #[tokio::test]
    async fn nome_valido_devolve_o_corpo_completo_sem_o_frontmatter() {
        let dir = TempDir::new();
        let caminho = escreve_skill_md(dir.path(), SKILL_MD_EXEMPLO);
        let tool = SkillTool::new(vec![SkillDescriptor {
            name: "minha-skill".into(),
            description: "uma skill de teste".into(),
            path: caminho,
        }]);

        let saida = tool.execute(json!({ "name": "minha-skill" })).await;

        assert!(!saida.is_error);
        assert_eq!(
            saida.content,
            "# Corpo da skill\n\nInstruções completas aqui."
        );
        assert!(
            !saida.content.contains("description:"),
            "frontmatter não deve vazar no corpo devolvido"
        );
    }

    #[tokio::test]
    async fn nome_desconhecido_e_erro_tratado_sem_panic() {
        let tool = SkillTool::new(vec![]);

        let saida = tool.execute(json!({ "name": "nao-existe" })).await;

        assert!(saida.is_error);
        assert!(saida.content.contains("nao-existe"));
    }

    #[tokio::test]
    async fn argumento_name_ausente_e_erro_tratado() {
        let tool = SkillTool::new(vec![]);

        let saida = tool.execute(json!({})).await;

        assert!(saida.is_error);
    }

    #[tokio::test]
    async fn claude_skills_vazio_ainda_registra_a_tool_so_sem_skill_para_carregar() {
        let tool = SkillTool::new(vec![]);

        assert_eq!(tool.name(), "skill");
        let saida = tool.execute(json!({ "name": "qualquer" })).await;
        assert!(saida.is_error, "tool existe, só a skill pedida não");
    }

    #[tokio::test]
    async fn tool_respeita_deny_do_permission_gate_como_qualquer_outra() {
        let dir = TempDir::new();
        let caminho = escreve_skill_md(dir.path(), SKILL_MD_EXEMPLO);
        let mut permissions = Permissions::default();
        permissions.deny.push("skill".to_string());
        let mut registry = ToolRegistry::new(PermissionGate::new(permissions));
        registry.register(Arc::new(SkillTool::new(vec![SkillDescriptor {
            name: "minha-skill".into(),
            description: "uma skill de teste".into(),
            path: caminho,
        }])));

        let call = ToolCall {
            id: "1".into(),
            name: "skill".into(),
            arguments: json!({ "name": "minha-skill" }),
        };
        let outcome = registry.execute(&call).await;

        assert!(matches!(outcome, ExecutionOutcome::Denied(_)));
    }

    #[test]
    fn corpo_sem_frontmatter_de_arquivo_sem_delimitadores_devolve_por_inteiro() {
        let texto = "sem frontmatter nenhum\nsó corpo\n";
        assert_eq!(corpo_sem_frontmatter(texto), texto);
    }
}
