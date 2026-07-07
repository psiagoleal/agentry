// Caminho relativo: crates/core/src/config/mod.rs
//! Configuração em camadas (MT-04): perfil → projeto → ambiente.
//!
//! Consome o **mínimo** do `settings-schema:1` (ADR-0003): parâmetros de modelo
//! e permissões `deny`/`ask`. O merge segue duas regras:
//!
//! 1. **Campo escalar:** a camada mais específica vence (env > projeto > perfil).
//! 2. **Permissões:** **união** entre camadas — um `deny` herdado nunca é
//!    removido por uma camada mais específica (fail-closed, ADR-0002).
//!
//! Versão de schema divergente da suportada ⇒ [`ConfigError::UnsupportedSchema`]
//! (abortar com mensagem explícita, nunca degradar silenciosamente — ADR-0003).

pub mod privacy;

use serde::{Deserialize, Serialize};

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

/// Uma camada de configuração (mínimo do `settings-schema:1`).
///
/// Todos os campos são opcionais: uma camada só sobrescreve o que declara.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    /// Versão do schema declarada pelo artefato (ausente ⇒ assume a suportada).
    #[serde(default)]
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
        }
    }
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
}

impl Config {
    /// Resolve a configuração final a partir das camadas, na ordem da menos
    /// específica para a mais específica (perfil, projeto, ambiente).
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
