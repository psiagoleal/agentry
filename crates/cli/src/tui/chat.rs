// Caminho relativo: crates/cli/src/tui/chat.rs
//! Estado de renderização da view de chat (MT-72/ADR-0027) — traduz
//! [`StreamEvent`] em atualizações de um histórico de mensagens. Puro e
//! testável sem terminal real: o laço de eventos
//! (`crates/cli/src/tui/mod.rs`) só chama [`ChatState::registrar_mensagem_usuario`]/
//! [`ChatState::aplicar_evento`] — nenhuma lógica de *streaming* mora aqui.

use std::collections::HashMap;

use agentry_core::model::StreamEvent;

/// Interpreta `argumentos_json` (acumulado de `ToolCallDelta`) como os
/// argumentos de `todo_write` e formata um *checklist* legível — `None`
/// quando o JSON não interpreta (fragmentos ainda incompletos, ou um
/// modelo confuso mandando algo malformado; achado real da rodada 4:
/// modelos locais mais fracos às vezes erram a forma dos argumentos).
/// Nunca entra em pânico — só `serde_json`/acesso a campo, tudo com
/// `Option`. `[x]` concluído, `[~]` em andamento, `[ ]` pendente (também o
/// padrão para um `status` desconhecido — degrada para "ainda não feito"
/// em vez de esconder o item).
fn formatar_checklist_todo(argumentos_json: &str) -> Option<String> {
    let valor: serde_json::Value = serde_json::from_str(argumentos_json).ok()?;
    let items = valor.get("items")?.as_array()?;
    if items.is_empty() {
        return None;
    }

    let mut saida = String::from("lista de tarefas:\n");
    for item in items {
        let conteudo = item.get("content")?.as_str()?;
        let status = item
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending");
        let marcador = match status {
            "completed" => "[x]",
            "in_progress" => "[~]",
            _ => "[ ]",
        };
        saida.push_str("  ");
        saida.push_str(marcador);
        saida.push(' ');
        saida.push_str(conteudo);
        saida.push('\n');
    }
    Some(saida)
}

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

/// Chamada de tool em andamento na mensagem aberta, reacumulada a partir de
/// `ToolCallStart`/`ToolCallDelta` (MT-107, ADR-0034) — só existe para
/// reconstituir os argumentos completos de `todo_write` ao final do turno
/// (nenhuma outra tool precisa disso hoje: seus fragmentos de JSON
/// continuam sem representação visual). Descartada a cada `MessageEnd`.
#[derive(Debug)]
struct ChamadaEmAndamento {
    nome: String,
    argumentos: String,
}

/// Histórico de mensagens da view de chat.
#[derive(Debug, Default)]
pub struct ChatState {
    mensagens: Vec<Mensagem>,
    chamadas_em_andamento: HashMap<String, ChamadaEmAndamento>,
    /// Resultados de tool já executados (`StreamEvent::ToolCallResult`,
    /// ADR-0035/MT-114), por `id` de chamada — armazenamento só de
    /// passagem para este ticket (o consumo/exibição de verdade, e a
    /// decisão de quando limpar cada entrada, é escopo do MT-115, que vai
    /// substituir o texto corrido de `Mensagem` por blocos estruturados).
    /// **Deliberadamente não compartilha o ciclo de vida de
    /// `chamadas_em_andamento`**: aquele mapa é limpo a cada `MessageEnd`
    /// (MT-107), mas o resultado de uma tool chega **depois** do
    /// `MessageEnd` do turno que a chamou (`Session::run_streaming` emite
    /// `ToolCallResult` só depois de repassar os eventos do turno) — se
    /// reaproveitássemos o mesmo mapa, a entrada já teria sido apagada
    /// antes do resultado chegar.
    resultados_de_tools: HashMap<String, (String, bool)>,
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
    /// turno como concluído, `ToolCallStart` acrescenta um marcador inline
    /// (`⚙ usando <tool>...`) para dar alguma visibilidade de que o agente
    /// está agindo, não só respondendo texto — achado num teste manual de
    /// usabilidade (a TUI não mostrava nada enquanto o agente criava/editava
    /// arquivos). `draw` (`crates/cli/src/tui/mod.rs`) estiliza essas linhas
    /// de forma diferente do texto normal.
    ///
    /// `ToolCallStart`/`ToolCallDelta` também alimentam
    /// [`Self::chamadas_em_andamento`] (MT-107, ADR-0034) — reacumula os
    /// fragmentos de JSON do lado da TUI (mesma técnica do
    /// `StreamAggregator` privado do núcleo, só que na camada de
    /// renderização) só para reconstituir os argumentos de `todo_write` ao
    /// `MessageEnd`; um *checklist* formatado é anexado ao turno quando o
    /// JSON acumulado interpreta corretamente, silenciosamente ignorado
    /// (sem pânico, sem erro exibido) quando não — o marcador genérico já
    /// emitido continua sendo o único traço visível nesse caso. Nenhuma
    /// outra tool usa esse mapa; seus fragmentos continuam sem
    /// representação própria. Sem turno aberto (nenhuma mensagem enviada
    /// ainda), o evento é ignorado, não um erro.
    pub fn aplicar_evento(&mut self, evento: &StreamEvent) {
        if let StreamEvent::ToolCallStart { id, name } = evento {
            self.chamadas_em_andamento.insert(
                id.clone(),
                ChamadaEmAndamento {
                    nome: name.clone(),
                    argumentos: String::new(),
                },
            );
        }
        if let StreamEvent::ToolCallDelta { id, delta } = evento {
            if let Some(chamada) = self.chamadas_em_andamento.get_mut(id) {
                chamada.argumentos.push_str(delta);
            }
        }

        let Some(ultima) = self.mensagens.last_mut() else {
            return;
        };
        match evento {
            StreamEvent::TextDelta { text } => ultima.texto.push_str(text),
            StreamEvent::ToolCallStart { name, .. } => {
                if !ultima.texto.is_empty() && !ultima.texto.ends_with('\n') {
                    ultima.texto.push('\n');
                }
                ultima.texto.push_str("⚙ usando ");
                ultima.texto.push_str(name);
                ultima.texto.push_str("...\n");
            }
            StreamEvent::MessageEnd { .. } => {
                ultima.concluida = true;
                for chamada in self.chamadas_em_andamento.values() {
                    if chamada.nome != "todo_write" {
                        continue;
                    }
                    if let Some(checklist) = formatar_checklist_todo(&chamada.argumentos) {
                        if !ultima.texto.is_empty() && !ultima.texto.ends_with('\n') {
                            ultima.texto.push('\n');
                        }
                        ultima.texto.push_str(&checklist);
                    }
                }
                self.chamadas_em_andamento.clear();
            }
            StreamEvent::MessageStart | StreamEvent::ToolCallDelta { .. } => {}
            StreamEvent::ToolCallResult {
                id,
                content,
                is_error,
            } => {
                self.resultados_de_tools
                    .insert(id.clone(), (content.clone(), *is_error));
            }
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

    /// Acrescenta uma mensagem independente ao histórico, já concluída —
    /// usada por eventos que não são um turno de chat (MT-88, ADR-0030:
    /// resultado de `Ctrl+Z`/*undo*), diferente de [`Self::marcar_erro`],
    /// que anexa ao turno **já aberto** em vez de criar um novo. Pode ser
    /// chamada a qualquer momento (histórico vazio ou não).
    pub fn registrar_mensagem_sistema(&mut self, texto: String) {
        self.mensagens.push(Mensagem {
            autor: Autor::Agente,
            texto,
            concluida: true,
        });
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
    fn tool_call_start_acrescenta_marcador_inline_visivel() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("crie um arquivo".into());

        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "fs_write".into(),
        });

        assert_eq!(
            estado.mensagens().last().unwrap().texto,
            "⚙ usando fs_write...\n"
        );
    }

    #[test]
    fn tool_call_start_depois_de_texto_pula_linha_antes_do_marcador() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());
        estado.aplicar_evento(&StreamEvent::TextDelta {
            text: "vou criar o arquivo".into(),
        });

        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "fs_write".into(),
        });

        assert_eq!(
            estado.mensagens().last().unwrap().texto,
            "vou criar o arquivo\n⚙ usando fs_write...\n"
        );
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
    fn message_start_e_tool_call_delta_nao_tem_representacao_visual_propria() {
        // `ToolCallStart` É visível (marcador inline, ver os testes
        // `tool_call_start_*` acima) — só `MessageStart`/`ToolCallDelta`
        // continuam sem efeito no texto (fragmentos de JSON de argumento
        // não são legíveis).
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());

        estado.aplicar_evento(&StreamEvent::MessageStart);
        estado.aplicar_evento(&StreamEvent::ToolCallDelta {
            id: "1".into(),
            delta: "{}".into(),
        });

        assert_eq!(estado.mensagens().last().unwrap().texto, "");
        assert!(!estado.mensagens().last().unwrap().concluida);
    }

    // --- formatar_checklist_todo / integração com todo_write (MT-107,
    // ADR-0034) ---

    #[test]
    fn formatar_checklist_todo_com_json_valido_monta_o_checklist() {
        let json = r#"{"items":[
            {"content":"ler o arquivo","status":"completed"},
            {"content":"editar o arquivo","status":"in_progress"},
            {"content":"rodar os testes","status":"pending"}
        ]}"#;

        let checklist = formatar_checklist_todo(json).expect("JSON válido deve formatar");

        assert_eq!(
            checklist,
            "lista de tarefas:\n  [x] ler o arquivo\n  [~] editar o arquivo\n  [ ] rodar os testes\n"
        );
    }

    #[test]
    fn formatar_checklist_todo_com_json_invalido_devolve_none() {
        assert_eq!(formatar_checklist_todo("isto não é json"), None);
        assert_eq!(formatar_checklist_todo(r#"{"items": "não é array"}"#), None);
        assert_eq!(
            formatar_checklist_todo(r#"{"items":[{"status":"pending"}]}"#),
            None,
            "item sem 'content' invalida a lista inteira"
        );
    }

    #[test]
    fn formatar_checklist_todo_com_lista_vazia_devolve_none() {
        assert_eq!(formatar_checklist_todo(r#"{"items":[]}"#), None);
    }

    #[test]
    fn formatar_checklist_todo_status_desconhecido_vira_pendente() {
        let json = r#"{"items":[{"content":"x","status":"algo-estranho"}]}"#;

        let checklist = formatar_checklist_todo(json).unwrap();

        assert_eq!(checklist, "lista de tarefas:\n  [ ] x\n");
    }

    #[test]
    fn todo_write_com_json_valido_anexa_checklist_ao_turno() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("faça uma tarefa de vários passos".into());

        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "todo_write".into(),
        });
        estado.aplicar_evento(&StreamEvent::ToolCallDelta {
            id: "call_1".into(),
            delta: r#"{"items":[{"content":"passo 1","#.into(),
        });
        estado.aplicar_evento(&StreamEvent::ToolCallDelta {
            id: "call_1".into(),
            delta: r#""status":"pending"}]}"#.into(),
        });
        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });

        let texto = &estado.mensagens().last().unwrap().texto;
        assert!(texto.contains("⚙ usando todo_write..."));
        assert!(texto.contains("[ ] passo 1"), "texto: {texto:?}");
    }

    #[test]
    fn todo_write_com_json_invalido_nao_anexa_checklist() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());

        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "todo_write".into(),
        });
        estado.aplicar_evento(&StreamEvent::ToolCallDelta {
            id: "call_1".into(),
            delta: "isto não fecha o json".into(),
        });
        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });

        let texto = &estado.mensagens().last().unwrap().texto;
        assert_eq!(
            texto, "⚙ usando todo_write...\n",
            "só o marcador genérico, sem pânico"
        );
    }

    #[test]
    fn tool_diferente_de_todo_write_nao_gera_checklist() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("leia um arquivo".into());

        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "fs_read".into(),
        });
        estado.aplicar_evento(&StreamEvent::ToolCallDelta {
            id: "call_1".into(),
            delta: r#"{"items":[{"content":"x","status":"pending"}]}"#.into(),
        });
        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });

        let texto = &estado.mensagens().last().unwrap().texto;
        assert_eq!(
            texto, "⚙ usando fs_read...\n",
            "argumentos de outra tool nunca viram checklist, mesmo parecendo válidos"
        );
    }

    #[test]
    fn mapa_de_chamadas_em_andamento_reseta_a_cada_message_end() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("primeira".into());
        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "todo_write".into(),
        });
        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });

        // Segundo turno: um ToolCallDelta com o MESMO id de antes (não
        // deveria existir na prática, mas prova que o mapa foi limpo) não
        // reaproveita nada do turno anterior.
        estado.registrar_mensagem_usuario("segunda".into());
        estado.aplicar_evento(&StreamEvent::ToolCallDelta {
            id: "call_1".into(),
            delta: r#"{"items":[]}"#.into(),
        });
        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });

        assert_eq!(estado.mensagens().last().unwrap().texto, "");
    }

    // --- ToolCallResult (ADR-0035/MT-114) -- armazenamento de passagem,
    // consumo/exibição de verdade só a partir do MT-115 ---

    #[test]
    fn tool_call_result_fica_disponivel_por_id() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("crie um arquivo".into());
        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "fs_write".into(),
        });
        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });

        assert_eq!(estado.resultados_de_tools.get("call_1"), None);

        estado.aplicar_evento(&StreamEvent::ToolCallResult {
            id: "call_1".into(),
            content: "arquivo criado".into(),
            is_error: false,
        });

        assert_eq!(
            estado.resultados_de_tools.get("call_1"),
            Some(&("arquivo criado".to_string(), false))
        );
    }

    #[test]
    fn tool_call_result_sobrevive_ao_message_end_do_proprio_turno() {
        // Diferente de `chamadas_em_andamento` (limpo a cada MessageEnd),
        // o resultado chega DEPOIS do MessageEnd do turno que pediu a
        // tool -- não pode ser perdido por causa dessa limpeza.
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("crie um arquivo".into());
        estado.aplicar_evento(&StreamEvent::ToolCallStart {
            id: "call_1".into(),
            name: "fs_write".into(),
        });
        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });
        estado.aplicar_evento(&StreamEvent::ToolCallResult {
            id: "call_1".into(),
            content: "erro: permissão negada".into(),
            is_error: true,
        });

        assert_eq!(
            estado.resultados_de_tools.get("call_1"),
            Some(&("erro: permissão negada".to_string(), true))
        );
    }

    #[test]
    fn resultado_de_tool_desconhecido_e_none() {
        let estado = ChatState::new();
        assert_eq!(estado.resultados_de_tools.get("nunca-existiu"), None);
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

    #[test]
    fn registrar_mensagem_sistema_acrescenta_uma_mensagem_concluida_independente_do_turno() {
        let mut estado = ChatState::new();

        // Sem nenhum turno aberto (histórico vazio) — diferente de
        // `marcar_erro`, que exige um turno já em aberto.
        estado.registrar_mensagem_sistema("[undo] 'a.txt' restaurado".into());

        assert_eq!(estado.mensagens().len(), 1);
        assert_eq!(estado.mensagens()[0].texto, "[undo] 'a.txt' restaurado");
        assert!(estado.mensagens()[0].concluida);
    }

    #[test]
    fn registrar_mensagem_sistema_nao_altera_um_turno_ja_em_aberto() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());
        estado.aplicar_evento(&StreamEvent::TextDelta {
            text: "respondendo...".into(),
        });

        estado.registrar_mensagem_sistema("[undo] 'a.txt' restaurado".into());

        assert_eq!(
            estado.mensagens().len(),
            3,
            "mensagem de sistema é um turno novo, não anexado ao turno do agente em voo"
        );
        assert_eq!(estado.mensagens()[1].texto, "respondendo...");
        assert!(
            !estado.mensagens()[1].concluida,
            "o turno do agente em voo continua em aberto, intocado"
        );
        assert_eq!(estado.mensagens()[2].texto, "[undo] 'a.txt' restaurado");
    }
}
