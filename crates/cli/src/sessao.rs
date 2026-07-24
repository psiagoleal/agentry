// Caminho relativo: crates/cli/src/sessao.rs
//! Comandos de sessão — `/save`, `--resume`, `/sessions` (MT-121/122/123,
//! ADR-0036). Camada fina sobre `agentry_core::session::persist`: aqui só
//! decide **onde** gravar/ler (`.agentry/session/`), gera o `id`, e monta a
//! mensagem de aviso de retenção — a serialização em si vive no núcleo,
//! sem saber nada sobre arquivos.
//!
//! Persistência estritamente **opt-in** (a ADR-0032 continua proibindo
//! persistência automática do conteúdo integral de uma conversa como
//! padrão) — este módulo só grava quando explicitamente chamado por
//! `/save`, nunca sozinho.

use std::path::{Path, PathBuf};

use agentry_core::session::persist::{self, MetadadosDeSessao};
use agentry_core::session::Session;

/// Resolve `.agentry/session/`, criando o diretório se preciso — reaproveita
/// `state_dir::ensure_state_dir` (que já garante `.agentry/` e seu
/// `.gitignore` próprio, ADR-0017) e só acrescenta o subdiretório `session`.
///
/// # Errors
///
/// Devolve o `io::Error` de criar qualquer um dos diretórios.
fn diretorio_de_sessoes(workspace_root: &Path) -> std::io::Result<PathBuf> {
    let estado = agentry_core::state_dir::ensure_state_dir(workspace_root)?;
    let sessoes = estado.join("session");
    std::fs::create_dir_all(&sessoes)?;
    Ok(sessoes)
}

/// `(ano, mês, dia, hora, minuto, segundo)` UTC do instante atual —
/// conversão manual de segundos-desde-*epoch* (sem `chrono`/`time`, nenhuma
/// dependência nova, ADR-0004): algoritmo `civil_from_days` (Howard
/// Hinnant, domínio público — a mesma conta usada internamente por várias
/// bibliotecas de data C++/Rust, incluindo o próprio `chrono`).
fn agora_utc() -> (i64, u32, u32, u32, u32, u32) {
    let segundos_desde_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("relógio do sistema não deve estar antes de 1970")
        .as_secs() as i64;
    let dias = segundos_desde_epoch.div_euclid(86400);
    let segundos_do_dia = segundos_desde_epoch.rem_euclid(86400);
    let (ano, mes, dia) = civil_from_days(dias);
    let hora = (segundos_do_dia / 3600) as u32;
    let minuto = ((segundos_do_dia % 3600) / 60) as u32;
    let segundo = (segundos_do_dia % 60) as u32;
    (ano, mes, dia, hora, minuto, segundo)
}

/// Dias desde a época Unix (1970-01-01) → (ano, mês, dia), calendário
/// gregoriano proléptico. `civil_from_days`, Howard Hinnant (domínio
/// público, <https://howardhinnant.github.io/date_algorithms.html>).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// `id` de uma sessão salva: *timestamp* UTC compacto (`AAAAMMDD-HHMMSS`),
/// com `-<nome>` sufixado (sanitizado — só `[a-z0-9-]`, resto descartado)
/// quando `nome` é dado. Sempre prefixado pelo *timestamp*, pra listagem em
/// ordem cronológica (`/sessions`, MT-123) nunca depender do usuário ter
/// nomeado de forma ordenável (ADR-0036).
fn gerar_id(nome: Option<&str>) -> String {
    let (ano, mes, dia, hora, minuto, segundo) = agora_utc();
    let timestamp = format!("{ano:04}{mes:02}{dia:02}-{hora:02}{minuto:02}{segundo:02}");
    match nome.map(sanitizar_nome).filter(|s| !s.is_empty()) {
        Some(sanitizado) => format!("{timestamp}-{sanitizado}"),
        None => timestamp,
    }
}

fn sanitizar_nome(nome: &str) -> String {
    nome.to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect()
}

/// Aviso de retenção impresso **toda vez** que uma sessão é salva — sem
/// *flag* pra silenciar (ADR-0036, diretriz de conformidade: obrigatório,
/// não é um aviso "só na primeira vez").
fn aviso_de_retencao(caminho: &Path) -> String {
    format!(
        "sessão salva em {}\naviso: pode conter informação sensível da conversa; o \
         diretório já está fora do controle de versão (.agentry/.gitignore), mas o arquivo \
         continua no disco até você apagá-lo",
        caminho.display()
    )
}

/// Salva a sessão corrente em `.agentry/session/<id>.md` — comando `/save
/// [nome]` (REPL/TUI). Devolve a mensagem a mostrar ao usuário (caminho +
/// aviso de retenção obrigatório).
///
/// # Errors
///
/// Devolve o `io::Error` de criar o diretório ou escrever o arquivo.
pub fn salvar(
    workspace_root: &Path,
    nome: Option<&str>,
    sessao: &Session,
    task_class: &str,
) -> std::io::Result<String> {
    let id = gerar_id(nome);
    let (ano, mes, dia, hora, minuto, segundo) = agora_utc();
    let criado_em = format!("{ano:04}-{mes:02}-{dia:02}T{hora:02}:{minuto:02}:{segundo:02}Z");
    let usage = sessao.usage_total();
    let metadados = MetadadosDeSessao {
        id: id.clone(),
        criado_em,
        provider: sessao.provider_name().to_string(),
        model: sessao.model().to_string(),
        task_class: task_class.to_string(),
        usage_input_tokens: usage.input_tokens,
        usage_output_tokens: usage.output_tokens,
    };
    let markdown = persist::serializar_para_markdown(&metadados, sessao.messages());

    let diretorio = diretorio_de_sessoes(workspace_root)?;
    let caminho = diretorio.join(format!("{id}.md"));
    std::fs::write(&caminho, markdown)?;

    Ok(aviso_de_retencao(&caminho))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn civil_from_days_bate_com_referencias_conhecidas() {
        // Referências geradas via `date -u -d "<data>" +%s`.
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(1784851200 / 86400), (2026, 7, 24));
        assert_eq!(civil_from_days(951868800 / 86400), (2000, 3, 1));
    }

    #[test]
    fn agora_utc_bate_com_hora_do_dia_de_um_timestamp_conhecido() {
        // 1784917845 = 2026-07-24T18:30:45Z (`date -u -d ... +%s`).
        let segundos_do_dia = 1784917845i64.rem_euclid(86400);
        assert_eq!(segundos_do_dia / 3600, 18);
        assert_eq!((segundos_do_dia % 3600) / 60, 30);
        assert_eq!(segundos_do_dia % 60, 45);
    }

    #[test]
    fn gerar_id_sem_nome_e_so_o_timestamp() {
        let id = gerar_id(None);
        assert_eq!(id.len(), "AAAAMMDD-HHMMSS".len());
        assert!(id.chars().all(|c| c.is_ascii_digit() || c == '-'));
    }

    #[test]
    fn gerar_id_com_nome_sanitiza_e_sufixa() {
        let id = gerar_id(Some("Minha Sessão! Importante"));
        assert!(
            id.ends_with("-minhasessoimportante"),
            "id: {id:?}, sanitização remove espaços/acentos/pontuação"
        );
    }

    #[test]
    fn gerar_id_com_nome_so_de_caracteres_invalidos_ignora_o_nome() {
        let id = gerar_id(Some("!!!"));
        assert_eq!(id.len(), "AAAAMMDD-HHMMSS".len());
    }

    #[test]
    fn sanitizar_nome_mantem_so_letras_digitos_e_hifen() {
        assert_eq!(sanitizar_nome("Minha Sessão-1"), "minhasesso-1");
        assert_eq!(sanitizar_nome("café com leite"), "cafcomleite");
    }
}
