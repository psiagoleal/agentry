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

/// Lista os arquivos `.md` de `.agentry/session/`, em ordem alfabética
/// (== cronológica, já que `id` é sempre prefixado pelo *timestamp*
/// `AAAAMMDD-HHMMSS`) — auxiliar de [`carregar_sessao`] e de `/sessions`
/// (MT-123). Diretório ausente é uma lista vazia, não erro (ainda não
/// existe nenhuma sessão salva, caso normal na primeira vez).
fn listar_arquivos_de_sessao(workspace_root: &Path) -> std::io::Result<Vec<PathBuf>> {
    let diretorio = diretorio_de_sessoes(workspace_root)?;
    let mut arquivos: Vec<PathBuf> = std::fs::read_dir(&diretorio)?
        .filter_map(|entrada| entrada.ok())
        .map(|entrada| entrada.path())
        .filter(|caminho| caminho.extension().is_some_and(|ext| ext == "md"))
        .collect();
    arquivos.sort();
    Ok(arquivos)
}

/// Localiza o arquivo de sessão salva correspondente a `id_ou_nome` (string
/// vazia = mais recente) — correspondência exata do nome de arquivo
/// primeiro, senão prefixo único do `id`. Zero ou várias correspondências
/// são erro claro (nunca uma escolha arbitrária, ADR-0036).
///
/// # Errors
///
/// Devolve uma mensagem de erro legível: nenhuma sessão salva, nenhuma
/// correspondência, ou correspondência ambígua (lista os candidatos).
fn localizar_arquivo(workspace_root: &Path, id_ou_nome: &str) -> Result<PathBuf, String> {
    let arquivos = listar_arquivos_de_sessao(workspace_root).map_err(|e| e.to_string())?;
    if arquivos.is_empty() {
        return Err("nenhuma sessão salva em .agentry/session/".to_string());
    }
    if id_ou_nome.is_empty() {
        return Ok(arquivos
            .last()
            .expect("lista não vazia checada acima")
            .clone());
    }

    let exato = format!("{id_ou_nome}.md");
    if let Some(caminho) = arquivos
        .iter()
        .find(|c| c.file_name().and_then(|n| n.to_str()) == Some(exato.as_str()))
    {
        return Ok(caminho.clone());
    }

    let correspondentes: Vec<&PathBuf> = arquivos
        .iter()
        .filter(|c| {
            c.file_stem()
                .and_then(|n| n.to_str())
                .is_some_and(|stem| stem.starts_with(id_ou_nome))
        })
        .collect();
    match correspondentes.as_slice() {
        [unico] => Ok((*unico).clone()),
        [] => Err(format!("nenhuma sessão corresponde a '{id_ou_nome}'")),
        varios => {
            let nomes: Vec<String> = varios
                .iter()
                .filter_map(|c| c.file_stem().and_then(|n| n.to_str()))
                .map(str::to_string)
                .collect();
            Err(format!(
                "'{id_ou_nome}' corresponde a várias sessões, seja mais específico: {}",
                nomes.join(", ")
            ))
        }
    }
}

/// Localiza e desserializa uma sessão salva — `--resume [id-ou-nome]`
/// (MT-122). `id_ou_nome` vazio retoma a mais recente.
///
/// # Errors
///
/// Devolve uma mensagem de erro legível (nenhuma sessão salva, ambígua, ou
/// arquivo corrompido/editado de forma inválida — nunca uma sessão
/// retomada silenciosamente truncada, ADR-0036).
pub fn carregar_sessao(
    workspace_root: &Path,
    id_ou_nome: &str,
) -> Result<Vec<agentry_core::model::Message>, String> {
    let caminho = localizar_arquivo(workspace_root, id_ou_nome)?;
    let texto = std::fs::read_to_string(&caminho).map_err(|e| e.to_string())?;
    let (_, mensagens) = persist::desserializar_de_markdown(&texto).map_err(|e| e.to_string())?;
    Ok(mensagens)
}

/// Resumo de uma sessão salva — o suficiente para o usuário reconhecer qual
/// `id` passar a `--resume` sem ter que abrir o arquivo (`/sessions`,
/// MT-123).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoDeSessao {
    pub id: String,
    pub criado_em: String,
    pub titulo: String,
}

/// Primeira linha não vazia de `texto`, truncada a 60 caracteres — título
/// legível pra `/sessions` a partir da primeira mensagem de usuário.
fn titulo_de(texto: &str) -> String {
    let primeira_linha = texto.lines().next().unwrap_or("").trim();
    if primeira_linha.chars().count() > 60 {
        let truncado: String = primeira_linha.chars().take(57).collect();
        format!("{truncado}...")
    } else {
        primeira_linha.to_string()
    }
}

/// Lista as sessões salvas em `.agentry/session/`, mais recente primeiro —
/// `/sessions` (MT-123). Cada sessão é lida e desserializada individualmente
/// (reaproveita `persist::desserializar_de_markdown`, MT-120, sem um
/// segundo caminho de *parsing* só pra metadados); um arquivo corrompido ou
/// editado à mão de forma inválida é **ignorado**, não aborta a listagem
/// inteira — o usuário ainda quer ver as sessões boas mesmo se uma estiver
/// quebrada (diferente de `carregar_sessao`, que precisa da sessão
/// específica pedida e por isso falha alto se ela estiver corrompida).
///
/// # Errors
///
/// Devolve erro apenas se o próprio diretório `.agentry/session/` não puder
/// ser lido (ex.: permissão) — lista vazia (não erro) quando não há nenhuma
/// sessão salva ainda.
pub fn listar_sessoes(workspace_root: &Path) -> std::io::Result<Vec<ResumoDeSessao>> {
    let mut arquivos = listar_arquivos_de_sessao(workspace_root)?;
    arquivos.reverse(); // listar_arquivos_de_sessao ordena do mais antigo pro mais recente
    let mut resumos = Vec::new();
    for caminho in arquivos {
        let Ok(texto) = std::fs::read_to_string(&caminho) else {
            continue;
        };
        let Ok((metadados, mensagens)) = persist::desserializar_de_markdown(&texto) else {
            continue;
        };
        let titulo = mensagens
            .iter()
            .find(|mensagem| mensagem.role == agentry_core::model::Role::User)
            .map(|mensagem| titulo_de(&mensagem.text_content()))
            .unwrap_or_else(|| "(sem mensagem de usuário)".to_string());
        resumos.push(ResumoDeSessao {
            id: metadados.id,
            criado_em: metadados.criado_em,
            titulo,
        });
    }
    Ok(resumos)
}

/// Formata a saída de `/sessions` — usada tanto pelo REPL quanto pela TUI,
/// fonte única (mesmo padrão de `aviso_de_retencao`).
#[must_use]
pub fn formatar_lista_de_sessoes(sessoes: &[ResumoDeSessao]) -> String {
    if sessoes.is_empty() {
        return "nenhuma sessão salva ainda (use /save)".to_string();
    }
    let mut saida = String::from("sessões salvas (mais recente primeiro):\n");
    for sessao in sessoes {
        saida.push_str(&format!(
            "  {} — {} — {}\n",
            sessao.id, sessao.criado_em, sessao.titulo
        ));
    }
    saida.pop(); // remove a última quebra de linha
    saida
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::model::{Message, Role};

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

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `state_dir`/`checkpoint`/`main`, MT-38).
    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-sessao-test-{}-{}",
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

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Escreve uma sessão de teste diretamente (sem precisar de uma
    /// `Session`/`Router`/`MockProvider` reais) — `carregar_sessao` só lê e
    /// desserializa, então um arquivo pronto (via `persist`, já testado à
    /// parte no núcleo) é suficiente pra testar a lógica de localização.
    fn escrever_sessao_de_teste(
        workspace_root: &Path,
        id: &str,
        mensagens: &[agentry_core::model::Message],
    ) {
        let metadados = MetadadosDeSessao {
            id: id.to_string(),
            criado_em: "2026-07-24T00:00:00Z".to_string(),
            provider: "ollama".to_string(),
            model: "modelo-x".to_string(),
            task_class: "chat".to_string(),
            usage_input_tokens: 0,
            usage_output_tokens: 0,
        };
        let markdown = persist::serializar_para_markdown(&metadados, mensagens);
        let diretorio = diretorio_de_sessoes(workspace_root).expect("deve criar o diretório");
        std::fs::write(diretorio.join(format!("{id}.md")), markdown)
            .expect("deve escrever o arquivo de teste");
    }

    #[test]
    fn carregar_sessao_sem_nenhuma_salva_e_erro_claro() {
        let dir = TempDir::new();
        let resultado = carregar_sessao(dir.path(), "");
        assert!(resultado.is_err());
        assert!(resultado.unwrap_err().contains("nenhuma sessão salva"));
    }

    #[test]
    fn carregar_sessao_vazio_retoma_a_mais_recente() {
        let dir = TempDir::new();
        let mensagens_antiga = vec![agentry_core::model::Message::text(
            agentry_core::model::Role::User,
            "primeira",
        )];
        let mensagens_nova = vec![agentry_core::model::Message::text(
            agentry_core::model::Role::User,
            "segunda",
        )];
        escrever_sessao_de_teste(dir.path(), "20260101-000000", &mensagens_antiga);
        escrever_sessao_de_teste(dir.path(), "20260724-000000", &mensagens_nova);

        let mensagens = carregar_sessao(dir.path(), "").expect("deve carregar a mais recente");
        assert_eq!(mensagens, mensagens_nova);
    }

    #[test]
    fn carregar_sessao_por_id_exato() {
        let dir = TempDir::new();
        let mensagens = vec![agentry_core::model::Message::text(
            agentry_core::model::Role::User,
            "oi",
        )];
        escrever_sessao_de_teste(dir.path(), "20260724-000000-minha", &mensagens);

        let carregadas = carregar_sessao(dir.path(), "20260724-000000-minha")
            .expect("deve carregar por id exato");
        assert_eq!(carregadas, mensagens);
    }

    #[test]
    fn carregar_sessao_com_prefixo_que_nao_bate_e_erro_claro() {
        // Prefixo é sobre o `id` inteiro, que começa pelo timestamp --
        // "minha" não é prefixo de "20260724-000000-minha".
        let dir = TempDir::new();
        let mensagens = vec![agentry_core::model::Message::text(
            agentry_core::model::Role::User,
            "oi",
        )];
        escrever_sessao_de_teste(dir.path(), "20260724-000000-minha", &mensagens);

        let resultado = carregar_sessao(dir.path(), "minha");
        assert!(resultado.is_err());
    }

    #[test]
    fn carregar_sessao_por_prefixo_do_timestamp() {
        let dir = TempDir::new();
        let mensagens = vec![agentry_core::model::Message::text(
            agentry_core::model::Role::User,
            "oi",
        )];
        escrever_sessao_de_teste(dir.path(), "20260724-153000-minha", &mensagens);

        let carregadas =
            carregar_sessao(dir.path(), "20260724").expect("prefixo do timestamp deve bastar");
        assert_eq!(carregadas, mensagens);
    }

    #[test]
    fn carregar_sessao_ambigua_e_erro_com_candidatos_listados() {
        let dir = TempDir::new();
        let mensagens = vec![agentry_core::model::Message::text(
            agentry_core::model::Role::User,
            "oi",
        )];
        escrever_sessao_de_teste(dir.path(), "20260724-000000-um", &mensagens);
        escrever_sessao_de_teste(dir.path(), "20260724-000001-dois", &mensagens);

        let resultado = carregar_sessao(dir.path(), "20260724");
        assert!(resultado.is_err());
        let erro = resultado.unwrap_err();
        assert!(erro.contains("várias sessões"));
        assert!(erro.contains("20260724-000000-um"));
        assert!(erro.contains("20260724-000001-dois"));
    }

    #[test]
    fn carregar_sessao_inexistente_e_erro_claro() {
        let dir = TempDir::new();
        escrever_sessao_de_teste(
            dir.path(),
            "20260724-000000",
            &[agentry_core::model::Message::text(
                agentry_core::model::Role::User,
                "oi",
            )],
        );

        let resultado = carregar_sessao(dir.path(), "nao-existe");
        assert!(resultado.is_err());
        assert!(resultado
            .unwrap_err()
            .contains("nenhuma sessão corresponde"));
    }

    #[test]
    fn salvar_e_carregar_sessao_fazem_round_trip() {
        use agentry_core::provider::mock::MockProvider;
        use agentry_core::router::{CallPreset, ResolvedRoute};
        use agentry_core::session::{Session, TokenBudget, ToolExecutor};
        use std::sync::Arc;

        struct NoopExecutor;
        impl ToolExecutor for NoopExecutor {
            fn execute(
                &self,
                call: &agentry_core::model::ToolCall,
            ) -> agentry_core::provider::BoxFuture<'_, agentry_core::model::ToolResult>
            {
                let call_id = call.id.clone();
                Box::pin(async move {
                    agentry_core::model::ToolResult {
                        call_id,
                        content: String::new(),
                        is_error: false,
                    }
                })
            }
        }

        let dir = TempDir::new();
        let mock: Arc<dyn agentry_core::provider::LlmProvider> =
            Arc::new(MockProvider::new("ollama"));
        let rota = ResolvedRoute::new(mock, "modelo-x", CallPreset::default());
        let mut sessao = Session::new(rota, Arc::new(NoopExecutor), TokenBudget::new(10_000));
        sessao.push_user_message("oi, tudo bem?");

        let aviso = salvar(dir.path(), None, &sessao, "chat").expect("deve salvar");
        assert!(aviso.contains("sessão salva em"));

        let mensagens_carregadas =
            carregar_sessao(dir.path(), "").expect("deve carregar a sessão recém-salva");
        assert_eq!(
            mensagens_carregadas,
            vec![Message::text(Role::User, "oi, tudo bem?")]
        );
    }

    // --- MT-123: /sessions ---

    #[test]
    fn listar_sessoes_sem_nenhuma_salva_e_lista_vazia_nao_erro() {
        let dir = TempDir::new();
        let sessoes = listar_sessoes(dir.path()).expect("diretório vazio não deve falhar");
        assert!(sessoes.is_empty());
    }

    #[test]
    fn listar_sessoes_ordena_mais_recente_primeiro_e_extrai_titulo() {
        let dir = TempDir::new();
        escrever_sessao_de_teste(
            dir.path(),
            "20260101-000000",
            &[Message::text(Role::User, "pergunta antiga")],
        );
        escrever_sessao_de_teste(
            dir.path(),
            "20260724-000000",
            &[Message::text(Role::User, "pergunta nova")],
        );

        let sessoes = listar_sessoes(dir.path()).expect("deve listar");

        assert_eq!(sessoes.len(), 2);
        assert_eq!(sessoes[0].id, "20260724-000000");
        assert_eq!(sessoes[0].titulo, "pergunta nova");
        assert_eq!(sessoes[1].id, "20260101-000000");
        assert_eq!(sessoes[1].titulo, "pergunta antiga");
    }

    #[test]
    fn listar_sessoes_sem_mensagem_de_usuario_usa_titulo_padrao() {
        let dir = TempDir::new();
        escrever_sessao_de_teste(
            dir.path(),
            "20260724-000000",
            &[Message::text(Role::System, "prompt de sistema só")],
        );

        let sessoes = listar_sessoes(dir.path()).expect("deve listar");

        assert_eq!(sessoes[0].titulo, "(sem mensagem de usuário)");
    }

    #[test]
    fn listar_sessoes_ignora_arquivo_corrompido_sem_abortar_a_listagem() {
        let dir = TempDir::new();
        escrever_sessao_de_teste(
            dir.path(),
            "20260101-000000",
            &[Message::text(Role::User, "sessão boa")],
        );
        let diretorio = diretorio_de_sessoes(dir.path()).expect("deve criar o diretório");
        std::fs::write(
            diretorio.join("20260724-quebrada.md"),
            "não é markdown válido",
        )
        .expect("deve escrever o arquivo corrompido");

        let sessoes = listar_sessoes(dir.path()).expect("deve listar mesmo com um arquivo ruim");

        assert_eq!(sessoes.len(), 1);
        assert_eq!(sessoes[0].titulo, "sessão boa");
    }

    #[test]
    fn titulo_de_trunca_linhas_longas_em_60_caracteres() {
        let longa = "a".repeat(100);
        let titulo = titulo_de(&longa);
        assert_eq!(titulo.chars().count(), 60); // 57 + "..."
        assert!(titulo.ends_with("..."));
    }

    #[test]
    fn formatar_lista_de_sessoes_vazia_avisa_para_usar_save() {
        let texto = formatar_lista_de_sessoes(&[]);
        assert!(texto.contains("/save"));
    }

    #[test]
    fn formatar_lista_de_sessoes_mostra_id_data_e_titulo() {
        let sessoes = vec![ResumoDeSessao {
            id: "20260724-000000".to_string(),
            criado_em: "2026-07-24T00:00:00Z".to_string(),
            titulo: "pergunta nova".to_string(),
        }];
        let texto = formatar_lista_de_sessoes(&sessoes);
        assert!(texto.contains("20260724-000000"));
        assert!(texto.contains("2026-07-24T00:00:00Z"));
        assert!(texto.contains("pergunta nova"));
    }
}
