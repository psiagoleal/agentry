// Caminho relativo: crates/cli/src/tui/mod.rs
//! Modo TUI opt-in (`--tui`, ADR-0027) — laço de eventos: histórico de chat
//! rolável (MT-70/71) agora ligado à `Session`/`Router` reais (MT-72).
//!
//! `Session::run_streaming` roda numa *task* separada (`tokio::spawn`); o
//! *callback* (já genérico desde o MT-10) envia cada [`StreamEvent`] (já
//! `Clone`) por um canal (`tokio::sync::mpsc`) de volta ao laço de eventos
//! principal, que faz `tokio::select!` entre eventos de terminal (lidos numa
//! *thread* dedicada, já que `crossterm::event::read` é bloqueante) e
//! eventos de *stream* do canal — **nenhuma mudança em `crates/core`**, a
//! API de *callback* já era genérica o suficiente (ADR-0027).
//!
//! Usa `ratatui::try_init`/`ratatui::restore` (em vez de montar o backend
//! `crossterm` na mão) — já instalam o *hook* de panic que restaura o
//! terminal antes de propagar, exatamente o padrão recomendado pela própria
//! documentação do `ratatui` para não deixar o terminal do usuário quebrado.
//!
//! Fora de escopo desta ticket: confirmação de tool via widget (MT-74 — sob
//! `ask`, a `Session` ainda usa o `Confirmer`/`Prompter` de texto simples já
//! injetados por `main()`, o que pode brigar com o modo bruto do terminal;
//! aceito por ora, só para não travar) e seletor de modelo (MT-73).

mod chat;
mod keybind;

use std::io;
use std::sync::Arc;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout};
use ratatui::text::Line;
use ratatui::widgets::{Block, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use tokio::sync::mpsc;

use agentry_core::model::StreamEvent;
use agentry_core::router::Router;
use agentry_core::session::{Session, SessionError, SessionOutcome};

use chat::{Autor, ChatState};
use keybind::Action;

/// Evento recebido pelo laço principal a partir da *task* de streaming.
enum EventoAgente {
    /// Fragmento de resposta do modelo — repassado a
    /// [`ChatState::aplicar_evento`].
    Stream(StreamEvent),
    /// O turno terminou (com sucesso ou erro) — devolve a posse da
    /// [`Session`] para que o próximo envio possa reaproveitá-la.
    Concluido(Box<TurnoConcluido>),
}

struct TurnoConcluido {
    sessao: Session,
    resultado: Result<SessionOutcome, SessionError>,
}

/// Estado do laço de eventos: histórico de chat, caixa de entrada e posição
/// de rolagem. Separado do laço de E/S para ser testável sem terminal real.
struct Estado {
    chat: ChatState,
    entrada: String,
    scroll: u16,
    /// `true` enquanto um turno está em voo (a `Session` foi movida para a
    /// *task* de streaming) — bloqueia um novo envio até a resposta atual
    /// terminar.
    enviando: bool,
}

impl Estado {
    fn new() -> Self {
        Self {
            chat: ChatState::new(),
            entrada: String::new(),
            scroll: 0,
            enviando: false,
        }
    }

    fn rolar_para_cima(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    fn rolar_para_baixo(&mut self) {
        let maximo = self.chat.mensagens().len().saturating_sub(1) as u16;
        self.scroll = (self.scroll + 1).min(maximo);
    }

    /// Move o texto da caixa de entrada para o histórico como mensagem do
    /// usuário e abre o turno do agente — função pura, testável sem
    /// terminal/`Session` reais. Entrada vazia (ou só espaços) não envia
    /// nada, devolve `None`.
    fn preparar_envio(&mut self) -> Option<String> {
        if self.entrada.trim().is_empty() {
            return None;
        }
        let texto = std::mem::take(&mut self.entrada);
        self.chat.registrar_mensagem_usuario(texto.clone());
        self.enviando = true;
        Some(texto)
    }
}

/// Tela: histórico de chat (área rolável) em cima, caixa de entrada fixa
/// embaixo — rodapé da caixa de entrada mostra a legenda de *keybindings*
/// (lida direto de [`keybind::legenda`], nunca um texto solto).
fn draw(frame: &mut Frame<'_>, estado: &Estado) {
    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(3)])
        .split(frame.area());

    let linhas: Vec<Line> = estado
        .chat
        .mensagens()
        .iter()
        .map(|mensagem| {
            let prefixo = match mensagem.autor {
                Autor::Usuario => "usuário: ",
                Autor::Agente => "agente: ",
            };
            Line::from(format!("{prefixo}{}", mensagem.texto))
        })
        .collect();
    let historico = Paragraph::new(linhas)
        .block(Block::bordered().title(" agentry "))
        .scroll((estado.scroll, 0));
    frame.render_widget(historico, areas[0]);

    let titulo_entrada = if estado.enviando {
        " aguardando resposta... "
    } else {
        " mensagem "
    };
    let rodape = format!(" {} ", keybind::legenda());
    let caixa_de_entrada = Paragraph::new(estado.entrada.as_str()).block(
        Block::bordered()
            .title(titulo_entrada)
            .title_bottom(Line::from(rodape).alignment(Alignment::Center)),
    );
    frame.render_widget(caixa_de_entrada, areas[1]);
}

/// Ponto de entrada do modo TUI (`--tui`) — recebe a mesma `Session`/`Router`
/// já montados por `main()` (reaproveitados, nunca reconstruídos). O
/// terminal é restaurado em qualquer caminho de saída (normal ou erro).
///
/// # Errors
///
/// Devolve o `io::Error` de inicializar, desenhar ou ler eventos do
/// terminal.
pub async fn run(session: Session, router: Router) -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let resultado = loop_eventos(&mut terminal, session, router).await;
    ratatui::restore();
    resultado
}

/// Lê eventos de terminal numa *thread* dedicada (`crossterm::event::read`
/// é bloqueante) e os repassa por canal — permite ao laço principal
/// combiná-los com os eventos de *stream* assíncronos via `tokio::select!`.
fn iniciar_leitor_de_terminal() -> mpsc::UnboundedReceiver<io::Result<Event>> {
    let (tx, rx) = mpsc::unbounded_channel();
    std::thread::spawn(move || loop {
        let lido = event::read();
        let deve_parar = lido.is_err();
        if tx.send(lido).is_err() || deve_parar {
            break;
        }
    });
    rx
}

/// Move `sessao` para uma *task* separada e roda `run_streaming` nela —
/// cada [`StreamEvent`] chega ao laço principal por `tx`; ao final (sucesso
/// ou erro), a posse da `Session` volta por `tx` também, nunca perdida.
fn disparar_turno(
    mut sessao: Session,
    texto: String,
    router: Arc<Router>,
    tx: mpsc::UnboundedSender<EventoAgente>,
) {
    sessao.push_user_message(texto);
    tokio::spawn(async move {
        let resultado = sessao
            .run_streaming(
                |evento| {
                    let _ = tx.send(EventoAgente::Stream(evento.clone()));
                },
                router.as_ref(),
            )
            .await;
        let _ = tx.send(EventoAgente::Concluido(Box::new(TurnoConcluido {
            sessao,
            resultado,
        })));
    });
}

/// Só caracteres digitados sem modificador (ou só `Shift`, que já vem
/// refletido no próprio `char` maiúsculo em terminais comuns) viram texto na
/// caixa de entrada — qualquer outro modificador (`Ctrl`/`Alt`/`Super`) não
/// é uma tecla de digitação normal.
fn e_apenas_digitacao(modifiers: KeyModifiers) -> bool {
    modifiers.difference(KeyModifiers::SHIFT).is_empty()
}

async fn loop_eventos(
    terminal: &mut DefaultTerminal,
    sessao_inicial: Session,
    router: Router,
) -> io::Result<()> {
    let router = Arc::new(router);
    let mut estado = Estado::new();
    let mut sessao_atual = Some(sessao_inicial);
    let mut rx_terminal = iniciar_leitor_de_terminal();
    let (tx_agente, mut rx_agente) = mpsc::unbounded_channel::<EventoAgente>();

    loop {
        terminal.draw(|frame| draw(frame, &estado))?;

        tokio::select! {
            evento_terminal = rx_terminal.recv() => {
                let Some(lido) = evento_terminal else {
                    return Ok(());
                };
                let evento = lido?;
                let Event::Key(key) = evento else {
                    continue;
                };
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match keybind::resolve(key) {
                    Some(Action::Quit) => return Ok(()),
                    Some(Action::ScrollUp) => estado.rolar_para_cima(),
                    Some(Action::ScrollDown) => estado.rolar_para_baixo(),
                    Some(Action::Send) => {
                        if let Some(sessao) = sessao_atual.take() {
                            match estado.preparar_envio() {
                                Some(texto) => disparar_turno(
                                    sessao,
                                    texto,
                                    Arc::clone(&router),
                                    tx_agente.clone(),
                                ),
                                None => sessao_atual = Some(sessao),
                            }
                        }
                    }
                    None => match key.code {
                        KeyCode::Backspace => {
                            estado.entrada.pop();
                        }
                        KeyCode::Char(c) if e_apenas_digitacao(key.modifiers) => {
                            estado.entrada.push(c);
                        }
                        _ => {}
                    },
                }
            }
            Some(evento_agente) = rx_agente.recv() => {
                match evento_agente {
                    EventoAgente::Stream(stream_evt) => estado.chat.aplicar_evento(&stream_evt),
                    EventoAgente::Concluido(concluido) => {
                        if let Err(erro) = &concluido.resultado {
                            estado.chat.marcar_erro(&erro.to_string());
                        }
                        sessao_atual = Some(concluido.sessao);
                        estado.enviando = false;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preparar_envio_move_o_texto_para_o_historico_e_marca_enviando() {
        let mut estado = Estado::new();
        estado.entrada = "oi".into();

        let enviado = estado.preparar_envio();

        assert_eq!(enviado, Some("oi".to_string()));
        assert_eq!(estado.entrada, "");
        assert!(estado.enviando);
        assert_eq!(estado.chat.mensagens().len(), 2);
        assert_eq!(estado.chat.mensagens()[0].texto, "oi");
    }

    #[test]
    fn preparar_envio_com_entrada_vazia_nao_envia_nada() {
        let mut estado = Estado::new();

        let enviado = estado.preparar_envio();

        assert_eq!(enviado, None);
        assert!(!estado.enviando);
        assert!(estado.chat.mensagens().is_empty());
    }

    #[test]
    fn preparar_envio_com_entrada_so_espacos_nao_envia_nada() {
        let mut estado = Estado::new();
        estado.entrada = "   ".into();

        assert_eq!(estado.preparar_envio(), None);
    }

    #[test]
    fn rolar_para_cima_no_topo_permanece_em_zero() {
        let mut estado = Estado::new();

        estado.rolar_para_cima();

        assert_eq!(estado.scroll, 0);
    }

    #[test]
    fn rolar_para_baixo_sem_mensagens_permanece_em_zero() {
        let mut estado = Estado::new();

        estado.rolar_para_baixo();

        assert_eq!(estado.scroll, 0);
    }

    #[test]
    fn rolar_para_baixo_satura_no_numero_de_mensagens() {
        let mut estado = Estado::new();
        estado.entrada = "oi".into();
        estado.preparar_envio(); // 2 mensagens (usuário + turno do agente)

        for _ in 0..10 {
            estado.rolar_para_baixo();
        }

        assert_eq!(estado.scroll, 1);
    }

    #[test]
    fn e_apenas_digitacao_aceita_nenhum_modificador_ou_so_shift() {
        assert!(e_apenas_digitacao(KeyModifiers::NONE));
        assert!(e_apenas_digitacao(KeyModifiers::SHIFT));
        assert!(!e_apenas_digitacao(KeyModifiers::CONTROL));
        assert!(!e_apenas_digitacao(KeyModifiers::ALT));
    }
}
