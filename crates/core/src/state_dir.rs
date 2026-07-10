// Caminho relativo: crates/core/src/state_dir.rs
//! Diretório de estado local por projeto (MT-38, ADR-0017).
//!
//! Resolve onde o `agentry` deve persistir seu próprio estado (memória de
//! sessão, índices RAG, audit log — cada um em seu próprio ticket futuro):
//! `<raiz>/.agentry/`, nunca um diretório global do usuário. `<raiz>` é o
//! primeiro ancestral do diretório de trabalho que contém `.git` (arquivo
//! ou diretório — cobre *worktrees*), subindo a partir do cwd; sem `.git`
//! em nenhum ancestral, `<raiz>` é o próprio cwd — mesma técnica de
//! descoberta de raiz que o git usa, funcionando corretamente em
//! monorepo/subdiretório sem caso especial.
//!
//! Na primeira escrita, `.agentry/.gitignore` é criado com o conteúdo `*`
//! — o diretório se autoexclui do controle de versão sem nunca tocar no
//! `.gitignore` do projeto (arquivo que o `agentry` não é dono). Como as
//! tools de leitura já existentes ([`crate::tools::fs`] do MT-12,
//! [`crate::tools::repo_map`] do MT-21) usam a crate `ignore`, que
//! respeita `.gitignore` por padrão, `.agentry/` já sai de graça de
//! qualquer varredura de repo-map/RAG — nenhuma tool precisa de caso
//! especial para não indexar/ler a própria memória do agente.
//!
//! Este módulo só resolve o diretório e garante sua auto-exclusão — o uso
//! concreto por qualquer subsistema (índices RAG, sessão, audit log) fica
//! para o ticket de cada um, conforme decidido na ADR-0017.

use std::io;
use std::path::{Path, PathBuf};

const NOME_DIRETORIO: &str = ".agentry";
const CONTEUDO_GITIGNORE: &str = "*\n";

/// Resolve a raiz do projeto a partir de `start`: o primeiro ancestral
/// (incluindo o próprio `start`) que contém `.git` (arquivo ou diretório).
/// Sem `.git` em nenhum ancestral, devolve `start` — nunca a raiz do
/// sistema de arquivos.
#[must_use]
pub fn resolve_root(start: &Path) -> PathBuf {
    let mut atual = start;
    loop {
        if atual.join(".git").exists() {
            return atual.to_path_buf();
        }
        match atual.parent() {
            Some(pai) => atual = pai,
            None => return start.to_path_buf(),
        }
    }
}

/// Garante que `<raiz>/.agentry/` exista (raiz resolvida por
/// [`resolve_root`] a partir de `start`) e que `.agentry/.gitignore`
/// tenha o conteúdo `*`, criando-o se ainda não existir. Idempotente:
/// chamadas repetidas não duplicam nem sobrescrevem um `.gitignore` já
/// presente (mesmo que customizado pelo usuário).
///
/// # Errors
///
/// Devolve o [`io::Error`] de criar o diretório ou escrever o
/// `.gitignore`, sem tratamento especial — reflete diretamente a falha
/// do sistema de arquivos (ex.: permissão negada).
pub fn ensure_state_dir(start: &Path) -> io::Result<PathBuf> {
    let estado = resolve_root(start).join(NOME_DIRETORIO);
    std::fs::create_dir_all(&estado)?;

    let gitignore = estado.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, CONTEUDO_GITIGNORE)?;
    }

    Ok(estado)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo — sem depender de uma crate de teste nova (mesma disciplina
    /// já usada em `tools/fs.rs`, MT-12).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-state-dir-test-{}-{}",
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

    #[test]
    fn resolve_root_encontra_ancestral_com_git_diretorio() {
        let dir = TempDir::new();
        let raiz = dir.path();
        std::fs::create_dir_all(raiz.join(".git")).expect("cria .git");
        let subdir = raiz.join("src").join("nested");
        std::fs::create_dir_all(&subdir).expect("cria subdiretório");

        assert_eq!(resolve_root(&subdir), raiz);
    }

    #[test]
    fn resolve_root_encontra_ancestral_com_git_arquivo_worktree() {
        let dir = TempDir::new();
        let raiz = dir.path();
        std::fs::write(raiz.join(".git"), "gitdir: /outro/lugar\n").expect("cria .git arquivo");
        let subdir = raiz.join("src");
        std::fs::create_dir_all(&subdir).expect("cria subdiretório");

        assert_eq!(resolve_root(&subdir), raiz);
    }

    #[test]
    fn resolve_root_sem_git_em_nenhum_ancestral_cai_no_start() {
        // O diretório temporário do teste não tem `.git` em nenhum
        // ancestral real (fica sob o temp dir do sistema, fora de
        // qualquer repositório) — não deve subir até a raiz do sistema de
        // arquivos, e sim devolver o próprio `start`.
        let dir = TempDir::new();

        assert_eq!(resolve_root(dir.path()), dir.path());
    }

    #[test]
    fn ensure_state_dir_cria_o_gitignore_com_conteudo_asterisco() {
        let dir = TempDir::new();

        let estado = ensure_state_dir(dir.path()).expect("deve criar o diretório de estado");

        assert_eq!(estado, dir.path().join(".agentry"));
        let gitignore = std::fs::read_to_string(estado.join(".gitignore"))
            .expect("deve ter criado o .gitignore");
        assert_eq!(gitignore, "*\n");
    }

    #[test]
    fn ensure_state_dir_e_idempotente_e_nao_sobrescreve_gitignore_customizado() {
        let dir = TempDir::new();

        let primeira = ensure_state_dir(dir.path()).expect("primeira chamada deve funcionar");

        // Simula uma customização do usuário no .gitignore próprio.
        std::fs::write(
            primeira.join(".gitignore"),
            "*\n!segredo-nao-e-de-verdade.txt\n",
        )
        .expect("simula customização");

        let segunda = ensure_state_dir(dir.path()).expect("segunda chamada não deve quebrar");

        assert_eq!(primeira, segunda);
        let gitignore = std::fs::read_to_string(segunda.join(".gitignore"))
            .expect("gitignore deve continuar existindo");
        assert_eq!(
            gitignore, "*\n!segredo-nao-e-de-verdade.txt\n",
            "chamada repetida não deve sobrescrever customização do usuário"
        );
    }
}
