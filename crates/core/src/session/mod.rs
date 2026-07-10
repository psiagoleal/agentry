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

pub mod reviewer;

use std::collections::HashMap;
use std::sync::Arc;

use crate::model::{ContentBlock, Message, Role, StreamEvent, ToolCall, ToolResult, Usage};
use crate::provider::{BoxFuture, ChatRequest, LlmProvider, ProviderError, ToolSpec};
use crate::router::{CallPreset, ResolvedRoute, Router, RouterError};

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
}

/// Resultado de rodar o loop até encerrar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionOutcome {
    /// Por que o loop parou.
    pub reason: StopReason,
    /// Consumo total de tokens acumulado em todos os turnos.
    pub usage: Usage,
    /// Número de turnos (chamadas ao provider) executados.
    pub turns: u32,
}

/// Erros do agent loop.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionError {
    /// O provider devolveu um erro.
    Provider(ProviderError),
    /// O Router não conseguiu resolver uma `task-class` pedida (ex.: `"compact"`, MT-36).
    Router(RouterError),
}

impl core::fmt::Display for SessionError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Provider(e) => write!(f, "erro do provider: {e}"),
            Self::Router(e) => write!(f, "erro de roteamento: {e}"),
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
        }
    }

    /// Declara as tools oferecidas ao modelo (via [`ChatRequest::tools`]).
    #[must_use]
    pub fn with_tools(mut self, tools: Vec<ToolSpec>) -> Self {
        self.tools = tools;
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

        self.messages = vec![Message::system(resposta.message.text_content())];
        Ok(())
    }

    /// Histórico de mensagens acumulado até aqui.
    #[must_use]
    pub fn messages(&self) -> &[Message] {
        &self.messages
    }

    /// Garante que a mensagem de sistema do preset (se houver) esteja no
    /// início do histórico — insere só uma vez; chamadas seguintes (novos
    /// turnos, ou novas mensagens de usuário) não duplicam.
    fn ensure_system_prompt(&mut self) {
        if let Some(system_prompt) = self.preset.system_prompt.clone() {
            if !self.messages.iter().any(|m| m.role == Role::System) {
                self.messages.insert(0, Message::system(system_prompt));
            }
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
    /// Devolve `Some(outcome)` quando o loop deve parar neste turno.
    async fn after_response(
        &mut self,
        message: Message,
        turn_usage: Usage,
        consumed: &mut Usage,
        turns: u32,
    ) -> Option<SessionOutcome> {
        *consumed = Usage {
            input_tokens: consumed.input_tokens + turn_usage.input_tokens,
            output_tokens: consumed.output_tokens + turn_usage.output_tokens,
        };
        let tool_calls = extract_tool_calls(&message);
        self.messages.push(message);

        if consumed.total() >= self.budget.max_tokens {
            return Some(SessionOutcome {
                reason: StopReason::BudgetExceeded,
                usage: *consumed,
                turns,
            });
        }

        if tool_calls.is_empty() {
            return Some(SessionOutcome {
                reason: StopReason::Done,
                usage: *consumed,
                turns,
            });
        }

        let mut result_blocks = Vec::with_capacity(tool_calls.len());
        for call in &tool_calls {
            let result = self.executor.execute(call).await;
            result_blocks.push(ContentBlock::ToolResult(result));
        }
        self.messages.push(Message {
            role: Role::Tool,
            content: result_blocks,
        });

        None
    }

    /// Roda o loop (não-streaming) até obter uma resposta final ou estourar
    /// o orçamento de tokens.
    ///
    /// # Errors
    ///
    /// Devolve [`SessionError::Provider`] se o provider falhar em qualquer turno.
    pub async fn run(&mut self) -> Result<SessionOutcome, SessionError> {
        let mut consumed = Usage::default();
        let mut turns = 0u32;
        loop {
            turns += 1;
            let request = self.build_request();
            let response = self
                .provider
                .chat(request)
                .await
                .map_err(SessionError::Provider)?;
            if let Some(outcome) = self
                .after_response(response.message, response.usage, &mut consumed, turns)
                .await
            {
                return Ok(outcome);
            }
        }
    }

    /// Roda o loop com streaming: `on_event` é chamado para cada
    /// [`StreamEvent`] recebido em cada turno (ex.: para exibir texto
    /// incrementalmente numa CLI), e os eventos são agregados na mensagem
    /// final do turno antes de decidir tool-calls/orçamento, igual a [`Self::run`].
    ///
    /// # Errors
    ///
    /// Devolve [`SessionError::Provider`] se o provider falhar em qualquer turno.
    pub async fn run_streaming<F>(
        &mut self,
        mut on_event: F,
    ) -> Result<SessionOutcome, SessionError>
    where
        F: FnMut(&StreamEvent),
    {
        let mut consumed = Usage::default();
        let mut turns = 0u32;
        loop {
            turns += 1;
            let request = self.build_request();
            let mut stream = self
                .provider
                .chat_stream(request)
                .await
                .map_err(SessionError::Provider)?;

            let mut aggregator = StreamAggregator::default();
            while let Some(evento) = stream.recv().await {
                let evento = evento.map_err(SessionError::Provider)?;
                on_event(&evento);
                aggregator.apply(&evento);
            }
            let (message, turn_usage) = aggregator.into_message();

            if let Some(outcome) = self
                .after_response(message, turn_usage, &mut consumed, turns)
                .await
            {
                return Ok(outcome);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

        let outcome = session.run().await.expect("loop deve completar");

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

        let outcome = session.run().await.expect("loop deve completar");
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
            .run()
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

        let mut eventos_recebidos = 0usize;
        let outcome = session
            .run_streaming(|_evento| eventos_recebidos += 1)
            .await
            .expect("loop de streaming deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(outcome.usage.total(), 14);
        assert_eq!(executor.chamadas.load(Ordering::SeqCst), 1);
        assert_eq!(eventos_recebidos, 8, "4 eventos por turno, 2 turnos");

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
            .run()
            .await
            .expect_err("erro do provider deve propagar");
        assert_eq!(
            erro,
            SessionError::Provider(ProviderError::Network("fora do ar".into()))
        );
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

        session.run().await.expect("loop deve completar");

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
        session.run().await.expect("primeiro turno deve completar");

        session.push_user_message("segunda pergunta");
        session.run().await.expect("segundo turno deve completar");

        let historico = session.messages();
        let mensagens_de_sistema = historico.iter().filter(|m| m.role == Role::System).count();
        assert_eq!(
            mensagens_de_sistema, 1,
            "system_prompt não deve duplicar entre chamadas a run()"
        );
        assert_eq!(historico[0].role, Role::System);
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
        session.run().await.expect("primeiro turno deve completar");

        let novo_preset = CallPreset {
            temperature: Some(0.9),
            ..CallPreset::default()
        };
        session.apply_route(ResolvedRoute::new(mock.clone(), "modelo-novo", novo_preset));
        session.push_user_message("segunda pergunta");
        session.run().await.expect("segundo turno deve completar");

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
        session.run().await.expect("turno deve completar");

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
        session.run().await.expect("turno deve completar");
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
        session.run().await.expect("turno deve completar");
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
}
