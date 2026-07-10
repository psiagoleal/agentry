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
}
