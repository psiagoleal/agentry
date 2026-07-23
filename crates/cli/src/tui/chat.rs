// Caminho relativo: crates/cli/src/tui/chat.rs
//! Estado de renderização da view de chat (MT-72/ADR-0027) — traduz
//! [`StreamEvent`] em atualizações de um histórico de mensagens. Puro e
//! testável sem terminal real: o laço de eventos
//! (`crates/cli/src/tui/mod.rs`) só chama [`ChatState::registrar_mensagem_usuario`]/
//! [`ChatState::aplicar_evento`] — nenhuma lógica de *streaming* mora aqui.

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

/// Um trecho de conteúdo de uma [`Mensagem`] (ADR-0035/MT-115) — texto
/// corrido ou uma chamada de tool, cada uma com seu próprio ciclo de vida
/// (`id`/nome conhecidos desde `ToolCallStart`, argumentos acumulados por
/// `ToolCallDelta`, resultado quando/se `ToolCallResult` chegar). Substituiu
/// a `String` única de antes do MT-115 — necessário para dar suporte real a
/// recolher/expandir uma chamada de tool (escopo do MT-116/117): uma
/// `String` com o marcador embutido no meio do texto não tem como
/// "endereçar" o pedaço certo pra trocar de conteúdo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bloco {
    Texto(String),
    Tool {
        id: String,
        nome: String,
        argumentos: String,
        resultado: Option<(String, bool)>,
        expandido: bool,
    },
}

/// Um turno do histórico: quem falou, os blocos acumulados até agora, e se
/// o turno já terminou (`StreamEvent::MessageEnd`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mensagem {
    pub autor: Autor,
    pub blocos: Vec<Bloco>,
    pub concluida: bool,
}

impl Mensagem {
    /// Texto visível equivalente ao que a mensagem inteira mostrava antes
    /// do MT-115 — concatena os blocos de texto e, para cada bloco de
    /// tool, a mesma linha de marcador `⚙ usando <nome>...` de sempre,
    /// seguida do *checklist* de `todo_write` quando os argumentos
    /// acumulados já interpretam (MT-107/ADR-0034). Calculado sob demanda
    /// em vez de anexado ao texto no `MessageEnd` — evita ter que decidir
    /// "já anexei esse *checklist* ou não" quando o mesmo turno tem
    /// múltiplas rodadas de tool-call (blocos não são mais limpos a cada
    /// `MessageEnd`, diferente do antigo `chamadas_em_andamento`).
    ///
    /// Estado de expansão/resultado ainda não influencia esta saída — é
    /// escopo do MT-116. Existe pra [`super::montar_linhas_do_historico`]
    /// continuar processando uma `String` só, sem mudar sua lógica de
    /// Markdown/wrap neste ticket (regressão visual zero).
    pub(crate) fn texto_visivel(&self) -> String {
        let mut saida = String::new();
        for bloco in &self.blocos {
            match bloco {
                Bloco::Texto(texto) => saida.push_str(texto),
                Bloco::Tool {
                    nome, argumentos, ..
                } => {
                    if !saida.is_empty() && !saida.ends_with('\n') {
                        saida.push('\n');
                    }
                    saida.push_str("⚙ usando ");
                    saida.push_str(nome);
                    saida.push_str("...\n");
                    if nome == "todo_write" {
                        if let Some(checklist) = formatar_checklist_todo(argumentos) {
                            saida.push_str(&checklist);
                        }
                    }
                }
            }
        }
        saida
    }
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
            blocos: vec![Bloco::Texto(texto)],
            concluida: true,
        });
        self.mensagens.push(Mensagem {
            autor: Autor::Agente,
            blocos: Vec::new(),
            concluida: false,
        });
    }

    /// Aplica um [`StreamEvent`] ao turno do agente em aberto (o último da
    /// lista) — `TextDelta` cresce o bloco de texto corrente (ou abre um
    /// novo, se o último bloco for uma chamada de tool), `ToolCallStart`
    /// abre um novo [`Bloco::Tool`] (`draw`, em `crates/cli/src/tui/mod.rs`,
    /// ainda o renderiza como o marcador `⚙ usando <tool>...` de sempre —
    /// achado num teste manual de usabilidade: a TUI não mostrava nada
    /// enquanto o agente criava/editava arquivos), `ToolCallDelta` acumula
    /// nos argumentos do bloco correspondente (por `id`), `MessageEnd`
    /// marca o turno como concluído.
    ///
    /// `ToolCallResult` (ADR-0035/MT-114) é tratado à parte, **antes** de
    /// pegar a última mensagem: o resultado de uma tool chega **depois**
    /// do `MessageEnd` do turno que a pediu, mas ainda dentro do mesmo
    /// turno de usuário (`Session::run_streaming` só fecha esta mensagem
    /// quando o usuário manda a próxima) — então o bloco correspondente
    /// ainda está na mensagem em aberto na prática, mas a busca varre todas
    /// as mensagens (mais recente primeiro) por segurança, em vez de supor
    /// essa ordem. Sem turno aberto (nenhuma mensagem enviada ainda), todo
    /// evento é ignorado, não um erro.
    pub fn aplicar_evento(&mut self, evento: &StreamEvent) {
        if let StreamEvent::ToolCallResult {
            id,
            content,
            is_error,
        } = evento
        {
            for mensagem in self.mensagens.iter_mut().rev() {
                let alvo = mensagem
                    .blocos
                    .iter_mut()
                    .rev()
                    .find_map(|bloco| match bloco {
                        Bloco::Tool {
                            id: bid, resultado, ..
                        } if bid == id => Some(resultado),
                        _ => None,
                    });
                if let Some(resultado) = alvo {
                    *resultado = Some((content.clone(), *is_error));
                    return;
                }
            }
            return;
        }

        let Some(ultima) = self.mensagens.last_mut() else {
            return;
        };
        match evento {
            StreamEvent::MessageStart => {}
            StreamEvent::TextDelta { text } => match ultima.blocos.last_mut() {
                Some(Bloco::Texto(acumulado)) => acumulado.push_str(text),
                _ => ultima.blocos.push(Bloco::Texto(text.clone())),
            },
            StreamEvent::ToolCallStart { id, name } => {
                ultima.blocos.push(Bloco::Tool {
                    id: id.clone(),
                    nome: name.clone(),
                    argumentos: String::new(),
                    resultado: None,
                    expandido: false,
                });
            }
            StreamEvent::ToolCallDelta { id, delta } => {
                let bloco = ultima
                    .blocos
                    .iter_mut()
                    .rev()
                    .find_map(|bloco| match bloco {
                        Bloco::Tool {
                            id: bid,
                            argumentos,
                            ..
                        } if bid == id => Some(argumentos),
                        _ => None,
                    });
                if let Some(argumentos) = bloco {
                    argumentos.push_str(delta);
                }
            }
            StreamEvent::MessageEnd { .. } => {
                ultima.concluida = true;
            }
            StreamEvent::ToolCallResult { .. } => {
                unreachable!("tratado acima, antes do `last_mut`")
            }
        }
    }

    /// Marca o turno em aberto como concluído, anexando `mensagem` como um
    /// novo bloco de texto — usado quando `Session::run_streaming` devolve
    /// `Err` (falha do provider/router/reviewer): o turno nunca fica
    /// pendurado indefinidamente como "ainda respondendo". Sempre um bloco
    /// **novo** (nunca anexado a um bloco de tool em aberto, que não faria
    /// sentido). Sem turno aberto, é ignorado (mesmo padrão de
    /// `aplicar_evento`).
    pub fn marcar_erro(&mut self, mensagem: &str) {
        let Some(ultima) = self.mensagens.last_mut() else {
            return;
        };
        let mut texto_erro = String::new();
        if !ultima.blocos.is_empty() {
            texto_erro.push_str("\n\n");
        }
        texto_erro.push_str("[erro] ");
        texto_erro.push_str(mensagem);
        ultima.blocos.push(Bloco::Texto(texto_erro));
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
            blocos: vec![Bloco::Texto(texto)],
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
        assert_eq!(estado.mensagens()[0].texto_visivel(), "oi");
        assert!(estado.mensagens()[0].concluida);
        assert_eq!(estado.mensagens()[1].autor, Autor::Agente);
        assert_eq!(estado.mensagens()[1].texto_visivel(), "");
        assert!(!estado.mensagens()[1].concluida);
    }

    #[test]
    fn text_delta_cresce_o_texto_acumulado_do_turno_aberto() {
        let mut estado = ChatState::new();
        estado.registrar_mensagem_usuario("oi".into());

        estado.aplicar_evento(&StreamEvent::TextDelta { text: "ol".into() });
        estado.aplicar_evento(&StreamEvent::TextDelta { text: "á!".into() });

        assert_eq!(estado.mensagens().last().unwrap().texto_visivel(), "olá!");
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
            estado.mensagens().last().unwrap().texto_visivel(),
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
            estado.mensagens().last().unwrap().texto_visivel(),
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

        assert_eq!(estado.mensagens().last().unwrap().texto_visivel(), "");
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

        let texto = estado.mensagens().last().unwrap().texto_visivel();
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

        let texto = estado.mensagens().last().unwrap().texto_visivel();
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

        let texto = estado.mensagens().last().unwrap().texto_visivel();
        assert_eq!(
            texto, "⚙ usando fs_read...\n",
            "argumentos de outra tool nunca viram checklist, mesmo parecendo válidos"
        );
    }

    #[test]
    fn tool_call_delta_com_id_de_turno_anterior_nao_vaza_para_o_turno_novo() {
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
        // deveria existir na prática) não acha nenhum bloco de tool na
        // mensagem nova (`registrar_mensagem_usuario` sempre abre
        // `blocos: Vec::new()`) -- é ignorado, mesmo padrão de um id
        // desconhecido.
        estado.registrar_mensagem_usuario("segunda".into());
        estado.aplicar_evento(&StreamEvent::ToolCallDelta {
            id: "call_1".into(),
            delta: r#"{"items":[]}"#.into(),
        });
        estado.aplicar_evento(&StreamEvent::MessageEnd {
            usage: Usage::default(),
        });

        assert_eq!(estado.mensagens().last().unwrap().texto_visivel(), "");
    }

    // --- ToolCallResult (ADR-0035/MT-114) -- armazenamento de passagem,
    // consumo/exibição de verdade só a partir do MT-115 ---

    /// Acha o bloco de tool com `id` na mensagem mais recente que o contém
    /// e devolve seu `resultado` -- auxiliar de teste (não existe acessor
    /// público equivalente, por enquanto: consumo real do resultado é
    /// escopo do MT-116).
    fn resultado_de(estado: &ChatState, id: &str) -> Option<(String, bool)> {
        estado.mensagens().iter().rev().find_map(|mensagem| {
            mensagem.blocos.iter().find_map(|bloco| match bloco {
                Bloco::Tool {
                    id: bid, resultado, ..
                } if bid == id => resultado.clone(),
                _ => None,
            })
        })
    }

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

        assert_eq!(resultado_de(&estado, "call_1"), None);

        estado.aplicar_evento(&StreamEvent::ToolCallResult {
            id: "call_1".into(),
            content: "arquivo criado".into(),
            is_error: false,
        });

        assert_eq!(
            resultado_de(&estado, "call_1"),
            Some(("arquivo criado".to_string(), false))
        );
    }

    #[test]
    fn tool_call_result_sobrevive_ao_message_end_do_proprio_turno() {
        // O bloco de tool continua na mesma mensagem em aberto -- o
        // resultado chega DEPOIS do MessageEnd do turno que pediu a tool,
        // mas antes do próximo `registrar_mensagem_usuario`, então ainda
        // encontra o bloco certo.
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
            resultado_de(&estado, "call_1"),
            Some(("erro: permissão negada".to_string(), true))
        );
    }

    #[test]
    fn resultado_de_tool_desconhecido_e_none() {
        let estado = ChatState::new();
        assert_eq!(resultado_de(&estado, "nunca-existiu"), None);
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
        let texto = ultima.texto_visivel();
        assert!(texto.contains("começando a respo"));
        assert!(texto.contains("erro do provider: timeout"));
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
        assert_eq!(
            estado.mensagens()[0].texto_visivel(),
            "[undo] 'a.txt' restaurado"
        );
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
        assert_eq!(estado.mensagens()[1].texto_visivel(), "respondendo...");
        assert!(
            !estado.mensagens()[1].concluida,
            "o turno do agente em voo continua em aberto, intocado"
        );
        assert_eq!(
            estado.mensagens()[2].texto_visivel(),
            "[undo] 'a.txt' restaurado"
        );
    }
}
