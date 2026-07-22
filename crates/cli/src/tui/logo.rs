// Caminho relativo: crates/cli/src/tui/logo.rs
//! Logo de abertura da TUI (MT-111) — ícone colorido em halfblock/truecolor,
//! pré-processado *offline* (`assets/logo/gerar-logo-icone.py`, não roda como
//! parte do `cargo build`) a partir do logo oficial do projeto e embutido
//! como asset binário via [`include_bytes!`] — **nenhuma dependência nova**:
//! não decodificamos imagem em runtime, só lemos bytes RGB já prontos.
//!
//! Técnica (mesma usada por ferramentas como `chafa`/`viu`): cada linha de
//! terminal representa duas linhas de pixel da imagem, desenhada como `▀`
//! com cor de primeiro plano = pixel de cima e cor de fundo = pixel de baixo
//! — dá pra mostrar uma imagem real com boa fidelidade em qualquer terminal
//! com suporte a 24 bits de cor, sem precisar de protocolo de imagem
//! (Sixel/Kitty) nem de decodificador de PNG embutido no binário.
//!
//! Terminais sem suporte a 24 bits de cor (ou com `NO_COLOR` setado) caem no
//! robô em ASCII simples ([`ROBO_ASCII`]) — sem cor, mas sempre legível.

use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};

/// Dimensões do asset gerado por `assets/logo/gerar-logo-icone.py` — mudam
/// junto se o asset for regerado numa resolução diferente.
const LARGURA: usize = 44;
const ALTURA_PX: usize = 30;

static PIXELS: &[u8] = include_bytes!("../../assets/logo-icone.rgb");

const ROBO_ASCII: &[&str] = &[
    "        ●",
    "        │",
    "    ┌───┴───┐",
    "    │ ◉   ◉ │",
    "    │       │",
    "    │ ╰───╯ │",
    "    └──┬─┬──┘",
    "       █ █",
];

/// Monta as linhas completas da tela de abertura: ícone (colorido ou
/// ASCII, conforme [`suporta_truecolor`]) seguido do nome do projeto e da
/// instrução inicial — mesmo conteúdo textual de sempre, só o ícone muda.
pub(super) fn linhas() -> Vec<Line<'static>> {
    let mut linhas = if suporta_truecolor() {
        icone_colorido()
    } else {
        icone_ascii()
    };
    linhas.push(Line::from(""));
    linhas.push(Line::from("a g e n t r y"));
    linhas.push(Line::from(""));
    linhas.push(Line::from("digite uma mensagem e Enter para começar"));
    linhas
}

/// Heurística padrão de ecossistema de terminal: `NO_COLOR` (qualquer valor)
/// desliga cor de propósito; `COLORTERM=truecolor`/`24bit` é o sinal mais
/// comum de suporte a 24 bits de cor (usado por `chafa`, `bat`, `delta`,
/// entre outros). Ausência de sinal claro cai no lado seguro (ASCII sem
/// cor) — mostrar uma imagem corrompida por falta de suporte real seria
/// pior do que o robô simples.
fn suporta_truecolor() -> bool {
    decide_truecolor(
        std::env::var_os("NO_COLOR").is_some(),
        std::env::var("COLORTERM").ok().as_deref(),
    )
}

/// Núcleo puro de [`suporta_truecolor`], sem tocar `std::env` — testável
/// sem mutar variáveis de ambiente do processo (que exigiriam `unsafe` a
/// partir do Rust 1.82 por mexerem em estado global compartilhado entre
/// threads de teste).
fn decide_truecolor(no_color_setado: bool, colorterm: Option<&str>) -> bool {
    if no_color_setado {
        return false;
    }
    matches!(colorterm, Some("truecolor") | Some("24bit"))
}

fn icone_ascii() -> Vec<Line<'static>> {
    ROBO_ASCII.iter().map(|linha| Line::from(*linha)).collect()
}

/// Constrói o ícone a partir de [`PIXELS`] — uma linha de terminal por par
/// de linhas de pixel. Sequências horizontais consecutivas com o mesmo par
/// de cores (cima/baixo) são unidas num único [`Span`] em vez de um por
/// pixel — a imagem tem bastante fundo preto contíguo, então isso evita
/// dezenas de `Span`s idênticos por linha.
fn icone_colorido() -> Vec<Line<'static>> {
    let linhas_terminal = ALTURA_PX / 2;
    let mut linhas = Vec::with_capacity(linhas_terminal);
    for linha in 0..linhas_terminal {
        let mut spans: Vec<Span<'static>> = Vec::new();
        let mut estilo_atual: Option<Style> = None;
        let mut repeticoes = 0usize;
        for coluna in 0..LARGURA {
            let estilo = Style::new()
                .fg(pixel(coluna, linha * 2))
                .bg(pixel(coluna, linha * 2 + 1));
            match estilo_atual {
                Some(atual) if atual == estilo => repeticoes += 1,
                _ => {
                    if let Some(atual) = estilo_atual {
                        spans.push(Span::styled("▀".repeat(repeticoes), atual));
                    }
                    estilo_atual = Some(estilo);
                    repeticoes = 1;
                }
            }
        }
        if let Some(atual) = estilo_atual {
            spans.push(Span::styled("▀".repeat(repeticoes), atual));
        }
        linhas.push(Line::from(spans));
    }
    linhas
}

fn pixel(coluna: usize, linha: usize) -> Color {
    let indice = (linha * LARGURA + coluna) * 3;
    Color::Rgb(PIXELS[indice], PIXELS[indice + 1], PIXELS[indice + 2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_tem_o_tamanho_esperado() {
        assert_eq!(PIXELS.len(), LARGURA * ALTURA_PX * 3);
    }

    #[test]
    fn icone_colorido_tem_uma_linha_de_terminal_por_par_de_linhas_de_pixel() {
        assert_eq!(icone_colorido().len(), ALTURA_PX / 2);
    }

    #[test]
    fn icone_ascii_nao_fica_vazio() {
        assert!(!icone_ascii().is_empty());
        assert!(icone_ascii().iter().all(|l| !l.spans.is_empty()));
    }

    #[test]
    fn decide_truecolor_desliga_com_no_color_mesmo_com_colorterm_truecolor() {
        assert!(!decide_truecolor(true, Some("truecolor")));
    }

    #[test]
    fn decide_truecolor_liga_so_com_colorterm_truecolor_ou_24bit() {
        assert!(decide_truecolor(false, Some("truecolor")));
        assert!(decide_truecolor(false, Some("24bit")));
        assert!(!decide_truecolor(false, Some("outracoisa")));
        assert!(
            !decide_truecolor(false, None),
            "sem nenhum sinal, cai no lado seguro (ASCII)"
        );
    }
}
