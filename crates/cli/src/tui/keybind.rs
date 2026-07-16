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
    /// Envia o conteúdo atual da caixa de entrada como mensagem do usuário
    /// (MT-72).
    Send,
    /// Abre o seletor de modelo/*provider* (MT-73) — reinterpretada pelo
    /// laço de eventos conforme o modo ativo (fora do seletor: abre; dentro
    /// dele, sem efeito adicional).
    OpenModelPicker,
    /// Cancela um modo secundário aberto (ex.: fecha o seletor de modelo
    /// sem escolher nada, recusa uma confirmação de tool pendente) — sem
    /// efeito no modo de chat normal.
    Cancel,
    /// Alterna o *toggle* `auto`/`normal` de confirmação de tool sob `ask`
    /// (MT-74) — **nunca** afeta uma tool sob `deny` (invariante estrutural
    /// de `RegistryToolExecutor::execute`, não desta tecla).
    ToggleAuto,
    /// Desfaz o checkpoint mais recente de `fs_write`/`fs_edit` (MT-88,
    /// ADR-0030) — mesma `CheckpointStore::undo()` da flag `--undo`
    /// (*one-shot*) e do comando `/undo` (REPL).
    Undo,
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
///
/// **Revisão do MT-72** (registrada em `docs/decisoes-autonomas.md`): os
/// atalhos de letra do MT-70/71 (`q` para sair, `k`/`j` para rolar) foram
/// removidos daqui — a partir desta ticket existe uma caixa de entrada de
/// texto real, e uma letra não pode significar simultaneamente "ação fixa"
/// e "caractere digitado" sem um modo explícito (fora de escopo). `Ctrl+C`
/// (não ambíguo, convenção universal de terminal) continua saindo em
/// qualquer contexto; setas continuam rolando (não colidem com digitação).
pub const DEFINITIONS: &[KeyBinding] = &[
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
        action: Action::ScrollDown,
        code: KeyCode::Down,
        modifiers: KeyModifiers::NONE,
        description: "rola o histórico para baixo",
    },
    KeyBinding {
        action: Action::Send,
        code: KeyCode::Enter,
        modifiers: KeyModifiers::NONE,
        description: "envia a mensagem digitada",
    },
    KeyBinding {
        action: Action::OpenModelPicker,
        code: KeyCode::Char('p'),
        modifiers: KeyModifiers::CONTROL,
        description: "abre o seletor de modelo/provider",
    },
    KeyBinding {
        action: Action::Cancel,
        code: KeyCode::Esc,
        modifiers: KeyModifiers::NONE,
        description: "fecha o seletor/recusa a confirmação",
    },
    KeyBinding {
        action: Action::ToggleAuto,
        code: KeyCode::Char('a'),
        modifiers: KeyModifiers::CONTROL,
        description: "alterna confirmação automática de tools sob ask",
    },
    KeyBinding {
        action: Action::Undo,
        code: KeyCode::Char('z'),
        modifiers: KeyModifiers::CONTROL,
        description: "desfaz o último fs_write/fs_edit (checkpoint)",
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
        let mut evento = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        evento.kind = KeyEventKind::Release;
        assert_eq!(resolve(evento), None);
    }

    #[test]
    fn letra_solta_sem_modificador_nunca_e_uma_acao_fixa() {
        // MT-72: 'q'/'k'/'j' precisam ficar livres para a caixa de entrada
        // de texto — só Ctrl+C sai, só setas rolam.
        for c in ['q', 'k', 'j'] {
            let evento = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
            assert_eq!(resolve(evento), None, "'{c}' não deveria ser uma ação fixa");
        }
    }

    #[test]
    fn legenda_tem_uma_entrada_por_acao_distinta_nunca_duas_para_quit() {
        let legenda = legenda();
        assert_eq!(legenda.matches("sai do modo TUI").count(), 1);
        assert!(legenda.contains("rola o histórico para cima"));
        assert!(legenda.contains("rola o histórico para baixo"));
    }
}
