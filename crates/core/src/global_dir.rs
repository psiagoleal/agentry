// Caminho relativo: crates/core/src/global_dir.rs
//! Resolução do diretório de configuração global do usuário (`~/.agentry/`,
//! MT-127/ADR-0038) — preferências pessoais reutilizáveis entre projetos
//! (`agentry.settings.json`, mesmo schema do arquivo por-projeto) e,
//! futuramente, credenciais de provider (`credentials.json`, MT-128).
//!
//! Resolve `$HOME` (Unix) / `%USERPROFILE%` (Windows) via
//! `std::env::var_os` — sem a crate `dirs`/`directories` (ADR-0004, sem
//! dependência nova). Sem nenhuma das duas variáveis definidas (raro, mas
//! possível em ambiente restrito/contêiner), a configuração global é
//! simplesmente ausente — nunca um erro fatal, mesmo espírito de "arquivo
//! ausente não é erro" já usado por [`crate::config::Settings::from_file`].

use std::ffi::OsString;
use std::path::PathBuf;

/// Resolve o diretório *home* — `$HOME` primeiro, `%USERPROFILE%` como
/// *fallback* (nunca os dois somados). Uma variável presente mas vazia
/// (`HOME=""`) conta como ausente, não como "home é a raiz relativa vazia".
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    home_dir_de(|nome| std::env::var_os(nome))
}

/// Núcleo testável de [`home_dir`] — recebe a função de busca de variável
/// de ambiente como parâmetro, para os testes não precisarem mutar
/// `std::env` de verdade (que é global ao processo e correria risco de
/// interferir com testes rodando em paralelo, mesmo princípio já aplicado
/// por `Settings::from_env_vars`/`from_process_env`).
fn home_dir_de(buscar_var: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    let nao_vazia = |valor: OsString| (!valor.is_empty()).then_some(valor);
    buscar_var("HOME")
        .and_then(nao_vazia)
        .or_else(|| buscar_var("USERPROFILE").and_then(nao_vazia))
        .map(PathBuf::from)
}

/// Caminho de `~/.agentry/agentry.settings.json` (ADR-0038) — só resolve o
/// caminho, não cria nada (mesmo espírito de
/// [`crate::state_dir::agentry_settings_path`]). `None` sem
/// `$HOME`/`%USERPROFILE%`.
#[must_use]
pub fn global_settings_path() -> Option<PathBuf> {
    global_settings_path_de(home_dir())
}

fn global_settings_path_de(home: Option<PathBuf>) -> Option<PathBuf> {
    home.map(|home| home.join(".agentry").join("agentry.settings.json"))
}

/// Caminho de `~/.agentry/credentials.json` (ADR-0038, MT-128) — mesma
/// lógica de [`global_settings_path`], schema separado.
#[must_use]
pub fn global_credentials_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".agentry").join("credentials.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn home_dir_de_prefere_home_quando_ambas_definidas() {
        let resultado = home_dir_de(|nome| match nome {
            "HOME" => Some("/home/usuario".into()),
            "USERPROFILE" => Some(r"C:\Users\usuario".into()),
            _ => None,
        });
        assert_eq!(resultado, Some(PathBuf::from("/home/usuario")));
    }

    #[test]
    fn home_dir_de_cai_em_userprofile_sem_home() {
        let resultado = home_dir_de(|nome| match nome {
            "USERPROFILE" => Some(r"C:\Users\usuario".into()),
            _ => None,
        });
        assert_eq!(resultado, Some(PathBuf::from(r"C:\Users\usuario")));
    }

    #[test]
    fn home_dir_de_sem_nenhuma_das_duas_e_none() {
        let resultado = home_dir_de(|_nome| None);
        assert_eq!(resultado, None);
    }

    #[test]
    fn home_dir_de_com_home_vazia_cai_em_userprofile() {
        let resultado = home_dir_de(|nome| match nome {
            "HOME" => Some("".into()),
            "USERPROFILE" => Some(r"C:\Users\usuario".into()),
            _ => None,
        });
        assert_eq!(resultado, Some(PathBuf::from(r"C:\Users\usuario")));
    }

    #[test]
    fn global_settings_path_de_junta_agentry_e_o_nome_do_arquivo() {
        let resultado = global_settings_path_de(Some(PathBuf::from("/home/usuario")));
        assert_eq!(
            resultado,
            Some(PathBuf::from(
                "/home/usuario/.agentry/agentry.settings.json"
            ))
        );
    }

    #[test]
    fn global_settings_path_de_sem_home_e_none() {
        assert_eq!(global_settings_path_de(None), None);
    }
}
