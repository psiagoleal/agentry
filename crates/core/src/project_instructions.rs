// Caminho relativo: crates/core/src/project_instructions.rs
//! Leitura de instruções de projeto — `AGENTS.md`/`CLAUDE.md` (ADR-0023 §1,
//! MT-59).
//!
//! `AGENTS.md` é o **primário** (fonte única de verdade, convenção
//! multi-agente [agents.md](https://agents.md)); `CLAUDE.md` é lido só como
//! *fallback*, quando `AGENTS.md` está ausente — **nunca os dois juntos**,
//! mesma precedência sem *merge* já usada por `.agentryignore`/
//! `.claudeignore` (ADR-0020). Um arquivo coberto pelo `.agentryignore`
//! ativo é pulado como se não existisse, sem controle de confidencialidade
//! paralelo: quem já usa `.agentryignore` para esconder algo do agente
//! continua com uma única fonte de verdade.

use std::path::Path;

use ignore::gitignore::Gitignore;

/// Nome do arquivo de instruções primário.
const PRIMARY_FILE_NAME: &str = "AGENTS.md";
/// Nome do arquivo de *fallback* — lido só na ausência do primário.
const FALLBACK_FILE_NAME: &str = "CLAUDE.md";

/// Lê `AGENTS.md` (primário) ou, na ausência dele, `CLAUDE.md` (*fallback*)
/// na raiz do projeto — nunca os dois. `None` quando nenhum dos dois está
/// disponível (ausente, coberto por `ignore`, ou erro de I/O ao ler um
/// arquivo presente e não ignorado) — mensagem de sistema ausente é
/// preferível a abortar a sessão por um arquivo de contexto opcional.
#[must_use]
pub fn load_project_instructions(root: &Path, ignore: &Gitignore) -> Option<String> {
    for nome in [PRIMARY_FILE_NAME, FALLBACK_FILE_NAME] {
        let caminho = root.join(nome);
        if ignore.matched(&caminho, false).is_ignore() {
            continue;
        }
        if let Ok(conteudo) = std::fs::read_to_string(&caminho) {
            return Some(conteudo);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ignore::gitignore::GitignoreBuilder;
    use std::path::PathBuf;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `state_dir`/`tools::fs`, MT-38/12).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-project-instructions-test-{}-{}",
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

    #[test]
    fn agents_md_presente_e_lido() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("AGENTS.md"), "regras do projeto").unwrap();

        let conteudo = load_project_instructions(dir.path(), &sem_ignore());

        assert_eq!(conteudo.as_deref(), Some("regras do projeto"));
    }

    #[test]
    fn claude_md_so_e_lido_quando_agents_md_esta_ausente() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("AGENTS.md"), "conteúdo do agents").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "conteúdo do claude").unwrap();

        let conteudo = load_project_instructions(dir.path(), &sem_ignore());

        assert_eq!(
            conteudo.as_deref(),
            Some("conteúdo do agents"),
            "AGENTS.md presente nunca deve cair para CLAUDE.md — nunca os dois juntos"
        );
    }

    #[test]
    fn claude_md_e_lido_como_fallback_na_ausencia_de_agents_md() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("CLAUDE.md"), "conteúdo do claude").unwrap();

        let conteudo = load_project_instructions(dir.path(), &sem_ignore());

        assert_eq!(conteudo.as_deref(), Some("conteúdo do claude"));
    }

    #[test]
    fn nenhum_dos_dois_presente_e_none() {
        let dir = TempDir::new();

        assert_eq!(load_project_instructions(dir.path(), &sem_ignore()), None);
    }

    #[test]
    fn arquivo_coberto_por_ignore_nunca_e_lido() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("AGENTS.md"), "segredo do projeto").unwrap();
        let mut builder = GitignoreBuilder::new(dir.path());
        builder.add_line(None, "AGENTS.md").unwrap();
        let ignore = builder.build().unwrap();

        assert_eq!(
            load_project_instructions(dir.path(), &ignore),
            None,
            "AGENTS.md coberto pelo ignore e sem CLAUDE.md como fallback deve resultar em None"
        );
    }

    #[test]
    fn agents_md_ignorado_ainda_permite_fallback_para_claude_md() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("AGENTS.md"), "segredo do projeto").unwrap();
        std::fs::write(dir.path().join("CLAUDE.md"), "aponta pro AGENTS.md").unwrap();
        let mut builder = GitignoreBuilder::new(dir.path());
        builder.add_line(None, "AGENTS.md").unwrap();
        let ignore = builder.build().unwrap();

        let conteudo = load_project_instructions(dir.path(), &ignore);

        assert_eq!(conteudo.as_deref(), Some("aponta pro AGENTS.md"));
    }
}
