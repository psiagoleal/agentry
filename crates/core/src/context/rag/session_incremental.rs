// Caminho relativo: crates/core/src/context/rag/session_incremental.rs
//! Indexação incremental dos chunks de sessão (MT-135, ADR-0039).
//!
//! **Mais simples que [`super::incremental`] (código):** sessões salvas
//! são *write-once* (`sessao::salvar`, `crates/cli/src/sessao.rs`, sempre
//! gera um `id` novo com *timestamp* — nunca sobrescreve um arquivo já
//! existente, confirmado na ADR-0039 §1). Não há *diff* de conteúdo
//! mutável a detectar (ao contrário de um arquivo de código, que pode ser
//! editado): uma sessão já presente no manifesto **nunca** precisa ser
//! reprocessada — só sessões novas (ausentes do manifesto) disparam
//! [`super::session_chunk::chunk_session`]. Por isso este módulo não usa
//! hash de conteúdo — presença no manifesto já é sinal suficiente.
//!
//! Mesmo formato de manifesto persistido em
//! `<estado>/index/session_manifest.json` (dentro do diretório resolvido
//! por `state_dir::ensure_state_dir`, MT-38) e mesma filosofia de
//! "manifesto ausente/corrompido não é erro, reprocessa tudo" do MT-29.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::session_chunk::{chunk_session, SessionChunk};
use crate::model::{Message, Role};

/// Uma sessão salva a (re)indexar: `session_id` (chave do manifesto e do
/// `SessionChunk::session_id` resultante) e as mensagens já desserializadas
/// (via `session::persist::desserializar_de_markdown`, MT-120).
pub struct SessaoFonte {
    pub session_id: String,
    pub mensagens: Vec<Message>,
}

/// Resultado de uma chamada a [`SessionIncrementalIndexer::reindex`].
pub struct SessionReindexResultado {
    /// Chunks de **todas** as sessões dadas — reaproveitados do manifesto
    /// quando já indexados, recém-gerados quando é a primeira vez que a
    /// sessão aparece.
    pub chunks: Vec<SessionChunk>,
    /// `session_id`s que precisaram ser processados nesta chamada — vazio
    /// quando nenhuma sessão nova apareceu desde a chamada anterior.
    pub sessoes_reprocessadas: Vec<String>,
}

/// Erros da indexação incremental de sessão — reflete só falha ao
/// **escrever** o manifesto atualizado em disco (E/S); `chunk_session` é
/// uma função pura que nunca falha, então não há equivalente ao
/// `IncrementalError::Ast` do lado de código.
#[derive(Debug)]
pub enum SessionIncrementalError {
    Manifest(String),
}

impl std::fmt::Display for SessionIncrementalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Manifest(msg) => write!(
                f,
                "falha ao persistir o manifesto de índice de sessão: {msg}"
            ),
        }
    }
}

impl std::error::Error for SessionIncrementalError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionChunkPersistido {
    message_index: usize,
    role: String,
    text: String,
}

impl SessionChunkPersistido {
    fn de_chunk(chunk: &SessionChunk) -> Self {
        Self {
            message_index: chunk.message_index,
            role: role_to_str(chunk.role).to_string(),
            text: chunk.text.clone(),
        }
    }

    fn para_chunk(&self, session_id: &str) -> SessionChunk {
        SessionChunk {
            session_id: session_id.to_string(),
            message_index: self.message_index,
            role: role_from_str(&self.role),
            text: self.text.clone(),
        }
    }
}

fn role_to_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        Role::System => "system",
        Role::Tool => "tool",
    }
}

fn role_from_str(role: &str) -> Role {
    match role {
        "assistant" => Role::Assistant,
        "system" => Role::System,
        "tool" => Role::Tool,
        _ => Role::User,
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifesto {
    #[serde(default)]
    sessoes: HashMap<String, Vec<SessionChunkPersistido>>,
}

/// Indexador incremental: mantém o manifesto `session_id`→chunks em
/// `<estado>/index/session_manifest.json` entre chamadas a
/// [`Self::reindex`].
pub struct SessionIncrementalIndexer {
    manifest_path: PathBuf,
}

impl SessionIncrementalIndexer {
    /// `estado_dir` é o diretório já resolvido por
    /// `state_dir::ensure_state_dir` (MT-38) — este construtor não resolve
    /// nem cria nada sozinho, só decide o caminho do manifesto dentro dele.
    #[must_use]
    pub fn new(estado_dir: &Path) -> Self {
        Self {
            manifest_path: estado_dir.join("index").join("session_manifest.json"),
        }
    }

    /// Manifesto ausente ou corrompido não é erro — devolve um manifesto
    /// vazio, fazendo todas as sessões parecerem "novas" (reprocessadas).
    fn carregar_manifesto(&self) -> Manifesto {
        std::fs::read_to_string(&self.manifest_path)
            .ok()
            .and_then(|texto| serde_json::from_str(&texto).ok())
            .unwrap_or_default()
    }

    fn salvar_manifesto(&self, manifesto: &Manifesto) -> Result<(), SessionIncrementalError> {
        if let Some(pai) = self.manifest_path.parent() {
            std::fs::create_dir_all(pai)
                .map_err(|e| SessionIncrementalError::Manifest(e.to_string()))?;
        }
        let texto = serde_json::to_string(manifesto)
            .map_err(|e| SessionIncrementalError::Manifest(e.to_string()))?;
        std::fs::write(&self.manifest_path, texto)
            .map_err(|e| SessionIncrementalError::Manifest(e.to_string()))
    }

    /// (Re)indexa `sessoes`: reaproveita os chunks já persistidos para toda
    /// sessão já presente no manifesto (write-once — nunca precisa
    /// reprocessar), chunka (via [`chunk_session`]) só as sessões novas.
    /// Sessões presentes no manifesto anterior mas ausentes de `sessoes`
    /// desta vez são removidas do manifesto (não ficam acumulando para
    /// sempre — mesmo comportamento do MT-29 para arquivos removidos).
    ///
    /// # Errors
    ///
    /// Devolve [`SessionIncrementalError::Manifest`] se a escrita do
    /// manifesto atualizado falhar.
    pub fn reindex(
        &self,
        sessoes: &[SessaoFonte],
    ) -> Result<SessionReindexResultado, SessionIncrementalError> {
        let mut manifesto = self.carregar_manifesto();
        let ids_atuais: std::collections::HashSet<&str> =
            sessoes.iter().map(|s| s.session_id.as_str()).collect();
        manifesto
            .sessoes
            .retain(|id, _| ids_atuais.contains(id.as_str()));

        let mut chunks_totais = Vec::new();
        let mut reprocessadas = Vec::new();

        for sessao in sessoes {
            let chunks_da_sessao =
                if let Some(persistidos) = manifesto.sessoes.get(&sessao.session_id) {
                    persistidos
                        .iter()
                        .map(|p| p.para_chunk(&sessao.session_id))
                        .collect::<Vec<_>>()
                } else {
                    let novos = chunk_session(&sessao.session_id, &sessao.mensagens);
                    manifesto.sessoes.insert(
                        sessao.session_id.clone(),
                        novos.iter().map(SessionChunkPersistido::de_chunk).collect(),
                    );
                    reprocessadas.push(sessao.session_id.clone());
                    novos
                };

            chunks_totais.extend(chunks_da_sessao);
        }

        self.salvar_manifesto(&manifesto)?;

        Ok(SessionReindexResultado {
            chunks: chunks_totais,
            sessoes_reprocessadas: reprocessadas,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-session-incremental-test-{}-{}",
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

    fn sessao(id: &str, mensagens: Vec<Message>) -> SessaoFonte {
        SessaoFonte {
            session_id: id.to_string(),
            mensagens,
        }
    }

    #[test]
    fn primeira_chamada_reprocessa_todas_as_sessoes() {
        let dir = TempDir::new();
        let indexer = SessionIncrementalIndexer::new(dir.path());

        let resultado = indexer
            .reindex(&[
                sessao("s1", vec![Message::user("pergunta 1")]),
                sessao("s2", vec![Message::user("pergunta 2")]),
            ])
            .expect("primeira indexação deve funcionar");

        assert_eq!(resultado.chunks.len(), 2);
        let mut reprocessadas = resultado.sessoes_reprocessadas;
        reprocessadas.sort();
        assert_eq!(reprocessadas, vec!["s1", "s2"]);
    }

    #[test]
    fn segunda_chamada_sem_mudancas_nao_reprocessa_nada() {
        let dir = TempDir::new();
        let indexer = SessionIncrementalIndexer::new(dir.path());
        indexer
            .reindex(&[sessao("s1", vec![Message::user("pergunta 1")])])
            .expect("primeira indexação deve funcionar");

        let resultado = indexer
            .reindex(&[sessao("s1", vec![Message::user("pergunta 1")])])
            .expect("segunda indexação deve funcionar");

        assert!(resultado.sessoes_reprocessadas.is_empty());
        assert_eq!(
            resultado.chunks.len(),
            1,
            "chunks continuam disponíveis, só reaproveitados"
        );
    }

    #[test]
    fn sessao_nova_e_detectada_e_indexada_sem_reprocessar_as_ja_indexadas() {
        let dir = TempDir::new();
        let indexer = SessionIncrementalIndexer::new(dir.path());
        indexer
            .reindex(&[sessao("s1", vec![Message::user("pergunta 1")])])
            .expect("primeira indexação deve funcionar");

        let resultado = indexer
            .reindex(&[
                sessao("s1", vec![Message::user("pergunta 1")]),
                sessao("s2", vec![Message::user("pergunta 2")]), // nova
            ])
            .expect("segunda indexação deve funcionar");

        assert_eq!(resultado.sessoes_reprocessadas, vec!["s2"]);
        assert_eq!(resultado.chunks.len(), 2);
    }

    #[test]
    fn sessao_ja_indexada_reaproveita_do_manifesto_mesmo_que_o_input_mude() {
        // Documenta a suposição de "write-once": ao contrário do lado de
        // código (que detecta mudança por hash de conteúdo), uma sessão já
        // presente no manifesto NUNCA é reprocessada, mesmo que as
        // mensagens dadas nesta chamada sejam diferentes -- premissa
        // válida porque `sessao::salvar` nunca reescreve um arquivo de
        // sessão já existente.
        let dir = TempDir::new();
        let indexer = SessionIncrementalIndexer::new(dir.path());
        indexer
            .reindex(&[sessao("s1", vec![Message::user("pergunta original")])])
            .expect("primeira indexação deve funcionar");

        let resultado = indexer
            .reindex(&[sessao("s1", vec![Message::user("pergunta diferente")])])
            .expect("segunda indexação deve funcionar");

        assert!(resultado.sessoes_reprocessadas.is_empty());
        assert_eq!(resultado.chunks[0].text, "pergunta original");
    }

    #[test]
    fn sessao_removida_do_conjunto_atual_e_removida_do_manifesto() {
        let dir = TempDir::new();
        let indexer = SessionIncrementalIndexer::new(dir.path());
        indexer
            .reindex(&[
                sessao("s1", vec![Message::user("pergunta 1")]),
                sessao("s2", vec![Message::user("pergunta 2")]),
            ])
            .expect("primeira indexação deve funcionar");

        let resultado = indexer
            .reindex(&[sessao("s1", vec![Message::user("pergunta 1")])]) // s2 não existe mais
            .expect("segunda indexação deve funcionar");

        assert!(resultado.sessoes_reprocessadas.is_empty());
        assert_eq!(resultado.chunks.len(), 1);

        let manifesto = indexer.carregar_manifesto();
        assert!(!manifesto.sessoes.contains_key("s2"));
    }

    #[test]
    fn manifesto_corrompido_nao_e_erro_reprocessa_tudo() {
        let dir = TempDir::new();
        let indexer = SessionIncrementalIndexer::new(dir.path());
        std::fs::create_dir_all(dir.path().join("index")).expect("cria dir do índice");
        std::fs::write(
            dir.path().join("index").join("session_manifest.json"),
            "{ isso não é json",
        )
        .expect("escreve manifesto corrompido");

        let resultado = indexer
            .reindex(&[sessao("s1", vec![Message::user("pergunta 1")])])
            .expect("manifesto corrompido não deve impedir a indexação");

        assert_eq!(resultado.sessoes_reprocessadas, vec!["s1"]);
        assert_eq!(resultado.chunks.len(), 1);
    }
}
