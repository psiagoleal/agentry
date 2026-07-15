// Caminho relativo: crates/cli/src/tui/mod.rs
//! Modo TUI opt-in (`--tui`, MT-70/ADR-0027) — *scaffold* mínimo: entra na
//! tela alternativa, desenha um conteúdo estático e sai limpo em `q`/`Ctrl+C`.
//!
//! Usa `ratatui::try_init`/`ratatui::restore` (em vez de montar o backend
//! `crossterm` na mão) — já instalam o *hook* de panic que restaura o
//! terminal antes de propagar, exatamente o padrão recomendado pela própria
//! documentação do `ratatui` para não deixar o terminal do usuário quebrado.
//!
//! Integração com `Session`/`Router` fica para o MT-72; *keybindings*
//! configuráveis (além do `q`/`Ctrl+C` fixos aqui) ficam para o MT-71.

use std::io;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::Alignment;
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::{DefaultTerminal, Frame};

/// Ação pura resolvida a partir de uma tecla — extraída do laço de eventos
/// para ser testável sem terminal real (critério de aceite do MT-70; o laço
/// de eventos em si, que depende de E/S de terminal real, não é coberto por
/// teste automatizado nesta ticket).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    /// Sai do modo TUI (`q` ou `Ctrl+C`).
    Quit,
    /// Tecla sem ação mapeada nesta ticket — ignorada, mesmo padrão de
    /// "comando desconhecido não derruba o REPL" já usado no REPL de texto
    /// (MT-14).
    Unknown,
}

/// Resolve uma tecla pressionada para a [`Action`] correspondente. Só
/// considera eventos de **pressionar** (`KeyEventKind::Press`) — em
/// terminais que emitem o protocolo estendido do `crossterm` (ex.: Windows),
/// uma tecla também gera evento de *release*, que dobraria a ação caso não
/// filtrado aqui.
fn action_for_key(key: KeyEvent) -> Action {
    if key.kind != KeyEventKind::Press {
        return Action::Unknown;
    }
    match key.code {
        KeyCode::Char('q') => Action::Quit,
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => Action::Quit,
        _ => Action::Unknown,
    }
}

/// Tela estática do *scaffold* — título + instrução de saída (ADR-0027: view
/// de chat real com histórico rolante é o MT-71/72, fora de escopo aqui).
fn draw(frame: &mut Frame<'_>) {
    let conteudo = vec![
        Line::from("agentry — modo TUI".bold()),
        Line::from(""),
        Line::from("pressione 'q' para sair"),
    ];
    let paragrafo = Paragraph::new(conteudo)
        .block(Block::bordered().title(" agentry "))
        .alignment(Alignment::Center);
    frame.render_widget(paragrafo, frame.area());
}

/// Ponto de entrada do modo TUI (`--tui`) — laço de eventos mínimo: desenha
/// a tela estática e processa teclado só o suficiente para sair
/// (`q`/`Ctrl+C`). O terminal é restaurado em qualquer caminho de saída
/// (normal ou erro) — nunca só no caminho feliz.
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
    loop {
        terminal.draw(draw)?;
        if let Event::Key(key) = event::read()? {
            if action_for_key(key) == Action::Quit {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tecla(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn q_minusculo_sai() {
        assert_eq!(
            action_for_key(tecla(KeyCode::Char('q'), KeyModifiers::NONE)),
            Action::Quit
        );
    }

    #[test]
    fn ctrl_c_sai() {
        assert_eq!(
            action_for_key(tecla(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Action::Quit
        );
    }

    #[test]
    fn c_sem_ctrl_nao_sai() {
        assert_eq!(
            action_for_key(tecla(KeyCode::Char('c'), KeyModifiers::NONE)),
            Action::Unknown
        );
    }

    #[test]
    fn tecla_sem_acao_mapeada_e_ignorada() {
        assert_eq!(
            action_for_key(tecla(KeyCode::Char('x'), KeyModifiers::NONE)),
            Action::Unknown
        );
    }

    #[test]
    fn evento_de_release_e_ignorado_mesmo_para_q() {
        let mut key = tecla(KeyCode::Char('q'), KeyModifiers::NONE);
        key.kind = KeyEventKind::Release;
        assert_eq!(action_for_key(key), Action::Unknown);
    }
}
