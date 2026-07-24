// Caminho relativo: crates/core/src/session/persist.rs
//! Serialização de sessão em Markdown (ADR-0036, MT-119/120) — persistência
//! **opt-in** (`/save`/`--resume`, MT-121/122), nunca automática (a ADR-0032
//! continua proibindo isso como padrão). Formato: *front matter* simples
//! (linhas `chave: valor`, sem biblioteca YAML — nenhuma dependência nova,
//! ADR-0004) seguido de uma seção `## <Papel>` por [`Message`], onde
//! `ContentBlock::Text` vira prosa e `ContentBlock::ToolCall`/`ToolResult`
//! viram blocos cercados `tool-call`/`tool-result` (JSON de uma linha,
//! reaproveitando `Serialize`/`Deserialize` já existente desses tipos — nenhum
//! formato novo inventado).
//!
//! Este módulo só serializa/desserializa em memória (`String` ↔
//! `Vec<Message>`) — não sabe nada sobre `.agentry/session/`, nomes de
//! arquivo, ou avisos de retenção (isso é MT-121/122, na CLI).

use crate::model::{ContentBlock, Message, Role, ToolCall, ToolResult};

/// Metadados de uma sessão salva — tudo que não é histórico de mensagens em
/// si. `Session` não guarda `id`/`criado_em`/`task_class` diretamente (são
/// decisões de quem chama `/save`, não do núcleo do laço de agente), por
/// isso ficam explícitos aqui em vez de extraídos de `Session`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadadosDeSessao {
    pub id: String,
    pub criado_em: String,
    pub provider: String,
    pub model: String,
    pub task_class: String,
    pub usage_input_tokens: u64,
    pub usage_output_tokens: u64,
}

/// Erro de desserialização (MT-120) — nunca falha silenciosamente
/// (diretriz de conformidade da ADR-0036): um arquivo editado à mão de
/// forma inválida é sempre reportado, nunca uma sessão retomada com
/// histórico truncado sem aviso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroDeSessao {
    /// *Front matter* ausente ou sem os dois delimitadores `---`.
    FrontMatterAusente,
    /// Uma linha do *front matter* não é `chave: valor`.
    FrontMatterInvalido(String),
    /// Cabeçalho `## <algo>` não é um papel reconhecido.
    PapelDesconhecido(String),
    /// JSON malformado dentro de um bloco `tool-call`/`tool-result`.
    JsonInvalido(String),
}

impl core::fmt::Display for ErroDeSessao {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::FrontMatterAusente => {
                write!(f, "arquivo de sessão sem front matter (linhas '---')")
            }
            Self::FrontMatterInvalido(linha) => {
                write!(f, "linha de front matter inválida: {linha:?}")
            }
            Self::PapelDesconhecido(papel) => {
                write!(f, "cabeçalho de papel desconhecido: '## {papel}'")
            }
            Self::JsonInvalido(detalhe) => write!(f, "JSON inválido num bloco de tool: {detalhe}"),
        }
    }
}

impl std::error::Error for ErroDeSessao {}

fn papel_para_cabecalho(role: &Role) -> &'static str {
    match role {
        Role::System => "Sistema",
        Role::User => "Usuário",
        Role::Assistant => "Agente",
        Role::Tool => "Tool",
    }
}

fn cabecalho_para_papel(texto: &str) -> Option<Role> {
    match texto {
        "Sistema" => Some(Role::System),
        "Usuário" => Some(Role::User),
        "Agente" => Some(Role::Assistant),
        "Tool" => Some(Role::Tool),
        _ => None,
    }
}

/// Serializa `metadados` + `mensagens` no formato Markdown da ADR-0036.
#[must_use]
pub fn serializar_para_markdown(metadados: &MetadadosDeSessao, mensagens: &[Message]) -> String {
    let mut saida = String::new();
    saida.push_str("---\n");
    saida.push_str(&format!("id: {}\n", metadados.id));
    saida.push_str(&format!("criado_em: {}\n", metadados.criado_em));
    saida.push_str(&format!("provider: {}\n", metadados.provider));
    saida.push_str(&format!("model: {}\n", metadados.model));
    saida.push_str(&format!("task_class: {}\n", metadados.task_class));
    saida.push_str(&format!(
        "usage_input_tokens: {}\n",
        metadados.usage_input_tokens
    ));
    saida.push_str(&format!(
        "usage_output_tokens: {}\n",
        metadados.usage_output_tokens
    ));
    saida.push_str("---\n");

    for mensagem in mensagens {
        saida.push_str("\n## ");
        saida.push_str(papel_para_cabecalho(&mensagem.role));
        saida.push('\n');
        for bloco in &mensagem.content {
            saida.push('\n');
            match bloco {
                ContentBlock::Text { text } => {
                    saida.push_str(text);
                    saida.push('\n');
                }
                ContentBlock::ToolCall(call) => {
                    saida.push_str("```tool-call\n");
                    saida.push_str(
                        &serde_json::to_string(call).expect("ToolCall sempre serializável"),
                    );
                    saida.push_str("\n```\n");
                }
                ContentBlock::ToolResult(result) => {
                    saida.push_str("```tool-result\n");
                    saida.push_str(
                        &serde_json::to_string(result).expect("ToolResult sempre serializável"),
                    );
                    saida.push_str("\n```\n");
                }
            }
        }
    }
    saida
}

/// Desserializa o formato produzido por [`serializar_para_markdown`] de
/// volta em `(metadados, mensagens)`.
///
/// # Errors
///
/// Devolve [`ErroDeSessao`] para qualquer conteúdo que não siga o formato —
/// nunca silenciosamente ignora/trunca (ADR-0036).
pub fn desserializar_de_markdown(
    texto: &str,
) -> Result<(MetadadosDeSessao, Vec<Message>), ErroDeSessao> {
    let (front_matter, corpo) = extrair_front_matter(texto)?;
    let metadados = parsear_front_matter(&front_matter)?;
    let mensagens = parsear_corpo(corpo)?;
    Ok((metadados, mensagens))
}

fn extrair_front_matter(texto: &str) -> Result<(String, &str), ErroDeSessao> {
    let resto = texto
        .strip_prefix("---\n")
        .ok_or(ErroDeSessao::FrontMatterAusente)?;
    let fim = resto
        .find("\n---\n")
        .ok_or(ErroDeSessao::FrontMatterAusente)?;
    let front_matter = resto[..fim].to_string();
    let corpo = &resto[fim + "\n---\n".len()..];
    Ok((front_matter, corpo))
}

fn parsear_front_matter(front_matter: &str) -> Result<MetadadosDeSessao, ErroDeSessao> {
    let mut campos = HashMapDeCampos::default();
    for linha in front_matter.lines() {
        if linha.trim().is_empty() {
            continue;
        }
        let (chave, valor) = linha
            .split_once(':')
            .ok_or_else(|| ErroDeSessao::FrontMatterInvalido(linha.to_string()))?;
        campos.definir(chave.trim(), valor.trim());
    }
    campos.para_metadados()
}

/// Pequeno acumulador de campos do *front matter* — evita puxar uma crate de
/// mapa ordenado só pra sete chaves fixas conhecidas de antemão.
#[derive(Default)]
struct HashMapDeCampos {
    id: Option<String>,
    criado_em: Option<String>,
    provider: Option<String>,
    model: Option<String>,
    task_class: Option<String>,
    usage_input_tokens: Option<u64>,
    usage_output_tokens: Option<u64>,
}

impl HashMapDeCampos {
    fn definir(&mut self, chave: &str, valor: &str) {
        match chave {
            "id" => self.id = Some(valor.to_string()),
            "criado_em" => self.criado_em = Some(valor.to_string()),
            "provider" => self.provider = Some(valor.to_string()),
            "model" => self.model = Some(valor.to_string()),
            "task_class" => self.task_class = Some(valor.to_string()),
            "usage_input_tokens" => self.usage_input_tokens = valor.parse().ok(),
            "usage_output_tokens" => self.usage_output_tokens = valor.parse().ok(),
            _ => {}
        }
    }

    fn para_metadados(self) -> Result<MetadadosDeSessao, ErroDeSessao> {
        Ok(MetadadosDeSessao {
            id: self.id.unwrap_or_default(),
            criado_em: self.criado_em.unwrap_or_default(),
            provider: self.provider.unwrap_or_default(),
            model: self.model.unwrap_or_default(),
            task_class: self.task_class.unwrap_or_default(),
            usage_input_tokens: self.usage_input_tokens.unwrap_or(0),
            usage_output_tokens: self.usage_output_tokens.unwrap_or(0),
        })
    }
}

fn parsear_corpo(corpo: &str) -> Result<Vec<Message>, ErroDeSessao> {
    let mut mensagens = Vec::new();
    let mut role_atual: Option<Role> = None;
    let mut blocos_atuais: Vec<ContentBlock> = Vec::new();
    let mut texto_acumulado = String::new();

    let mut linhas = corpo.lines().peekable();
    while let Some(linha) = linhas.next() {
        if let Some(papel_texto) = linha.strip_prefix("## ") {
            fechar_texto_acumulado(&mut texto_acumulado, &mut blocos_atuais);
            if let Some(role) = role_atual.take() {
                mensagens.push(Message {
                    role,
                    content: std::mem::take(&mut blocos_atuais),
                });
            }
            role_atual = Some(
                cabecalho_para_papel(papel_texto)
                    .ok_or_else(|| ErroDeSessao::PapelDesconhecido(papel_texto.to_string()))?,
            );
            continue;
        }

        if let Some(tipo) = linha
            .strip_prefix("```tool-call")
            .map(|_| "tool-call")
            .or_else(|| linha.strip_prefix("```tool-result").map(|_| "tool-result"))
        {
            fechar_texto_acumulado(&mut texto_acumulado, &mut blocos_atuais);
            let mut json = String::new();
            for linha_json in linhas.by_ref() {
                if linha_json == "```" {
                    break;
                }
                if !json.is_empty() {
                    json.push('\n');
                }
                json.push_str(linha_json);
            }
            if tipo == "tool-call" {
                let call: ToolCall = serde_json::from_str(&json)
                    .map_err(|e| ErroDeSessao::JsonInvalido(e.to_string()))?;
                blocos_atuais.push(ContentBlock::ToolCall(call));
            } else {
                let result: ToolResult = serde_json::from_str(&json)
                    .map_err(|e| ErroDeSessao::JsonInvalido(e.to_string()))?;
                blocos_atuais.push(ContentBlock::ToolResult(result));
            }
            continue;
        }

        if !texto_acumulado.is_empty() {
            texto_acumulado.push('\n');
        }
        texto_acumulado.push_str(linha);
    }

    fechar_texto_acumulado(&mut texto_acumulado, &mut blocos_atuais);
    if let Some(role) = role_atual {
        mensagens.push(Message {
            role,
            content: blocos_atuais,
        });
    }

    Ok(mensagens)
}

/// Fecha o texto corrido acumulado (se houver algo além de linhas em
/// branco) como um `ContentBlock::Text`, na ordem em que apareceu.
fn fechar_texto_acumulado(texto_acumulado: &mut String, blocos: &mut Vec<ContentBlock>) {
    let aparado = texto_acumulado.trim();
    if !aparado.is_empty() {
        blocos.push(ContentBlock::Text {
            text: aparado.to_string(),
        });
    }
    texto_acumulado.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadados_de_teste() -> MetadadosDeSessao {
        MetadadosDeSessao {
            id: "20260724-183000".into(),
            criado_em: "2026-07-24T18:30:00Z".into(),
            provider: "litellm".into(),
            model: "gpt-4o".into(),
            task_class: "chat".into(),
            usage_input_tokens: 1234,
            usage_output_tokens: 567,
        }
    }

    #[test]
    fn serializa_front_matter_com_todos_os_metadados() {
        let markdown = serializar_para_markdown(&metadados_de_teste(), &[]);

        assert!(markdown.starts_with("---\n"));
        assert!(markdown.contains("id: 20260724-183000\n"));
        assert!(markdown.contains("criado_em: 2026-07-24T18:30:00Z\n"));
        assert!(markdown.contains("provider: litellm\n"));
        assert!(markdown.contains("model: gpt-4o\n"));
        assert!(markdown.contains("task_class: chat\n"));
        assert!(markdown.contains("usage_input_tokens: 1234\n"));
        assert!(markdown.contains("usage_output_tokens: 567\n"));
    }

    #[test]
    fn serializa_mensagem_de_texto_como_secao_com_papel_certo() {
        let mensagens = vec![Message::text(Role::User, "oi, tudo bem?")];
        let markdown = serializar_para_markdown(&metadados_de_teste(), &mensagens);

        assert!(markdown.contains("## Usuário"));
        assert!(markdown.contains("oi, tudo bem?"));
    }

    #[test]
    fn serializa_tool_call_e_tool_result_como_blocos_cercados() {
        let mensagens = vec![
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: "call_1".into(),
                    name: "fs_write".into(),
                    arguments: serde_json::json!({"path": "a.txt"}),
                })],
            },
            Message {
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(ToolResult {
                    call_id: "call_1".into(),
                    content: "arquivo criado".into(),
                    is_error: false,
                })],
            },
        ];
        let markdown = serializar_para_markdown(&metadados_de_teste(), &mensagens);

        assert!(markdown.contains("```tool-call\n"));
        assert!(markdown.contains(r#""name":"fs_write""#));
        assert!(markdown.contains("```tool-result\n"));
        assert!(markdown.contains(r#""content":"arquivo criado""#));
    }

    #[test]
    fn round_trip_completo_preserva_mensagens_e_metadados() {
        let metadados = metadados_de_teste();
        let mensagens = vec![
            Message::text(Role::System, "seja conciso"),
            Message::text(Role::User, "leia a.txt"),
            Message {
                role: Role::Assistant,
                content: vec![
                    ContentBlock::Text {
                        text: "vou ler o arquivo".into(),
                    },
                    ContentBlock::ToolCall(ToolCall {
                        id: "call_1".into(),
                        name: "fs_read".into(),
                        arguments: serde_json::json!({"path": "a.txt"}),
                    }),
                ],
            },
            Message {
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(ToolResult {
                    call_id: "call_1".into(),
                    content: "conteúdo do arquivo".into(),
                    is_error: false,
                })],
            },
            Message::text(Role::Assistant, "o arquivo diz isso: ..."),
        ];

        let markdown = serializar_para_markdown(&metadados, &mensagens);
        let (metadados_lidos, mensagens_lidas) =
            desserializar_de_markdown(&markdown).expect("deve desserializar sem erro");

        assert_eq!(metadados_lidos, metadados);
        assert_eq!(mensagens_lidas, mensagens);
    }

    #[test]
    fn round_trip_com_duas_secoes_do_mesmo_papel_em_sequencia_preserva_a_ordem() {
        let mensagens = vec![
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: "call_1".into(),
                    name: "fs_read".into(),
                    arguments: serde_json::json!({"path": "a.txt"}),
                })],
            },
            Message {
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(ToolResult {
                    call_id: "call_1".into(),
                    content: "a".into(),
                    is_error: false,
                })],
            },
            Message {
                role: Role::Assistant,
                content: vec![ContentBlock::ToolCall(ToolCall {
                    id: "call_2".into(),
                    name: "fs_read".into(),
                    arguments: serde_json::json!({"path": "b.txt"}),
                })],
            },
            Message {
                role: Role::Tool,
                content: vec![ContentBlock::ToolResult(ToolResult {
                    call_id: "call_2".into(),
                    content: "b".into(),
                    is_error: false,
                })],
            },
        ];

        let markdown = serializar_para_markdown(&metadados_de_teste(), &mensagens);
        let (_, mensagens_lidas) =
            desserializar_de_markdown(&markdown).expect("deve desserializar sem erro");

        assert_eq!(mensagens_lidas, mensagens);
    }

    #[test]
    fn desserializar_sem_front_matter_e_erro_tratado() {
        let resultado = desserializar_de_markdown("## Usuário\n\noi\n");
        assert_eq!(resultado, Err(ErroDeSessao::FrontMatterAusente));
    }

    #[test]
    fn desserializar_com_papel_desconhecido_e_erro_tratado() {
        let markdown = serializar_para_markdown(&metadados_de_teste(), &[]);
        let markdown_com_papel_invalido = format!("{markdown}\n## Alienígena\n\ntexto\n");

        let resultado = desserializar_de_markdown(&markdown_com_papel_invalido);
        assert_eq!(
            resultado,
            Err(ErroDeSessao::PapelDesconhecido("Alienígena".to_string()))
        );
    }

    #[test]
    fn desserializar_com_json_invalido_num_bloco_de_tool_e_erro_tratado() {
        let markdown = serializar_para_markdown(&metadados_de_teste(), &[]);
        let markdown_com_json_ruim =
            format!("{markdown}\n## Agente\n\n```tool-call\nisto não é json\n```\n");

        let resultado = desserializar_de_markdown(&markdown_com_json_ruim);
        assert!(matches!(resultado, Err(ErroDeSessao::JsonInvalido(_))));
    }

    #[test]
    fn front_matter_com_linha_sem_dois_pontos_e_erro_tratado() {
        let resultado = desserializar_de_markdown("---\nisto não tem dois pontos\n---\n");
        assert!(matches!(
            resultado,
            Err(ErroDeSessao::FrontMatterInvalido(_))
        ));
    }
}
