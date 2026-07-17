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
//! `oneshot` do restante do módulo. Para `fs_write`/`fs_edit` sob `ask`, o
//! pedido já chega com o diff pronto ([`diff::LinhaDiff`], MT-75) — montado
//! do lado do `TuiConfirmer`, não aqui.

mod ask_user;
mod chat;
pub(crate) mod diff;
mod keybind;
mod model_picker;

pub use ask_user::TuiPrompter;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Clear, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use tokio::sync::mpsc;

use agentry_core::model::{StreamEvent, ToolCall, Usage};
use agentry_core::router::{RouteTarget, Router, RuntimeOverride};
use agentry_core::session::{Session, SessionError, SessionOutcome};

use crate::repl;
use crate::tool_executor::PedidoHumano;
use chat::{Autor, ChatState};
use diff::LinhaDiff;
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
        /// Diff pronto (MT-75) para `fs_write`/`fs_edit` — `None` para
        /// qualquer outra tool, que continua mostrando os argumentos
        /// brutos.
        diff: Option<Vec<LinhaDiff>>,
        responder: tokio::sync::oneshot::Sender<bool>,
    },
    /// Pergunta de texto livre da tool `ask_user` (`TuiPrompter`,
    /// ADR-0024) — `entrada` é a resposta sendo digitada. `selecionada`
    /// (MT-98) é o índice destacado em `options` quando não-vazio;
    /// `Up`/`Down` movem o destaque, `Enter` com `entrada` vazia envia o
    /// texto exato da opção destacada (elimina a ambiguidade de o modelo
    /// ter que "traduzir" um número como "1"/"2" de volta pra opção
    /// certa — achado na rodada 4: mesma pergunta, respostas idênticas em
    /// forma numérica, comportamento inconsistente entre rodadas). Digitar
    /// qualquer texto ignora a seleção e envia o texto livre, como sempre.
    Pergunta {
        question: String,
        options: Vec<String>,
        entrada: String,
        selecionada: usize,
        responder: tokio::sync::oneshot::Sender<String>,
    },
}

/// Enviada no lugar de uma `String` vazia quando o usuário cancela a
/// pergunta (`Esc`) — achado na rodada 4: antes, `Esc` mandava
/// `String::new()`, indistinguível de "usuário respondeu vazio e apertou
/// Enter" do ponto de vista do modelo (que via os dois casos como a
/// mesma execução de tool bem-sucedida, sem nenhum sinal de que o usuário
/// não quis responder).
const SENTINELA_CANCELAMENTO_PERGUNTA: &str = "(usuário cancelou a pergunta, sem responder)";

/// Move o índice destacado entre as opções de uma `SolicitacaoAtiva::Pergunta`,
/// saturando nos limites — mesmo espírito de
/// `SeletorDeModeloEstado::mover_selecao`, função livre porque a seleção
/// aqui não precisa de nenhum outro estado (busca/filtro) além do índice.
fn mover_selecao_pergunta(atual: usize, delta: isize, total_opcoes: usize) -> usize {
    if total_opcoes == 0 {
        return 0;
    }
    let atual = atual.min(total_opcoes - 1) as isize;
    (atual + delta).clamp(0, total_opcoes as isize - 1) as usize
}

/// Decide o texto a enviar de volta pro `ask_user` ao apertar `Enter`:
/// campo vazio + alguma opção declarada envia o texto exato da opção
/// destacada por `selecionada` (nunca um número que o modelo teria que
/// "traduzir" de volta); campo preenchido envia o texto livre, sempre —
/// a seleção nunca sobrescreve o que o usuário digitou de propósito.
/// Função pura, testável sem terminal/canais reais.
fn resposta_da_pergunta(entrada: String, options: &[String], selecionada: usize) -> String {
    if entrada.trim().is_empty() && !options.is_empty() {
        options[selecionada].clone()
    } else {
        entrada
    }
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
    /// Uso de tokens acumulado da sessão até o último turno concluído
    /// (MT-84, ADR-0029) — copiado de `Session::usage_total()` a cada
    /// `EventoAgente::Concluido` (a `Session` em si vive fora de `Estado`,
    /// movida para a *task* de streaming durante um turno em voo; ver
    /// [`disparar_turno`]). Renderizado no rodapé por [`draw`].
    usage_total: Usage,
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
            usage_total: Usage::default(),
        }
    }

    /// `scroll` conta a distância (em linhas já quebradas/wrapped) a partir
    /// do **fim** da conversa — "rolar para cima" (ver histórico antigo)
    /// aumenta essa distância. Sem teto aqui: o teto real (não passar do
    /// início do histórico) é aplicado em `draw`, que é quem sabe quantas
    /// linhas existem depois do wrap da largura atual do terminal.
    fn rolar_para_cima(&mut self) {
        self.scroll = self.scroll.saturating_add(1);
    }

    /// Inverso de [`Self::rolar_para_cima`] — "rolar para baixo" (voltar
    /// para o fim/mensagens novas) diminui a distância, saturando em zero
    /// (zero = fim da conversa, nunca fica "negativo").
    fn rolar_para_baixo(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    /// Move o texto da caixa de entrada para o histórico como mensagem do
    /// usuário e abre o turno do agente — função pura, testável sem
    /// terminal/`Session` reais. Entrada vazia (ou só espaços) não envia
    /// nada, devolve `None`. Sempre volta o scroll para o fim da conversa
    /// (mesmo comportamento de qualquer chat: enviar uma mensagem nova
    /// mostra a mensagem nova, mesmo que o usuário tivesse rolado para
    /// cima olhando histórico antigo).
    fn preparar_envio(&mut self) -> Option<String> {
        if self.entrada.trim().is_empty() {
            return None;
        }
        let texto = std::mem::take(&mut self.entrada);
        self.chat.registrar_mensagem_usuario(texto.clone());
        self.enviando = true;
        self.scroll = 0;
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

/// Texto do rodapé da caixa de entrada: legenda de *keybindings* (lida
/// direto de [`keybind::legenda`]) seguida do uso de tokens acumulado da
/// sessão (MT-84, ADR-0029; mesma formatação de [`crate::formatar_uso`]
/// usada pelo resumo *one-shot*/`/usage` do REPL) — função pura, separada de
/// [`draw`] para ser testável sem terminal real.
fn rodape_da_entrada(estado: &Estado) -> String {
    format!(
        " {} · {} ",
        keybind::legenda(),
        crate::formatar_uso(estado.usage_total)
    )
}

/// Texto da mensagem de sistema mostrada no histórico de chat após
/// `Ctrl+Z`/*undo* (MT-88, ADR-0030) — função pura, separada do laço de
/// eventos para ser testável sem terminal real; usa
/// [`crate::formatar_undo`] em sucesso (mesma formatação da flag
/// `--undo`/comando `/undo`), o `Display` de
/// [`agentry_core::checkpoint::CheckpointError`] em erro.
fn mensagem_de_undo(
    resultado: Result<
        agentry_core::checkpoint::UndoOutcome,
        agentry_core::checkpoint::CheckpointError,
    >,
) -> String {
    match resultado {
        Ok(outcome) => format!("[undo] {}", crate::formatar_undo(&outcome)),
        Err(erro) => format!("[undo] erro: {erro}"),
    }
}

/// Processa um comando de texto (`/usage`, `/undo`, `/remember`,
/// `/compact`, `/task-class`, `/provider`, `/temperature`, `/top_p`,
/// `/max_tokens`, `/system`, `/reasoning`) digitado na caixa de entrada da
/// TUI — achado num teste manual de usabilidade: antes desta função, todo
/// texto começando com `/` era enviado ao modelo como mensagem de chat
/// comum, que "inventava" uma resposta plausível em vez de rodar o comando
/// de verdade (`/usage` respondia um número que não vinha de lugar nenhum;
/// `/remember` não gravava nada). Mesma disciplina de reaproveitamento do
/// resto do projeto: chama [`repl::aplicar_comando`] e os mesmos tipos já
/// usados pelo REPL de texto (`CheckpointStore`, `MemoryStore`,
/// `Session::compact`) — nenhuma segunda implementação divergente.
///
/// `/model` é a única exceção deliberada: precisaria de `&mut Router` para
/// declarar um candidato novo ([`set_chat_route`]), mas o `Router` da TUI
/// é compartilhado (`Arc`) com a *task* de streaming em voo — mesma
/// restrição que levou o subagente (ADR-0031/MT-91) a montar sua própria
/// instância em vez de compartilhar uma só. Reaproveitar esse padrão aqui
/// (uma segunda instância "equivalente") não ajudaria: o problema não é
/// *ter* um `Router` mutável, é que a TUI só tem *um* ponto de verdade
/// para ele, também lido pela *task* de streaming — então `/model`
/// simplesmente recusa e aponta para o seletor (`Ctrl+P`), que já resolve
/// o mesmo problema sem exigir mutação (escolhe entre candidatos já
/// declarados). `/init` também fica de fora (bootstrap de configuração,
/// não faz sentido no meio de uma sessão interativa já rodando).
async fn processar_comando_de_texto(
    comando: &str,
    sessao: &mut Session,
    router: &Router,
    overrides: &mut RuntimeOverride,
    task_class: &mut String,
    checkpoint_store: &agentry_core::checkpoint::CheckpointStore,
    workspace_root: &std::path::Path,
) -> String {
    if comando == "compact" {
        return match sessao.compact(router).await {
            Ok(()) => "sessão compactada".to_string(),
            Err(erro) => format!("erro: {erro}"),
        };
    }
    if comando == "usage" {
        return format!(
            "uso desta sessão: {}",
            crate::formatar_uso(sessao.usage_total())
        );
    }
    if comando == "undo" {
        return mensagem_de_undo(checkpoint_store.undo());
    }
    if comando == "remember" || comando.starts_with("remember ") {
        let fato = comando.strip_prefix("remember").unwrap_or("").trim();
        if fato.is_empty() {
            return "uso: /remember <fato>".to_string();
        }
        let store = agentry_core::memory::MemoryStore::new(workspace_root);
        return match store.remember(fato) {
            Ok(()) => format!("lembrado: {fato}"),
            Err(erro) => format!("erro: {erro}"),
        };
    }
    if comando == "task-class" || comando.starts_with("task-class ") {
        let nome = comando.strip_prefix("task-class").unwrap_or("").trim();
        if nome.is_empty() {
            return "uso: /task-class <nome>".to_string();
        }
        return match router.resolve_with_override(nome, overrides) {
            Ok(rota) => {
                *task_class = nome.to_string();
                sessao.apply_route(rota);
                format!("task-class alterada para: {nome}")
            }
            Err(erro) => format!("erro: {erro}"),
        };
    }
    if comando == "model" || comando.starts_with("model ") {
        return "troca de modelo na TUI é pelo seletor (Ctrl+P), não pelo comando /model"
            .to_string();
    }
    if comando == "init" || comando.starts_with("init ") {
        return "/init não é suportado na TUI (bootstrap de configuração; rode fora de uma \
                sessão interativa já em andamento)"
            .to_string();
    }

    match repl::aplicar_comando(comando, overrides) {
        Ok((mensagem, _mudou_model)) => match router.resolve_with_override(task_class, overrides) {
            Ok(rota) => {
                sessao.apply_route(rota);
                mensagem
            }
            Err(erro) => format!("erro: {erro}"),
        },
        Err(erro) => erro,
    }
}

/// Logo de abertura (puramente decorativo) — mostrado centralizado no
/// histórico enquanto nenhuma mensagem foi trocada ainda; desaparece para
/// sempre assim que a primeira mensagem é enviada (mesmo padrão de tela de
/// boas-vindas de qualquer CLI interativa).
const LOGO_ABERTURA: &[&str] = &[
    "   ┌─────────────┐",
    "   │  ◉       ◉  │",
    "   │             │",
    "   │      ▽      │",
    "   └───┬─────┬───┘",
    "       █     █",
    "",
    "      a g e n t r y",
    "",
    "  digite uma mensagem e Enter para começar",
];

/// Quebra `palavra` em pedaços de no máximo `largura` caracteres — só entra
/// em jogo para uma "palavra" (sem espaço) mais larga que a coluna
/// inteira (ex.: um caminho de arquivo longo); o caso comum (palavra cabe
/// inteira) devolve um único pedaço, sem cópia extra.
fn fatiar_palavra_longa(palavra: &str, largura: usize) -> Vec<String> {
    if palavra.chars().count() <= largura {
        return vec![palavra.to_string()];
    }
    palavra
        .chars()
        .collect::<Vec<_>>()
        .chunks(largura.max(1))
        .map(|pedaco| pedaco.iter().collect())
        .collect()
}

/// Quebra `texto` em linhas que cabem em `largura` colunas (*word wrap*
/// manual, greedy) — decisão deliberada de não usar
/// `ratatui::widgets::Wrap` (achado num teste manual de usabilidade: sem
/// nenhum wrap, texto mais largo que o terminal simplesmente desaparecia
/// para fora da tela). Quebras de linha explícitas do próprio texto (`\n`,
/// ex.: um bloco de código) são preservadas como limites de linha, nunca
/// unidas numa só.
fn quebrar_em_linhas(texto: &str, largura: usize) -> Vec<String> {
    let largura = largura.max(1);
    let mut saida = Vec::new();
    for linha_original in texto.split('\n') {
        let palavras: Vec<String> = linha_original
            .split(' ')
            .flat_map(|palavra| fatiar_palavra_longa(palavra, largura))
            .collect();

        let mut atual = String::new();
        for palavra in palavras {
            if atual.is_empty() {
                atual = palavra;
            } else if atual.chars().count() + 1 + palavra.chars().count() <= largura {
                atual.push(' ');
                atual.push_str(&palavra);
            } else {
                saida.push(std::mem::take(&mut atual));
                atual = palavra;
            }
        }
        saida.push(atual);
    }
    saida
}

/// Estilo por autor da mensagem — única "formatação" aplicada por ora
/// (destacar quem falou); rendering de Markdown propriamente dito (negrito,
/// blocos de código) fica para uma extensão futura, ver conversa de
/// usabilidade. Linhas do marcador de tool call (`ChatState::aplicar_evento`,
/// prefixo `⚙`) recebem um estilo próprio, discreto, independente do autor.
fn estilo_da_mensagem(autor: Autor) -> Style {
    match autor {
        Autor::Usuario => Style::default().fg(Color::Cyan),
        Autor::Agente => Style::default().fg(Color::White),
    }
}

const ESTILO_MARCADOR_DE_TOOL: Style = Style::new()
    .fg(Color::DarkGray)
    .add_modifier(Modifier::ITALIC);

/// Monta as linhas já quebradas/estilizadas do histórico inteiro, dado
/// quantas colunas estão disponíveis — função pura (sem `Frame`), testável
/// sem terminal real. Cada mensagem ganha um prefixo (`"usuário: "`/
/// `"agente: "`) só na primeira linha; as linhas de continuação (por wrap)
/// alinham por baixo do prefixo (recuo pendurado), sem repeti-lo.
fn montar_linhas_do_historico(estado: &Estado, largura_disponivel: usize) -> Vec<Line<'static>> {
    let mut linhas = Vec::new();
    for mensagem in estado.chat.mensagens() {
        let prefixo = match mensagem.autor {
            Autor::Usuario => "usuário: ",
            Autor::Agente => "agente: ",
        };
        let recuo = " ".repeat(prefixo.chars().count());
        let largura_do_texto = largura_disponivel
            .saturating_sub(prefixo.chars().count())
            .max(1);
        let estilo_base = estilo_da_mensagem(mensagem.autor);

        for (indice, linha) in quebrar_em_linhas(&mensagem.texto, largura_do_texto)
            .iter()
            .enumerate()
        {
            let texto_da_linha = if indice == 0 {
                format!("{prefixo}{linha}")
            } else {
                format!("{recuo}{linha}")
            };
            let estilo = if linha.trim_start().starts_with('⚙') {
                ESTILO_MARCADOR_DE_TOOL
            } else {
                estilo_base
            };
            linhas.push(Line::styled(texto_da_linha, estilo));
        }
    }
    linhas
}

/// Altura total (linhas de conteúdo + 2 de borda) da caixa de entrada,
/// dado quantas linhas o texto digitado ocupa depois do *wrap* — cresce
/// junto com o texto, até `teto` (também em unidades de altura total com
/// borda; nunca menor que 3, a altura mínima de sempre). Função pura, sem
/// depender de `Frame`/terminal real — achado num teste manual de
/// usabilidade: a caixa de entrada não tinha *wrap* nem crescia,
/// diferente do histórico (corrigido na rodada 2).
fn altura_da_entrada(linhas_de_conteudo: usize, teto: u16) -> u16 {
    let altura_com_borda = (linhas_de_conteudo.max(1) as u16).saturating_add(2);
    altura_com_borda.clamp(3, teto.max(3))
}

/// Preenche `linhas` com linhas em branco no **início** até `altura_minima`,
/// só quando a conversa é mais curta que a área visível — âncora o
/// conteúdo real no fim da caixa, mesmo comportamento de qualquer chat
/// (Slack/Discord/iMessage sempre "grudam" embaixo; espaço vazio, se
/// houver, fica em cima). Achado num teste manual de usabilidade: sem
/// isto, uma conversa curta aparecia no topo da caixa com uma faixa em
/// branco embaixo — o `deslocamento_do_topo` de `draw` já ancorava
/// corretamente quando a conversa **excede** a área visível, mas "mostrar
/// o fim" e "mostrar do topo" coincidem matematicamente quando ela cabe
/// inteira. Sem efeito quando `linhas.len() >= altura_minima` (nada a
/// fazer, o scroll cuida do resto).
fn com_padding_no_topo(mut linhas: Vec<Line<'static>>, altura_minima: usize) -> Vec<Line<'static>> {
    if linhas.len() < altura_minima {
        let preenchimento = altura_minima - linhas.len();
        let mut com_padding = vec![Line::from(""); preenchimento];
        com_padding.append(&mut linhas);
        return com_padding;
    }
    linhas
}

/// Tela: histórico de chat (área rolável) em cima, caixa de entrada fixa
/// embaixo — rodapé da caixa de entrada mostra [`rodape_da_entrada`]. Com o
/// seletor de modelo aberto, um modal centralizado é desenhado por cima.
fn draw(frame: &mut Frame<'_>, estado: &Estado) {
    // Largura interna já conhecida antes do `Layout::split` — um *split*
    // vertical preserva a largura cheia em todas as áreas, então dá pra
    // calcular a altura da caixa de entrada (que depende de quantas linhas
    // o texto digitado ocupa depois do *wrap*) antes de montar o layout.
    let largura_interna = frame.area().width.saturating_sub(2) as usize;
    // Teto da caixa de entrada: um terço da altura do terminal, entre 3
    // (mínimo de sempre) e 12 linhas de altura total — generoso o
    // bastante pra mensagens de várias linhas sem tomar a tela inteira.
    let teto_altura_entrada = (frame.area().height / 3).clamp(3, 12);
    let linhas_entrada = quebrar_em_linhas(&estado.entrada, largura_interna.max(1));
    let altura_entrada = altura_da_entrada(linhas_entrada.len(), teto_altura_entrada);

    let areas = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(altura_entrada)])
        .split(frame.area());

    // Altura *interna* ao bloco com borda do histórico (2 linhas a menos
    // que a área cheia) — mesma conta usada pelo `ratatui` para desenhar
    // `Block::bordered()`.
    let altura_interna = areas[0].height.saturating_sub(2) as usize;

    if estado.chat.mensagens().is_empty() {
        let preenchimento_vertical = altura_interna.saturating_sub(LOGO_ABERTURA.len()) / 2;
        let mut linhas_do_logo = vec![Line::from(""); preenchimento_vertical];
        linhas_do_logo.extend(LOGO_ABERTURA.iter().map(|linha| Line::from(*linha)));
        let logo = Paragraph::new(linhas_do_logo)
            .alignment(Alignment::Center)
            .block(Block::bordered().title(" agentry "));
        frame.render_widget(logo, areas[0]);
    } else {
        let linhas = com_padding_no_topo(
            montar_linhas_do_historico(estado, largura_interna),
            altura_interna,
        );
        // `estado.scroll` conta "quantas linhas rolar para cima a partir do
        // fim" (0 = fim da conversa, sempre visível assim que uma mensagem
        // nova chega — achado num teste manual de usabilidade: a conversa
        // abria no topo, com a mensagem mais nova só visível depois de rolar
        // manualmente até o fim). Convertido aqui para o deslocamento
        // "a partir do topo" que a API do `ratatui` espera.
        let deslocamento_maximo = linhas.len().saturating_sub(altura_interna.max(1));
        let scroll_efetivo = (estado.scroll as usize).min(deslocamento_maximo);
        let deslocamento_do_topo = (deslocamento_maximo - scroll_efetivo) as u16;
        let historico = Paragraph::new(linhas)
            .block(Block::bordered().title(" agentry "))
            .scroll((deslocamento_do_topo, 0));
        frame.render_widget(historico, areas[0]);
    }

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
    let rodape = rodape_da_entrada(estado);
    // Altura *interior* (sem borda) da caixa de entrada — quando o texto
    // excede o teto, `linhas_entrada.len() > altura_interior_entrada`;
    // mostra sempre a **cauda** (o cursor está sempre no fim do texto, já
    // que a edição hoje só existe por `push`/`pop` no fim da `String`, sem
    // navegação de cursor no meio — não precisa de scroll configurável
    // separado, é sempre "role o bastante pra mostrar a última linha").
    let altura_interior_entrada = areas[1].height.saturating_sub(2) as usize;
    let deslocamento_entrada = linhas_entrada
        .len()
        .saturating_sub(altura_interior_entrada.max(1)) as u16;
    let caixa_de_entrada = Paragraph::new(linhas_entrada.join("\n"))
        .block(
            Block::bordered()
                .title(titulo_entrada)
                .title_bottom(Line::from(rodape).alignment(Alignment::Center)),
        )
        .scroll((deslocamento_entrada, 0));
    frame.render_widget(caixa_de_entrada, areas[1]);

    // Cursor real do terminal (pisca com o estilo nativo do terminal do
    // usuário, sem widget sintético) — só faz sentido quando a caixa de
    // entrada é de fato o alvo do foco, isto é, nenhum modal por cima
    // (seletor de modelo/pergunta ao agente) está capturando o teclado.
    if estado.seletor.is_none() && estado.solicitacao.is_none() {
        let ultima_linha = linhas_entrada.len().saturating_sub(1);
        let linha_visivel = ultima_linha.saturating_sub(deslocamento_entrada as usize) as u16;
        let coluna = linhas_entrada
            .last()
            .map(|linha| linha.chars().count())
            .unwrap_or(0) as u16;
        frame.set_cursor_position((areas[1].x + 1 + coluna, areas[1].y + 1 + linha_visivel));
    }

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
        SolicitacaoAtiva::Confirmacao { call, diff, .. } => {
            // Área maior que a de confirmação genérica — o diff de um
            // fs_write/fs_edit real costuma ter mais linhas do que cabe no
            // modal compacto original.
            let area = area_centralizada(70, 60, frame.area());
            frame.render_widget(Clear, area);
            let mut linhas = vec![Line::from(format!("tool: {}", call.name))];
            match diff {
                Some(linhas_diff) if !linhas_diff.is_empty() => {
                    linhas.push(Line::from(""));
                    linhas.extend(linhas_diff.iter().map(linha_de_diff));
                }
                _ => {
                    linhas.push(Line::from(format!("argumentos: {}", call.arguments)));
                }
            }
            linhas.push(Line::from(""));
            linhas.push(Line::from("Enter aprova · Esc recusa"));
            let paragrafo = Paragraph::new(linhas)
                .block(Block::bordered().title(" confirmar execução de tool "));
            frame.render_widget(paragrafo, area);
        }
        SolicitacaoAtiva::Pergunta {
            question,
            options,
            entrada,
            selecionada,
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
                let destacada = indice == *selecionada;
                let marcador = if destacada { "> " } else { "  " };
                let texto = format!("{marcador}{}. {opcao}", indice + 1);
                if destacada {
                    linhas.push(Line::styled(
                        texto,
                        Style::default().add_modifier(Modifier::BOLD),
                    ));
                } else {
                    linhas.push(Line::from(texto));
                }
            }
            if !options.is_empty() {
                linhas.push(Line::from(
                    "↑↓ escolhe · Enter (vazio) envia a opção · ou digite livremente",
                ));
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

/// Renderiza uma [`LinhaDiff`] como `Line` — prefixo `-`/`+`/` ` (mesma
/// convenção do `diff` do Unix) com cor vermelha/verde para linhas
/// removidas/adicionadas; linhas inalteradas ficam sem estilo especial.
fn linha_de_diff(linha: &LinhaDiff) -> Line<'static> {
    match linha {
        LinhaDiff::Removida(texto) => {
            Line::styled(format!("- {texto}"), Style::default().fg(Color::Red))
        }
        LinhaDiff::Adicionada(texto) => {
            Line::styled(format!("+ {texto}"), Style::default().fg(Color::Green))
        }
        LinhaDiff::Inalterada(texto) => Line::from(format!("  {texto}")),
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
    workspace_root: std::path::PathBuf,
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
        workspace_root,
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

// `workspace_root` (MT-88/ADR-0030, para o `CheckpointStore` de `Ctrl+Z`)
// leva a contagem a 8 — cada parâmetro já é uma peça distinta montada por
// `main()`/`run()` (terminal, sessão, roteador, ...), sem par natural para
// agrupar num `struct` de config só por isso.
#[allow(clippy::too_many_arguments)]
async fn loop_eventos(
    terminal: &mut DefaultTerminal,
    sessao_inicial: Session,
    router: Router,
    mut task_class: String,
    overrides: RuntimeOverride,
    mut rx_humano: mpsc::UnboundedReceiver<PedidoHumano>,
    auto: Arc<AtomicBool>,
    workspace_root: std::path::PathBuf,
) -> io::Result<()> {
    let router = Arc::new(router);
    let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(workspace_root.clone());
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
                        SolicitacaoAtiva::Confirmacao {
                            call,
                            diff,
                            responder,
                        } => match acao {
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
                                estado.solicitacao = Some(SolicitacaoAtiva::Confirmacao {
                                    call,
                                    diff,
                                    responder,
                                });
                            }
                        },
                        SolicitacaoAtiva::Pergunta {
                            question,
                            options,
                            mut entrada,
                            mut selecionada,
                            responder,
                        } => match acao {
                            Some(Action::Quit) => {
                                let _ = responder.send(SENTINELA_CANCELAMENTO_PERGUNTA.to_string());
                                return Ok(());
                            }
                            Some(Action::Send) => {
                                let resposta = resposta_da_pergunta(entrada, &options, selecionada);
                                let _ = responder.send(resposta);
                            }
                            Some(Action::Cancel) => {
                                let _ = responder.send(SENTINELA_CANCELAMENTO_PERGUNTA.to_string());
                            }
                            Some(Action::ScrollUp) => {
                                selecionada = mover_selecao_pergunta(selecionada, -1, options.len());
                                estado.solicitacao = Some(SolicitacaoAtiva::Pergunta {
                                    question,
                                    options,
                                    entrada,
                                    selecionada,
                                    responder,
                                });
                            }
                            Some(Action::ScrollDown) => {
                                selecionada = mover_selecao_pergunta(selecionada, 1, options.len());
                                estado.solicitacao = Some(SolicitacaoAtiva::Pergunta {
                                    question,
                                    options,
                                    entrada,
                                    selecionada,
                                    responder,
                                });
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
                                    selecionada,
                                    responder,
                                });
                            }
                            _ => {
                                estado.solicitacao = Some(SolicitacaoAtiva::Pergunta {
                                    question,
                                    options,
                                    entrada,
                                    selecionada,
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
                        Some(Action::OpenModelPicker)
                        | Some(Action::ToggleAuto)
                        | Some(Action::Undo)
                        | None => {
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
                        Some(Action::Undo) => {
                            estado
                                .chat
                                .registrar_mensagem_sistema(mensagem_de_undo(checkpoint_store.undo()));
                        }
                        Some(Action::OpenModelPicker) => {
                            let candidatos = router
                                .route_entry(&task_class)
                                .map(|entry| model_picker::a_partir_de_candidatos(&entry.candidates))
                                .unwrap_or_default();
                            estado.seletor = Some(SeletorDeModeloEstado::novo(candidatos));
                        }
                        Some(Action::Send) if estado.entrada.trim() == "/exit"
                            || estado.entrada.trim() == "/quit" =>
                        {
                            return Ok(());
                        }
                        Some(Action::Send) if estado.entrada.trim().starts_with('/') => {
                            if let Some(sessao) = sessao_atual.as_mut() {
                                let comando = estado
                                    .entrada
                                    .trim()
                                    .strip_prefix('/')
                                    .unwrap_or_default()
                                    .to_string();
                                estado.entrada.clear();
                                let mensagem = processar_comando_de_texto(
                                    &comando,
                                    sessao,
                                    router.as_ref(),
                                    &mut estado.overrides,
                                    &mut task_class,
                                    &checkpoint_store,
                                    &workspace_root,
                                )
                                .await;
                                estado.chat.registrar_mensagem_sistema(mensagem);
                            }
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
                        estado.usage_total = concluido.sessao.usage_total();
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
                    PedidoHumano::Confirmacao {
                        call,
                        diff,
                        responder,
                    } => SolicitacaoAtiva::Confirmacao {
                        call,
                        diff,
                        responder,
                    },
                    PedidoHumano::Pergunta {
                        question,
                        options,
                        responder,
                    } => SolicitacaoAtiva::Pergunta {
                        question,
                        options,
                        entrada: String::new(),
                        selecionada: 0,
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
    use ratatui::crossterm::event::KeyEvent;

    fn estado_vazio() -> Estado {
        Estado::new(RuntimeOverride::default(), Arc::new(AtomicBool::new(false)))
    }

    // --- quebrar_em_linhas / fatiar_palavra_longa (achado de usabilidade:
    // texto maior que a largura do terminal desaparecia da tela) ---

    #[test]
    fn quebrar_em_linhas_nao_quebra_texto_que_ja_cabe() {
        assert_eq!(quebrar_em_linhas("oi tudo bem", 20), vec!["oi tudo bem"]);
    }

    #[test]
    fn quebrar_em_linhas_quebra_no_espaco_mais_proximo_do_limite() {
        assert_eq!(
            quebrar_em_linhas("uma frase razoavelmente longa para quebrar", 10),
            vec!["uma frase", "razoavelme", "nte longa", "para", "quebrar"]
        );
    }

    #[test]
    fn quebrar_em_linhas_preserva_quebras_de_linha_explicitas() {
        assert_eq!(
            quebrar_em_linhas("linha um\nlinha dois", 20),
            vec!["linha um", "linha dois"]
        );
    }

    #[test]
    fn quebrar_em_linhas_preserva_linha_em_branco() {
        assert_eq!(
            quebrar_em_linhas("antes\n\ndepois", 20),
            vec!["antes", "", "depois"]
        );
    }

    #[test]
    fn fatiar_palavra_longa_corta_uma_palavra_maior_que_a_largura_inteira() {
        assert_eq!(
            fatiar_palavra_longa("umapalavraenormesemespacos", 10),
            vec!["umapalavra", "enormeseme", "spacos"]
        );
    }

    #[test]
    fn fatiar_palavra_longa_devolve_a_palavra_intacta_quando_ja_cabe() {
        assert_eq!(fatiar_palavra_longa("oi", 10), vec!["oi"]);
    }

    #[test]
    fn montar_linhas_do_historico_da_recuo_pendurado_nas_linhas_de_continuacao() {
        let mut estado = estado_vazio();
        estado.entrada = "uma mensagem razoavelmente comprida para quebrar em linhas".into();
        estado.preparar_envio();

        // "usuário: " tem 9 caracteres — largura pequena o bastante para
        // forçar quebra em várias linhas.
        let linhas = montar_linhas_do_historico(&estado, 20);

        let primeira: String = linhas[0].to_string();
        assert!(primeira.starts_with("usuário: "), "linha: {primeira:?}");
        let segunda: String = linhas[1].to_string();
        assert!(
            segunda.starts_with("         "),
            "continuação deveria começar com o mesmo recuo do prefixo, veio: {segunda:?}"
        );
        assert!(
            !segunda.contains("usuário:"),
            "prefixo não deve se repetir na continuação"
        );
    }

    #[test]
    fn montar_linhas_do_historico_estiliza_marcador_de_tool_de_forma_diferente() {
        let mut estado = estado_vazio();
        estado.entrada = "crie um arquivo".into();
        estado.preparar_envio();
        estado.chat.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "fs_write".into(),
        });

        let linhas = montar_linhas_do_historico(&estado, 40);
        let linha_do_marcador = linhas
            .iter()
            .find(|linha| linha.to_string().contains('⚙'))
            .expect("deve haver uma linha com o marcador de tool");

        assert_eq!(linha_do_marcador.style, ESTILO_MARCADOR_DE_TOOL);
    }

    // --- altura_da_entrada (achado de usabilidade: caixa de entrada sem
    // wrap nem altura dinâmica, diferente do histórico) ---

    // --- mover_selecao_pergunta / resposta_da_pergunta (achado de
    // usabilidade: ask_user respondido por número "1"/"2" tinha
    // comportamento inconsistente entre rodadas idênticas do mesmo prompt) ---

    #[test]
    fn mover_selecao_pergunta_satura_nos_limites() {
        assert_eq!(
            mover_selecao_pergunta(0, -1, 2),
            0,
            "não desce abaixo de zero"
        );
        assert_eq!(mover_selecao_pergunta(0, 1, 2), 1);
        assert_eq!(
            mover_selecao_pergunta(1, 1, 2),
            1,
            "não passa da última opção"
        );
    }

    #[test]
    fn mover_selecao_pergunta_sem_opcoes_permanece_em_zero() {
        assert_eq!(mover_selecao_pergunta(0, 1, 0), 0);
        assert_eq!(mover_selecao_pergunta(5, -1, 0), 0);
    }

    #[test]
    fn resposta_da_pergunta_com_campo_vazio_envia_o_texto_exato_da_opcao_destacada() {
        let options = vec!["Manter".to_string(), "Apagar".to_string()];

        assert_eq!(
            resposta_da_pergunta(String::new(), &options, 1),
            "Apagar",
            "deve enviar o texto da opção, nunca o número"
        );
    }

    #[test]
    fn resposta_da_pergunta_com_texto_digitado_ignora_a_selecao() {
        let options = vec!["Manter".to_string(), "Apagar".to_string()];

        assert_eq!(
            resposta_da_pergunta("resposta livre".to_string(), &options, 0),
            "resposta livre"
        );
    }

    #[test]
    fn resposta_da_pergunta_sem_opcoes_e_sempre_texto_livre_mesmo_vazio() {
        assert_eq!(resposta_da_pergunta(String::new(), &[], 0), "");
    }

    #[test]
    fn altura_da_entrada_texto_vazio_e_a_minima() {
        assert_eq!(altura_da_entrada(0, 12), 3);
        assert_eq!(altura_da_entrada(1, 12), 3);
    }

    #[test]
    fn altura_da_entrada_cresce_com_mais_linhas_de_conteudo() {
        assert_eq!(altura_da_entrada(3, 12), 5);
        assert_eq!(altura_da_entrada(5, 12), 7);
    }

    #[test]
    fn altura_da_entrada_satura_no_teto() {
        assert_eq!(altura_da_entrada(50, 12), 12);
    }

    #[test]
    fn altura_da_entrada_nunca_fica_abaixo_de_tres_mesmo_com_teto_baixo() {
        assert_eq!(altura_da_entrada(1, 2), 3);
    }

    // --- com_padding_no_topo (achado de usabilidade: conversa curta
    // aparecia no topo da caixa, com espaço em branco embaixo) ---

    #[test]
    fn com_padding_no_topo_preenche_quando_conversa_e_mais_curta_que_a_area() {
        let linhas = vec![Line::from("usuário: oi"), Line::from("agente: olá!")];

        let resultado = com_padding_no_topo(linhas, 5);

        assert_eq!(resultado.len(), 5);
        assert_eq!(resultado[0], Line::from(""));
        assert_eq!(resultado[1], Line::from(""));
        assert_eq!(resultado[2], Line::from(""));
        assert_eq!(resultado[3], Line::from("usuário: oi"));
        assert_eq!(resultado[4], Line::from("agente: olá!"));
    }

    #[test]
    fn com_padding_no_topo_nao_altera_quando_conversa_ja_preenche_a_area() {
        let linhas = vec![
            Line::from("linha 1"),
            Line::from("linha 2"),
            Line::from("linha 3"),
        ];

        let resultado = com_padding_no_topo(linhas.clone(), 3);

        assert_eq!(resultado, linhas);
    }

    #[test]
    fn com_padding_no_topo_nao_altera_quando_conversa_excede_a_area() {
        let linhas: Vec<Line> = (0..10).map(|i| Line::from(format!("linha {i}"))).collect();

        let resultado = com_padding_no_topo(linhas.clone(), 5);

        assert_eq!(
            resultado, linhas,
            "conversa maior que a área não deve ganhar padding"
        );
    }

    #[test]
    fn historia_curta_fica_ancorada_no_fim_apos_padding_mais_scroll() {
        // Reproduz a matemática de `draw`: depois do padding, uma conversa
        // curta ocupa exatamente `altura_interna` linhas, então
        // `deslocamento_maximo` é 0 e a última linha real cai na última
        // linha visível — sem isto, `deslocamento_do_topo` também dava 0,
        // mas a partir do topo de um vetor sem padding (conteúdo real no
        // topo, em vez de embaixo).
        let mut estado = estado_vazio();
        estado.entrada = "oi".into();
        estado.preparar_envio();

        let altura_interna = 20;
        let linhas = com_padding_no_topo(montar_linhas_do_historico(&estado, 40), altura_interna);
        let deslocamento_maximo = linhas.len().saturating_sub(altura_interna);

        assert_eq!(linhas.len(), altura_interna);
        assert_eq!(deslocamento_maximo, 0);
        assert!(
            linhas.last().unwrap().to_string().starts_with("agente:")
                || linhas.last().unwrap().to_string().starts_with("usuário:"),
            "última linha visível deve ser conteúdo real, não padding: {:?}",
            linhas.last()
        );
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
    fn rolar_para_cima_aumenta_a_distancia_do_fim_da_conversa() {
        let mut estado = estado_vazio();

        estado.rolar_para_cima();
        assert_eq!(estado.scroll, 1);

        estado.rolar_para_cima();
        assert_eq!(estado.scroll, 2);
    }

    #[test]
    fn rolar_para_baixo_sem_mensagens_permanece_em_zero() {
        let mut estado = estado_vazio();

        estado.rolar_para_baixo();

        assert_eq!(estado.scroll, 0);
    }

    #[test]
    fn rolar_para_baixo_nunca_fica_negativo_mesmo_depois_de_rolar_para_cima() {
        let mut estado = estado_vazio();
        estado.rolar_para_cima();
        estado.rolar_para_cima();

        for _ in 0..10 {
            estado.rolar_para_baixo();
        }

        assert_eq!(estado.scroll, 0);
    }

    #[test]
    fn preparar_envio_reseta_o_scroll_para_o_fim_da_conversa() {
        let mut estado = estado_vazio();
        estado.rolar_para_cima();
        estado.rolar_para_cima();
        assert_eq!(estado.scroll, 2);

        estado.entrada = "oi".into();
        estado.preparar_envio();

        assert_eq!(estado.scroll, 0);
    }

    #[test]
    fn rodape_da_entrada_inclui_a_legenda_e_o_uso_de_tokens_corrente() {
        let mut estado = estado_vazio();
        assert!(
            rodape_da_entrada(&estado).contains("0 tokens de entrada, 0 de saída (total: 0)"),
            "sessão nova deve mostrar uso zerado"
        );

        // Simula o que `EventoAgente::Concluido` faz ao final de um turno
        // real (`estado.usage_total = concluido.sessao.usage_total()`) —
        // sem terminal/Session reais, só o campo que `draw`/`rodape_da_entrada`
        // consomem.
        estado.usage_total = Usage {
            input_tokens: 10,
            output_tokens: 5,
        };

        let rodape = rodape_da_entrada(&estado);
        assert!(
            rodape.contains("10 tokens de entrada, 5 de saída (total: 15)"),
            "rodapé deve refletir o uso acumulado após um turno: {rodape}"
        );
        assert!(
            rodape.contains(&keybind::legenda()),
            "rodapé continua incluindo a legenda de keybindings, não só o uso"
        );
    }

    #[test]
    fn mensagem_de_undo_de_sucesso_usa_a_mesma_formatacao_do_undo_do_repl_e_one_shot() {
        let outcome = agentry_core::checkpoint::UndoOutcome {
            path: "a.txt".to_string(),
            acao: agentry_core::checkpoint::UndoAcao::Restaurado,
        };
        assert_eq!(
            mensagem_de_undo(Ok(outcome)),
            "[undo] 'a.txt' restaurado ao conteúdo anterior"
        );
    }

    #[test]
    fn mensagem_de_undo_de_erro_reporta_o_erro_sem_panic() {
        let mensagem = mensagem_de_undo(Err(agentry_core::checkpoint::CheckpointError::Vazio));
        assert_eq!(
            mensagem,
            "[undo] erro: nenhum checkpoint disponível para desfazer"
        );
    }

    #[test]
    fn ctrl_z_resolve_para_action_undo_sem_colidir_com_nenhuma_tecla_ja_mapeada() {
        let evento = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(keybind::resolve(evento), Some(Action::Undo));
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

    // --- processar_comando_de_texto (achado de usabilidade: /usage, /undo,
    // /remember digitados na TUI viravam mensagem de chat comum) ---

    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-tui-mod-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio do sistema não deve estar antes de 1970")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unico);
            std::fs::create_dir_all(&path).expect("deve criar diretório temporário de teste");
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn router_com_task_class(nome: &str, mock: Arc<MockProvider>) -> Router {
        let mut router = Router::new(EgressClass::LocalOnly);
        router.register_provider(mock.clone());
        router.set_route(
            nome,
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "mock",
                    "modelo-inicial",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );
        router
    }

    #[tokio::test]
    async fn comando_usage_devolve_o_uso_real_da_sessao_sem_chamar_o_provider() {
        let mock = Arc::new(MockProvider::new("mock"));
        let mut sessao = sessao_de_teste(mock.clone());
        let router = router_com_task_class("chat", mock.clone());
        let dir = TempDir::new();
        let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        let mut overrides = RuntimeOverride::default();
        let mut task_class = "chat".to_string();

        let mensagem = processar_comando_de_texto(
            "usage",
            &mut sessao,
            &router,
            &mut overrides,
            &mut task_class,
            &checkpoint_store,
            dir.path(),
        )
        .await;

        assert_eq!(
            mensagem,
            format!(
                "uso desta sessão: {}",
                crate::formatar_uso(sessao.usage_total())
            )
        );
        assert_eq!(mock.chat_requests().len(), 0, "não deve chamar o provider");
    }

    #[tokio::test]
    async fn comando_remember_grava_de_verdade_no_memory_store() {
        let mock = Arc::new(MockProvider::new("mock"));
        let mut sessao = sessao_de_teste(mock.clone());
        let router = router_com_task_class("chat", mock);
        let dir = TempDir::new();
        let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        let mut overrides = RuntimeOverride::default();
        let mut task_class = "chat".to_string();

        let mensagem = processar_comando_de_texto(
            "remember gosta de café",
            &mut sessao,
            &router,
            &mut overrides,
            &mut task_class,
            &checkpoint_store,
            dir.path(),
        )
        .await;

        assert_eq!(mensagem, "lembrado: gosta de café");
        let fatos = agentry_core::memory::MemoryStore::new(dir.path())
            .load()
            .expect("deve carregar o que acabou de gravar");
        assert_eq!(fatos, vec!["gosta de café".to_string()]);
    }

    #[tokio::test]
    async fn comando_remember_sem_fato_pede_uso_sem_gravar_nada() {
        let mock = Arc::new(MockProvider::new("mock"));
        let mut sessao = sessao_de_teste(mock.clone());
        let router = router_com_task_class("chat", mock);
        let dir = TempDir::new();
        let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        let mut overrides = RuntimeOverride::default();
        let mut task_class = "chat".to_string();

        let mensagem = processar_comando_de_texto(
            "remember",
            &mut sessao,
            &router,
            &mut overrides,
            &mut task_class,
            &checkpoint_store,
            dir.path(),
        )
        .await;

        assert_eq!(mensagem, "uso: /remember <fato>");
        let fatos = agentry_core::memory::MemoryStore::new(dir.path())
            .load()
            .expect("ausência de arquivo não é erro");
        assert!(fatos.is_empty());
    }

    #[tokio::test]
    async fn comando_undo_reaproveita_a_mesma_formatacao_do_ctrl_z() {
        let mock = Arc::new(MockProvider::new("mock"));
        let mut sessao = sessao_de_teste(mock.clone());
        let router = router_com_task_class("chat", mock);
        let dir = TempDir::new();
        let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        let mut overrides = RuntimeOverride::default();
        let mut task_class = "chat".to_string();

        let mensagem = processar_comando_de_texto(
            "undo",
            &mut sessao,
            &router,
            &mut overrides,
            &mut task_class,
            &checkpoint_store,
            dir.path(),
        )
        .await;

        // Sem nenhum checkpoint gravado — mesma mensagem de erro que
        // `mensagem_de_undo`/`Ctrl+Z` já produzem (fonte única, ver doc).
        assert_eq!(mensagem, mensagem_de_undo(checkpoint_store.undo()));
    }

    #[tokio::test]
    async fn comando_task_class_desconhecida_devolve_erro_sem_mudar_a_sessao() {
        let mock = Arc::new(MockProvider::new("mock"));
        let mut sessao = sessao_de_teste(mock.clone());
        let router = router_com_task_class("chat", mock);
        let dir = TempDir::new();
        let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        let mut overrides = RuntimeOverride::default();
        let mut task_class = "chat".to_string();

        let mensagem = processar_comando_de_texto(
            "task-class nao-existe",
            &mut sessao,
            &router,
            &mut overrides,
            &mut task_class,
            &checkpoint_store,
            dir.path(),
        )
        .await;

        assert!(mensagem.starts_with("erro:"), "mensagem: {mensagem:?}");
        assert_eq!(
            task_class, "chat",
            "task-class ativa não deve mudar em falha"
        );
    }

    #[tokio::test]
    async fn comando_model_recusa_e_aponta_para_o_seletor() {
        let mock = Arc::new(MockProvider::new("mock"));
        let mut sessao = sessao_de_teste(mock.clone());
        let router = router_com_task_class("chat", mock);
        let dir = TempDir::new();
        let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        let mut overrides = RuntimeOverride::default();
        let mut task_class = "chat".to_string();

        let mensagem = processar_comando_de_texto(
            "model gpt-4",
            &mut sessao,
            &router,
            &mut overrides,
            &mut task_class,
            &checkpoint_store,
            dir.path(),
        )
        .await;

        assert!(mensagem.contains("Ctrl+P"), "mensagem: {mensagem:?}");
        assert_eq!(overrides.model, None, "não deve tentar mudar o modelo");
    }

    #[tokio::test]
    async fn comando_generico_passa_por_aplicar_comando_e_reaplica_a_rota() {
        let mock = Arc::new(MockProvider::new("mock"));
        let mut sessao = sessao_de_teste(mock.clone());
        let router = router_com_task_class("chat", mock);
        let dir = TempDir::new();
        let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        let mut overrides = RuntimeOverride::default();
        let mut task_class = "chat".to_string();

        let mensagem = processar_comando_de_texto(
            "temperature 0.2",
            &mut sessao,
            &router,
            &mut overrides,
            &mut task_class,
            &checkpoint_store,
            dir.path(),
        )
        .await;

        assert_eq!(mensagem, "temperature alterada para: 0.2");
        assert_eq!(overrides.temperature, Some(0.2));
    }

    #[tokio::test]
    async fn comando_desconhecido_devolve_erro_sem_efeito_colateral() {
        let mock = Arc::new(MockProvider::new("mock"));
        let mut sessao = sessao_de_teste(mock.clone());
        let router = router_com_task_class("chat", mock);
        let dir = TempDir::new();
        let checkpoint_store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        let mut overrides = RuntimeOverride::default();
        let mut task_class = "chat".to_string();

        let mensagem = processar_comando_de_texto(
            "isso-nao-existe",
            &mut sessao,
            &router,
            &mut overrides,
            &mut task_class,
            &checkpoint_store,
            dir.path(),
        )
        .await;

        assert_eq!(mensagem, "comando desconhecido: /isso-nao-existe");
    }
}
