// Caminho relativo: crates/cli/src/tui/keybind.rs
//! Tabela única de *keybindings* (MT-71/ADR-0027) — mapa nome de ação →
//! tecla *default* + descrição, mesmo espírito de
//! `packages/tui/src/config/keybind.ts` do OpenCode (referência de UX, não
//! de código — *stack* deles é TypeScript/SolidJS, só a **ideia** importa).
//! Widgets consultam a ação pelo nome (via [`resolve`]), nunca a tecla
//! bruta diretamente — desacopla o mapeamento de tecla da lógica de cada
//! widget.
//!
//! Customização de *keybind* pelo usuário fica fora de escopo desta ticket
//! (só a tabela *default* fixa abaixo).

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

/// Ação de UI resolvida a partir de uma tecla — a única forma pela qual o
/// laço de eventos e os widgets devem reagir a teclado (nunca inspecionando
/// `KeyCode`/`KeyModifiers` fora deste módulo).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    /// Sai do modo TUI.
    Quit,
    /// Rola o histórico de mensagens para cima (mensagens mais antigas).
    ScrollUp,
    /// Rola o histórico de mensagens para baixo (mensagens mais recentes).
    ScrollDown,
}

/// Uma entrada da tabela de *keybindings*: ação, tecla *default* (com
/// modificadores) e descrição legível — a descrição não é consumida ainda
/// nesta ticket (uma tela de ajuda é candidata natural futura), mas já faz
/// parte do "mapa único" desde já, em vez de acrescentada depois.
#[derive(Debug, Clone, Copy)]
pub struct KeyBinding {
    pub action: Action,
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
    pub description: &'static str,
}

/// Tabela *default* de *keybindings* — a única fonte de verdade de
/// tecla→ação consultada por [`resolve`].
pub const DEFINITIONS: &[KeyBinding] = &[
    KeyBinding {
        action: Action::Quit,
        code: KeyCode::Char('q'),
        modifiers: KeyModifiers::NONE,
        description: "sai do modo TUI",
    },
    KeyBinding {
        action: Action::Quit,
        code: KeyCode::Char('c'),
        modifiers: KeyModifiers::CONTROL,
        description: "sai do modo TUI",
    },
    KeyBinding {
        action: Action::ScrollUp,
        code: KeyCode::Up,
        modifiers: KeyModifiers::NONE,
        description: "rola o histórico para cima",
    },
    KeyBinding {
        action: Action::ScrollUp,
        code: KeyCode::Char('k'),
        modifiers: KeyModifiers::NONE,
        description: "rola o histórico para cima",
    },
    KeyBinding {
        action: Action::ScrollDown,
        code: KeyCode::Down,
        modifiers: KeyModifiers::NONE,
        description: "rola o histórico para baixo",
    },
    KeyBinding {
        action: Action::ScrollDown,
        code: KeyCode::Char('j'),
        modifiers: KeyModifiers::NONE,
        description: "rola o histórico para baixo",
    },
];

/// Resolve uma tecla pressionada para a [`Action`] correspondente,
/// consultando [`DEFINITIONS`] — tecla sem ação mapeada devolve `None`
/// (ignorada, mesmo padrão de "comando desconhecido não derruba o REPL" já
/// usado no REPL de texto, MT-14). Só considera eventos de **pressionar**
/// (`KeyEventKind::Press`) — terminais que também emitem evento de
/// *release* (ex.: Windows) dobrariam a ação se não filtrados aqui.
pub fn resolve(key: KeyEvent) -> Option<Action> {
    if key.kind != KeyEventKind::Press {
        return None;
    }
    DEFINITIONS
        .iter()
        .find(|def| def.code == key.code && def.modifiers == key.modifiers)
        .map(|def| def.action)
}

/// Monta a legenda de rodapé (`tecla: descrição · tecla: descrição · ...`)
/// a partir de [`DEFINITIONS`] — uma entrada por [`Action`] distinta (a
/// primeira tecla declarada para ela), lida direto da tabela em vez de um
/// texto solto mantido à parte — a mesma tabela que resolve teclas também
/// documenta a si mesma.
pub fn legenda() -> String {
    let mut vistas = std::collections::HashSet::new();
    DEFINITIONS
        .iter()
        .filter(|def| vistas.insert(def.action))
        .map(|def| format!("{}: {}", rotulo_tecla(def.code), def.description))
        .collect::<Vec<_>>()
        .join(" · ")
}

/// Rótulo curto de exibição de uma [`KeyCode`] — só as variantes usadas em
/// [`DEFINITIONS`] hoje; teclas fora dessas caem no `Debug` padrão.
fn rotulo_tecla(code: KeyCode) -> String {
    match code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        outro => format!("{outro:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tabela_nao_tem_duas_acoes_para_a_mesma_tecla_default() {
        let mut vistas = std::collections::HashSet::new();
        for def in DEFINITIONS {
            let chave = (def.code, def.modifiers);
            assert!(
                vistas.insert(chave),
                "tecla {:?} com modificadores {:?} mapeada mais de uma vez na tabela default",
                def.code,
                def.modifiers
            );
        }
    }

    #[test]
    fn resolucao_de_tecla_para_acao_funciona_para_todas_as_entradas_da_tabela() {
        for def in DEFINITIONS {
            let evento = KeyEvent::new(def.code, def.modifiers);
            assert_eq!(resolve(evento), Some(def.action));
        }
    }

    #[test]
    fn tecla_sem_acao_mapeada_nao_e_erro() {
        let evento = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(resolve(evento), None);
    }

    #[test]
    fn evento_de_release_e_ignorado_mesmo_para_tecla_mapeada() {
        let mut evento = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        evento.kind = KeyEventKind::Release;
        assert_eq!(resolve(evento), None);
    }

    #[test]
    fn legenda_tem_uma_entrada_por_acao_distinta_nunca_duas_para_quit() {
        let legenda = legenda();
        assert_eq!(legenda.matches("sai do modo TUI").count(), 1);
        assert!(legenda.contains("rola o histórico para cima"));
        assert!(legenda.contains("rola o histórico para baixo"));
    }
}
