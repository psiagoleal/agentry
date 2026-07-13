// Caminho relativo: crates/core/src/guardrail/mod.rs
//! Guardrail Gate (MT-43, ADR-0007): correspondência **determinística** de
//! conteúdo — substring/palavra-chave, sem `regex` (mesma filosofia de
//! `tools::fs::FsSearchTool` e de `egress::redact`) — aplicada em dois
//! pontos de uma chamada de LLM: **entrada** (mensagem do usuário, antes de
//! ir ao provider) e **saída** (resposta do turno, antes do Reviewer/
//! ADR-0015 e antes de retornar ao chamador). Distinto do gate de permissão
//! de tools (`tools::permission`, MT-11) e da allowlist de egresso
//! (`egress::allowlist`, ADR-0002) — nenhum dos dois cobre conteúdo.
//!
//! `GuardrailGate` guarda as regras de cada lado; `GuardrailGate::check`
//! decide o efeito (`Allowed`/`Redacted`/`Blocked`) e audita toda regra que
//! efetivamente agiu via [`GuardrailAuditSink`] — um par novo, análogo a
//! `egress::audit::AuditEntry`/`AuditSink`, mas sem os campos
//! `profile`/`egress_class` (irrelevantes a uma checagem de conteúdo, ADR-0007
//! §6). Nunca loga o texto casado — só `direction`/`rule_id`/`action`/`task`.
//!
//! `block` sempre vence `redact` quando ambos casam no mesmo texto (checado
//! primeiro, sem exceção); múltiplas regras `redact` que casam aplicam todas
//! as máscaras, não só a primeira. Fiação com `Config`/`Session` fica para os
//! próximos tickets (MT-44/45) — este módulo não depende de nenhum dos dois.

use serde::{Deserialize, Serialize};

use crate::egress::redact::REDACTED_PLACEHOLDER;

/// Ação de uma [`GuardrailRule`] quando o padrão casa.
///
/// `Serialize`/`Deserialize` (`rename_all = "lowercase"`) reaproveitados
/// diretamente pelo schema de `Settings` (MT-44, ADR-0007 §2) — mesmo tipo
/// nos dois lados (regra em memória e regra vinda de
/// `agentry.settings.json`), sem um enum paralelo só para o JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GuardrailAction {
    /// Mascara o trecho casado com [`REDACTED_PLACEHOLDER`]; o turno segue normalmente.
    Redact,
    /// Substitui a mensagem inteira por um aviso fixo (fora deste módulo —
    /// ver `Session`, MT-45); nunca afrouxado por uma camada mais
    /// específica (ADR-0007 §3).
    Block,
}

impl GuardrailAction {
    /// Severidade da ação — `Block` > `Redact` — usada para resolver colisão
    /// de `id` entre camadas de configuração (MT-44): a mais severa sempre
    /// vence, nunca a mais permissiva. Mesmo papel de `EgressClass::rank`
    /// (ADR-0002), mas para duas ações em vez de três classes.
    #[must_use]
    pub fn rank(self) -> u8 {
        match self {
            Self::Redact => 0,
            Self::Block => 1,
        }
    }
}

impl std::fmt::Display for GuardrailAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Block => "block",
            Self::Redact => "redact",
        })
    }
}

/// Lado de uma chamada de LLM em que uma [`GuardrailRule`] se aplica.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardrailDirection {
    /// Mensagem do usuário, antes de ir ao provider.
    Input,
    /// Resposta do turno, antes do Reviewer/retorno ao chamador.
    Output,
}

impl std::fmt::Display for GuardrailDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Input => "input",
            Self::Output => "output",
        })
    }
}

/// Uma regra de guardrail: identificador (para log/aviso), padrão de
/// correspondência (substring literal, *case-insensitive*) e ação.
///
/// Mesmo tipo usado literalmente pelo schema de `Settings` (MT-44) — o JSON
/// usa `match` (nome reservado em Rust), daí o `rename` no campo
/// `match_text`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuardrailRule {
    /// Identificador único da regra — nunca o texto casado, aparece em
    /// avisos/auditoria no lugar dele.
    pub id: String,
    /// Substring a procurar, comparada sem diferenciar maiúsculas/minúsculas.
    /// Padrão vazio nunca casa (evita bloquear/redigir tudo por engano de
    /// configuração).
    #[serde(rename = "match")]
    pub match_text: String,
    pub action: GuardrailAction,
}

impl GuardrailRule {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        match_text: impl Into<String>,
        action: GuardrailAction,
    ) -> Self {
        Self {
            id: id.into(),
            match_text: match_text.into(),
            action,
        }
    }
}

/// Resultado de [`GuardrailGate::check`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GuardrailCheckResult {
    /// Nenhuma regra casou.
    Allowed,
    /// Uma ou mais regras `redact` casaram — texto já mascarado.
    Redacted(String),
    /// Uma regra `block` casou — carrega o `id` da regra, não o texto.
    Blocked(String),
}

/// Entrada de auditoria de uma regra que efetivamente agiu (nunca para
/// `Allowed`) — par análogo a `egress::audit::AuditEntry`, sem
/// `profile`/`egress_class` (a `Session`, único chamador, não possui nenhum
/// dos dois — ADR-0007 §6). Nunca carrega o texto casado nem o conteúdo da
/// mensagem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuardrailAuditEntry {
    pub direction: GuardrailDirection,
    pub rule_id: String,
    pub action: GuardrailAction,
    pub task: String,
}

impl std::fmt::Display for GuardrailAuditEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[guardrail] {} {} -> {} (regra: {})",
            self.task, self.direction, self.action, self.rule_id
        )
    }
}

/// Recebe cada [`GuardrailAuditEntry`] produzida por [`GuardrailGate::check`].
///
/// Análogo a `egress::transport::AuditSink` — só emitido quando uma regra
/// efetivamente age; uma correspondência ausente (`Allowed`) nunca gera
/// entrada, mesmo espírito do módulo de egresso (só audita tentativas de
/// fato).
pub trait GuardrailAuditSink: Send + Sync {
    fn record(&self, entry: GuardrailAuditEntry);
}

/// As regras de guardrail de um lado (entrada) e do outro (saída) de uma
/// chamada de LLM.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GuardrailGate {
    pub input: Vec<GuardrailRule>,
    pub output: Vec<GuardrailRule>,
}

impl GuardrailGate {
    fn regras(&self, direction: GuardrailDirection) -> &[GuardrailRule] {
        match direction {
            GuardrailDirection::Input => &self.input,
            GuardrailDirection::Output => &self.output,
        }
    }

    /// Checa `texto` contra as regras de `direction`. `block` é sempre
    /// checado primeiro — qualquer regra `block` que case vence, mesmo que
    /// uma regra `redact` também casasse no mesmo texto (ADR-0007 §3/§4);
    /// só quando nenhuma `block` casa é que as `redact` são aplicadas, todas
    /// que casarem, não só a primeira. Toda regra que efetivamente age gera
    /// uma [`GuardrailAuditEntry`] via `sink`; `Allowed` nunca audita nada.
    pub fn check(
        &self,
        direction: GuardrailDirection,
        texto: &str,
        task: &str,
        sink: &dyn GuardrailAuditSink,
    ) -> GuardrailCheckResult {
        let regras = self.regras(direction);

        for regra in regras {
            if regra.action == GuardrailAction::Block
                && contains_case_insensitive(texto, &regra.match_text)
            {
                sink.record(GuardrailAuditEntry {
                    direction,
                    rule_id: regra.id.clone(),
                    action: GuardrailAction::Block,
                    task: task.to_string(),
                });
                return GuardrailCheckResult::Blocked(regra.id.clone());
            }
        }

        let mut atual = texto.to_string();
        let mut alguma_redacao = false;
        for regra in regras {
            if regra.action != GuardrailAction::Redact {
                continue;
            }
            if let Some(mascarado) = mask_all_case_insensitive(&atual, &regra.match_text) {
                atual = mascarado;
                alguma_redacao = true;
                sink.record(GuardrailAuditEntry {
                    direction,
                    rule_id: regra.id.clone(),
                    action: GuardrailAction::Redact,
                    task: task.to_string(),
                });
            }
        }

        if alguma_redacao {
            GuardrailCheckResult::Redacted(atual)
        } else {
            GuardrailCheckResult::Allowed
        }
    }
}

/// `texto` contém `padrao` (comparação *case-insensitive*, sem `regex`)?
/// Padrão vazio nunca casa.
fn contains_case_insensitive(texto: &str, padrao: &str) -> bool {
    !padrao.is_empty()
        && texto
            .to_ascii_lowercase()
            .contains(&padrao.to_ascii_lowercase())
}

/// Mascara todas as ocorrências (*case-insensitive*) de `padrao` em `texto`
/// com [`REDACTED_PLACEHOLDER`]; `None` se nada casar. Compara via
/// `to_ascii_lowercase` (não `regex`, ADR-0007 §1) — deslocamentos de byte
/// preservados porque a conversão ASCII nunca muda o comprimento da string,
/// diferente de um `to_lowercase` Unicode genérico; limitação aceita: só
/// casa variação de maiúsculas/minúsculas ASCII, mesma disciplina prática já
/// usada alhures no crate (`egress::redact`, `tools::shell::ShellPolicy`).
fn mask_all_case_insensitive(texto: &str, padrao: &str) -> Option<String> {
    if padrao.is_empty() {
        return None;
    }
    let padrao_lower = padrao.to_ascii_lowercase();
    let texto_lower = texto.to_ascii_lowercase();
    if !texto_lower.contains(&padrao_lower) {
        return None;
    }

    let mut resultado = String::with_capacity(texto.len());
    let mut restante = texto;
    let mut restante_lower = texto_lower.as_str();
    while let Some(pos) = restante_lower.find(&padrao_lower) {
        resultado.push_str(&restante[..pos]);
        resultado.push_str(REDACTED_PLACEHOLDER);
        let fim = pos + padrao_lower.len();
        restante = &restante[fim..];
        restante_lower = &restante_lower[fim..];
    }
    resultado.push_str(restante);
    Some(resultado)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct SinkColetor(Mutex<Vec<GuardrailAuditEntry>>);

    impl GuardrailAuditSink for SinkColetor {
        fn record(&self, entry: GuardrailAuditEntry) {
            self.0
                .lock()
                .expect("mutex do coletor não deve envenenar")
                .push(entry);
        }
    }

    impl SinkColetor {
        fn entradas(&self) -> Vec<GuardrailAuditEntry> {
            self.0
                .lock()
                .expect("mutex do coletor não deve envenenar")
                .clone()
        }
    }

    #[test]
    fn substring_casa_case_insensitive() {
        let gate = GuardrailGate {
            input: vec![GuardrailRule::new(
                "bloqueia-senha",
                "SENHA:",
                GuardrailAction::Block,
            )],
            output: vec![],
        };
        let sink = SinkColetor::default();

        let resultado = gate.check(
            GuardrailDirection::Input,
            "minha senha: 12345",
            "tarefa de teste",
            &sink,
        );

        assert_eq!(
            resultado,
            GuardrailCheckResult::Blocked("bloqueia-senha".to_string())
        );
    }

    #[test]
    fn block_vence_redact_quando_ambos_casam_no_mesmo_texto() {
        let gate = GuardrailGate {
            input: vec![
                GuardrailRule::new("mascara-foo", "foo", GuardrailAction::Redact),
                GuardrailRule::new("bloqueia-bar", "bar", GuardrailAction::Block),
            ],
            output: vec![],
        };
        let sink = SinkColetor::default();

        let resultado = gate.check(
            GuardrailDirection::Input,
            "texto com foo e bar juntos",
            "tarefa",
            &sink,
        );

        assert_eq!(
            resultado,
            GuardrailCheckResult::Blocked("bloqueia-bar".to_string())
        );
        let entradas = sink.entradas();
        assert_eq!(
            entradas.len(),
            1,
            "block vence antes de qualquer redact ser sequer avaliado"
        );
        assert_eq!(entradas[0].action, GuardrailAction::Block);
    }

    #[test]
    fn multiplos_redact_mascaram_todas_as_ocorrencias_nao_so_a_primeira() {
        let gate = GuardrailGate {
            input: vec![],
            output: vec![
                GuardrailRule::new("mascara-foo", "foo", GuardrailAction::Redact),
                GuardrailRule::new("mascara-bar", "BAR", GuardrailAction::Redact),
            ],
        };
        let sink = SinkColetor::default();

        let resultado = gate.check(
            GuardrailDirection::Output,
            "foo aparece e bar também aparece, e foo de novo",
            "tarefa",
            &sink,
        );

        let GuardrailCheckResult::Redacted(texto) = resultado else {
            panic!("esperava Redacted");
        };
        assert!(!texto.to_ascii_lowercase().contains("foo"));
        assert!(!texto.to_ascii_lowercase().contains("bar"));
        assert_eq!(texto.matches(REDACTED_PLACEHOLDER).count(), 3);
        assert_eq!(sink.entradas().len(), 2, "uma entrada por regra que agiu");
    }

    #[test]
    fn nenhuma_regra_casando_devolve_allowed_sem_gerar_entrada() {
        let gate = GuardrailGate {
            input: vec![GuardrailRule::new(
                "bloqueia-x",
                "palavra-nao-presente",
                GuardrailAction::Block,
            )],
            output: vec![],
        };
        let sink = SinkColetor::default();

        let resultado = gate.check(
            GuardrailDirection::Input,
            "texto qualquer sem relação",
            "tarefa",
            &sink,
        );

        assert_eq!(resultado, GuardrailCheckResult::Allowed);
        assert!(sink.entradas().is_empty());
    }

    #[test]
    fn regra_que_age_gera_exatamente_uma_entrada_nunca_com_o_texto_casado() {
        let gate = GuardrailGate {
            input: vec![],
            output: vec![GuardrailRule::new(
                "mascara-segredo",
                "segredo-abc",
                GuardrailAction::Redact,
            )],
        };
        let sink = SinkColetor::default();

        gate.check(
            GuardrailDirection::Output,
            "a resposta contém segredo-abc no meio",
            "tarefa-x",
            &sink,
        );

        let entradas = sink.entradas();
        assert_eq!(entradas.len(), 1);
        assert_eq!(entradas[0].rule_id, "mascara-segredo");
        let exibicao = entradas[0].to_string();
        assert!(!exibicao.contains("segredo-abc"));
    }

    #[test]
    fn padrao_vazio_nunca_casa() {
        let gate = GuardrailGate {
            input: vec![GuardrailRule::new(
                "regra-vazia",
                "",
                GuardrailAction::Block,
            )],
            output: vec![],
        };
        let sink = SinkColetor::default();

        let resultado = gate.check(GuardrailDirection::Input, "qualquer coisa", "tarefa", &sink);

        assert_eq!(resultado, GuardrailCheckResult::Allowed);
        assert!(sink.entradas().is_empty());
    }

    #[test]
    fn gate_sem_regras_nunca_bloqueia_nem_redige() {
        let gate = GuardrailGate::default();
        let sink = SinkColetor::default();

        let resultado = gate.check(GuardrailDirection::Output, "qualquer resposta", "t", &sink);

        assert_eq!(resultado, GuardrailCheckResult::Allowed);
    }

    #[test]
    fn rank_do_block_e_maior_que_o_do_redact() {
        assert!(GuardrailAction::Block.rank() > GuardrailAction::Redact.rank());
    }

    #[test]
    fn display_da_entrada_de_auditoria_nao_expoe_texto_casado() {
        let entrada = GuardrailAuditEntry {
            direction: GuardrailDirection::Output,
            rule_id: "mascara-segredo".to_string(),
            action: GuardrailAction::Redact,
            task: "tarefa-x".to_string(),
        };
        let texto = entrada.to_string();
        assert!(texto.contains("mascara-segredo"));
        assert!(texto.contains("redact"));
        assert!(texto.contains("output"));
    }
}
