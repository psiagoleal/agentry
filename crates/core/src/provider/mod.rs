// Caminho relativo: crates/core/src/provider/mod.rs
//! Camada de providers de LLM (MT-03).
//!
//! Define a [`LlmProvider`], a fronteira única pela qual o `agentry` conversa com
//! modelos (ADR-0001): chat, chat com streaming, *tool-calling* (via
//! [`ChatRequest::tools`] + blocos `ToolCall` na resposta) e embeddings.
//! [`mock::MockProvider`] é o provider de teste (MT-03); [`ollama::OllamaProvider`]
//! é o primeiro provider real, local, sobre o transporte único (MT-08).
//! [`openai_compat::OpenAiCompatProvider`] (MT-15) cobre vLLM/OpenRouter/gateways
//! LiteLLM sobre a mesma API OpenAI-compatible. O adapter Anthropic entra no
//! MT-16, sempre por cima do mesmo transporte auditável (ADR-0002).
//!
//! A trait é *dyn-compatible* (o router do MT-09 precisa de despacho dinâmico):
//! os métodos devolvem [`BoxFuture`] em vez de usar `async fn` nativo.

pub mod mock;
pub mod ollama;
pub mod openai_compat;

use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize};

use crate::model::{Message, StreamEvent, Usage};

/// Futuro empacotado usado pelos métodos da [`LlmProvider`].
///
/// Mantém a trait *dyn-compatible* sem depender do crate `async-trait`.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Canal de eventos de uma resposta em streaming.
///
/// O provider envia [`StreamEvent`]s (ou erro) e fecha o canal ao terminar;
/// o consumidor drena com `recv().await` até receber `None`.
pub type ChatStream = tokio::sync::mpsc::Receiver<Result<StreamEvent, ProviderError>>;

/// Especificação de uma tool oferecida ao modelo (*tool-calling*).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolSpec {
    /// Nome único da tool.
    pub name: String,
    /// Descrição do que a tool faz (orienta o modelo).
    pub description: String,
    /// JSON Schema dos argumentos aceitos.
    pub input_schema: serde_json::Value,
}

/// Requisição de chat a um provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Identificador do modelo no provider (ex.: `"llama3.1:8b"`).
    pub model: String,
    /// Histórico da conversa, em ordem.
    pub messages: Vec<Message>,
    /// Tools oferecidas ao modelo (vazio ⇒ sem *tool-calling*).
    #[serde(default)]
    pub tools: Vec<ToolSpec>,
    /// Limite de tokens de saída, se imposto.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Temperatura de amostragem, se definida (MT-31, ADR-0008).
    #[serde(default)]
    pub temperature: Option<f32>,
    /// *Top-p* (*nucleus sampling*), se definido (MT-31, ADR-0008).
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Ativa (ou desativa explicitamente) o raciocínio estendido do modelo,
    /// se ele suportar (MT-32, ADR-0014). Representação abstrata — cada
    /// adapter traduz para seu mecanismo nativo (Ollama: campo `think`).
    #[serde(default)]
    pub reasoning: Option<bool>,
}

impl ChatRequest {
    /// Cria uma requisição mínima (sem tools nem parâmetros de amostragem).
    #[must_use]
    pub fn new(model: impl Into<String>, messages: Vec<Message>) -> Self {
        Self {
            model: model.into(),
            messages,
            tools: Vec::new(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            reasoning: None,
        }
    }
}

/// Resposta completa (não-streaming) de um chat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatResponse {
    /// Mensagem produzida pelo modelo (pode conter blocos `ToolCall`).
    pub message: Message,
    /// Consumo de tokens da interação.
    pub usage: Usage,
}

/// Requisição de embeddings a um provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingsRequest {
    /// Identificador do modelo de embeddings no provider.
    pub model: String,
    /// Textos a vetorizar, em ordem.
    pub input: Vec<String>,
}

/// Resposta de embeddings: um vetor por texto de entrada, na mesma ordem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmbeddingsResponse {
    /// Vetores resultantes, na ordem de [`EmbeddingsRequest::input`].
    pub vectors: Vec<Vec<f32>>,
    /// Consumo de tokens da interação.
    pub usage: Usage,
}

/// Erros da camada de provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderError {
    /// Falha de rede/transporte ao alcançar o provider.
    Network(String),
    /// O provider respondeu com erro de API.
    Api {
        /// Código de status HTTP, quando houver.
        status: Option<u16>,
        /// Mensagem de erro reportada.
        message: String,
    },
    /// Resposta recebida mas fora do formato esperado.
    InvalidResponse(String),
    /// Capacidade não suportada por este provider (ex.: embeddings).
    Unsupported(String),
}

impl core::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Network(msg) => write!(f, "erro de rede: {msg}"),
            Self::Api {
                status: Some(s),
                message,
            } => write!(f, "erro de API (status {s}): {message}"),
            Self::Api {
                status: None,
                message,
            } => write!(f, "erro de API: {message}"),
            Self::InvalidResponse(msg) => write!(f, "resposta inválida: {msg}"),
            Self::Unsupported(msg) => write!(f, "não suportado: {msg}"),
        }
    }
}

impl std::error::Error for ProviderError {}

/// Contrato único de acesso a modelos de linguagem (ADR-0001).
///
/// Toda chamada a um LLM passa por uma implementação desta trait; nenhum código
/// fora da camada de providers fala com APIs de modelo diretamente. Implementações
/// reais fazem rede **exclusivamente** através do transporte único (ADR-0002).
pub trait LlmProvider: Send + Sync {
    /// Nome do provider (para roteamento, logs e audit trail).
    fn name(&self) -> &str;

    /// Envia uma conversa e recebe a resposta completa.
    fn chat(&self, request: ChatRequest) -> BoxFuture<'_, Result<ChatResponse, ProviderError>>;

    /// Envia uma conversa e recebe os eventos incrementais da resposta.
    fn chat_stream(&self, request: ChatRequest)
        -> BoxFuture<'_, Result<ChatStream, ProviderError>>;

    /// Vetoriza textos com um modelo de embeddings.
    fn embeddings(
        &self,
        request: EmbeddingsRequest,
    ) -> BoxFuture<'_, Result<EmbeddingsResponse, ProviderError>>;
}
