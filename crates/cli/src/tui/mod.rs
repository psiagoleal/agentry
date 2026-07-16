// Caminho relativo: crates/cli/src/tui/mod.rs
//! Modo TUI opt-in (`--tui`, ADR-0027) — laço de eventos: histórico de chat
//! rolável (MT-70/71) ligado à `Session`/`Router` reais (MT-72), com
//! seletor de modelo/*provider* por busca difusa (MT-73) e widgets de
//! confirmação de tool/pergunta ao usuário (MT-74).
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
//! `TuiConfirmer`/`TuiPrompter` (`crates/cli/src/tool_executor.rs`,
//! `crates/cli/src/tui/ask_user.rs`, MT-74) rodam dentro da *task* de
//! streaming (não no laço de eventos, que possui o terminal) — pedidos de
//! confirmação/pergunta chegam aqui por
//! [`crate::tool_executor::PedidoHumano`], mesma disciplina de canal +
//! `oneshot` do restante do módulo.

mod ask_user;
mod chat;
mod keybind;
mod model_picker;

pub use ask_user::TuiPrompter;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use tokio::sync::mpsc;

use agentry_core::model::{StreamEvent, ToolCall};
use agentry_core::router::{RouteTarget, Router, RuntimeOverride};
use agentry_core::session::{Session, SessionError, SessionOutcome};

use crate::tool_executor::PedidoHumano;
use chat::{Autor, ChatState};
use keybind::Action;
use model_picker::CandidatoExibicao;

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

/// Pedido de interação humana em aberto (MT-74) — só existe enquanto o
/// laço de eventos espera uma resposta do usuário para repassar pelo
/// `oneshot` do [`PedidoHumano`] original; tem prioridade sobre o seletor
/// de modelo e o chat normal (só uma dessas três coisas fica em primeiro
/// plano por vez).
enum SolicitacaoAtiva {
    /// Confirmação de uma tool-call pendente sob `ask` (`TuiConfirmer`).
    Confirmacao {
        call: ToolCall,
        responder: tokio::sync::oneshot::Sender<bool>,
    },
    /// Pergunta de texto livre da tool `ask_user` (`TuiPrompter`,
    /// ADR-0024) — `entrada` é a resposta sendo digitada.
    Pergunta {
        question: String,
        options: Vec<String>,
        entrada: String,
        responder: tokio::sync::oneshot::Sender<String>,
    },
}

/// Estado do seletor de modelo/*provider* (MT-73) — só existe enquanto o
/// seletor está aberto (`Estado::seletor: Option<Self>`); fechar o seletor é
/// simplesmente voltar esse campo a `None`.
struct SeletorDeModeloEstado {
    /// Todos os candidatos declarados na `task-class` ativa, sem filtrar —
    /// [`Self::candidatos_filtrados`] aplica a busca a cada consulta.
    candidatos: Vec<CandidatoExibicao>,
    consulta: String,
    selecionado: usize,
    /// Mensagem do último `Router::resolve_with_override` que falhou (ex.:
    /// candidato exige mais classe de egresso do que a sessão ativa
    /// permite, ADR-0002) — `None` enquanto nada foi tentado ainda ou a
    /// última tentativa funcionou.
    erro: Option<String>,
}

impl SeletorDeModeloEstado {
    fn novo(candidatos: Vec<CandidatoExibicao>) -> Self {
        Self {
            candidatos,
            consulta: String::new(),
            selecionado: 0,
            erro: None,
        }
    }

    fn candidatos_filtrados(&self) -> Vec<CandidatoExibicao> {
        model_picker::buscar(&self.candidatos, &self.consulta)
    }

    /// Move a seleção dentro da lista filtrada atual, saturando nos
    /// limites (nunca um índice fora da lista).
    fn mover_selecao(&mut self, delta: isize) {
        let total = self.candidatos_filtrados().len();
        if total == 0 {
            self.selecionado = 0;
            return;
        }
        let atual = self.selecionado.min(total - 1) as isize;
        self.selecionado = (atual + delta).clamp(0, total as isize - 1) as usize;
    }

    /// O candidato atualmente selecionado na lista filtrada — `None` só
    /// quando a busca não casa com nenhum candidato declarado.
    fn escolhido(&self) -> Option<RouteTarget> {
        let filtrados = self.candidatos_filtrados();
        let indice = self.selecionado.min(filtrados.len().checked_sub(1)?);
        filtrados.get(indice).map(|c| c.alvo.clone())
    }

    /// Qualquer edição da consulta invalida a seleção/erro anteriores.
    fn editar_consulta(&mut self, f: impl FnOnce(&mut String)) {
        f(&mut self.consulta);
        self.selecionado = 0;
        self.erro = None;
    }
}

/// Estado do laço de eventos: histórico de chat, caixa de entrada, posição
/// de rolagem e o seletor de modelo (quando aberto). Separado do laço de
/// E/S para ser testável sem terminal real.
struct Estado {
    chat: ChatState,
    entrada: String,
    scroll: u16,
    /// `true` enquanto um turno está em voo (a `Session` foi movida para a
    /// *task* de streaming) — bloqueia um novo envio até a resposta atual
    /// terminar.
    enviando: bool,
    /// `Some` só enquanto o seletor de modelo/*provider* está aberto
    /// (MT-73) — controla tanto o estado quanto qual modo o laço de
    /// eventos está em (nenhum campo `Modo` redundante).
    seletor: Option<SeletorDeModeloEstado>,
    /// Override de `provider`/`model`/parâmetros ativo — herda o que veio
    /// das flags de invocação (`--model`, `--temperature`, ...) e é
    /// atualizado quando o seletor confirma uma escolha; mesmo campo que
    /// `session_override` no REPL de texto (MT-14/MT-33).
    overrides: RuntimeOverride,
    /// `Some` só enquanto há um pedido de confirmação/pergunta pendente do
    /// `TuiConfirmer`/`TuiPrompter` (MT-74) — mesmo padrão de `seletor`,
    /// tem prioridade sobre ele e sobre o chat normal.
    solicitacao: Option<SolicitacaoAtiva>,
    /// *Toggle* `auto`/`normal` de confirmação de tool sob `ask` (MT-74) —
    /// `Arc` compartilhado com o `TuiConfirmer` injetado na `Session`
    /// (construído em `main()`); alternado por [`Action::ToggleAuto`].
    /// **Nunca** afeta uma tool sob `deny` — ver a doc de `TuiConfirmer`.
    auto: Arc<AtomicBool>,
}

impl Estado {
    fn new(overrides: RuntimeOverride, auto: Arc<AtomicBool>) -> Self {
        Self {
            chat: ChatState::new(),
            entrada: String::new(),
            scroll: 0,
            enviando: false,
            seletor: None,
            overrides,
            solicitacao: None,
            auto,
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

/// Aplica a escolha do seletor à sessão: monta o mesmo
/// `RuntimeOverride`/`Router::resolve_with_override` já usados pelos
/// comandos `/model`/`/provider` de texto do REPL (`crates/cli/src/repl.rs`,
/// reaproveitado, não duplicado) e chama `Session::apply_route` com o
/// resultado.
///
/// # Errors
///
/// Devolve o erro (formatado) de `Router::resolve_with_override` —
/// tipicamente quando o candidato escolhido exige mais classe de egresso do
/// que a sessão ativa permite (ADR-0002 *fail-closed*: o seletor nunca
/// contorna essa checagem, só chama a mesma função que o REPL já usa).
fn aplicar_selecao(
    alvo: &RouteTarget,
    task_class: &str,
    router: &Router,
    overrides: &mut RuntimeOverride,
    sessao: &mut Session,
) -> Result<(), String> {
    overrides.provider = Some(alvo.provider.clone());
    overrides.model = Some(alvo.model.clone());
    let rota = router
        .resolve_with_override(task_class, overrides)
        .map_err(|erro| erro.to_string())?;
    sessao.apply_route(rota);
    Ok(())
}

/// Tela: histórico de chat (área rolável) em cima, caixa de entrada fixa
/// embaixo — rodapé da caixa de entrada mostra a legenda de *keybindings*
/// (lida direto de [`keybind::legenda`], nunca um texto solto). Com o
/// seletor de modelo aberto, um modal centralizado é desenhado por cima.
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

    let modo_auto = if estado.auto.load(Ordering::Relaxed) {
        " [auto]"
    } else {
        ""
    };
    let titulo_entrada = if estado.enviando {
        format!(" aguardando resposta...{modo_auto} ")
    } else {
        format!(" mensagem{modo_auto} ")
    };
    let rodape = format!(" {} ", keybind::legenda());
    let caixa_de_entrada = Paragraph::new(estado.entrada.as_str()).block(
        Block::bordered()
            .title(titulo_entrada)
            .title_bottom(Line::from(rodape).alignment(Alignment::Center)),
    );
    frame.render_widget(caixa_de_entrada, areas[1]);

    if let Some(seletor) = &estado.seletor {
        draw_seletor(frame, seletor);
    }
    if let Some(solicitacao) = &estado.solicitacao {
        draw_solicitacao(frame, solicitacao);
    }
}

/// Modal de confirmação de tool (`TuiConfirmer`) ou pergunta de texto livre
/// (`TuiPrompter`, ADR-0024) — desenhado por cima de tudo (mesmo do
/// seletor de modelo, embora os dois não coexistam na prática: um pedido
/// de confirmação só existe com um turno em voo, quando o seletor já está
/// bloqueado por falta de `Session` disponível).
fn draw_solicitacao(frame: &mut Frame<'_>, solicitacao: &SolicitacaoAtiva) {
    match solicitacao {
        SolicitacaoAtiva::Confirmacao { call, .. } => {
            let area = area_centralizada(60, 30, frame.area());
            frame.render_widget(Clear, area);
            let texto = vec![
                Line::from(format!("tool: {}", call.name)),
                Line::from(format!("argumentos: {}", call.arguments)),
                Line::from(""),
                Line::from("Enter aprova · Esc recusa"),
            ];
            let paragrafo = Paragraph::new(texto)
                .block(Block::bordered().title(" confirmar execução de tool "));
            frame.render_widget(paragrafo, area);
        }
        SolicitacaoAtiva::Pergunta {
            question,
            options,
            entrada,
            ..
        } => {
            let area = area_centralizada(60, 40, frame.area());
            frame.render_widget(Clear, area);
            let layout = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Min(2), Constraint::Length(3)])
                .split(area);

            let mut linhas = vec![Line::from(question.as_str())];
            for (indice, opcao) in options.iter().enumerate() {
                linhas.push(Line::from(format!("  {}. {opcao}", indice + 1)));
            }
            let pergunta =
                Paragraph::new(linhas).block(Block::bordered().title(" pergunta do agente "));
            frame.render_widget(pergunta, layout[0]);

            let resposta = Paragraph::new(entrada.as_str())
                .block(Block::bordered().title(" sua resposta (Enter envia, Esc cancela) "));
            frame.render_widget(resposta, layout[1]);
        }
    }
}

/// Área centralizada ocupando `percent_x`/`percent_y` da tela — idioma
/// padrão do `ratatui` para modais.
fn area_centralizada(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1])[1]
}

/// Modal do seletor de modelo/*provider*: caixa de busca em cima, lista de
/// candidatos filtrados embaixo (marcador `>` na seleção atual) — a última
/// mensagem de erro de `Router::resolve_with_override`, se houver, aparece
/// no título da lista em vez do rótulo genérico.
fn draw_seletor(frame: &mut Frame<'_>, seletor: &SeletorDeModeloEstado) {
    let area = area_centralizada(60, 60, frame.area());
    frame.render_widget(Clear, area);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(1)])
        .split(area);

    let busca = Paragraph::new(seletor.consulta.as_str()).block(
        Block::bordered().title(" selecionar modelo/provider (Esc cancela, Enter confirma) "),
    );
    frame.render_widget(busca, layout[0]);

    let filtrados = seletor.candidatos_filtrados();
    let linhas: Vec<Line> = if filtrados.is_empty() {
        vec![Line::from("nenhum candidato corresponde à busca")]
    } else {
        let indice_selecionado = seletor.selecionado.min(filtrados.len() - 1);
        filtrados
            .iter()
            .enumerate()
            .map(|(i, candidato)| {
                let marcador = if i == indice_selecionado { "> " } else { "  " };
                Line::from(format!("{marcador}{}", candidato.rotulo))
            })
            .collect()
    };
    let titulo_lista = seletor.erro.as_deref().map_or_else(
        || " candidatos declarados ".to_string(),
        |erro| format!(" erro: {erro} "),
    );
    let lista = Paragraph::new(linhas).block(Block::bordered().title(titulo_lista));
    frame.render_widget(lista, layout[1]);
}

/// Ponto de entrada do modo TUI (`--tui`) — recebe a mesma `Session`/
/// `Router`/`task_class`/`overrides` já montados por `main()` (reaproveitados,
/// nunca reconstruídos), mais o lado receptor de [`PedidoHumano`] e o
/// *toggle* `auto` compartilhados com o `TuiConfirmer`/`TuiPrompter`
/// injetados na `Session` (MT-74). O terminal é restaurado em qualquer
/// caminho de saída (normal ou erro).
///
/// # Errors
///
/// Devolve o `io::Error` de inicializar, desenhar ou ler eventos do
/// terminal.
pub async fn run(
    session: Session,
    router: Router,
    task_class: String,
    overrides: RuntimeOverride,
    rx_humano: mpsc::UnboundedReceiver<PedidoHumano>,
    auto: Arc<AtomicBool>,
) -> io::Result<()> {
    let mut terminal = ratatui::try_init()?;
    let resultado = loop_eventos(
        &mut terminal,
        session,
        router,
        task_class,
        overrides,
        rx_humano,
        auto,
    )
    .await;
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
    task_class: String,
    overrides: RuntimeOverride,
    mut rx_humano: mpsc::UnboundedReceiver<PedidoHumano>,
    auto: Arc<AtomicBool>,
) -> io::Result<()> {
    let router = Arc::new(router);
    let mut estado = Estado::new(overrides, auto);
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
                let acao = keybind::resolve(key);

                if let Some(solicitacao) = estado.solicitacao.take() {
                    match solicitacao {
                        SolicitacaoAtiva::Confirmacao { call, responder } => match acao {
                            Some(Action::Quit) => {
                                let _ = responder.send(false);
                                return Ok(());
                            }
                            Some(Action::Send) => {
                                let _ = responder.send(true);
                            }
                            Some(Action::Cancel) => {
                                let _ = responder.send(false);
                            }
                            _ => {
                                estado.solicitacao =
                                    Some(SolicitacaoAtiva::Confirmacao { call, responder });
                            }
                        },
                        SolicitacaoAtiva::Pergunta {
                            question,
                            options,
                            mut entrada,
                            responder,
                        } => match acao {
                            Some(Action::Quit) => {
                                let _ = responder.send(String::new());
                                return Ok(());
                            }
                            Some(Action::Send) => {
                                let _ = responder.send(entrada);
                            }
                            Some(Action::Cancel) => {
                                let _ = responder.send(String::new());
                            }
                            None => {
                                match key.code {
                                    KeyCode::Backspace => {
                                        entrada.pop();
                                    }
                                    KeyCode::Char(c) if e_apenas_digitacao(key.modifiers) => {
                                        entrada.push(c);
                                    }
                                    _ => {}
                                }
                                estado.solicitacao = Some(SolicitacaoAtiva::Pergunta {
                                    question,
                                    options,
                                    entrada,
                                    responder,
                                });
                            }
                            _ => {
                                estado.solicitacao = Some(SolicitacaoAtiva::Pergunta {
                                    question,
                                    options,
                                    entrada,
                                    responder,
                                });
                            }
                        },
                    }
                } else if estado.seletor.is_some() {
                    match acao {
                        Some(Action::Quit) => return Ok(()),
                        Some(Action::Cancel) => estado.seletor = None,
                        Some(Action::ScrollUp) => {
                            if let Some(seletor) = estado.seletor.as_mut() {
                                seletor.mover_selecao(-1);
                            }
                        }
                        Some(Action::ScrollDown) => {
                            if let Some(seletor) = estado.seletor.as_mut() {
                                seletor.mover_selecao(1);
                            }
                        }
                        Some(Action::Send) => {
                            let escolhido = estado.seletor.as_ref().and_then(SeletorDeModeloEstado::escolhido);
                            if let (Some(alvo), Some(sessao)) = (escolhido, sessao_atual.as_mut()) {
                                match aplicar_selecao(
                                    &alvo,
                                    &task_class,
                                    &router,
                                    &mut estado.overrides,
                                    sessao,
                                ) {
                                    Ok(()) => estado.seletor = None,
                                    Err(erro) => {
                                        if let Some(seletor) = estado.seletor.as_mut() {
                                            seletor.erro = Some(erro);
                                        }
                                    }
                                }
                            }
                        }
                        Some(Action::OpenModelPicker) | Some(Action::ToggleAuto) | None => {
                            if let (None, Some(seletor)) = (acao, estado.seletor.as_mut()) {
                                match key.code {
                                    KeyCode::Backspace => {
                                        seletor.editar_consulta(|c| { c.pop(); });
                                    }
                                    KeyCode::Char(c) if e_apenas_digitacao(key.modifiers) => {
                                        seletor.editar_consulta(|consulta| consulta.push(c));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                } else {
                    match acao {
                        Some(Action::Quit) => return Ok(()),
                        Some(Action::ScrollUp) => estado.rolar_para_cima(),
                        Some(Action::ScrollDown) => estado.rolar_para_baixo(),
                        Some(Action::Cancel) => {}
                        Some(Action::ToggleAuto) => {
                            estado.auto.fetch_xor(true, Ordering::Relaxed);
                        }
                        Some(Action::OpenModelPicker) => {
                            let candidatos = router
                                .route_entry(&task_class)
                                .map(|entry| model_picker::a_partir_de_candidatos(&entry.candidates))
                                .unwrap_or_default();
                            estado.seletor = Some(SeletorDeModeloEstado::novo(candidatos));
                        }
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
            Some(pedido) = rx_humano.recv() => {
                // Chega da task de streaming (`TuiConfirmer`/`TuiPrompter`,
                // rodando dentro de `Session::run_streaming`) — nunca do
                // laço de eventos, então não há conflito com um
                // `solicitacao` já em aberto (só um turno em voo por vez).
                estado.solicitacao = Some(match pedido {
                    PedidoHumano::Confirmacao { call, responder } => {
                        SolicitacaoAtiva::Confirmacao { call, responder }
                    }
                    PedidoHumano::Pergunta {
                        question,
                        options,
                        responder,
                    } => SolicitacaoAtiva::Pergunta {
                        question,
                        options,
                        entrada: String::new(),
                        responder,
                    },
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::config::privacy::EgressClass;
    use agentry_core::model::{Message, Usage};
    use agentry_core::provider::mock::MockProvider;
    use agentry_core::provider::ChatResponse;
    use agentry_core::router::{CallPreset, ResolvedRoute, RouteEntry};
    use agentry_core::session::{TokenBudget, ToolExecutor};

    fn estado_vazio() -> Estado {
        Estado::new(RuntimeOverride::default(), Arc::new(AtomicBool::new(false)))
    }

    #[test]
    fn preparar_envio_move_o_texto_para_o_historico_e_marca_enviando() {
        let mut estado = estado_vazio();
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
        let mut estado = estado_vazio();

        let enviado = estado.preparar_envio();

        assert_eq!(enviado, None);
        assert!(!estado.enviando);
        assert!(estado.chat.mensagens().is_empty());
    }

    #[test]
    fn preparar_envio_com_entrada_so_espacos_nao_envia_nada() {
        let mut estado = estado_vazio();
        estado.entrada = "   ".into();

        assert_eq!(estado.preparar_envio(), None);
    }

    #[test]
    fn rolar_para_cima_no_topo_permanece_em_zero() {
        let mut estado = estado_vazio();

        estado.rolar_para_cima();

        assert_eq!(estado.scroll, 0);
    }

    #[test]
    fn rolar_para_baixo_sem_mensagens_permanece_em_zero() {
        let mut estado = estado_vazio();

        estado.rolar_para_baixo();

        assert_eq!(estado.scroll, 0);
    }

    #[test]
    fn rolar_para_baixo_satura_no_numero_de_mensagens() {
        let mut estado = estado_vazio();
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

    fn candidato_exibicao(provider: &str, model: &str) -> CandidatoExibicao {
        CandidatoExibicao {
            rotulo: format!("{provider}/{model}"),
            alvo: RouteTarget::new(provider, model, EgressClass::LocalOnly),
        }
    }

    #[test]
    fn mover_selecao_satura_nos_limites_da_lista_filtrada() {
        let mut seletor = SeletorDeModeloEstado::novo(vec![
            candidato_exibicao("ollama", "a"),
            candidato_exibicao("ollama", "b"),
        ]);

        seletor.mover_selecao(-1);
        assert_eq!(seletor.selecionado, 0, "não desce abaixo de zero");

        seletor.mover_selecao(1);
        assert_eq!(seletor.selecionado, 1);

        seletor.mover_selecao(1);
        assert_eq!(seletor.selecionado, 1, "não passa do último candidato");
    }

    #[test]
    fn mover_selecao_sem_candidatos_permanece_em_zero() {
        let mut seletor = SeletorDeModeloEstado::novo(vec![]);

        seletor.mover_selecao(1);

        assert_eq!(seletor.selecionado, 0);
    }

    #[test]
    fn escolhido_devolve_o_alvo_do_candidato_selecionado() {
        let mut seletor = SeletorDeModeloEstado::novo(vec![
            candidato_exibicao("ollama", "a"),
            candidato_exibicao("litellm", "b"),
        ]);
        seletor.mover_selecao(1);

        let escolhido = seletor.escolhido().expect("deve haver um candidato");

        assert_eq!(escolhido.provider, "litellm");
        assert_eq!(escolhido.model, "b");
    }

    #[test]
    fn escolhido_e_none_quando_a_busca_nao_casa_com_nada() {
        let mut seletor = SeletorDeModeloEstado::novo(vec![candidato_exibicao("ollama", "a")]);
        seletor.editar_consulta(|c| c.push_str("zzz"));

        assert_eq!(seletor.escolhido(), None);
    }

    #[test]
    fn editar_consulta_reseta_selecao_e_erro() {
        let mut seletor = SeletorDeModeloEstado::novo(vec![
            candidato_exibicao("ollama", "a"),
            candidato_exibicao("litellm", "b"),
        ]);
        seletor.mover_selecao(1);
        seletor.erro = Some("erro anterior".into());

        seletor.editar_consulta(|c| c.push('x'));

        assert_eq!(seletor.selecionado, 0);
        assert_eq!(seletor.erro, None);
    }

    struct NoopExecutor;
    impl ToolExecutor for NoopExecutor {
        fn execute(
            &self,
            call: &agentry_core::model::ToolCall,
        ) -> agentry_core::provider::BoxFuture<'_, agentry_core::model::ToolResult> {
            let call_id = call.id.clone();
            Box::pin(async move {
                agentry_core::model::ToolResult {
                    call_id,
                    content: String::new(),
                    is_error: false,
                }
            })
        }
    }

    fn sessao_de_teste(mock: Arc<MockProvider>) -> Session {
        let route = ResolvedRoute::new(mock, "modelo-inicial", CallPreset::default());
        Session::new(route, Arc::new(NoopExecutor), TokenBudget::new(10_000))
    }

    #[tokio::test]
    async fn aplicar_selecao_reaproveita_resolve_with_override_e_muda_a_rota_da_sessao() {
        let mut router = Router::new(EgressClass::LocalOnly);
        let candidato_ollama = Arc::new(MockProvider::new("ollama"));
        let candidato_litellm = Arc::new(MockProvider::new("litellm"));
        router.register_provider(candidato_ollama.clone());
        router.register_provider(candidato_litellm.clone());
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![
                    RouteTarget::new("ollama", "modelo-inicial", EgressClass::LocalOnly),
                    RouteTarget::new("litellm", "modelo-nuvem", EgressClass::LocalOnly),
                ],
                preset: CallPreset::default(),
            },
        );
        let mut sessao = sessao_de_teste(candidato_ollama.clone());
        let mut overrides = RuntimeOverride::default();
        let alvo = RouteTarget::new("litellm", "modelo-nuvem", EgressClass::LocalOnly);

        aplicar_selecao(&alvo, "chat", &router, &mut overrides, &mut sessao)
            .expect("candidato declarado deve resolver");

        assert_eq!(overrides.provider.as_deref(), Some("litellm"));
        assert_eq!(overrides.model.as_deref(), Some("modelo-nuvem"));

        // Prova que a rota da sessão realmente mudou: o próximo turno bate
        // no provider recém-selecionado, nunca mais no inicial.
        candidato_litellm.enqueue_chat(Ok(ChatResponse {
            message: Message::assistant("ok"),
            usage: Usage::default(),
        }));
        sessao.push_user_message("oi");
        sessao.run(&router).await.expect("deve completar");

        assert_eq!(candidato_litellm.chat_requests().len(), 1);
        assert_eq!(candidato_ollama.chat_requests().len(), 0);
    }

    #[test]
    fn aplicar_selecao_com_egresso_insuficiente_devolve_erro_sem_mudar_a_sessao() {
        // A sessão está em LocalOnly; o único candidato declarado exige
        // CloudOk — a seleção deve falhar (fail-closed, ADR-0002), nunca
        // contornar a checagem de egresso do Router.
        let mut router = Router::new(EgressClass::LocalOnly);
        let provider = Arc::new(MockProvider::new("litellm"));
        router.register_provider(provider.clone());
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "litellm",
                    "modelo-nuvem",
                    EgressClass::CloudOk,
                )],
                preset: CallPreset::default(),
            },
        );
        let mut sessao = sessao_de_teste(provider);
        let mut overrides = RuntimeOverride::default();
        let alvo = RouteTarget::new("litellm", "modelo-nuvem", EgressClass::CloudOk);

        let resultado = aplicar_selecao(&alvo, "chat", &router, &mut overrides, &mut sessao);

        assert!(resultado.is_err());
    }
}
