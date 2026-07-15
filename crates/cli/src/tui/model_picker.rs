// Caminho relativo: crates/cli/src/tui/model_picker.rs
//! Busca difusa sobre os candidatos já declarados na `task-class` ativa
//! (`RouteEntry.candidates`, MT-73/ADR-0027) — evolução dos comandos
//! `/model <nome>`/`/provider <nome>` de texto exato do REPL (MT-14/50).
//!
//! **Nunca** introduz um candidato novo — a lista vem inteira de
//! [`agentry_core::router::Router::route_entry`], já a única fonte de
//! verdade (mesma disciplina vetada pelo ADR-0014 para `--model`/
//! `--provider`). "Busca difusa" aqui é um casamento de subsequência
//! simples, não uma dependência de *fuzzy-matching* — a lista de
//! candidatos de uma `task-class` é sempre pequena (poucos candidatos
//! declarados), um algoritmo mínimo já é suficiente e correto, e uma
//! dependência nova para isso exigiria ADR-0004 sem necessidade real (mesmo
//! espírito do parser de frontmatter do MT-60 e do casamento de guardrail
//! por substring do ADR-0007).

use agentry_core::router::RouteTarget;

/// Um candidato exibível no seletor: rótulo de busca/exibição
/// (`"<provider>/<modelo>"`) mais o [`RouteTarget`] original, aplicado via
/// `RuntimeOverride` quando escolhido (nunca reconstruído a partir do
/// rótulo).
#[derive(Debug, Clone, PartialEq)]
pub struct CandidatoExibicao {
    pub rotulo: String,
    pub alvo: RouteTarget,
}

/// Converte os candidatos brutos de uma [`agentry_core::router::RouteEntry`]
/// para a forma exibível do seletor, preservando a ordem de preferência
/// declarada.
pub fn a_partir_de_candidatos(candidatos: &[RouteTarget]) -> Vec<CandidatoExibicao> {
    candidatos
        .iter()
        .map(|alvo| CandidatoExibicao {
            rotulo: format!("{}/{}", alvo.provider, alvo.model),
            alvo: alvo.clone(),
        })
        .collect()
}

/// Filtra e ordena `candidatos` por correspondência aproximada
/// (subsequência de caracteres, sem diferenciar maiúsculas/minúsculas)
/// contra `consulta` — candidatos cujo rótulo não contém todos os
/// caracteres de `consulta`, na mesma ordem, são descartados; os demais são
/// ordenados pelo trecho de casamento mais compacto primeiro (ordem
/// original preservada em caso de empate — [`Vec::sort_by_key`] é
/// estável). `consulta` vazia devolve todos os candidatos, na ordem
/// original, sem filtrar.
#[must_use]
pub fn buscar(candidatos: &[CandidatoExibicao], consulta: &str) -> Vec<CandidatoExibicao> {
    if consulta.trim().is_empty() {
        return candidatos.to_vec();
    }
    let consulta = consulta.to_lowercase();
    let mut pontuados: Vec<(usize, &CandidatoExibicao)> = candidatos
        .iter()
        .filter_map(|c| {
            pontuar_subsequencia(&c.rotulo.to_lowercase(), &consulta)
                .map(|pontuacao| (pontuacao, c))
        })
        .collect();
    pontuados.sort_by_key(|(pontuacao, _)| *pontuacao);
    pontuados.into_iter().map(|(_, c)| c.clone()).collect()
}

/// Devolve o tamanho do trecho de `texto` (varredura da esquerda para a
/// direita, gulosa) que contém todos os caracteres de `consulta`, na mesma
/// ordem, não necessariamente contíguos — `None` se `consulta` não é uma
/// subsequência de `texto`. Não é garantidamente o **menor** trecho
/// possível (isso exigiria uma segunda varredura para encolher a janela),
/// só uma aproximação boa o suficiente para ordenar poucos candidatos.
fn pontuar_subsequencia(texto: &str, consulta: &str) -> Option<usize> {
    let mut chars_consulta = consulta.chars();
    let mut alvo = chars_consulta.next()?;
    let mut inicio = None;
    for (i, c) in texto.char_indices() {
        if c == alvo {
            if inicio.is_none() {
                inicio = Some(i);
            }
            match chars_consulta.next() {
                Some(proximo) => alvo = proximo,
                None => return Some(i - inicio.unwrap() + 1),
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::config::privacy::EgressClass;

    fn candidato(provider: &str, model: &str) -> CandidatoExibicao {
        CandidatoExibicao {
            rotulo: format!("{provider}/{model}"),
            alvo: RouteTarget::new(provider, model, EgressClass::LocalOnly),
        }
    }

    #[test]
    fn a_partir_de_candidatos_preserva_ordem_e_monta_rotulo_provider_modelo() {
        let brutos = vec![
            RouteTarget::new("ollama", "llama3.1:8b", EgressClass::LocalOnly),
            RouteTarget::new("litellm", "gpt-x", EgressClass::CloudOk),
        ];

        let exibiveis = a_partir_de_candidatos(&brutos);

        assert_eq!(exibiveis.len(), 2);
        assert_eq!(exibiveis[0].rotulo, "ollama/llama3.1:8b");
        assert_eq!(exibiveis[1].rotulo, "litellm/gpt-x");
    }

    #[test]
    fn consulta_vazia_devolve_todos_os_candidatos_na_ordem_original() {
        let candidatos = vec![
            candidato("ollama", "llama3.1:8b"),
            candidato("litellm", "gpt-x"),
        ];

        let resultado = buscar(&candidatos, "");

        assert_eq!(resultado, candidatos);
    }

    #[test]
    fn consulta_filtra_candidatos_sem_a_subsequencia() {
        let candidatos = vec![
            candidato("ollama", "llama3.1:8b"),
            candidato("litellm", "gpt-x"),
        ];

        let resultado = buscar(&candidatos, "gpt");

        assert_eq!(resultado.len(), 1);
        assert_eq!(resultado[0].rotulo, "litellm/gpt-x");
    }

    #[test]
    fn consulta_como_subsequencia_nao_contigua_ainda_casa() {
        let candidatos = vec![candidato("ollama", "llama3.1:8b")];

        // "l38" é subsequência de "ollama/llama3.1:8b" (l...3...8), mesmo
        // sem ser um trecho contíguo.
        let resultado = buscar(&candidatos, "l38");

        assert_eq!(resultado.len(), 1);
    }

    #[test]
    fn consulta_sem_nenhum_candidato_correspondente_devolve_lista_vazia() {
        let candidatos = vec![candidato("ollama", "llama3.1:8b")];

        let resultado = buscar(&candidatos, "zzz");

        assert!(resultado.is_empty());
    }

    #[test]
    fn candidato_com_trecho_mais_compacto_vem_primeiro() {
        let candidatos = vec![
            candidato("ollama", "aXbXcXbXaXcX"), // "abc" espalhado, trecho longo
            candidato("litellm", "abc-modelo"),  // "abc" contíguo, trecho curto
        ];

        let resultado = buscar(&candidatos, "abc");

        assert_eq!(resultado[0].rotulo, "litellm/abc-modelo");
    }

    #[test]
    fn busca_nao_diferencia_maiusculas_de_minusculas() {
        let candidatos = vec![candidato("Ollama", "Llama3.1:8b")];

        let resultado = buscar(&candidatos, "OLLAMA");

        assert_eq!(resultado.len(), 1);
    }
}
