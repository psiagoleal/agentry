// Caminho relativo: crates/cli/src/streaming.rs
//! Roda um turno da [`Session`] com streaming, escrevendo o texto conforme
//! chega em qualquer `impl Write` — genérico para permitir testar contra um
//! buffer em memória (`Vec<u8>`), sem depender do `stdout` real.

use std::io::Write;

use agentry_core::model::StreamEvent;
use agentry_core::router::Router;
use agentry_core::session::{Session, SessionError, SessionOutcome};

/// Roda a sessão em modo streaming, escrevendo cada [`StreamEvent::TextDelta`]
/// em `output` assim que chega, e uma quebra de linha ao final do turno.
/// `router` é repassado a [`Session::run_streaming`] — só usado de fato se a
/// sessão tiver alguma auditoria do Reviewer habilitada (MT-35, ADR-0015);
/// nenhuma flag/comando de CLI liga isso ainda (fora de escopo do MT-35).
///
/// # Errors
///
/// Devolve o erro do [`Session::run_streaming`] convertido para `String`.
pub async fn stream_to_writer<W: Write>(
    session: &mut Session,
    mut output: W,
    router: &Router,
) -> Result<SessionOutcome, String> {
    let outcome = session
        .run_streaming(
            |evento| {
                if let StreamEvent::TextDelta { text } = evento {
                    let _ = write!(output, "{text}");
                    let _ = output.flush();
                }
            },
            router,
        )
        .await
        .map_err(|e: SessionError| e.to_string())?;
    writeln!(output).map_err(|e| e.to_string())?;
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::config::privacy::EgressClass;
    use agentry_core::model::{ToolCall, ToolResult, Usage};
    use agentry_core::provider::{mock::MockProvider, BoxFuture};
    use agentry_core::router::{CallPreset, ResolvedRoute};
    use agentry_core::session::{StopReason, TokenBudget, ToolExecutor};
    use std::sync::Arc;

    struct NoopExecutor;
    impl ToolExecutor for NoopExecutor {
        fn execute(&self, call: &ToolCall) -> BoxFuture<'_, ToolResult> {
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

    #[tokio::test]
    async fn escreve_o_texto_conforme_chega_e_devolve_o_outcome() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta { text: "ol".into() },
            StreamEvent::TextDelta { text: "á!".into() },
            StreamEvent::MessageEnd {
                usage: Usage::default(),
            },
        ]);
        let mut session = Session::new(
            ResolvedRoute::new(mock, "modelo-x", CallPreset::default()),
            Arc::new(NoopExecutor),
            TokenBudget::new(10_000),
        );
        session.push_user_message("oi");

        let mut saida = Vec::new();
        let router = Router::new(EgressClass::LocalOnly);
        let outcome = stream_to_writer(&mut session, &mut saida, &router)
            .await
            .expect("deve completar");

        assert_eq!(
            StopReason::Done,
            outcome.reason,
            "sem tool-call, o turno deve terminar em Done"
        );
        assert_eq!(String::from_utf8(saida).unwrap(), "olá!\n");
    }

    #[tokio::test]
    async fn resumo_de_uso_nunca_aparece_no_stdout_do_streaming() {
        // MT-83/ADR-0029: o resumo de uso do modo one-shot é impresso pelo
        // chamador (`main.rs`, via `eprintln!`/`formatar_uso`) **depois** de
        // `stream_to_writer` retornar, nunca dentro dele — este teste prova
        // que o buffer que representa `stdout` aqui nunca carrega esse texto,
        // só a resposta em si (separação stdout/stderr).
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_stream(vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta {
                text: "resposta".into(),
            },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
            },
        ]);
        let mut session = Session::new(
            ResolvedRoute::new(mock, "modelo-x", CallPreset::default()),
            Arc::new(NoopExecutor),
            TokenBudget::new(10_000),
        );
        session.push_user_message("oi");

        let mut stdout_simulado = Vec::new();
        let router = Router::new(EgressClass::LocalOnly);
        stream_to_writer(&mut session, &mut stdout_simulado, &router)
            .await
            .expect("deve completar");

        let stdout_texto = String::from_utf8(stdout_simulado).unwrap();
        assert_eq!(stdout_texto, "resposta\n");
        assert!(
            !stdout_texto.contains("tokens"),
            "resumo de uso não deve vazar para o stdout do streaming"
        );

        // O dado de uso já está disponível na sessão para o chamador
        // formatar e emitir em stderr (main.rs) — verificado aqui, não
        // dentro de `stream_to_writer`.
        assert_eq!(
            session.usage_total(),
            Usage {
                input_tokens: 10,
                output_tokens: 5
            }
        );
        assert_eq!(
            crate::formatar_uso(session.usage_total()),
            "10 tokens de entrada, 5 de saída (total: 15)"
        );
    }
}
