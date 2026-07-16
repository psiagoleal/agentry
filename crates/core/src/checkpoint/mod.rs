// Caminho relativo: crates/core/src/checkpoint/mod.rs
//! Checkpoints e *undo* de mudanças de arquivo feitas pelo agente (MT-86,
//! ADR-0030) — pilha *LIFO* persistida em `.agentry/checkpoints.json`
//! (mesmo diretório de estado local da ADR-0017), auto-excluído do git
//! pelo `.gitignore` que [`crate::state_dir::ensure_state_dir`] já garante.
//!
//! Só registra o que [`crate::tools::checkpoint::CheckpointingTool`]
//! (MT-87) grava — este módulo não sabe nada sobre `fs_write`/`fs_edit`,
//! só manipula entradas `(path, conteúdo anterior)` e sabe restaurar/
//! remover um arquivo dado esse par. Teto fixo (não configurável nesta
//! versão, YAGNI) — o checkpoint mais antigo é descartado silenciosamente
//! ao ultrapassar.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::state_dir::ensure_state_dir;

/// Nome do arquivo de manifesto dentro de `.agentry/` — um único array JSON,
/// sem arquivo separado por checkpoint (design mínimo: nada a nomear/
/// colidir por servidor ou por tool).
const NOME_ARQUIVO: &str = "checkpoints.json";

/// Teto de checkpoints retidos — constante fixa (ADR-0030 §"Teto de
/// checkpoints"), não configurável nesta versão.
const TETO_CHECKPOINTS: usize = 50;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct CheckpointEntry {
    /// Caminho relativo à raiz do workspace, exatamente como recebido do
    /// argumento `path` de `fs_write`/`fs_edit` — mesma string, nunca
    /// canonicalizada, para restaurar/remover no mesmo lugar de origem.
    path: String,
    /// Conteúdo do arquivo **antes** da mudança; `None` significa que o
    /// arquivo não existia (o `undo` correspondente remove, não restaura).
    conteudo_antes: Option<String>,
}

/// O que [`CheckpointStore::undo`] fez de fato — para o chamador (CLI/REPL/
/// TUI) reportar ao usuário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UndoAcao {
    /// O arquivo existia antes da mudança desfeita; conteúdo restaurado.
    Restaurado,
    /// O arquivo não existia antes da mudança desfeita; removido.
    Removido,
}

/// Resultado de um [`CheckpointStore::undo`] bem-sucedido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoOutcome {
    /// Caminho (relativo à raiz do workspace) do arquivo afetado.
    pub path: String,
    pub acao: UndoAcao,
}

/// Erros do ciclo de vida de checkpoint — sempre tratado, nunca `panic`.
#[derive(Debug)]
pub enum CheckpointError {
    /// Falha de I/O ao ler/escrever `.agentry/checkpoints.json` ou o
    /// arquivo-alvo do `undo`.
    Io(String),
    /// `undo()` chamado sem nenhum checkpoint disponível.
    Vazio,
}

impl core::fmt::Display for CheckpointError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Io(msg) => write!(f, "falha de I/O no checkpoint: {msg}"),
            Self::Vazio => write!(f, "nenhum checkpoint disponível para desfazer"),
        }
    }
}

impl std::error::Error for CheckpointError {}

/// Gerencia a pilha de checkpoints de um workspace — `record`/`undo` leem e
/// reescrevem `.agentry/checkpoints.json` a cada chamada (sem estado em
/// memória entre chamadas): mesma raiz de workspace, arquivo único, nenhuma
/// necessidade de *lock* além do já garantido pelo processo único da CLI.
pub struct CheckpointStore {
    workspace_root: PathBuf,
}

impl CheckpointStore {
    /// Cria o *store* para o workspace em `workspace_root` — não toca o
    /// disco até a primeira chamada de [`Self::record`]/[`Self::undo`].
    #[must_use]
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }

    fn caminho_manifesto(&self) -> Result<PathBuf, CheckpointError> {
        ensure_state_dir(&self.workspace_root)
            .map(|dir| dir.join(NOME_ARQUIVO))
            .map_err(|e| CheckpointError::Io(e.to_string()))
    }

    fn carregar(&self) -> Result<Vec<CheckpointEntry>, CheckpointError> {
        let caminho = self.caminho_manifesto()?;
        match fs::read_to_string(&caminho) {
            Ok(conteudo) => serde_json::from_str(&conteudo)
                .map_err(|e| CheckpointError::Io(format!("manifesto corrompido: {e}"))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(CheckpointError::Io(e.to_string())),
        }
    }

    fn salvar(&self, entradas: &[CheckpointEntry]) -> Result<(), CheckpointError> {
        let caminho = self.caminho_manifesto()?;
        let conteudo = serde_json::to_string_pretty(entradas)
            .map_err(|e| CheckpointError::Io(e.to_string()))?;
        fs::write(&caminho, conteudo).map_err(|e| CheckpointError::Io(e.to_string()))
    }

    /// Acrescenta um checkpoint ao topo da pilha — `path` é o mesmo caminho
    /// relativo recebido pela tool, `conteudo_antes` o conteúdo do arquivo
    /// antes da mudança (`None` se o arquivo não existia). Descarta o
    /// checkpoint mais antigo se ultrapassar [`TETO_CHECKPOINTS`].
    ///
    /// # Errors
    ///
    /// Devolve [`CheckpointError::Io`] se `.agentry/checkpoints.json` não
    /// puder ser lido/escrito.
    pub fn record(
        &self,
        path: impl Into<String>,
        conteudo_antes: Option<String>,
    ) -> Result<(), CheckpointError> {
        let mut entradas = self.carregar()?;
        entradas.push(CheckpointEntry {
            path: path.into(),
            conteudo_antes,
        });
        if entradas.len() > TETO_CHECKPOINTS {
            entradas.remove(0);
        }
        self.salvar(&entradas)
    }

    /// Desempilha o checkpoint mais recente e restaura (ou remove) o
    /// arquivo correspondente.
    ///
    /// # Errors
    ///
    /// Devolve [`CheckpointError::Vazio`] se não houver nenhum checkpoint;
    /// [`CheckpointError::Io`] se a restauração/remoção falhar — nesse
    /// caso o checkpoint **permanece** na pilha (nunca descartado sem
    /// sucesso de fato, para não perder a chance de tentar de novo).
    pub fn undo(&self) -> Result<UndoOutcome, CheckpointError> {
        let mut entradas = self.carregar()?;
        let Some(ultima) = entradas.last().cloned() else {
            return Err(CheckpointError::Vazio);
        };

        let alvo = self.workspace_root.join(&ultima.path);
        let acao = match &ultima.conteudo_antes {
            Some(conteudo) => {
                if let Some(pai) = alvo.parent() {
                    fs::create_dir_all(pai).map_err(|e| CheckpointError::Io(e.to_string()))?;
                }
                fs::write(&alvo, conteudo).map_err(|e| CheckpointError::Io(e.to_string()))?;
                UndoAcao::Restaurado
            }
            None => {
                match fs::remove_file(&alvo) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(CheckpointError::Io(e.to_string())),
                }
                UndoAcao::Removido
            }
        };

        entradas.pop();
        self.salvar(&entradas)?;

        Ok(UndoOutcome {
            path: ultima.path,
            acao,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo — sem depender de uma crate de teste nova (mesma disciplina
    /// já usada em `tools/fs.rs`/`state_dir.rs`, MT-12/MT-38).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-checkpoint-test-{}-{}",
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

    fn workspace_temporario() -> TempDir {
        TempDir::new()
    }

    #[test]
    fn undo_sem_nenhum_checkpoint_e_erro_tratado() {
        let dir = workspace_temporario();
        let store = CheckpointStore::new(dir.path());

        let erro = store
            .undo()
            .expect_err("pilha vazia deve falhar de forma tratada");

        assert!(matches!(erro, CheckpointError::Vazio));
    }

    #[test]
    fn record_seguido_de_undo_restaura_o_conteudo_anterior() {
        let dir = workspace_temporario();
        let arquivo = dir.path().join("a.txt");
        fs::write(&arquivo, "conteúdo original").unwrap();
        let store = CheckpointStore::new(dir.path());

        store
            .record("a.txt", Some("conteúdo original".to_string()))
            .expect("record deve funcionar");
        fs::write(&arquivo, "conteúdo novo, sobrescrito pela tool").unwrap();

        let outcome = store.undo().expect("undo deve funcionar");

        assert_eq!(outcome.path, "a.txt");
        assert_eq!(outcome.acao, UndoAcao::Restaurado);
        assert_eq!(fs::read_to_string(&arquivo).unwrap(), "conteúdo original");
    }

    #[test]
    fn undo_de_checkpoint_sem_conteudo_anterior_remove_o_arquivo() {
        let dir = workspace_temporario();
        let arquivo = dir.path().join("novo.txt");
        let store = CheckpointStore::new(dir.path());

        // `fs_write` criou um arquivo que não existia antes.
        fs::write(&arquivo, "conteúdo criado pela tool").unwrap();
        store
            .record("novo.txt", None)
            .expect("record deve funcionar");

        let outcome = store.undo().expect("undo deve funcionar");

        assert_eq!(outcome.acao, UndoAcao::Removido);
        assert!(!arquivo.exists(), "undo de criação deve remover o arquivo");
    }

    #[test]
    fn dois_checkpoints_desfazem_na_ordem_inversa_lifo() {
        let dir = workspace_temporario();
        let arquivo = dir.path().join("a.txt");
        let store = CheckpointStore::new(dir.path());

        fs::write(&arquivo, "v1").unwrap();
        store.record("a.txt", None).expect("primeiro record");
        fs::write(&arquivo, "v2").unwrap();
        store
            .record("a.txt", Some("v1".to_string()))
            .expect("segundo record");
        fs::write(&arquivo, "v3").unwrap();

        let primeiro_undo = store.undo().expect("desfaz v3 -> v2");
        assert_eq!(primeiro_undo.acao, UndoAcao::Restaurado);
        assert_eq!(fs::read_to_string(&arquivo).unwrap(), "v1");

        let segundo_undo = store.undo().expect("desfaz v2 -> criação, remove");
        assert_eq!(segundo_undo.acao, UndoAcao::Removido);
        assert!(!arquivo.exists());
    }

    #[test]
    fn teto_descarta_o_checkpoint_mais_antigo_quando_excede() {
        let dir = workspace_temporario();
        let store = CheckpointStore::new(dir.path());

        for i in 0..(TETO_CHECKPOINTS + 5) {
            store
                .record(format!("arquivo-{i}.txt"), None)
                .expect("record deve funcionar");
        }

        let entradas = store.carregar().expect("carregar deve funcionar");
        assert_eq!(entradas.len(), TETO_CHECKPOINTS);
        // O mais antigo retido deve ser o índice 5 (os 5 primeiros, 0..5,
        // foram descartados por excederem o teto).
        assert_eq!(entradas[0].path, "arquivo-5.txt");
    }

    #[test]
    fn checkpoints_persistem_em_agentry_checkpoints_json_auto_excluido_do_git() {
        let dir = workspace_temporario();
        let store = CheckpointStore::new(dir.path());

        store.record("a.txt", None).expect("record deve funcionar");

        let manifesto = dir.path().join(".agentry").join("checkpoints.json");
        assert!(manifesto.exists());
        let gitignore = dir.path().join(".agentry").join(".gitignore");
        assert!(
            gitignore.exists(),
            ".agentry/.gitignore deve existir (auto-exclusão, ADR-0017)"
        );
    }
}
