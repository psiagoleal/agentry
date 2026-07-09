// Caminho relativo: crates/core/src/context/rag/chunk.rs
//! Chunking AST-aware para RAG (MT-25, ADR-0011).
//!
//! Reaproveita a extração de símbolos do MT-18 (`ast::extract_symbols`) —
//! não duplica a detecção de função/classe/método por linguagem, que já
//! roda a *tags query* do `tree-sitter` (ADR-0010). Cada símbolo extraído
//! vira um chunk: o texto-fonte do símbolo **inteiro** (nunca truncado ou
//! partido no meio, ao contrário de chunking por tamanho fixo de token, que
//! quebraria uma função ao acaso) mais metadados (arquivo, símbolo, tipo,
//! *range*) suficientes para indexação (MT-26/27) e para correlacionar o
//! chunk de volta à origem.
//!
//! **Chunks podem se sobrepor deliberadamente:** um símbolo aninhado (ex.:
//! `fn` definida dentro do corpo de outra `fn`) produz um chunk próprio cujo
//! `range` fica contido no do símbolo externo — ambos são indexados
//! independentemente (multi-granularidade — buscar pela lógica interna
//! encontra o chunk pequeno; buscar pelo contexto da função externa
//! encontra o chunk grande), não é um defeito de deduplicação pendente.

use std::ops::Range;

use crate::context::ast::{extract_symbols, AstError, Language, SymbolKind};

/// Um chunk pronto para indexação: o texto-fonte de um símbolo completo,
/// com metadados suficientes para correlacionar de volta ao arquivo/símbolo
/// de origem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chunk {
    /// Caminho do arquivo de origem (mesma chave usada pelo repo-map, MT-19).
    pub file: String,
    /// Nome do símbolo.
    pub symbol: String,
    /// Tipo do símbolo — mesma taxonomia do MT-18.
    pub kind: SymbolKind,
    /// Extensão do símbolo no arquivo-fonte, em bytes.
    pub range: Range<usize>,
    /// Texto-fonte do chunk — sempre exatamente `source[range]`, nunca uma
    /// aproximação ou truncamento.
    pub text: String,
}

/// Gera os chunks de `source` (identificado por `file`), um por símbolo
/// função/classe/método extraído (MT-18). A ordem segue a ordem de
/// extração de [`extract_symbols`] — não é significativa por si (não é
/// necessariamente a ordem de aparição no arquivo).
///
/// # Errors
///
/// Devolve [`AstError`] nos mesmos casos de [`extract_symbols`] — falha
/// interna de parser/gramática (incompatibilidade de versão), não um
/// problema no `source` dado.
pub fn chunk_file(file: &str, source: &str, language: Language) -> Result<Vec<Chunk>, AstError> {
    let symbols = extract_symbols(source, language)?;
    Ok(symbols
        .into_iter()
        .map(|symbol| Chunk {
            file: file.to_string(),
            symbol: symbol.name,
            kind: symbol.kind,
            text: source[symbol.range.clone()].to_string(),
            range: symbol.range,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunks_de_um_arquivo_rust_tem_metadados_corretos_e_texto_completo() {
        let source = "\
struct Ponto {
    x: i32,
    y: i32,
}

impl Ponto {
    fn distancia(&self) -> f64 {
        0.0
    }
}

fn soma(a: i32, b: i32) -> i32 {
    a + b
}
";
        let chunks = chunk_file("src/geometria.rs", source, Language::Rust).expect("deve parsear");
        assert_eq!(chunks.len(), 3);

        let por_simbolo: std::collections::HashMap<_, _> =
            chunks.iter().map(|c| (c.symbol.as_str(), c)).collect();

        let ponto = por_simbolo.get("Ponto").expect("Ponto deve virar chunk");
        assert_eq!(ponto.file, "src/geometria.rs");
        assert_eq!(ponto.kind, SymbolKind::Class);
        assert_eq!(ponto.text, &source[ponto.range.clone()]);
        assert!(ponto.text.starts_with("struct Ponto"));
        assert!(
            ponto.text.contains("y: i32"),
            "chunk não deveria truncar o corpo do struct"
        );

        let distancia = por_simbolo
            .get("distancia")
            .expect("distancia deve virar chunk");
        assert_eq!(distancia.kind, SymbolKind::Method);
        assert!(distancia.text.starts_with("fn distancia"));
        assert!(
            distancia.text.trim_end().ends_with('}'),
            "chunk não deveria quebrar a função no meio — corpo completo esperado"
        );

        let soma = por_simbolo.get("soma").expect("soma deve virar chunk");
        assert_eq!(soma.kind, SymbolKind::Function);
        assert!(soma.text.starts_with("fn soma"));
        assert!(
            soma.text.contains("a + b"),
            "chunk não deveria truncar o corpo da função"
        );
    }

    #[test]
    fn chunks_de_um_arquivo_python_tem_metadados_corretos() {
        let source = "\
class Calculadora:
    def somar(self, a, b):
        return a + b


def multiplicar(a, b):
    return a * b
";
        let chunks = chunk_file("src/calc.py", source, Language::Python).expect("deve parsear");
        assert_eq!(chunks.len(), 3);

        let por_simbolo: std::collections::HashMap<_, _> =
            chunks.iter().map(|c| (c.symbol.as_str(), c)).collect();

        let calculadora = por_simbolo
            .get("Calculadora")
            .expect("Calculadora deve virar chunk");
        assert_eq!(calculadora.file, "src/calc.py");
        assert_eq!(calculadora.kind, SymbolKind::Class);
        assert!(calculadora.text.contains("def somar"));

        let multiplicar = por_simbolo
            .get("multiplicar")
            .expect("multiplicar deve virar chunk");
        assert!(multiplicar.text.trim_end().ends_with("a * b"));
    }

    #[test]
    fn simbolo_aninhado_produz_chunk_proprio_contido_no_chunk_externo() {
        let source = "\
fn externa() -> i32 {
    fn interna() -> i32 {
        1
    }
    interna() + 1
}
";
        let chunks = chunk_file("src/lib.rs", source, Language::Rust).expect("deve parsear");

        let externa = chunks
            .iter()
            .find(|c| c.symbol == "externa")
            .expect("externa deve virar chunk");
        let interna = chunks
            .iter()
            .find(|c| c.symbol == "interna")
            .expect("interna deve virar chunk próprio");

        assert!(
            externa.range.start <= interna.range.start && interna.range.end <= externa.range.end,
            "o chunk da função aninhada deve ficar contido no range da função externa"
        );
        assert!(externa.text.contains(&interna.text));
    }

    #[test]
    fn fonte_vazia_nao_produz_chunks() {
        assert_eq!(
            chunk_file("vazio.rs", "", Language::Rust).expect("deve parsear"),
            Vec::new()
        );
    }
}
