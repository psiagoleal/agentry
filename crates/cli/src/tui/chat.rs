// Caminho relativo: crates/cli/src/tui/chat.rs
//! Estado de renderização da view de chat (MT-72/ADR-0027) — traduz
//! [`StreamEvent`] em atualizações de um histórico de mensagens. Puro e
//! testável sem terminal real: o laço de eventos
//! (`crates/cli/src/tui/mod.rs`) só chama [`ChatState::registrar_mensagem_usuario`]/
//! [`ChatState::aplicar_evento`] — nenhuma lógica de *streaming* mora aqui.

use agentry_core::model::StreamEvent;

/// Quem produziu uma [`Mensagem`] do histórico.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Autor {
    Usuario,
    Agente,
}

/// Um turno do histórico: quem falou, o texto acumulado até agora, e se o
/// turno já terminou (`StreamEvent::MessageEnd`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mensagem {
    pub autor: Autor,
    pub texto: String,
    pub concluida: bool,
}

/// Histórico de mensagens da view de chat.
#[derive(Debug, Default)]
pub struct ChatState {
    mensagens: Vec<Mensagem>,
}

impl ChatState {
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn mensagens(&self) -> &[Mensagem] {
        &self.mensagens
    }

    /// Adiciona a mensagem do usuário ao histórico e abre, na sequência, um
    /// turno vazio (ainda não concluído) para a resposta do agente que está
    /// prestes a começar — [`aplicar_evento`](Self::aplicar_evento) sempre
    /// escreve nesse último turno.
    pub fn registrar_mensagem_usuario(&mut self, texto: String) {
        self.mensagens.push(Mensagem {
            autor: Autor::Usuario,
            texto,
            concluida: true,
        });
        self.mensagens.push(Mensagem {
            autor: Autor::Agente,
            texto: String::new(),
            concluida: false,
        });
    }

    /// Aplica um [`StreamEvent`] ao turno do agente em aberto (o último da
    /// lista) — `TextDelta` cresce o texto acumulado, `MessageEnd` marca o
    /// turno como concluído. Sem turno aberto (nenhuma mensagem enviada
    /// ainda), o evento é ignorado, não um erro. Outros eventos
    /// (`MessageStart`, `ToolCallStart`/`Delta`) não têm representação
    /// visual nesta ticket — confirmação de tool via widget é o MT-74.
    pub fn aplicar_evento(&mut self, evento: &StreamEvent) {
        let Some(ultima) = self.mensagens.last_mut() else {
            return;
        };
        match evento {
            StreamEvent::TextDelta { text } => ultima.texto.push_str(text),
            StreamEvent::MessageEnd { .. } => ultima.concluida = true,
            StreamEvent::MessageStart
            | StreamEvent::ToolCallStart { .. }
            | StreamEvent::ToolCallDelta { .. } => {}
        }
    }

    /// Marca o turno em aberto como concluído, anexando `mensagem` ao texto
    /// acumulado — usado quando `Session::run_streaming` devolve `Err`
    /// (falha do provider/router/reviewer): o turno nunca fica pendurado
    /// indefinidamente como "ainda respondendo". Sem turno aberto, é
    /// ignorado (mesmo padrão de `aplicar_evento`).
    pub fn marcar_erro(&mut self, mensagem: &str) {
        let Some(ultima) = self.mensagens.last_mut() else {
            return;
        };
        if !ultima.texto.is_empty() {
            ultima.texto.push_str("\n\n");
        }
        ultima.texto.push_str("[erro] ");
        ultima.texto.push_str(mensagem);
        ultima.concluida = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::model::Usage;

    #[test]
    fn registrar_mensagem_usuario_abre_um_turno_vazio_para_o_agente() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());

        assert_eq!(estado.mensagens().len(), 2);
        assert_eq!(estado.mensagens()[0].autor, Autor::Usuario);
        assert_eq!(estado.mensagens()[0].texto, "oi");
        assert!(estado.mensagens()[0].concluida);
        assert_eq!(estado.mensagens()[1].autor, Autor::Agente);
        assert_eq!(estado.mensagens()[1].texto, "");
        assert!(!estado.mensagens()[1].concluida);
    }

    #[test]
    fn text_delta_cresce_o_texto_acumulado_do_turno_aberto() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());

        estado.aplicar_evento(&StreamEvent::TextDelta { text: "ol".into() });
        estado.aplicar_evento(&StreamEvent::TextDelta { text: "á!".into() });

        assert_eq!(estado.mensagens().last().unwrap().texto, "olá!");
    }

    #[test]
    fn message_end_marca_o_turno_como_concluido() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());
        assert!(!estado.mensagens().last().unwrap().concluida);

        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });

        assert!(estado.mensagens().last().unwrap().concluida);
    }

    #[test]
    fn evento_sem_turno_aberto_e_ignorado_sem_panic() {
        let mut estado = ChatState::new();

        estado.aplicar_evento(&StreamEvent::TextDelta { text: "x".into() });

        assert!(estado.mensagens().is_empty());
    }

    #[test]
    fn eventos_sem_representacao_visual_nao_alteram_o_texto() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());

        estado.aplicar_evento(&StreamEvent::MessageStart);
        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "1".into(),
            name: "fs_read".into(),
        });
        estado.aplicar_evento(&StreamEvent::ToolCallDelta {
            id: "1".into(),
            delta: "{}".into(),
        });

        assert_eq!(estado.mensagens().last().unwrap().texto, "");
        assert!(!estado.mensagens().last().unwrap().concluida);
    }

    #[test]
    fn marcar_erro_conclui_o_turno_com_a_mensagem_anexada() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());
        estado.aplicar_evento(&StreamEvent::TextDelta {
            text: "começando a respo".into(),
        });

        estado.marcar_erro("erro do provider: timeout");

        let ultima = estado.mensagens().last().unwrap();
        assert!(ultima.concluida);
        assert!(ultima.texto.contains("começando a respo"));
        assert!(ultima.texto.contains("erro do provider: timeout"));
    }

    #[test]
    fn marcar_erro_sem_turno_aberto_e_ignorado_sem_panic() {
        let mut estado = ChatState::new();

        estado.marcar_erro("erro do provider: timeout");

        assert!(estado.mensagens().is_empty());
    }
}
