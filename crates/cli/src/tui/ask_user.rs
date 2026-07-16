// Caminho relativo: crates/cli/src/tui/ask_user.rs
//! `TuiPrompter` (MT-74/ADR-0027): implementação de
//! [`Prompter`](agentry_core::tools::ask_user::Prompter) para o modo TUI —
//! em vez de `print!`/`read_line` (`InteractivePrompter`,
//! `crates/cli/src/tool_executor.rs`, que briga com o modo bruto do
//! terminal, achado do MT-72), envia um
//! [`PedidoHumano::Pergunta`](crate::tool_executor::PedidoHumano) pelo
//! canal compartilhado com o [`TuiConfirmer`](crate::tool_executor::TuiConfirmer)
//! e aguarda a resposta pelo `oneshot` do pedido. Diferente do `Confirmer`,
//! não tem *toggle* `auto` — a tool `ask_user` (ADR-0024) existe
//! justamente para perguntar algo ao usuário; pular a pergunta
//! contrariaria o propósito da tool, não é uma aceleração de UX.

use tokio::sync::{mpsc, oneshot};

use agentry_core::provider::BoxFuture;
use agentry_core::tools::ask_user::Prompter;

use crate::tool_executor::PedidoHumano;

pub struct TuiPrompter {
    tx: mpsc::UnboundedSender<PedidoHumano>,
}

impl TuiPrompter {
    /// Cria o `Prompter` — `tx` é o remetente compartilhado com
    /// `TuiConfirmer` (mesmo canal, mesmo laço de eventos do lado
    /// receptor).
    #[must_use]
    pub fn new(tx: mpsc::UnboundedSender<PedidoHumano>) -> Self {
        Self { tx }
    }
}

impl Prompter for TuiPrompter {
    fn ask(&self, question: &str, options: &[String]) -> BoxFuture<'_, String> {
        let (responder, receptor) = oneshot::channel();
        let pedido = PedidoHumano::Pergunta {
            question: question.to_string(),
            options: options.to_vec(),
            responder,
        };
        let tx = self.tx.clone();
        Box::pin(async move {
            if tx.send(pedido).is_err() {
                // Laço de eventos encerrado — devolve resposta vazia
                // (mesmo padrão de falha de leitura do `InteractivePrompter`).
                return String::new();
            }
            receptor.await.unwrap_or_default()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn envia_pedido_pelo_canal_e_aguarda_a_resposta() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let prompter = TuiPrompter::new(tx);

        let responder_tarefa = tokio::spawn(async move {
            let pedido = rx.recv().await.expect("deve chegar um pedido");
            match pedido {
                PedidoHumano::Pergunta {
                    question,
                    options,
                    responder,
                } => {
                    assert_eq!(question, "qual sua cor favorita?");
                    assert_eq!(options, vec!["azul".to_string(), "verde".to_string()]);
                    responder.send("azul".to_string()).expect("laço vivo");
                }
                PedidoHumano::Confirmacao { .. } => panic!("esperava um pedido de pergunta"),
            }
        });

        let resposta = prompter
            .ask(
                "qual sua cor favorita?",
                &["azul".to_string(), "verde".to_string()],
            )
            .await;

        assert_eq!(resposta, "azul");
        responder_tarefa
            .await
            .expect("dublê não deve entrar em pânico");
    }

    #[tokio::test]
    async fn sem_ninguem_do_outro_lado_do_canal_devolve_resposta_vazia() {
        let (tx, rx) = mpsc::unbounded_channel();
        drop(rx); // simula o laço de eventos já encerrado
        let prompter = TuiPrompter::new(tx);

        let resposta = prompter.ask("pergunta qualquer?", &[]).await;

        assert_eq!(resposta, "");
    }
}
