// Caminho relativo: crates/cli/src/repl.rs
//! REPL interativo (MT-14): lê linhas, trata comandos `/model`,
//! `/temperature`, `/top_p`, `/system`, `/max_tokens`, `/reasoning` como
//! override de **sessão** (ADR-0014/MT-33) — persiste para os turnos
//! seguintes até ser trocado de novo — `/compact` (MT-37, ADR-0016) para
//! compactar o histórico da sessão via `Session::compact` — e qualquer
//! outra linha como mensagem de usuário.
//!
//! Genérico sobre `Read`/`Write` (não amarrado a `stdin`/`stdout` reais) para
//! ser testável com buffers em memória.

use std::io::{BufRead, Write};

use agentry_core::config::privacy::EgressClass;
use agentry_core::router::{CallPreset, RouteEntry, RouteTarget, Router, RuntimeOverride};
use agentry_core::session::Session;

use crate::streaming::stream_to_writer;

/// Nome do provider único desta CLI na v0.1 (Ollama local) — outros
/// providers chegam nos MT-15/16.
const PROVIDER: &str = "ollama";
/// `task-class` única usada por esta CLI na v0.1 — routing por tipo de
/// tarefa fica para quando o `settings-schema` real existir.
pub(crate) const TASK_CLASS: &str = "chat";

/// Reconfigura a entrada de roteamento `chat` para ter `modelo` como único
/// candidato (Ollama, `local-only`), preservando o preset-base (ex.:
/// `max_tokens` vindo da configuração). Chamado sempre que o usuário troca
/// de modelo via `/model` — o candidato precisa existir declarado antes de
/// [`Router::resolve_with_override`] poder escolhê-lo (ADR-0014/MT-33: o
/// override nunca introduz um alvo não vetado; aqui é a própria CLI, a
/// pedido explícito do humano, quem declara o novo candidato).
pub fn set_chat_route(router: &mut Router, modelo: &str, preset_base: &CallPreset) {
    router.set_route(
        TASK_CLASS,
        RouteEntry {
            candidates: vec![RouteTarget::new(PROVIDER, modelo, EgressClass::LocalOnly)],
            preset: preset_base.clone(),
        },
    );
}

/// Interpreta um valor textual on/off (`on`/`true`/`1` ou `off`/`false`/`0`) —
/// usado tanto pelo comando `/reasoning` quanto pela flag `--reasoning` do
/// modo one-shot, para não duplicar a regra de aceitação em dois lugares.
///
/// # Errors
///
/// Devolve erro se `valor` não casar com nenhuma das grafias reconhecidas.
pub(crate) fn parse_bool_toggle(valor: &str) -> Result<bool, String> {
    match valor.to_lowercase().as_str() {
        "on" | "true" | "1" => Ok(true),
        "off" | "false" | "0" => Ok(false),
        _ => Err(format!("valor inválido (esperado on|off): '{valor}'")),
    }
}

/// Aplica um comando de barra (`/nome valor`) sobre `overrides`.
///
/// Devolve a mensagem de confirmação e se o campo `model` foi tocado (para o
/// chamador decidir se precisa declarar um novo candidato via
/// [`set_chat_route`] antes de resolver de novo).
fn aplicar_comando(
    comando: &str,
    overrides: &mut RuntimeOverride,
) -> Result<(String, bool), String> {
    let mut partes = comando.splitn(2, ' ');
    let nome = partes.next().unwrap_or("");
    let valor = partes.next().unwrap_or("").trim();

    match nome {
        "model" => {
            if valor.is_empty() {
                return Err("uso: /model <nome>".into());
            }
            overrides.model = Some(valor.to_string());
            Ok((format!("modelo alterado para: {valor}"), true))
        }
        "temperature" => {
            let n: f32 = valor
                .parse()
                .map_err(|_| format!("valor inválido para temperature: '{valor}'"))?;
            overrides.temperature = Some(n);
            Ok((format!("temperature alterada para: {n}"), false))
        }
        "top_p" | "top-p" => {
            let n: f32 = valor
                .parse()
                .map_err(|_| format!("valor inválido para top_p: '{valor}'"))?;
            overrides.top_p = Some(n);
            Ok((format!("top_p alterado para: {n}"), false))
        }
        "max_tokens" | "max-tokens" => {
            let n: u32 = valor
                .parse()
                .map_err(|_| format!("valor inválido para max_tokens: '{valor}'"))?;
            overrides.max_tokens = Some(n);
            Ok((format!("max_tokens alterado para: {n}"), false))
        }
        "system" => {
            if valor.is_empty() {
                return Err("uso: /system <texto>".into());
            }
            overrides.system_prompt = Some(valor.to_string());
            Ok((
                "system prompt (da próxima mensagem em diante) atualizado".to_string(),
                false,
            ))
        }
        "reasoning" => {
            let ligado = parse_bool_toggle(valor)
                .map_err(|_| format!("uso: /reasoning on|off (veio '{valor}')"))?;
            overrides.reasoning = Some(ligado);
            Ok((format!("reasoning alterado para: {ligado}"), false))
        }
        outro => Err(format!("comando desconhecido: /{outro}")),
    }
}

/// Roda o REPL até `/exit`, `/quit` ou EOF na entrada.
///
/// `session_override` é o estado inicial (tipicamente vindo das flags de
/// invocação); comandos de barra atualizam esse mesmo estado, que passa a
/// valer para os turnos seguintes até ser trocado de novo (ADR-0014).
///
/// # Errors
///
/// Devolve erro se I/O em `input`/`output` falhar, ou se uma resolução de
/// rota após um comando falhar (ex.: classe de egresso insuficiente).
pub async fn run_repl<R: BufRead, W: Write>(
    mut input: R,
    mut output: W,
    session: &mut Session,
    router: &mut Router,
    preset_base: &CallPreset,
    mut session_override: RuntimeOverride,
) -> Result<(), String> {
    loop {
        write!(output, "> ").map_err(|e| e.to_string())?;
        output.flush().map_err(|e| e.to_string())?;

        let mut linha = String::new();
        let lidos = input.read_line(&mut linha).map_err(|e| e.to_string())?;
        if lidos == 0 {
            break; // EOF
        }
        let linha = linha.trim();
        if linha.is_empty() {
            continue;
        }
        if linha == "/exit" || linha == "/quit" {
            break;
        }
        if linha == "/compact" {
            match session.compact(router).await {
                Ok(()) => writeln!(output, "sessão compactada").map_err(|e| e.to_string())?,
                Err(erro) => writeln!(output, "erro: {erro}").map_err(|e| e.to_string())?,
            }
            continue;
        }

        if let Some(comando) = linha.strip_prefix('/') {
            match aplicar_comando(comando, &mut session_override) {
                Ok((mensagem, mudou_model)) => {
                    writeln!(output, "{mensagem}").map_err(|e| e.to_string())?;
                    if mudou_model {
                        if let Some(modelo) = session_override.model.clone() {
                            set_chat_route(router, &modelo, preset_base);
                        }
                    }
                    let rota = router
                        .resolve_with_override(TASK_CLASS, &session_override)
                        .map_err(|e| e.to_string())?;
                    session.apply_route(rota);
                }
                Err(erro) => {
                    writeln!(output, "erro: {erro}").map_err(|e| e.to_string())?;
                }
            }
            continue;
        }

        session.push_user_message(linha);
        stream_to_writer(session, &mut output, router).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::model::{Message, Role, StreamEvent, ToolCall, ToolResult, Usage};
    use agentry_core::provider::mock::MockProvider;
    use agentry_core::session::{TokenBudget, ToolExecutor};
    use std::io::Cursor;
    use std::sync::Arc;

    struct NoopExecutor;
    impl ToolExecutor for NoopExecutor {
        fn execute(&self, call: &ToolCall) -> agentry_core::provider::BoxFuture<'_, ToolResult> {
            let call_id = call.id.clone();
            Box::pin(async move {
                ToolResult {
                    call_id,
                    content: String::new(),
                    is_error: false,
                }
            })
        }
    }

    fn roteiro_de_resposta(texto: &str) -> Vec<StreamEvent> {
        vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta {
                text: texto.to_string(),
            },
            StreamEvent::MessageEnd {
                usage: Usage::default(),
            },
        ]
    }

    fn router_com_ollama(mock: Arc<MockProvider>, modelo_inicial: &str) -> Router {
        let mut router = Router::new(EgressClass::LocalOnly);
        router.register_provider(mock);
        set_chat_route(&mut router, modelo_inicial, &CallPreset::default());
        router
    }

    #[tokio::test]
    async fn comando_repl_muda_parametro_para_turnos_seguintes() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        mock.enqueue_stream(roteiro_de_resposta("primeira"));
        mock.enqueue_stream(roteiro_de_resposta("segunda"));

        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada =
            "/temperature 0.9\nprimeira mensagem\n/temperature 0.2\nsegunda mensagem\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            &CallPreset::default(),
            RuntimeOverride::default(),
        )
        .await
        .expect("repl deve rodar sem erro");

        let requisicoes = mock.chat_requests();
        assert_eq!(requisicoes.len(), 2);
        assert_eq!(requisicoes[0].temperature, Some(0.9));
        assert_eq!(requisicoes[1].temperature, Some(0.2));
    }

    #[tokio::test]
    async fn comando_model_declara_novo_candidato_e_troca_de_fato() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        mock.enqueue_stream(roteiro_de_resposta("ok"));

        let mut router = router_com_ollama(mock.clone(), "modelo-antigo");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/model modelo-novo\nmensagem\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            &CallPreset::default(),
            RuntimeOverride::default(),
        )
        .await
        .expect("repl deve rodar sem erro");

        let requisicoes = mock.chat_requests();
        assert_eq!(requisicoes.len(), 1);
        assert_eq!(requisicoes[0].model, "modelo-novo");

        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("modelo alterado para: modelo-novo"));
    }

    #[tokio::test]
    async fn comando_desconhecido_nao_derruba_o_repl() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        mock.enqueue_stream(roteiro_de_resposta("ok"));

        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/nao-existe\nmensagem\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            &CallPreset::default(),
            RuntimeOverride::default(),
        )
        .await
        .expect("comando desconhecido não deve interromper o repl");

        assert_eq!(
            mock.chat_requests().len(),
            1,
            "a mensagem seguinte ainda deve rodar"
        );
        assert!(String::from_utf8(saida.clone())
            .unwrap()
            .contains("comando desconhecido"));
    }

    #[tokio::test]
    async fn exit_encerra_sem_processar_mais_nada() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/exit\nmensagem que nunca deveria rodar\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            &CallPreset::default(),
            RuntimeOverride::default(),
        )
        .await
        .expect("deve encerrar limpo");

        assert_eq!(mock.chat_requests().len(), 0);
    }

    /// Registra a `task-class` `"compact"` sobre o mesmo mock/modelo já
    /// registrado por [`router_com_ollama`] (MT-37).
    fn com_task_class_compact(mut router: Router, modelo: &str) -> Router {
        router.set_route(
            "compact",
            RouteEntry {
                candidates: vec![RouteTarget::new(PROVIDER, modelo, EgressClass::LocalOnly)],
                preset: CallPreset::default(),
            },
        );
        router
    }

    #[tokio::test]
    async fn comando_compact_reduz_historico_a_uma_unica_mensagem_de_sistema() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        mock.enqueue_stream(roteiro_de_resposta("resposta original"));
        mock.enqueue_chat(Ok(agentry_core::provider::ChatResponse {
            message: Message::assistant("resumo da conversa"),
            usage: Usage::default(),
        }));

        let mut router =
            com_task_class_compact(router_com_ollama(mock.clone(), "modelo-x"), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "mensagem original\n/compact\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            &CallPreset::default(),
            RuntimeOverride::default(),
        )
        .await
        .expect("repl deve rodar sem erro");

        assert_eq!(session.messages().len(), 1);
        assert_eq!(session.messages()[0].role, Role::System);

        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("compactada"));
    }

    #[tokio::test]
    async fn comando_compact_com_erro_nao_derruba_o_repl() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        mock.enqueue_stream(roteiro_de_resposta("resposta original"));
        // Nenhuma resposta enfileirada para a chamada de compactação: falha.

        let mut router =
            com_task_class_compact(router_com_ollama(mock.clone(), "modelo-x"), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "mensagem original\n/compact\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            &CallPreset::default(),
            RuntimeOverride::default(),
        )
        .await
        .expect("erro de compactação não deve derrubar o repl");

        // Tudo-ou-nada: histórico da primeira mensagem (user + assistant)
        // permanece intocado.
        assert_eq!(session.messages().len(), 2);

        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("erro:"));
    }

    #[tokio::test]
    async fn comando_compact_com_historico_vazio_nao_falha() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        let mut router =
            com_task_class_compact(router_com_ollama(mock.clone(), "modelo-x"), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/compact\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            &CallPreset::default(),
            RuntimeOverride::default(),
        )
        .await
        .expect("histórico vazio não deve causar erro");

        assert!(session.messages().is_empty());
        assert_eq!(mock.chat_requests().len(), 0);
    }
}
