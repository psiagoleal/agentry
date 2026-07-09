// Caminho relativo: crates/core/src/context/rag/lexical_index.rs
//! Índice lexical (BM25) sobre os chunks do RAG (MT-26, ADR-0011).
//!
//! Usa `tantivy` — embutido/*in-process*, sem servidor externo nem ponte
//! FFI (ADR-0011) — para permitir consulta por identificador exato (nome de
//! função/variável), o caso em que busca puramente semântica (MT-27) erra.
//! Complementa, não substitui, o índice semântico; a combinação dos dois
//! (busca híbrida + *reranking*) é o MT-28.

use std::ops::Range;

use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, Schema, Value, STORED, STRING, TEXT};
use tantivy::{doc, Index, IndexReader, IndexWriter, TantivyDocument};

use super::chunk::Chunk;
use crate::context::ast::SymbolKind;

/// Erros do índice lexical — todos indicam falha interna do `tantivy`
/// (schema/consulta malformados, escrita no índice), não um problema nos
/// chunks ou na consulta dados pelo chamador em uso normal.
#[derive(Debug)]
pub enum LexicalIndexError {
    Tantivy(String),
    QueryParse(String),
}

impl std::fmt::Display for LexicalIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tantivy(msg) => write!(f, "erro no índice lexical (tantivy): {msg}"),
            Self::QueryParse(msg) => write!(f, "consulta lexical inválida: {msg}"),
        }
    }
}

impl std::error::Error for LexicalIndexError {}

fn kind_to_str(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Function => "function",
        SymbolKind::Method => "method",
        SymbolKind::Class => "class",
    }
}

fn kind_from_str(kind: &str) -> SymbolKind {
    match kind {
        "method" => SymbolKind::Method,
        "class" => SymbolKind::Class,
        _ => SymbolKind::Function,
    }
}

struct Campos {
    file: Field,
    symbol: Field,
    kind: Field,
    range_start: Field,
    range_end: Field,
    text: Field,
}

fn construir_schema() -> (Schema, Campos) {
    let mut builder = Schema::builder();
    let file = builder.add_text_field("file", STRING | STORED);
    let symbol = builder.add_text_field("symbol", TEXT | STORED);
    let kind = builder.add_text_field("kind", STRING | STORED);
    let range_start = builder.add_u64_field("range_start", STORED);
    let range_end = builder.add_u64_field("range_end", STORED);
    let text = builder.add_text_field("text", TEXT | STORED);
    let schema = builder.build();
    (
        schema,
        Campos {
            file,
            symbol,
            kind,
            range_start,
            range_end,
            text,
        },
    )
}

/// Índice lexical (BM25) sobre um conjunto de chunks (MT-25).
///
/// Embutido/*in-process* (sem servidor externo) — construído uma vez a
/// partir de `Vec<Chunk>` via [`LexicalIndex::build`], consultado via
/// [`LexicalIndex::search`].
pub struct LexicalIndex {
    index: Index,
    reader: IndexReader,
    campos: Campos,
}

impl LexicalIndex {
    /// Constrói o índice a partir dos chunks dados. Consome `chunks`: o
    /// texto original já foi copiado para dentro do índice (campo
    /// `STORED`) e é reconstruído em [`LexicalIndex::search`].
    ///
    /// # Errors
    ///
    /// Devolve [`LexicalIndexError::Tantivy`] se a escrita no índice
    /// falhar — falha interna do `tantivy`, não um problema nos chunks
    /// dados.
    pub fn build(chunks: Vec<Chunk>) -> Result<Self, LexicalIndexError> {
        let (schema, campos) = construir_schema();
        let index = Index::create_in_ram(schema);
        let mut writer: IndexWriter = index
            .writer(50_000_000)
            .map_err(|e| LexicalIndexError::Tantivy(e.to_string()))?;

        for chunk in chunks {
            writer
                .add_document(doc!(
                    campos.file => chunk.file,
                    campos.symbol => chunk.symbol,
                    campos.kind => kind_to_str(chunk.kind).to_string(),
                    campos.range_start => chunk.range.start as u64,
                    campos.range_end => chunk.range.end as u64,
                    campos.text => chunk.text,
                ))
                .map_err(|e| LexicalIndexError::Tantivy(e.to_string()))?;
        }
        writer
            .commit()
            .map_err(|e| LexicalIndexError::Tantivy(e.to_string()))?;

        let reader = index
            .reader()
            .map_err(|e| LexicalIndexError::Tantivy(e.to_string()))?;

        Ok(Self {
            index,
            reader,
            campos,
        })
    }

    /// Consulta o índice por `query` (BM25), devolvendo até `limite` chunks
    /// reconstruídos a partir dos campos armazenados, do mais para o menos
    /// relevante. O campo `symbol` tem peso maior que `text` — uma consulta
    /// por identificador exato (nome de função/variável) deve encontrar o
    /// chunk correspondente no topo, não apenas em meio a ocorrências
    /// incidentais do termo no corpo de outros chunks.
    ///
    /// # Errors
    ///
    /// Devolve [`LexicalIndexError::QueryParse`] se `query` não for uma
    /// consulta válida na sintaxe do `tantivy`; [`LexicalIndexError::Tantivy`]
    /// se a busca em si falhar internamente.
    pub fn search(&self, query: &str, limite: usize) -> Result<Vec<Chunk>, LexicalIndexError> {
        let searcher = self.reader.searcher();
        let mut parser =
            QueryParser::for_index(&self.index, vec![self.campos.symbol, self.campos.text]);
        parser.set_field_boost(self.campos.symbol, 2.0);

        let query = parser
            .parse_query(query)
            .map_err(|e| LexicalIndexError::QueryParse(e.to_string()))?;

        let top_docs = searcher
            .search(&query, &TopDocs::with_limit(limite).order_by_score())
            .map_err(|e| LexicalIndexError::Tantivy(e.to_string()))?;

        top_docs
            .into_iter()
            .map(|(_score, endereco)| {
                let doc: TantivyDocument = searcher
                    .doc(endereco)
                    .map_err(|e| LexicalIndexError::Tantivy(e.to_string()))?;
                Ok(self.doc_para_chunk(&doc))
            })
            .collect()
    }

    fn doc_para_chunk(&self, doc: &TantivyDocument) -> Chunk {
        let texto_de = |campo: Field| -> String {
            doc.get_first(campo)
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string()
        };
        let u64_de =
            |campo: Field| -> u64 { doc.get_first(campo).and_then(|v| v.as_u64()).unwrap_or(0) };

        let range_start = u64_de(self.campos.range_start) as usize;
        let range_end = u64_de(self.campos.range_end) as usize;

        Chunk {
            file: texto_de(self.campos.file),
            symbol: texto_de(self.campos.symbol),
            kind: kind_from_str(&texto_de(self.campos.kind)),
            range: Range {
                start: range_start,
                end: range_end,
            },
            text: texto_de(self.campos.text),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ast::Language;
    use crate::context::rag::chunk::chunk_file;

    fn chunks_de_exemplo() -> Vec<Chunk> {
        let source = "\
fn soma(a: i32, b: i32) -> i32 {
    a + b
}

fn multiplica(a: i32, b: i32) -> i32 {
    a * b
}

struct Ponto {
    x: i32,
    y: i32,
}
";
        chunk_file("src/lib.rs", source, Language::Rust).expect("deve parsear")
    }

    #[test]
    fn consulta_por_identificador_exato_devolve_o_chunk_esperado_no_topo() {
        let indice = LexicalIndex::build(chunks_de_exemplo()).expect("deve construir o índice");

        let resultados = indice
            .search("multiplica", 5)
            .expect("busca deve funcionar");

        assert!(
            !resultados.is_empty(),
            "deveria encontrar ao menos um chunk"
        );
        assert_eq!(resultados[0].symbol, "multiplica");
    }

    #[test]
    fn consulta_sem_correspondencia_devolve_lista_vazia() {
        let indice = LexicalIndex::build(chunks_de_exemplo()).expect("deve construir o índice");

        let resultados = indice
            .search("identificador_que_nao_existe_em_lugar_nenhum", 5)
            .expect("busca deve funcionar mesmo sem resultado");

        assert!(resultados.is_empty());
    }

    #[test]
    fn limite_restringe_a_quantidade_de_resultados() {
        let indice = LexicalIndex::build(chunks_de_exemplo()).expect("deve construir o índice");

        // "a" aparece no corpo de soma/multiplica e no struct Ponto — mas o
        // limite restringe a busca a 1 resultado mesmo havendo mais matches.
        let resultados = indice.search("a", 1).expect("busca deve funcionar");

        assert_eq!(resultados.len(), 1);
    }

    #[test]
    fn chunk_reconstruido_preserva_todos_os_metadados() {
        let indice = LexicalIndex::build(chunks_de_exemplo()).expect("deve construir o índice");

        let resultados = indice.search("soma", 5).expect("busca deve funcionar");
        let soma = resultados
            .into_iter()
            .find(|c| c.symbol == "soma")
            .expect("soma deve ter sido indexado e encontrado");

        assert_eq!(soma.file, "src/lib.rs");
        assert_eq!(soma.kind, SymbolKind::Function);
        assert!(soma.text.contains("a + b"));
        assert_eq!(soma.range.end - soma.range.start, soma.text.len());
    }
}
