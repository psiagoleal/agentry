// Caminho relativo: crates/cli/src/repl.rs
//! REPL interativo (MT-14): lê linhas, trata comandos `/model`,
//! `/temperature`, `/top_p`, `/system`, `/max_tokens`, `/reasoning` como
//! override de **sessão** (ADR-0014/MT-33) — persiste para os turnos
//! seguintes até ser trocado de novo — `/compact` (MT-37, ADR-0016) para
//! compactar o histórico da sessão via `Session::compact` — e qualquer
//! outra linha como mensagem de usuário. `/init` (MT-41, ADR-0019) materializa
//! `.agentry/agentry.settings.json` via [`crate::run_init_local`] — mesma
//! função usada pela flag `--init` do modo one-shot, sem duplicar a lógica.
//! `/usage` (MT-83, ADR-0029) imprime o uso de tokens acumulado da sessão
//! até aquele ponto, via [`crate::formatar_uso`] — mesma formatação do
//! resumo do modo *one-shot*, sem *side-effect* na conversa. `/undo`
//! (MT-87, ADR-0030) desfaz o checkpoint mais recente de `fs_write`/
//! `fs_edit` via [`agentry_core::checkpoint::CheckpointStore::undo`] —
//! mesma lógica da flag `--undo` do modo *one-shot*.
//!
//! Genérico sobre `Read`/`Write` (não amarrado a `stdin`/`stdout` reais) para
//! ser testável com buffers em memória.

use std::io::{BufRead, Write};
use std::path::Path;
use std::sync::Arc;

use agentry_core::config::privacy::EgressClass;
use agentry_core::router::{CallPreset, RouteEntry, RouteTarget, Router, RuntimeOverride};
use agentry_core::session::Session;

use crate::streaming::stream_to_writer;

/// Nome do provider único desta CLI na v0.1 (Ollama local) — outros
/// providers chegam nos MT-15/16. `pub(crate)` para que `main.rs` sintetize
/// os defaults de `compact`/`guardrail-compliance` (MT-56, ADR-0021) sem
/// repetir o literal `"ollama"` uma terceira vez.
pub(crate) const PROVIDER: &str = "ollama";
/// `task-class` **default** desta CLI, voltada ao usuário (interativo e
/// one-shot) — ADR-0021. Outras task-classes declaradas em
/// `taskClasses` (MT-55) ou internas (`compact`/`guardrail-compliance`)
/// passam a ter rota real a partir do MT-56; a seleção por invocação é
/// feita via `--task-class`/`/task-class`, nunca por esta constante.
pub(crate) const TASK_CLASS: &str = "chat";

/// Reconfigura a entrada de roteamento `chat` (sempre `chat`, nunca a
/// task-class ativa escolhida via `/task-class` — ver a constante
/// [`TASK_CLASS`]) para ter `modelo` (Ollama, `local-only`) como candidato
/// preferencial, seguido de `candidato_extra` se houver (ex.: o endpoint
/// LiteLLM resolvido de `providers.litellm`, MT-49) — preserva o
/// preset-base (ex.: `max_tokens` vindo da configuração). Chamado sempre
/// que o usuário troca de modelo via `/model` — como [`Router::set_route`]
/// substitui a entrada inteira (não existe "adicionar candidato"), **todo**
/// candidato desejado precisa ser redeclarado aqui a cada chamada, não só o
/// Ollama, senão um candidato extra já registrado desapareceria
/// silenciosamente na primeira troca de modelo. O candidato precisa existir
/// declarado antes de [`Router::resolve_with_override`] poder escolhê-lo
/// (ADR-0014/MT-33: o override nunca introduz um alvo não vetado; aqui é a
/// própria CLI, a pedido explícito do humano, quem declara os candidatos).
pub fn set_chat_route(
    router: &mut Router,
    modelo: &str,
    preset_base: &CallPreset,
    candidato_extra: Option<&RouteTarget>,
) {
    let mut candidates = vec![RouteTarget::new(PROVIDER, modelo, EgressClass::LocalOnly)];
    candidates.extend(candidato_extra.cloned());
    router.set_route(
        TASK_CLASS,
        RouteEntry {
            candidates,
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
        "provider" => {
            if valor.is_empty() {
                return Err("uso: /provider <nome>".into());
            }
            overrides.provider = Some(valor.to_string());
            // Restringe a escolha aos candidatos já declarados na rota
            // (ADR-0014) — nenhum candidato novo é introduzido aqui, então
            // não há necessidade de redeclarar a rota como `/model` faz;
            // `resolve_with_override`, chamado logo em seguida por
            // `run_repl`, já filtra pelo provider pedido.
            Ok((format!("provider alterado para: {valor}"), false))
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

/// Configuração estável do REPL — não muda turno a turno, diferente de
/// `session`/`router`/`session_override` (agrupada à parte só para não
/// estourar o limite de argumentos de [`run_repl`], `clippy::too_many_arguments`).
#[derive(Clone, Copy)]
pub struct ReplConfig<'a> {
    /// Raiz usada pelo comando `/init` (MT-41, ADR-0019) para localizar/criar
    /// `.agentry/agentry.settings.json` — passada explicitamente (em vez de
    /// `run_repl` ler `std::env::current_dir()` por conta própria) para que
    /// os testes nunca escrevam no diretório real do processo.
    pub workspace_root: &'a Path,
    /// Preset-base da `task-class` de chat, reaplicado a cada `/model`.
    pub preset_base: &'a CallPreset,
    /// Candidato de rota extra (hoje só LiteLLM, se `providers.litellm`
    /// estiver configurado, MT-49) — redeclarado a cada `/model` (ver
    /// [`set_chat_route`]), já que `Router::set_route` substitui a entrada
    /// inteira em vez de aceitar um candidato adicional.
    pub candidato_extra: Option<&'a RouteTarget>,
}

/// Roda o REPL até `/exit`, `/quit` ou EOF na entrada. `session_override` é
/// o estado inicial (tipicamente vindo das flags de invocação); comandos de
/// barra atualizam esse mesmo estado, que passa a valer para os turnos
/// seguintes até ser trocado de novo (ADR-0014). `task_class` é a
/// task-class **ativa** desta sessão (tipicamente [`TASK_CLASS`] ou o valor
/// de `--task-class`, MT-56/ADR-0021) — toda resolução de rota após um
/// comando usa esse nome, até `/task-class <outro-nome>` trocá-lo.
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
    mut session_override: RuntimeOverride,
    mut task_class: String,
    config: &ReplConfig<'_>,
) -> Result<(), String> {
    let ReplConfig {
        workspace_root,
        preset_base,
        candidato_extra,
    } = *config;
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
        if linha == "/usage" {
            writeln!(
                output,
                "uso desta sessão: {}",
                crate::formatar_uso(session.usage_total())
            )
            .map_err(|e| e.to_string())?;
            continue;
        }
        if linha == "/undo" {
            let store = agentry_core::checkpoint::CheckpointStore::new(workspace_root);
            match store.undo() {
                Ok(outcome) => writeln!(output, "{}", crate::formatar_undo(&outcome))
                    .map_err(|e| e.to_string())?,
                Err(erro) => writeln!(output, "erro: {erro}").map_err(|e| e.to_string())?,
            }
            continue;
        }
        if linha == "/task-class" || linha.starts_with("/task-class ") {
            let nome = linha.strip_prefix("/task-class").unwrap_or("").trim();
            if nome.is_empty() {
                writeln!(output, "uso: /task-class <nome>").map_err(|e| e.to_string())?;
                continue;
            }
            // Mesmo padrão de override vetado do `--provider`/`--model`
            // (ADR-0014): só troca de fato se a task-class pedida resolver
            // de verdade — nome desconhecido ou candidato indisponível
            // (`Router::resolve_with_override`) é erro reportado, sem
            // deixar a sessão num estado inconsistente (task-class ativa
            // só muda quando a resolução funciona).
            match router.resolve_with_override(nome, &session_override) {
                Ok(rota) => {
                    task_class = nome.to_string();
                    session.apply_route(rota);
                    writeln!(output, "task-class alterada para: {nome}")
                        .map_err(|e| e.to_string())?;
                }
                Err(erro) => {
                    writeln!(output, "erro: {erro}").map_err(|e| e.to_string())?;
                }
            }
            continue;
        }
        if linha == "/init" || linha.starts_with("/init ") {
            let perfil = linha.strip_prefix("/init").unwrap_or("").trim();
            let resultado = if perfil.is_empty() {
                crate::run_init_local(workspace_root)
            } else {
                let sink: Arc<dyn agentry_core::transport::AuditSink> =
                    Arc::new(crate::StderrAuditSink);
                match crate::init::fetch_profile_settings(perfil, sink).await {
                    Ok(conteudo) => crate::write_settings_if_absent(workspace_root, &conteudo),
                    Err(erro) => {
                        writeln!(output, "erro: {erro}").map_err(|e| e.to_string())?;
                        continue;
                    }
                }
            };
            match resultado {
                Ok(outcome) => crate::escrever_resultado_init(&outcome, &mut output)
                    .map_err(|e| e.to_string())?,
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
                            set_chat_route(router, &modelo, preset_base, candidato_extra);
                        }
                    }
                    let rota = router
                        .resolve_with_override(&task_class, &session_override)
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
        set_chat_route(&mut router, modelo_inicial, &CallPreset::default(), None);
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
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
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
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
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
    async fn comando_provider_troca_para_o_candidato_extra_sem_precisar_de_model() {
        let mock_ollama = Arc::new(MockProvider::new(PROVIDER));
        let mock_litellm = Arc::new(MockProvider::new("litellm"));
        mock_litellm.enqueue_stream(roteiro_de_resposta("resposta do litellm"));

        let mut router = Router::new(EgressClass::LocalOnly);
        router.register_provider(mock_ollama.clone());
        router.register_provider(mock_litellm.clone());
        let candidato_litellm = RouteTarget::new("litellm", "modelo-30b", EgressClass::LocalOnly);
        set_chat_route(
            &mut router,
            "modelo-ollama",
            &CallPreset::default(),
            Some(&candidato_litellm),
        );
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/provider litellm\nmensagem\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: Some(&candidato_litellm),
            },
        )
        .await
        .expect("repl deve rodar sem erro");

        assert_eq!(
            mock_litellm.chat_requests().len(),
            1,
            "mensagem deve ter ido para o candidato litellm"
        );
        assert_eq!(
            mock_ollama.chat_requests().len(),
            0,
            "ollama nunca deveria ser chamado depois do /provider"
        );

        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("provider alterado para: litellm"));
    }

    #[tokio::test]
    async fn comando_provider_com_nome_desconhecido_propaga_erro_de_resolucao_sem_panic() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        mock.enqueue_stream(roteiro_de_resposta("ok"));

        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/provider nao-existe\nmensagem\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
        )
        .await
        .expect_err("provider inexistente deve propagar erro, mas sem panic");

        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("provider alterado para: nao-existe"));
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
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
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
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
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
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
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
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
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
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
        )
        .await
        .expect("histórico vazio não deve causar erro");

        assert!(session.messages().is_empty());
        assert_eq!(mock.chat_requests().len(), 0);
    }

    #[tokio::test]
    async fn comando_usage_imprime_o_total_acumulado_sem_alterar_historico_nem_preset() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta {
                text: "resposta".to_string(),
            },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            },
        ]);

        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "mensagem original\n/usage\n/temperature 0.5\n/usage\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
        )
        .await
        .expect("repl deve rodar sem erro");

        assert_eq!(
            session.usage_total(),
            Usage {
                input_tokens: 10,
                output_tokens: 5
            }
        );
        let historico_antes_do_usage = session.messages().len();
        assert_eq!(
            historico_antes_do_usage, 2,
            "user + assistant, /usage não deve ter side-effect no histórico"
        );

        let saida_texto = String::from_utf8(saida).unwrap();
        let ocorrencias = saida_texto.matches("uso desta sessão:").count();
        assert_eq!(
            ocorrencias, 2,
            "as duas chamadas de /usage devem imprimir o resumo"
        );
        assert!(saida_texto.contains("10 tokens de entrada, 5 de saída (total: 15)"));
    }

    #[tokio::test]
    async fn comando_undo_desfaz_o_checkpoint_mais_recente() {
        let dir = TempDir::new();
        std::fs::write(dir.path().join("a.txt"), "original").unwrap();
        let store = agentry_core::checkpoint::CheckpointStore::new(dir.path());
        store
            .record("a.txt", Some("original".to_string()))
            .expect("record deve funcionar");
        std::fs::write(dir.path().join("a.txt"), "sobrescrito pela tool").unwrap();

        let mock = Arc::new(MockProvider::new(PROVIDER));
        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/undo\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: dir.path(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
        )
        .await
        .expect("repl deve rodar sem erro");

        assert_eq!(
            std::fs::read_to_string(dir.path().join("a.txt")).unwrap(),
            "original",
            "/undo deve restaurar o conteúdo anterior"
        );
        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("'a.txt' restaurado ao conteúdo anterior"));
    }

    #[tokio::test]
    async fn comando_undo_sem_nenhum_checkpoint_e_erro_tratado_sem_derrubar_o_repl() {
        let dir = TempDir::new();
        let mock = Arc::new(MockProvider::new(PROVIDER));
        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/undo\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: dir.path(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
        )
        .await
        .expect("repl deve rodar sem erro, mesmo com /undo falhando");

        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("erro:"));
    }

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `state_dir`/`config`/`main::tests`,
    /// MT-38/39/41) — usado pelos testes de `/init` e `/undo` (MT-87), que
    /// de fato escrevem em disco (os demais testes deste módulo passam
    /// `std::env::temp_dir()` compartilhado porque nunca tocam o disco de
    /// verdade).
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-cli-repl-test-{}-{}",
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

    #[tokio::test]
    async fn comando_init_materializa_o_arquivo_pela_mesma_funcao_do_flag_cli() {
        let dir = TempDir::new();
        let mock = Arc::new(MockProvider::new(PROVIDER));
        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/init\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: dir.path(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
        )
        .await
        .expect("/init não deve derrubar o repl");

        let caminho = crate::state_dir::agentry_settings_path(dir.path());
        let conteudo = std::fs::read_to_string(&caminho)
            .expect("/init deve ter criado o arquivo via a mesma run_init_local do --init");
        assert_eq!(conteudo, crate::GENERIC_SETTINGS_EXAMPLE);

        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("criado:"));
        assert!(saida_texto.contains(crate::MANUAL_SETUP_HINT));
        assert_eq!(
            mock.chat_requests().len(),
            0,
            "/init não deve chamar o provider"
        );
    }

    // --- MT-56: comando `/task-class` (ADR-0021) ---

    #[tokio::test]
    async fn comando_task_class_troca_para_outra_rota_declarada_e_mensagens_seguintes_vao_para_ela()
    {
        let mock_chat = Arc::new(MockProvider::new(PROVIDER));
        let mock_revisao = Arc::new(MockProvider::new("litellm"));
        mock_revisao.enqueue_stream(roteiro_de_resposta("resposta da revisão"));

        let mut router = router_com_ollama(mock_chat.clone(), "modelo-x");
        router.register_provider(mock_revisao.clone());
        router.set_route(
            "revisao",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "litellm",
                    "modelo-revisao",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/task-class revisao\nmensagem\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
        )
        .await
        .expect("repl deve rodar sem erro");

        assert_eq!(mock_revisao.chat_requests().len(), 1);
        assert_eq!(
            mock_chat.chat_requests().len(),
            0,
            "chat não deveria ser chamado depois do /task-class"
        );
        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("task-class alterada para: revisao"));
    }

    #[tokio::test]
    async fn comando_task_class_com_nome_desconhecido_propaga_erro_sem_derrubar_o_repl() {
        let mock = Arc::new(MockProvider::new(PROVIDER));
        mock.enqueue_stream(roteiro_de_resposta("ok"));

        let mut router = router_com_ollama(mock.clone(), "modelo-x");
        let rota = router.resolve(TASK_CLASS).expect("deve resolver");
        let mut session = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(100_000));

        let entrada = "/task-class nao-existe\nmensagem\n/exit\n";
        let mut saida = Vec::new();

        run_repl(
            Cursor::new(entrada.as_bytes()),
            &mut saida,
            &mut session,
            &mut router,
            RuntimeOverride::default(),
            TASK_CLASS.to_string(),
            &ReplConfig {
                workspace_root: &std::env::temp_dir(),
                preset_base: &CallPreset::default(),
                candidato_extra: None,
            },
        )
        .await
        .expect("nome de task-class desconhecido não deve derrubar o repl, só reportar erro");

        assert_eq!(
            mock.chat_requests().len(),
            1,
            "task-class ativa continua 'chat' — a mensagem seguinte ainda deve rodar nela"
        );
        let saida_texto = String::from_utf8(saida).unwrap();
        assert!(saida_texto.contains("erro:"));
    }
}
