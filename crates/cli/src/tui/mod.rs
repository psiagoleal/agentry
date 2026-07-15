// Caminho relativo: crates/cli/src/tui/mod.rs
//! Modo TUI opt-in (`--tui`, ADR-0027) — laço de eventos: entra na tela
//! alternativa, desenha um histórico de mensagens rolável e sai limpo em
//! `q`/`Ctrl+C` (MT-70). Navegação (`↑`/`k`/`↓`/`j`, MT-71) já funciona
//! sobre a tabela de *keybindings* de [`keybind`], mas o histórico ainda é
//! **mock/estático** — prova a navegação antes de acoplar o *streaming*
//! real com `Session`/`Router` (MT-72).
//!
//! Usa `ratatui::try_init`/`ratatui::restore` (em vez de montar o backend
//! `crossterm` na mão) — já instalam o *hook* de panic que restaura o
//! terminal antes de propagar, exatamente o padrão recomendado pela própria
//! documentação do `ratatui` para não deixar o terminal do usuário quebrado.

mod keybind;

use std::io;

use ratatui::crossterm::event::{self, Event};
use ratatui::layout::Alignment;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::{DefaultTerminal, Frame};

use keybind::Action;

/// Histórico de mensagens **mock** (MT-71) — só para provar a navegação
/// funcionando; substituído pelo histórico real da `Session` no MT-72.
const MENSAGENS_MOCK: &[&str] = &[
    "usuário: oi, tudo bem?",
    "agente: tudo certo! Como posso ajudar?",
    "usuário: o que é o modo TUI?",
    "agente: um modo interativo opcional (--tui) que roda sobre a mesma \
     Session/Router do REPL de texto — nenhuma lógica de domínio duplicada.",
    "usuário: e como eu saio dele?",
    "agente: pressione 'q' ou Ctrl+C a qualquer momento.",
];

/// Estado de navegação do laço de eventos — offset de rolagem sobre
/// [`MENSAGENS_MOCK`]. Separado do laço de E/S para ser testável sem
/// terminal real (critério de aceite do MT-70/71).
struct Estado {
    scroll: usize,
}

impl Estado {
    fn new() -> Self {
        Self { scroll: 0 }
    }

    /// Aplica uma [`Action`] de navegação ao estado — função pura.
    /// `ScrollUp`/`ScrollDown` saturam nos limites do histórico (nunca rola
    /// para um índice negativo nem além da última mensagem); `Quit` não
    /// altera o estado (tratada no laço de eventos, que encerra antes de
    /// desenhar de novo).
    fn aplicar(&mut self, action: Action) {
        match action {
            Action::ScrollUp => self.scroll = self.scroll.saturating_sub(1),
            Action::ScrollDown => {
                let maximo = MENSAGENS_MOCK.len().saturating_sub(1);
                self.scroll = (self.scroll + 1).min(maximo);
            }
            Action::Quit => {}
        }
    }
}

/// Tela do *scaffold*: histórico de mensagens (rolável) num bloco com
/// título e instrução de saída/navegação no rodapé.
fn draw(frame: &mut Frame<'_>, estado: &Estado) {
    let linhas: Vec<Line> = MENSAGENS_MOCK.iter().map(|m| Line::from(*m)).collect();
    let rodape = format!(" {} ", keybind::legenda());
    let paragrafo = Paragraph::new(linhas)
        .block(
            Block::bordered()
                .title(" agentry ")
                .title_bottom(Line::from(rodape).alignment(Alignment::Center)),
        )
        .scroll((estado.scroll as u16, 0));
    frame.render_widget(paragrafo, frame.area());
}

/// Ponto de entrada do modo TUI (`--tui`) — laço de eventos: desenha o
/// histórico e processa teclado via [`keybind::resolve`] (nunca inspeciona
/// `KeyCode` diretamente aqui). O terminal é restaurado em qualquer caminho
/// de saída (normal ou erro) — nunca só no caminho feliz.
///
/// # Errors
///
/// Devolve o `io::Error` de inicializar, desenhar ou ler eventos do
/// terminal.
pub fn run() -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let resultado = loop_eventos(&mut terminal);
    ratatui::restore();
    resultado
}

fn loop_eventos(terminal: &mut DefaultTerminal) -> io::Result<()> {
    let mut estado = Estado::new();
    loop {
        terminal.draw(|frame| draw(frame, &estado))?;
        if let Event::Key(key) = event::read()? {
            match keybind::resolve(key) {
                Some(Action::Quit) => return Ok(()),
                Some(acao) => estado.aplicar(acao),
                None => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_up_no_topo_permanece_em_zero() {
        let mut estado = Estado::new();
        estado.aplicar(Action::ScrollUp);
        assert_eq!(estado.scroll, 0);
    }

    #[test]
    fn scroll_down_avanca_um_por_vez() {
        let mut estado = Estado::new();
        estado.aplicar(Action::ScrollDown);
        assert_eq!(estado.scroll, 1);
    }

    #[test]
    fn scroll_down_satura_no_final_do_historico() {
        let mut estado = Estado::new();
        for _ in 0..(MENSAGENS_MOCK.len() + 5) {
            estado.aplicar(Action::ScrollDown);
        }
        assert_eq!(estado.scroll, MENSAGENS_MOCK.len() - 1);
    }

    #[test]
    fn scroll_down_depois_up_volta_um_passo() {
        let mut estado = Estado::new();
        estado.aplicar(Action::ScrollDown);
        estado.aplicar(Action::ScrollDown);
        estado.aplicar(Action::ScrollUp);
        assert_eq!(estado.scroll, 1);
    }

    #[test]
    fn quit_nao_altera_o_estado_de_navegacao() {
        let mut estado = Estado::new();
        estado.aplicar(Action::ScrollDown);
        let scroll_antes = estado.scroll;
        estado.aplicar(Action::Quit);
        assert_eq!(estado.scroll, scroll_antes);
    }
}
