// Caminho relativo: crates/core/src/guardrail/detector.rs
//! Detectores nomeados de dado pessoal (ADR-0043).
//!
//! Reconhecem PII por **formato**, não por palavra — o que fecha a evasão
//! observada na rodada 8: bloquear o literal `cpf` fez o modelo reformatar os
//! dados sem aquele rótulo e enviar o resto assim mesmo.
//!
//! **Sem motor de regex** (ADR-0007 §"sem regex" continua valendo, ADR-0043
//! explica por quê): varredura direta, custo linear no tamanho do texto, sem
//! *backtracking* possível e sem dependência nova.
//!
//! **Dígito verificador é obrigatório** onde existe (`cpf`/`cnpj` por módulo
//! 11, `cartao-credito` por Luhn). Sem essa checagem, qualquer identificador
//! numérico do mesmo tamanho — chave estrangeira, número de pedido — viraria
//! um achado, e um `redact` corromperia dado legítimo em silêncio. Falso
//! positivo aqui não é incômodo: é corrupção de saída sem aviso.
//!
//! Nenhuma função deste módulo devolve o **valor** encontrado — só posições e
//! contagens. É o que permite auditar "saíram 4 CPFs" sem transformar o audit
//! log num novo repositório de dado sensível (diretriz da ADR-0043).

use serde::{Deserialize, Serialize};

/// Marcador que substitui um achado sob `redact`.
///
/// Tamanho fixo e sem nenhum dígito do original: preservar "os quatro últimos"
/// é padrão comum em recibos, mas aqui reduziria o espaço de busca de quem
/// tentasse reconstruir o valor a partir do log.
pub const MARCADOR_REDIGIDO: &str = "[redigido]";

/// Detector embutido de dado pessoal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Detector {
    /// CPF, com ou sem pontuação, dígitos verificadores conferidos.
    Cpf,
    /// CNPJ, com ou sem pontuação, dígitos verificadores conferidos.
    Cnpj,
    /// Endereço de e-mail.
    Email,
    /// Telefone brasileiro (10 ou 11 dígitos, DDD válido), com ou sem
    /// pontuação e com `+55` opcional.
    TelefoneBr,
    /// Cartão de crédito (13 a 19 dígitos, validado por Luhn).
    CartaoCredito,
}

impl Detector {
    /// Nome estável do detector — o que aparece em auditoria e configuração.
    #[must_use]
    pub fn nome(self) -> &'static str {
        match self {
            Self::Cpf => "cpf",
            Self::Cnpj => "cnpj",
            Self::Email => "email",
            Self::TelefoneBr => "telefone-br",
            Self::CartaoCredito => "cartao-credito",
        }
    }

    /// Todos os achados em `texto`, como intervalos de bytes, em ordem e sem
    /// sobreposição.
    #[must_use]
    pub fn achados(self, texto: &str) -> Vec<(usize, usize)> {
        match self {
            Self::Cpf => achados_numericos(texto, 11, valida_cpf),
            Self::Cnpj => achados_numericos(texto, 14, valida_cnpj),
            Self::CartaoCredito => achados_de_cartao(texto),
            Self::Email => achados_de_email(texto),
            Self::TelefoneBr => achados_de_telefone(texto),
        }
    }
}

/// Substitui cada achado por [`MARCADOR_REDIGIDO`], devolvendo o texto novo e
/// quantos achados houve.
#[must_use]
pub fn redigir(texto: &str, detector: Detector) -> (String, usize) {
    let achados = detector.achados(texto);
    if achados.is_empty() {
        return (texto.to_string(), 0);
    }
    let mut saida = String::with_capacity(texto.len());
    let mut cursor = 0;
    for (inicio, fim) in &achados {
        saida.push_str(&texto[cursor..*inicio]);
        saida.push_str(MARCADOR_REDIGIDO);
        cursor = *fim;
    }
    saida.push_str(&texto[cursor..]);
    (saida, achados.len())
}

// ---- Varredura genérica de sequências numéricas pontuadas ----

/// `true` para os caracteres que podem aparecer *dentro* de um número
/// formatado (`123.456.789-00`, `(11) 98765-4321`) sem interromper a
/// sequência.
///
/// Os parênteses entram por causa do DDD; para `cpf`/`cnpj`/`cartao-credito`
/// isso só poderia criar achado falso se os dígitos verificadores conferissem
/// **através** do parêntese, o que a validação já barra.
fn e_pontuacao_interna(c: u8) -> bool {
    matches!(c, b'.' | b'-' | b'/' | b' ' | b'(' | b')')
}

/// Acha sequências com exatamente `digitos` dígitos (ignorando pontuação
/// interna) que passem em `valida`.
///
/// Uma sequência **maior** que `digitos` nunca é fatiada para caber: um número
/// de 15 dígitos não contém um CNPJ, contém outra coisa. Fatiar produziria
/// achados falsos em identificadores longos.
fn achados_numericos(
    texto: &str,
    digitos: usize,
    valida: fn(&[u8]) -> bool,
) -> Vec<(usize, usize)> {
    let bytes = texto.as_bytes();
    let mut achados = Vec::new();
    let mut i = 0;

    while i < bytes.len() {
        if !bytes[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        // Consome a sequência inteira de dígitos+pontuação a partir daqui.
        let inicio = i;
        let mut colhidos: Vec<u8> = Vec::with_capacity(digitos + 4);
        let mut fim = i;
        let mut j = i;
        while j < bytes.len() {
            if bytes[j].is_ascii_digit() {
                colhidos.push(bytes[j] - b'0');
                fim = j + 1;
                j += 1;
            } else if e_pontuacao_interna(bytes[j]) {
                // Pula a **corrida** inteira de pontuação e só continua se um
                // dígito vier logo depois. Um caractere de cada vez não daria
                // conta de `) ` em `(11) 98765-4321` (achado real de teste).
                let mut k = j;
                while k < bytes.len() && e_pontuacao_interna(bytes[k]) {
                    k += 1;
                }
                if k < bytes.len() && bytes[k].is_ascii_digit() {
                    j = k;
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        if colhidos.len() == digitos && valida(&colhidos) {
            achados.push((inicio, fim));
        }
        i = fim.max(inicio + 1);
    }
    achados
}

/// Dígitos verificadores de CPF (módulo 11).
fn valida_cpf(d: &[u8]) -> bool {
    if d.len() != 11 || d.iter().all(|x| *x == d[0]) {
        return false; // `111.111.111-11` passa no módulo 11 e não é CPF real.
    }
    for (tamanho, posicao) in [(9usize, 9usize), (10, 10)] {
        let soma: u32 = (0..tamanho)
            .map(|i| u32::from(d[i]) * u32::try_from(tamanho + 1 - i).unwrap_or(0))
            .sum();
        let resto = (soma * 10) % 11;
        let esperado = if resto == 10 { 0 } else { resto };
        if esperado != u32::from(d[posicao]) {
            return false;
        }
    }
    true
}

/// Dígitos verificadores de CNPJ (módulo 11, pesos 2..9 cíclicos).
fn valida_cnpj(d: &[u8]) -> bool {
    if d.len() != 14 || d.iter().all(|x| *x == d[0]) {
        return false;
    }
    for (tamanho, posicao) in [(12usize, 12usize), (13, 13)] {
        let mut peso = 2;
        let mut soma = 0u32;
        for i in (0..tamanho).rev() {
            soma += u32::from(d[i]) * peso;
            peso = if peso == 9 { 2 } else { peso + 1 };
        }
        let resto = soma % 11;
        let esperado = if resto < 2 { 0 } else { 11 - resto };
        if esperado != u32::from(d[posicao]) {
            return false;
        }
    }
    true
}

/// Algoritmo de Luhn, usado por cartões de crédito.
fn valida_luhn(d: &[u8]) -> bool {
    let mut soma = 0u32;
    for (indice, digito) in d.iter().rev().enumerate() {
        let mut v = u32::from(*digito);
        if indice % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        soma += v;
    }
    soma % 10 == 0
}

/// Cartão de crédito: 13 a 19 dígitos que passem em Luhn.
///
/// Faixa de tamanho (em vez de tamanho fixo) porque as bandeiras divergem —
/// 13 para Visa antigo, 19 para alguns co-branded.
fn achados_de_cartao(texto: &str) -> Vec<(usize, usize)> {
    (13..=19)
        .flat_map(|n| achados_numericos(texto, n, valida_luhn))
        .fold(Vec::new(), |mut acc: Vec<(usize, usize)>, achado| {
            // `achados_numericos` só casa a sequência inteira, então dois
            // tamanhos nunca produzem o mesmo intervalo; ordenar mantém a
            // saída determinística para o `redigir`.
            if !acc.contains(&achado) {
                acc.push(achado);
            }
            acc.sort_unstable();
            acc
        })
}

/// Telefone brasileiro: 10 ou 11 dígitos (com DDD), `+55` opcional.
///
/// Rejeita DDD fora de 11..=99 — sem isso, qualquer sequência de 10 dígitos
/// viraria telefone.
fn achados_de_telefone(texto: &str) -> Vec<(usize, usize)> {
    let valida = |d: &[u8]| {
        let ddd = u32::from(d[0]) * 10 + u32::from(d[1]);
        // Celular (11 dígitos) começa com 9 depois do DDD.
        (11..=99).contains(&ddd) && (d.len() == 10 || d[2] == 9)
    };
    let mut achados = achados_numericos(texto, 11, valida);
    achados.extend(achados_numericos(texto, 10, valida));
    achados.sort_unstable();
    achados
}

/// E-mail: `local@dominio.tld`, com pelo menos um ponto no domínio.
fn achados_de_email(texto: &str) -> Vec<(usize, usize)> {
    fn e_local(c: u8) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'%' | b'+' | b'-')
    }
    fn e_dominio(c: u8) -> bool {
        c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-')
    }

    let bytes = texto.as_bytes();
    let mut achados = Vec::new();
    for (pos, _) in texto.match_indices('@') {
        let mut inicio = pos;
        while inicio > 0 && e_local(bytes[inicio - 1]) {
            inicio -= 1;
        }
        let mut fim = pos + 1;
        while fim < bytes.len() && e_dominio(bytes[fim]) {
            fim += 1;
        }
        // Domínio não pode terminar em ponto/hífen (pega a pontuação da frase).
        while fim > pos + 1 && matches!(bytes[fim - 1], b'.' | b'-') {
            fim -= 1;
        }
        let dominio = &texto[pos + 1..fim];
        if inicio < pos && dominio.contains('.') && !dominio.starts_with('.') {
            achados.push((inicio, fim));
        }
    }
    achados
}

#[cfg(test)]
mod tests {
    use super::*;

    /// CPFs válidos (dígitos verificadores conferem) usados só nos testes.
    const CPF_VALIDO: &str = "529.982.247-25";
    const CPF_VALIDO_SEM_PONTUACAO: &str = "52998224725";

    #[test]
    fn cpf_valido_e_detectado_com_e_sem_pontuacao() {
        assert_eq!(Detector::Cpf.achados(CPF_VALIDO).len(), 1);
        assert_eq!(Detector::Cpf.achados(CPF_VALIDO_SEM_PONTUACAO).len(), 1);
    }

    /// O ponto da validação de dígito: um identificador qualquer de 11 dígitos
    /// **não** é CPF, e marcá-lo corromperia dado legítimo em silêncio.
    #[test]
    fn numero_de_11_digitos_que_nao_e_cpf_nao_e_detectado() {
        assert!(Detector::Cpf.achados("pedido 12345678901").is_empty());
    }

    #[test]
    fn cpf_com_todos_os_digitos_iguais_nao_e_detectado() {
        // Passa no módulo 11 por acidente aritmético, mas não é CPF real.
        assert!(Detector::Cpf.achados("111.111.111-11").is_empty());
    }

    #[test]
    fn sequencia_maior_que_o_tamanho_do_detector_nao_e_fatiada() {
        // 15 dígitos: não contém um CPF, contém outra coisa.
        let texto = format!("{CPF_VALIDO_SEM_PONTUACAO}9999");
        assert!(Detector::Cpf.achados(&texto).is_empty());
    }

    #[test]
    fn cnpj_valido_e_detectado_e_invalido_nao() {
        assert_eq!(Detector::Cnpj.achados("11.222.333/0001-81").len(), 1);
        assert!(Detector::Cnpj.achados("11.222.333/0001-99").is_empty());
    }

    #[test]
    fn email_e_detectado_sem_engolir_a_pontuacao_da_frase() {
        let texto = "escreva para ana.silva@exemplo.com.br, por favor.";
        let achados = Detector::Email.achados(texto);
        assert_eq!(achados.len(), 1);
        let (i, f) = achados[0];
        assert_eq!(&texto[i..f], "ana.silva@exemplo.com.br");
    }

    #[test]
    fn arroba_sem_dominio_valido_nao_e_email() {
        assert!(Detector::Email.achados("preco @ 10 reais").is_empty());
        assert!(Detector::Email.achados("usuario@localhost").is_empty());
    }

    #[test]
    fn telefone_br_detectado_e_ddd_invalido_recusado() {
        assert_eq!(Detector::TelefoneBr.achados("(11) 98765-4321").len(), 1);
        assert!(
            Detector::TelefoneBr.achados("0123456789").is_empty(),
            "DDD 01 não existe"
        );
    }

    #[test]
    fn cartao_valido_por_luhn_e_detectado() {
        assert_eq!(Detector::CartaoCredito.achados("4111111111111111").len(), 1);
        assert!(
            Detector::CartaoCredito
                .achados("4111111111111112")
                .is_empty(),
            "falha em Luhn não é cartão"
        );
    }

    #[test]
    fn redigir_substitui_por_marcador_de_tamanho_fixo_sem_preservar_digito() {
        let (saida, n) = redigir(&format!("CPF: {CPF_VALIDO} fim"), Detector::Cpf);
        assert_eq!(n, 1);
        assert_eq!(saida, format!("CPF: {MARCADOR_REDIGIDO} fim"));
        assert!(
            !saida.contains("25") && !saida.contains("529"),
            "nenhum dígito do original pode sobreviver: {saida}"
        );
    }

    #[test]
    fn redigir_lida_com_varios_achados_na_mesma_linha() {
        let texto = format!("{CPF_VALIDO} e {CPF_VALIDO_SEM_PONTUACAO}");
        let (saida, n) = redigir(&texto, Detector::Cpf);
        assert_eq!(n, 2);
        assert_eq!(saida, format!("{MARCADOR_REDIGIDO} e {MARCADOR_REDIGIDO}"));
    }

    #[test]
    fn texto_sem_achado_volta_intacto() {
        let (saida, n) = redigir("nada aqui", Detector::Cpf);
        assert_eq!(n, 0);
        assert_eq!(saida, "nada aqui");
    }

    /// O caso que motivou a ADR-0043: os dados da rodada 8, no formato em que
    /// vazaram — sem a palavra `cpf` em lugar nenhum.
    #[test]
    fn linha_de_csv_sem_a_palavra_cpf_ainda_e_detectada() {
        let linha = "1,Ana Ribeiro,529.982.247-25,ana@exemplo.test,enterprise";
        assert_eq!(Detector::Cpf.achados(linha).len(), 1);
        assert_eq!(Detector::Email.achados(linha).len(), 1);
    }

    #[test]
    fn nome_serializa_em_kebab_case_estavel() {
        assert_eq!(Detector::TelefoneBr.nome(), "telefone-br");
        assert_eq!(
            serde_json::to_string(&Detector::CartaoCredito).expect("serializa"),
            "\"cartao-credito\""
        );
    }
}
