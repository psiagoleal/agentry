// Caminho relativo: crates/core/src/context/rag/session_lexical_index.rs
//! Índice lexical (BM25) sobre os chunks de sessão (MT-132, ADR-0039).
//!
//! Análogo a [`super::lexical_index`] (código), mesma técnica (`tantivy`,
//! embutido/*in-process*) sobre um *schema* próprio de
//! [`super::session_chunk::SessionChunk`] — permite consulta por termo
//! exato (nome, identificador de arquivo, palavra específica) que a busca
//! puramente semântica (MT-133) erra. Complementa, não substitui, o índice
//! semântico; a combinação dos dois fica no MT-134.

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument};

use super::session_chunk::SessionChunk;
use crate::model::Role;

/// Erros do índice lexical de sessão — mesma semântica de
/// [`super::lexical_index::LexicalIndexError`]: indicam falha interna do
/// `tantivy`, não um problema nos chunks/consulta dados pelo chamador.
#[derive(Debug)]
pub enum SessionLexicalIndexError {
    Tantivy(String),
    QueryParse(String),
}

impl std::fmt::Display for SessionLexicalIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tantivy(msg) => write!(f, "erro no índice lexical de sessão (tantivy): {msg}"),
            Self::QueryParse(msg) => write!(f, "consulta lexical inválida: {msg}"),
        }
    }
}

impl std::error::Error for SessionLexicalIndexError {}

struct Campos {
    session_id: Field,
    message_index: Field,
    role: Field,
    text: Field,
}

fn construir_schema() -> (Schema, Campos) {
    let mut builder = Schema::builder();
    let session_id = builder.add_text_field("session_id", STRING | STORED);
    let message_index = builder.add_u64_field("message_index", STORED);
    let role = builder.add_text_field("role", STRING | STORED);
    let text = builder.add_text_field("text", TEXT | STORED);
    let schema = builder.build();
    (
        schema,
        Campos {
            session_id,
            message_index,
            role,
            text,
        },
    )
}

fn role_to_str(role: Role) -> &'static str {
    match role {
        Role::User => "user",
        Role::Assistant => "assistant",
        // Nunca aparecem num `SessionChunk` de verdade (filtrados em
        // `chunk_session`) — mapeamento total só para não precisar de
        // `Option`/`panic!` aqui; nunca exercitado em uso normal.
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

/// Índice lexical (BM25) sobre um conjunto de [`SessionChunk`]s.
///
/// Embutido/*in-process*, construído uma vez via
/// [`SessionLexicalIndex::build`], consultado via
/// [`SessionLexicalIndex::search`].
pub struct SessionLexicalIndex {
    index: Index,
    reader: IndexReader,
    campos: Campos,
}

impl SessionLexicalIndex {
    /// Constrói o índice a partir dos chunks dados. Consome `chunks`: o
    /// texto original já foi copiado para dentro do índice (campo
    /// `STORED`) e é reconstruído em [`SessionLexicalIndex::search`].
    ///
    /// # Errors
    ///
    /// Devolve [`SessionLexicalIndexError::Tantivy`] se a escrita no
    /// índice falhar — falha interna do `tantivy`, não um problema nos
    /// chunks dados.
    pub fn build(chunks: Vec<SessionChunk>) -> Result<Self, SessionLexicalIndexError> {
        let (schema, campos) = construir_schema();
        let index = Index::create_in_ram(schema);
        let mut writer: IndexWriter = index
            .writer(50_000_000)
            .map_err(|e| SessionLexicalIndexError::Tantivy(e.to_string()))?;

        for chunk in chunks {
            writer
                .add_document(doc!(
                    campos.session_id => chunk.session_id,
                    campos.message_index => chunk.message_index as u64,
                    campos.role => role_to_str(chunk.role).to_string(),
                    campos.text => chunk.text,
                ))
                .map_err(|e| SessionLexicalIndexError::Tantivy(e.to_string()))?;
        }
        writer
            .commit()
            .map_err(|e| SessionLexicalIndexError::Tantivy(e.to_string()))?;

        let reader = index
            .reader()
            .map_err(|e| SessionLexicalIndexError::Tantivy(e.to_string()))?;

        Ok(Self {
            index,
            reader,
            campos,
        })
    }

    /// Consulta o índice por `query` (BM25), devolvendo até `limite`
    /// chunks reconstruídos a partir dos campos armazenados, do mais para
    /// o menos relevante.
    ///
    /// # Errors
    ///
    /// Devolve [`SessionLexicalIndexError::QueryParse`] se `query` não for
    /// uma consulta válida na sintaxe do `tantivy`;
    /// [`SessionLexicalIndexError::Tantivy`] se a busca em si falhar
    /// internamente.
    pub fn search(
        &self,
        query: &str,
        limite: usize,
    ) -> Result<Vec<SessionChunk>, SessionLexicalIndexError> {
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.campos.text]);

        let query = parser
            .parse_query(query)
            .map_err(|e| SessionLexicalIndexError::QueryParse(e.to_string()))?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limite).order_by_score())
            .map_err(|e| SessionLexicalIndexError::Tantivy(e.to_string()))?;

        top_docs
            .into_iter()
            .map(|(_score, endereco)| {
                let doc: TantivyDocument = searcher
                    .doc(endereco)
                    .map_err(|e| SessionLexicalIndexError::Tantivy(e.to_string()))?;
                Ok(self.doc_para_chunk(&doc))
            })
            .collect()
    }

    fn doc_para_chunk(&self, doc: &TantivyDocument) -> SessionChunk {
        let texto_de = |campo: Field| -> String {
            doc.get_first(campo)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let u64_de =
            |campo: Field| -> u64 { doc.get_first(campo).and_then(|v| v.as_u64()).unwrap_or(0) };

        SessionChunk {
            session_id: texto_de(self.campos.session_id),
            message_index: u64_de(self.campos.message_index) as usize,
            role: role_from_str(&texto_de(self.campos.role)),
            text: texto_de(self.campos.text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::rag::session_chunk::chunk_session;
    use crate::model::Message;

    fn chunks_de_exemplo() -> Vec<SessionChunk> {
        let mensagens = vec![
            Message::user("qual a capital da frança"),
            Message::assistant("a capital da frança é paris"),
            Message::user("e da alemanha"),
            Message::assistant("berlim é a capital da alemanha"),
        ];
        chunk_session("20260724-000000", &mensagens)
    }

    #[test]
    fn consulta_por_termo_exato_devolve_o_chunk_esperado_no_topo() {
        let indice =
            SessionLexicalIndex::build(chunks_de_exemplo()).expect("deve construir o índice");

        let resultados = indice.search("berlim", 5).expect("busca deve funcionar");

        assert!(
            !resultados.is_empty(),
            "deveria encontrar ao menos um chunk"
        );
        assert!(resultados[0].text.contains("berlim"));
    }

    #[test]
    fn consulta_sem_correspondencia_devolve_lista_vazia() {
        let indice =
            SessionLexicalIndex::build(chunks_de_exemplo()).expect("deve construir o índice");

        let resultados = indice
            .search("identificador_que_nao_existe_em_lugar_nenhum", 5)
            .expect("busca deve funcionar mesmo sem resultado");

        assert!(resultados.is_empty());
    }

    #[test]
    fn limite_restringe_a_quantidade_de_resultados() {
        let indice =
            SessionLexicalIndex::build(chunks_de_exemplo()).expect("deve construir o índice");

        // "capital" aparece em três das quatro mensagens -- o limite
        // restringe a busca a 1 resultado mesmo havendo mais matches.
        let resultados = indice.search("capital", 1).expect("busca deve funcionar");

        assert_eq!(resultados.len(), 1);
    }

    #[test]
    fn chunk_reconstruido_preserva_todos_os_metadados() {
        let indice =
            SessionLexicalIndex::build(chunks_de_exemplo()).expect("deve construir o índice");

        let resultados = indice.search("berlim", 5).expect("busca deve funcionar");
        let chunk = &resultados[0];

        assert_eq!(chunk.session_id, "20260724-000000");
        assert_eq!(chunk.role, Role::Assistant);
        assert_eq!(chunk.message_index, 3);
    }
}
