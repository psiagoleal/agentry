// Caminho relativo: crates/core/src/session/reviewer.rs
//! Reviewer — auditoria semântica de tarefas via `task-class` dedicada
//! (MT-34, ADR-0015).
//!
//! Cada tipo de auditoria ([`AuditKind`]) é, para efeitos de roteamento,
//! uma `task-class` própria (`"review-<tipo>"`), resolvida pelo `Router`
//! (MT-09) como qualquer outra — nenhuma infraestrutura nova. O veredito
//! estruturado (ADR-0012) é obtido enquadrando-o como uma **tool-call**
//! (`submit_review(verdict, notes)`), não texto solto: é o único jeito de
//! reaproveitar de verdade o mecanismo de saída estruturada já existente
//! (hoje só ativo em `OllamaProvider` quando `ChatRequest.tools` não é
//! vazio) sem tocar `provider/ollama.rs` nem inventar *parsing* de JSON
//! solto — a mesma técnica que o *reranking* do MT-28 usou não se aplica
//! bem aqui porque não há um encaixe natural de tool-call ali, ao
//! contrário de "envie seu veredito".
//!
//! Disparo (pós-`Done`, modos `advisory`/`blocking`) e integração ao agent
//! loop ficam em `session/mod.rs` — o MT-35. Este módulo só monta a
//! requisição de um tipo de auditoria e interpreta o veredito devolvido.

use crate::model::{ContentBlock, Message};
use crate::provider::{ChatRequest, ProviderError, ToolSpec};
use crate::router::{Router, RouterError};

const TOOL_SUBMIT_REVIEW: &str = "submit_review";

/// Tipo de auditoria — lista inicial do ADR-0015, extensível.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditKind {
    Correctness,
    Security,
    GuardrailCompliance,
    TaskCompletion,
}

impl AuditKind {
    /// `task-class` própria desta auditoria, resolvida pelo Router (MT-09).
    #[must_use]
    pub fn task_class(self) -> &'static str {
        match self {
            Self::Correctness => "review-correctness",
            Self::Security => "review-security",
            Self::GuardrailCompliance => "review-guardrail-compliance",
            Self::TaskCompletion => "review-task-completion",
        }
    }

    fn instrucao(self) -> &'static str {
        match self {
            Self::Correctness => {
                "Avalie se o resultado abaixo está correto em relação ao que foi pedido."
            }
            Self::Security => {
                "Avalie se o resultado abaixo introduz algum risco de segurança (ex.: \
                 segredos expostos, comando destrutivo, vulnerabilidade introduzida)."
            }
            Self::GuardrailCompliance => {
                "Avalie se o resultado abaixo respeita as diretrizes/guardrails combinados \
                 para esta tarefa."
            }
            Self::TaskCompletion => {
                "Avalie se o resultado abaixo cumpre integralmente o que foi pedido, sem \
                 deixar partes incompletas."
            }
        }
    }
}

/// Veredito de uma auditoria.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Veredito {
    Pass,
    Fail,
}

/// Resultado de uma auditoria concluída.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewResult {
    pub kind: AuditKind,
    pub veredito: Veredito,
    pub notas: String,
}

/// Erros do Reviewer — nenhum indica um problema no artefato revisado em
/// si, e sim uma falha de infraestrutura (roteamento, provider) ou uma
/// resposta do modelo que não segue o protocolo esperado (sem chamar
/// `submit_review`, ou com um `verdict` fora de `pass`/`fail`).
#[derive(Debug, Clone, PartialEq)]
pub enum ReviewerError {
    Router(RouterError),
    Provider(ProviderError),
    /// O modelo respondeu sem chamar a tool `submit_review`.
    VeredictoAusente,
    /// A tool `submit_review` foi chamada, mas os argumentos não formam um
    /// veredito válido (`verdict` ausente/fora de `pass`/`fail`).
    VeredictoInvalido(String),
}

impl std::fmt::Display for ReviewerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Router(e) => write!(f, "falha ao rotear a auditoria: {e}"),
            Self::Provider(e) => write!(f, "provider da auditoria falhou: {e}"),
            Self::VeredictoAusente => {
                write!(
                    f,
                    "modelo não chamou '{TOOL_SUBMIT_REVIEW}' para dar o veredito"
                )
            }
            Self::VeredictoInvalido(msg) => write!(f, "veredito inválido: {msg}"),
        }
    }
}

impl std::error::Error for ReviewerError {}

fn tool_spec_submit_review() -> ToolSpec {
    ToolSpec {
        name: TOOL_SUBMIT_REVIEW.to_string(),
        description: "Envia o veredito desta auditoria.".to_string(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "verdict": {
                    "type": "string",
                    "enum": ["pass", "fail"],
                    "description": "Veredito da auditoria."
                },
                "notes": {
                    "type": "string",
                    "description": "Notas explicando o veredito."
                }
            },
            "required": ["verdict", "notes"]
        }),
    }
}

fn montar_prompt(kind: AuditKind, instrucao_original: &str, artefato: &str) -> String {
    format!(
        "{instrucao_kind}\n\nInstrução original da tarefa:\n{instrucao_original}\n\n\
         Resultado a avaliar:\n{artefato}\n\nEnvie seu veredito via a tool '{TOOL_SUBMIT_REVIEW}'.",
        instrucao_kind = kind.instrucao(),
    )
}

/// Roda a auditoria `kind` sobre `artefato` (o resultado final da tarefa),
/// dado `instrucao_original` (o pedido original) para contexto — resolve a
/// `task-class` de `kind` via `router` e monta a requisição com a tool
/// `submit_review`, interpretando o veredito estruturado devolvido.
///
/// # Errors
///
/// Devolve [`ReviewerError::Router`] se a `task-class` de `kind` não
/// resolver; [`ReviewerError::Provider`] se a chamada de chat falhar;
/// [`ReviewerError::VeredictoAusente`]/[`ReviewerError::VeredictoInvalido`]
/// se a resposta não seguir o protocolo esperado.
pub async fn review(
    kind: AuditKind,
    router: &Router,
    instrucao_original: &str,
    artefato: &str,
) -> Result<ReviewResult, ReviewerError> {
    let route = router
        .resolve(kind.task_class())
        .map_err(ReviewerError::Router)?;

    let mut request = ChatRequest::new(
        route.model.clone(),
        vec![Message::user(montar_prompt(
            kind,
            instrucao_original,
            artefato,
        ))],
    );
    request.tools = vec![tool_spec_submit_review()];
    request.max_tokens = route.preset.max_tokens;
    request.temperature = route.preset.temperature;
    request.top_p = route.preset.top_p;
    request.is_model_switch = route.is_model_switch;

    let resposta = route
        .provider
        .chat(request)
        .await
        .map_err(ReviewerError::Provider)?;

    let chamada = resposta
        .message
        .content
        .iter()
        .find_map(|bloco| match bloco {
            ContentBlock::ToolCall(chamada) if chamada.name == TOOL_SUBMIT_REVIEW => Some(chamada),
            _ => None,
        })
        .ok_or(ReviewerError::VeredictoAusente)?;

    let veredito_str = chamada
        .arguments
        .get("verdict")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            ReviewerError::VeredictoInvalido("campo 'verdict' ausente ou inválido".to_string())
        })?;
    let veredito = match veredito_str {
        "pass" => Veredito::Pass,
        "fail" => Veredito::Fail,
        outro => {
            return Err(ReviewerError::VeredictoInvalido(format!(
                "valor de 'verdict' desconhecido: '{outro}'"
            )))
        }
    };
    let notas = chamada
        .arguments
        .get("notes")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    Ok(ReviewResult {
        kind,
        veredito,
        notas,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::privacy::EgressClass;
    use crate::model::{Role, ToolCall, Usage};
    use crate::provider::mock::MockProvider;
    use crate::provider::ChatResponse;
    use crate::router::{CallPreset, RouteEntry, RouteTarget};
    use std::sync::Arc;

    fn router_com_review(mock: Arc<MockProvider>, task_class: &str) -> Router {
        let mut router = Router::new(EgressClass::LocalOnly);
        router.register_provider(mock);
        router.set_route(
            task_class,
            RouteEntry {
                candidates: vec![RouteTarget::new("mock", "modelo-x", EgressClass::LocalOnly)],
                preset: CallPreset::default(),
            },
        );
        router
    }

    fn resposta_com_veredito(verdict: &str, notes: &str) -> ChatResponse {
        ChatResponse {
            message: Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: "call-1".into(),
                    name: TOOL_SUBMIT_REVIEW.to_string(),
                    arguments: serde_json::json!({ "verdict": verdict, "notes": notes }),
                })],
            },
            usage: Usage::default(),
        }
    }

    #[tokio::test]
    async fn monta_a_requisicao_certa_por_tipo_de_auditoria() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_com_veredito("pass", "tudo certo")));
        let router = router_com_review(mock.clone(), "review-security");

        review(
            AuditKind::Security,
            &router,
            "implemente a função soma",
            "fn soma(a: i32, b: i32) -> i32 { a + b }",
        )
        .await
        .expect("deve funcionar");

        let requests = mock.chat_requests();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].tools.len(), 1);
        assert_eq!(requests[0].tools[0].name, TOOL_SUBMIT_REVIEW);
        let texto = requests[0].messages[0].text_content();
        assert!(texto.contains("implemente a função soma"));
        assert!(texto.contains("fn soma"));
    }

    #[tokio::test]
    async fn interpreta_veredito_pass() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_com_veredito("pass", "ok")));
        let router = router_com_review(mock, "review-correctness");

        let resultado = review(AuditKind::Correctness, &router, "instrução", "artefato")
            .await
            .expect("deve funcionar");

        assert_eq!(resultado.veredito, Veredito::Pass);
        assert_eq!(resultado.notas, "ok");
        assert_eq!(resultado.kind, AuditKind::Correctness);
    }

    #[tokio::test]
    async fn interpreta_veredito_fail() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_com_veredito("fail", "faltou tratar erro")));
        let router = router_com_review(mock, "review-task-completion");

        let resultado = review(AuditKind::TaskCompletion, &router, "instrução", "artefato")
            .await
            .expect("deve funcionar");

        assert_eq!(resultado.veredito, Veredito::Fail);
        assert_eq!(resultado.notas, "faltou tratar erro");
    }

    #[tokio::test]
    async fn resposta_sem_tool_call_e_erro_veredicto_ausente() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(ChatResponse {
            message: Message::assistant("parece bom"),
            usage: Usage::default(),
        }));
        let router = router_com_review(mock, "review-security");

        let erro = review(AuditKind::Security, &router, "instrução", "artefato")
            .await
            .expect_err("resposta sem tool-call deve ser erro");

        assert!(matches!(erro, ReviewerError::VeredictoAusente));
    }

    #[tokio::test]
    async fn verdict_desconhecido_e_erro_tratado() {
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(resposta_com_veredito("talvez", "incerto")));
        let router = router_com_review(mock, "review-security");

        let erro = review(AuditKind::Security, &router, "instrução", "artefato")
            .await
            .expect_err("verdict desconhecido deve ser erro");

        assert!(matches!(erro, ReviewerError::VeredictoInvalido(_)));
    }

    #[tokio::test]
    async fn task_class_nao_roteada_e_erro_de_router() {
        let router = Router::new(EgressClass::LocalOnly); // nenhuma rota registrada

        let erro = review(AuditKind::Security, &router, "instrução", "artefato")
            .await
            .expect_err("task-class não roteada deve ser erro");

        assert!(matches!(erro, ReviewerError::Router(_)));
    }
}
