// Caminho relativo: crates/core/src/context/repo_map/graph.rs
//! Grafo de referências entre arquivos (MT-19, ADR-0010).
//!
//! Para cada arquivo de entrada, extrai (via a mesma *tags query* do
//! `tree-sitter` usada pelo MT-18, `crate::context::ast`) dois conjuntos:
//! **definições** (qualquer `definition.*` da query — função, método,
//! classe/struct/enum, macro, módulo etc.; deliberadamente **sem** o filtro
//! função/classe/método do MT-18, já que uma referência pode legitimamente
//! apontar para uma constante, trait/macro definida em outro arquivo) e
//! **referências** (qualquer `reference.*` — chamada de função/método,
//! implementação de trait). Uma aresta dirigida `A -> B` é criada quando
//! `A` referencia um nome definido em `B`, com peso igual à contagem de
//! referências — **sem auto-referência** (arestas de um arquivo para ele
//! mesmo são descartadas: não ajudam a decidir relevância *entre* arquivos,
//! que é o propósito do grafo — o MT-20 rankeia sobre ele).
//!
//! Este módulo tem seu próprio parse+query (não reaproveita
//! `ast::extract_symbols`, que filtra só função/classe/método e descarta
//! `reference.*`) — escopo deste ticket é só `repo_map/graph.rs`.

use std::collections::HashMap;

use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor};

use crate::context::ast::Language;

/// Um arquivo-fonte de entrada para a construção do grafo.
#[derive(Debug, Clone, Copy)]
pub struct SourceFile<'a> {
    /// Caminho do arquivo — identifica o nó no grafo (não precisa ser um
    /// caminho real de filesystem; só uma chave estável por arquivo).
    pub path: &'a str,
    /// Conteúdo do arquivo.
    pub source: &'a str,
    /// Linguagem do arquivo.
    pub language: Language,
}

/// Grafo dirigido de referências entre arquivos: uma aresta `A -> B`
/// significa que `A` referencia pelo menos um símbolo definido em `B`, com
/// peso igual à contagem de referências observadas.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ReferenceGraph {
    edges: HashMap<(String, String), u32>,
}

impl ReferenceGraph {
    /// Peso da aresta `from -> to` (`0` se não houver aresta).
    #[must_use]
    pub fn weight(&self, from: &str, to: &str) -> u32 {
        self.edges
            .get(&(from.to_string(), to.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Número de arestas (pares únicos `from -> to` com peso > 0).
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Itera as arestas como `(origem, destino, peso)`.
    pub fn edges(&self) -> impl Iterator<Item = (&str, &str, u32)> {
        self.edges
            .iter()
            .map(|((from, to), peso)| (from.as_str(), to.as_str(), *peso))
    }
}

/// Constrói o grafo de referências sobre `files`.
///
/// # Panics
///
/// Não deveria entrar em pânico em uso normal — os `expect`s internos só
/// disparariam por incompatibilidade de versão entre `tree-sitter` e a
/// gramática (mesmo invariante do MT-18).
#[must_use]
pub fn build_reference_graph(files: &[SourceFile<'_>]) -> ReferenceGraph {
    // Passo 1: por arquivo, extrai (nomes definidos, nomes referenciados).
    let extraidos: Vec<(&str, Vec<String>, Vec<String>)> = files
        .iter()
        .map(|file| {
            let (defs, refs) = parse_defs_and_refs(file.source, file.language);
            (file.path, defs, refs)
        })
        .collect();

    // Passo 2: nome -> arquivos que o definem (pode ser mais de um).
    let mut definidores: HashMap<&str, Vec<&str>> = HashMap::new();
    for (path, defs, _) in &extraidos {
        for nome in defs {
            definidores.entry(nome.as_str()).or_default().push(path);
        }
    }

    // Passo 3: cada referência gera (ou incrementa) uma aresta para cada
    // arquivo que define aquele nome, exceto o próprio arquivo de origem.
    let mut edges: HashMap<(String, String), u32> = HashMap::new();
    for (path, _, refs) in &extraidos {
        for nome in refs {
            let Some(arquivos_definidores) = definidores.get(nome.as_str()) else {
                continue;
            };
            for &definidor in arquivos_definidores {
                if definidor == *path {
                    continue; // sem auto-referência
                }
                *edges
                    .entry((path.to_string(), definidor.to_string()))
                    .or_insert(0) += 1;
            }
        }
    }

    ReferenceGraph { edges }
}

fn ts_language(language: Language) -> tree_sitter::Language {
    match language {
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
    }
}

fn tags_query(language: Language) -> &'static str {
    match language {
        Language::Rust => tree_sitter_rust::TAGS_QUERY,
        Language::Python => tree_sitter_python::TAGS_QUERY,
    }
}

/// Roda a *tags query* da linguagem sobre `source` e devolve os nomes que
/// aparecem em captures `definition.*` e `reference.*`, respectivamente —
/// sem filtrar por tipo de definição (ver nota de módulo).
fn parse_defs_and_refs(source: &str, language: Language) -> (Vec<String>, Vec<String>) {
    let ts_language = ts_language(language);

    let mut parser = Parser::new();
    parser
        .set_language(&ts_language)
        .expect("gramática deve ser aceita pelo parser (mesmo invariante do MT-18)");

    let tree = parser
        .parse(source, None)
        .expect("parse não deveria retornar None sem timeout/cancelamento configurado");

    let query = Query::new(&ts_language, tags_query(language))
        .expect("tags query da gramática deve compilar (mesmo invariante do MT-18)");

    let mut defs = Vec::new();
    let mut refs = Vec::new();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
    while let Some(m) = matches.next() {
        let mut nome: Option<&str> = None;
        let mut e_definicao = false;
        let mut e_referencia = false;

        for capture in m.captures {
            let nome_capture = query.capture_names()[capture.index as usize];
            if nome_capture == "name" {
                nome = capture.node.utf8_text(source.as_bytes()).ok();
            } else if nome_capture.starts_with("definition.") {
                e_definicao = true;
            } else if nome_capture.starts_with("reference.") {
                e_referencia = true;
            }
        }

        if let Some(nome) = nome {
            if e_definicao {
                defs.push(nome.to_string());
            }
            if e_referencia {
                refs.push(nome.to_string());
            }
        }
    }

    (defs, refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grafo_reflete_referencia_entre_dois_arquivos() {
        let a = "fn ajudante() -> i32 {\n    42\n}\n";
        let b = "\
fn principal() {
    let valor = ajudante();
    let outro = ajudante();
    println!(\"{}\", valor + outro);
}
";
        let files = vec![
            SourceFile {
                path: "a.rs",
                source: a,
                language: Language::Rust,
            },
            SourceFile {
                path: "b.rs",
                source: b,
                language: Language::Rust,
            },
        ];

        let grafo = build_reference_graph(&files);

        assert_eq!(
            grafo.weight("b.rs", "a.rs"),
            2,
            "b.rs chama ajudante() duas vezes; ajudante é definida em a.rs"
        );
        assert_eq!(
            grafo.weight("a.rs", "b.rs"),
            0,
            "a.rs não referencia nada de b.rs"
        );
        assert_eq!(
            grafo.edge_count(),
            1,
            "println! não é definido em nenhum arquivo conhecido — não gera aresta"
        );
    }

    #[test]
    fn sem_auto_referencia() {
        let a = "\
fn ajudante() -> i32 {
    42
}

fn principal() -> i32 {
    ajudante()
}
";
        let files = vec![SourceFile {
            path: "a.rs",
            source: a,
            language: Language::Rust,
        }];

        let grafo = build_reference_graph(&files);

        assert_eq!(
            grafo.edge_count(),
            0,
            "referência a um símbolo definido no próprio arquivo não deve gerar aresta"
        );
    }

    #[test]
    fn referencia_a_nome_desconhecido_nao_gera_aresta() {
        let a = "fn principal() {\n    algo_que_nao_existe();\n}\n";
        let files = vec![SourceFile {
            path: "a.rs",
            source: a,
            language: Language::Rust,
        }];

        let grafo = build_reference_graph(&files);

        assert_eq!(grafo.edge_count(), 0);
    }

    #[test]
    fn grafo_funciona_para_python() {
        let a = "def ajudante():\n    return 42\n";
        let b = "\
def principal():
    valor = ajudante()
    return valor
";
        let files = vec![
            SourceFile {
                path: "a.py",
                source: a,
                language: Language::Python,
            },
            SourceFile {
                path: "b.py",
                source: b,
                language: Language::Python,
            },
        ];

        let grafo = build_reference_graph(&files);

        assert_eq!(grafo.weight("b.py", "a.py"), 1);
        assert_eq!(grafo.edge_count(), 1);
    }

    #[test]
    fn grafo_vazio_para_arquivos_sem_definicoes_ou_referencias_cruzadas() {
        let files = vec![
            SourceFile {
                path: "a.rs",
                source: "fn a() {}\n",
                language: Language::Rust,
            },
            SourceFile {
                path: "b.rs",
                source: "fn b() {}\n",
                language: Language::Rust,
            },
        ];

        let grafo = build_reference_graph(&files);

        assert_eq!(grafo.edge_count(), 0);
    }
}
