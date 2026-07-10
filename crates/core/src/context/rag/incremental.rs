// Caminho relativo: crates/core/src/context/rag/incremental.rs
//! Indexação incremental dos chunks do RAG (MT-29, ADR-0011).
//!
//! Reembeda/reindexa só arquivos alterados desde a última chamada, nunca
//! o repositório inteiro — detecção por hash de conteúdo (não `git diff`:
//! o repo-map/RAG já opera fora de um repositório git em potencial, ver
//! ADR-0017, e um hash de conteúdo é mais simples e não depende de um
//! binário `git` no `PATH`). O estado entre invocações do processo — o
//! "manifesto" arquivo→hash→chunks — persiste em
//! `<estado>/index/manifest.json`, dentro do diretório resolvido pelo
//! MT-38 (`state_dir::ensure_state_dir`); este módulo não resolve esse
//! diretório sozinho, recebe-o já pronto de quem o chama.
//!
//! Um manifesto ausente (primeira execução) ou corrompido (versão antiga
//! incompatível, escrita interrompida) **não é erro** — é tratado como
//! manifesto vazio, e todos os arquivos são reprocessados; o pior caso é
//! uma reindexação completa (o comportamento anterior a este ticket), não
//! uma indexação que falha. Já uma falha ao **escrever** o manifesto
//! atualizado é reportada como erro — significa que a próxima chamada não
//! teria como saber o que já foi indexado, degradando a funcionalidade
//! silenciosamente (proibido pelo ADR-0011: "indexação falhar
//! silenciosamente sem log observável").
//!
//! Não reimplementa a serialização de [`Chunk`] de forma genérica — ele
//! não ganha `Serialize`/`Deserialize` para isso; este módulo converte
//! para uma representação própria ([`ChunkPersistido`]), mesmo padrão já
//! usado por [`super::lexical_index`]/[`super::semantic_index`] para os
//! próprios formatos de armazenamento deles.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::chunk::{chunk_file, Chunk};
use super::{kind_from_str, kind_to_str};
use crate::context::ast::{AstError, Language};

/// Um arquivo-fonte a (re)indexar: caminho (chave do manifesto e do
/// [`Chunk::file`] resultante), conteúdo atual e linguagem para
/// [`chunk_file`].
pub struct ArquivoFonte {
    pub caminho: String,
    pub fonte: String,
    pub language: Language,
}

/// Resultado de uma chamada a [`IncrementalIndexer::reindex`].
pub struct ReindexResultado {
    /// Chunks de **todos** os arquivos dados — reaproveitados do
    /// manifesto quando o conteúdo não mudou, recém-extraídos quando
    /// mudou (ou é a primeira vez que o arquivo aparece).
    pub chunks: Vec<Chunk>,
    /// Caminhos dos arquivos que precisaram ser reprocessados nesta
    /// chamada — vazio quando nada mudou desde a chamada anterior.
    pub arquivos_reprocessados: Vec<String>,
}

/// Erros da indexação incremental — [`Ast`](IncrementalError::Ast) reflete
/// falha real de parser/gramática ao extrair símbolos de um arquivo
/// (mesmos casos de [`AstError`]); [`Manifest`](IncrementalError::Manifest)
/// reflete falha ao **escrever** o manifesto atualizado em disco (E/S).
#[derive(Debug)]
pub enum IncrementalError {
    Ast { caminho: String, causa: AstError },
    Manifest(String),
}

impl std::fmt::Display for IncrementalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ast { caminho, causa } => {
                write!(f, "falha ao extrair símbolos de '{caminho}': {causa}")
            }
            Self::Manifest(msg) => write!(f, "falha ao persistir o manifesto de índice: {msg}"),
        }
    }
}

impl std::error::Error for IncrementalError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ChunkPersistido {
    symbol: String,
    kind: String,
    range_start: usize,
    range_end: usize,
    text: String,
}

impl ChunkPersistido {
    fn de_chunk(chunk: &Chunk) -> Self {
        Self {
            symbol: chunk.symbol.clone(),
            kind: kind_to_str(chunk.kind).to_string(),
            range_start: chunk.range.start,
            range_end: chunk.range.end,
            text: chunk.text.clone(),
        }
    }

    fn para_chunk(&self, caminho: &str) -> Chunk {
        Chunk {
            file: caminho.to_string(),
            symbol: self.symbol.clone(),
            kind: kind_from_str(&self.kind),
            range: self.range_start..self.range_end,
            text: self.text.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EntradaArquivo {
    hash: u64,
    chunks: Vec<ChunkPersistido>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Manifesto {
    #[serde(default)]
    arquivos: HashMap<String, EntradaArquivo>,
}

fn hash_conteudo(fonte: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    fonte.hash(&mut hasher);
    hasher.finish()
}

/// Indexador incremental: mantém o manifesto arquivo→hash→chunks em
/// `<estado>/index/manifest.json` entre chamadas a [`Self::reindex`].
pub struct IncrementalIndexer {
    manifest_path: PathBuf,
}

impl IncrementalIndexer {
    /// `estado_dir` é o diretório já resolvido por
    /// `state_dir::ensure_state_dir` (MT-38) — este construtor não resolve
    /// nem cria nada sozinho, só decide o caminho do manifesto dentro dele.
    #[must_use]
    pub fn new(estado_dir: &Path) -> Self {
        Self {
            manifest_path: estado_dir.join("index").join("manifest.json"),
        }
    }

    /// Manifesto ausente ou corrompido não é erro — devolve um manifesto
    /// vazio, fazendo todos os arquivos parecerem "novos" (reprocessados).
    fn carregar_manifesto(&self) -> Manifesto {
        std::fs::read_to_string(&self.manifest_path)
            .ok()
            .and_then(|texto| serde_json::from_str(&texto).ok())
            .unwrap_or_default()
    }

    fn salvar_manifesto(&self, manifesto: &Manifesto) -> Result<(), IncrementalError> {
        if let Some(pai) = self.manifest_path.parent() {
            std::fs::create_dir_all(pai).map_err(|e| IncrementalError::Manifest(e.to_string()))?;
        }
        let texto = serde_json::to_string(manifesto)
            .map_err(|e| IncrementalError::Manifest(e.to_string()))?;
        std::fs::write(&self.manifest_path, texto)
            .map_err(|e| IncrementalError::Manifest(e.to_string()))
    }

    /// (Re)indexa `arquivos`: reaproveita os chunks já persistidos para
    /// quem não mudou de conteúdo desde a última chamada (mesmo hash),
    /// reextrai (via [`chunk_file`]) só quem mudou ou é novo. Arquivos
    /// presentes no manifesto anterior mas ausentes de `arquivos` desta
    /// vez são removidos do manifesto (não ficam acumulando para sempre).
    ///
    /// # Errors
    ///
    /// Devolve [`IncrementalError::Ast`] se a extração de símbolos de um
    /// arquivo alterado falhar; [`IncrementalError::Manifest`] se a
    /// escrita do manifesto atualizado falhar.
    pub fn reindex(&self, arquivos: &[ArquivoFonte]) -> Result<ReindexResultado, IncrementalError> {
        let mut manifesto = self.carregar_manifesto();
        let caminhos_atuais: std::collections::HashSet<&str> =
            arquivos.iter().map(|a| a.caminho.as_str()).collect();
        manifesto
            .arquivos
            .retain(|caminho, _| caminhos_atuais.contains(caminho.as_str()));

        let mut chunks_totais = Vec::new();
        let mut reprocessados = Vec::new();

        for arquivo in arquivos {
            let hash_atual = hash_conteudo(&arquivo.fonte);
            let entrada_reaproveitavel = manifesto
                .arquivos
                .get(&arquivo.caminho)
                .filter(|entrada| entrada.hash == hash_atual);

            let chunks_do_arquivo = if let Some(entrada) = entrada_reaproveitavel {
                entrada
                    .chunks
                    .iter()
                    .map(|c| c.para_chunk(&arquivo.caminho))
                    .collect::<Vec<_>>()
            } else {
                let novos = chunk_file(&arquivo.caminho, &arquivo.fonte, arquivo.language)
                    .map_err(|causa| IncrementalError::Ast {
                        caminho: arquivo.caminho.clone(),
                        causa,
                    })?;
                manifesto.arquivos.insert(
                    arquivo.caminho.clone(),
                    EntradaArquivo {
                        hash: hash_atual,
                        chunks: novos.iter().map(ChunkPersistido::de_chunk).collect(),
                    },
                );
                reprocessados.push(arquivo.caminho.clone());
                novos
            };

            chunks_totais.extend(chunks_do_arquivo);
        }

        self.salvar_manifesto(&manifesto)?;

        Ok(ReindexResultado {
            chunks: chunks_totais,
            arquivos_reprocessados: reprocessados,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo — sem depender de uma crate de teste nova (mesma disciplina
    /// já usada em `tools/fs.rs` do MT-12 e `state_dir.rs` do MT-38).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-incremental-test-{}-{}",
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

    fn arquivo(caminho: &str, fonte: &str) -> ArquivoFonte {
        ArquivoFonte {
            caminho: caminho.to_string(),
            fonte: fonte.to_string(),
            language: Language::Rust,
        }
    }

    const FONTE_SOMA: &str = "fn soma(a: i32, b: i32) -> i32 {\n    a + b\n}\n";
    const FONTE_SOMA_V2: &str = "fn soma(a: i32, b: i32) -> i32 {\n    a + b + 1\n}\n";
    const FONTE_MULTIPLICA: &str = "fn multiplica(a: i32, b: i32) -> i32 {\n    a * b\n}\n";

    #[test]
    fn primeira_chamada_reprocessa_todos_os_arquivos() {
        let dir = TempDir::new();
        let indexer = IncrementalIndexer::new(dir.path());

        let resultado = indexer
            .reindex(&[
                arquivo("a.rs", FONTE_SOMA),
                arquivo("b.rs", FONTE_MULTIPLICA),
            ])
            .expect("primeira indexação deve funcionar");

        assert_eq!(resultado.chunks.len(), 2);
        let mut reprocessados = resultado.arquivos_reprocessados;
        reprocessados.sort();
        assert_eq!(reprocessados, vec!["a.rs", "b.rs"]);
    }

    #[test]
    fn segunda_chamada_com_arquivos_inalterados_nao_reprocessa_nada() {
        let dir = TempDir::new();
        let indexer = IncrementalIndexer::new(dir.path());
        indexer
            .reindex(&[
                arquivo("a.rs", FONTE_SOMA),
                arquivo("b.rs", FONTE_MULTIPLICA),
            ])
            .expect("primeira indexação deve funcionar");

        let resultado = indexer
            .reindex(&[
                arquivo("a.rs", FONTE_SOMA),
                arquivo("b.rs", FONTE_MULTIPLICA),
            ])
            .expect("segunda indexação deve funcionar");

        assert!(resultado.arquivos_reprocessados.is_empty());
        assert_eq!(
            resultado.chunks.len(),
            2,
            "chunks continuam disponíveis, só reaproveitados"
        );
    }

    #[test]
    fn alterar_um_arquivo_dispara_reindexacao_so_dele() {
        let dir = TempDir::new();
        let indexer = IncrementalIndexer::new(dir.path());
        indexer
            .reindex(&[
                arquivo("a.rs", FONTE_SOMA),
                arquivo("b.rs", FONTE_MULTIPLICA),
            ])
            .expect("primeira indexação deve funcionar");

        let resultado = indexer
            .reindex(&[
                arquivo("a.rs", FONTE_SOMA_V2),    // mudou
                arquivo("b.rs", FONTE_MULTIPLICA), // igual
            ])
            .expect("segunda indexação deve funcionar");

        assert_eq!(resultado.arquivos_reprocessados, vec!["a.rs"]);
        assert_eq!(resultado.chunks.len(), 2);
        let soma = resultado
            .chunks
            .iter()
            .find(|c| c.symbol == "soma")
            .expect("soma deve continuar presente");
        assert!(
            soma.text.contains("a + b + 1"),
            "chunk reprocessado deve refletir o novo conteúdo"
        );
    }

    #[test]
    fn arquivo_removido_do_conjunto_atual_e_removido_do_manifesto() {
        let dir = TempDir::new();
        let indexer = IncrementalIndexer::new(dir.path());
        indexer
            .reindex(&[
                arquivo("a.rs", FONTE_SOMA),
                arquivo("b.rs", FONTE_MULTIPLICA),
            ])
            .expect("primeira indexação deve funcionar");

        let resultado = indexer
            .reindex(&[arquivo("a.rs", FONTE_SOMA)]) // b.rs não existe mais
            .expect("segunda indexação deve funcionar");

        assert!(resultado.arquivos_reprocessados.is_empty());
        assert_eq!(resultado.chunks.len(), 1);

        let manifesto = indexer.carregar_manifesto();
        assert!(!manifesto.arquivos.contains_key("b.rs"));
    }

    #[test]
    fn manifesto_corrompido_nao_e_erro_reprocessa_tudo() {
        let dir = TempDir::new();
        let indexer = IncrementalIndexer::new(dir.path());
        std::fs::create_dir_all(dir.path().join("index")).expect("cria dir do índice");
        std::fs::write(
            dir.path().join("index").join("manifest.json"),
            "{ isso não é json",
        )
        .expect("escreve manifesto corrompido");

        let resultado = indexer
            .reindex(&[arquivo("a.rs", FONTE_SOMA)])
            .expect("manifesto corrompido não deve impedir a indexação");

        assert_eq!(resultado.arquivos_reprocessados, vec!["a.rs"]);
        assert_eq!(resultado.chunks.len(), 1);
    }
}
