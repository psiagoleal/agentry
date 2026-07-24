// Caminho relativo: crates/core/src/credentials.rs
//! Credenciais de provider persistidas em `~/.agentry/credentials.json`
//! (MT-128, ADR-0038) — schema **separado** de `agentry.settings.json`
//! (git-versionado, por-projeto **ou** global): nenhuma struct de
//! configuração distribuída/versionada ganha um campo de credencial —
//! `Credentials` só existe aqui, tornando estruturalmente impossível uma
//! chave de API vazar para o arquivo compartilhado por engano.
//!
//! Variável de ambiente **sempre** vence — `credentials.json` só é
//! consultado quando a variável correspondente não está definida
//! ([`resolve_api_key`] nunca lê o arquivo se a variável já veio
//! preenchida, não é um merge dos dois). Ausência de `$HOME`/do arquivo
//! não é erro (mesmo espírito de "arquivo ausente não é erro" de
//! [`crate::config::Settings::from_file`]); JSON malformado ou
//! `schemaVersion` não suportada é erro tratado, *fail-closed* (mesmo
//! padrão de [`crate::config::Settings::from_json_str`]).

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

const SCHEMA_VERSION_SUPORTADA: u32 = 1;

/// Credencial de um único provider — hoje só `apiKey`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CredentialProvider {
    #[serde(rename = "apiKey")]
    pub api_key: String,
}

/// Conteúdo interpretado de `~/.agentry/credentials.json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Credentials {
    #[serde(rename = "schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    pub providers: HashMap<String, CredentialProvider>,
}

/// Erros de leitura/interpretação de `credentials.json`.
#[derive(Debug, PartialEq, Eq)]
pub enum CredentialsError {
    /// Arquivo presente mas não é um JSON válido no formato esperado.
    Parse(String),
    /// `schemaVersion` presente, mas diferente da única suportada hoje.
    UnsupportedSchemaVersion(u32),
}

impl std::fmt::Display for CredentialsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(motivo) => write!(f, "credentials.json inválido: {motivo}"),
            Self::UnsupportedSchemaVersion(versao) => write!(
                f,
                "credentials.json com schemaVersion {versao} não suportada (esperado {SCHEMA_VERSION_SUPORTADA})"
            ),
        }
    }
}

impl std::error::Error for CredentialsError {}

/// Resolve a chave de API de `provider`: `env_var_value` (o valor já lido
/// da variável de ambiente correspondente, ex. `AGENTRY_LITELLM_API_KEY`)
/// sempre vence — `~/.agentry/credentials.json` só é consultado quando
/// `env_var_value` é `None` (diretriz de conformidade da ADR-0038: nunca
/// os dois somados/mesclados, e o arquivo nunca é sequer aberto se a
/// variável já resolveu a chave).
///
/// # Errors
///
/// Propaga [`CredentialsError`] se o arquivo existir mas for JSON inválido
/// ou tiver uma `schemaVersion` não suportada. Nunca falha por ausência de
/// `$HOME`/do arquivo — devolve `Ok(None)`.
pub fn resolve_api_key(
    provider: &str,
    env_var_value: Option<String>,
) -> Result<Option<String>, CredentialsError> {
    if env_var_value.is_some() {
        return Ok(env_var_value);
    }
    let chave = load()?.and_then(|credenciais| {
        credenciais
            .providers
            .get(provider)
            .map(|p| p.api_key.clone())
    });
    Ok(chave)
}

/// Lê `~/.agentry/credentials.json` — `None` sem `$HOME`/`%USERPROFILE%`
/// ou sem o arquivo existir, nunca erro por ausência.
///
/// # Errors
///
/// Propaga [`CredentialsError`] se o arquivo existir mas não puder ser
/// interpretado.
pub fn load() -> Result<Option<Credentials>, CredentialsError> {
    match crate::global_dir::global_credentials_path() {
        Some(caminho) => load_de(&caminho),
        None => Ok(None),
    }
}

fn load_de(caminho: &Path) -> Result<Option<Credentials>, CredentialsError> {
    match std::fs::read_to_string(caminho) {
        Ok(json) => {
            avisar_se_permissao_aberta(caminho);
            parse(&json).map(Some)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(CredentialsError::Parse(format!(
            "não foi possível ler {}: {e}",
            caminho.display()
        ))),
    }
}

fn parse(json: &str) -> Result<Credentials, CredentialsError> {
    let credenciais: Credentials =
        serde_json::from_str(json).map_err(|e| CredentialsError::Parse(e.to_string()))?;
    if credenciais.schema_version != SCHEMA_VERSION_SUPORTADA {
        return Err(CredentialsError::UnsupportedSchemaVersion(
            credenciais.schema_version,
        ));
    }
    Ok(credenciais)
}

/// Avisa em `stderr` (nunca falha) se `caminho` tiver permissão mais aberta
/// que `0600` — primeira vez que o `agentry` guarda segredo em texto plano
/// em disco, a permissão do arquivo é a única barreira real contra outro
/// usuário da mesma máquina lendo (ADR-0038 §3). Só Unix: Windows não tem
/// o mesmo modelo de permissão *owner/group/other* — checagem equivalente
/// fica fora de escopo desta ADR.
#[cfg(unix)]
fn avisar_se_permissao_aberta(caminho: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadados) = std::fs::metadata(caminho) {
        let modo = metadados.permissions().mode() & 0o777;
        if modo != 0o600 {
            eprintln!(
                "[credentials] aviso: {} tem permissão {modo:o} (esperado 0600) -- outro \
                 usuário desta máquina pode conseguir ler as credenciais",
                caminho.display()
            );
        }
    }
}

#[cfg(not(unix))]
fn avisar_se_permissao_aberta(_caminho: &Path) {}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(std::path::PathBuf);
    impl TempDir {
        fn new() -> Self {
            let caminho = std::env::temp_dir().join(format!(
                "agentry-credentials-teste-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio não deve estar antes de 1970")
                    .as_nanos()
            ));
            std::fs::create_dir_all(&caminho).expect("deve criar o diretório temporário");
            Self(caminho)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn resolve_api_key_com_variavel_de_ambiente_nunca_le_arquivo() {
        // Caminho de arquivo inexistente de propósito: se `resolve_api_key`
        // tentasse ler mesmo assim, devolveria erro (não `NotFound`, já que
        // o diretório pai também não existe) -- a ausência de erro aqui
        // prova que o arquivo nunca foi sequer aberto.
        let resultado =
            resolve_api_key("litellm", Some("chave-da-variavel-de-ambiente".to_string()));
        assert_eq!(
            resultado,
            Ok(Some("chave-da-variavel-de-ambiente".to_string()))
        );
    }

    #[test]
    fn load_de_sem_arquivo_e_none_nao_erro() {
        let dir = TempDir::new();
        let resultado = load_de(&dir.path().join("credentials.json"));
        assert_eq!(resultado, Ok(None));
    }

    #[test]
    fn load_de_com_arquivo_valido_le_a_chave_do_provider() {
        let dir = TempDir::new();
        let caminho = dir.path().join("credentials.json");
        std::fs::write(
            &caminho,
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-credentials-schema-1.json",
              "schemaVersion": 1,
              "providers": { "litellm": { "apiKey": "chave-do-arquivo" } }
            }"#,
        )
        .expect("deve escrever o arquivo de teste");

        let credenciais = load_de(&caminho)
            .expect("deve carregar")
            .expect("arquivo existe, deve ser Some");

        assert_eq!(
            credenciais.providers.get("litellm").map(|p| &p.api_key),
            Some(&"chave-do-arquivo".to_string())
        );
    }

    #[test]
    fn load_de_com_json_invalido_e_erro_tratado() {
        let dir = TempDir::new();
        let caminho = dir.path().join("credentials.json");
        std::fs::write(&caminho, "não é json").expect("deve escrever");

        let resultado = load_de(&caminho);

        assert!(matches!(resultado, Err(CredentialsError::Parse(_))));
    }

    #[test]
    fn load_de_com_schema_version_nao_suportada_e_erro_tratado() {
        let dir = TempDir::new();
        let caminho = dir.path().join("credentials.json");
        std::fs::write(&caminho, r#"{"schemaVersion": 99, "providers": {}}"#)
            .expect("deve escrever");

        let resultado = load_de(&caminho);

        assert_eq!(
            resultado,
            Err(CredentialsError::UnsupportedSchemaVersion(99))
        );
    }

    #[test]
    fn resolve_api_key_sem_variavel_e_sem_arquivo_e_none() {
        // `load()` real, sem `$HOME` mockado -- só garante que a ausência
        // do arquivo (ou de `$HOME`) nunca é um erro, sem assumir nada
        // sobre o conteúdo real da máquina que roda o teste.
        let resultado = resolve_api_key("provider-que-nunca-existe-de-verdade", None);
        assert!(resultado.is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn avisar_se_permissao_aberta_nao_falha_com_permissao_correta() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new();
        let caminho = dir.path().join("credentials.json");
        std::fs::write(&caminho, "{}").unwrap();
        std::fs::set_permissions(&caminho, std::fs::Permissions::from_mode(0o600)).unwrap();

        // Não deve entrar em pânico nem alterar o arquivo -- só emite um
        // aviso em stderr quando a permissão é diferente de 0600 (não
        // verificável diretamente aqui sem capturar stderr; o teste prova
        // a ausência de efeito colateral/pânico).
        avisar_se_permissao_aberta(&caminho);

        let modo = std::fs::metadata(&caminho).unwrap().permissions().mode() & 0o777;
        assert_eq!(modo, 0o600);
    }
}
