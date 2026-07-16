// Caminho relativo: crates/core/src/memory/mod.rs
//! Memória de projeto **explícita** entre sessões (MT-93, ADR-0032) —
//! `MemoryStore` grava/carrega fatos pontuais em `.agentry/memory.json`
//! (mesmo diretório de estado local da ADR-0017, auto-excluído do git).
//!
//! **Sempre um ato do usuário, nunca uma decisão do modelo:** este módulo
//! não expõe nenhuma [`crate::tools::Tool`] — só é alcançado pelo comando
//! `/remember` (REPL) e pela flag `--remember` (modo *one-shot*),
//! `crates/cli/src/main.rs`/`crates/cli/src/repl.rs` (MT-94). Um fato só
//! entra em `.agentry/memory.json` porque o usuário digitou o comando —
//! nenhum caminho automático.
//!
//! Diferente de `.agentry/checkpoints.json` (MT-86): sem teto de entradas
//! (fatos são curados manualmente, um por comando explícito, não gerados a
//! cada chamada de tool — risco de crescimento descontrolado muito menor) e
//! sem `undo`/remoção nesta versão (editar o arquivo é o caminho).

use std::fs;
use std::path::PathBuf;

use crate::state_dir::ensure_state_dir;

/// Nome do arquivo de memória dentro de `.agentry/` — um único array JSON
/// de *strings*, sem estrutura extra (mais fácil de editar à mão também).
const NOME_ARQUIVO: &str = "memory.json";

/// Erros do ciclo de vida de memória — sempre tratado, nunca `panic`.
#[derive(Debug)]
pub enum MemoryError {
    /// Falha de I/O ao ler/escrever `.agentry/memory.json`.
    Io(String),
}

impl core::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "falha de I/O na memória de projeto: {msg}"),
        }
    }
}

impl std::error::Error for MemoryError {}

/// Gerencia os fatos de memória de um workspace — `remember`/`load` leem e
/// reescrevem `.agentry/memory.json` a cada chamada (sem estado em memória
/// entre chamadas, mesmo padrão de `crate::checkpoint::CheckpointStore`).
pub struct MemoryStore {
    workspace_root: PathBuf,
}

impl MemoryStore {
    /// Cria o *store* para o workspace em `workspace_root` — não toca o
    /// disco até a primeira chamada de [`Self::remember`]/[`Self::load`].
    #[must_use]
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }

    fn caminho_arquivo(&self) -> Result<PathBuf, MemoryError> {
        ensure_state_dir(&self.workspace_root)
            .map(|dir| dir.join(NOME_ARQUIVO))
            .map_err(|e| MemoryError::Io(e.to_string()))
    }

    /// Carrega todos os fatos já gravados, em ordem — arquivo ausente
    /// devolve lista vazia (nunca erro, mesmo comportamento de uma sessão
    /// sem nenhum fato lembrado ainda).
    ///
    /// # Errors
    ///
    /// Devolve [`MemoryError::Io`] se o arquivo existir mas não puder ser
    /// lido, ou se o conteúdo não for um array JSON de *strings* válido.
    pub fn load(&self) -> Result<Vec<String>, MemoryError> {
        let caminho = self.caminho_arquivo()?;
        match fs::read_to_string(&caminho) {
            Ok(conteudo) => serde_json::from_str(&conteudo)
                .map_err(|e| MemoryError::Io(format!("memória corrompida: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(MemoryError::Io(e.to_string())),
        }
    }

    /// Acrescenta `fato` à lista já gravada — nunca sobrescreve os fatos
    /// anteriores, sempre soma ao final.
    ///
    /// # Errors
    ///
    /// Devolve [`MemoryError::Io`] se `.agentry/memory.json` não puder ser
    /// lido/escrito.
    pub fn remember(&self, fato: impl Into<String>) -> Result<(), MemoryError> {
        let mut fatos = self.load()?;
        fatos.push(fato.into());
        let caminho = self.caminho_arquivo()?;
        let conteudo =
            serde_json::to_string_pretty(&fatos).map_err(|e| MemoryError::Io(e.to_string()))?;
        fs::write(&caminho, conteudo).map_err(|e| MemoryError::Io(e.to_string()))
    }
}

/// Formata a lista de fatos para injeção no *system prompt* de uma
/// [`crate::session::Session`] (via `Session::with_memoria`, MT-94) — lista
/// vazia devolve string vazia (mesmo padrão de
/// [`crate::skills::render_skills_list`] com nenhuma skill descoberta):
/// `Session::ensure_system_prompt` já ignora um bloco vazio.
#[must_use]
pub fn render_memoria(fatos: &[String]) -> String {
    if fatos.is_empty() {
        return String::new();
    }
    let mut saida =
        String::from("Memória do projeto (fatos que você pediu para lembrar entre sessões):");
    for fato in fatos {
        saida.push('\n');
        saida.push_str("- ");
        saida.push_str(fato);
    }
    saida
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo — sem depender de uma crate de teste nova (mesma disciplina
    /// já usada em `checkpoint/mod.rs`/`tools/fs.rs`, MT-12/MT-86).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-memory-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio do sistema não deve estar antes de 1970")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unico);
            fs::create_dir_all(&path).expect("deve criar diretório temporário de teste");
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn load_sem_nenhum_arquivo_ainda_gravado_devolve_lista_vazia_nao_erro() {
        let dir = TempDir::new();
        let store = MemoryStore::new(dir.path());

        let fatos = store.load().expect("load deve funcionar mesmo sem arquivo");

        assert!(fatos.is_empty());
    }

    #[test]
    fn remember_seguido_de_load_devolve_o_fato_gravado() {
        let dir = TempDir::new();
        let store = MemoryStore::new(dir.path());

        store
            .remember("o usuário prefere respostas em português")
            .expect("remember deve funcionar");

        let fatos = store.load().expect("load deve funcionar");
        assert_eq!(
            fatos,
            vec!["o usuário prefere respostas em português".to_string()]
        );
    }

    #[test]
    fn multiplos_remember_acumulam_em_ordem_sem_sobrescrever() {
        let dir = TempDir::new();
        let store = MemoryStore::new(dir.path());

        store.remember("fato 1").expect("primeiro remember");
        store.remember("fato 2").expect("segundo remember");
        store.remember("fato 3").expect("terceiro remember");

        let fatos = store.load().expect("load deve funcionar");
        assert_eq!(fatos, vec!["fato 1", "fato 2", "fato 3"]);
    }

    #[test]
    fn memoria_persiste_em_agentry_memory_json_auto_excluido_do_git() {
        let dir = TempDir::new();
        let store = MemoryStore::new(dir.path());

        store.remember("fato").expect("remember deve funcionar");

        let arquivo = dir.path().join(".agentry").join("memory.json");
        assert!(arquivo.exists());
        let gitignore = dir.path().join(".agentry").join(".gitignore");
        assert!(
            gitignore.exists(),
            ".agentry/.gitignore deve existir (auto-exclusão, ADR-0017)"
        );
    }

    #[test]
    fn render_memoria_de_lista_vazia_devolve_string_vazia() {
        assert_eq!(render_memoria(&[]), String::new());
    }

    #[test]
    fn render_memoria_lista_cada_fato_com_marcador() {
        let fatos = vec!["fato A".to_string(), "fato B".to_string()];

        let texto = render_memoria(&fatos);

        assert!(texto.contains("- fato A"));
        assert!(texto.contains("- fato B"));
        assert!(texto.starts_with("Memória do projeto"));
    }
}
