// Caminho relativo: crates/core/src/router/mod.rs
//! Router / Policy Engine (MT-09).
//!
//! Mapeia uma `task-class` (identificador livre, definido pelo perfil/projeto)
//! para `(provider, modelo, classe de egresso)`, com fallback por
//! disponibilidade, e resolve os presets de parâmetros de chamada por
//! `task-class` do ADR-0008 (`temperature`/`top_p`/`system_prompt`/`max_tokens`).
//!
//! Aplica a mesma disciplina **fail-closed** já usada pela allowlist de
//! egresso (MT-05, ADR-0002): uma `task-class` só é roteada para um candidato
//! cuja classe de egresso mínima seja **coberta** pela classe ativa da sessão
//! ([`EgressClass::permits`]). Isso vale mesmo que o provider de nuvem esteja
//! disponível e registrado — uma tarefa marcada sensível (classe ativa
//! `local-only`) nunca alcança um candidato que exija `cloud-ok`, porque o
//! candidato é descartado antes de qualquer checagem de disponibilidade.

use std::collections::HashMap;
use std::sync::Arc;

use crate::config::privacy::EgressClass;
use crate::provider::LlmProvider;

/// Preset de parâmetros de chamada por `task-class` (ADR-0008).
///
/// Nenhum campo é obrigatório: ausência cai no *default* do provider.
/// `system_prompt` aqui é só o texto padrão a usar — cabe a quem consome o
/// preset (o agent loop, MT-10) antepô-lo à conversa como `Message::system`
/// comum (MT-02); o preset não inventa um formato de mensagem próprio.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CallPreset {
    /// Temperatura de amostragem, se definida.
    pub temperature: Option<f32>,
    /// *Top-p* (*nucleus sampling*), se definido.
    pub top_p: Option<f32>,
    /// Texto de *system prompt* padrão para esta `task-class`.
    pub system_prompt: Option<String>,
    /// Limite de tokens de saída, se definido.
    pub max_tokens: Option<u32>,
}

/// Um candidato de roteamento: provider nomeado (via [`LlmProvider::name`]) +
/// modelo, com a classe de egresso mínima que ele exige.
#[derive(Debug, Clone, PartialEq)]
pub struct RouteTarget {
    /// Nome do provider (deve casar com [`LlmProvider::name`] do provider registrado).
    pub provider: String,
    /// Identificador do modelo nesse provider.
    pub model: String,
    /// Classe de egresso mínima exigida para alcançar este candidato.
    pub egress_class: EgressClass,
}

impl RouteTarget {
    /// Cria um candidato de roteamento.
    #[must_use]
    pub fn new(
        provider: impl Into<String>,
        model: impl Into<String>,
        egress_class: EgressClass,
    ) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
            egress_class,
        }
    }
}

/// Entrada de roteamento de uma `task-class`: candidatos em ordem de
/// preferência (o primeiro cuja classe é permitida **e** cujo provider está
/// registrado vence) mais o preset de chamada.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RouteEntry {
    /// Candidatos, do mais preferido ao menos preferido.
    pub candidates: Vec<RouteTarget>,
    /// Preset de parâmetros de chamada desta `task-class`.
    pub preset: CallPreset,
}

/// Erros de roteamento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouterError {
    /// Nenhuma [`RouteEntry`] cadastrada para esta `task-class`.
    UnknownTaskClass(String),
    /// Existe entrada, mas nenhum candidato passou (classe insuficiente ou
    /// provider não registrado) sob a classe de egresso ativa.
    NoAvailableRoute {
        /// A `task-class` que falhou a resolver.
        task_class: String,
    },
}

impl core::fmt::Display for RouterError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownTaskClass(tc) => write!(f, "task-class desconhecida: '{tc}'"),
            Self::NoAvailableRoute { task_class } => write!(
                f,
                "nenhuma rota disponível para a task-class '{task_class}' sob a classe de \
                 egresso ativa da sessão"
            ),
        }
    }
}

impl std::error::Error for RouterError {}

/// Uma rota já resolvida: provider pronto para uso, modelo e preset de chamada.
pub struct ResolvedRoute {
    /// Provider a usar.
    pub provider: Arc<dyn LlmProvider>,
    /// Modelo a usar nesse provider.
    pub model: String,
    /// Preset de parâmetros de chamada da `task-class` resolvida.
    pub preset: CallPreset,
}

impl ResolvedRoute {
    /// Cria uma rota já resolvida (normalmente devolvida por [`Router::resolve`]).
    #[must_use]
    pub fn new(
        provider: Arc<dyn LlmProvider>,
        model: impl Into<String>,
        preset: CallPreset,
    ) -> Self {
        Self {
            provider,
            model: model.into(),
            preset,
        }
    }
}

impl core::fmt::Debug for ResolvedRoute {
    // `LlmProvider` não exige `Debug` (MT-03); imprime só o nome do provider.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResolvedRoute")
            .field("provider", &self.provider.name())
            .field("model", &self.model)
            .field("preset", &self.preset)
            .finish()
    }
}

/// O Router: mapeia `task-class` para candidatos e resolve contra os
/// providers registrados e a classe de egresso ativa da sessão.
pub struct Router {
    routes: HashMap<String, RouteEntry>,
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    egress_class: EgressClass,
}

impl Router {
    /// Cria um Router vazio sob a classe de egresso ativa dada.
    #[must_use]
    pub fn new(egress_class: EgressClass) -> Self {
        Self {
            routes: HashMap::new(),
            providers: HashMap::new(),
            egress_class,
        }
    }

    /// Registra um provider disponível (chave: [`LlmProvider::name`]).
    pub fn register_provider(&mut self, provider: Arc<dyn LlmProvider>) {
        self.providers.insert(provider.name().to_string(), provider);
    }

    /// Define (ou substitui) a entrada de roteamento de uma `task-class`.
    pub fn set_route(&mut self, task_class: impl Into<String>, entry: RouteEntry) {
        self.routes.insert(task_class.into(), entry);
    }

    /// Resolve a `task-class` para uma rota utilizável.
    ///
    /// Percorre os candidatos na ordem declarada; o primeiro cuja classe de
    /// egresso mínima é coberta pela classe ativa **e** cujo provider está
    /// registrado vence. Um candidato que exige mais do que a classe ativa
    /// permite é descartado **antes** de checar disponibilidade — por isso
    /// uma tarefa sensível nunca alcança um provider de nuvem, mesmo que ele
    /// esteja registrado e disponível.
    ///
    /// # Errors
    ///
    /// Devolve [`RouterError::UnknownTaskClass`] se a `task-class` não tiver
    /// entrada cadastrada; [`RouterError::NoAvailableRoute`] se nenhum
    /// candidato passar (todos exigem classe insuficiente ou não têm
    /// provider registrado).
    pub fn resolve(&self, task_class: &str) -> Result<ResolvedRoute, RouterError> {
        let entry = self
            .routes
            .get(task_class)
            .ok_or_else(|| RouterError::UnknownTaskClass(task_class.to_string()))?;

        for candidate in &entry.candidates {
            if !self.egress_class.permits(candidate.egress_class) {
                continue;
            }
            if let Some(provider) = self.providers.get(&candidate.provider) {
                return Ok(ResolvedRoute {
                    provider: Arc::clone(provider),
                    model: candidate.model.clone(),
                    preset: entry.preset.clone(),
                });
            }
        }

        Err(RouterError::NoAvailableRoute {
            task_class: task_class.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider::mock::MockProvider;

    fn router_com_providers(egress_class: EgressClass, nomes: &[&str]) -> Router {
        let mut router = Router::new(egress_class);
        for nome in nomes {
            router.register_provider(Arc::new(MockProvider::new(*nome)));
        }
        router
    }

    #[test]
    fn roteia_pela_classe_de_egresso_ativa() {
        let mut router = router_com_providers(EgressClass::LocalOnly, &["ollama"]);
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "ollama",
                    "llama3.1:8b",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );

        let rota = router
            .resolve("chat")
            .expect("deve resolver sob local-only");
        assert_eq!(rota.provider.name(), "ollama");
        assert_eq!(rota.model, "llama3.1:8b");
    }

    #[test]
    fn tarefa_sensivel_nunca_roteia_para_provider_de_nuvem_mesmo_disponivel() {
        // "anthropic" está registrado e disponível, mas é o único candidato e
        // exige cloud-ok; a sessão está em local-only.
        let mut router = router_com_providers(EgressClass::LocalOnly, &["anthropic"]);
        router.set_route(
            "dados-sensiveis",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "anthropic",
                    "claude-x",
                    EgressClass::CloudOk,
                )],
                preset: CallPreset::default(),
            },
        );

        let erro = router
            .resolve("dados-sensiveis")
            .expect_err("nunca deve rotear tarefa sensível para provider de nuvem");
        assert_eq!(
            erro,
            RouterError::NoAvailableRoute {
                task_class: "dados-sensiveis".into()
            }
        );
    }

    #[test]
    fn fallback_por_indisponibilidade_funciona() {
        // "anthropic" não está registrado (indisponível); "ollama" está.
        // A sessão permite nuvem, mas o fallback deve cair no candidato local.
        let mut router = router_com_providers(EgressClass::CloudOk, &["ollama"]);
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![
                    RouteTarget::new("anthropic", "claude-x", EgressClass::CloudOk),
                    RouteTarget::new("ollama", "llama3.1:8b", EgressClass::LocalOnly),
                ],
                preset: CallPreset::default(),
            },
        );

        let rota = router
            .resolve("chat")
            .expect("deve cair no fallback disponível");
        assert_eq!(rota.provider.name(), "ollama");
        assert_eq!(rota.model, "llama3.1:8b");
    }

    #[test]
    fn preset_de_task_class_e_aplicado() {
        let mut router = router_com_providers(EgressClass::LocalOnly, &["ollama"]);
        let preset = CallPreset {
            temperature: Some(0.2),
            top_p: Some(0.9),
            system_prompt: Some("Você é um assistente de código.".into()),
            max_tokens: Some(2048),
        };
        router.set_route(
            "code-gen",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "ollama",
                    "qwen2.5-coder:14b",
                    EgressClass::LocalOnly,
                )],
                preset: preset.clone(),
            },
        );

        let rota = router.resolve("code-gen").expect("deve resolver");
        assert_eq!(rota.preset, preset);
    }

    #[test]
    fn task_class_desconhecida_e_erro() {
        let router = router_com_providers(EgressClass::CloudOk, &[]);
        let erro = router
            .resolve("nao-existe")
            .expect_err("task-class sem entrada deve falhar");
        assert_eq!(erro, RouterError::UnknownTaskClass("nao-existe".into()));
    }

    #[test]
    fn todos_os_candidatos_bloqueados_ou_indisponiveis_e_erro() {
        // "ollama" exige classe que a sessão não tem; "anthropic" nem está registrado.
        let mut router = router_com_providers(EgressClass::LocalOnly, &["ollama-outro-nome"]);
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![
                    RouteTarget::new("anthropic", "claude-x", EgressClass::CloudOk),
                    RouteTarget::new("ollama", "llama3.1:8b", EgressClass::CloudOptOut),
                ],
                preset: CallPreset::default(),
            },
        );

        let erro = router
            .resolve("chat")
            .expect_err("nenhum candidato deveria passar");
        assert_eq!(
            erro,
            RouterError::NoAvailableRoute {
                task_class: "chat".into()
            }
        );
    }
}
