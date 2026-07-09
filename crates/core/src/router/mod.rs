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
//!
//! O Router também rastreia, por provider, o último modelo resolvido e
//! sinaliza troca de modelo (`ResolvedRoute::is_model_switch`, MT-17,
//! ADR-0009) — usado pelo adapter Ollama para escolher timeout frio/quente e
//! decidir `keep_alive`, já que só o Router sabe com antecedência qual
//! `(provider, modelo)` a próxima chamada vai usar. O rastreio é otimista
//! (assume que toda resolução será de fato usada) e exige `Mutex` porque
//! `resolve`/`resolve_with_override` continuam recebendo `&self` — o Router
//! é compartilhado (via `Arc`) entre chamadas potencialmente concorrentes.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

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
    /// Ativa (ou desativa explicitamente) o raciocínio estendido do modelo
    /// para esta `task-class`, se ele suportar (MT-32, ADR-0014).
    pub reasoning: Option<bool>,
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

/// Override de parâmetros de chamada em tempo real (MT-33, ADR-0014).
///
/// **Nunca** contém classe de egresso nem permissões — essas continuam
/// fixas pela resolução de [`crate::config::Config`] (MT-04) feita na
/// inicialização da sessão; nada aqui muda o que é *permitido*, só decide
/// **como** a chamada é feita dentro do que já foi permitido. `model`/
/// `provider`, em particular, só podem escolher entre os candidatos já
/// declarados na [`RouteEntry`] da `task-class` (ver
/// [`Router::resolve_with_override`]) — nunca um alvo novo, não vetado.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RuntimeOverride {
    /// Restringe a escolha aos candidatos deste provider, se definido.
    pub provider: Option<String>,
    /// Restringe a escolha aos candidatos deste modelo, se definido.
    pub model: Option<String>,
    /// Sobrescreve `temperature` do preset da `task-class`.
    pub temperature: Option<f32>,
    /// Sobrescreve `top_p` do preset da `task-class`.
    pub top_p: Option<f32>,
    /// Sobrescreve `system_prompt` do preset da `task-class`.
    pub system_prompt: Option<String>,
    /// Sobrescreve `max_tokens` do preset da `task-class`.
    pub max_tokens: Option<u32>,
    /// Sobrescreve `reasoning` do preset da `task-class`.
    pub reasoning: Option<bool>,
}

impl RuntimeOverride {
    /// Aplica este override por cima de `base`: campos definidos aqui
    /// vencem; ausentes caem no valor de `base`. Mesma convenção de
    /// precedência do MT-04 (`Settings::merged_over`) — use para combinar
    /// override de sessão (`base`) com override de chamada única (`self`).
    #[must_use]
    pub fn merged_over(self, base: Self) -> Self {
        Self {
            provider: self.provider.or(base.provider),
            model: self.model.or(base.model),
            temperature: self.temperature.or(base.temperature),
            top_p: self.top_p.or(base.top_p),
            system_prompt: self.system_prompt.or(base.system_prompt),
            max_tokens: self.max_tokens.or(base.max_tokens),
            reasoning: self.reasoning.or(base.reasoning),
        }
    }
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
    /// Indica se esta resolução implica **troca de modelo** no provider
    /// (MT-17, ADR-0009) em relação à última resolução para o mesmo
    /// provider — só o Router sabe disso com antecedência. `false` quando a
    /// rota é construída diretamente via [`Self::new`] (fora do Router,
    /// tipicamente em teste); use [`Self::with_model_switch`] para simular o
    /// sinal ligado.
    pub is_model_switch: bool,
}

impl ResolvedRoute {
    /// Cria uma rota já resolvida (normalmente devolvida por [`Router::resolve`]),
    /// com `is_model_switch: false` — quem precisa simular uma troca de
    /// modelo em teste usa [`Self::with_model_switch`].
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
            is_model_switch: false,
        }
    }

    /// Sobrescreve [`Self::is_model_switch`] — usado por testes que
    /// precisam simular uma troca de modelo sem passar pelo Router.
    #[must_use]
    pub fn with_model_switch(mut self, is_model_switch: bool) -> Self {
        self.is_model_switch = is_model_switch;
        self
    }
}

impl core::fmt::Debug for ResolvedRoute {
    // `LlmProvider` não exige `Debug` (MT-03); imprime só o nome do provider.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ResolvedRoute")
            .field("provider", &self.provider.name())
            .field("model", &self.model)
            .field("preset", &self.preset)
            .field("is_model_switch", &self.is_model_switch)
            .finish()
    }
}

/// O Router: mapeia `task-class` para candidatos e resolve contra os
/// providers registrados e a classe de egresso ativa da sessão.
pub struct Router {
    routes: HashMap<String, RouteEntry>,
    providers: HashMap<String, Arc<dyn LlmProvider>>,
    egress_class: EgressClass,
    /// Último modelo resolvido por provider (MT-17, ADR-0009) — rastreio
    /// otimista usado só para sinalizar `is_model_switch`; não afeta a
    /// decisão de roteamento em si.
    last_model: Mutex<HashMap<String, String>>,
}

impl Router {
    /// Cria um Router vazio sob a classe de egresso ativa dada.
    #[must_use]
    pub fn new(egress_class: EgressClass) -> Self {
        Self {
            routes: HashMap::new(),
            providers: HashMap::new(),
            egress_class,
            last_model: Mutex::new(HashMap::new()),
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
        self.resolve_with_override(task_class, &RuntimeOverride::default())
    }

    /// Resolve a `task-class` como [`Self::resolve`], mas aplicando um
    /// [`RuntimeOverride`] (MT-33, ADR-0014) por cima do preset e, se
    /// `provider`/`model` estiverem definidos no override, restringindo a
    /// escolha **apenas** aos candidatos já declarados na [`RouteEntry`] que
    /// casem com eles — nunca um alvo novo, não vetado.
    ///
    /// A checagem de classe de egresso continua idêntica à de
    /// [`Self::resolve`] para o candidato escolhido: o override **nunca**
    /// contorna o *fail-closed* do ADR-0002 — se o candidato pedido exigir
    /// mais do que a classe ativa permite, a resolução falha como qualquer
    /// outra, mesmo que o usuário tenha pedido aquele modelo explicitamente.
    ///
    /// # Errors
    ///
    /// Mesmos casos de [`Self::resolve`]; também
    /// [`RouterError::NoAvailableRoute`] se o override pedir um
    /// `provider`/`model` que não está entre os candidatos declarados para
    /// a `task-class`.
    pub fn resolve_with_override(
        &self,
        task_class: &str,
        overrides: &RuntimeOverride,
    ) -> Result<ResolvedRoute, RouterError> {
        let entry = self
            .routes
            .get(task_class)
            .ok_or_else(|| RouterError::UnknownTaskClass(task_class.to_string()))?;

        let candidatos = entry.candidates.iter().filter(|candidato| {
            overrides
                .provider
                .as_deref()
                .map_or(true, |p| p == candidato.provider)
                && overrides
                    .model
                    .as_deref()
                    .map_or(true, |m| m == candidato.model)
        });

        for candidate in candidatos {
            if !self.egress_class.permits(candidate.egress_class) {
                continue;
            }
            if let Some(provider) = self.providers.get(&candidate.provider) {
                let preset = CallPreset {
                    temperature: overrides.temperature.or(entry.preset.temperature),
                    top_p: overrides.top_p.or(entry.preset.top_p),
                    system_prompt: overrides
                        .system_prompt
                        .clone()
                        .or_else(|| entry.preset.system_prompt.clone()),
                    max_tokens: overrides.max_tokens.or(entry.preset.max_tokens),
                    reasoning: overrides.reasoning.or(entry.preset.reasoning),
                };
                let is_model_switch = self.mark_resolved(&candidate.provider, &candidate.model);
                return Ok(ResolvedRoute {
                    provider: Arc::clone(provider),
                    model: candidate.model.clone(),
                    preset,
                    is_model_switch,
                });
            }
        }

        Err(RouterError::NoAvailableRoute {
            task_class: task_class.to_string(),
        })
    }

    /// Registra `model` como o último modelo resolvido para `provider` e
    /// devolve se isso representa uma **troca** em relação ao valor anterior
    /// (MT-17, ADR-0009). Rastreio otimista: assume que toda resolução será
    /// de fato usada para uma chamada.
    fn mark_resolved(&self, provider: &str, model: &str) -> bool {
        let mut last_model = self
            .last_model
            .lock()
            .expect("mutex do Router não deve envenenar");
        let trocou = last_model.get(provider).map(String::as_str) != Some(model);
        last_model.insert(provider.to_string(), model.to_string());
        trocou
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
            reasoning: Some(true),
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

    #[test]
    fn override_de_temperature_sobrescreve_o_preset_da_task_class() {
        let mut router = router_com_providers(EgressClass::LocalOnly, &["ollama"]);
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "ollama",
                    "llama3.1:8b",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset {
                    temperature: Some(0.5),
                    ..CallPreset::default()
                },
            },
        );

        let overrides = RuntimeOverride {
            temperature: Some(0.9),
            ..RuntimeOverride::default()
        };
        let rota = router
            .resolve_with_override("chat", &overrides)
            .expect("deve resolver");

        assert_eq!(rota.preset.temperature, Some(0.9));
    }

    #[test]
    fn campos_ausentes_no_override_caem_no_preset_da_task_class() {
        let mut router = router_com_providers(EgressClass::LocalOnly, &["ollama"]);
        let preset = CallPreset {
            temperature: Some(0.2),
            top_p: Some(0.9),
            system_prompt: Some("padrão".into()),
            max_tokens: Some(2048),
            reasoning: Some(true),
        };
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "ollama",
                    "llama3.1:8b",
                    EgressClass::LocalOnly,
                )],
                preset: preset.clone(),
            },
        );

        let rota = router
            .resolve_with_override("chat", &RuntimeOverride::default())
            .expect("deve resolver");

        assert_eq!(rota.preset, preset);
    }

    #[test]
    fn override_de_model_escolhe_candidato_especifico_ja_declarado() {
        let mut router = router_com_providers(EgressClass::LocalOnly, &["ollama"]);
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![
                    RouteTarget::new("ollama", "llama3.1:8b", EgressClass::LocalOnly),
                    RouteTarget::new("ollama", "qwen2.5-coder:14b", EgressClass::LocalOnly),
                ],
                preset: CallPreset::default(),
            },
        );

        let overrides = RuntimeOverride {
            model: Some("qwen2.5-coder:14b".into()),
            ..RuntimeOverride::default()
        };
        let rota = router
            .resolve_with_override("chat", &overrides)
            .expect("deve resolver o candidato pedido");

        assert_eq!(rota.model, "qwen2.5-coder:14b");
    }

    #[test]
    fn override_de_model_que_viola_classe_de_egresso_e_bloqueado() {
        // A sessão está em local-only; o override pede explicitamente o
        // candidato de nuvem — mesmo pedido explicitamente, não pode passar.
        let mut router = router_com_providers(EgressClass::LocalOnly, &["anthropic"]);
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "anthropic",
                    "claude-x",
                    EgressClass::CloudOk,
                )],
                preset: CallPreset::default(),
            },
        );

        let overrides = RuntimeOverride {
            model: Some("claude-x".into()),
            ..RuntimeOverride::default()
        };
        let erro = router
            .resolve_with_override("chat", &overrides)
            .expect_err("override nunca deve contornar o fail-closed do ADR-0002");

        assert_eq!(
            erro,
            RouterError::NoAvailableRoute {
                task_class: "chat".into()
            }
        );
    }

    #[test]
    fn override_de_model_inexistente_entre_candidatos_e_erro() {
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

        let overrides = RuntimeOverride {
            model: Some("modelo-nao-declarado".into()),
            ..RuntimeOverride::default()
        };
        let erro = router
            .resolve_with_override("chat", &overrides)
            .expect_err("modelo não declarado como candidato deve falhar");

        assert_eq!(
            erro,
            RouterError::NoAvailableRoute {
                task_class: "chat".into()
            }
        );
    }

    #[test]
    fn merged_over_da_precedencia_ao_override_mais_especifico() {
        let sessao = RuntimeOverride {
            temperature: Some(0.5),
            model: Some("modelo-da-sessao".into()),
            ..RuntimeOverride::default()
        };
        let chamada_unica = RuntimeOverride {
            temperature: Some(0.1),
            ..RuntimeOverride::default()
        };

        let efetivo = chamada_unica.merged_over(sessao);

        assert_eq!(efetivo.temperature, Some(0.1), "chamada única vence");
        assert_eq!(
            efetivo.model,
            Some("modelo-da-sessao".into()),
            "ausente na chamada única cai na sessão"
        );
    }

    #[test]
    fn is_model_switch_e_true_na_primeira_resolucao_do_provider() {
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

        let rota = router.resolve("chat").expect("deve resolver");
        assert!(
            rota.is_model_switch,
            "primeira resolução do provider deve sinalizar troca"
        );
    }

    #[test]
    fn is_model_switch_e_false_ao_resolver_o_mesmo_modelo_de_novo() {
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

        router.resolve("chat").expect("primeira resolução");
        let segunda = router.resolve("chat").expect("segunda resolução");
        assert!(
            !segunda.is_model_switch,
            "mesmo modelo do mesmo provider não deve sinalizar troca"
        );
    }

    #[test]
    fn is_model_switch_e_true_ao_trocar_de_modelo_no_mesmo_provider() {
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
        router.resolve("chat").expect("primeira resolução");

        router.set_route(
            "outra-task",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "ollama",
                    "qwen2.5-coder:14b",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );
        let rota = router.resolve("outra-task").expect("deve resolver");
        assert!(
            rota.is_model_switch,
            "modelo diferente no mesmo provider deve sinalizar troca"
        );
    }

    #[test]
    fn is_model_switch_e_rastreado_por_provider_independentemente() {
        let mut router =
            router_com_providers(EgressClass::LocalOnly, &["ollama", "outro-provider"]);
        router.set_route(
            "chat-a",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "ollama",
                    "modelo-x",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );
        router.set_route(
            "chat-b",
            RouteEntry {
                candidates: vec![RouteTarget::new(
                    "outro-provider",
                    "modelo-x",
                    EgressClass::LocalOnly,
                )],
                preset: CallPreset::default(),
            },
        );

        router
            .resolve("chat-a")
            .expect("primeira resolução do provider ollama");
        let rota_b = router
            .resolve("chat-b")
            .expect("primeira resolução do outro provider");
        assert!(
            rota_b.is_model_switch,
            "primeira resolução de um provider diferente também é troca, mesmo que outro \
             provider já tenha visto esse mesmo nome de modelo"
        );
    }
}
