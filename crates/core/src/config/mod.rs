// Caminho relativo: crates/core/src/config/mod.rs
//! Configuração em camadas (MT-04): perfil → projeto → ambiente.
//!
//! Consome o **mínimo** do `settings-schema:1` (ADR-0003): parâmetros de modelo
//! e permissões `deny`/`ask`; mais a primeira fatia do artefato
//! `.agentry/agentry.settings.json` (ADR-0018): as flags de contexto
//! (`context.repoMap`/`semanticRag`/`lspGrounding`) e provider
//! (`providers.ollama.structuredOutput`); mais o schema do Guardrail Gate
//! (`guardrails.input`/`guardrails.output`, MT-44/ADR-0007). O merge segue
//! três regras:
//!
//! 1. **Campo escalar:** a camada mais específica vence (env > arquivo > perfil).
//! 2. **Permissões:** **união** entre camadas — um `deny` herdado nunca é
//!    removido por uma camada mais específica (fail-closed, ADR-0002).
//! 3. **Regras de guardrail:** união por `id` — regra nova é adicionada; o
//!    mesmo `id` em duas camadas resolve para a ação mais severa
//!    (`GuardrailAction::rank`, `block` > `redact`), nunca a mais permissiva
//!    (ADR-0007 §3).
//!
//! [`Settings::from_file`] localiza e carrega `.agentry/agentry.settings.json`
//! (MT-39): ausência não é erro (usa os *defaults* de cada ADR de origem — todos
//! `true` para a fatia do ADR-0018 §5); JSON presente e malformado, ou com
//! `schemaVersion` divergente, é a mesma falha fail-closed abaixo.
//!
//! Versão de schema divergente da suportada ⇒ [`ConfigError::UnsupportedSchema`]
//! (abortar com mensagem explícita, nunca degradar silenciosamente — ADR-0003).

pub mod privacy;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::guardrail::{GuardrailGate, GuardrailRule};
use crate::router::{CallPreset, RouteEntry, RouteTarget};
use privacy::{EgressClass, Profile};

/// Versão do `settings-schema` suportada por este binário (contrato interop v1).
pub const SUPPORTED_SETTINGS_SCHEMA: u32 = 1;

/// Prefixo das variáveis de ambiente reconhecidas.
pub const ENV_PREFIX: &str = "AGENTRY_";

/// Erros de carga/validação de configuração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// O artefato declara uma versão de schema que este binário não suporta.
    UnsupportedSchema {
        /// Versão encontrada no artefato.
        found: u32,
        /// Versão suportada ([`SUPPORTED_SETTINGS_SCHEMA`]).
        supported: u32,
    },
    /// Conteúdo malformado (JSON inválido ou campo com tipo errado).
    Parse(String),
}

impl core::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnsupportedSchema { found, supported } => write!(
                f,
                "settings-schema:{found} não suportado (este binário suporta \
                 settings-schema:{supported}); abortando por fail-closed (ADR-0003)"
            ),
            Self::Parse(msg) => write!(f, "configuração malformada: {msg}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Permissões mínimas do `settings-schema:1`: padrões de comando/tool.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Permissions {
    /// Sempre bloqueado.
    #[serde(default)]
    pub deny: Vec<String>,
    /// Requer confirmação explícita.
    #[serde(default)]
    pub ask: Vec<String>,
}

impl Permissions {
    /// União com outra camada, sem duplicatas e preservando a ordem
    /// (herdadas primeiro). `deny`/`ask` só crescem entre camadas.
    fn union(mut self, overlay: Self) -> Self {
        for d in overlay.deny {
            if !self.deny.contains(&d) {
                self.deny.push(d);
            }
        }
        for a in overlay.ask {
            if !self.ask.contains(&a) {
                self.ask.push(a);
            }
        }
        self
    }
}

/// Uma capacidade `{ "enabled": bool }` do bloco `context.*` (ADR-0018 §5).
/// Ausente/`None` ⇒ a camada não opina; o *default* (`true`) é aplicado só em
/// [`Config::resolve`], nunca aqui — uma camada intermediária vazia não pode
/// "desligar" o que uma camada mais específica ainda vai declarar.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FeatureToggle {
    /// `true`/`false` explícito; `None` ⇒ herda da camada anterior.
    #[serde(default)]
    pub enabled: Option<bool>,
}

impl FeatureToggle {
    fn merged_over(self, base: Self) -> Self {
        Self {
            enabled: self.enabled.or(base.enabled),
        }
    }
}

/// Bloco `context.*` do schema mínimo (ADR-0018 §5): flags de contexto já
/// mecanicamente prontas no código (repo-map/RAG semântico/LSP-grounding).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ContextSettings {
    /// `context.repoMap.enabled` (ADR-0010).
    #[serde(default, rename = "repoMap")]
    pub repo_map: FeatureToggle,
    /// `context.semanticRag.enabled` (ADR-0011).
    #[serde(default, rename = "semanticRag")]
    pub semantic_rag: FeatureToggle,
    /// `context.lspGrounding.enabled` (ADR-0013).
    #[serde(default, rename = "lspGrounding")]
    pub lsp_grounding: FeatureToggle,
    /// `context.gitignore.enabled` (ADR-0020 §3) — respeito **opcional** a
    /// `.gitignore`, em união com `.agentryignore`/`.claudeignore` (nunca em
    /// substituição). Ausente ⇒ `false` (`Config::resolve`) — reduzir
    /// ruído de contexto é opt-in, nunca muda o comportamento de quem não
    /// configurou nada.
    #[serde(default, rename = "gitignore")]
    pub gitignore: FeatureToggle,
    /// `context.agentsFile.enabled` (ADR-0023) — leitura de `AGENTS.md`
    /// (primário) ou `CLAUDE.md` (*fallback*, nunca os dois) como instruções
    /// de projeto, injetadas na mensagem de sistema. Ausente ⇒ `true`
    /// (`Config::resolve`) — mesma categoria de custo baixo/benefício alto
    /// das três primeiras flags acima (leitura local de um arquivo pequeno).
    #[serde(default, rename = "agentsFile")]
    pub agents_file: FeatureToggle,
}

impl ContextSettings {
    fn merged_over(self, base: Self) -> Self {
        Self {
            repo_map: self.repo_map.merged_over(base.repo_map),
            semantic_rag: self.semantic_rag.merged_over(base.semantic_rag),
            lsp_grounding: self.lsp_grounding.merged_over(base.lsp_grounding),
            gitignore: self.gitignore.merged_over(base.gitignore),
            agents_file: self.agents_file.merged_over(base.agents_file),
        }
    }
}

/// Configuração específica do provider Ollama, dentro de `providers.ollama`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OllamaSettings {
    /// `providers.ollama.structuredOutput` (ADR-0012).
    #[serde(default, rename = "structuredOutput")]
    pub structured_output: Option<bool>,
}

impl OllamaSettings {
    fn merged_over(self, base: Self) -> Self {
        Self {
            structured_output: self.structured_output.or(base.structured_output),
        }
    }
}

/// Configuração do endpoint LiteLLM (ADR-0006), dentro de `providers.litellm`.
///
/// A chave de API **nunca** entra aqui — vem de variável de ambiente
/// (`AGENTRY_LITELLM_API_KEY`, MT-49), nunca do arquivo de configuração
/// (mesma disciplina de "segredo nunca versionado" do projeto).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct LiteLlmSettings {
    /// `providers.litellm.baseUrl` — URL base do gateway (ex.:
    /// `https://litellm.minhaempresa.com`).
    #[serde(default, rename = "baseUrl")]
    pub base_url: Option<String>,
    /// `providers.litellm.model` — identificador do modelo nesse gateway.
    #[serde(default)]
    pub model: Option<String>,
    /// `providers.litellm.egressClass` — classe de egresso deste endpoint
    /// (ADR-0006: sempre explícita por endpoint de proxy; ausente ⇒
    /// `Config::resolve` aplica o *default* `cloud-ok` de risco, nunca
    /// inferido do host aqui).
    #[serde(default, rename = "egressClass")]
    pub egress_class: Option<EgressClass>,
}

impl LiteLlmSettings {
    fn merged_over(self, base: Self) -> Self {
        Self {
            base_url: self.base_url.or(base.base_url),
            model: self.model.or(base.model),
            egress_class: self.egress_class.or(base.egress_class),
        }
    }
}

/// Bloco `providers.*` do schema mínimo (ADR-0018 §5).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ProvidersSettings {
    /// `providers.ollama`.
    #[serde(default)]
    pub ollama: OllamaSettings,
    /// `providers.litellm` (ADR-0006).
    #[serde(default)]
    pub litellm: LiteLlmSettings,
}

impl ProvidersSettings {
    fn merged_over(self, base: Self) -> Self {
        Self {
            ollama: self.ollama.merged_over(base.ollama),
            litellm: self.litellm.merged_over(base.litellm),
        }
    }
}

/// Bloco `guardrails.*` do schema mínimo (ADR-0007 §2, MT-44) — regras de
/// correspondência determinística do Guardrail Gate (MT-43), aplicadas na
/// entrada (mensagem de usuário) e na saída (resposta do turno) de uma
/// chamada de LLM. Reaproveita [`GuardrailRule`] literalmente (mesmo tipo
/// dos dois lados: regra em memória e regra vinda do artefato) — sem um
/// tipo paralelo só para o JSON.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct GuardrailSettings {
    /// `guardrails.input` — checado contra a mensagem de usuário mais
    /// recente, antes de qualquer chamada ao provider.
    #[serde(default)]
    pub input: Vec<GuardrailRule>,
    /// `guardrails.output` — checado contra a resposta do turno, antes do
    /// Reviewer (ADR-0015).
    #[serde(default)]
    pub output: Vec<GuardrailRule>,
}

impl GuardrailSettings {
    fn merged_over(self, base: Self) -> Self {
        Self {
            input: merge_regras_de_guardrail(base.input, self.input),
            output: merge_regras_de_guardrail(base.output, self.output),
        }
    }
}

/// Une duas listas de regras de guardrail por `id` (ADR-0007 §3): regra nova
/// (`id` inédito) é sempre adicionada; o mesmo `id` presente nas duas listas
/// resolve para a ação mais severa (`GuardrailAction::rank`, `block` >
/// `redact`) — nunca a mais permissiva, mesmo espírito de
/// `Permissions::union` generalizado para severidade em vez de só
/// crescimento de lista.
fn merge_regras_de_guardrail(
    base: Vec<GuardrailRule>,
    overlay: Vec<GuardrailRule>,
) -> Vec<GuardrailRule> {
    let mut resultado = base;
    for regra_nova in overlay {
        match resultado.iter_mut().find(|regra| regra.id == regra_nova.id) {
            Some(existente) => {
                if regra_nova.action.rank() > existente.action.rank() {
                    existente.action = regra_nova.action;
                }
            }
            None => resultado.push(regra_nova),
        }
    }
    resultado
}

/// Um candidato de roteamento configurável para uma task-class
/// (`taskClasses.<nome>.candidates[]`, ADR-0021) — mesma forma de
/// [`RouteTarget`], que este tipo produz diretamente (`into_route_target`);
/// sem tipo novo de roteamento, só a camada de configuração sobre o que já
/// existe (ADR-0008/0014). Os três campos são obrigatórios dentro de um
/// candidato — um candidato sem `provider`/`model`/`egressClass` não é
/// utilizável, então a ausência de qualquer um é erro de parsing (fail
/// closed), não um valor *default* silencioso.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskClassCandidateSettings {
    pub provider: String,
    pub model: String,
    #[serde(rename = "egressClass")]
    pub egress_class: EgressClass,
}

impl TaskClassCandidateSettings {
    fn into_route_target(self) -> RouteTarget {
        RouteTarget::new(self.provider, self.model, self.egress_class)
    }
}

/// Preset de parâmetros de chamada configurável para uma task-class
/// (`taskClasses.<nome>.preset`, ADR-0021) — mesma forma de [`CallPreset`],
/// que este tipo produz diretamente (`into_call_preset`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskClassPresetSettings {
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default, rename = "topP")]
    pub top_p: Option<f32>,
    #[serde(default, rename = "maxTokens")]
    pub max_tokens: Option<u32>,
    #[serde(default, rename = "systemPrompt")]
    pub system_prompt: Option<String>,
    #[serde(default)]
    pub reasoning: Option<bool>,
}

impl TaskClassPresetSettings {
    fn merged_over(self, base: Self) -> Self {
        Self {
            temperature: self.temperature.or(base.temperature),
            top_p: self.top_p.or(base.top_p),
            max_tokens: self.max_tokens.or(base.max_tokens),
            system_prompt: self.system_prompt.or(base.system_prompt),
            reasoning: self.reasoning.or(base.reasoning),
        }
    }

    fn into_call_preset(self) -> CallPreset {
        CallPreset {
            temperature: self.temperature,
            top_p: self.top_p,
            system_prompt: self.system_prompt,
            max_tokens: self.max_tokens,
            reasoning: self.reasoning,
        }
    }
}

/// Uma task-class configurável por completo (`taskClasses.<nome>`,
/// ADR-0021): candidatos de roteamento, em ordem de preferência, + preset
/// de parâmetros de chamada.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TaskClassSettings {
    #[serde(default)]
    pub candidates: Vec<TaskClassCandidateSettings>,
    #[serde(default)]
    pub preset: TaskClassPresetSettings,
}

impl TaskClassSettings {
    fn merged_over(self, base: Self) -> Self {
        Self {
            candidates: merge_candidatos_de_task_class(base.candidates, self.candidates),
            preset: self.preset.merged_over(base.preset),
        }
    }

    fn into_route_entry(self) -> RouteEntry {
        RouteEntry {
            candidates: self
                .candidates
                .into_iter()
                .map(TaskClassCandidateSettings::into_route_target)
                .collect(),
            preset: self.preset.into_call_preset(),
        }
    }
}

/// Une duas listas de candidatos de task-class por par (`provider`,
/// `model`): candidato novo é adicionado; o mesmo par presente nas duas
/// listas resolve para a classe de egresso **mais restritiva** das duas —
/// nunca a mais permissiva (mesma disciplina fail-closed de
/// `merge_regras_de_guardrail`/MT-44, adaptada de "ação mais severa" para
/// "classe menos permissiva", `EgressClass::rank` — ADR-0021).
fn merge_candidatos_de_task_class(
    base: Vec<TaskClassCandidateSettings>,
    overlay: Vec<TaskClassCandidateSettings>,
) -> Vec<TaskClassCandidateSettings> {
    let mut resultado = base;
    for candidato_novo in overlay {
        match resultado
            .iter_mut()
            .find(|c| c.provider == candidato_novo.provider && c.model == candidato_novo.model)
        {
            Some(existente) => {
                if candidato_novo.egress_class.rank() < existente.egress_class.rank() {
                    existente.egress_class = candidato_novo.egress_class;
                }
            }
            None => resultado.push(candidato_novo),
        }
    }
    resultado
}

/// Une dois mapas de task-classes por nome (ADR-0021): task-class nova
/// (nome inédito) é sempre adicionada; o mesmo nome nas duas camadas
/// resolve via [`TaskClassSettings::merged_over`] (candidatos por par
/// provider/model, preset campo a campo).
fn merge_task_classes(
    base: HashMap<String, TaskClassSettings>,
    overlay: HashMap<String, TaskClassSettings>,
) -> HashMap<String, TaskClassSettings> {
    let mut resultado = base;
    for (nome, config_nova) in overlay {
        let mesclada = match resultado.remove(&nome) {
            Some(config_existente) => config_nova.merged_over(config_existente),
            None => config_nova,
        };
        resultado.insert(nome, mesclada);
    }
    resultado
}

/// Uma camada de configuração (mínimo do `settings-schema:1` + fatia do
/// ADR-0018).
///
/// Todos os campos são opcionais: uma camada só sobrescreve o que declara.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Versão do schema declarada pelo artefato (ausente ⇒ assume a suportada).
    ///
    /// Aceita tanto `schema_version` (convenção original da ADR-0003) quanto
    /// `schemaVersion` (grafia real do artefato `.agentry/agentry.settings.json`,
    /// ADR-0018 §5) — mesmo campo, duas grafias de origens distintas.
    #[serde(default, alias = "schemaVersion")]
    pub schema_version: Option<u32>,
    /// Identificador do perfil ativo (`empresa` / `externo-confidencial` / `pessoal`).
    #[serde(default)]
    pub profile: Option<String>,
    /// Modelo padrão para chat.
    #[serde(default)]
    pub model: Option<String>,
    /// Limite de tokens de saída.
    #[serde(default)]
    pub max_tokens: Option<u32>,
    /// Permissões `deny`/`ask`.
    #[serde(default)]
    pub permissions: Permissions,
    /// Bloco `context.*` (ADR-0018 §5).
    #[serde(default)]
    pub context: ContextSettings,
    /// Bloco `providers.*` (ADR-0018 §5).
    #[serde(default)]
    pub providers: ProvidersSettings,
    /// Bloco `guardrails.*` (ADR-0007 §2).
    #[serde(default)]
    pub guardrails: GuardrailSettings,
    /// Bloco `taskClasses` (ADR-0021) — task-classes configuráveis por
    /// nome. Ausência total ⇒ `Config` não sintetiza nenhuma automaticamente
    /// (decisão registrada em `docs/decisoes-autonomas.md`: sintetizar
    /// defaults concretos de provider/modelo é responsabilidade da CLI,
    /// MT-56 — `crates/core` não deve conhecer `"ollama"` como escolha de
    /// produto).
    #[serde(default, rename = "taskClasses")]
    pub task_classes: HashMap<String, TaskClassSettings>,
}

impl Settings {
    /// Interpreta uma camada a partir de JSON (`.claude/settings.json`),
    /// validando a versão de schema (fail-closed).
    pub fn from_json_str(json: &str) -> Result<Self, ConfigError> {
        let settings: Self =
            serde_json::from_str(json).map_err(|e| ConfigError::Parse(e.to_string()))?;
        if let Some(found) = settings.schema_version {
            if found != SUPPORTED_SETTINGS_SCHEMA {
                return Err(ConfigError::UnsupportedSchema {
                    found,
                    supported: SUPPORTED_SETTINGS_SCHEMA,
                });
            }
        }
        Ok(settings)
    }

    /// Monta a camada de ambiente a partir de pares `NOME=valor`.
    ///
    /// Reconhece `AGENTRY_PROFILE`, `AGENTRY_MODEL` e `AGENTRY_MAX_TOKENS`;
    /// valor numérico inválido é erro explícito (não é ignorado em silêncio).
    pub fn from_env_vars<I>(vars: I) -> Result<Self, ConfigError>
    where
        I: IntoIterator<Item = (String, String)>,
    {
        let mut camada = Self::default();
        for (nome, valor) in vars {
            match nome.as_str() {
                "AGENTRY_PROFILE" => camada.profile = Some(valor),
                "AGENTRY_MODEL" => camada.model = Some(valor),
                "AGENTRY_MAX_TOKENS" => {
                    let n = valor.parse::<u32>().map_err(|_| {
                        ConfigError::Parse(format!(
                            "AGENTRY_MAX_TOKENS deve ser inteiro positivo, veio {valor:?}"
                        ))
                    })?;
                    camada.max_tokens = Some(n);
                }
                _ => {}
            }
        }
        Ok(camada)
    }

    /// Monta a camada de ambiente lendo o ambiente do processo.
    pub fn from_process_env() -> Result<Self, ConfigError> {
        Self::from_env_vars(std::env::vars().filter(|(nome, _)| nome.starts_with(ENV_PREFIX)))
    }

    /// Localiza e interpreta `.agentry/agentry.settings.json` a partir da raiz
    /// resolvida por [`crate::state_dir::resolve_root`] (MT-38/ADR-0017,
    /// caminho exato via [`crate::state_dir::agentry_settings_path`]).
    ///
    /// Ausência do arquivo **não é erro** — devolve a camada vazia
    /// (`Settings::default`), mesmo espírito do "manifesto ausente" do MT-29
    /// (ADR-0018 §4): o projeto simplesmente usa os *defaults* de cada
    /// capacidade. JSON presente e malformado, ou com `schemaVersion`
    /// divergente da suportada, propaga o mesmo erro fail-closed de
    /// [`Self::from_json_str`] — nunca um *panic*.
    pub fn from_file(start: &std::path::Path) -> Result<Self, ConfigError> {
        let caminho = crate::state_dir::agentry_settings_path(start);
        match std::fs::read_to_string(&caminho) {
            Ok(json) => Self::from_json_str(&json),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(ConfigError::Parse(format!(
                "não foi possível ler {}: {e}",
                caminho.display()
            ))),
        }
    }

    /// Aplica esta camada por cima de `base`: escalares desta camada vencem;
    /// permissões são unidas (só crescem).
    #[must_use]
    pub fn merged_over(self, base: Self) -> Self {
        Self {
            schema_version: self.schema_version.or(base.schema_version),
            profile: self.profile.or(base.profile),
            model: self.model.or(base.model),
            max_tokens: self.max_tokens.or(base.max_tokens),
            permissions: base.permissions.union(self.permissions),
            context: self.context.merged_over(base.context),
            providers: self.providers.merged_over(base.providers),
            guardrails: self.guardrails.merged_over(base.guardrails),
            task_classes: merge_task_classes(base.task_classes, self.task_classes),
        }
    }
}

/// Endpoint LiteLLM resolvido (ADR-0006) — `base_url`/`model` já garantidos
/// presentes (`Config.litellm` só é `Some` quando os dois estão
/// declarados); `egress_class` já resolvido para o *default* de risco
/// (`cloud-ok`) quando a camada não declarou nenhum.
#[derive(Debug, Clone, PartialEq)]
pub struct LiteLlmConfig {
    pub base_url: String,
    pub model: String,
    pub egress_class: EgressClass,
}

/// Configuração final resolvida, pronta para o router e o transporte.
#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    /// Perfil reconhecido, se houver (desconhecido ⇒ `None`).
    pub profile: Option<Profile>,
    /// Classe de egresso resolvida (**fail-closed**: sem perfil válido ⇒ `local-only`).
    pub egress_class: EgressClass,
    /// Modelo padrão para chat, se definido.
    pub model: Option<String>,
    /// Limite de tokens de saída, se definido.
    pub max_tokens: Option<u32>,
    /// Permissões unificadas de todas as camadas.
    pub permissions: Permissions,
    /// `context.repoMap.enabled` (ADR-0010); nenhuma camada define ⇒ `true`.
    pub repo_map_enabled: bool,
    /// `context.semanticRag.enabled` (ADR-0011); nenhuma camada define ⇒ `true`.
    pub semantic_rag_enabled: bool,
    /// `context.lspGrounding.enabled` (ADR-0013); nenhuma camada define ⇒ `true`.
    pub lsp_grounding_enabled: bool,
    /// `context.gitignore.enabled` (ADR-0020 §3) — respeito opcional a
    /// `.gitignore`, em união com `.agentryignore`/`.claudeignore`; nenhuma
    /// camada define ⇒ `false` (opt-in, MT-53).
    pub respect_gitignore: bool,
    /// `context.agentsFile.enabled` (ADR-0023); nenhuma camada define ⇒
    /// `true`. Controla só a leitura de `AGENTS.md`/`CLAUDE.md`
    /// (`crates/core/src/project_instructions.rs`) — não afeta descoberta de
    /// skills (`.claude/skills/`), que não tem *opt-out* próprio.
    pub agents_file_enabled: bool,
    /// `providers.ollama.structuredOutput` (ADR-0012); nenhuma camada define ⇒ `true`.
    pub ollama_structured_output: bool,
    /// Guardrail Gate resolvido (`guardrails.input`/`guardrails.output`,
    /// ADR-0007) — nenhuma camada define ⇒ `GuardrailGate::default()` (sem
    /// regras, nada é checado). Consumido por `Session::with_guardrails`
    /// (MT-45).
    pub guardrails: GuardrailGate,
    /// Endpoint LiteLLM resolvido (`providers.litellm`, ADR-0006) — `None`
    /// quando `base_url` ou `model` não estão declarados (LiteLLM
    /// simplesmente não está configurado, não é um erro). Consumido pela
    /// CLI para registrar um segundo candidato de provider (MT-49).
    pub litellm: Option<LiteLlmConfig>,
    /// Task-classes declaradas pelo usuário (`taskClasses`, ADR-0021), já
    /// convertidas para os tipos do `Router` (`RouteEntry`/`RouteTarget`/
    /// `CallPreset`, ADR-0008/0014) — prontas para `Router::set_route`.
    /// Mapa vazio quando nada é declarado; `Config` **não** sintetiza
    /// nenhuma task-class interna (`chat`/`compact`/`guardrail-compliance`)
    /// — essa responsabilidade é da CLI (MT-56), que já tem os defaults
    /// concretos de provider/modelo (`crates/core` não deveria conhecer
    /// `"ollama"` como escolha de produto).
    pub task_classes: HashMap<String, RouteEntry>,
}

impl Config {
    /// Resolve a configuração final a partir das camadas, na ordem da menos
    /// específica para a mais específica (perfil, arquivo, ambiente).
    #[must_use]
    pub fn resolve(layers: Vec<Settings>) -> Self {
        let merged = layers
            .into_iter()
            .fold(Settings::default(), |acc, layer| layer.merged_over(acc));
        let profile = merged.profile.as_deref().and_then(Profile::parse);
        let egress_class = EgressClass::resolve(merged.profile.as_deref());
        Self {
            profile,
            egress_class,
            model: merged.model,
            max_tokens: merged.max_tokens,
            permissions: merged.permissions,
            repo_map_enabled: merged.context.repo_map.enabled.unwrap_or(true),
            semantic_rag_enabled: merged.context.semantic_rag.enabled.unwrap_or(true),
            lsp_grounding_enabled: merged.context.lsp_grounding.enabled.unwrap_or(true),
            respect_gitignore: merged.context.gitignore.enabled.unwrap_or(false),
            agents_file_enabled: merged.context.agents_file.enabled.unwrap_or(true),
            ollama_structured_output: merged.providers.ollama.structured_output.unwrap_or(true),
            guardrails: GuardrailGate {
                input: merged.guardrails.input,
                output: merged.guardrails.output,
            },
            litellm: match (
                merged.providers.litellm.base_url,
                merged.providers.litellm.model,
            ) {
                (Some(base_url), Some(model)) => Some(LiteLlmConfig {
                    base_url,
                    model,
                    egress_class: merged
                        .providers
                        .litellm
                        .egress_class
                        .unwrap_or(EgressClass::CloudOk),
                }),
                _ => None,
            },
            task_classes: merged
                .task_classes
                .into_iter()
                .map(|(nome, config)| (nome, config.into_route_entry()))
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guardrail::GuardrailAction;

    fn camada_perfil(profile: &str) -> Settings {
        Settings {
            profile: Some(profile.into()),
            ..Settings::default()
        }
    }

    #[test]
    fn perfil_empresa_resolve_local_only() {
        let cfg = Config::resolve(vec![camada_perfil("empresa")]);
        assert_eq!(cfg.profile, Some(Profile::Empresa));
        assert_eq!(cfg.egress_class, EgressClass::LocalOnly);
    }

    #[test]
    fn perfil_pessoal_resolve_cloud_ok() {
        let cfg = Config::resolve(vec![camada_perfil("pessoal")]);
        assert_eq!(cfg.egress_class, EgressClass::CloudOk);
    }

    #[test]
    fn sem_perfil_falha_fechado_em_local_only() {
        let cfg = Config::resolve(vec![Settings::default()]);
        assert_eq!(cfg.profile, None);
        assert_eq!(cfg.egress_class, EgressClass::LocalOnly);
    }

    #[test]
    fn perfil_ambiguo_falha_fechado_em_local_only() {
        let cfg = Config::resolve(vec![camada_perfil("producao")]);
        assert_eq!(cfg.profile, None);
        assert_eq!(cfg.egress_class, EgressClass::LocalOnly);
    }

    #[test]
    fn camada_mais_especifica_vence_nos_escalares() {
        let perfil = Settings {
            profile: Some("empresa".into()),
            model: Some("llama3.1:8b".into()),
            max_tokens: Some(1024),
            ..Settings::default()
        };
        let projeto = Settings {
            model: Some("qwen2.5-coder:14b".into()),
            ..Settings::default()
        };
        let env = Settings {
            max_tokens: Some(4096),
            ..Settings::default()
        };

        let cfg = Config::resolve(vec![perfil, projeto, env]);
        assert_eq!(cfg.egress_class, EgressClass::LocalOnly, "perfil herdado");
        assert_eq!(cfg.model.as_deref(), Some("qwen2.5-coder:14b"));
        assert_eq!(cfg.max_tokens, Some(4096));
    }

    #[test]
    fn permissoes_sao_uniao_e_deny_nunca_encolhe() {
        let perfil = Settings {
            permissions: Permissions {
                deny: vec!["rm -rf".into()],
                ask: vec!["git push".into()],
            },
            ..Settings::default()
        };
        // Camada de projeto tenta "esvaziar" as permissões: não pode encolher.
        let projeto = Settings {
            permissions: Permissions {
                deny: vec!["curl".into()],
                ask: vec![],
            },
            ..Settings::default()
        };

        let cfg = Config::resolve(vec![perfil, projeto]);
        assert_eq!(cfg.permissions.deny, vec!["rm -rf", "curl"]);
        assert_eq!(cfg.permissions.ask, vec!["git push"]);
    }

    #[test]
    fn json_minimo_do_settings_schema_1() {
        let json = r#"{
            "schema_version": 1,
            "profile": "empresa",
            "model": "llama3.1:8b",
            "permissions": { "deny": ["rm -rf"], "ask": ["git push"] }
        }"#;
        let settings = Settings::from_json_str(json).expect("schema 1 deve carregar");
        assert_eq!(settings.profile.as_deref(), Some("empresa"));
        assert_eq!(settings.permissions.deny, vec!["rm -rf"]);
    }

    #[test]
    fn versao_de_schema_divergente_aborta() {
        let erro = Settings::from_json_str(r#"{ "schema_version": 2 }"#)
            .expect_err("schema 2 deve ser rejeitado");
        assert_eq!(
            erro,
            ConfigError::UnsupportedSchema {
                found: 2,
                supported: SUPPORTED_SETTINGS_SCHEMA
            }
        );
        assert!(erro.to_string().contains("fail-closed"));
    }

    #[test]
    fn json_malformado_e_erro_explicito() {
        assert!(matches!(
            Settings::from_json_str("{ perfil: sem aspas }"),
            Err(ConfigError::Parse(_))
        ));
    }

    #[test]
    fn camada_de_ambiente_reconhece_variaveis() {
        let camada = Settings::from_env_vars([
            ("AGENTRY_PROFILE".to_string(), "pessoal".to_string()),
            ("AGENTRY_MODEL".to_string(), "claude-sonnet-5".to_string()),
            ("AGENTRY_MAX_TOKENS".to_string(), "2048".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ])
        .expect("variáveis válidas");
        assert_eq!(camada.profile.as_deref(), Some("pessoal"));
        assert_eq!(camada.model.as_deref(), Some("claude-sonnet-5"));
        assert_eq!(camada.max_tokens, Some(2048));
    }

    #[test]
    fn max_tokens_invalido_no_ambiente_e_erro_explicito() {
        let erro =
            Settings::from_env_vars([("AGENTRY_MAX_TOKENS".to_string(), "muitos".to_string())])
                .expect_err("número inválido não pode ser ignorado em silêncio");
        assert!(matches!(erro, ConfigError::Parse(_)));
    }

    #[test]
    fn json_do_artefato_agentry_settings_json_carrega_a_fatia_do_adr_0018() {
        // Exemplo exato da ADR-0018 §5: schemaVersion camelCase, $schema
        // ignorado (campo desconhecido), context/providers aninhados.
        let json = r#"{
            "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
            "schemaVersion": 1,
            "permissions": { "deny": [], "ask": ["shell_exec"] },
            "context": {
                "repoMap": { "enabled": true },
                "semanticRag": { "enabled": false },
                "lspGrounding": { "enabled": true }
            },
            "providers": { "ollama": { "structuredOutput": false } }
        }"#;
        let settings = Settings::from_json_str(json).expect("fatia do ADR-0018 deve carregar");
        assert_eq!(settings.schema_version, Some(1));
        assert_eq!(settings.context.repo_map.enabled, Some(true));
        assert_eq!(settings.context.semantic_rag.enabled, Some(false));
        assert_eq!(settings.context.lsp_grounding.enabled, Some(true));
        assert_eq!(settings.providers.ollama.structured_output, Some(false));
    }

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `state_dir::tests::TempDir`, MT-38).
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-config-test-{}-{}",
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

    #[test]
    fn from_file_com_arquivo_ausente_nao_e_erro_e_usa_defaults() {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.path().join(".git")).expect("cria .git");

        let camada = Settings::from_file(dir.path()).expect("ausência não é erro");
        assert_eq!(camada, Settings::default());
    }

    #[test]
    fn from_file_com_arquivo_presente_e_valido_le_corretamente() {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.path().join(".git")).expect("cria .git");
        let estado = dir.path().join(".agentry");
        std::fs::create_dir_all(&estado).expect("cria .agentry");
        std::fs::write(
            estado.join("agentry.settings.json"),
            r#"{
                "schemaVersion": 1,
                "permissions": { "deny": ["shell_exec"], "ask": [] },
                "context": { "semanticRag": { "enabled": false } }
            }"#,
        )
        .expect("escreve o artefato de configuração");

        let camada = Settings::from_file(dir.path()).expect("arquivo válido deve carregar");
        assert_eq!(camada.permissions.deny, vec!["shell_exec"]);
        assert_eq!(camada.context.semantic_rag.enabled, Some(false));
        assert_eq!(
            camada.context.repo_map.enabled, None,
            "campo ausente fica None"
        );
    }

    #[test]
    fn from_file_com_json_invalido_e_erro_tratado_nunca_panic() {
        let dir = TempDir::new();
        std::fs::create_dir_all(dir.path().join(".git")).expect("cria .git");
        let estado = dir.path().join(".agentry");
        std::fs::create_dir_all(&estado).expect("cria .agentry");
        std::fs::write(estado.join("agentry.settings.json"), "{ isto não é json }")
            .expect("escreve JSON malformado de propósito");

        let erro = Settings::from_file(dir.path()).expect_err("JSON malformado deve ser erro");
        assert!(matches!(erro, ConfigError::Parse(_)));
    }

    #[test]
    fn config_resolve_com_arquivo_ausente_usa_defaults_true_da_adr_0018() {
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(cfg.repo_map_enabled);
        assert!(cfg.semantic_rag_enabled);
        assert!(cfg.lsp_grounding_enabled);
        assert!(cfg.ollama_structured_output);
    }

    #[test]
    fn ausencia_de_context_gitignore_resolve_false_ador_0020_opt_in() {
        // Diferente das outras flags de context.* (default true): respeitar
        // .gitignore é opt-in — ausência nunca muda o comportamento de quem
        // não configurou nada (MT-53).
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(!cfg.respect_gitignore);
    }

    #[test]
    fn context_gitignore_enabled_true_resolve_respect_gitignore_true() {
        let camada =
            Settings::from_json_str(r#"{ "context": { "gitignore": { "enabled": true } } }"#)
                .expect("JSON válido");
        let cfg = Config::resolve(vec![camada]);
        assert!(cfg.respect_gitignore);
    }

    #[test]
    fn ausencia_de_context_agents_file_resolve_true_mesma_categoria_das_outras_tres() {
        // ADR-0023: diferente de gitignore (opt-in), agentsFile segue o
        // default true das três primeiras flags de context.* (custo baixo).
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(cfg.agents_file_enabled);
    }

    #[test]
    fn context_agents_file_enabled_false_resolve_agents_file_enabled_false() {
        let camada =
            Settings::from_json_str(r#"{ "context": { "agentsFile": { "enabled": false } } }"#)
                .expect("JSON válido");
        let cfg = Config::resolve(vec![camada]);
        assert!(!cfg.agents_file_enabled);
    }

    #[test]
    fn env_sobrescreve_o_arquivo_quando_ambos_definem_o_mesmo_campo() {
        let arquivo = Settings::from_json_str(
            r#"{ "context": { "semanticRag": { "enabled": true } }, "providers": { "ollama": { "structuredOutput": true } } }"#,
        )
        .expect("arquivo válido");
        let env = Settings::from_json_str(
            r#"{ "context": { "semanticRag": { "enabled": false } }, "providers": { "ollama": { "structuredOutput": false } } }"#,
        )
        .expect("ambiente válido");

        // Ordem de camadas do Config::resolve: perfil < arquivo < ambiente.
        let cfg = Config::resolve(vec![Settings::default(), arquivo, env]);
        assert!(!cfg.semantic_rag_enabled, "ambiente deve vencer o arquivo");
        assert!(
            !cfg.ollama_structured_output,
            "ambiente deve vencer o arquivo"
        );
    }

    #[test]
    fn json_com_guardrails_input_e_output_carrega_corretamente() {
        let json = r#"{
            "guardrails": {
                "input": [
                    { "id": "bloqueia-senha", "match": "senha:", "action": "block" }
                ],
                "output": [
                    { "id": "mascara-host", "match": "internal.corp", "action": "redact" }
                ]
            }
        }"#;
        let settings = Settings::from_json_str(json).expect("guardrails devem carregar");

        assert_eq!(settings.guardrails.input.len(), 1);
        assert_eq!(settings.guardrails.input[0].id, "bloqueia-senha");
        assert_eq!(settings.guardrails.input[0].match_text, "senha:");
        assert_eq!(settings.guardrails.input[0].action, GuardrailAction::Block);

        assert_eq!(settings.guardrails.output.len(), 1);
        assert_eq!(
            settings.guardrails.output[0].action,
            GuardrailAction::Redact
        );
    }

    #[test]
    fn camada_mais_especifica_adiciona_regra_de_id_novo_sem_apagar_as_herdadas() {
        let perfil = Settings {
            guardrails: GuardrailSettings {
                input: vec![GuardrailRule::new(
                    "regra-do-perfil",
                    "x",
                    GuardrailAction::Redact,
                )],
                output: vec![],
            },
            ..Settings::default()
        };
        let arquivo = Settings {
            guardrails: GuardrailSettings {
                input: vec![GuardrailRule::new(
                    "regra-do-arquivo",
                    "y",
                    GuardrailAction::Block,
                )],
                output: vec![],
            },
            ..Settings::default()
        };

        let cfg = Config::resolve(vec![perfil, arquivo]);

        assert_eq!(cfg.guardrails.input.len(), 2, "as duas regras sobrevivem");
        assert!(cfg
            .guardrails
            .input
            .iter()
            .any(|r| r.id == "regra-do-perfil"));
        assert!(cfg
            .guardrails
            .input
            .iter()
            .any(|r| r.id == "regra-do-arquivo"));
    }

    #[test]
    fn mesmo_id_em_duas_camadas_resolve_para_a_acao_mais_severa_nas_duas_ordens() {
        // Perfil redact, arquivo block (mais severo) — vence o arquivo.
        let perfil = Settings {
            guardrails: GuardrailSettings {
                input: vec![GuardrailRule::new(
                    "mesma-regra",
                    "x",
                    GuardrailAction::Redact,
                )],
                output: vec![],
            },
            ..Settings::default()
        };
        let arquivo = Settings {
            guardrails: GuardrailSettings {
                input: vec![GuardrailRule::new(
                    "mesma-regra",
                    "x",
                    GuardrailAction::Block,
                )],
                output: vec![],
            },
            ..Settings::default()
        };
        let cfg = Config::resolve(vec![perfil, arquivo]);
        assert_eq!(cfg.guardrails.input.len(), 1);
        assert_eq!(cfg.guardrails.input[0].action, GuardrailAction::Block);

        // Ordem invertida: perfil block, arquivo redact (mais fraco) — o
        // bloqueio herdado nunca é afrouxado.
        let perfil = Settings {
            guardrails: GuardrailSettings {
                input: vec![GuardrailRule::new(
                    "mesma-regra",
                    "x",
                    GuardrailAction::Block,
                )],
                output: vec![],
            },
            ..Settings::default()
        };
        let arquivo = Settings {
            guardrails: GuardrailSettings {
                input: vec![GuardrailRule::new(
                    "mesma-regra",
                    "x",
                    GuardrailAction::Redact,
                )],
                output: vec![],
            },
            ..Settings::default()
        };
        let cfg = Config::resolve(vec![perfil, arquivo]);
        assert_eq!(cfg.guardrails.input.len(), 1);
        assert_eq!(
            cfg.guardrails.input[0].action,
            GuardrailAction::Block,
            "camada mais específica nunca afrouxa uma regra herdada"
        );
    }

    #[test]
    fn ausencia_da_chave_guardrails_nao_e_erro_e_nao_gera_nenhuma_regra() {
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(cfg.guardrails.input.is_empty());
        assert!(cfg.guardrails.output.is_empty());
    }

    // --- MT-48: schema `providers.litellm` (ADR-0006) ---

    #[test]
    fn litellm_completo_resolve_os_tres_campos_exatos() {
        let json = r#"{
            "providers": {
                "litellm": {
                    "baseUrl": "https://litellm.minhaempresa.com",
                    "model": "empresa/gpt-30b",
                    "egressClass": "cloud-opt-out"
                }
            }
        }"#;
        let camada = Settings::from_json_str(json).expect("JSON válido");
        let cfg = Config::resolve(vec![camada]);

        let litellm = cfg
            .litellm
            .expect("providers.litellm completo deve resolver Some");
        assert_eq!(litellm.base_url, "https://litellm.minhaempresa.com");
        assert_eq!(litellm.model, "empresa/gpt-30b");
        assert_eq!(litellm.egress_class, EgressClass::CloudOptOut);
    }

    #[test]
    fn litellm_sem_egress_class_resolve_cloud_ok_ador_0006_fail_closed_invertido() {
        let json = r#"{
            "providers": {
                "litellm": {
                    "baseUrl": "http://litellm.interno:4000",
                    "model": "time-a/modelo-30b"
                }
            }
        }"#;
        let camada = Settings::from_json_str(json).expect("JSON válido");
        let cfg = Config::resolve(vec![camada]);

        let litellm = cfg
            .litellm
            .expect("base_url + model presentes devem resolver Some");
        assert_eq!(
            litellm.egress_class,
            EgressClass::CloudOk,
            "ADR-0006: ausência de classe declarada é tratada como cloud-ok (risco), \
             nunca inferida como local-only pelo host"
        );
    }

    #[test]
    fn litellm_com_apenas_base_url_ou_apenas_model_resolve_none() {
        let so_base_url =
            Settings::from_json_str(r#"{ "providers": { "litellm": { "baseUrl": "http://x" } } }"#)
                .expect("JSON válido");
        assert!(Config::resolve(vec![so_base_url]).litellm.is_none());

        let so_model =
            Settings::from_json_str(r#"{ "providers": { "litellm": { "model": "m" } } }"#)
                .expect("JSON válido");
        assert!(Config::resolve(vec![so_model]).litellm.is_none());
    }

    #[test]
    fn ausencia_do_bloco_litellm_resolve_none_comportamento_atual_preservado() {
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(cfg.litellm.is_none());
    }

    #[test]
    fn litellm_camada_mais_especifica_sobrescreve_campo_a_campo() {
        let arquivo = Settings::from_json_str(
            r#"{ "providers": { "litellm": {
                "baseUrl": "http://arquivo",
                "model": "modelo-arquivo",
                "egressClass": "local-only"
            } } }"#,
        )
        .expect("arquivo válido");
        let env =
            Settings::from_json_str(r#"{ "providers": { "litellm": { "model": "modelo-env" } } }"#)
                .expect("ambiente válido");

        // Ordem de camadas do Config::resolve: perfil < arquivo < ambiente.
        let cfg = Config::resolve(vec![Settings::default(), arquivo, env]);
        let litellm = cfg.litellm.expect("deve resolver Some");
        assert_eq!(
            litellm.base_url, "http://arquivo",
            "ambiente não declarou baseUrl — arquivo continua valendo"
        );
        assert_eq!(
            litellm.model, "modelo-env",
            "ambiente deve vencer o arquivo"
        );
        assert_eq!(
            litellm.egress_class,
            EgressClass::LocalOnly,
            "ambiente não declarou egressClass — arquivo continua valendo"
        );
    }

    // --- MT-55: schema `taskClasses` (ADR-0021) ---

    #[test]
    fn task_class_completa_resolve_route_entry_com_candidatos_e_preset_exatos() {
        let json = r#"{
            "taskClasses": {
                "revisao": {
                    "candidates": [
                        { "provider": "litellm", "model": "gpt-30b", "egressClass": "local-only" },
                        { "provider": "ollama", "model": "llama3.1:8b", "egressClass": "local-only" }
                    ],
                    "preset": {
                        "temperature": 0.2,
                        "topP": 0.9,
                        "maxTokens": 4096,
                        "systemPrompt": "revise com cuidado",
                        "reasoning": true
                    }
                }
            }
        }"#;
        let camada = Settings::from_json_str(json).expect("JSON válido");
        let cfg = Config::resolve(vec![camada]);

        let entry = cfg
            .task_classes
            .get("revisao")
            .expect("task-class 'revisao' deve estar presente");
        assert_eq!(entry.candidates.len(), 2);
        assert_eq!(entry.candidates[0].provider, "litellm");
        assert_eq!(entry.candidates[0].model, "gpt-30b");
        assert_eq!(entry.candidates[0].egress_class, EgressClass::LocalOnly);
        assert_eq!(entry.candidates[1].provider, "ollama");
        assert_eq!(entry.preset.temperature, Some(0.2));
        assert_eq!(entry.preset.top_p, Some(0.9));
        assert_eq!(entry.preset.max_tokens, Some(4096));
        assert_eq!(
            entry.preset.system_prompt.as_deref(),
            Some("revise com cuidado")
        );
        assert_eq!(entry.preset.reasoning, Some(true));
    }

    #[test]
    fn ausencia_de_task_classes_resolve_mapa_vazio_sem_sintetizar_nada() {
        // Decisão registrada em docs/decisoes-autonomas.md: Config não
        // sintetiza chat/compact/guardrail-compliance — isso é
        // responsabilidade da CLI (MT-56), que conhece os defaults
        // concretos de provider/modelo.
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(cfg.task_classes.is_empty());
    }

    #[test]
    fn task_class_declarada_em_camada_mais_especifica_sobrescreve_preset_do_mesmo_nome() {
        let arquivo = Settings::from_json_str(
            r#"{ "taskClasses": { "chat": {
                "candidates": [{ "provider": "ollama", "model": "modelo-arquivo", "egressClass": "local-only" }],
                "preset": { "temperature": 0.5 }
            } } }"#,
        )
        .expect("arquivo válido");
        let env = Settings::from_json_str(
            r#"{ "taskClasses": { "chat": { "preset": { "temperature": 0.1 } } } }"#,
        )
        .expect("ambiente válido");

        let cfg = Config::resolve(vec![Settings::default(), arquivo, env]);
        let entry = cfg.task_classes.get("chat").expect("deve existir");
        assert_eq!(
            entry.preset.temperature,
            Some(0.1),
            "ambiente deve vencer o arquivo no mesmo campo"
        );
        assert_eq!(
            entry.candidates[0].model, "modelo-arquivo",
            "ambiente não redeclarou candidates — candidato do arquivo continua"
        );
    }

    #[test]
    fn merge_por_nome_adiciona_task_class_nova_sem_apagar_a_herdada() {
        let arquivo = Settings::from_json_str(
            r#"{ "taskClasses": { "chat": {
                "candidates": [{ "provider": "ollama", "model": "m", "egressClass": "local-only" }]
            } } }"#,
        )
        .expect("arquivo válido");
        let env = Settings::from_json_str(
            r#"{ "taskClasses": { "compact": {
                "candidates": [{ "provider": "ollama", "model": "m2", "egressClass": "local-only" }]
            } } }"#,
        )
        .expect("ambiente válido");

        let cfg = Config::resolve(vec![Settings::default(), arquivo, env]);
        assert!(cfg.task_classes.contains_key("chat"), "herdada do arquivo");
        assert!(cfg.task_classes.contains_key("compact"), "nova do ambiente");
    }

    #[test]
    fn mesmo_candidato_em_duas_camadas_nunca_afrouxa_a_classe_de_egresso() {
        let perfil = Settings::from_json_str(
            r#"{ "taskClasses": { "chat": {
                "candidates": [{ "provider": "litellm", "model": "m", "egressClass": "local-only" }]
            } } }"#,
        )
        .expect("perfil válido");
        let arquivo = Settings::from_json_str(
            r#"{ "taskClasses": { "chat": {
                "candidates": [{ "provider": "litellm", "model": "m", "egressClass": "cloud-ok" }]
            } } }"#,
        )
        .expect("arquivo válido");

        let cfg = Config::resolve(vec![perfil, arquivo]);
        let entry = cfg.task_classes.get("chat").expect("deve existir");
        assert_eq!(
            entry.candidates.len(),
            1,
            "mesmo par provider/model, não duplica"
        );
        assert_eq!(
            entry.candidates[0].egress_class,
            EgressClass::LocalOnly,
            "camada mais específica nunca afrouxa a classe de um candidato já herdado"
        );

        // Ordem invertida: perfil cloud-ok, arquivo local-only (mais
        // restrito) — a mais restrita sempre vence, nas duas ordens.
        let perfil = Settings::from_json_str(
            r#"{ "taskClasses": { "chat": {
                "candidates": [{ "provider": "litellm", "model": "m", "egressClass": "cloud-ok" }]
            } } }"#,
        )
        .expect("perfil válido");
        let arquivo = Settings::from_json_str(
            r#"{ "taskClasses": { "chat": {
                "candidates": [{ "provider": "litellm", "model": "m", "egressClass": "local-only" }]
            } } }"#,
        )
        .expect("arquivo válido");
        let cfg = Config::resolve(vec![perfil, arquivo]);
        assert_eq!(
            cfg.task_classes.get("chat").unwrap().candidates[0].egress_class,
            EgressClass::LocalOnly
        );
    }
}
