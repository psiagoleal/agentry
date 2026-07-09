// Caminho relativo: crates/core/src/context/repo_map/rank.rs
//! Ranking de relevância estilo PageRank sobre o grafo de referências
//! (MT-20, ADR-0010).
//!
//! PageRank **personalizado** (mesma técnica usada pelo repo-map do Aider):
//! em vez de distribuir a massa de teleporte uniformemente entre todos os
//! nós (PageRank clássico), ela é concentrada nos arquivos "semente" —
//! fazendo a relevância propagar pelas arestas do grafo (MT-19) a partir de
//! onde a tarefa atual já está ancorada (arquivos mencionados/abertos na
//! sessão), em vez de medir importância global do repositório.

use std::collections::{HashMap, HashSet};

use super::graph::ReferenceGraph;

/// Fator de amortecimento padrão do PageRank (convenção usual da literatura).
const DEFAULT_DAMPING: f64 = 0.85;
/// Iterações da iteração de potência — suficiente para convergir em grafos
/// do tamanho de um repositório de código (não escala de web-scale).
const DEFAULT_ITERATIONS: usize = 20;

/// Rankeia `nodes` por relevância a partir de `seeds`, usando `graph`.
///
/// Devolve `(nó, pontuação)` em ordem decrescente de pontuação — em empate,
/// por ordem alfabética do nó, para determinismo — **excluindo** os
/// próprios nós de `seeds` (o objetivo é rankear "os demais", já que os
/// nós semente já são conhecidos como relevantes por definição). `seeds`
/// vazio cai no PageRank clássico (teleporte uniforme entre todos os
/// `nodes`). Nomes em `seeds` que não estejam em `nodes` são ignorados.
#[must_use]
pub fn rank(graph: &ReferenceGraph, nodes: &[&str], seeds: &[&str]) -> Vec<(String, f64)> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let total_nos = nodes.len() as f64;
    let seed_set: HashSet<&str> = seeds
        .iter()
        .copied()
        .filter(|no| nodes.contains(no))
        .collect();

    let personalizacao: HashMap<&str, f64> = if seed_set.is_empty() {
        nodes.iter().map(|&no| (no, 1.0 / total_nos)).collect()
    } else {
        let peso = 1.0 / seed_set.len() as f64;
        nodes
            .iter()
            .map(|&no| (no, if seed_set.contains(no) { peso } else { 0.0 }))
            .collect()
    };

    let arestas: Vec<(&str, &str, u32)> = graph.edges().collect();

    let mut peso_saida: HashMap<&str, f64> = nodes.iter().map(|&no| (no, 0.0)).collect();
    for &(de, _, peso) in &arestas {
        if let Some(total) = peso_saida.get_mut(de) {
            *total += f64::from(peso);
        }
    }

    let mut scores: HashMap<&str, f64> = nodes.iter().map(|&no| (no, 1.0 / total_nos)).collect();

    for _ in 0..DEFAULT_ITERATIONS {
        let mut novo: HashMap<&str, f64> = nodes
            .iter()
            .map(|&no| (no, (1.0 - DEFAULT_DAMPING) * personalizacao[no]))
            .collect();

        // Massa presa em nós sem aresta de saída — redistribuída conforme a
        // personalização, mesma convenção do PageRank personalizado (sem
        // isso, a massa desses nós desapareceria a cada iteração).
        let massa_presa: f64 = nodes
            .iter()
            .filter(|&&no| peso_saida[no] == 0.0)
            .map(|&no| scores[no])
            .sum();

        for &(de, para, peso) in &arestas {
            // `de`/`para` podem referenciar arquivos fora de `nodes` (o
            // grafo pode cobrir mais arquivos do que os que esta chamada
            // quer rankear) — ambos ignorados em silêncio quando não
            // fazem parte do universo desta chamada.
            let Some(&total_saida) = peso_saida.get(de) else {
                continue;
            };
            if total_saida == 0.0 {
                continue;
            }
            let contribuicao = DEFAULT_DAMPING * scores[de] * f64::from(peso) / total_saida;
            if let Some(valor) = novo.get_mut(para) {
                *valor += contribuicao;
            }
        }

        for &no in nodes {
            *novo.get_mut(no).expect("nó sempre presente no mapa") +=
                DEFAULT_DAMPING * massa_presa * personalizacao[no];
        }

        scores = novo;
    }

    let mut ranking: Vec<(String, f64)> = nodes
        .iter()
        .filter(|&&no| !seed_set.contains(no))
        .map(|&no| (no.to_string(), scores[no]))
        .collect();

    ranking.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    ranking
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::ast::Language;
    use crate::context::repo_map::graph::{build_reference_graph, SourceFile};

    /// Grafo de exemplo conhecido: `seed.rs` referencia `popular.rs` (que
    /// também referenciada por `outro.rs`) e `obscuro.rs` (referenciada uma
    /// só vez); `isolado.rs` não tem nenhuma ligação com nada.
    fn grafo_de_exemplo() -> ReferenceGraph {
        let arquivos = vec![
            SourceFile {
                path: "seed.rs",
                source: "fn desde_seed() {\n    popular();\n    popular();\n    obscuro();\n}\n",
                language: Language::Rust,
            },
            SourceFile {
                path: "popular.rs",
                source: "fn popular() {}\n",
                language: Language::Rust,
            },
            SourceFile {
                path: "obscuro.rs",
                source: "fn obscuro() {}\n",
                language: Language::Rust,
            },
            SourceFile {
                path: "isolado.rs",
                source: "fn isolado() {}\n",
                language: Language::Rust,
            },
        ];
        build_reference_graph(&arquivos)
    }

    #[test]
    fn arquivo_mais_referenciado_a_partir_da_semente_fica_no_topo() {
        let grafo = grafo_de_exemplo();
        let nodes = ["seed.rs", "popular.rs", "obscuro.rs", "isolado.rs"];
        let seeds = ["seed.rs"];

        let ranking = rank(&grafo, &nodes, &seeds);
        let posicoes: HashMap<&str, usize> = ranking
            .iter()
            .enumerate()
            .map(|(i, (nome, _))| (nome.as_str(), i))
            .collect();

        assert!(
            !posicoes.contains_key("seed.rs"),
            "o próprio nó semente não deve aparecer no ranking dos 'demais'"
        );
        assert!(
            posicoes["popular.rs"] < posicoes["obscuro.rs"],
            "popular.rs (referenciada duas vezes pela semente) deve ficar acima de \
             obscuro.rs (referenciada uma vez); ranking: {ranking:?}"
        );
        assert!(
            posicoes["obscuro.rs"] < posicoes["isolado.rs"],
            "obscuro.rs (referenciado pela semente) deve ficar acima de isolado.rs (sem \
             nenhuma ligação); ranking: {ranking:?}"
        );
    }

    #[test]
    fn seeds_vazio_cai_no_pagerank_classico_e_rankeia_todos() {
        let grafo = grafo_de_exemplo();
        let nodes = ["seed.rs", "popular.rs", "obscuro.rs", "isolado.rs"];

        let ranking = rank(&grafo, &nodes, &[]);

        assert_eq!(
            ranking.len(),
            nodes.len(),
            "sem semente, nenhum nó é excluído do ranking"
        );
    }

    #[test]
    fn todos_os_nos_como_semente_produz_ranking_vazio() {
        let grafo = grafo_de_exemplo();
        let nodes = ["seed.rs", "popular.rs"];

        let ranking = rank(&grafo, &nodes, &["seed.rs", "popular.rs"]);

        assert!(ranking.is_empty());
    }

    #[test]
    fn semente_desconhecida_e_ignorada_sem_panico() {
        let grafo = grafo_de_exemplo();
        let nodes = ["seed.rs", "popular.rs"];

        let ranking = rank(&grafo, &nodes, &["nao-existe.rs"]);

        // Semente inexistente não casa com nada em `nodes` — cai no
        // fallback de personalização vazia (uniforme), mas nenhum nó real
        // é excluído por engano.
        assert_eq!(ranking.len(), 2);
    }
}
