// Caminho relativo: crates/core/src/session/mod.rs
//! Agent loop ReAct mínimo (MT-10): laço mensagem → tool-call → observação,
//! com streaming e orçamento de tokens, sobre qualquer [`LlmProvider`]
//! (`MockProvider` do MT-03 ou o adapter Ollama do MT-08).
//!
//! `Session` é construída a partir de uma [`ResolvedRoute`] (Router, MT-09) —
//! não recebe provider/modelo soltos — e aplica o [`CallPreset`] resolvido a
//! cada turno (MT-31, ADR-0008): `temperature`/`top_p`/`max_tokens` vão no
//! `ChatRequest`; `system_prompt` (se houver) é anteposto ao histórico como
//! `Message::system(...)` comum, uma única vez.
//!
//! Execução real de tools (fs, shell) ainda não existe — chega nos MT-11+.
//! Aqui só o contrato [`ToolExecutor`] que o loop consome, dyn-compatible via
//! [`BoxFuture`] no mesmo padrão de [`LlmProvider`] (MT-03), sem `async-trait`.
//!
//! [`reviewer`] traz o Reviewer — auditoria semântica pós-`Done` via
//! `task-class` dedicada (MT-34, ADR-0015); a integração ao loop
//! (`run`/`run_streaming`, modos `advisory`/`blocking` com retry limitado)
//! é o MT-35.

pub mod persist;
pub mod reviewer;

use std::collections::HashMap;
use std::ops::ControlFlow;
use std::sync::Arc;

use crate::guardrail::{
    GuardrailAuditEntry, GuardrailAuditSink, GuardrailCheckResult, GuardrailDirection,
    GuardrailGate,
};
use crate::model::{ContentBlock, Message, Role, StreamEvent, ToolCall, ToolResult, Usage};
use crate::provider::{BoxFuture, ChatRequest, LlmProvider, ProviderError, ToolSpec};
use crate::router::{CallPreset, ResolvedRoute, Router, RouterError};
use reviewer::{AuditKind, ReviewResult, ReviewerError, Veredito};

/// Rótulo de tarefa usado nas [`GuardrailAuditEntry`] emitidas pela sessão
/// (MT-45, ADR-0007) — só identifica a origem no log de auditoria, não
/// afeta a decisão de bloqueio/redação.
const GUARDRAIL_TASK: &str = "session::guardrail";

/// Executa uma chamada de tool solicitada pelo modelo e devolve a observação.
///
/// Implementações reais (fs, shell, etc.) chegam nos MT-11+; esta trait é só
/// o contrato que o agent loop consome.
pub trait ToolExecutor: Send + Sync {
    /// Executa `call` e devolve o [`ToolResult`] observado pelo loop.
    fn execute(&self, call: &ToolCall) -> BoxFuture<'_, ToolResult>;
}

/// Orçamento de tokens do agent loop: total (entrada + saída) que o loop
/// pode consumir antes de encerrar, mesmo com tool-calls pendentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenBudget {
    /// Total de tokens que o loop pode consumir antes de parar.
    pub max_tokens: u64,
}

impl TokenBudget {
    /// Cria um orçamento com o limite dado.
    #[must_use]
    pub fn new(max_tokens: u64) -> Self {
        Self { max_tokens }
    }
}

/// Razão pela qual o loop encerrou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// O modelo respondeu sem solicitar nenhuma tool (resposta final).
    Done,
    /// O orçamento de tokens foi atingido antes de uma resposta final.
    BudgetExceeded,
    /// O teto de turnos consecutivos com tool-call foi atingido antes de
    /// uma resposta final (ADR-0033) — independente do orçamento de
    /// tokens. Rede de segurança contra um modelo que fica chamando tools
    /// indefinidamente (achado na rodada 4 de teste manual: `ask_user`
    /// respondido, modelo seguiu chamando tools/mandando mensagens em
    /// loop). Histórico e uso acumulado preservados, mesmo padrão de
    /// [`Self::BudgetExceeded`] — nunca um erro fatal.
    MaxTurnsExceeded,
}

/// Teto *default* de turnos consecutivos com tool-call (ADR-0033) quando
/// [`Session`] não é construída com [`Session::with_max_tool_turns`] — bem
/// mais generoso que qualquer tarefa legítima costuma precisar, só para
/// nunca deixar um modelo em loop sem controle independente do orçamento
/// de tokens (que pode ser grande o bastante para o loop parecer "travado"
/// por muito tempo antes de parar).
pub const DEFAULT_MAX_TOOL_TURNS: u32 = 25;

/// Resultado de rodar o loop até encerrar.
#[derive(Debug, Clone, PartialEq)]
pub struct SessionOutcome {
    /// Por que o loop parou.
    pub reason: StopReason,
    /// Consumo total de tokens acumulado em todos os turnos.
    pub usage: Usage,
    /// Número de turnos (chamadas ao provider) executados.
    pub turns: u32,
    /// Vereditos do Reviewer (MT-34/35, ADR-0015) — vazio quando nenhuma
    /// auditoria está habilitada para a sessão (*default*). Uma falha
    /// persistente (veredito `Fail` em modo `Blocking` mesmo após esgotar
    /// o teto de retentativas) aparece aqui, nunca suprimida.
    pub reviews: Vec<ReviewResult>,
    /// Regras de Guardrail Gate que efetivamente agiram nesta chamada a
    /// `run`/`run_streaming` (entrada e/ou saída, MT-45/ADR-0007) — vazio
    /// quando nenhum guardrail está habilitado (*default*) ou nenhuma regra
    /// casou. Mesmas entradas emitidas ao `GuardrailAuditSink` configurado,
    /// aqui só para observabilidade direta do chamador/teste.
    pub guardrail_hits: Vec<GuardrailAuditEntry>,
}

/// Modo de disparo de uma auditoria habilitada (MT-35, ADR-0015).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewMode {
    /// O veredito é anexado a [`SessionOutcome::reviews`] — nunca bloqueia
    /// a resposta de chegar ao usuário.
    Advisory,
    /// Um veredito `Fail` gera um turno corretivo (notas como observação),
    /// até [`Session::with_reviews`]'s `retry_limit` retentativas; esgotado
    /// o teto, a falha persistente é exposta, nunca suprimida.
    Blocking,
}

/// Auditoria habilitada para a sessão: tipo + modo de disparo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewConfig {
    pub kind: AuditKind,
    pub mode: ReviewMode,
}

/// Erros do agent loop.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionError {
    /// O provider devolveu um erro.
    Provider(ProviderError),
    /// O Router não conseguiu resolver uma `task-class` pedida (ex.: `"compact"`, MT-36).
    Router(RouterError),
    /// O Reviewer (MT-34/35) falhou ao rodar uma auditoria habilitada.
    Reviewer(ReviewerError),
}

impl core::fmt::Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Provider(e) => write!(f, "erro do provider: {e}"),
            Self::Router(e) => write!(f, "erro de roteamento: {e}"),
            Self::Reviewer(e) => write!(f, "erro do reviewer: {e}"),
        }
    }
}

impl std::error::Error for SessionError {}

/// Acumula os eventos de um [`crate::provider::ChatStream`] num turno em uma
/// [`Message`] final + [`Usage`] — a mesma reconstrução que um cliente de
/// streaming real (CLI, MT-14) faria para exibir a resposta incrementalmente
/// e, ao final, ter a mensagem completa para o histórico.
#[derive(Default)]
struct StreamAggregator {
    text: String,
    ordem: Vec<String>,
    tool_calls: HashMap<String, (String, String)>,
    usage: Usage,
}

impl StreamAggregator {
    fn apply(&mut self, event: &StreamEvent) {
        match event {
            StreamEvent::MessageStart => {}
            StreamEvent::TextDelta { text } => self.text.push_str(text),
            StreamEvent::ToolCallStart { id, name } => {
                self.ordem.push(id.clone());
                self.tool_calls
                    .insert(id.clone(), (name.clone(), String::new()));
            }
            StreamEvent::ToolCallDelta { id, delta } => {
                if let Some((_, argumentos)) = self.tool_calls.get_mut(id) {
                    argumentos.push_str(delta);
                }
            }
            StreamEvent::MessageEnd { usage } => self.usage = *usage,
            // Resultado de uma tool já executada (ADR-0035/MT-114) não é
            // parte da mensagem do *modelo* sendo acumulada aqui -- é uma
            // notificação de canal lateral, emitida por quem chama
            // `after_response`, não por este agregador.
            StreamEvent::ToolCallResult { .. } => {}
        }
    }

    fn into_message(self) -> (Message, Usage) {
        let mut blocks = Vec::new();
        if !self.text.is_empty() {
            blocks.push(ContentBlock::Text { text: self.text });
        }
        for id in &self.ordem {
            if let Some((name, argumentos_json)) = self.tool_calls.get(id) {
                let arguments =
                    serde_json::from_str(argumentos_json).unwrap_or(serde_json::Value::Null);
                blocks.push(ContentBlock::ToolCall(ToolCall {
                    id: id.clone(),
                    name: name.clone(),
                    arguments,
                }));
            }
        }
        (
            Message {
                role: Role::Assistant,
                content: blocks,
            },
            self.usage,
        )
    }
}

fn extract_tool_calls(message: &Message) -> Vec<ToolCall> {
    message
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::ToolCall(chamada) => Some(chamada.clone()),
            _ => None,
        })
        .collect()
}

/// Emite `texto` como uma sequência sintética `MessageStart`/`TextDelta`/
/// `MessageEnd` (MT-47) — usada por `run_streaming` no lugar dos eventos
/// brutos do provider quando o Guardrail Gate de saída está habilitado: o
/// texto final (original, mascarado ou o aviso fixo, conforme o resultado de
/// `aplicar_guardrail_saida`) só é conhecido depois que o turno inteiro já
/// foi acumulado, então não há como preservar o *chunking* original — um
/// único `TextDelta` equivale, para quem consome `on_event`, ao mesmo texto
/// final que chegaria ao histórico de qualquer forma.
fn emitir_texto_como_eventos<F: FnMut(&StreamEvent)>(on_event: &mut F, texto: &str, usage: Usage) {
    on_event(&StreamEvent::MessageStart);
    if !texto.is_empty() {
        on_event(&StreamEvent::TextDelta {
            text: texto.to_string(),
        });
    }
    on_event(&StreamEvent::MessageEnd { usage });
}

/// Renderiza o histórico como transcript de texto simples para o prompt de
/// compactação (MT-36) — não é um formato de fio de provider nenhum, só uma
/// representação legível o bastante para o modelo resumir.
fn render_transcript(messages: &[Message]) -> String {
    messages
        .iter()
        .map(|message| {
            let papel = match message.role {
                Role::System => "sistema",
                Role::User => "usuário",
                Role::Assistant => "assistente",
                Role::Tool => "tool",
            };
            let conteudo = message
                .content
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => text.clone(),
                    ContentBlock::ToolCall(chamada) => {
                        format!(
                            "[chamou a tool '{}' com {}]",
                            chamada.name, chamada.arguments
                        )
                    }
                    ContentBlock::ToolResult(resultado) => {
                        if resultado.is_error {
                            format!(
                                "[erro da tool ({}): {}]",
                                resultado.call_id, resultado.content
                            )
                        } else {
                            format!(
                                "[resultado da tool ({}): {}]",
                                resultado.call_id, resultado.content
                            )
                        }
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            format!("{papel}: {conteudo}")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// Encaminha cada [`GuardrailAuditEntry`] ao [`GuardrailAuditSink`] real
/// configurado na sessão e também as acumula localmente — permite popular
/// [`SessionOutcome::guardrail_hits`] (MT-45) sem duplicar a decisão de
/// auditoria em dois lugares. `Mutex` (não `RefCell`) porque
/// `GuardrailAuditSink` exige `Send + Sync`, mesmo não havendo concorrência
/// real aqui (mesma disciplina já usada pelos coletores de teste do MT-43).
struct ColetorDuplo<'a> {
    externo: &'a dyn GuardrailAuditSink,
    coletados: std::sync::Mutex<Vec<GuardrailAuditEntry>>,
}

impl<'a> ColetorDuplo<'a> {
    fn new(externo: &'a dyn GuardrailAuditSink) -> Self {
        Self {
            externo,
            coletados: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn into_entradas(self) -> Vec<GuardrailAuditEntry> {
        self.coletados
            .into_inner()
            .expect("mutex do coletor não deve envenenar")
    }
}

impl GuardrailAuditSink for ColetorDuplo<'_> {
    fn record(&self, entry: GuardrailAuditEntry) {
        self.externo.record(entry.clone());
        self.coletados
            .lock()
            .expect("mutex do coletor não deve envenenar")
            .push(entry);
    }
}

/// Uma sessão do agent loop: histórico de mensagens + provider + executor de
/// tools + orçamento de tokens.
pub struct Session {
    provider: Arc<dyn LlmProvider>,
    model: String,
    preset: CallPreset,
    /// Sinal de troca de modelo da última rota aplicada (MT-17, ADR-0009) —
    /// repassado ao `ChatRequest` do próximo turno; só o adapter Ollama
    /// consome hoje.
    is_model_switch: bool,
    tools: Vec<ToolSpec>,
    executor: Arc<dyn ToolExecutor>,
    messages: Vec<Message>,
    budget: TokenBudget,
    /// Auditorias do Reviewer habilitadas (MT-34/35, ADR-0015) — vazio por
    /// padrão ("desligado por padrão", diferente do pacote ADR-0010..0013).
    reviews: Vec<ReviewConfig>,
    /// Teto de retentativas para vereditos `Fail` em modo `Blocking` (mesma
    /// disciplina de limite do [`TokenBudget`] — nunca loopar indefinidamente).
    review_retry_limit: u32,
    /// Guardrail Gate habilitado (MT-43/44/45, ADR-0007) — `None` por
    /// padrão (nenhuma checagem), mesmo "desligado até configurado" de
    /// [`Self::reviews`].
    guardrails: Option<(Arc<GuardrailGate>, Arc<dyn GuardrailAuditSink>)>,
    /// Instruções de projeto (`AGENTS.md`/`CLAUDE.md`, MT-59/ADR-0023) —
    /// `None` por padrão (nenhum arquivo lido/configurado). Concatenadas
    /// antes do `system_prompt` do preset em [`Self::ensure_system_prompt`].
    project_instructions: Option<String>,
    /// Memória de projeto explícita (`/remember`/`--remember`, MT-94/
    /// ADR-0032), já renderizada (`memory::render_memoria`) — `None` por
    /// padrão (nenhum fato gravado ainda). Concatenada logo depois das
    /// instruções de projeto em [`Self::ensure_system_prompt`] — mesma
    /// categoria de contexto durável específico do projeto, mas curado
    /// pelo usuário em vez de commitado no repositório.
    memoria: Option<String>,
    /// Lista compacta de skills descobertas (`SKILL.md`, MT-60/ADR-0023),
    /// já renderizada (`skills::render_skills_list`) — `None` por padrão.
    /// Concatenada **por último** em [`Self::ensure_system_prompt`], depois
    /// das instruções de projeto e do `system_prompt` do preset.
    skills_list: Option<String>,
    /// Uso de tokens acumulado ao longo de **toda** a sessão (MT-82,
    /// ADR-0029) — soma o `Usage` de cada turno concluído (`run`/
    /// `run_streaming`) e de cada chamada de [`Self::compact`], nunca
    /// reseta sozinho (só existe uma sessão nova para zerar). Distinto do
    /// `consumed` local de `run`/`run_streaming`, que só existe durante
    /// **uma** chamada (para decidir estouro de [`TokenBudget`]) — este
    /// campo persiste entre chamadas, exposto via [`Self::usage_total`].
    usage_total: Usage,
    /// Teto de turnos consecutivos com tool-call (ADR-0033) — `run`/
    /// `run_streaming` param com [`StopReason::MaxTurnsExceeded`] ao
    /// atingi-lo, independente do orçamento de tokens. [`DEFAULT_MAX_TOOL_TURNS`]
    /// por padrão; ajustável via [`Self::with_max_tool_turns`].
    max_tool_turns: u32,
}

impl Session {
    /// Cria uma sessão a partir de uma rota já resolvida pelo Router
    /// (ADR-0008/MT-09) — sem tools declaradas; use [`Self::with_tools`].
    #[must_use]
    pub fn new(route: ResolvedRoute, executor: Arc<dyn ToolExecutor>, budget: TokenBudget) -> Self {
        Self {
            provider: route.provider,
            model: route.model,
            preset: route.preset,
            is_model_switch: route.is_model_switch,
            tools: Vec::new(),
            executor,
            messages: Vec::new(),
            budget,
            reviews: Vec::new(),
            review_retry_limit: 0,
            guardrails: None,
            project_instructions: None,
            memoria: None,
            skills_list: None,
            usage_total: Usage::default(),
            max_tool_turns: DEFAULT_MAX_TOOL_TURNS,
        }
    }

    /// Ajusta o teto de turnos consecutivos com tool-call (ADR-0033) —
    /// *default* [`DEFAULT_MAX_TOOL_TURNS`] se nunca chamado.
    #[must_use]
    pub fn with_max_tool_turns(mut self, max_tool_turns: u32) -> Self {
        self.max_tool_turns = max_tool_turns;
        self
    }

    /// Declara as tools oferecidas ao modelo (via [`ChatRequest::tools`]).
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolSpec>) -> Self {
        self.tools = tools;
        self
    }

    /// Habilita auditorias do Reviewer (MT-34/35, ADR-0015) para esta
    /// sessão — *default* vazio (nenhuma auditoria roda). `retry_limit` é
    /// o teto de retentativas para vereditos `Fail` em modo
    /// [`ReviewMode::Blocking`].
    #[must_use]
    pub fn with_reviews(mut self, reviews: Vec<ReviewConfig>, retry_limit: u32) -> Self {
        self.reviews = reviews;
        self.review_retry_limit = retry_limit;
        self
    }

    /// Habilita o Guardrail Gate (MT-43/44/45, ADR-0007) para esta sessão —
    /// *default* nenhum (nenhuma checagem de entrada/saída roda). `gate` traz
    /// as regras resolvidas por `Config` (MT-44); `sink` recebe cada
    /// [`GuardrailAuditEntry`] emitida por uma regra que efetivamente agiu.
    #[must_use]
    pub fn with_guardrails(
        mut self,
        gate: Arc<GuardrailGate>,
        sink: Arc<dyn GuardrailAuditSink>,
    ) -> Self {
        self.guardrails = Some((gate, sink));
        self
    }

    /// Define as instruções de projeto (`AGENTS.md`/`CLAUDE.md`, MT-59/
    /// ADR-0023) desta sessão — *default* nenhuma. Concatenadas antes do
    /// `system_prompt` do preset numa única mensagem de sistema (ver
    /// [`Self::ensure_system_prompt`]); chamar de novo antes do primeiro
    /// turno substitui o valor anterior (mesmo padrão dos demais builders).
    #[must_use]
    pub fn with_project_instructions(mut self, texto: impl Into<String>) -> Self {
        self.project_instructions = Some(texto.into());
        self
    }

    /// Define a memória de projeto explícita (`/remember`/`--remember`,
    /// MT-94/ADR-0032) desta sessão — *default* nenhuma. Já deve vir
    /// renderizada (`memory::render_memoria`); concatenada logo depois das
    /// instruções de projeto na mensagem de sistema (ver
    /// [`Self::ensure_system_prompt`]).
    #[must_use]
    pub fn with_memoria(mut self, texto: impl Into<String>) -> Self {
        self.memoria = Some(texto.into());
        self
    }

    /// Define a lista compacta de skills descobertas (`SKILL.md`, MT-60/
    /// ADR-0023) desta sessão — *default* nenhuma. Já deve vir renderizada
    /// (`skills::render_skills_list`); concatenada **por último** na
    /// mensagem de sistema (ver [`Self::ensure_system_prompt`]).
    #[must_use]
    pub fn with_skills_list(mut self, texto: impl Into<String>) -> Self {
        self.skills_list = Some(texto.into());
        self
    }

    /// Acrescenta uma mensagem de usuário ao histórico antes de rodar o loop.
    pub fn push_user_message(&mut self, text: impl Into<String>) {
        self.messages.push(Message::user(text));
    }

    /// Aplica uma nova rota (provider/modelo/preset) à sessão, **preservando**
    /// o histórico de mensagens acumulado.
    ///
    /// Usado pelo REPL (MT-14) quando o usuário troca de modelo/parâmetro via
    /// comando (`/model`, `/temperature` etc.) — a conversa continua, só a
    /// rota resolvida muda a partir do próximo turno. Note que uma
    /// `system_prompt` diferente na nova rota **não** substitui a mensagem de
    /// sistema já inserida no histórico (`ensure_system_prompt` só age uma
    /// vez); trocar o *system prompt* no meio de uma conversa começada é uma
    /// interação fora do escopo do MT-14.
    pub fn apply_route(&mut self, route: ResolvedRoute) {
        self.provider = route.provider;
        self.model = route.model;
        self.preset = route.preset;
        self.is_model_switch = route.is_model_switch;
    }

    /// Compacta o histórico acumulado num único resumo (MT-36, ADR-0016):
    /// resolve a `task-class` `"compact"` via `router`, pede um resumo em uma
    /// chamada de chat simples (sem tools, sem streaming) e substitui
    /// `self.messages` inteiro por uma única mensagem de sistema com o
    /// resumo. Histórico vazio é um no-op.
    ///
    /// Disparo é sempre explícito — este método nunca é chamado
    /// automaticamente pelo loop (ADR-0016); quem decide quando compactar é
    /// quem chama (ex.: comando `/compact` do REPL, MT-37).
    ///
    /// A chamada de compactação em si consome tokens reais — seu `Usage` é
    /// somado a [`Self::usage_total`] como qualquer outro turno (MT-82,
    /// ADR-0029); o total nunca **reseta** por compactar (resumir histórico
    /// não é "começar de novo" do ponto de vista de uso já consumido).
    ///
    /// # Errors
    ///
    /// Devolve [`SessionError::Router`] se a `task-class` `"compact"` não
    /// resolver, ou [`SessionError::Provider`] se a chamada de compactação
    /// falhar — em qualquer um dos dois casos, `self.messages` permanece
    /// intocado (tudo-ou-nada).
    pub async fn compact(&mut self, router: &Router) -> Result<(), SessionError> {
        if self.messages.is_empty() {
            return Ok(());
        }

        let route = router.resolve("compact").map_err(SessionError::Router)?;
        let instrucao = format!(
            "Resuma de forma concisa a conversa abaixo, preservando decisões, fatos e \
             qualquer estado necessário para continuar o trabalho. Responda apenas com \
             o resumo, sem comentários adicionais.\n\n{}",
            render_transcript(&self.messages)
        );

        let mut request = ChatRequest::new(route.model.clone(), vec![Message::user(instrucao)]);
        request.max_tokens = route.preset.max_tokens;
        request.temperature = route.preset.temperature;
        request.top_p = route.preset.top_p;
        request.is_model_switch = route.is_model_switch;

        let resposta = route
            .provider
            .chat(request)
            .await
            .map_err(SessionError::Provider)?;

        self.usage_total = Usage {
            input_tokens: self.usage_total.input_tokens + resposta.usage.input_tokens,
            output_tokens: self.usage_total.output_tokens + resposta.usage.output_tokens,
        };
        self.messages = vec![Message::system(resposta.message.text_content())];
        Ok(())
    }

    /// Histórico de mensagens acumulado até aqui.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Nome do provider corrente (`LlmProvider::name`) — usado por `/save`
    /// (MT-121, ADR-0036) para registrar nos metadados da sessão salva.
    #[must_use]
    pub fn provider_name(&self) -> &str {
        self.provider.name()
    }

    /// Identificador do modelo corrente — mesmo uso do
    /// [`Self::provider_name`].
    #[must_use]
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Uso de tokens acumulado ao longo de **toda** a sessão até aqui —
    /// soma de cada turno (`run`/`run_streaming`) e de cada [`Self::compact`]
    /// já concluídos (MT-82, ADR-0029). Zerado só ao criar uma [`Session`]
    /// nova (`Self::new`); nunca reseta sozinho, inclusive após `compact`.
    #[must_use]
    pub fn usage_total(&self) -> Usage {
        self.usage_total
    }

    /// Garante que a mensagem de sistema esteja no início do histórico —
    /// insere só uma vez; chamadas seguintes (novos turnos, ou novas
    /// mensagens de usuário) não duplicam. Concatena, nesta ordem: as
    /// instruções de projeto (`AGENTS.md`/`CLAUDE.md`, MT-59/ADR-0023 — mais
    /// gerais), a memória de projeto explícita (`/remember`/`--remember`,
    /// MT-94/ADR-0032 — mesma categoria de contexto durável, mas curado
    /// pelo usuário), o `system_prompt` do preset da `task-class` ativa
    /// (mais específico) e, por último, a lista compacta de skills
    /// descobertas (MT-60/ADR-0023) — separados por uma linha em branco
    /// entre os presentes; uma única mensagem de sistema, nunca mais de uma.
    fn ensure_system_prompt(&mut self) {
        if self.messages.iter().any(|m| m.role == Role::System) {
            return;
        }
        let combinado = [
            self.project_instructions.as_deref(),
            self.memoria.as_deref(),
            self.preset.system_prompt.as_deref(),
            self.skills_list.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join("\n\n");
        if !combinado.is_empty() {
            self.messages.insert(0, Message::system(combinado));
        }
    }

    fn build_request(&mut self) -> ChatRequest {
        self.ensure_system_prompt();
        ChatRequest {
            model: self.model.clone(),
            messages: self.messages.clone(),
            tools: self.tools.clone(),
            max_tokens: self.preset.max_tokens,
            temperature: self.preset.temperature,
            top_p: self.preset.top_p,
            reasoning: self.preset.reasoning,
            is_model_switch: self.is_model_switch,
        }
    }

    /// Processa a resposta de um turno: soma o uso, decide se o orçamento
    /// estourou, e — se houver tool-calls e orçamento restante — executa cada
    /// uma e acrescenta a observação ao histórico como mensagem `Tool`.
    ///
    /// `resultados` (ADR-0035/MT-114) recebe uma cópia de cada [`ToolResult`]
    /// executado nesta chamada, na ordem de execução — mesmo padrão de
    /// parâmetro de saída por mutação já usado por `consumed: &mut Usage`.
    /// `Session::run` (não-*streaming*) passa um `Vec` descartável;
    /// `Session::run_streaming` drena e emite cada um como
    /// [`StreamEvent::ToolCallResult`] via `on_event`. Fica vazio sempre que
    /// esta chamada não executa nenhuma tool (ex.: resposta final sem
    /// tool-calls, ou parada por orçamento/teto de turnos antes de executar).
    ///
    /// Devolve `Some(outcome)` quando o loop deve parar neste turno.
    async fn after_response(
        &mut self,
        message: Message,
        turn_usage: Usage,
        consumed: &mut Usage,
        turns: u32,
        resultados: &mut Vec<ToolResult>,
    ) -> Option<SessionOutcome> {
        *consumed = Usage {
            input_tokens: consumed.input_tokens + turn_usage.input_tokens,
            output_tokens: consumed.output_tokens + turn_usage.output_tokens,
        };
        self.usage_total = Usage {
            input_tokens: self.usage_total.input_tokens + turn_usage.input_tokens,
            output_tokens: self.usage_total.output_tokens + turn_usage.output_tokens,
        };
        let tool_calls = extract_tool_calls(&message);
        self.messages.push(message);

        if consumed.total() >= self.budget.max_tokens {
            return Some(SessionOutcome {
                reason: StopReason::BudgetExceeded,
                usage: *consumed,
                turns,
                reviews: Vec::new(),
                guardrail_hits: Vec::new(),
            });
        }

        if tool_calls.is_empty() {
            return Some(SessionOutcome {
                reason: StopReason::Done,
                usage: *consumed,
                turns,
                reviews: Vec::new(),
                guardrail_hits: Vec::new(),
            });
        }

        // Turno tem tool-calls (o loop continuaria) e já atingiu o teto
        // (ADR-0033) — para **antes** de executar mais uma rodada de
        // tools, nunca depois; o histórico já acumulado até aqui (incluindo
        // a mensagem do modelo que pediu essas tool-calls, já empurrada
        // acima) é preservado, mesmo padrão de `BudgetExceeded`.
        if turns >= self.max_tool_turns {
            return Some(SessionOutcome {
                reason: StopReason::MaxTurnsExceeded,
                usage: *consumed,
                turns,
                reviews: Vec::new(),
                guardrail_hits: Vec::new(),
            });
        }

        let mut result_blocks = Vec::with_capacity(tool_calls.len());
        for call in &tool_calls {
            let result = self.executor.execute(call).await;
            resultados.push(result.clone());
            result_blocks.push(ContentBlock::ToolResult(result));
        }
        self.messages.push(Message {
            role: Role::Tool,
            content: result_blocks,
        });

        None
    }

    /// Aplica o Guardrail Gate (ADR-0007 §1) sobre a mensagem de usuário
    /// mais recente, **antes** de qualquer chamada ao provider (MT-45).
    /// `Some(outcome)` decide o desfecho do turno sem tocar o provider
    /// (bloqueio: substitui a mensagem por um aviso fixo e sinaliza
    /// `StopReason::Done` com zero turnos); `None` segue o fluxo normal —
    /// nada casou, ou casou `redact` e a mensagem em `self.messages` já
    /// saiu mascarada. Entradas de auditoria (bloqueio ou redação) são
    /// acrescentadas a `hits`, para o chamador anexar ao `SessionOutcome`
    /// final, qualquer que seja o caminho de saída do loop.
    fn aplicar_guardrail_entrada(
        &mut self,
        hits: &mut Vec<GuardrailAuditEntry>,
    ) -> Option<SessionOutcome> {
        let (gate, sink) = self.guardrails.clone()?;
        let indice = self.messages.iter().rposition(|m| m.role == Role::User)?;
        let texto = self.messages[indice].text_content();

        let coletor = ColetorDuplo::new(sink.as_ref());
        let resultado = gate.check(GuardrailDirection::Input, &texto, GUARDRAIL_TASK, &coletor);
        hits.extend(coletor.into_entradas());

        match resultado {
            GuardrailCheckResult::Allowed => None,
            GuardrailCheckResult::Redacted(mascarado) => {
                self.messages[indice] = Message::user(mascarado);
                None
            }
            GuardrailCheckResult::Blocked(regra_id) => {
                self.messages.push(Message::assistant(format!(
                    "[guardrail] mensagem bloqueada pela regra '{regra_id}' antes de chegar ao provider."
                )));
                Some(SessionOutcome {
                    reason: StopReason::Done,
                    usage: Usage::default(),
                    turns: 0,
                    reviews: Vec::new(),
                    guardrail_hits: Vec::new(),
                })
            }
        }
    }

    /// Aplica o Guardrail Gate (ADR-0007 §1) sobre a última mensagem
    /// (resposta do turno) — **antes** do Reviewer (ADR-0015): não faz
    /// sentido auditar semanticamente um conteúdo que acabou de ser
    /// substituído (MT-45). `Blocked` substitui a resposta pelo aviso fixo
    /// e devolve `ControlFlow::Break` (nunca chega a chamar
    /// `revisar_ou_continuar`); `Redacted` mascara a mensagem em
    /// `self.messages` e devolve `ControlFlow::Continue` — o Reviewer, se
    /// habilitado, roda em cima do texto já mascarado.
    ///
    /// **Limitação conhecida:** em `run_streaming`, o texto já foi entregue
    /// a `on_event` (e, tipicamente, exibido ao usuário em tempo real)
    /// turno a turno, *antes* de chegar aqui — um bloqueio/redação de saída
    /// só protege o histórico (`self.messages`) e qualquer turno seguinte
    /// (Reviewer, próxima chamada ao provider), não o que já foi
    /// transmitido ao vivo. Corrigir isso exigiria *buffer* da resposta
    /// inteira antes de emitir qualquer evento, o que desfaria o propósito
    /// de streaming — fora do escopo deste ticket.
    fn aplicar_guardrail_saida(
        &mut self,
        mut outcome: SessionOutcome,
        hits: &mut Vec<GuardrailAuditEntry>,
    ) -> ControlFlow<SessionOutcome, SessionOutcome> {
        let Some((gate, sink)) = self.guardrails.clone() else {
            return ControlFlow::Continue(outcome);
        };
        let Some(indice) = self
            .messages
            .iter()
            .rposition(|m| m.role == Role::Assistant)
        else {
            return ControlFlow::Continue(outcome);
        };
        let texto = self.messages[indice].text_content();

        let coletor = ColetorDuplo::new(sink.as_ref());
        let resultado = gate.check(GuardrailDirection::Output, &texto, GUARDRAIL_TASK, &coletor);
        hits.extend(coletor.into_entradas());

        match resultado {
            GuardrailCheckResult::Allowed => ControlFlow::Continue(outcome),
            GuardrailCheckResult::Redacted(mascarado) => {
                self.messages[indice] = Message::assistant(mascarado);
                ControlFlow::Continue(outcome)
            }
            GuardrailCheckResult::Blocked(regra_id) => {
                self.messages[indice] = Message::assistant(format!(
                    "[guardrail] resposta bloqueada pela regra '{regra_id}'."
                ));
                outcome.reviews = Vec::new();
                ControlFlow::Break(outcome)
            }
        }
    }

    /// Depois que [`Self::after_response`] sinaliza [`StopReason::Done`],
    /// roda as auditorias habilitadas (MT-35, ADR-0015) e decide se o loop
    /// deve parar (`Break`, com os vereditos anexados a `outcome.reviews`)
    /// ou continuar por mais um turno (`Continue`, com uma observação
    /// corretiva no histórico) — só quando um veredito `Fail` em modo
    /// [`ReviewMode::Blocking`] ainda tem retentativa disponível.
    /// `self.reviews` vazio devolve `Break` imediatamente, sem tocar
    /// `router` — nenhuma auditoria roda se não habilitada.
    async fn revisar_ou_continuar(
        &mut self,
        mut outcome: SessionOutcome,
        router: &Router,
        tentativas: &mut u32,
    ) -> Result<ControlFlow<SessionOutcome>, SessionError> {
        if self.reviews.is_empty() {
            return Ok(ControlFlow::Break(outcome));
        }

        let artefato = self
            .messages
            .last()
            .map(Message::text_content)
            .unwrap_or_default();
        let instrucao_original = self
            .messages
            .iter()
            .find(|m| m.role == Role::User)
            .map(Message::text_content)
            .unwrap_or_default();

        let mut resultados = Vec::with_capacity(self.reviews.len());
        for config in self.reviews.clone() {
            let resultado = reviewer::review(config.kind, router, &instrucao_original, &artefato)
                .await
                .map_err(SessionError::Reviewer)?;
            resultados.push(resultado);
        }

        let reprovacao_bloqueante =
            self.reviews
                .iter()
                .zip(&resultados)
                .any(|(config, resultado)| {
                    config.mode == ReviewMode::Blocking && resultado.veredito == Veredito::Fail
                });

        outcome.reviews = resultados.clone();

        if !reprovacao_bloqueante || *tentativas >= self.review_retry_limit {
            return Ok(ControlFlow::Break(outcome));
        }

        *tentativas += 1;
        let notas = resultados
            .iter()
            .filter(|r| r.veredito == Veredito::Fail)
            .map(|r| r.notas.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        self.messages.push(Message::user(format!(
            "A revisão automática reprovou o resultado anterior. Ajuste considerando: {notas}"
        )));

        Ok(ControlFlow::Continue(()))
    }

    /// Roda o loop (não-streaming) até obter uma resposta final ou estourar
    /// o orçamento de tokens. `router` é usado só se houver auditorias do
    /// Reviewer habilitadas (MT-35, ADR-0015) — resolve a `task-class` de
    /// cada uma, diferente da `task-class` principal já resolvida na
    /// construção da sessão.
    ///
    /// # Errors
    ///
    /// Devolve [`SessionError::Provider`] se o provider falhar em qualquer
    /// turno; [`SessionError::Reviewer`] se uma auditoria habilitada falhar.
    pub async fn run(&mut self, router: &Router) -> Result<SessionOutcome, SessionError> {
        let mut consumed = Usage::default();
        let mut turns = 0u32;
        let mut tentativas_de_revisao = 0u32;
        let mut guardrail_hits = Vec::new();

        if let Some(mut outcome) = self.aplicar_guardrail_entrada(&mut guardrail_hits) {
            outcome.guardrail_hits = guardrail_hits;
            return Ok(outcome);
        }

        loop {
            turns += 1;
            let request = self.build_request();
            let response = self
                .provider
                .chat(request)
                .await
                .map_err(SessionError::Provider)?;
            // `run` não é *streaming* -- não há `on_event` pra emitir
            // `StreamEvent::ToolCallResult`, então os resultados executados
            // são descartados aqui (mesma decisão de escopo do MT-107: sem
            // paridade de exibição de tool fora da TUI).
            let Some(mut outcome) = self
                .after_response(
                    response.message,
                    response.usage,
                    &mut consumed,
                    turns,
                    &mut Vec::new(),
                )
                .await
            else {
                continue;
            };

            if outcome.reason != StopReason::Done {
                outcome.guardrail_hits = guardrail_hits;
                return Ok(outcome);
            }

            outcome = match self.aplicar_guardrail_saida(outcome, &mut guardrail_hits) {
                ControlFlow::Break(mut outcome_final) => {
                    outcome_final.guardrail_hits = guardrail_hits;
                    return Ok(outcome_final);
                }
                ControlFlow::Continue(outcome) => outcome,
            };

            match self
                .revisar_ou_continuar(outcome, router, &mut tentativas_de_revisao)
                .await?
            {
                ControlFlow::Break(mut outcome_final) => {
                    outcome_final.guardrail_hits = guardrail_hits;
                    return Ok(outcome_final);
                }
                ControlFlow::Continue(()) => continue,
            }
        }
    }

    /// Roda o loop com streaming: `on_event` é chamado para cada
    /// [`StreamEvent`] recebido em cada turno (ex.: para exibir texto
    /// incrementalmente numa CLI), e os eventos são agregados na mensagem
    /// final do turno antes de decidir tool-calls/orçamento, igual a
    /// [`Self::run`]. `router` tem o mesmo papel de [`Self::run`] — só usado
    /// se houver auditorias do Reviewer habilitadas (MT-35, ADR-0015).
    ///
    /// **Buffer condicional (MT-47, ADR-0007):** sem nenhuma regra em
    /// `guardrails.output`, `on_event` recebe cada evento em tempo real,
    /// exatamente como antes — sem essa condição, o streaming continua
    /// 100% ao vivo. Com ao menos uma regra em `guardrails.output`, os
    /// eventos de **cada turno** deixam de ser repassados conforme chegam:
    /// são acumulados via [`StreamAggregator`] (como já acontecia) e também
    /// guardados em ordem; só depois de decidido o desfecho do turno é que
    /// `on_event` é chamado. Num turno com tool-calls (não é a resposta
    /// final), os eventos originais são repassados em lote no fim do turno
    /// — nenhuma checagem de saída se aplica a eles (o Guardrail Gate só
    /// audita a resposta final, mesma disciplina do MT-45). No turno que
    /// encerra com [`StopReason::Done`], depois de
    /// [`Self::aplicar_guardrail_saida`] decidir Allowed/Redacted/Blocked,
    /// `on_event` recebe eventos **sintéticos** ([`emitir_texto_como_eventos`])
    /// com o texto já resolvido — nunca os eventos brutos originais, que no
    /// caso `Redacted`/`Blocked` ainda carregam o texto sem máscara.
    ///
    /// # Errors
    ///
    /// Devolve [`SessionError::Provider`] se o provider falhar em qualquer
    /// turno; [`SessionError::Reviewer`] se uma auditoria habilitada falhar.
    pub async fn run_streaming<F>(
        &mut self,
        mut on_event: F,
        router: &Router,
    ) -> Result<SessionOutcome, SessionError>
    where
        F: FnMut(&StreamEvent),
    {
        let mut consumed = Usage::default();
        let mut turns = 0u32;
        let mut tentativas_de_revisao = 0u32;
        let mut guardrail_hits = Vec::new();
        let buffer_saida = self
            .guardrails
            .as_ref()
            .is_some_and(|(gate, _)| !gate.output.is_empty());

        if let Some(mut outcome) = self.aplicar_guardrail_entrada(&mut guardrail_hits) {
            outcome.guardrail_hits = guardrail_hits;
            return Ok(outcome);
        }

        loop {
            turns += 1;
            let request = self.build_request();
            let mut stream = self
                .provider
                .chat_stream(request)
                .await
                .map_err(SessionError::Provider)?;

            let mut aggregator = StreamAggregator::default();
            let mut eventos_do_turno = Vec::new();
            while let Some(evento) = stream.recv().await {
                let evento = evento.map_err(SessionError::Provider)?;
                if buffer_saida {
                    eventos_do_turno.push(evento.clone());
                } else {
                    on_event(&evento);
                }
                aggregator.apply(&evento);
            }
            let (message, turn_usage) = aggregator.into_message();

            let mut resultados_de_tools = Vec::new();
            let Some(mut outcome) = self
                .after_response(
                    message,
                    turn_usage,
                    &mut consumed,
                    turns,
                    &mut resultados_de_tools,
                )
                .await
            else {
                // Turno com tool-calls, não é a resposta final — nenhuma
                // checagem de saída se aplica; repassa os eventos originais
                // em lote (mesmo conteúdo do modo ao vivo, só atrasado até o
                // fim do turno), depois o resultado de cada tool já
                // executada (ADR-0035/MT-114) -- sempre depois do
                // `ToolCallStart`/`ToolCallDelta` correspondente, nunca
                // antes.
                for evento in &eventos_do_turno {
                    on_event(evento);
                }
                for resultado in &resultados_de_tools {
                    on_event(&StreamEvent::ToolCallResult {
                        id: resultado.call_id.clone(),
                        content: resultado.content.clone(),
                        is_error: resultado.is_error,
                    });
                }
                continue;
            };

            if outcome.reason != StopReason::Done {
                for evento in &eventos_do_turno {
                    on_event(evento);
                }
                outcome.guardrail_hits = guardrail_hits;
                return Ok(outcome);
            }

            outcome = match self.aplicar_guardrail_saida(outcome, &mut guardrail_hits) {
                ControlFlow::Break(mut outcome_final) => {
                    if buffer_saida {
                        let texto = self
                            .messages
                            .last()
                            .map(Message::text_content)
                            .unwrap_or_default();
                        emitir_texto_como_eventos(&mut on_event, &texto, turn_usage);
                    }
                    outcome_final.guardrail_hits = guardrail_hits;
                    return Ok(outcome_final);
                }
                ControlFlow::Continue(outcome) => {
                    // Sem regra de saída, os eventos já foram repassados ao
                    // vivo durante a leitura do stream, acima.
                    if buffer_saida {
                        let texto = self
                            .messages
                            .last()
                            .map(Message::text_content)
                            .unwrap_or_default();
                        emitir_texto_como_eventos(&mut on_event, &texto, turn_usage);
                    }
                    outcome
                }
            };

            match self
                .revisar_ou_continuar(outcome, router, &mut tentativas_de_revisao)
                .await?
            {
                ControlFlow::Break(mut outcome_final) => {
                    outcome_final.guardrail_hits = guardrail_hits;
                    return Ok(outcome_final);
                }
                ControlFlow::Continue(()) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardrail::GuardrailAction;
    use crate::provider::mock::MockProvider;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Executor de teste: sempre devolve `"ok"` e conta quantas vezes rodou.
    #[derive(Default)]
    struct CountingExecutor {
        chamadas: AtomicUsize,
    }

    impl ToolExecutor for CountingExecutor {
        fn execute(&self, call: &ToolCall) -> BoxFuture<'_, ToolResult> {
            self.chamadas.fetch_add(1, Ordering::SeqCst);
            let call_id = call.id.clone();
            Box::pin(async move {
                ToolResult {
                    call_id,
                    content: "ok".into(),
                    is_error: false,
                }
            })
        }
    }

    fn resposta_com_tool_call(id: &str, nome: &str, usage: Usage) -> crate::provider::ChatResponse {
        crate::provider::ChatResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: id.into(),
                    name: nome.into(),
                    arguments: serde_json::json!({}),
                })],
            },
            usage,
        }
    }

    fn resposta_final(texto: &str, usage: Usage) -> crate::provider::ChatResponse {
        crate::provider::ChatResponse {
            message: Message::assistant(texto),
            usage,
        }
    }

    /// Rota resolvida de teste, com preset padrão (sem `temperature`/`system_prompt`/etc.).
    fn route(provider: Arc<dyn LlmProvider>) -> ResolvedRoute {
        ResolvedRoute::new(provider, "modelo-x", CallPreset::default())
    }

    /// Router de teste sem nenhuma rota registrada — usado por testes cujo
    /// `Session` não tem nenhuma auditoria do Reviewer habilitada (MT-35):
    /// `revisar_ou_continuar` nunca toca `router` quando `self.reviews` está
    /// vazio, então um Router "vazio" é seguro mesmo passado para `run`/
    /// `run_streaming`.
    fn router_vazio() -> Router {
        use crate::config::privacy::EgressClass;
        Router::new(EgressClass::LocalOnly)
    }

    #[tokio::test]
    async fn ciclo_completo_de_tool_call_termina_com_resposta_final() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_com_tool_call(
            "call-1",
            "fs_read",
            Usage {
                input_tokens: 5,
                output_tokens: 3,
            },
        )));
        mock.enqueue_chat(Ok(resposta_final(
            "pronto!",
            Usage {
                input_tokens: 8,
                output_tokens: 2,
            },
        )));
        let executor = Arc::new(CountingExecutor::default());

        let mut session = Session::new(
            route(mock.clone()),
            executor.clone(),
            TokenBudget::new(1000),
        );
        session.push_user_message("leia o arquivo");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("loop deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(outcome.turns, 2);
        assert_eq!(outcome.usage.total(), 18);
        assert_eq!(executor.chamadas.load(Ordering::SeqCst), 1);

        let historico = session.messages();
        assert_eq!(
            historico.len(),
            4,
            "user, assistant(tool_call), tool, assistant(final)"
        );
        assert_eq!(historico[0].role, Role::User);
        assert_eq!(historico[2].role, Role::Tool);
        assert_eq!(
            historico[2].content,
            vec![ContentBlock::ToolResult(ToolResult {
                call_id: "call-1".into(),
                content: "ok".into(),
                is_error: false,
            })]
        );
        assert_eq!(historico[3], Message::assistant("pronto!"));

        // O segundo turno deve ter enviado o histórico com a observação da tool.
        let requisicoes = mock.chat_requests();
        assert_eq!(requisicoes.len(), 2);
        assert_eq!(requisicoes[1].messages.len(), 3);
    }

    #[tokio::test]
    async fn multiplas_tool_calls_no_mesmo_turno_executam_todas() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(crate::provider::ChatResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::ToolCall(ToolCall {
                        id: "call-1".into(),
                        name: "fs_read".into(),
                        arguments: serde_json::json!({}),
                    }),
                    ContentBlock::ToolCall(ToolCall {
                        id: "call-2".into(),
                        name: "fs_write".into(),
                        arguments: serde_json::json!({}),
                    }),
                ],
            },
            usage: Usage::default(),
        }));
        mock.enqueue_chat(Ok(resposta_final("feito", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());

        let mut session = Session::new(route(mock), executor.clone(), TokenBudget::new(1000));
        session.push_user_message("faça duas coisas");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("loop deve completar");
        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(executor.chamadas.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn encerra_no_orcamento_antes_de_executar_tool_pendente() {
        let mock = Arc::new(MockProvider::new("mock"));
        // Só uma resposta enfileirada: se o loop tentasse rodar de novo sem
        // parar no orçamento, receberia erro de fila vazia (e o teste falharia).
        mock.enqueue_chat(Ok(resposta_com_tool_call(
            "call-1",
            "fs_read",
            Usage {
                input_tokens: 50,
                output_tokens: 50,
            },
        )));
        let executor = Arc::new(CountingExecutor::default());

        let mut session =
            Session::new(route(mock.clone()), executor.clone(), TokenBudget::new(100));
        session.push_user_message("tarefa longa");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("loop deve encerrar no orçamento");

        assert_eq!(outcome.reason, StopReason::BudgetExceeded);
        assert_eq!(outcome.turns, 1);
        assert_eq!(
            executor.chamadas.load(Ordering::SeqCst),
            0,
            "tool pendente não deve ser executada após estourar o orçamento"
        );
        assert_eq!(mock.chat_requests().len(), 1);
    }

    // --- MT-101/ADR-0033: teto de turnos consecutivos com tool-call,
    // independente do orçamento de tokens ---

    #[tokio::test]
    async fn para_no_teto_de_turnos_sem_executar_a_rodada_que_estourou() {
        let mock = Arc::new(MockProvider::new("mock"));
        // 3 respostas com tool-call enfileiradas, mas o teto é 2 — se o
        // loop tentasse rodar uma terceira vez, receberia erro de fila
        // vazia (MockProvider só tem 3) ou, pior, executaria a tool da
        // rodada que já deveria ter parado.
        for i in 0..3 {
            mock.enqueue_chat(Ok(resposta_com_tool_call(
                &format!("call-{i}"),
                "fs_read",
                Usage::default(),
            )));
        }
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            route(mock.clone()),
            executor.clone(),
            TokenBudget::new(1_000_000),
        )
        .with_max_tool_turns(2);
        session.push_user_message("tarefa");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("loop deve parar no teto, não é erro");

        assert_eq!(outcome.reason, StopReason::MaxTurnsExceeded);
        assert_eq!(outcome.turns, 2);
        assert_eq!(
            executor.chamadas.load(Ordering::SeqCst),
            1,
            "só a tool-call do turno 1 deve ter sido executada; a do turno 2 (que estourou) não"
        );
        assert_eq!(
            mock.chat_requests().len(),
            2,
            "não deve chamar o provider uma terceira vez depois de parar no teto"
        );
    }

    #[tokio::test]
    async fn sessao_abaixo_do_teto_de_turnos_termina_normalmente_em_done() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_com_tool_call(
            "call-1",
            "fs_read",
            Usage::default(),
        )));
        mock.enqueue_chat(Ok(resposta_final("pronto", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            route(mock.clone()),
            executor.clone(),
            TokenBudget::new(1_000_000),
        )
        .with_max_tool_turns(25);
        session.push_user_message("tarefa curta");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("loop deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(outcome.turns, 2);
        assert_eq!(executor.chamadas.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn contador_de_turnos_reseta_a_cada_nova_mensagem_de_usuario() {
        let mock = Arc::new(MockProvider::new("mock"));
        // Primeira mensagem: 1 turno com tool-call + 1 turno final (2 no total).
        mock.enqueue_chat(Ok(resposta_com_tool_call(
            "call-1",
            "fs_read",
            Usage::default(),
        )));
        mock.enqueue_chat(Ok(resposta_final("primeira resposta", Usage::default())));
        // Segunda mensagem: se o contador não resetasse, turns começaria em
        // 2 (herdado da primeira chamada a run()) e um teto de 2 pararia
        // essa segunda mensagem imediatamente, sem completar.
        mock.enqueue_chat(Ok(resposta_final("segunda resposta", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            route(mock.clone()),
            executor.clone(),
            TokenBudget::new(1_000_000),
        )
        .with_max_tool_turns(2);

        session.push_user_message("primeira tarefa");
        let primeiro_outcome = session
            .run(&router_vazio())
            .await
            .expect("primeira mensagem deve completar");
        assert_eq!(primeiro_outcome.reason, StopReason::Done);
        assert_eq!(primeiro_outcome.turns, 2);

        session.push_user_message("segunda tarefa");
        let segundo_outcome = session
            .run(&router_vazio())
            .await
            .expect("segunda mensagem deve completar, contador de turnos resetado");
        assert_eq!(segundo_outcome.reason, StopReason::Done);
        assert_eq!(
            segundo_outcome.turns, 1,
            "contador de turnos deve recomeçar do zero para a nova mensagem"
        );
    }

    #[tokio::test]
    async fn run_streaming_agrega_eventos_e_completa_ciclo_de_tool_call() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::ToolCallStart {
                id: "call-1".into(),
                name: "fs_read".into(),
            },
            StreamEvent::ToolCallDelta {
                id: "call-1".into(),
                delta: "{\"path\":\"a.txt\"}".into(),
            },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 4,
                    output_tokens: 6,
                },
            },
        ]);
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta { text: "ol".into() },
            StreamEvent::TextDelta { text: "á!".into() },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 2,
                    output_tokens: 2,
                },
            },
        ]);
        let executor = Arc::new(CountingExecutor::default());

        let mut session = Session::new(route(mock), executor.clone(), TokenBudget::new(1000));
        session.push_user_message("leia a.txt");

        let mut eventos_recebidos = Vec::new();
        let outcome = session
            .run_streaming(
                |evento| eventos_recebidos.push(evento.clone()),
                &router_vazio(),
            )
            .await
            .expect("loop de streaming deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(outcome.usage.total(), 14);
        assert_eq!(executor.chamadas.load(Ordering::SeqCst), 1);
        assert_eq!(
            eventos_recebidos.len(),
            9,
            "4 eventos do 1o turno + ToolCallResult (ADR-0035/MT-114) + 4 do 2o turno"
        );
        assert_eq!(
            eventos_recebidos[4],
            StreamEvent::ToolCallResult {
                id: "call-1".into(),
                content: "ok".into(),
                is_error: false,
            },
            "resultado da tool vem logo depois dos eventos do turno que a pediu"
        );

        let historico = session.messages();
        assert_eq!(historico[1].role, Role::Assistant);
        assert_eq!(
            historico[1].content,
            vec![ContentBlock::ToolCall(ToolCall {
                id: "call-1".into(),
                name: "fs_read".into(),
                arguments: serde_json::json!({"path": "a.txt"}),
            })]
        );
        assert_eq!(historico[3], Message::assistant("olá!"));
    }

    #[tokio::test]
    async fn erro_do_provider_e_propagado() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Err(ProviderError::Network("fora do ar".into())));
        let executor = Arc::new(CountingExecutor::default());

        let mut session = Session::new(route(mock), executor, TokenBudget::new(1000));
        session.push_user_message("oi");

        let erro = session
            .run(&router_vazio())
            .await
            .expect_err("erro do provider deve propagar");
        assert_eq!(
            erro,
            SessionError::Provider(ProviderError::Network("fora do ar".into()))
        );
    }

    /// Router de teste com uma `task-class` de auditoria registrada para
    /// `mock_revisor` (MT-35) — separado do provider principal da sessão,
    /// para deixar claro que a auditoria pode rodar num provider/modelo
    /// diferente da tarefa original (ADR-0015).
    fn router_com_review(mock_revisor: Arc<MockProvider>, task_class: &str) -> Router {
        use crate::config::privacy::EgressClass;
        use crate::router::{RouteEntry, RouteTarget};

        let mut router = Router::new(EgressClass::LocalOnly);
        router.register_provider(mock_revisor);
        router.set_route(
            task_class,
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "mock-revisor",
                    "modelo-revisor",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );
        router
    }

    fn resposta_com_veredito(verdict: &str, notes: &str) -> crate::provider::ChatResponse {
        crate::provider::ChatResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: "call-review".into(),
                    name: "submit_review".into(),
                    arguments: serde_json::json!({ "verdict": verdict, "notes": notes }),
                })],
            },
            usage: Usage::default(),
        }
    }

    #[tokio::test]
    async fn advisory_com_veredito_fail_nao_bloqueia_a_resposta() {
        let mock_principal = Arc::new(MockProvider::new("mock-principal"));
        mock_principal.enqueue_chat(Ok(resposta_final("resultado final", Usage::default())));
        let mock_revisor = Arc::new(MockProvider::new("mock-revisor"));
        mock_revisor.enqueue_chat(Ok(resposta_com_veredito("fail", "tem um bug")));
        let router = router_com_review(mock_revisor, "review-security");

        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            route(mock_principal.clone()),
            executor,
            TokenBudget::new(10_000),
        )
        .with_reviews(
            vec![ReviewConfig {
                kind: AuditKind::Security,
                mode: ReviewMode::Advisory,
            }],
            0,
        );
        session.push_user_message("implemente a função soma");

        let outcome = session.run(&router).await.expect("loop deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(outcome.reviews.len(), 1);
        assert_eq!(outcome.reviews[0].veredito, Veredito::Fail);
        assert_eq!(outcome.reviews[0].notas, "tem um bug");
        assert_eq!(
            mock_principal.chat_requests().len(),
            1,
            "advisory nunca dispara turno corretivo"
        );
    }

    #[tokio::test]
    async fn blocking_reprovado_dispara_retry_ate_o_teto_e_desiste() {
        let mock_principal = Arc::new(MockProvider::new("mock-principal"));
        let mock_revisor = Arc::new(MockProvider::new("mock-revisor"));
        for _ in 0..3 {
            mock_principal.enqueue_chat(Ok(resposta_final("resultado", Usage::default())));
            mock_revisor.enqueue_chat(Ok(resposta_com_veredito("fail", "ainda com bug")));
        }
        let router = router_com_review(mock_revisor, "review-correctness");

        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            route(mock_principal.clone()),
            executor,
            TokenBudget::new(10_000),
        )
        .with_reviews(
            vec![ReviewConfig {
                kind: AuditKind::Correctness,
                mode: ReviewMode::Blocking,
            }],
            2, // teto: 2 retentativas além da primeira tentativa
        );
        session.push_user_message("implemente a função soma");

        let outcome = session.run(&router).await.expect("loop deve completar");

        assert_eq!(
            outcome.reason,
            StopReason::Done,
            "a falha persistente não impede o loop de terminar"
        );
        assert_eq!(outcome.turns, 3, "1 tentativa inicial + 2 retentativas");
        assert_eq!(
            mock_principal.chat_requests().len(),
            3,
            "cada retentativa gera um novo turno da tarefa principal"
        );
        assert_eq!(outcome.reviews.len(), 1);
        assert_eq!(
            outcome.reviews[0].veredito,
            Veredito::Fail,
            "falha persistente após esgotar o teto é exposta, nunca suprimida"
        );
    }

    #[tokio::test]
    async fn blocking_aprovado_na_primeira_tentativa_nao_gera_retry() {
        let mock_principal = Arc::new(MockProvider::new("mock-principal"));
        mock_principal.enqueue_chat(Ok(resposta_final("resultado", Usage::default())));
        let mock_revisor = Arc::new(MockProvider::new("mock-revisor"));
        mock_revisor.enqueue_chat(Ok(resposta_com_veredito("pass", "tudo certo")));
        let router = router_com_review(mock_revisor, "review-security");

        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            route(mock_principal.clone()),
            executor,
            TokenBudget::new(10_000),
        )
        .with_reviews(
            vec![ReviewConfig {
                kind: AuditKind::Security,
                mode: ReviewMode::Blocking,
            }],
            5,
        );
        session.push_user_message("implemente a função soma");

        let outcome = session.run(&router).await.expect("loop deve completar");

        assert_eq!(outcome.turns, 1);
        assert_eq!(mock_principal.chat_requests().len(), 1);
        assert_eq!(outcome.reviews[0].veredito, Veredito::Pass);
    }

    #[tokio::test]
    async fn nenhuma_auditoria_habilitada_nao_chama_o_reviewer() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("resultado", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        // Sem with_reviews — reviews fica vazio (default). Um router sem
        // nenhuma rota registrada (nem "review-*") provaria, se o Reviewer
        // fosse chamado por engano, um SessionError::Reviewer(Router(_)).
        let router = router_vazio();

        let mut session = Session::new(route(mock), executor, TokenBudget::new(10_000));
        session.push_user_message("implemente a função soma");

        let outcome = session.run(&router).await.expect("loop deve completar");

        assert!(outcome.reviews.is_empty());
    }

    #[tokio::test]
    async fn preset_de_task_class_chega_ao_chat_request() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("ok", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let preset = CallPreset {
            temperature: Some(0.3),
            top_p: Some(0.8),
            system_prompt: Some("Você é útil.".into()),
            max_tokens: Some(512),
            reasoning: Some(true),
        };
        let route = ResolvedRoute::new(mock.clone(), "modelo-x", preset);
        let mut session = Session::new(route, executor, TokenBudget::new(1000));
        session.push_user_message("oi");

        session
            .run(&router_vazio())
            .await
            .expect("loop deve completar");

        let requisicoes = mock.chat_requests();
        assert_eq!(requisicoes.len(), 1);
        let req = &requisicoes[0];
        assert_eq!(req.temperature, Some(0.3));
        assert_eq!(req.top_p, Some(0.8));
        assert_eq!(req.max_tokens, Some(512));
        assert_eq!(req.reasoning, Some(true));
        assert_eq!(req.messages[0], Message::system("Você é útil."));
    }

    #[tokio::test]
    async fn system_prompt_nao_duplica_entre_chamadas_a_run() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("primeira resposta", Usage::default())));
        mock.enqueue_chat(Ok(resposta_final("segunda resposta", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let preset = CallPreset {
            system_prompt: Some("Instrução fixa.".into()),
            ..CallPreset::default()
        };
        let mut session = Session::new(
            ResolvedRoute::new(mock, "modelo-x", preset),
            executor,
            TokenBudget::new(10_000),
        );

        session.push_user_message("primeira pergunta");
        session
            .run(&router_vazio())
            .await
            .expect("primeiro turno deve completar");

        session.push_user_message("segunda pergunta");
        session
            .run(&router_vazio())
            .await
            .expect("segundo turno deve completar");

        let historico = session.messages();
        let mensagens_de_sistema = historico.iter().filter(|m| m.role == Role::System).count();
        assert_eq!(
            mensagens_de_sistema, 1,
            "system_prompt não deve duplicar entre chamadas a run()"
        );
        assert_eq!(historico[0].role, Role::System);
    }

    // --- MT-59: instruções de projeto concatenadas ao system_prompt (ADR-0023) ---

    #[tokio::test]
    async fn instrucoes_de_projeto_e_system_prompt_do_preset_coexistem_numa_unica_mensagem() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("ok", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let preset = CallPreset {
            system_prompt: Some("Instrução da task-class.".into()),
            ..CallPreset::default()
        };
        let mut session = Session::new(
            ResolvedRoute::new(mock.clone(), "modelo-x", preset),
            executor,
            TokenBudget::new(10_000),
        )
        .with_project_instructions("Regras do projeto (AGENTS.md).");
        session.push_user_message("oi");

        session.run(&router_vazio()).await.expect("deve completar");

        let requisicoes = mock.chat_requests();
        assert_eq!(
            requisicoes[0].messages[0],
            Message::system("Regras do projeto (AGENTS.md).\n\nInstrução da task-class."),
            "instruções de projeto vêm primeiro, task-class depois, numa única mensagem"
        );
        let mensagens_de_sistema = session
            .messages()
            .iter()
            .filter(|m| m.role == Role::System)
            .count();
        assert_eq!(mensagens_de_sistema, 1);
    }

    #[tokio::test]
    async fn so_instrucoes_de_projeto_sem_preset_system_prompt_ainda_vira_mensagem_de_sistema() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("ok", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            ResolvedRoute::new(mock.clone(), "modelo-x", CallPreset::default()),
            executor,
            TokenBudget::new(10_000),
        )
        .with_project_instructions("Regras do projeto.");
        session.push_user_message("oi");

        session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(
            mock.chat_requests()[0].messages[0],
            Message::system("Regras do projeto.")
        );
    }

    #[tokio::test]
    async fn sem_instrucoes_de_projeto_nem_preset_nenhuma_mensagem_de_sistema_e_inserida() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("ok", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            ResolvedRoute::new(mock.clone(), "modelo-x", CallPreset::default()),
            executor,
            TokenBudget::new(10_000),
        );
        session.push_user_message("oi");

        session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(mock.chat_requests()[0].messages[0].role, Role::User);
    }

    // --- MT-60: lista de skills concatenada por último (ADR-0023) ---

    #[tokio::test]
    async fn lista_de_skills_e_concatenada_por_ultimo_apos_projeto_e_preset() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("ok", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let preset = CallPreset {
            system_prompt: Some("Instrução da task-class.".into()),
            ..CallPreset::default()
        };
        let mut session = Session::new(
            ResolvedRoute::new(mock.clone(), "modelo-x", preset),
            executor,
            TokenBudget::new(10_000),
        )
        .with_project_instructions("Regras do projeto.")
        .with_skills_list("- adr-writer: cria ADRs.");
        session.push_user_message("oi");

        session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(
            mock.chat_requests()[0].messages[0],
            Message::system(
                "Regras do projeto.\n\nInstrução da task-class.\n\n- adr-writer: cria ADRs."
            )
        );
    }

    #[tokio::test]
    async fn memoria_e_concatenada_logo_apos_instrucoes_de_projeto_antes_do_preset() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("ok", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let preset = CallPreset {
            system_prompt: Some("Instrução da task-class.".into()),
            ..CallPreset::default()
        };
        let mut session = Session::new(
            ResolvedRoute::new(mock.clone(), "modelo-x", preset),
            executor,
            TokenBudget::new(10_000),
        )
        .with_project_instructions("Regras do projeto.")
        .with_memoria("- o usuário prefere respostas em português")
        .with_skills_list("- adr-writer: cria ADRs.");
        session.push_user_message("oi");

        session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(
            mock.chat_requests()[0].messages[0],
            Message::system(
                "Regras do projeto.\n\n- o usuário prefere respostas em português\n\n\
                 Instrução da task-class.\n\n- adr-writer: cria ADRs."
            )
        );
    }

    #[tokio::test]
    async fn sem_memoria_gravada_nenhum_bloco_vazio_e_inserido() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("ok", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            ResolvedRoute::new(mock.clone(), "modelo-x", CallPreset::default()),
            executor,
            TokenBudget::new(10_000),
        )
        .with_project_instructions("Regras do projeto.");
        session.push_user_message("oi");

        session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(
            mock.chat_requests()[0].messages[0],
            Message::system("Regras do projeto."),
            "sem with_memoria, nada extra deve ser concatenado"
        );
    }

    #[tokio::test]
    async fn sem_skills_descobertas_nenhuma_lista_vazia_e_inserida() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("ok", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(
            ResolvedRoute::new(mock.clone(), "modelo-x", CallPreset::default()),
            executor,
            TokenBudget::new(10_000),
        )
        .with_project_instructions("Regras do projeto.");
        session.push_user_message("oi");

        session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(
            mock.chat_requests()[0].messages[0],
            Message::system("Regras do projeto."),
            "sem with_skills_list, nada extra deve ser concatenado"
        );
    }

    #[tokio::test]
    async fn apply_route_troca_modelo_e_preset_preservando_historico() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("primeira resposta", Usage::default())));
        mock.enqueue_chat(Ok(resposta_final("segunda resposta", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());

        let mut session = Session::new(
            ResolvedRoute::new(mock.clone(), "modelo-antigo", CallPreset::default()),
            executor,
            TokenBudget::new(10_000),
        );
        session.push_user_message("primeira pergunta");
        session
            .run(&router_vazio())
            .await
            .expect("primeiro turno deve completar");

        let novo_preset = CallPreset {
            temperature: Some(0.9),
            ..CallPreset::default()
        };
        session.apply_route(ResolvedRoute::new(mock.clone(), "modelo-novo", novo_preset));
        session.push_user_message("segunda pergunta");
        session
            .run(&router_vazio())
            .await
            .expect("segundo turno deve completar");

        // Histórico preservado através da troca de rota.
        assert_eq!(session.messages().len(), 4);
        assert_eq!(session.messages()[0], Message::user("primeira pergunta"));

        let requisicoes = mock.chat_requests();
        assert_eq!(requisicoes[0].model, "modelo-antigo");
        assert_eq!(requisicoes[0].temperature, None);
        assert_eq!(requisicoes[1].model, "modelo-novo");
        assert_eq!(requisicoes[1].temperature, Some(0.9));
    }

    /// Router de teste com a `task-class` `"compact"` já registrada para o
    /// mesmo provider mock (MT-36).
    fn router_com_compact(provider: Arc<MockProvider>) -> Router {
        use crate::config::privacy::EgressClass;
        use crate::router::{RouteEntry, RouteTarget};

        let mut router = Router::new(EgressClass::LocalOnly);
        router.register_provider(provider);
        router.set_route(
            "compact",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "mock",
                    "modelo-compact",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );
        router
    }

    #[tokio::test]
    async fn compact_bem_sucedida_substitui_historico_por_um_unico_resumo() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("primeira resposta", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000));
        session.push_user_message("pergunta original");
        session
            .run(&router_vazio())
            .await
            .expect("turno deve completar");

        mock.enqueue_chat(Ok(resposta_final("resumo da conversa", Usage::default())));
        let router = router_com_compact(mock.clone());
        session
            .compact(&router)
            .await
            .expect("compactação deve funcionar");

        assert_eq!(session.messages().len(), 1);
        assert_eq!(session.messages()[0].role, Role::System);
        assert_eq!(
            session.messages()[0].content,
            vec![ContentBlock::Text {
                text: "resumo da conversa".into()
            }]
        );
    }

    #[tokio::test]
    async fn compact_com_falha_do_provider_preserva_historico_original() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("primeira resposta", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000));
        session.push_user_message("pergunta original");
        session
            .run(&router_vazio())
            .await
            .expect("turno deve completar");
        let historico_antes = session.messages().to_vec();

        // Nenhuma resposta enfileirada para a chamada de compactação: o mock
        // devolve erro "sem resposta enfileirada".
        let router = router_com_compact(mock.clone());
        let erro = session
            .compact(&router)
            .await
            .expect_err("deve falhar sem resposta enfileirada");

        assert!(matches!(erro, SessionError::Provider(_)));
        assert_eq!(session.messages(), historico_antes.as_slice());
    }

    #[tokio::test]
    async fn compact_sem_task_class_registrada_e_erro_de_router_sem_chamar_o_provider() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("primeira resposta", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000));
        session.push_user_message("pergunta original");
        session
            .run(&router_vazio())
            .await
            .expect("turno deve completar");
        let historico_antes = session.messages().to_vec();

        let router = Router::new(crate::config::privacy::EgressClass::LocalOnly); // sem rota "compact"
        let erro = session
            .compact(&router)
            .await
            .expect_err("task-class desconhecida deve falhar");

        assert!(matches!(erro, SessionError::Router(_)));
        assert_eq!(session.messages(), historico_antes.as_slice());
        assert_eq!(
            mock.chat_requests().len(),
            1,
            "nenhuma chamada de compactação deveria ter sido feita"
        );
    }

    #[tokio::test]
    async fn compact_com_historico_vazio_e_no_op() {
        let mock = Arc::new(MockProvider::new("mock"));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000));

        let router = router_com_compact(mock.clone());
        session
            .compact(&router)
            .await
            .expect("histórico vazio deve ser no-op, não erro");

        assert!(session.messages().is_empty());
        assert_eq!(
            mock.chat_requests().len(),
            0,
            "nenhuma chamada deveria ter sido feita para histórico vazio"
        );
    }

    #[tokio::test]
    async fn sessao_recem_criada_comeca_com_usage_total_zerado() {
        let mock = Arc::new(MockProvider::new("mock"));
        let executor = Arc::new(CountingExecutor::default());
        let session = Session::new(route(mock), executor, TokenBudget::new(10_000));

        assert_eq!(session.usage_total(), Usage::default());
    }

    #[tokio::test]
    async fn um_turno_soma_corretamente_ao_usage_total() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final(
            "resposta",
            Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
        )));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(route(mock), executor, TokenBudget::new(10_000));
        session.push_user_message("pergunta");

        session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(
            session.usage_total(),
            Usage {
                input_tokens: 10,
                output_tokens: 5
            }
        );
    }

    #[tokio::test]
    async fn multiplos_turnos_acumulam_no_usage_total_em_vez_de_sobrescrever() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final(
            "primeira",
            Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
        )));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000));
        session.push_user_message("primeira pergunta");
        session
            .run(&router_vazio())
            .await
            .expect("primeiro turno deve completar");

        mock.enqueue_chat(Ok(resposta_final(
            "segunda",
            Usage {
                input_tokens: 7,
                output_tokens: 3,
            },
        )));
        session.push_user_message("segunda pergunta");
        session
            .run(&router_vazio())
            .await
            .expect("segundo turno deve completar");

        assert_eq!(
            session.usage_total(),
            Usage {
                input_tokens: 17,
                output_tokens: 8
            },
            "os dois turnos devem se somar, nenhum sobrescreve o outro"
        );
    }

    #[tokio::test]
    async fn compact_soma_seu_proprio_uso_e_nunca_zera_o_total_acumulado() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final(
            "primeira resposta",
            Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
        )));
        let executor = Arc::new(CountingExecutor::default());
        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000));
        session.push_user_message("pergunta original");
        session
            .run(&router_vazio())
            .await
            .expect("turno deve completar");
        assert_eq!(
            session.usage_total(),
            Usage {
                input_tokens: 10,
                output_tokens: 5
            }
        );

        mock.enqueue_chat(Ok(resposta_final(
            "resumo da conversa",
            Usage {
                input_tokens: 20,
                output_tokens: 8,
            },
        )));
        let router = router_com_compact(mock.clone());
        session
            .compact(&router)
            .await
            .expect("compactação deve funcionar");

        assert_eq!(
            session.usage_total(),
            Usage {
                input_tokens: 30,
                output_tokens: 13
            },
            "compact soma seu próprio uso ao total, nunca reseta o que já foi consumido"
        );
    }

    /// Coletor de [`GuardrailAuditEntry`] de teste (MT-45) — mesma
    /// disciplina de [`crate::guardrail::tests`] (`Mutex`, não `RefCell`,
    /// porque `GuardrailAuditSink` exige `Send + Sync`).
    #[derive(Default)]
    struct SinkColetorDeTeste(std::sync::Mutex<Vec<GuardrailAuditEntry>>);

    impl GuardrailAuditSink for SinkColetorDeTeste {
        fn record(&self, entry: GuardrailAuditEntry) {
            self.0
                .lock()
                .expect("mutex do coletor não deve envenenar")
                .push(entry);
        }
    }

    #[tokio::test]
    async fn regra_de_entrada_block_nunca_chama_o_provider() {
        let mock = Arc::new(MockProvider::new("mock"));
        // Nenhuma resposta enfileirada de propósito: se o provider fosse
        // chamado, o mock devolveria erro de fila vazia — provando que a
        // chamada nunca aconteceu de fato, não só que o teste não observou.
        let executor = Arc::new(CountingExecutor::default());
        let gate = Arc::new(crate::guardrail::GuardrailGate {
            input: vec![crate::guardrail::GuardrailRule::new(
                "bloqueia-senha",
                "senha:",
                GuardrailAction::Block,
            )],
            output: vec![],
        });
        let sink = Arc::new(SinkColetorDeTeste::default());

        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000))
            .with_guardrails(gate, sink);
        session.push_user_message("minha senha: 12345");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("bloqueio de entrada não deve ser erro");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(outcome.turns, 0);
        assert_eq!(mock.chat_requests().len(), 0, "o provider nunca é chamado");
        assert_eq!(outcome.guardrail_hits.len(), 1);
        assert_eq!(outcome.guardrail_hits[0].action, GuardrailAction::Block);
        assert_eq!(
            outcome.guardrail_hits[0].direction,
            GuardrailDirection::Input
        );
    }

    #[tokio::test]
    async fn regra_de_entrada_redact_chega_ao_provider_mascarada() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final("tudo certo", Usage::default())));
        let executor = Arc::new(CountingExecutor::default());
        let gate = Arc::new(crate::guardrail::GuardrailGate {
            input: vec![crate::guardrail::GuardrailRule::new(
                "mascara-segredo",
                "segredo-abc",
                GuardrailAction::Redact,
            )],
            output: vec![],
        });
        let sink = Arc::new(SinkColetorDeTeste::default());

        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000))
            .with_guardrails(gate, sink);
        session.push_user_message("o valor é segredo-abc, use com cuidado");

        let outcome = session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        let requisicoes = mock.chat_requests();
        assert_eq!(requisicoes.len(), 1);
        let texto_enviado = requisicoes[0].messages[0].text_content();
        assert!(!texto_enviado.contains("segredo-abc"));
        assert!(texto_enviado.contains(crate::egress::redact::REDACTED_PLACEHOLDER));
        assert_eq!(outcome.guardrail_hits.len(), 1);
        assert_eq!(outcome.guardrail_hits[0].action, GuardrailAction::Redact);
    }

    #[tokio::test]
    async fn regra_de_saida_block_substitui_a_resposta_e_pula_o_reviewer_mesmo_com_reviews_habilitadas(
    ) {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final(
            "aqui está internal.corp no meio",
            Usage::default(),
        )));
        let executor = Arc::new(CountingExecutor::default());
        let gate = Arc::new(crate::guardrail::GuardrailGate {
            input: vec![],
            output: vec![crate::guardrail::GuardrailRule::new(
                "bloqueia-host",
                "internal.corp",
                GuardrailAction::Block,
            )],
        });
        let sink = Arc::new(SinkColetorDeTeste::default());

        // router_vazio() não tem nenhuma rota "review-*" registrada — se o
        // Reviewer fosse de fato chamado, resolve() falharia e run()
        // devolveria Err, não Ok.
        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000))
            .with_guardrails(gate, sink)
            .with_reviews(
                vec![ReviewConfig {
                    kind: AuditKind::Security,
                    mode: ReviewMode::Blocking,
                }],
                3,
            );
        session.push_user_message("qual o endereço do servidor?");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("bloqueio de saída não deve ser erro, e o Reviewer nunca deve rodar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert!(
            outcome.reviews.is_empty(),
            "Reviewer nunca roda sobre uma resposta já bloqueada"
        );
        let historico = session.messages();
        let resposta_final_texto = historico.last().unwrap().text_content();
        assert!(!resposta_final_texto.contains("internal.corp"));
        assert!(resposta_final_texto.contains("bloqueia-host"));
        assert_eq!(outcome.guardrail_hits.len(), 1);
        assert_eq!(
            outcome.guardrail_hits[0].direction,
            GuardrailDirection::Output
        );
    }

    #[tokio::test]
    async fn regra_de_saida_redact_mascara_a_resposta_e_o_reviewer_ainda_roda_em_cima_dela() {
        let mock_principal = Arc::new(MockProvider::new("mock-principal"));
        mock_principal.enqueue_chat(Ok(resposta_final(
            "a chave é segredo-xyz, guarde bem",
            Usage::default(),
        )));
        let mock_revisor = Arc::new(MockProvider::new("mock-revisor"));
        mock_revisor.enqueue_chat(Ok(resposta_com_veredito("pass", "sem problemas")));
        let router = router_com_review(mock_revisor.clone(), "review-security");

        let executor = Arc::new(CountingExecutor::default());
        let gate = Arc::new(crate::guardrail::GuardrailGate {
            input: vec![],
            output: vec![crate::guardrail::GuardrailRule::new(
                "mascara-segredo",
                "segredo-xyz",
                GuardrailAction::Redact,
            )],
        });
        let sink = Arc::new(SinkColetorDeTeste::default());

        let mut session = Session::new(
            route(mock_principal.clone()),
            executor,
            TokenBudget::new(10_000),
        )
        .with_guardrails(gate, sink)
        .with_reviews(
            vec![ReviewConfig {
                kind: AuditKind::Security,
                mode: ReviewMode::Advisory,
            }],
            0,
        );
        session.push_user_message("me dê uma chave de teste");

        let outcome = session.run(&router).await.expect("deve completar");

        assert_eq!(outcome.reviews.len(), 1, "o Reviewer roda normalmente");
        assert_eq!(outcome.reviews[0].veredito, Veredito::Pass);

        let historico = session.messages();
        let resposta_final_texto = historico.last().unwrap().text_content();
        assert!(!resposta_final_texto.contains("segredo-xyz"));

        // O Reviewer recebeu o texto já mascarado, não o original.
        let requisicoes_revisor = mock_revisor.chat_requests();
        let texto_para_o_revisor = requisicoes_revisor[0]
            .messages
            .iter()
            .map(Message::text_content)
            .collect::<Vec<_>>()
            .join(" ");
        assert!(!texto_para_o_revisor.contains("segredo-xyz"));
    }

    #[tokio::test]
    async fn sessao_sem_with_guardrails_nunca_aplica_nenhuma_checagem() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_final(
            "resposta com senha: 12345 e tudo mais",
            Usage::default(),
        )));
        let executor = Arc::new(CountingExecutor::default());

        // Sem with_guardrails — nenhum gate configurado.
        let mut session = Session::new(route(mock.clone()), executor, TokenBudget::new(10_000));
        session.push_user_message("minha senha: segredo");

        let outcome = session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert!(outcome.guardrail_hits.is_empty());
        assert_eq!(mock.chat_requests().len(), 1);
        // Mensagens preservadas exatamente como escritas, sem mascarar nada.
        assert_eq!(
            mock.chat_requests()[0].messages[0].text_content(),
            "minha senha: segredo"
        );
        assert_eq!(
            session.messages().last().unwrap().text_content(),
            "resposta com senha: 12345 e tudo mais"
        );
    }

    // --- MT-47: buffer condicional em run_streaming quando há guardrails de saída ---

    #[tokio::test]
    async fn run_streaming_com_guardrail_so_de_entrada_nao_ativa_o_buffer_de_saida() {
        // Regra só em `input` — `gate.output` continua vazio, então o
        // buffer condicional não deve ativar: streaming 100% ao vivo, igual
        // a uma sessão sem nenhum guardrail configurado.
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta { text: "ol".into() },
            StreamEvent::TextDelta { text: "á!".into() },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 2,
                    output_tokens: 2,
                },
            },
        ]);
        let executor = Arc::new(CountingExecutor::default());
        let gate = Arc::new(crate::guardrail::GuardrailGate {
            input: vec![crate::guardrail::GuardrailRule::new(
                "bloqueia-senha",
                "senha:",
                GuardrailAction::Block,
            )],
            output: vec![],
        });
        let sink = Arc::new(SinkColetorDeTeste::default());

        let mut session =
            Session::new(route(mock), executor, TokenBudget::new(1000)).with_guardrails(gate, sink);
        session.push_user_message("oi, tudo bem?");

        let mut eventos = Vec::new();
        let outcome = session
            .run_streaming(|evento| eventos.push(evento.clone()), &router_vazio())
            .await
            .expect("loop de streaming deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        // Mesmos 4 eventos brutos do provider, na ordem original — nenhum
        // evento sintético, nenhum atraso de turno inteiro.
        assert_eq!(eventos.len(), 4);
        assert_eq!(eventos[0], StreamEvent::MessageStart);
        assert_eq!(eventos[1], StreamEvent::TextDelta { text: "ol".into() });
        assert_eq!(eventos[2], StreamEvent::TextDelta { text: "á!".into() });
    }

    #[tokio::test]
    async fn run_streaming_com_regra_de_saida_block_nunca_emite_o_texto_original_so_o_aviso_sintetico(
    ) {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta {
                text: "minha senha: 12345".into(),
            },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 3,
                    output_tokens: 5,
                },
            },
        ]);
        let executor = Arc::new(CountingExecutor::default());
        let gate = Arc::new(crate::guardrail::GuardrailGate {
            input: vec![],
            output: vec![crate::guardrail::GuardrailRule::new(
                "bloqueia-senha",
                "senha:",
                GuardrailAction::Block,
            )],
        });
        let sink = Arc::new(SinkColetorDeTeste::default());

        let mut session =
            Session::new(route(mock), executor, TokenBudget::new(1000)).with_guardrails(gate, sink);
        session.push_user_message("qual a senha?");

        let mut eventos = Vec::new();
        let outcome = session
            .run_streaming(|evento| eventos.push(evento.clone()), &router_vazio())
            .await
            .expect("loop de streaming deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(outcome.usage.total(), 8, "usage do turno continua correto");
        assert_eq!(outcome.guardrail_hits.len(), 1);
        assert_eq!(outcome.guardrail_hits[0].action, GuardrailAction::Block);

        // Eventos sintéticos: MessageStart, um TextDelta com o aviso fixo,
        // MessageEnd — nunca o texto original com a senha.
        assert_eq!(eventos.len(), 3);
        assert_eq!(eventos[0], StreamEvent::MessageStart);
        match &eventos[1] {
            StreamEvent::TextDelta { text } => {
                assert!(!text.contains("12345"), "senha original nunca vaza");
                assert!(text.contains("bloqueada"));
            }
            outro => panic!("esperava TextDelta, veio {outro:?}"),
        }
        assert!(matches!(eventos[2], StreamEvent::MessageEnd { .. }));
    }

    #[tokio::test]
    async fn run_streaming_com_regra_de_saida_redact_emite_so_o_texto_mascarado_nunca_o_original() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta {
                text: "o valor é ".into(),
            },
            StreamEvent::TextDelta {
                text: "segredo-abc, guarde bem".into(),
            },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 3,
                    output_tokens: 6,
                },
            },
        ]);
        let executor = Arc::new(CountingExecutor::default());
        let gate = Arc::new(crate::guardrail::GuardrailGate {
            input: vec![],
            output: vec![crate::guardrail::GuardrailRule::new(
                "mascara-segredo",
                "segredo-abc",
                GuardrailAction::Redact,
            )],
        });
        let sink = Arc::new(SinkColetorDeTeste::default());

        let mut session =
            Session::new(route(mock), executor, TokenBudget::new(1000)).with_guardrails(gate, sink);
        session.push_user_message("qual o valor?");

        let mut eventos = Vec::new();
        let outcome = session
            .run_streaming(|evento| eventos.push(evento.clone()), &router_vazio())
            .await
            .expect("loop de streaming deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(outcome.usage.total(), 9, "usage do turno continua correto");
        assert_eq!(outcome.guardrail_hits.len(), 1);
        assert_eq!(outcome.guardrail_hits[0].action, GuardrailAction::Redact);

        assert_eq!(eventos.len(), 3);
        match &eventos[1] {
            StreamEvent::TextDelta { text } => {
                assert!(!text.contains("segredo-abc"), "texto original nunca vaza");
                assert!(text.contains(crate::egress::redact::REDACTED_PLACEHOLDER));
            }
            outro => panic!("esperava TextDelta, veio {outro:?}"),
        }
    }

    #[tokio::test]
    async fn run_streaming_com_guardrail_de_saida_e_tool_call_intermediario_repassa_o_turno_intermediario_em_lote(
    ) {
        // Turno 1 tem tool-call (não é a resposta final — nenhuma checagem
        // de saída se aplica a ele); turno 2 é a resposta final, sem
        // nenhuma regra casando (Allowed).
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::ToolCallStart {
                id: "call-1".into(),
                name: "fs_read".into(),
            },
            StreamEvent::ToolCallDelta {
                id: "call-1".into(),
                delta: "{\"path\":\"a.txt\"}".into(),
            },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 4,
                    output_tokens: 6,
                },
            },
        ]);
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta {
                text: "tudo certo".into(),
            },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 2,
                    output_tokens: 2,
                },
            },
        ]);
        let executor = Arc::new(CountingExecutor::default());
        let gate = Arc::new(crate::guardrail::GuardrailGate {
            input: vec![],
            output: vec![crate::guardrail::GuardrailRule::new(
                "mascara-segredo",
                "segredo-abc",
                GuardrailAction::Redact,
            )],
        });
        let sink = Arc::new(SinkColetorDeTeste::default());

        let mut session = Session::new(route(mock), executor.clone(), TokenBudget::new(1000))
            .with_guardrails(gate, sink);
        session.push_user_message("leia a.txt");

        let mut eventos = Vec::new();
        let outcome = session
            .run_streaming(|evento| eventos.push(evento.clone()), &router_vazio())
            .await
            .expect("loop de streaming deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(executor.chamadas.load(Ordering::SeqCst), 1);
        assert!(outcome.guardrail_hits.is_empty(), "nenhuma regra casou");

        // Turno 1 (tool-call) repassado em lote, exatamente como veio do
        // provider, seguido do resultado da tool (ADR-0035/MT-114); turno 2
        // (final, Allowed) via eventos sintéticos.
        assert_eq!(eventos.len(), 8);
        assert_eq!(eventos[0], StreamEvent::MessageStart);
        assert_eq!(
            eventos[1],
            StreamEvent::ToolCallStart {
                id: "call-1".into(),
                name: "fs_read".into(),
            }
        );
        assert_eq!(
            eventos[4],
            StreamEvent::ToolCallResult {
                id: "call-1".into(),
                content: "ok".into(),
                is_error: false,
            }
        );
        assert_eq!(eventos[5], StreamEvent::MessageStart);
        assert_eq!(
            eventos[6],
            StreamEvent::TextDelta {
                text: "tudo certo".into()
            }
        );
        assert!(matches!(eventos[7], StreamEvent::MessageEnd { .. }));
    }
}
