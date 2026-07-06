// Caminho relativo: crates/core/src/provider/mock.rs
//! Provider de teste (MT-03): respostas roteirizadas, sem rede.
//!
//! O [`MockProvider`] devolve respostas enfileiradas previamente e registra as
//! requisições recebidas, permitindo testar o agent loop, o router e as tools
//! sem tocar em nenhuma API real (ADR-0001/0002).

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::provider::{
    BoxFuture, ChatRequest, ChatResponse, ChatStream, EmbeddingsRequest, EmbeddingsResponse,
    LlmProvider, ProviderError,
};

use crate::model::StreamEvent;

/// Provider falso com respostas roteirizadas (FIFO) e registro de requisições.
#[derive(Debug, Default)]
pub struct MockProvider {
    name: String,
    chat_responses: Mutex<VecDeque<Result<ChatResponse, ProviderError>>>,
    stream_scripts: Mutex<VecDeque<Vec<StreamEvent>>>,
    embeddings_responses: Mutex<VecDeque<Result<EmbeddingsResponse, ProviderError>>>,
    chat_requests: Mutex<Vec<ChatRequest>>,
}

impl MockProvider {
    /// Cria um mock vazio com o nome dado.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Enfileira a próxima resposta de [`LlmProvider::chat`].
    pub fn enqueue_chat(&self, response: Result<ChatResponse, ProviderError>) {
        self.chat_responses
            .lock()
            .expect("mutex do mock não deve envenenar")
            .push_back(response);
    }

    /// Enfileira o roteiro de eventos do próximo [`LlmProvider::chat_stream`].
    pub fn enqueue_stream(&self, events: Vec<StreamEvent>) {
        self.stream_scripts
            .lock()
            .expect("mutex do mock não deve envenenar")
            .push_back(events);
    }

    /// Enfileira a próxima resposta de [`LlmProvider::embeddings`].
    pub fn enqueue_embeddings(&self, response: Result<EmbeddingsResponse, ProviderError>) {
        self.embeddings_responses
            .lock()
            .expect("mutex do mock não deve envenenar")
            .push_back(response);
    }

    /// Requisições de chat (e stream) recebidas até aqui, em ordem.
    #[must_use]
    pub fn chat_requests(&self) -> Vec<ChatRequest> {
        self.chat_requests
            .lock()
            .expect("mutex do mock não deve envenenar")
            .clone()
    }

    fn record(&self, request: &ChatRequest) {
        self.chat_requests
            .lock()
            .expect("mutex do mock não deve envenenar")
            .push(request.clone());
    }

    fn exhausted(&self, capability: &str) -> ProviderError {
        ProviderError::InvalidResponse(format!(
            "MockProvider '{}' sem {capability} enfileirado(a)",
            self.name
        ))
    }
}

impl LlmProvider for MockProvider {
    fn name(&self) -> &str {
        &self.name
    }

    fn chat(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatResponse, ProviderError>> {
        self.record(&request);
        let next = self
            .chat_responses
            .lock()
            .expect("mutex do mock não deve envenenar")
            .pop_front();
        Box::pin(async move { next.unwrap_or_else(|| Err(self.exhausted("resposta de chat"))) })
    }

    fn chat_stream(
        &self,
        request: ChatRequest,
    ) -> BoxFuture<'_, Result<ChatStream, ProviderError>> {
        self.record(&request);
        let script = self
            .stream_scripts
            .lock()
            .expect("mutex do mock não deve envenenar")
            .pop_front();
        Box::pin(async move {
            let events = script.ok_or_else(|| self.exhausted("roteiro de stream"))?;
            let (tx, rx) = tokio::sync::mpsc::channel(events.len().max(1));
            for event in events {
                tx.try_send(Ok(event))
                    .expect("canal do mock dimensionado para o roteiro inteiro");
            }
            // `tx` sai de escopo aqui: o canal fecha e o consumidor recebe `None` no fim.
            Ok(rx)
        })
    }

    fn embeddings(
        &self,
        request: EmbeddingsRequest,
    ) -> BoxFuture<'_, Result<EmbeddingsResponse, ProviderError>> {
        let _ = request;
        let next = self
            .embeddings_responses
            .lock()
            .expect("mutex do mock não deve envenenar")
            .pop_front();
        Box::pin(
            async move { next.unwrap_or_else(|| Err(self.exhausted("resposta de embeddings"))) },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ContentBlock, Message, Role, StreamEvent, ToolCall, Usage};

    fn resposta_texto(texto: &str) -> ChatResponse {
        ChatResponse {
            message: Message::assistant(texto),
            usage: Usage {
                input_tokens: 3,
                output_tokens: 5,
            },
        }
    }

    #[tokio::test]
    async fn chat_devolve_resposta_enfileirada_e_registra_requisicao() {
        let mock = MockProvider::new("mock");
        mock.enqueue_chat(Ok(resposta_texto("olá!")));

        let req = ChatRequest::new("modelo-x", vec![Message::user("oi")]);
        let resp = mock.chat(req.clone()).await.expect("chat deve responder");

        assert_eq!(resp.message, Message::assistant("olá!"));
        assert_eq!(resp.usage.total(), 8);
        assert_eq!(
            mock.chat_requests(),
            vec![req],
            "requisição deve ser registrada"
        );
    }

    #[tokio::test]
    async fn chat_suporta_tool_calling() {
        let mock = MockProvider::new("mock");
        let chamada = ToolCall {
            id: "call-1".into(),
            name: "fs_read".into(),
            arguments: serde_json::json!({ "path": "Cargo.toml" }),
        };
        mock.enqueue_chat(Ok(ChatResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(chamada.clone())],
            },
            usage: Usage::default(),
        }));

        let resp = mock
            .chat(ChatRequest::new(
                "modelo-x",
                vec![Message::user("leia o manifesto")],
            ))
            .await
            .expect("chat deve responder");

        assert_eq!(resp.message.content, vec![ContentBlock::ToolCall(chamada)]);
    }

    #[tokio::test]
    async fn chat_stream_entrega_roteiro_na_ordem_e_fecha() {
        let mock = MockProvider::new("mock");
        let roteiro = vec![
            StreamEvent::MessageStart,
            StreamEvent::TextDelta { text: "o".into() },
            StreamEvent::TextDelta { text: "i".into() },
            StreamEvent::MessageEnd {
                usage: Usage {
                    input_tokens: 1,
                    output_tokens: 2,
                },
            },
        ];
        mock.enqueue_stream(roteiro.clone());

        let mut stream = mock
            .chat_stream(ChatRequest::new("modelo-x", vec![Message::user("oi")]))
            .await
            .expect("stream deve abrir");

        let mut recebidos = Vec::new();
        while let Some(evento) = stream.recv().await {
            recebidos.push(evento.expect("roteiro não contém erros"));
        }
        assert_eq!(
            recebidos, roteiro,
            "eventos na ordem do roteiro, depois fecha"
        );
    }

    #[tokio::test]
    async fn embeddings_devolve_vetores_enfileirados() {
        let mock = MockProvider::new("mock");
        mock.enqueue_embeddings(Ok(EmbeddingsResponse {
            vectors: vec![vec![0.1, 0.2], vec![0.3, 0.4]],
            usage: Usage::default(),
        }));

        let resp = mock
            .embeddings(EmbeddingsRequest {
                model: "embed-x".into(),
                input: vec!["a".into(), "b".into()],
            })
            .await
            .expect("embeddings deve responder");

        assert_eq!(resp.vectors.len(), 2);
    }

    #[tokio::test]
    async fn fila_vazia_devolve_erro_e_nao_panica() {
        let mock = MockProvider::new("mock");
        let erro = mock
            .chat(ChatRequest::new("modelo-x", vec![Message::user("oi")]))
            .await
            .expect_err("sem resposta enfileirada deve dar erro");
        assert!(matches!(erro, ProviderError::InvalidResponse(_)));
    }

    #[tokio::test]
    async fn trait_e_dyn_compatible() {
        // Garante em tempo de compilação e execução que o router (MT-09) poderá
        // guardar providers heterogêneos atrás de `dyn LlmProvider`.
        let mock = MockProvider::new("mock-dyn");
        mock.enqueue_chat(Ok(resposta_texto("via dyn")));
        let provider: Box<dyn LlmProvider> = Box::new(mock);

        assert_eq!(provider.name(), "mock-dyn");
        let resp = provider
            .chat(ChatRequest::new("modelo-x", vec![Message::user("oi")]))
            .await
            .expect("chat via dyn deve responder");
        assert_eq!(resp.message, Message::assistant("via dyn"));
    }
}
