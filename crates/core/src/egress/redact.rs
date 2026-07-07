// Caminho relativo: crates/core/src/egress/redact.rs
//! Redação de segredos antes de qualquer log/audit trail (ADR-0002, skill
//! `secrets-guard`), **sem** dependência de regex — um tokenizador simples
//! sobre `str` basta para o padrão de segredo mais comum e mantém a árvore de
//! dependências pequena e auditável (ADR-0001).
//!
//! Duas camadas independentes, aplicadas juntas ("defesa em profundidade"):
//!
//! 1. **Por nome de campo:** cabeçalhos/campos conhecidos como sempre
//!    sensíveis (`authorization`, `x-api-key`, `cookie`, ...) têm o valor
//!    inteiro substituído, **não importa o conteúdo**.
//! 2. **Por padrão no texto:** o texto é dividido em "palavras" (sequências de
//!    alfanuméricos, `-`, `_`, `.`) separadas por qualquer outro caractere
//!    (espaço, `=`, `?`, `&`, `:`, aspas, colchetes...). Isso isola
//!    corretamente um segredo colado como `chave=sk-...` ou
//!    `?token=ghp_...`, sem precisar de regex. Tokens `Bearer <...>`, chaves
//!    com prefixos conhecidos (`sk-`, `ghp_`, `AKIA`, ...) e JWTs são
//!    mascarados onde quer que apareçam.
//!
//! Esta lista de padrões é uma heurística defensiva, não uma prova formal de
//! ausência de segredo — ao integrar o transporte real (MT-07), qualquer
//! campo *conhecido* como sensível deve preferir a camada 1 (nome de campo).

/// Texto usado no lugar do segredo redigido.
pub const REDACTED_PLACEHOLDER: &str = "[REDACTED]";

/// Nomes de campo (headers, chaves de JSON) tratados como sempre sensíveis,
/// comparados sem diferenciar maiúsculas/minúsculas.
const SENSITIVE_FIELD_NAMES: &[&str] = &[
    "authorization",
    "proxy-authorization",
    "x-api-key",
    "api-key",
    "apikey",
    "cookie",
    "set-cookie",
    "x-auth-token",
];

/// Prefixos conhecidos de chaves/tokens de provedores comuns.
const KNOWN_SECRET_PREFIXES: &[&str] = &[
    "sk-",
    "sk_",
    "ghp_",
    "gho_",
    "ghu_",
    "ghs_",
    "ghr_",
    "github_pat_",
    "AKIA",
    "ASIA",
    "xoxb-",
    "xoxp-",
    "xoxa-",
    "glpat-",
    "ya29.",
    "AIza",
];

/// Indica se `field_name` é considerado sempre sensível (comparação
/// case-insensitive), independentemente do conteúdo do valor.
#[must_use]
pub fn is_sensitive_field(field_name: &str) -> bool {
    SENSITIVE_FIELD_NAMES
        .iter()
        .any(|nome| nome.eq_ignore_ascii_case(field_name))
}

/// Redige o valor de um campo nomeado: sempre mascarado se o nome for
/// sensível (camada 1); caso contrário, passa pela redação de texto livre
/// (camada 2).
#[must_use]
pub fn redact_field(field_name: &str, value: &str) -> String {
    if is_sensitive_field(field_name) {
        REDACTED_PLACEHOLDER.to_string()
    } else {
        redact_text(value)
    }
}

/// Um segmento do texto tokenizado: "palavra" candidata a segredo, ou
/// separador (preservado verbatim na saída).
enum Segment<'a> {
    /// Sequência contínua de alfanuméricos, `-`, `_` ou `.`.
    Word(&'a str),
    /// Qualquer sequência de caracteres fora do conjunto acima.
    Sep(&'a str),
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.'
}

/// Quebra o texto em segmentos de palavra/separador, tratando `=`, `?`, `&`,
/// `:`, espaços etc. como fronteiras — assim um segredo colado é isolado sem
/// precisar de regex. Segmentos não redigidos são reemitidos byte a byte.
fn tokenize(text: &str) -> Vec<Segment<'_>> {
    let mut segmentos = Vec::new();
    let mut inicio = 0;
    let mut em_palavra: Option<bool> = None;
    for (indice, ch) in text.char_indices() {
        let eh_palavra = is_word_char(ch);
        match em_palavra {
            None => em_palavra = Some(eh_palavra),
            Some(atual) if atual != eh_palavra => {
                segmentos.push(if atual {
                    Segment::Word(&text[inicio..indice])
                } else {
                    Segment::Sep(&text[inicio..indice])
                });
                inicio = indice;
                em_palavra = Some(eh_palavra);
            }
            Some(_) => {}
        }
    }
    if let Some(atual) = em_palavra {
        segmentos.push(if atual {
            Segment::Word(&text[inicio..])
        } else {
            Segment::Sep(&text[inicio..])
        });
    }
    segmentos
}

/// Índice do próximo [`Segment::Word`] a partir de `from`, se houver.
fn next_word_index(segmentos: &[Segment<'_>], from: usize) -> Option<usize> {
    segmentos[from..]
        .iter()
        .position(|s| matches!(s, Segment::Word(_)))
        .map(|offset| from + offset)
}

/// Redige padrões conhecidos de segredo em texto livre (corpo de mensagem,
/// linha de log, URL com query string etc.), preservando o restante do texto
/// byte a byte.
#[must_use]
pub fn redact_text(text: &str) -> String {
    let segmentos = tokenize(text);
    let mut saida = String::with_capacity(text.len());
    let mut i = 0;
    while i < segmentos.len() {
        match segmentos[i] {
            Segment::Sep(s) => {
                saida.push_str(s);
                i += 1;
            }
            Segment::Word(palavra) => {
                if palavra.eq_ignore_ascii_case("bearer") {
                    saida.push_str(palavra);
                    match next_word_index(&segmentos, i + 1) {
                        Some(prox) => {
                            for seg in &segmentos[i + 1..prox] {
                                if let Segment::Sep(s) = seg {
                                    saida.push_str(s);
                                }
                            }
                            saida.push_str(REDACTED_PLACEHOLDER);
                            i = prox + 1;
                        }
                        None => i += 1,
                    }
                    continue;
                }
                if looks_like_secret_word(palavra) {
                    saida.push_str(REDACTED_PLACEHOLDER);
                } else {
                    saida.push_str(palavra);
                }
                i += 1;
            }
        }
    }
    saida
}

fn looks_like_secret_word(word: &str) -> bool {
    KNOWN_SECRET_PREFIXES
        .iter()
        .any(|prefixo| word.starts_with(prefixo))
        || looks_like_jwt(word)
}

/// Heurística de JWT: três segmentos separados por `.`, cada um com
/// caracteres base64url e comprimento mínimo plausível de header/payload/assinatura.
fn looks_like_jwt(word: &str) -> bool {
    let partes: Vec<&str> = word.split('.').collect();
    partes.len() == 3
        && partes.iter().all(|parte| {
            parte.len() >= 10
                && parte
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redige_token_bearer_preservando_o_resto_da_frase() {
        let redigido = redact_text("Authorization: Bearer abc123.def456.ghi789 para o endpoint");
        assert!(!redigido.contains("abc123.def456.ghi789"));
        assert!(redigido.contains(REDACTED_PLACEHOLDER));
        assert!(redigido.contains("para o endpoint"));
    }

    #[test]
    fn redige_chaves_com_prefixos_conhecidos() {
        for chave in [
            "sk-proj-abcdEFGH12345",
            "ghp_1234567890abcdefghij",
            "AKIAABCDEFGHIJKLMNOP",
            "glpat-abcdefghijklmnopqrst",
        ] {
            let redigido = redact_text(&format!("chave={chave}"));
            assert!(
                !redigido.contains(chave),
                "chave {chave} não deveria sobreviver à redação"
            );
        }
    }

    #[test]
    fn redige_segredo_colado_em_query_string_sem_espacos() {
        let redigido = redact_text("gateway.interno/chat?token=ghp_1234567890abcdefghij&x=1");
        assert!(!redigido.contains("ghp_1234567890abcdefghij"));
        assert!(redigido.contains("gateway.interno/chat?token="));
        assert!(redigido.contains("&x=1"));
    }

    #[test]
    fn redige_jwt_mas_preserva_texto_comum() {
        let jwt = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dQw4w9WgXcQ";
        let redigido = redact_text(&format!("token de sessão: {jwt} recebido"));
        assert!(!redigido.contains(jwt));
        assert!(redigido.contains("token de sessão:"));
        assert!(redigido.contains("recebido"));
    }

    #[test]
    fn texto_sem_segredo_passa_intacto() {
        let texto = "leitura do arquivo Cargo.toml concluída com sucesso";
        assert_eq!(redact_text(texto), texto);
    }

    #[test]
    fn campo_sensivel_e_sempre_redigido_independente_do_conteudo() {
        assert_eq!(
            redact_field("Authorization", "qualquer coisa aqui"),
            REDACTED_PLACEHOLDER
        );
        assert_eq!(
            redact_field("X-API-Key", "não parece nem um segredo conhecido"),
            REDACTED_PLACEHOLDER
        );
    }

    #[test]
    fn campo_nao_sensivel_passa_pela_redacao_por_padrao() {
        let valor = redact_field("comentario", "chave sk-abc123defghij embutida");
        assert!(!valor.contains("sk-abc123defghij"));

        let valor = redact_field("comentario", "nada de especial aqui");
        assert_eq!(valor, "nada de especial aqui");
    }

    #[test]
    fn is_sensitive_field_ignora_maiusculas_minusculas() {
        assert!(is_sensitive_field("authorization"));
        assert!(is_sensitive_field("AUTHORIZATION"));
        assert!(is_sensitive_field("Cookie"));
        assert!(!is_sensitive_field("content-type"));
    }
}
