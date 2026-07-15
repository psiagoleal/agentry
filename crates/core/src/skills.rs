// Caminho relativo: crates/core/src/skills.rs
//! Descoberta de skills (`SKILL.md`, ADR-0023 §2, MT-60).
//!
//! Reaproveita literalmente a convenção `.claude/skills/<nome>/SKILL.md` já
//! usada pelo Claude Code (e por este próprio repositório) — sem formato
//! próprio do `agentry`, para compatibilidade direta com projetos que já
//! têm skills definidas. Só o frontmatter (`name`/`description`) é
//! extraído aqui, para a lista compacta sempre presente no *system prompt*
//! (*progressive disclosure*); o corpo completo só é lido sob demanda pela
//! tool `skill` (MT-61).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ignore::gitignore::Gitignore;

/// Metadados de uma skill descoberta — corpo completo fica em `path`, lido
/// sob demanda (MT-61), nunca aqui.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDescriptor {
    /// Nome da skill (chave `name` do frontmatter).
    pub name: String,
    /// Descrição curta (chave `description` do frontmatter).
    pub description: String,
    /// Caminho absoluto do `SKILL.md` — usado pela tool `skill` (MT-61)
    /// para ler o corpo completo sob demanda.
    pub path: PathBuf,
}

/// Varre `<root>/.claude/skills/*/SKILL.md` (um nível de subdiretórios, sem
/// recursão) e extrai `name`/`description` do frontmatter de cada uma.
/// Diretório ausente não é erro (lista vazia — mesmo padrão de
/// `project_instructions::load_project_instructions`). `SKILL.md` coberto
/// por `.agentryignore`/`.claudeignore` é pulado, mesma disciplina de
/// confidencialidade. `SKILL.md` sem `name`/`description` reconhecíveis no
/// frontmatter é pulado **silenciosamente** — nunca interrompe a
/// descoberta das demais; este crate não tem infraestrutura de log (a
/// tolerância ao formato inesperado é o próprio tratamento do erro,
/// ADR-0023).
#[must_use]
pub fn discover_skills(root: &Path, ignore: &Gitignore) -> Vec<SkillDescriptor> {
    let skills_dir = root.join(".claude").join("skills");
    let Ok(entradas) = std::fs::read_dir(&skills_dir) else {
        return Vec::new();
    };

    let mut skills: Vec<SkillDescriptor> = entradas
        .flatten()
        .filter_map(|entrada| {
            let caminho = entrada.path().join("SKILL.md");
            if !caminho.is_file() || ignore.matched(&caminho, false).is_ignore() {
                return None;
            }
            let conteudo = std::fs::read_to_string(&caminho).ok()?;
            let (name, description) = parse_frontmatter(&conteudo)?;
            Some(SkillDescriptor {
                name,
                description,
                path: caminho,
            })
        })
        .collect();
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Extrai `name`/`description` do frontmatter YAML entre delimitadores
/// `---`/`---` no início do arquivo — **parser mínimo, não YAML genérico**
/// (decisão da ADR-0023, registrada em `docs/decisoes-autonomas.md`): cobre
/// só `chave: valor` numa única linha e o bloco dobrado `chave: >-`
/// (concatena, com espaço, as linhas indentadas seguintes até a próxima
/// chave ou o fim do frontmatter — aproximação suficiente do *folded block
/// scalar* do YAML para o uso real deste projeto). `None` se o arquivo não
/// abrir/fechar com `---`, ou se `name`/`description` não forem encontrados
/// ou estiverem vazios.
fn parse_frontmatter(conteudo: &str) -> Option<(String, String)> {
    let mut linhas = conteudo.lines();
    if linhas.next()?.trim() != "---" {
        return None;
    }

    let mut campos: HashMap<String, String> = HashMap::new();
    let mut chave_atual: Option<String> = None;
    let mut bloco_dobrado: Vec<String> = Vec::new();
    let mut fechou = false;

    for linha in linhas {
        if linha.trim() == "---" {
            fechou = true;
            break;
        }
        let indentada = linha.starts_with(' ') || linha.starts_with('\t');
        if indentada && chave_atual.is_some() {
            let texto = linha.trim();
            if !texto.is_empty() {
                bloco_dobrado.push(texto.to_string());
            }
            continue;
        }
        if let Some(chave) = chave_atual.take() {
            campos.insert(chave, bloco_dobrado.join(" "));
            bloco_dobrado.clear();
        }
        let Some((chave, valor)) = linha.split_once(':') else {
            continue;
        };
        let chave = chave.trim().to_string();
        let valor = valor.trim();
        if valor == ">-" || valor == ">" {
            chave_atual = Some(chave);
        } else if !valor.is_empty() {
            campos.insert(chave, valor.to_string());
        }
    }
    if let Some(chave) = chave_atual.take() {
        campos.insert(chave, bloco_dobrado.join(" "));
    }
    if !fechou {
        return None;
    }

    let name = campos.get("name")?.clone();
    let description = campos.get("description")?.clone();
    if name.is_empty() || description.is_empty() {
        return None;
    }
    Some((name, description))
}

/// Formata `skills` como lista compacta (`- nome: descrição` por linha),
/// concatenada à mensagem de sistema (junto de `project_instructions`,
/// MT-59) — dá ao modelo visibilidade de quais skills existem sem carregar
/// o corpo de nenhuma (*progressive disclosure*). Lista vazia devolve
/// string vazia — quem concatena decide não incluir nada nesse caso
/// (nenhum ruído quando não há skill nenhuma).
#[must_use]
pub fn render_skills_list(skills: &[SkillDescriptor]) -> String {
    if skills.is_empty() {
        return String::new();
    }
    let mut saida = String::from(
        "Skills disponíveis (use a tool `skill` com o nome para carregar as instruções completas):",
    );
    for skill in skills {
        saida.push('\n');
        saida.push_str("- ");
        saida.push_str(&skill.name);
        saida.push_str(": ");
        saida.push_str(&skill.description);
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignore::gitignore::GitignoreBuilder;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `project_instructions`/`state_dir`).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-skills-test-{}-{}",
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

    fn sem_ignore() -> Gitignore {
        Gitignore::empty()
    }

    fn cria_skill(dir: &Path, nome_pasta: &str, conteudo_skill_md: &str) {
        let pasta = dir.join(".claude").join("skills").join(nome_pasta);
        std::fs::create_dir_all(&pasta).unwrap();
        std::fs::write(pasta.join("SKILL.md"), conteudo_skill_md).unwrap();
    }

    /// Fixture real: cópia literal do frontmatter de
    /// `.claude/skills/adr-writer/SKILL.md` deste próprio repositório —
    /// prova que o parser mínimo cobre o bloco dobrado (`>-`) de verdade.
    /// *Raw string* (`r#"..."#`), não literal escapada com continuação de
    /// linha (`\` no fim da linha) — a continuação de linha do Rust remove
    /// os espaços de indentação do início da linha seguinte, destruindo
    /// exatamente a indentação do bloco dobrado que este teste precisa
    /// preservar.
    const SKILL_MD_ADR_WRITER: &str = r#"---
name: adr-writer
description: >-
  Cria e atualiza Registros de Decisão de Arquitetura (ADRs) no formato
  Status/Contexto/Decisão/Consequências/Conformidade, e exige consulta aos ADRs
  ativos antes de propor mudanças funcionais. Aciona ao decidir bibliotecas,
  solvers, padrões arquiteturais, restrições de stack, ou quando o usuário pedir
  para registrar/documentar uma decisão técnica.
---

# adr-writer — Registros de Decisão de Arquitetura

Corpo da skill, não usado por este ticket (MT-61 lê sob demanda).
"#;

    #[test]
    fn frontmatter_com_bloco_dobrado_real_e_parseado_corretamente() {
        let dir = TempDir::new();
        cria_skill(dir.path(), "adr-writer", SKILL_MD_ADR_WRITER);

        let skills = discover_skills(dir.path(), &sem_ignore());

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "adr-writer");
        assert!(skills[0]
            .description
            .starts_with("Cria e atualiza Registros de Decisão de Arquitetura (ADRs) no formato"));
        assert!(skills[0]
            .description
            .ends_with("para registrar/documentar uma decisão técnica."));
        assert!(
            !skills[0].description.contains('\n'),
            "bloco dobrado deve virar uma única linha, sem quebras"
        );
    }

    #[test]
    fn skill_md_sem_name_ou_description_e_pulada_sem_interromper_as_demais() {
        let dir = TempDir::new();
        cria_skill(dir.path(), "malformada", "---\nfoo: bar\n---\ncorpo\n");
        cria_skill(
            dir.path(),
            "valida",
            "---\nname: valida\ndescription: uma skill válida\n---\ncorpo\n",
        );

        let skills = discover_skills(dir.path(), &sem_ignore());

        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "valida");
    }

    #[test]
    fn diretorio_claude_skills_ausente_nao_e_erro() {
        let dir = TempDir::new();

        assert_eq!(discover_skills(dir.path(), &sem_ignore()), Vec::new());
    }

    #[test]
    fn skill_md_coberto_por_ignore_e_pulada() {
        let dir = TempDir::new();
        cria_skill(
            dir.path(),
            "escondida",
            "---\nname: escondida\ndescription: não deveria aparecer\n---\n",
        );
        let mut builder = GitignoreBuilder::new(dir.path());
        builder
            .add_line(None, ".claude/skills/escondida/SKILL.md")
            .unwrap();
        let ignore = builder.build().unwrap();

        assert_eq!(discover_skills(dir.path(), &ignore), Vec::new());
    }

    #[test]
    fn render_skills_list_formata_nome_e_descricao_por_linha() {
        let skills = vec![
            SkillDescriptor {
                name: "a".into(),
                description: "primeira".into(),
                path: PathBuf::new(),
            },
            SkillDescriptor {
                name: "b".into(),
                description: "segunda".into(),
                path: PathBuf::new(),
            },
        ];

        let lista = render_skills_list(&skills);

        assert!(lista.contains("- a: primeira"));
        assert!(lista.contains("- b: segunda"));
    }

    #[test]
    fn render_skills_list_vazia_devolve_string_vazia() {
        assert_eq!(render_skills_list(&[]), "");
    }
}
