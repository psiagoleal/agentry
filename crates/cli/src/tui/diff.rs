// Caminho relativo: crates/cli/src/tui/diff.rs
//! Geração de diff linha a linha (MT-75/ADR-0027) — para o modal de
//! confirmação de `fs_write`/`fs_edit` sob `ask` (`TuiConfirmer`, MT-74)
//! mostrar o que realmente muda, em vez dos argumentos brutos da
//! tool-call. Diff clássico por subsequência comum máxima (LCS,
//! implementação própria — o mesmo princípio do `diff` do Unix, sem
//! dependência nova para um problema estreito e bem definido, mesma
//! disciplina de MT-06/ADR-0007/MT-60/MT-73). Visualização linear
//! unificada; diff lado a lado/navegação por *hunk* ficam fora de escopo
//! desta ticket.

/// Uma linha do diff — do conteúdo antigo, do novo, ou presente nos dois.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinhaDiff {
    Removida(String),
    Adicionada(String),
    Inalterada(String),
}

/// Gera o diff linha a linha entre `antigo` e `novo` via subsequência
/// comum máxima — `antigo` vazio (arquivo novo) marca todas as linhas de
/// `novo` como [`LinhaDiff::Adicionada`]; `novo` vazio marcaria todas as
/// de `antigo` como [`LinhaDiff::Removida`] (caso sem uso real hoje —
/// `fs_write`/`fs_edit` nunca esvaziam um arquivo por completo através
/// desta função, mas o algoritmo já cobre o caso corretamente).
#[must_use]
pub fn diff_linhas(antigo: &str, novo: &str) -> Vec<LinhaDiff> {
    let linhas_antigas: Vec<&str> = antigo.lines().collect();
    let linhas_novas: Vec<&str> = novo.lines().collect();
    let tabela = tabela_lcs(&linhas_antigas, &linhas_novas);
    reconstroi_diff(&linhas_antigas, &linhas_novas, &tabela)
}

/// Tabela de programação dinâmica do comprimento da LCS — `tabela[i][j]`
/// é o comprimento da LCS entre `a[..i]` e `b[..j]`.
fn tabela_lcs(a: &[&str], b: &[&str]) -> Vec<Vec<usize>> {
    let (m, n) = (a.len(), b.len());
    let mut tabela = vec![vec![0usize; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            tabela[i][j] = if a[i - 1] == b[j - 1] {
                tabela[i - 1][j - 1] + 1
            } else {
                tabela[i - 1][j].max(tabela[i][j - 1])
            };
        }
    }
    tabela
}

/// Reconstrói o diff a partir da tabela de LCS, andando de trás para
/// frente (do fim de `a`/`b` até o início) e revertendo ao final.
fn reconstroi_diff(a: &[&str], b: &[&str], tabela: &[Vec<usize>]) -> Vec<LinhaDiff> {
    let mut resultado = Vec::new();
    let (mut i, mut j) = (a.len(), b.len());
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
            resultado.push(LinhaDiff::Inalterada(a[i - 1].to_string()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || tabela[i][j - 1] >= tabela[i - 1][j]) {
            resultado.push(LinhaDiff::Adicionada(b[j - 1].to_string()));
            j -= 1;
        } else {
            resultado.push(LinhaDiff::Removida(a[i - 1].to_string()));
            i -= 1;
        }
    }
    resultado.reverse();
    resultado
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arquivo_novo_sem_conteudo_antigo_marca_tudo_como_adicionado() {
        let diff = diff_linhas("", "linha 1\nlinha 2");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Adicionada("linha 1".to_string()),
                LinhaDiff::Adicionada("linha 2".to_string()),
            ]
        );
    }

    #[test]
    fn conteudo_identico_marca_tudo_como_inalterado() {
        let diff = diff_linhas("a\nb\nc", "a\nb\nc");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Inalterada("a".to_string()),
                LinhaDiff::Inalterada("b".to_string()),
                LinhaDiff::Inalterada("c".to_string()),
            ]
        );
    }

    #[test]
    fn linha_adicionada_no_meio_e_marcada_sozinha() {
        let diff = diff_linhas("a\nc", "a\nb\nc");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Inalterada("a".to_string()),
                LinhaDiff::Adicionada("b".to_string()),
                LinhaDiff::Inalterada("c".to_string()),
            ]
        );
    }

    #[test]
    fn linha_removida_do_meio_e_marcada_sozinha() {
        let diff = diff_linhas("a\nb\nc", "a\nc");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Inalterada("a".to_string()),
                LinhaDiff::Removida("b".to_string()),
                LinhaDiff::Inalterada("c".to_string()),
            ]
        );
    }

    #[test]
    fn substituicao_de_uma_linha_e_remocao_mais_adicao() {
        let diff = diff_linhas("a\nb\nc", "a\nx\nc");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Inalterada("a".to_string()),
                LinhaDiff::Removida("b".to_string()),
                LinhaDiff::Adicionada("x".to_string()),
                LinhaDiff::Inalterada("c".to_string()),
            ]
        );
    }

    #[test]
    fn tudo_removido_quando_o_novo_conteudo_e_vazio() {
        let diff = diff_linhas("a\nb", "");

        assert_eq!(
            diff,
            vec![
                LinhaDiff::Removida("a".to_string()),
                LinhaDiff::Removida("b".to_string()),
            ]
        );
    }

    #[test]
    fn dois_conteudos_vazios_nao_produzem_nenhuma_linha() {
        assert_eq!(diff_linhas("", ""), Vec::new());
    }
}
