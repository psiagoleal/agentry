// Caminho relativo: crates/core/src/context/ast.rs
//! Extração de símbolos AST-aware via `tree-sitter` (MT-18, ADR-0010).
//!
//! Reaproveita a *tags query* (`TAGS_QUERY`) que cada gramática já publica —
//! a mesma convenção usada por ferramentas de navegação de código (o
//! repo-map do Aider, a busca de símbolos do GitHub) — em vez de
//! reimplementar a detecção de símbolo nó a nó por linguagem. Cada consulta
//! captura o nome (`@name`) e a extensão do símbolo (`@definition.<tipo>`)
//! juntos, no mesmo casamento de padrão; esta extração só usa as captures
//! `definition.function`/`definition.method`/`definition.class` — as demais
//! (`definition.module`, `reference.call` etc., já presentes na mesma
//! query) ficam para quando o grafo de referências (MT-19) precisar delas.
//!
//! **Assimetria conhecida entre gramáticas:** a *tags query* do Rust
//! distingue `definition.method` (função dentro de um bloco `impl`) de
//! `definition.function` (função solta); a do Python **não** distingue —
//! todo `def`, dentro ou fora de uma classe, é `definition.function`. Isto
//! é convenção da própria gramática upstream, não uma limitação deste
//! módulo; documentado aqui para não surpreender quem consumir [`Symbol::kind`].

use std::ops::Range;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

/// Linguagem suportada para extração de símbolos.
///
/// Gramáticas adotadas individualmente, cada uma vetada por
/// maturidade/licença (ADR-0004) — sem adoção em lote conforme o suporte a
/// linguagens for sendo ampliado (ADR-0010).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    Rust,
    Python,
}

impl Language {
    fn ts_language(self) -> tree_sitter::Language {
        match self {
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Python => tree_sitter_python::LANGUAGE.into(),
        }
    }

    fn tags_query(self) -> &'static str {
        match self {
            Self::Rust => tree_sitter_rust::TAGS_QUERY,
            Self::Python => tree_sitter_python::TAGS_QUERY,
        }
    }
}

/// Tipo de símbolo extraído — mesma taxonomia da *tags query* upstream
/// (`definition.function`/`.method`/`.class`); demais captures da query são
/// ignoradas nesta v0.1 (fora de escopo do MT-18).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    /// Função solta (Rust: `fn` fora de `impl`; Python: qualquer `def`).
    Function,
    /// Método (Rust: `fn` dentro de um bloco `impl`). A gramática do Python
    /// não distingue método de função solta — ver nota de módulo.
    Method,
    /// Definição de tipo nomeado (Rust: `struct`/`enum`/`union`/`type`;
    /// Python: `class`).
    Class,
}

impl SymbolKind {
    fn from_capture_name(nome: &str) -> Option<Self> {
        match nome {
            "definition.function" => Some(Self::Function),
            "definition.method" => Some(Self::Method),
            "definition.class" => Some(Self::Class),
            _ => None,
        }
    }
}

/// Um símbolo extraído: nome, tipo e extensão (em bytes) no arquivo-fonte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Symbol {
    /// Nome do símbolo (identificador).
    pub name: String,
    /// Tipo do símbolo.
    pub kind: SymbolKind,
    /// Extensão do símbolo no arquivo-fonte, em bytes — do início da
    /// definição (ex.: a palavra-chave `fn`/`class`) ao fim do seu corpo.
    pub range: Range<usize>,
}

/// Erros de extração — ambos indicam um problema interno (incompatibilidade
/// de versão entre `tree-sitter` e a gramática), não um problema no
/// código-fonte dado.
#[derive(Debug)]
pub enum AstError {
    /// O parser não aceitou a gramática da linguagem.
    LanguageSetup(String),
    /// A *tags query* da gramática não compilou.
    QueryCompile(String),
}

impl core::fmt::Display for AstError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::LanguageSetup(msg) => write!(f, "falha ao configurar a linguagem: {msg}"),
            Self::QueryCompile(msg) => write!(f, "falha ao compilar a tags query: {msg}"),
        }
    }
}

impl std::error::Error for AstError {}

/// Extrai os símbolos de nível função/classe/método de `source`.
///
/// `source` malformado não é erro: o `tree-sitter` é tolerante a erro de
/// sintaxe e produz uma árvore parcial — símbolos que ainda casam com a
/// *tags query* são extraídos normalmente; o restante é ignorado em
/// silêncio (mesma postura best-effort de ferramentas de navegação de
/// código sobre um repositório real, onde arquivos podem estar
/// temporariamente inválidos).
///
/// # Errors
///
/// Devolve [`AstError`] se o parser não aceitar a gramática ou a *tags
/// query* da linguagem não compilar.
pub fn extract_symbols(source: &str, language: Language) -> Result<Vec<Symbol>, AstError> {
    let ts_language = language.ts_language();

    let mut parser = Parser::new();
    parser
        .set_language(&ts_language)
        .map_err(|e| AstError::LanguageSetup(e.to_string()))?;

    let tree = parser
        .parse(source, None)
        .expect("parse não deveria retornar None sem timeout/cancelamento configurado");

    let query = Query::new(&ts_language, language.tags_query())
        .map_err(|e| AstError::QueryCompile(e.to_string()))?;

    let mut symbols = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        let mut nome: Option<&str> = None;
        let mut definicao: Option<(SymbolKind, Range<usize>)> = None;

        for capture in m.captures {
            let nome_capture = query.capture_names()[capture.index as usize];
            if nome_capture == "name" {
                nome = capture.node.utf8_text(source.as_bytes()).ok();
            } else if let Some(kind) = SymbolKind::from_capture_name(nome_capture) {
                definicao = Some((kind, capture.node.byte_range()));
            }
        }

        if let (Some(nome), Some((kind, range))) = (nome, definicao) {
            merge_symbol(
                &mut symbols,
                Symbol {
                    name: nome.to_string(),
                    kind,
                    range,
                },
            );
        }
    }

    Ok(symbols)
}

/// Insere `symbol`, ou refina o já existente no mesmo `range`.
///
/// A *tags query* do Rust casa o mesmo nó de `fn` dentro de um `impl` **duas
/// vezes** — uma como `definition.method` (padrão específico, exige o `fn`
/// estar dentro de `declaration_list`) e outra como `definition.function`
/// (padrão genérico, casa qualquer `fn`) — produzindo dois matches
/// independentes para o mesmo símbolo. Quando dois símbolos compartilham o
/// mesmo `range`, a classificação mais específica (`Method`) vence sobre a
/// genérica (`Function`), em qualquer ordem de chegada.
fn merge_symbol(out: &mut Vec<Symbol>, symbol: Symbol) {
    if let Some(existente) = out.iter_mut().find(|s| s.range == symbol.range) {
        if existente.kind == SymbolKind::Function && symbol.kind == SymbolKind::Method {
            existente.kind = SymbolKind::Method;
        }
        return;
    }
    out.push(symbol);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extrai_simbolos_de_um_arquivo_rust() {
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
        let simbolos = extract_symbols(source, Language::Rust).expect("deve parsear");

        let por_nome: std::collections::HashMap<_, _> =
            simbolos.iter().map(|s| (s.name.as_str(), s)).collect();

        let ponto = por_nome.get("Ponto").expect("Ponto deve ser extraído");
        assert_eq!(ponto.kind, SymbolKind::Class);
        assert!(source[ponto.range.clone()].starts_with("struct Ponto"));

        let distancia = por_nome
            .get("distancia")
            .expect("distancia deve ser extraído");
        assert_eq!(
            distancia.kind,
            SymbolKind::Method,
            "fn dentro de impl deve ser Method"
        );
        assert!(source[distancia.range.clone()].starts_with("fn distancia"));

        let soma = por_nome.get("soma").expect("soma deve ser extraído");
        assert_eq!(
            soma.kind,
            SymbolKind::Function,
            "fn fora de impl deve ser Function"
        );
        assert!(source[soma.range.clone()].starts_with("fn soma"));

        assert_eq!(simbolos.len(), 3, "não deveria haver símbolos extras");
    }

    #[test]
    fn extrai_simbolos_de_um_arquivo_python() {
        let source = "\
class Calculadora:
    def somar(self, a, b):
        return a + b


def multiplicar(a, b):
    return a * b
";
        let simbolos = extract_symbols(source, Language::Python).expect("deve parsear");

        let por_nome: std::collections::HashMap<_, _> =
            simbolos.iter().map(|s| (s.name.as_str(), s)).collect();

        let calculadora = por_nome
            .get("Calculadora")
            .expect("Calculadora deve ser extraído");
        assert_eq!(calculadora.kind, SymbolKind::Class);
        assert!(source[calculadora.range.clone()].starts_with("class Calculadora"));

        let somar = por_nome.get("somar").expect("somar deve ser extraído");
        assert_eq!(
            somar.kind,
            SymbolKind::Function,
            "a tags query do Python não distingue método de função solta \
             (ver nota de módulo) — 'somar' vem como Function mesmo dentro da classe"
        );
        assert!(source[somar.range.clone()].starts_with("def somar"));

        let multiplicar = por_nome
            .get("multiplicar")
            .expect("multiplicar deve ser extraído");
        assert_eq!(multiplicar.kind, SymbolKind::Function);
        assert!(source[multiplicar.range.clone()].starts_with("def multiplicar"));

        assert_eq!(simbolos.len(), 3, "não deveria haver símbolos extras");
    }

    #[test]
    fn fonte_vazia_nao_produz_simbolos() {
        assert_eq!(
            extract_symbols("", Language::Rust).expect("deve parsear"),
            Vec::new()
        );
        assert_eq!(
            extract_symbols("", Language::Python).expect("deve parsear"),
            Vec::new()
        );
    }

    #[test]
    fn fonte_sintaticamente_invalida_ainda_extrai_o_que_der() {
        // `soma` é um item de nível superior completo e válido *antes* do
        // trecho malformado (struct nunca fechada, ao final do arquivo) —
        // tree-sitter faz recuperação de erro e não deveria precisar
        // invalidar o que já tinha parseado com sucesso antes do erro.
        let source = "fn soma(a: i32, b: i32) -> i32 { a + b }\n\nstruct Incompleta {\n";
        let simbolos = extract_symbols(source, Language::Rust).expect("deve parsear");

        assert!(
            simbolos.iter().any(|s| s.name == "soma"),
            "soma deveria ser extraído mesmo com erro de sintaxe em outro trecho"
        );
    }
}
