<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> Opcional em projetos solo; recomendado em colaborações. Atualizado a cada commit.
> Não inclua segredos. Mantido conforme a skill `handoff-updater`.

## Último turno

- **Data:** 2026-07-16
- **Branch:** `main`
- **Commit:** `60b1b41`
- **Fase:** Roadmap v0.1..v0.4 **fechados/imutáveis**; **Fase 10 concluída** (LiteLLM).
  **Execução autônoma em andamento** (`/loop /implementar-roadmap`, modelo Sonnet 5) — ver
  `docs/decisoes-autonomas.md` para decisões tomadas sozinho (**5 decisões registradas** até a
  Fase 15: MT-55 síntese de defaults de task-class deferida à CLI; ADR-0023 parser de
  frontmatter de `SKILL.md` próprio em vez de dependência YAML; revisão dos *keybindings* de
  letra do MT-71 no MT-72; `NoopAuditSink` sob `--tui` no MT-72; extensão de escopo com
  `Router::route_entry` no MT-73). **Fase 11 concluída inteira** (ADR-0020,
  `.agentryignore`, MT-52..54, `roadmap-v0.5.md`). **Fase 12 concluída inteira** (ADR-0021/0022,
  config de task-class de ponta a ponta, MT-55..58, `roadmap-v0.6.md`) — o tema mais enfatizado
  pelo usuário no planejamento original. **Fase 13 concluída inteira** (ADR-0023 `Accepted`,
  `docs/roadmap-v0.7.md`, MT-59..62) — memória de projeto (`AGENTS.md`/`CLAUDE.md`) e skills
  (*progressive disclosure* completo, descoberta + tool `skill`); **ADR-0003 também promovida a
  `Accepted`** (objetivo original — consumo de artefatos do `profiles` — cumprido). **Fase 14
  concluída inteira** (ADR-0024/0025/0026 `Accepted`, `docs/roadmap-v0.8.md`, MT-63..69) —
  `AskUser`, `WebFetch`+`WebSearch` (ADR-0025 inteira), `glob`, `shell_background`, documentação
  completa. **Housekeeping:** ADR-0020/0021/0022 promovidas de `Proposed` para `Accepted` (suas
  fases já concluídas há várias iterações; status ficou desatualizado).

  **Parada dura resolvida em 2026-07-15:** o mantenedor autorizou explicitamente `ratatui`
  (Fase 15) e `rmcp` (Fase 16) — as duas dependências que haviam pausado o loop autônomo (ver
  histórico abaixo). **Fase 15 preparada** (ADR-0027 `Proposed`, `docs/roadmap-v0.9.md`,
  MT-70..76) — maturidade de `ratatui` verificada de fato via `crates.io/api/v1/crates/ratatui`
  antes de fechar a ADR (MIT, 37,9M *downloads*, ativo desde 2023). Pronta para começar a
  implementação a partir do MT-70.

  **Fase 15 concluída inteira em 2026-07-15** (ADR-0027 `Accepted`, `docs/roadmap-v0.9.md`,
  MT-70..76) — *scaffold* `ratatui`/`crossterm`, tabela de *keybindings*, *streaming* real
  (`Session::run_streaming` numa *task* separada + canal, zero mudança em `crates/core`),
  seletor de modelo/*provider* por busca difusa, `TuiConfirmer`/`TuiPrompter` (com a
  invariante estrutural "`auto` nunca aprova sob `deny`"), visualizador de diff (LCS
  implementado do zero) e documentação de usuário. Três achados corrigidos durante a
  implementação, todos registrados em `docs/decisoes-autonomas.md`: revisão dos *keybindings*
  de letra do MT-71 (colidiam com a digitação real do MT-72); `NoopAuditSink` sob `--tui`
  (`eprintln!` corrompia a tela alternativa do `crossterm`); extensão do escopo de arquivos do
  MT-73 com `Router::route_entry` (acessor de leitura, não lógica nova). Confirmação de tool
  via LLM real não pôde ser demonstrada de ponta a ponta em nenhum dos smoke-tests manuais
  (MT-74/75) — mesmo achado de confiabilidade de tool-calling local já documentado desde o
  MT-61, não um defeito do código; a fiação em si tem cobertura automatizada completa.

  **Fase 16 preparada** — ADR-0028 (`Proposed`) decide: `rmcp` só com as *features*
  `client`+`transport-child-process` em produção (maturidade verificada via
  `crates.io/api/v1/crates/rmcp`: Apache-2.0, 15,9M *downloads*, repositório oficial
  `modelcontextprotocol/rust-sdk`); **v1 só suporta servidores MCP locais** (subprocesso,
  `stdio`) — servidores remotos exigiriam o cliente HTTP embutido do `rmcp`, que bypassaria o
  `Transport` único do projeto (ADR-0001) sem `Allowlist`/auditoria, uma questão de
  *fail-closed* (ADR-0002) explicitamente adiada para uma fase dedicada, nunca resolvida via
  atalho. `rmcp` vive em `crates/core` (mesmo lugar de `lsp-types`, ADR-0013); tools MCP
  entram no `ToolRegistry` com nome prefixado pelo servidor (`"<servidor>__<tool>"`), sob o
  mesmo `PermissionGate` de sempre. `docs/roadmap-v0.10.md` detalha os 5 tickets (MT-77..81 —
  numeração retoma do MT-77, livre desde que o *widget* de lista de tarefas foi descartado na
  preparação da Fase 15). Pronta para começar a implementação a partir do MT-77.

  **MT-77 concluído** — primeiro ticket de implementação da Fase 16: `rmcp` adicionado a
  `crates/core/Cargo.toml` (só `client`+`transport-child-process`, ainda não usado em código
  Rust — mesmo padrão de MT-55/56 já usado para `taskClasses`, schema antes de consumo). Novo
  bloco `mcpServers` em `agentry.settings.json`: `McpServerSettings { command, args,
  egressClass }`, `egress_class` sempre obrigatória, rejeitada em `Settings::from_json_str`
  quando diferente de `local-only` (`ConfigError::McpServerEgressNotSupported`) — antes mesmo
  do merge entre camadas, nunca conectada. `merge_mcp_servers` substitui a entrada inteira por
  nome (não mescla campo a campo como `taskClasses` — sem semântica clara de herdar só parte
  de "como spawnar este servidor"). Exemplo `--init` usa `echo` como comando inerte (decisão
  registrada em `docs/decisoes-autonomas.md`: `mcpServers` não tem a mesma camada de seleção
  explícita que torna os exemplos reais de `taskClasses` seguros). 6 testes novos + teste do
  exemplo `--init` estendido. Smoke-test manual: `--init` gera o bloco corretamente, JSON
  válido; carregar a config gerada e rodar uma tarefa real não falha.

  **MT-78 concluído** — `crates/core/src/mcp/mod.rs` (novo): `McpClient` spawna um servidor
  MCP via `rmcp::transport::child_process::TokioChildProcess`, completa o *handshake*
  (`ServiceExt::serve`) e lista as tools via `list_all_tools()`. Nenhum `Drop` manual
  necessário — o próprio `TokioChildProcess` do `rmcp` mata o subprocesso quando descartado
  (`ChildWithCleanup::drop`), validado empiricamente pelo teste de integração. **Achado
  técnico registrado em `docs/decisoes-autonomas.md`:** a primeira tentativa de fixture de
  teste usou a *feature* `server` do `rmcp` em `[dev-dependencies]` — compilou com `cargo
  build --bins --tests`, mas falhou em `cargo build --release` real, porque um alvo `[[bin]]`
  de `crates/core` (como `fake_mcp_server`) só recebe *features* de `[dependencies]`, nunca as
  de `[dev-dependencies]` (Cargo só estende `dev-dependencies` para `tests`/`examples`).
  Resolvido implementando o protocolo MCP na mão em `fake_mcp_server.rs` (JSON-RPC 2.0
  *newline-delimited* — mais simples que o `Content-Length` do LSP), usando os tipos de
  `rmcp::model` (sem *feature gate*, disponíveis só com `client`) para respostas corretas sem
  hand-typing nomes de campo. 3 testes de integração (`ciclo_de_vida_completo`, `Drop` sem
  `shutdown` não deixa processo órfão, comando inexistente é erro tratado) + 1 unitário.
  `cargo build --release` limpo — confirma que a superfície de produção do `rmcp` continua só
  `client`+`transport-child-process`.

  **MT-79 concluído** — `crates/core/src/tools/mcp.rs` (novo): cada tool descoberta por
  `McpClient` (MT-78) vira uma `McpTool` implementando a *trait* `Tool` (MT-11), nome de
  registro prefixado pelo servidor (`"<servidor>__<tool>"`, ADR-0028); `execute()` encaminha
  para o novo `McpClient::call_tool` — sob o mesmo `ToolRegistry`/`PermissionGate` de qualquer
  outra tool. `register_mcp_tools` (`crates/cli/src/main.rs`) conecta a cada servidor
  configurado; falha de **um** servidor é não-fatal (`stderr` + `continue`), não trava a CLI
  nem os demais servidores — distinto do MT-77, onde config inválida já é erro fatal.
  Corrigido durante o teste de ponta a ponta um *hang* no `fake_mcp_server` (MT-78): método
  sem resposta deixava o cliente `rmcp` real esperando para sempre, sem *timeout* próprio;
  agora responde `tools/call` de verdade e devolve erro JSON-RPC `-32601` para qualquer método
  desconhecido em vez de ignorar em silêncio. 8 testes novos (3 unitários + 5 de integração),
  `cargo build --release` limpo, smoke-test manual confirma que um servidor MCP fora do ar não
  impede a CLI de completar uma tarefa.

  **MT-80 concluído** — `McpClient::start_from_settings` (novo, `crates/core/src/mcp/mod.rs`):
  defesa em profundidade além do `Settings::from_json_str` (MT-77) — checa
  `egress_class == LocalOnly` **antes** de tocar em `Command`/`TokioChildProcess`, devolvendo
  `McpError::EgressNotSupported` sem spawnar nenhum subprocesso caso contrário. Garante que
  nenhum caminho de código, inclusive um `McpServerSettings` montado direto em Rust sem passar
  pelo parser, chega a conectar um servidor com classe de egresso diferente de `local-only`.
  `register_mcp_tools` (`crates/cli/src/main.rs`) passa a usar esse ponto de entrada em vez de
  extrair `command`/`args` manualmente; `McpClient::start` continua existindo à parte só
  porque a suíte de testes (`fake_mcp_server`) não passa por `McpServerSettings`. 2 testes
  novos (egresso remoto rejeitado sem spawnar; `local-only` com comando inexistente ainda
  falha ao spawnar, comportamento preservado). 368 testes em `agentry-core`, 104 em `agentry`,
  `cargo build --release` limpo. Nenhuma mudança de comportamento observável da CLI (o
  `Settings::from_json_str` já bloqueava esse caso antes de chegar aqui) — sem smoke-test
  manual adicional além do já feito no MT-79.

  **MT-81 concluído — fecha a Fase 16 inteira (MT-77..81).** `docs/usuario/configuracao.md`
  ganha a seção `mcpServers` (schema, exemplo real com `npx`/
  `@modelcontextprotocol/server-filesystem`, nome de tool sempre prefixado pelo servidor,
  nota de que um servidor fora do ar não trava a CLI). `docs/usuario/uso.md` ganha uma nota
  curta em "Ferramentas do agente". `docs/governanca/privacidade-e-egresso.md` ganha a seção
  "MCP e egresso" para o público de *compliance*: por que só servidores locais são suportados
  agora, a checagem em dois pontos independentes (*parsing* MT-77 + `start_from_settings`
  MT-80), por que servidores remotos ficam fora até uma fase dedicada, e que o que um
  subprocesso de servidor faz por conta própria não é controlado pelo `agentry` (mesmo nível
  de confiança de `shell_exec`). **ADR-0028 promovida de `Proposed` para `Accepted`**
  (`docs/adr/README.md` atualizado); `docs/roadmap-longo-prazo.md` marca a Fase 16
  `✅ concluída`, mesmo padrão usado para fechar a Fase 15. `mkdocs build --strict` limpo,
  *anchors* novos (`mcpServers`, `mcp-e-egresso`) conferidos no HTML gerado. Nenhuma mudança
  de código — fmt/clippy rodados como checagem de sanidade.

  **Fase 16 concluída inteira em 2026-07-16** (ADR-0028 `Accepted`, `docs/roadmap-v0.10.md`,
  MT-77..81) — cliente MCP (`McpClient`, `crates/core/src/mcp/`) spawna servidores locais via
  `rmcp`, completa o *handshake* e lista tools; tools MCP entram no `ToolRegistry`
  (`crates/core/src/tools/mcp.rs`) sob o mesmo `PermissionGate` de sempre, nome prefixado pelo
  servidor; classe de egresso checada em dois pontos independentes (*parsing* + conexão),
  nunca inferida; documentação de usuário e governança completa. Um achado técnico (*feature*
  `server` do `rmcp` não disponível em alvos `[[bin]]` com dependências de teste) e um bug de
  *hang* (método MCP sem resposta trava um cliente sem *timeout* próprio) corrigidos durante a
  implementação, ambos registrados/documentados nos commits dos respectivos tickets (MT-78 em
  `docs/decisoes-autonomas.md`; o *hang* do MT-79 foi correção de *bug*, não decisão de
  projeto).

  **Fase 16 fecha a última fase detalhada do roadmap de longo prazo** — próximo passo é
  **preparar a Fase 17+** (`docs/roadmap-longo-prazo.md` §Fase 17+: memória entre sessões,
  subagentes/orquestração, multimodal, checkpoints/*undo*, custo/uso visível — nenhuma tem ADR
  nem detalhamento de tickets ainda).

  **Fase 17 preparada** — decisão de qual das cinco frentes de "segunda onda" preparar
  primeiro registrada em `docs/decisoes-autonomas.md` (2026-07-16): **custo/uso visível**,
  escolhida por ser a única sem nenhuma pergunta de segurança/confidencialidade/egresso em
  aberto (as outras quatro passam a viver na **Fase 18+**, `docs/roadmap-longo-prazo.md`
  §Fase 18+ — nome do bloco renomeado ao nascer esta decisão; note-se acima que o parágrafo
  anterior ainda usa "Fase 17+" para o bloco genérico, texto histórico anterior a esta
  divisão). ADR-0029 (`Proposed`, `docs/adr/0029-uso-de-tokens-visivel-na-sessao.md`) decide:
  `Session` acumula `Usage` por sessão (nenhum tipo novo, só soma o que `Session` já calcula
  por turno); exposto em três pontos — resumo em `stderr` no modo *one-shot*, comando
  `/usage` no REPL, rodapé da TUI; contador **não persiste entre sessões** (fica para a frente
  "memória entre sessões", Fase 18+); custo em dinheiro fica deliberadamente fora de escopo
  (exigiria tabela de preço configurável, não é dado intrínseco ao provider como tokens são).
  `docs/roadmap-v0.11.md` detalha os 4 tickets (MT-82..85). Pronta para começar a
  implementação a partir do MT-82.

  **MT-82 concluído** — `Session` (`crates/core/src/session/mod.rs`) ganha o campo
  `usage_total` (`Usage`), somado a cada turno concluído (`after_response`, chamado por
  `run`/`run_streaming`) e a cada `Session::compact` — distinto do `consumed` local já
  existente, que só vive durante uma chamada a `run`/`run_streaming` (decide estouro de
  `TokenBudget`). Novo método `usage_total()` expõe o total acumulado. `compact()` também
  soma seu próprio uso ao total — decisão tomada durante a implementação: o texto do ticket
  só exigia "não resetar o contador", mas a diretriz de conformidade da ADR-0029 pede que o
  total reflita exatamente a soma dos `Usage` por turno já calculados por `Session`, e
  `compact()` já calcula um `Usage` que antes era silenciosamente descartado. 4 testes novos
  (sessão nova começa zerada; um turno soma; múltiplos turnos acumulam; `compact` soma e
  nunca zera). 372 testes em `agentry-core` (+4), 104 em `agentry`, `cargo build --release`
  limpo. Nenhuma mudança de comportamento observável da CLI ainda — `usage_total()` não é
  consumido por nenhum modo de invocação nesta ticket (MT-83/84).

  **MT-70 concluído** — primeiro ticket de implementação da Fase 15: `ratatui` (feature
  `crossterm`, `default-features = false` para árvore de dependências mínima) adicionada a
  `crates/cli`; flag `--tui` entra em `crates/cli/src/tui/mod.rs` (tela estática + `q`/`Ctrl+C`
  para sair) em vez do REPL de texto, sem tocar o caminho existente. Usa
  `ratatui::try_init`/`restore` (já instalam o *panic hook* que restaura o terminal antes de
  propagar) em vez de montar o backend `crossterm` na mão.

  **MT-71 concluído** — `crates/cli/src/tui/keybind.rs` (novo): tabela única `DEFINITIONS`
  (ação→tecla *default*+descrição, espírito de `packages/tui/src/config/keybind.ts` do
  OpenCode); `resolve()` traduz `KeyEvent` para `Option<Action>`, `legenda()` monta o rodapé de
  ajuda direto da tabela (torna a descrição de cada *binding* dado usado de verdade, não morto).
  O laço de eventos do MT-70 passa a consultar `keybind::resolve` (nunca inspeciona `KeyCode`
  direto) e a rolar um histórico de mensagens **mock** (`Estado::aplicar`, função pura, satura
  nos limites) via `↑`/`k`/`↓`/`j` — prova a navegação antes do *streaming* real (MT-72).

  **MT-72 concluído** — TUI ligada à `Session`/`Router` reais (mesma construção de `main()`,
  reaproveitada). `Session::run_streaming` roda numa *task* separada (`tokio::spawn`); o
  *callback* já genérico (MT-10) envia cada `StreamEvent` por canal ao laço principal, que faz
  `tokio::select!` entre eventos de terminal (lidos numa *thread* dedicada, já que
  `crossterm::event::read` bloqueia) e eventos de *stream* — **zero mudança em `crates/core`**.
  `crates/cli/src/tui/chat.rs` (novo) traduz `StreamEvent` em histórico de mensagens, puro e
  testável. Caixa de entrada de texto real substitui o histórico mock do MT-71.

  **Dois achados do smoke-test manual, ambos corrigidos e registrados em
  `docs/decisoes-autonomas.md`:** (1) os atalhos de letra do MT-71 (`q`/`k`/`j`) colidiam com a
  digitação real — revisados para só `Ctrl+C` (sair) e setas (rolar), letras livres para texto;
  (2) `StderrAuditSink` (`eprintln!` a cada chamada de rede) corrompia a tela alternativa do
  `crossterm` — `NoopAuditSink` (novo) descarta auditoria só sob `--tui`, preservando stderr
  normal no REPL/one-shot; um *widget* de log fica candidato a ticket futuro (YAGNI).

  **MT-73 concluído** — seletor de modelo/*provider* (`Ctrl+P`) com busca difusa (casamento de
  subsequência simples, `crates/cli/src/tui/model_picker.rs`, novo — sem dependência nova, mesma
  disciplina de MT-06/ADR-0007/MT-60). Novo `Router::route_entry` (`crates/core/src/router/mod.rs`)
  — acessor de leitura direto aos candidatos declarados de uma `task-class`, extensão de escopo
  registrada em `docs/decisoes-autonomas.md`. `aplicar_selecao` (`tui/mod.rs`) reaproveita
  `RuntimeOverride`/`Router::resolve_with_override` (mesmo mecanismo do `/model`/`/provider` do
  REPL) — candidato inexistente nunca é alcançável pela UI, egresso insuficiente continua
  *fail-closed* (ADR-0002). Smoke-test manual com dois modelos Ollama declarados: `Ctrl+P` abre
  o modal, filtro em tempo real, `Enter` confirma, `Esc` cancela, mensagem seguinte prova que a
  rota mudou de verdade (resposta veio do modelo recém-selecionado).

  **MT-74 concluído** — `TuiConfirmer`/`TuiPrompter` (`crates/cli/src/tool_executor.rs`,
  `crates/cli/src/tui/ask_user.rs`, novo) enviam `PedidoHumano` por canal ao laço de eventos
  (que possui o terminal — o `Confirmer`/`Prompter` rodam dentro da *task* de streaming, MT-72)
  e aguardam a resposta por `oneshot`. *Toggle* `auto`/`normal` (`Ctrl+A`) só acelera a
  aprovação de tools sob `ask` — invariante de segurança com teste dedicado nomeado
  (`modo_auto_do_tui_confirmer_nunca_aprova_uma_tool_sob_deny`), estrutural:
  `RegistryToolExecutor::execute` nem chama `Confirmer::confirm` para `Denied`. `TuiPrompter`
  não tem *toggle* (a tool `ask_user` existe para perguntar, pular contrariaria seu propósito).
  15 testes novos.

  Smoke-test manual: indicador `[auto]` no título da caixa de mensagem alterna corretamente
  com `Ctrl+A`, terminal não corrompe. **Confirmação de tool via LLM real não pôde ser
  demonstrada de ponta a ponta** — mesmo achado já documentado em MT-61/64/65/66/67/68: os
  modelos locais disponíveis neste ambiente (`llama3.1:8b`, `qwen2.5:7b`) narram em prosa em
  vez de emitir uma *tool-call* real, mesmo para tools já testadas e funcionais (não é um
  defeito do código). A fiação `TuiConfirmer`→canal→`oneshot` é coberta por testes
  automatizados que simulam exatamente esse *handshake* (o mesmo papel que o laço de eventos
  real desempenharia do lado receptor).

  **MT-75 concluído** — `crates/cli/src/tui/diff.rs` (novo): diff clássico por subsequência
  comum máxima (LCS, implementação própria — sem dependência nova, mesma disciplina de
  MT-06/ADR-0007/MT-60/MT-73). `tool_executor.rs::montar_diff_se_aplicavel` detecta
  `fs_write`/`fs_edit` pelo nome da tool e monta o diff lendo o conteúdo atual do arquivo
  (`fs::read_to_string`) — nenhuma mudança em `FsWriteTool`/`FsEditTool`; `TuiConfirmer` ganha
  `workspace_root` só para resolver o caminho relativo. `PedidoHumano`/`SolicitacaoAtiva::Confirmacao`
  carregam o diff pronto; o modal renderiza linhas `-`/`+` (vermelho/verde) quando presente,
  caindo nos argumentos brutos para qualquer outra tool. 25 testes novos, incluindo 5 com
  arquivos reais em disco (não só dublês).

  Smoke-test manual: TUI renderiza/responde normalmente. Confirmação de `fs_write` via LLM real
  não pôde ser demonstrada de ponta a ponta — mesmo achado documentado em
  MT-61/64/65/66/67/68/74.

  **MT-76 concluído — fecha a Fase 15 inteira (MT-70..76).** `docs/usuario/uso.md` ganha a
  seção "Modo TUI": `--tui` opt-in, tabela de *keybindings* *default* (`Enter`/setas/`Ctrl+P`/
  `Ctrl+A`/`Esc`/`Ctrl+C`), menção ao modal de diff e ao modal de `ask_user`, nota de que a
  trilha de governança não muda (nenhum caminho de rede/egresso novo). `--tui` adicionada à
  tabela de flags. **ADR-0027 promovida de `Proposed` para `Accepted`** (`docs/adr/README.md`
  atualizado). `mkdocs build --strict` limpo, *anchors* conferidos no HTML gerado. Nenhuma
  mudança de código — fmt/clippy/test rodados como checagem de sanidade.
  `docs/roadmap-longo-prazo.md` marca a Fase 15 `✅ concluída`.

## Metas cumpridas / Em andamento / Próximo passo

**Cumpridas — planejamento:**
- [x] Ecossistema de 2 repositórios: `ai-coding-agent-profiles` (política) ⇄ `agentry` (execução).
- [x] **ADR-0001..0006** + **contrato de interop v1** + `architecture.md` + `roadmap-v0.1.md` (MT-01..MT-16).
- [x] Fontes de modelos da v0.1: **Ollama, vLLM, Anthropic, LiteLLM** (ADR-0006; Copilot/Enterprise adiado).

**Cumpridas — implementação:**
- [x] **MT-01** — scaffold do workspace Cargo (`crates/cli` = bin `agentry`, `crates/core` = lib `agentry_core`), CI, lints (`ba74200`).
- [x] **ADR-0005 fechado** — CI em matriz `ubuntu/windows/macos` (fmt/clippy em um SO), `.gitattributes` com LF (`2feed85`).
- [x] **ADR-0006** — LiteLLM como fonte de modelos via adapter OpenAI-compatible (MT-15); endpoints de proxy exigem classe de egresso declarada, ausência ⇒ tratado como nuvem (`ab69934`).
- [x] **MT-02** — tipos de domínio em `crates/core/src/model/`: `Message`, `Role`, `ContentBlock`, `ToolCall`, `ToolResult`, `Usage`, `StreamEvent`; round-trip serde testado; validação local verde (fmt+clippy+test) (`f03c1ef`).
- [x] **MT-03** — `trait LlmProvider` (chat, chat_stream, tool-calling, embeddings) + `MockProvider` roteirizado em `crates/core/src/provider/`. Trait dyn-compatible via `BoxFuture` (sem `async-trait`); streaming por canal `tokio::sync::mpsc` (tokio só com feature `sync`); 6 testes novos, 14 no total, validação verde (`26b370e`).
- [x] **MT-04** — `crates/core/src/config/`: `Settings` (mínimo do `settings-schema:1`, ADR-0003) com merge perfil→projeto→env (permissões são união; `deny` nunca encolhe) e `privacy.rs` com perfil→classe de egresso (`privacy-taxonomy:1`). Fail-closed: perfil ausente/desconhecido ⇒ `local-only`; schema divergente ⇒ erro. 32 testes no total, validação verde (`b63fe6b`).
- [x] **MT-05** — `crates/core/src/egress/allowlist.rs`: decisão em memória (sem I/O) se um host é alcançável sob a classe de egresso ativa. Host fora da allowlist ou classe insuficiente ⇒ erro; entradas conflitantes para o mesmo host resolvem para a mais restritiva (fail-closed); suporta host exato e wildcard `*.sufixo` (sem casar domínio nu). `EgressClass` ganhou `rank()`/`permits()` em `config/privacy.rs`. 40 testes no total, validação verde (`a2120b7`).
- [x] **MT-06** — `crates/core/src/egress/redact.rs` (redação sem regex, via tokenizador próprio que isola segredos colados em `chave=`/`?token=` etc.) e `audit.rs` (`AuditEntry` estruturada com destino/perfil/classe/tarefa/outcome, redigindo automaticamente todo campo textual). 54 testes no total, validação verde (`9a89679`).
- [x] **MT-07** — `crates/core/src/transport/mod.rs`: único ponto do crate autorizado a fazer rede (via `reqwest`, com `rustls-tls` em vez de `native-tls`). Integra allowlist (MT-05) + audit log (MT-06): chamada bloqueada aborta **antes** de abrir conexão TCP; toda tentativa emite `AuditEntry`. Teste com servidor HTTP mock feito só com `tokio::net` (sem lib de mock nova) + teste-guarda que varre o código-fonte do crate confirmando que `reqwest::` só aparece em `transport/mod.rs`. 58 testes no total, `cargo build --release` verde (`1723c31`). **Fecha a Fase 2 (egresso).**
- [x] **MT-08** — `crates/core/src/provider/ollama.rs`: primeiro provider real (local), implementando `LlmProvider::chat`/`chat_stream` exclusivamente via `Transport` (nunca importa `reqwest`), herdando allowlist+audit automaticamente. `Transport` ganhou `post_json_lines` (streaming genérico por linhas, agnóstico de formato de provider) e `tokio` ganhou a feature `rt` em `[dependencies]` (não só dev). Durante o desenvolvimento, o teste-guarda do MT-07 pegou uma falha de design própria: `Transport::new` recebia `reqwest::Client` por parâmetro, obrigando quem construísse um `Transport` a importar `reqwest` também — corrigido fazendo `Transport::new` construir o client internamente, sem expor o tipo na API pública. 63 testes no total, `cargo build --release` verde (`4d961eb`).
- [x] **ADR-0007** (Proposed) — Guardrail Gate de conteúdo (entrada/saída de LLM), distinto do gate de tools (MT-11) e da allowlist de egresso (MT-05); regras via extensão do `settings-schema`, camada mais específica só reforça, nunca afrouxa.
- [x] **ADR-0008** (Proposed) — parâmetros de chamada de LLM (`temperature`/`top_p`) e presets de modelo por `task-class`, resolvidos pelo Router (MT-09); rejeita o Modelfile do Ollama como mecanismo de configuração (acopla a um provider). Ambos mudam a fronteira do `settings-schema` (posse do `profiles`) — pedido registrado em `docs/interop/exchange-log.md`; roadmap (MT-09/MT-11) aponta para os ADRs (`3ae5054`).
- [x] **MT-09** — `crates/core/src/router/mod.rs`: mapeia `task-class → (provider, modelo, classe de egresso)` com fallback por disponibilidade e resolve os presets de chamada do ADR-0008. `resolve()` descarta candidato que exige mais do que a classe ativa **antes** de checar disponibilidade — tarefa sensível nunca alcança provider de nuvem mesmo que ele esteja registrado; provider indisponível cai no próximo candidato. Esta é a peça que cobre a ideia de "orquestrador multi-modelo" discutida com o usuário (ver [[no-separate-orchestrator-project]]). 6 testes novos, 69 no total, `cargo build --release` verde (`e23390b`). **Fecha a Fase 3.**
- [x] **MT-10** — `crates/core/src/session/mod.rs`: `Session` com `run()` (chat agregado) e `run_streaming()` (chat_stream + `StreamAggregator` reconstruindo a mensagem final a partir dos eventos), ambos partilhando `after_response()` (soma uso, decide orçamento, executa tool-calls). Execução real de tools ainda não existe — o loop consome só o contrato `ToolExecutor` (dyn-compatible via `BoxFuture`, mesmo padrão do `LlmProvider`); implementações reais (fs/shell) chegam no MT-11+. Orçamento checado logo após cada resposta, **antes** de executar qualquer tool-call pendente. 5 testes novos, 74 no total, `cargo build --release` verde (`cdd4fc6`). **Abre a Fase 4.**
- [x] **ADR-0009** (Proposed) — timeout adaptativo + `keep_alive` configurável para troca de modelo em provider local: Router sinaliza `is_model_switch` em `ResolvedRoute` (rastreando o último modelo por provider); Transporte ganha timeout por chamada; `OllamaProvider` usa o sinal para timeout frio/quente e envia `keep_alive`. Motivado por uma lacuna real auditada: `Transport::new` hoje constrói `reqwest::Client::new()` sem nenhum timeout configurado. Muda a fronteira do `settings-schema` — registrado em `docs/interop/exchange-log.md`; micro-ticket **MT-17** adicionado à Fase 3 do roadmap (`ef69785`).
- [x] **MT-11** — `crates/core/src/tools/{mod.rs,permission.rs}`: `trait Tool` dyn-compatible via `BoxFuture` (mesmo padrão de `LlmProvider`/`ToolExecutor`) + `ToolRegistry` + `PermissionGate` reaproveitando `config::Permissions` (deny/ask do MT-04) em vez de inventar novo formato de política. `deny` (explícito ou tool não registrada) bloqueia sem executar; `ask` **sinaliza** devolvendo a `ToolCall` pendente (`ExecutionOutcome::NeedsConfirmation`) — nunca bloqueia esperando confirmação humana, isso fica para a CLI (MT-14); `allow` executa. Precedência fail-closed: `deny` checado antes de `ask` no mesmo nome. 10 testes novos, 84 no total, `cargo build --release` verde (`cf21f6f`).
- [x] **ADR-0010..0013** (Proposed) — pacote de 4 ADRs para "especialização de modelos open-source sem fine-tuning" (alvo: Qwen 8B-30B local via Ollama). **ADR-0010:** repo-map estilo Aider via `tree-sitter` (grafo de referências + ranking), sem vector DB — construído primeiro por ser mais barato. **ADR-0011:** RAG semântico local — chunking AST-aware (reaproveita ADR-0010) + índice lexical `tantivy` + índice semântico `lancedb` (via `LlmProvider::embeddings` já existente) + busca híbrida + reranker + indexação incremental; `tantivy`/`lancedb` escolhidos por serem nativos em Rust (sem ponte Python/FFI). **ADR-0012:** saída estruturada (constrained decoding) para tool-calling via o campo `format` já existente na API do Ollama — sem dependência nova. **ADR-0013:** tool de grounding via LSP (`lsp-types`+`lsp-server`), só leitura, falando com language server já instalado pelo usuário. Maturidade das 4 dependências novas verificada via `gh repo view`+crates.io antes de fechar os ADRs (todas MIT/Apache-2.0, ativas; `lsp-types` sem push há >1 ano, mitigado por ser dependência direta do `rust-analyzer` ativo — registrado para reverificação). Todas ativadas por padrão, desabilitáveis via `settings-schema` — mudança de fronteira registrada no `exchange-log.md`. Nova **Fase 6** + micro-tickets **MT-18..MT-30** adicionados ao roadmap via skill `micro-ticket-planner` (`70c0470`).
- [x] **ADR-0014** (Proposed) — override runtime de parâmetros de chamada: `CallPreset` (ADR-0008/MT-09) ganha campo `reasoning`; novo tipo `RuntimeOverride` (model/provider/temperature/top_p/system_prompt/max_tokens/reasoning) com precedência chamada-única (flag de CLI) > sessão (comando REPL, estilo `/model` do Claude Code) > preset de `task-class` > `settings-schema` > default do provider. **Fronteira de segurança:** `RuntimeOverride` nunca contém classe de egresso/permissões (continuam fixas pela resolução de `Config` na inicialização); override de model/provider continua sujeito à checagem de allowlist/classe do Router — nunca contorna o fail-closed do ADR-0002; override só vem de comando explícito, nunca inferido de conteúdo de mensagem/tool-output. **Lacuna descoberta e registrada:** `CallPreset` já existe no código desde o MT-09 mas `Session` nunca o consumia — o MT-31 fecha isso independentemente do reasoning. Micro-tickets **MT-31/32/33** adicionados à Fase 4, antes do MT-14 (`4775f33`).
- [x] **MT-31** — fecha a lacuna do ADR-0008/MT-09: `Session::new` passa a receber uma `ResolvedRoute` (em vez de provider/modelo soltos) e `build_request()` aplica o `CallPreset` resolvido — `temperature`/`top_p`/`max_tokens` no `ChatRequest` (`ChatRequest` ganhou os dois primeiros campos); `system_prompt` anteposto ao histórico via `ensure_system_prompt()`, sem duplicar entre chamadas a `run()`/`run_streaming()`. Escopo ampliado além do ticket original: também propaguei `temperature`/`top_p` até o `OllamaProvider` (`OllamaOptions`), já que deixar isso sem fio até o provider real tornaria o preset inútil na prática. 4 testes novos (2 em `session`, 2 em `ollama`), 88 no total, `cargo build --release` verde (`a31382a`).
- [x] **ADR-0015** (Proposed) — Reviewer: auditoria semântica por tipo (`correctness`/`security`/`guardrail-compliance`/`task-completion`), cada uma uma `task-class` própria roteada pelo Router (MT-09) como qualquer outra — sem infraestrutura nova, reaproveita Router+`ChatRequest`+saída estruturada (ADR-0012) inteiramente. Fecha a lacuna que o próprio ADR-0007 tinha deixado em aberto ("moderação semântica... v0.2, se necessária"). Disparo pós-`Done`; modos `advisory`/`blocking` (retry limitado por teto, falha persistente sempre exposta). **Default desligado** (diferente do pacote ADR-0010..0013): é uma segunda chamada completa de modelo por tarefa. Micro-tickets **MT-34/35** adicionados à Fase 4 (`5b5ee37`).
- [x] **MT-12** — `crates/core/src/tools/fs.rs`: `FsReadTool`, `FsWriteTool`, `FsEditTool` (substituição de ocorrência única) e `FsSearchTool` (substring literal, sem regex), todas implementando `Tool` (MT-11) sob o `ToolRegistry` existente, sem lógica de permissão própria. Caminho absoluto ou com `..` rejeitado antes de qualquer I/O; `.claudeignore` respeitado via a crate `ignore` (motor do `ripgrep` — maturidade verificada: 143M downloads, MIT, ativo) em vez de reimplementar semântica de glob na mão. 12 testes novos (diretório temporário com limpeza via `Drop`, sem dependência de teste nova), incluindo um teste de integração confirmando que `deny` impede a escrita de fato, não só sinaliza. 100 testes no total, `cargo build --release` verde (`814ba2f`).
- [x] **MT-13** — `crates/core/src/tools/shell.rs`: `ShellTool` com `ShellPolicy` própria — **inverte** a semântica do gate genérico do MT-11 (lá, nome fora das listas é `Allow`; aqui, comando fora de `allow` é sempre `Deny`), uma segunda camada de política interna à tool, além do `ToolRegistry`. `CommandRunner` é o gancho de sandbox pedido pelo ticket: execução real atrás de um trait dyn-compatible via `BoxFuture`, para um executor com sandbox real (namespaces/seccomp/contêiner) substituir o `SystemCommandRunner` (via `tokio::process`, `sh -c`/`cmd /C` por SO, ADR-0005) no futuro sem tocar a política. 9 testes novos — incluindo prova de que comando bloqueado nunca chega a chamar o executor, que `deny` no gate genérico do MT-11 barra antes da `ShellPolicy`, e um teste real via `SystemCommandRunner`. 105 testes no total, `cargo build --release` verde (`39211bc`).
- [x] **MT-32** — `CallPreset`/`ChatRequest` ganham `reasoning: Option<bool>`; `Session::build_request()` propaga; `OllamaProvider` traduz para o campo `think` (nível superior da API do Ollama, fora de `options`). Ausência nunca envia o campo, preservando o comportamento *default* do Ollama. 3 testes novos, 107 testes no total, `cargo build --release` verde (`0decd45`).
- [x] **MT-33** — `RuntimeOverride` (provider/model/temperature/top_p/system_prompt/max_tokens/reasoning) + `Router::resolve_with_override`, com `resolve()` agora um atalho para override vazio (testes existentes inalterados). Precedência via `merged_over` (mesma convenção de `Settings::merged_over`, MT-04). **Decisão de segurança central**: override de `model`/`provider` só escolhe entre candidatos **já declarados** na `RouteEntry` (nunca um alvo novo, não vetado) e continua sujeito à mesma checagem de classe de egresso — bloqueado mesmo quando pedido explicitamente, provando que o override nunca contorna o *fail-closed* do ADR-0002. 6 testes novos, 113 testes no total, `cargo build --release` verde (`3244dbc`). **ADR-0014 (MT-31/32/33) totalmente implementado.**
- [x] **MT-14** — `crates/cli/src/{main.rs,repl.rs}`: liga tudo em uma CLI real. `agentry "<tarefa>"` roda um turno via streaming (loop de tool-calls do MT-10) contra Ollama local e sai; sem tarefa, entra no REPL, com comandos `/model`/`/temperature`/`/top_p`/`/max_tokens`/`/system`/`/reasoning` como override de sessão (ADR-0014), persistindo até trocados de novo; flags equivalentes na invocação one-shot valem só para aquela chamada. `/model` declara o novo candidato na task-class `chat` antes de resolver — nunca contorna a checagem de classe de egresso do Router. Escopo ampliado com dois módulos de suporte (`streaming.rs`, `tool_executor.rs`, ambos em `crates/cli/src`) e duas extensões pontuais no core: `ToolRegistry::execute_confirmed` (roda uma tool após confirmação humana sem reconsultar o gate) e `Session::apply_route` (troca provider/modelo/preset preservando histórico). 8 testes novos na CLI, 116 no core, fmt/clippy limpos, `cargo build --release` verde, smoke-test manual do binário (`--help`, one-shot contra host sem Ollama falha limpo sem panic, REPL sai limpo em EOF) (`c226f3f`). **Fecha a Fase 4.**
- [x] **MT-15** — `crates/core/src/provider/openai_compat.rs`: `OpenAiCompatProvider` (vLLM/OpenRouter/gateways LiteLLM) sobre o Transporte único, cobrindo chat, streaming SSE (`data: {...}`, com acumulação incremental de `tool_calls` por índice) e tool-calling; diferente do Ollama, a API OpenAI exige `tool_call_id` por mensagem de resultado, então um `Message` de domínio com múltiplos `ToolResult` expande em várias `OpenAiMessage`. Dois testes cobrem literalmente os dois lados do critério de aceite do ADR-0006: endpoint com classe de egresso declarada na allowlist funciona; sem declaração é bloqueado (fail-closed), mesmo em host local. **Escopo estendido além do ticket, com aprovação explícita do usuário:** `Transport` (`crates/core/src/transport/mod.rs`) ganhou `with_api_key` (builder, não quebra chamadores existentes) — anexa `Authorization: Bearer` a toda requisição, gap real descoberto ao projetar o adapter (OpenRouter/LiteLLM em nuvem normalmente exigem chave de API, e nenhum outro módulo pode tocar `reqwest` para isso). 10 testes novos (9 no adapter, 1 no transporte), 126 testes no core + 8 na CLI, fmt/clippy limpos, `cargo build --release` verde (`0951111`).
- [x] **MT-16** — `crates/core/src/provider/anthropic.rs`: `AnthropicProvider` (Messages API) sobre o Transporte único, cobrindo chat, streaming SSE (eventos nomeados `message_start`/`content_block_start`/`content_block_delta`/`content_block_stop`/`message_delta`/`message_stop`) e tool use. A Messages API não tem papel `system` nem `tool` — prompt de sistema é extraído do histórico para o campo `system` de nível superior, e resultado de tool é um bloco `tool_result` **dentro** de uma mensagem `user` (ao contrário do OpenAI, múltiplos `ToolResult` cabem numa única mensagem, sem expandir). `max_tokens` é obrigatório na API — default de 4096 quando ausente no `ChatRequest`. `reasoning` (MT-32/ADR-0014) traduz para o campo nativo `thinking`; blocos de raciocínio na resposta são reconhecidos e descartados (sem variante de `StreamEvent` para carregá-los). **Ajuste não-quebrador na extensão do MT-15:** `Transport::with_api_key` (fixava `Authorization: Bearer`) generalizado para `Transport::with_header` (nome+valor arbitrário), já que a Messages API usa `x-api-key`+`anthropic-version`, esquema diferente — nenhum chamador real dependia do nome antigo além do próprio teste, que foi adaptado. 11 testes novos, 137 testes no core + 8 na CLI, fmt/clippy limpos, `cargo build --release` verde (`f62851d`). **Fecha a Fase 5 (demais providers).**
- [x] **CI: scan de segredos (gitleaks)** — job independente no pipeline (`.github/workflows/ci.yml`), complementar ao skill `secrets-guard` (comportamento do assistente) e à redação automática do audit log (ADR-0002/MT-06): varre todo push/PR por segredos commitados, sem depender do agente ter seguido a política. `gitleaks` (MIT, ativo) via `gitleaks-action`; `GITLEAKS_LICENSE` só é exigido para contas de organização, não pessoais — sem custo/dependência nova. Inspirado pela análise do repositório de referência `anomalyco/opencode` (`16bbe0b`).
- [x] **ADR-0016** (Proposed) — compactação de histórico de sessão (`Session::compact`): lacuna real auditada — `TokenBudget` só limita o sub-loop de tool-calls **dentro** de um turno (`consumed` é reiniciado a cada `run()`/`run_streaming()`), sem nenhuma relação com o tamanho acumulado de `self.messages` entre turnos; não havia nenhuma estratégia de recuperação para conversas longas. Decisão: `task-class` dedicada (`"compact"`), resolvida pelo Router como qualquer outra (mesmo padrão do Reviewer, ADR-0015); chamada de chat simples (sem tools/streaming) pedindo um resumo; substituição **total** do histórico por uma única mensagem de sistema (nunca parcial); disparo sempre explícito (nunca automático na v0.1); falha do provider preserva o histórico original intacto. Inspirado pela análise do OpenCode (a pedido do usuário, para ideias além de TUI) — conceito deles de separar "System Context" estável de "Session History" compactável informa a decisão, mas o formalismo completo de "Context Epoch"/cache de prompt do provider é deliberadamente deixado fora de escopo. Micro-tickets **MT-36/37** adicionados à Fase 4 (`80f7a81`).
- [x] **MT-36** — `Session::compact` (`crates/core/src/session/mod.rs`): resolve a `task-class` `"compact"` via Router, renderiza o histórico como transcript e pede um resumo via `LlmProvider::chat` (sem tools/streaming), substituindo `self.messages` inteiro por `vec![Message::system(resumo)]`. `SessionError` ganha a variante `Router` (erro de resolução de rota). Tudo-ou-nada: falha de router/provider nunca toca `self.messages`; histórico vazio é no-op. 4 testes novos, 141 testes no core + 8 na CLI, fmt/clippy limpos, `cargo build --release` verde (`7e217c4`).
- [x] **MT-37** — comando `/compact` no REPL (`crates/cli/src/repl.rs`): chama `Session::compact` (MT-36) e ecoa confirmação/erro; tratado como caso especial antes do dispatch genérico de `aplicar_comando` (precisa de `session`+`router` assíncronos, não só mutar `RuntimeOverride`). 3 testes novos, 11 testes na CLI + 141 no core, fmt/clippy limpos, `cargo build --release` verde, smoke-test manual do binário (`/compact` com histórico vazio não falha) (`f932e41`). **ADR-0016 (MT-36/37) totalmente implementado.**
- [x] **MT-17** — timeout adaptativo + `keep_alive` (ADR-0009): `Router` rastreia (via `Mutex`, já que `resolve`/`resolve_with_override` continuam `&self`) o último modelo resolvido por provider e sinaliza troca em `ResolvedRoute::is_model_switch` (rastreio otimista, não afeta a decisão de roteamento). `Transport::post_json`/`post_json_lines` aceitam timeout por chamada (`.timeout()` nativo do `reqwest`; `None` cai no *default* do `Client`). `OllamaProvider` usa `is_model_switch` (propagado de `ResolvedRoute` via `Session`/`ChatRequest`) para escolher entre timeout frio (`300s`, troca de modelo) e quente (`30s`, mesmo modelo), e envia `keep_alive` (`"30m"`) em toda chamada, sem exceção. **Escopo maior que o declarado** (`router/mod.rs`, `transport/mod.rs`, `provider/ollama.rs`): mudar a assinatura de `post_json`/`post_json_lines` obrigou atualizar os call-sites de `openai_compat.rs`/`anthropic.rs` (passam `None` — sem tratamento especial, como o próprio ADR-0009 já previa) e `ChatRequest` (`provider/mod.rs`) precisou do campo `is_model_switch` para o sinal atravessar de `Session` até o adapter sem duplicar a detecção de troca fora do Router (proibido pelo ADR). Exposição via `settings-schema` deliberadamente adiada (mesmo padrão dos defaults do MT-16) — sem entrada no `exchange-log` nesta v0.1. 10 testes novos, 151 testes no core + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`c07cf81`).
- [x] **MT-18** — `crates/core/src/context/ast.rs`: `extract_symbols` reaproveita a *tags query* (`TAGS_QUERY`) que cada gramática `tree-sitter` já publica — mesma convenção do repo-map do Aider e da busca de símbolos do GitHub — em vez de reimplementar a detecção de símbolo nó a nó por linguagem; cobre as captures `definition.function`/`.method`/`.class`, deixando `definition.module`/`reference.call` etc. (já presentes na mesma query) para quando o grafo de referências (MT-19) precisar delas. **Descoberta durante a implementação:** a *tags query* do Rust casa o mesmo `fn` dentro de `impl` duas vezes (`definition.method` específico + `definition.function` genérico) — `merge_symbol` deduplica por `range`, preferindo a classificação mais específica; a do Python não distingue método de função solta (assimetria real entre gramáticas, documentada no código). Dependências novas, cada uma vetada individualmente (ADR-0004): `tree-sitter` (MIT, 27M+ downloads), `tree-sitter-rust` (MIT, 13.6M), `tree-sitter-python` (MIT, 10.7M), `streaming-iterator` (Apache-2.0, 31M+ — já transitiva do `tree-sitter`). 4 testes novos, 155 testes no core + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`06ea5d8`).
- [x] **MT-19** — `crates/core/src/context/repo_map/graph.rs`: `build_reference_graph` roda a mesma *tags query* do MT-18, mas com parse+query próprio (não reaproveita `ast::extract_symbols`) extraindo tanto `definition.*` (sem o filtro função/classe/método — uma referência pode apontar para constante/trait/macro definida em outro arquivo) quanto `reference.*` (chamada de função/método, implementação de trait). Aresta dirigida `A -> B` por referência em `A` que casa com um nome definido em `B`, peso = contagem; **sem auto-referência** (não ajuda a decidir relevância entre arquivos, propósito do grafo que o MT-20 vai rankear). 5 testes novos (peso correto entre dois arquivos; sem auto-referência; nome desconhecido não gera aresta; mesmo mecanismo funciona para Python; arquivos sem relação não geram grafo), 160 testes no core + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`5b7a48e`).
- [x] **MT-20** — `crates/core/src/context/repo_map/rank.rs`: `rank()` implementa PageRank **personalizado** (mesma técnica do Aider) sobre o grafo do MT-19 — massa de teleporte concentrada nos arquivos "semente" (em vez de uniforme), propagada pelas arestas ponderadas pela contagem de referências e normalizadas pelo peso de saída; nós sem aresta de saída redistribuem massa conforme a personalização em vez de desaparecer; `seeds` vazio cai no PageRank clássico; os próprios nós de `seeds` são excluídos do ranking devolvido. **Dois bugs pegos durante a escrita dos testes** (nunca chegaram a ser commitados): indexação direta num `HashMap` de pesos de saída panicava quando uma aresta apontava para um nó fora do subconjunto de `nodes` passado (trocado por `.get()` com skip silencioso); e o cenário de teste original dependia de propagação de segunda ordem através de um nó sem personalização própria, que é zero por construção no PageRank personalizado — não bug do algoritmo, premissa errada do teste, corrigido para testar peso de aresta direto da semente. 4 testes novos, 164 testes no core + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`6ad4f6d`).
- [x] **MT-21** — `crates/core/src/tools/repo_map.rs`: `RepoMapTool` expõe o repo-map (MT-19/20) como `Tool` (MT-11) — lê arquivos-fonte sob uma raiz fixa respeitando `.claudeignore` (mesma técnica do MT-12, via `ignore::WalkBuilder`), filtra por extensão suportada (`.rs`/`.py`, mesmas linguagens do MT-18), constrói o grafo e devolve os arquivos mais relevantes a partir de `seeds` dados pelo modelo; roda sob o mesmo `ToolRegistry`/gate de permissão de qualquer outra tool. `register_repo_map_tool` decide, a partir de uma flag booleana, se a tool é registrada — mecanismo testável de `context.repo_map.enabled` (ADR-0010, *default* `true`) sem a fiação real com o `settings-schema` (fora de escopo — UI/CLI de configuração). 6 testes novos (ranking a partir da semente com peso correto; respeita `.claudeignore`; sem arquivos suportados não é erro; respeita o gate de permissão; flag ligada/desligada), 170 testes no core + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`2d11628`). **Fecha a trilha repo-map (MT-18..21, ADR-0010).**
- [x] **MT-22** — `crates/core/src/provider/ollama.rs`: `OllamaProvider` ganha `structured_output: bool` (*default* `true`, `with_structured_output` builder) — quando ativo e `ChatRequest.tools` não vazio, o campo `format` da API do Ollama recebe um JSON Schema combinado das `tools` (`oneOf` de `{name: <const>, arguments: <input_schema>}`), restringindo a geração da porção de tool-call ao formato esperado (ADR-0012) — reduz JSON malformado em modelos pequenos, sem fine-tuning e sem dependência nova. Fiação real da flag com o `settings-schema` (`providers.ollama.structured_output`) deliberadamente adiada, mesmo padrão do MT-16/MT-17 — a flag é hoje uma propriedade construída direto no provider. 4 testes novos (format presente com tools+flag ativa; ausente sem tools; ausente com a flag desativada mesmo havendo tools; round-trip via Transporte real nos dois sentidos), 174 testes no core + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`889d4e8`).
- [x] **MT-23** — `crates/core/src/context/lsp/client.rs`: `LspClient` inicia um *language server* já instalado no ambiente (`agentry` não empacota nenhum, ADR-0013) como subprocesso e fala JSON-RPC sobre `stdin`/`stdout` via `lsp_server::Message::read`/`write` (genéricos sobre `BufRead`/`Write` — reaproveitados do lado cliente, apesar do crate ser desenhado para o lado servidor) e os tipos do `lsp-types`. Cobre `start` → `initialize` (*handshake* completo) → `didOpen` → `shutdown` (espera resposta, `exit`, espera o processo terminar de verdade); `Drop` mata+espera como rede de segurança se `shutdown` nunca foi chamado. **Descoberta durante a implementação:** `InitializeParams::root_uri` é campo depreciado do `lsp-types` (em favor de `workspace_folders`) — traduzido internamente, sem vazar para a API pública do cliente. Ciclo de vida testado contra um `fake_lsp_server` (novo binário auxiliar em `crates/core/src/bin/`, não parte do produto) — o teste precisou virar teste de integração (`crates/core/tests/lsp_client.rs`) porque `CARGO_BIN_EXE_fake_lsp_server` só é definida pelo Cargo para alvos de integração do pacote, não para testes unitários dentro de `--lib`. Dependências novas vetadas por maturidade/licença (ADR-0013): `lsp-types` (MIT, 28M+ downloads) e `lsp-server` (Apache-2.0, 12M+ downloads), ambas do ecossistema `rust-analyzer`. 4 testes novos (3 de integração + 1 unitário), 175 testes na lib do core + 3 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`39ffd55`).
- [x] **MT-24** — `crates/core/src/tools/lsp.rs`: `LspHoverTool`/`LspDefinitionTool` expõem hover/*go-to-definition*/referências do `LspClient` (MT-23) como `Tool` (MT-11); `LspSession` inicia o *language server* sob demanda na primeira chamada e reaproveita o mesmo processo entre as duas tools (nunca spawna um por tool). Ausência do *language server* vira `ToolOutput::error`, nunca trava o agent loop. `register_lsp_tools` implementa o mecanismo testável de `context.lsp_grounding.enabled` (*default* `true`), mesmo padrão do MT-21. **Escopo maior que o declarado** (só `tools/lsp.rs`): o cliente do MT-23 só cobria `initialize`/`didOpen`/`shutdown` — `client.rs` ganhou um primitivo genérico (`LspClient::request<P, R>`) para enviar hover/definição/referências sem duplicar a lógica de request/response; `initialize`/`shutdown` foram refatorados para reusá-lo, sem mudar comportamento. `fake_lsp_server` (fixture do MT-23) ganhou uma resposta fixa para `textDocument/hover`. 5 testes novos (round-trip de hover via processo real, em `crates/core/tests/lsp_tools.rs` — integração, mesma razão do MT-23; ausência do LS é erro tratado; gate de permissão; flag ligada/desligada), 179 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`7b3777d`). **Fecha a trilha LSP (MT-23/24, ADR-0013).**
- [x] **MT-25** — `crates/core/src/context/rag/chunk.rs`: `chunk_file` reaproveita `ast::extract_symbols` (MT-18) — não duplica a detecção de função/classe/método — para gerar um `Chunk` (arquivo, símbolo, tipo, *range*, texto) por símbolo extraído; o texto do chunk é sempre `source[range]` exato, nunca truncado/partido no meio (ao contrário de chunking por tamanho fixo de token). **Comportamento documentado, não bug:** chunks podem se sobrepor quando um símbolo está aninhado dentro de outro (ex.: `fn` dentro de `fn`) — ambos viram chunks independentes, multi-granularidade deliberada. 4 testes novos (metadados corretos e texto completo em Rust; idem em Python; símbolo aninhado produz chunk próprio contido no chunk externo; fonte vazia não produz chunks), 183 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`00b9460`).
- [x] **MT-26** — `crates/core/src/context/rag/lexical_index.rs`: `LexicalIndex` indexa os chunks do MT-25 via `tantivy` (`Index::create_in_ram` — embutido, sem servidor externo/ponte FFI, ADR-0011); schema com `file`/`kind` exatos (`STRING`), `symbol`/`text` tokenizados (`TEXT`, BM25) e `range_start`/`range_end`, todos `STORED` para reconstruir o `Chunk` original a partir de um hit. `search()` usa `QueryParser` sobre `symbol`+`text` com boost 2x em `symbol` — consulta por identificador exato rankeia o chunk correspondente acima de ocorrências incidentais do termo no corpo de outros chunks. **Descoberta durante a implementação:** `TopDocs` (tantivy 0.26) não implementa `Collector` diretamente — precisa de `.order_by_score()`; pego pelo primeiro `cargo build` (E0277), confirmado no próprio rustdoc do crate. Dependência nova vetada por maturidade (ADR-0011, já verificada ao fechar o ADR): `tantivy` (MIT, 15M+ downloads, nativo em Rust). 4 testes novos (identificador exato no topo; consulta sem correspondência; limite restringe resultados; chunk reconstruído preserva todos os metadados), 187 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`93f7ccd`).

- [x] **MT-27** — `crates/core/src/context/rag/semantic_index.rs`: `SemanticIndex::build` chama `LlmProvider::embeddings` (MT-03) uma vez com o texto de todos os chunks (MT-25) e indexa os vetores resultantes numa tabela `lancedb` sobre `memory://` (embutido, sem servidor externo, mesma filosofia do índice lexical do MT-26); schema Arrow com colunas escalares (file/symbol/kind/range_start/range_end/text) + `vector` (`FixedSizeList<Float32>`), todas reconstituídas de volta em `Chunk` num hit de busca. `search()` roda k-NN via `nearest_to` **sem** criar um índice ANN — desnecessário/inadequado em escala pequena; o `lancedb` cai em busca exata por varredura sem índice construído. `chunks` vazio não é erro (mesmo padrão do MT-21/25): índice sem tabela por trás, busca sempre responde lista vazia. `kind_to_str`/`kind_from_str` (MT-26) promovidos de `lexical_index.rs` para `rag/mod.rs` (`pub(super)`) — reaproveitados por este módulo também, em vez de duplicar a conversão de `SymbolKind`. **Descoberta relevante:** `lance-encoding` (dependência transitiva do `lancedb`) exige o binário `protoc` no `PATH` em tempo de build — não estava disponível no ambiente nem, previsivelmente, nos runners do GitHub Actions; CI (`.github/workflows/ci.yml`) atualizado para instalar `protobuf-compiler`/`protobuf`/`protoc` via gerenciador de pacote nativo de cada SO da matriz (apt/brew/choco) tanto no job de lint quanto no de build-test, em vez de depender de uma Action de terceiro (`arduino/setup-protoc`, sem push desde 2024). Dependências novas vetadas por maturidade (ADR-0011, já verificada ao fechar o ADR): `lancedb` (Apache-2.0, 639K+ downloads, nativo em Rust sobre Arrow). 5 testes novos (vizinho mais próximo no topo; limite restringe resultados; chunks vazio não é erro; contagem de vetores inconsistente é erro; chunk reconstruído preserva todos os metadados), 192 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde (`a518c9e`).

- [x] **ADR-0017** (Proposed) — diretório de estado local por projeto (`.agentry/`) para memória, histórico e índices: lacuna real auditada — hoje o `agentry` não persiste nada em disco (audit log é stderr-only, `Session` é `Vec<Message>` em memória, os índices RAG do MT-26/27 recomeçam do zero a cada processo); sem decidir onde persistir, o MT-29 (indexação incremental) não teria como fazer sentido entre invocações de processo. Decisão, motivada explicitamente pelo usuário (padrão comum de agentes de codificação, inclusive esta própria sessão do Claude Code, quebra ao renomear/mover/copiar o projeto por chavear o estado no caminho absoluto): raiz `<raiz>/.agentry/` (primeiro ancestral do cwd com `.git`, *fallback* pro cwd) — nunca diretório global do usuário — com auto-exclusão via `.agentry/.gitignore` próprio (conteúdo `*`), nunca tocando no `.gitignore` do projeto; como as tools de leitura já existentes (MT-12/MT-21) respeitam `.gitignore` via a crate `ignore`, `.agentry/` já sai de graça de qualquer varredura de repo-map/RAG. Layout reservado (`.agentry/index/`, `.agentry/session/`, `.agentry/audit.log`) mas **não implementado** por esta ADR — cada subsistema decide quando/como consumir em seu próprio ticket. Micro-ticket **MT-38** adicionado à Fase 6 (resolução de raiz + gitignore próprio); **MT-29** passa a depender também de MT-38 (`49e79f9`).

- [x] **MT-28** — `crates/core/src/context/rag/hybrid_search.rs`: `fuse` combina os índices lexical (MT-26) e semântico (MT-27) via *reciprocal rank fusion* (constante de suavização 60) — um chunk presente nas duas listas acumula as duas contribuições, podendo superar um chunk isoladamente melhor rankeado numa única lista, exatamente o comportamento exigido pelo critério de aceite (resultado combinado reflete os dois sinais). `rerank` reordena via uma chamada de chat pedindo ao modelo um array JSON dos índices em ordem de relevância — reaproveita `LlmProvider::chat` (MT-03) diretamente, nenhuma API nova de reranking (ADR-0011); resposta que não for um array JSON válido/completo é erro (`RerankParse`), nunca mascarado; 0/1 chunk não chama o provider. `hybrid_search` compõe o pipeline completo. **Escopo maior que o declarado:** promovi `Message::text_content()` (`crates/core/src/model/mod.rs`) a partir do `extract_text` privado que já existia em `session/mod.rs` (MT-36) — reranking precisa da mesma extração de texto puro de uma resposta de chat; evitei duplicar a lógica pela segunda vez no pacote. 6 testes novos (fusão reflete os dois sinais; fuse respeita limite; reranking reordena caso conhecido; resposta malformada é erro tratado; 0/1 chunk não chama o provider; pipeline completo funde e reordena), 198 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova (`6968663`).

- [x] **MT-38** — `crates/core/src/state_dir.rs` (novo módulo de topo do core): `resolve_root` sobe a partir do cwd procurando `.git` (arquivo ou diretório — cobre *worktrees*) em cada ancestral, mesma técnica de descoberta do próprio git; sem `.git` em nenhum ancestral, devolve o próprio `start` (nunca a raiz do sistema de arquivos). `ensure_state_dir` cria `<raiz>/.agentry/` e, só se ainda não existir, `.agentry/.gitignore` com conteúdo `*` — idempotente por construção (`create_dir_all` + escrita condicional à ausência do arquivo), nunca sobrescreve uma customização do usuário. Nenhum subsistema (índices RAG, sessão, audit log) foi ligado a este diretório ainda — fora de escopo, conforme a própria ADR-0017 já previa. 5 testes novos (raiz com `.git` diretório; raiz com `.git` arquivo/worktree; sem `.git` cai no start; `.gitignore` criado com `*`; chamada repetida não sobrescreve customização), 203 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova (`33ed4c0`). **ADR-0017 totalmente implementada.**

- [x] **MT-29** — `crates/core/src/context/rag/incremental.rs`: `IncrementalIndexer::reindex` compara um hash de conteúdo (`std::hash::DefaultHasher` — não `git diff`, que exigiria um binário `git` no `PATH` e não cobre repositórios ainda não inicializados, ADR-0017) de cada `ArquivoFonte` contra um manifesto persistido (`<estado>/index/manifest.json`, dentro do diretório que o MT-38 já resolve/cria); conteúdo igual reaproveita os chunks já indexados, conteúdo novo/diferente reprocessa via `chunk_file` (MT-25) só aquele arquivo. Arquivos que somem do conjunto atual são removidos do manifesto. Manifesto ausente ou corrompido **não é erro** (cai para vazio, reprocessando tudo — pior caso é o comportamento pré-MT-29, não uma indexação que falha); falha ao **escrever** o manifesto atualizado é erro (a próxima chamada perderia o benefício incremental silenciosamente, proibido pelo ADR-0011). `ChunkPersistido` é uma representação própria de serialização (`Chunk` não ganha `Serialize`/`Deserialize`) — mesmo padrão já usado por `lexical_index.rs`/`semantic_index.rs`; `kind_to_str`/`kind_from_str` (MT-26) reaproveitados de novo. 5 testes novos (primeira chamada reprocessa tudo; segunda chamada com tudo inalterado não reprocessa nada; alterar um arquivo dispara reindexação só dele — critério de aceite literal do ticket; arquivo removido some do manifesto; manifesto corrompido não é erro), 208 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova (`38c18e1`).

- [x] **MT-30** — `crates/core/src/tools/code_search.rs`: `CodeSearchTool`/`CodeSearchSession` expõem a busca híbrida (MT-28) como `Tool` (MT-11), fechando a trilha inteira do RAG semântico (MT-25..30, ADR-0011) e, com ela, **a Fase 6 inteira**. `CodeSearchSession` mantém os índices lexical (MT-26) e semântico (MT-27) em cache (`tokio::sync::Mutex` — não `std::sync::Mutex`, precisa segurar o *lock* através de um `.await` ao reconstruir o índice semântico) entre chamadas, reconstruídos só quando `IncrementalIndexer::reindex` (MT-29) reporta que algum arquivo mudou; sem mudança nenhuma, a chamada reaproveita os índices prontos e só chama `LlmProvider::embeddings` uma vez (para a consulta em si) — é isso que dá ao MT-29 um efeito prático real dentro de uma sessão, não só um número em um teste isolado. **Limitação conhecida, documentada no módulo:** quando algo muda, o índice semântico reembeda todos os chunks atuais, não só os do arquivo alterado — `SemanticIndex::build` (MT-27) não tem uma API de inserção incremental de vetores; fica para quando houver demanda real. Duplica deliberadamente (não reaproveita) o laço de `WalkBuilder` de `tools/repo_map.rs` (MT-21) — documentado no próprio módulo como decisão, não descuido. `register_code_search_tool` respeita `context.semantic_rag.enabled` (ADR-0011, *default* `true`), mesmo mecanismo do MT-21/24. 7 testes novos (gate de permissão; flag ligada/desligada; busca devolve resultados formatados e reordenados pelo reranking; segunda chamada sem mudanças não reconstrói os índices — prova o cache; query vazia é erro tratado; sem arquivos suportados não é erro), 215 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova (`ef9caf5`).

- [x] **MT-34** — `crates/core/src/session/reviewer.rs` (novo): `review(kind, router, instrucao_original, artefato)` resolve a `task-class` própria de cada tipo de auditoria (`"review-correctness"`/`"review-security"`/`"review-guardrail-compliance"`/`"review-task-completion"`) via `Router` (MT-09) como qualquer outra — nenhuma infraestrutura nova. O veredito estruturado (ADR-0012) é obtido enquadrando-o como uma **tool-call** (`submit_review(verdict, notes)`), não texto solto — único jeito de reaproveitar de verdade o mecanismo de saída estruturada já existente (hoje só ativo em `OllamaProvider` quando `tools` não é vazio) sem tocar `provider/ollama.rs`; diferente do *reranking* do MT-28 (que usou *parsing* de JSON solto por falta de um encaixe natural de tool-call ali), aqui "envie seu veredito" é uma tool-call natural. Resposta sem chamar `submit_review` ou com `verdict` fora de `pass`/`fail` é erro tratado (`VeredictoAusente`/`VeredictoInvalido`), nunca ignorado em silêncio. 6 testes novos, 221 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova (`edffd28`).
- [x] **MT-35** — `crates/core/src/session/mod.rs`: `Session` ganha `reviews: Vec<ReviewConfig>`/`review_retry_limit: u32` (*default* vazio/`0` — "desligado por padrão", ADR-0015) via `with_reviews`; `SessionOutcome` ganha `reviews: Vec<ReviewResult>`; `SessionError` ganha `Reviewer(ReviewerError)`. Novo helper privado `revisar_ou_continuar` (compartilhado entre `run`/`run_streaming`, para não duplicar a mesma decisão duas vezes): após `StopReason::Done`, roda cada auditoria habilitada via `reviewer::review`; devolve `ControlFlow::Break(outcome)` (vereditos anexados) se não houver reprovação bloqueante ou o teto já foi atingido; `ControlFlow::Continue` — incrementando o contador e injetando uma observação corretiva (`Message::user` com as notas) — só quando há `Fail` em modo `Blocking` com retentativa sobrando. `reviews` vazio devolve `Break` imediatamente sem tocar `router` — nenhuma auditoria roda se não habilitada. **Mudança de assinatura deliberada** (mesmo espírito do MT-17): `run`/`run_streaming` passam a receber `router: &Router` (o Reviewer resolve uma `task-class` diferente da principal); ripple mecânico em `crates/cli/src/{streaming.rs,main.rs,repl.rs}` (todos já tinham um `Router` em escopo no ponto de chamada — nenhum habilita `reviews`, então o Reviewer nunca roda de fato via CLI nesta v0.1, consistente com "UI/CLI de configuração" fora de escopo). 4 testes novos (`advisory` com `fail` não bloqueia; `blocking` reprovado dispara retry até o teto e expõe a falha persistente; `blocking` aprovado de primeira não gera retry; `reviews` vazio nunca chama o Reviewer mesmo com Router sem nenhuma rota `review-*`), 225 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova (`254b139`). **ADR-0015 totalmente implementada — fecha o único item que restava aberto em todo o roadmap v0.1.**

- [x] **Build final Linux/Windows** — Linux nativo (`target/release/agentry`) confirmado a cada ticket da sessão. Windows via cross-compile local (`target/x86_64-pc-windows-gnu/release/agentry.exe`, ~259MB) usando `mingw-w64`; achado: o `update-alternatives` do Debian/Ubuntu registra a variante `win32` como *default* para `x86_64-w64-mingw32-gcc`, mas o `std` do Rust exige a variante `posix` — contornado apontando o linker direto para `/usr/bin/x86_64-w64-mingw32-gcc-posix` num `.cargo/config.toml` **local, não versionado**. Documentado em `docs/testing.md`.
- [x] **README real + guia de testes + scripts de automação** — `README.md` era o template genérico do perfil PESSOAL (nunca preenchido para o `agentry` de verdade); reescrito com pré-requisitos/instalação/uso reais. `docs/testing.md` (novo): configuração inicial e comandos de teste por SO, espelhando `.github/workflows/ci.yml`. `scripts/test.sh`/`.ps1` (novos): mesma sequência do CI (fmt/clippy/test/build), local. `scripts/usability-test.sh`/`.ps1` (novos): simulam a primeira configuração e o primeiro uso simples — não lógica interna, a *experiência* de quem acabou de clonar o repo (build do zero, `--help` sem config, Ollama ausente deve dar erro tratado sem *panic*, verificação do modelo *default*, uma tarefa *one-shot* real). Rodado nesta sessão contra um Ollama real (containers do usuário: `llama3.1:8b`/`qwen2.5:7b`/`qwen3.5:2b`) — os 5 cenários passaram, incluindo a tarefa *one-shot* de verdade (`0791411`).
- [x] **Fix de usabilidade: audit log poluindo stderr** — achado real do `scripts/usability-test.sh`: `StderrAuditSink` (`crates/cli/src/main.rs`) imprimia `{entry:?}` (o *dump* de `Debug` de `AuditEntry`, 2-3 linhas com nomes de campo) a cada chamada de egresso, poluindo a saída de quem só queria ver a resposta/erro da tarefa. `EgressClass` (`crates/core/src/config/privacy.rs`) e `AuditEntry` (`crates/core/src/egress/audit.rs`) ganharam `impl Display` (uma linha compacta, ex.: `chat_stream -> http://127.0.0.1:11434/api/chat (local-only, allowed)`); `StderrAuditSink` passou a usar `{entry}` em vez de `{entry:?}`. O *trail* continua obrigatório pelo ADR-0002 (nenhum campo omitido) — só o formato de impressão mudou. 4 testes novos, 228 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde. Confirmado manualmente contra Ollama real (`4bd6ee6`).

- [x] **ADR-0017 emendada + ADR-0018** — revisão do roadmap pós-v0.1 identificou que todas as
  seis extensões de `settings-schema` propostas na sessão ficaram com formato "a confirmar
  com o `profiles`" sem nunca terem sido de fato confirmadas. Investigação direta do
  `ai-coding-agent-profiles` revelou que o artefato hoje existente
  (`.claude/settings.json`) é o formato **nativo do Claude Code** (padrões `Bash(...)` em
  `permissions`), incompatível por design com `agentry::config::Permissions` (nomes exatos
  de tool). Decisão: artefato próprio, `.agentry/agentry.settings.json` — dentro da mesma
  pasta da ADR-0017 (MT-38), com uma **exceção nomeada** na auto-exclusão do
  `.gitignore` (`CONTEUDO_GITIGNORE` em `crates/core/src/state_dir.rs` passa de `"*\n"`
  para `"*\n!agentry.settings.json\n"` — 2 testes novos, um deles documentação executável da
  intenção: só uma exceção, nunca um padrão amplo). Primeira fatia de schema congelada
  (`permissions` + as 4 *flags* booleanas já mecanicamente prontas — repo-map/RAG/LSP/saída
  estruturada). `docs/interop/exchange-log.md` ganhou a sétima entrada; novo
  `docs/roadmap-v0.2.md` (Fase 7: MT-39 carregamento do arquivo, MT-40 consumo real das 4
  flags) — v0.1 permanece fechado/imutável. 6 testes na lib do core (novos+atualizados),
  229 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos,
  `cargo build --release` verde. Nenhuma dependência nova (`be4f000`). Trabalho equivalente
  feito **na mesma sessão** do lado `ai-coding-agent-profiles` (ver handoff daquele repo).

- [x] **Fix: `.agentry/.gitignore` não podia se autoignorar** — achado real ao distribuir o
  mesmo conteúdo pelo `ai-coding-agent-profiles` (ADR-0006 daquele repo): um `.gitignore`
  com só `*` ignora **a si mesmo**, e `git add .agentry/.gitignore` o descartava em
  silêncio. `CONTEUDO_GITIGNORE` (`crates/core/src/state_dir.rs`) ganhou uma segunda
  exceção puramente técnica (`!.gitignore`) — não é um segundo artefato de política, só a
  mecânica para a exceção de `agentry.settings.json` funcionar de fato. 3 testes
  atualizados/novos, um deles usando a própria crate `ignore` (`GitignoreBuilder`) para
  provar diretamente que o arquivo não se autoignora. 230 testes na lib do core (229 + 1) +
  4 de integração + 11 na CLI, fmt/clippy limpos, `cargo build --release` verde
  (`fb99c02`).

- [x] **MT-39** — `crates/core/src/config/mod.rs`: `Settings` ganha os blocos `context.*`
  (`repoMap`/`semanticRag`/`lspGrounding`, cada um `{ enabled: Option<bool> }` via
  `FeatureToggle`) e `providers.ollama.structuredOutput` (fatia do ADR-0018 §5), cada bloco
  com seu próprio `merged_over` (mesma convenção de camada por camada já usada em
  `Permissions::union`); `schema_version` ganha `#[serde(alias = "schemaVersion")]` — o
  artefato real usa a grafia camelCase da ADR-0018, diferente da grafia original
  snake_case da ADR-0003. `Settings::from_file` (novo) localiza `.agentry/agentry.settings.json`
  via `state_dir::agentry_settings_path` (nova função em `crates/core/src/state_dir.rs` — só
  resolve o caminho, não cria diretório/gitignore, já que carregar configuração é
  leitura, não escrita) e reaproveita `from_json_str`: ausência do arquivo não é erro
  (`Settings::default`), JSON malformado é `ConfigError::Parse` tratado (nunca *panic*).
  `Config` ganha os 4 booleanos resolvidos (`repo_map_enabled`/`semantic_rag_enabled`/
  `lsp_grounding_enabled`/`ollama_structured_output`, *default* `true` quando nenhuma camada
  define — mesmo *default* das ADRs de origem). 7 testes novos (1 em `state_dir`, 6 em
  `config`, incluindo os 4 critérios de aceite literais do ticket: ausência não é erro,
  arquivo válido carrega, JSON inválido é erro tratado, ambiente sobrescreve o arquivo),
  237 testes na lib do core + 4 de integração + 11 na CLI, fmt/clippy limpos,
  `cargo build --release` verde. Nenhuma dependência nova (`b3357a6`).

- [x] **MT-40** — `crates/cli/src/main.rs`: até este commit, `repo_map`/`code_search`/
  `lsp_hover`/`lsp_definition` nunca tinham sido registradas na CLI de verdade (só existiam
  testadas dentro dos próprios módulos de tool, MT-21/24/30) e o `OllamaProvider` sempre
  saía com o *default* hardcoded (`structured_output: true`) — o ticket supunha que já
  havia fiação a substituir, mas na prática esta foi a primeira vez que as 4 capacidades
  ficaram de fato acessíveis pelo binário real. Nova `register_context_tools` (extraída de
  `main()` para ser testável sem rodar o binário inteiro) chama os 3 `register_*_tool` já
  existentes com os booleanos da `Config` resolvida (MT-39); `code_search` reaproveita o
  mesmo provider Ollama já registrado no Router para embeddings/reranking (clonado antes de
  `register_provider` consumir o `Arc`), não um segundo cliente. `OllamaProvider` ganhou
  `.with_structured_output(cfg.ollama_structured_output)` no builder. **Decisão registrada
  em comentário:** o *language server* de `lsp_hover`/`lsp_definition` fica hardcoded em
  `rust-analyzer` — seleção por linguagem/projeto é um ticket futuro, fora do escopo
  declarado ("UI/CLI de configuração"). 3 testes novos (flags true/false via a função
  extraída + inspeção de `ToolRegistry::specs()`; ausência de arquivo preserva o
  comportamento anterior); `ollama_structured_output` não ganhou teste próprio na CLI — a
  leitura do arquivo já é coberta pelo MT-39 e o efeito no `OllamaProvider` já é coberto
  pelo MT-22, a única peça nova aqui é uma chamada de builder de uma linha. Smoke-test
  manual contra Ollama real confirma que a fiação não regrediu o caminho feliz. 237 testes
  na lib do core + 4 de integração + 14 na CLI (11 + 3), fmt/clippy limpos,
  `cargo build --release` verde. Nenhuma dependência nova (`35362f6`). **Fecha o MT-40, a
  Fase 7 e o loop do `settings-schema:1` com o `ai-coding-agent-profiles` aberto desde o
  bootstrap do ecossistema.**

- [x] **ADR-0019** (Proposed) — bootstrap de `.agentry/agentry.settings.json` via `--init`
  (CLI)/`/init` (REPL): sem `--profile`, cria só o exemplo genérico da ADR-0018 §5, zero
  rede; com `--profile <nome>`, busca o arquivo real daquele perfil no `ai-coding-agent-
  profiles` **público**. Duas abordagens descartadas antes de fechar o desenho: `curl
  <script> | sh` (execução de código remoto sem *pinning*/revisão — anti-padrão de *supply
  chain*) e buscar o JSON direto **fora** do `Transport` — esta última, ao revisar contra os
  ADRs `Accepted` (disciplina da skill `adr-writer`), viola literalmente a Diretriz de
  Conformidade da ADR-0002 ("proibido qualquer chamada de rede fora do módulo de transporte
  central"). Resolvido sem emendar a ADR-0002: `Transport::new` já aceita uma
  `Allowlist`/`EgressClass` próprias por instância, então o bootstrap ganha uma instância
  dedicada (allowlist restrita a um host fixo, `EgressClass::CloudOk`) — cumpre a ADR-0002
  ao pé da letra em vez de contorná-la. Referência do `profiles` buscada fica **pinada como
  constante no código** (nunca "latest" dinâmico — reprodutibilidade sobre frescor,
  decisão explícita do usuário); comando manual (`setup-profile.sh`) sempre impresso como
  alternativa; falha de rede com `--profile` explícito nunca cai silenciosamente no exemplo
  genérico. `docs/interop/exchange-log.md` ganhou a oitava troca.
- [x] **Roadmap v0.3** (`docs/roadmap-v0.3.md`, novo — v0.2 permanece fechado/imutável) —
  ADR-0019 quebrada em 2 micro-tickets via skill `micro-ticket-planner`: **MT-41** (bootstrap
  local sem `--profile`, zero rede, reaproveita `state_dir::ensure_state_dir`/MT-38/39,
  idempotente) e **MT-42** (bootstrap via rede com `--profile`, `Transport` dedicado com
  `Allowlist`/`EgressClass::CloudOk` próprias, referência pinada, validação por
  `Settings::from_json_str` antes de gravar).
- [x] **MT-41** — `crates/cli/src/main.rs`: nova flag `--init` (`conflicts_with = "tarefa"`
  via clap); `crates/cli/src/repl.rs`: novo comando `/init`. Ambos chamam a mesma
  `run_init_local`/`escrever_resultado_init` (definidas em `main.rs`, visíveis para `repl.rs`
  via `crate::` — mesmo padrão de compartilhamento já usado por
  `overrides_from_args`/`parse_bool_toggle`). `run_init_local` reaproveita
  `state_dir::ensure_state_dir` (cria `.agentry/`+`.gitignore`, MT-38) e
  `state_dir::agentry_settings_path` (MT-39); grava o exemplo genérico exato da ADR-0018 §5
  só quando o arquivo ainda não existe — nunca sobrescreve customização do usuário.
  `escrever_resultado_init` sempre imprime também o comando manual (`setup-profile.sh`) como
  alternativa. **Mudança de assinatura:** `run_repl` ganha `workspace_root: &Path` (usado
  pelo `/init`), passado explicitamente em vez de ler `std::env::current_dir()` — os 7
  call-sites de teste já existentes passam `std::env::temp_dir()` (nenhum chama `/init`). 4
  testes novos (3 cobrindo os critérios de aceite diretamente sobre `run_init_local`/
  `escrever_resultado_init`; 1 rodando `/init` de ponta a ponta via `run_repl`, provando que
  `--init` e `/init` produzem o mesmo arquivo pela mesma função). Smoke-test manual do
  binário real confirma: cria com o conteúdo exato; segunda chamada não sobrescreve; `--init`
  + tarefa juntos é rejeitado pelo clap. 237 testes na lib do core + 4 de integração + 18 na
  CLI (14 + 4), fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova
  (`3a2075b`).
- [x] **MT-42** — `crates/core/src/transport/mod.rs`: `Transport` ganha `get_text` (GET
  simples, mesma política de egresso/audit de `post_json`) — necessário porque o `Transport`
  só tinha métodos POST; **escopo maior que o declarado no ticket** (arquivos previstos eram
  só `crates/cli/*`), mas inevitável — sem isso o fetch teria que ir por fora do `Transport`,
  violando a ADR-0002 (mesmo conflito já resolvido na própria ADR-0019). `crates/cli/src/init.rs`
  (novo): `fetch_profile_settings` busca o `agentry.settings.json` real de um perfil no
  `ai-coding-agent-profiles` público via uma instância de `Transport` **dedicada ao
  bootstrap** (`Allowlist` restrita a `raw.githubusercontent.com`, `EgressClass::CloudOk` —
  nunca a classe da sessão real), numa referência (commit) fixa gravada como constante
  (`d3ed413fbfcbb83da268bef540b924c26e2c3a2f`, HEAD real do `profiles` no momento do commit)
  — nunca "latest". Valida com `Settings::from_json_str` (`schemaVersion`) antes de aceitar;
  perfil desconhecido é rejeitado antes de qualquer rede. Núcleo parametrizado
  (`base_url`/`host`) para os testes apontarem a um servidor local, nunca o GitHub real.
  `crates/cli/src/main.rs` ganha `--profile` (`requires = "init"`); `run_init_local`
  refatorado sobre `write_settings_if_absent` (compartilhada entre o caminho local do MT-41 e
  o via rede daqui). `crates/cli/src/repl.rs`: `/init <perfil>` aceita o mesmo argumento.
  **Smoke-test manual do binário real contra o GitHub de verdade** confirma: busca o
  `agentry.settings.json` real do perfil `empresa` (com `_comentario` e `deny`/`ask`
  diferenciados preservados); perfil desconhecido falha antes de qualquer rede; `--profile`
  sem `--init` é rejeitado pelo clap. 7 testes novos (2 em `transport`, 5 em `init`), 239
  testes na lib do core + 4 de integração + 23 na CLI (18 + 5), fmt/clippy limpos,
  `cargo build --release` verde. Nenhuma dependência nova (`4f54169`). **Fecha o MT-42, a
  ADR-0019 e a Fase 8.**
- [x] **ADR-0007 emendada** — schema mínimo do Guardrail Gate fechado: `guardrails.input`/
  `guardrails.output` (array de `{ id, match, action }`, `action` em `block`/`redact`);
  substring/palavra-chave *case-insensitive*, sem `regex` nova (mesma filosofia de
  `fs_search`); merge por camada por `id`, mais severo vence (`block` > `redact`) —
  generalização de `Permissions::union`; bloqueio (entrada ou saída) substitui a mensagem por
  aviso fixo e a sessão continua normalmente, sem erro/retry; bloqueio na entrada nunca chama
  o provider; auditoria via par novo `GuardrailAuditEntry`/`GuardrailAuditSink` (não
  `AuditEntry`/`AuditSink` literais — carregam `profile`/`egress_class` irrelevantes a uma
  checagem de conteúdo), nunca loga o texto casado. A moderação semântica que a ADR-0007
  adiava para "v0.2" já foi coberta pela ADR-0015 (Reviewer) — complementares, não
  sobrepostas. `docs/interop/exchange-log.md` ganhou a nona troca (`a7db76d`).
- [x] **Roadmap v0.4** (`docs/roadmap-v0.4.md`, novo — v0.3 permanece fechado/imutável) —
  Guardrail Gate quebrado em 4 micro-tickets via skill `micro-ticket-planner`: **MT-43**
  (módulo `guardrail` novo — tipos/correspondência/auditoria, sem tocar Config/Session),
  **MT-44** (`GuardrailSettings` em `Config`, mesmo padrão de merge do MT-39), **MT-45**
  (`Session` aplica entrada/saída, hooks em `run`/`run_streaming` antes do Reviewer), **MT-46**
  (consumo real na CLI). Nenhum código implementado ainda.
- [x] **MT-43** — `crates/core/src/guardrail/mod.rs` (novo módulo de topo, paralelo a
  `egress`/`tools`): `GuardrailAction` (`Block`/`Redact`, `rank()` análogo a
  `EgressClass::rank()` — `Block` > `Redact`, para o merge por camada do MT-44),
  `GuardrailDirection` (`Input`/`Output`), `GuardrailRule` (`id`/`match_text`/`action`),
  `GuardrailCheckResult` (`Allowed`/`Redacted`/`Blocked`), `GuardrailGate` com `check()` —
  substring/palavra-chave *case-insensitive* via `to_ascii_lowercase`, sem `regex` (ADR-0007
  §1). `block` sempre checado primeiro, vence `redact` no mesmo texto; múltiplos `redact`
  que casam mascaram todas as ocorrências (`REDACTED_PLACEHOLDER` de `egress::redact`,
  reaproveitado por consistência visual). Auditoria via par novo `GuardrailAuditEntry`/
  `GuardrailAuditSink` — análogo a `AuditEntry`/`AuditSink` (MT-06), não literal (`profile`/
  `egress_class` não fazem sentido numa checagem de conteúdo); nunca loga o texto casado, só
  `direction`/`rule_id`/`action`/`task`; só emitido quando uma regra efetivamente age. Módulo
  isolado por design — não toca `Config` nem `Session` ainda. 9 testes novos, 248 testes na
  lib do core + 4 de integração + 23 na CLI, fmt/clippy limpos, `cargo build --release`
  verde. Nenhuma dependência nova (`7627c53`).
- [x] **MT-44** — `crates/core/src/guardrail/mod.rs`: `GuardrailAction`/`GuardrailRule`
  ganham `Serialize`/`Deserialize` (`rename_all = "lowercase"` na ação; `match_text`
  renomeado para `match` no JSON, palavra reservada em Rust) — mesmo tipo reaproveitado
  literalmente nos dois lados (regra em memória e regra do artefato), sem tipo paralelo só
  para o JSON. `crates/core/src/config/mod.rs`: `Settings` ganha `guardrails:
  GuardrailSettings` (schema `guardrails.input`/`guardrails.output`, ADR-0007 §2);
  `merged_over` une por `id` entre camadas — regra nova é adicionada, mesmo `id` em duas
  camadas resolve para a ação mais severa via `GuardrailAction::rank` (`block` > `redact`),
  nunca a mais permissiva (generalização de `Permissions::union`). `Config` ganha
  `guardrails: GuardrailGate`, resolvido direto da `GuardrailSettings` mesclada — reaproveita
  o tipo do MT-43 em vez de expor dois `Vec` soltos. 4 testes novos, 252 testes na lib do
  core + 4 de integração + 23 na CLI, fmt/clippy limpos, `cargo build --release` verde.
  Nenhuma dependência nova (`3039554`).
- [x] **MT-45** — `crates/core/src/session/mod.rs`: `Session::with_guardrails(gate, sink)`
  (*default* `None`, mesmo "desligado até configurado" de `with_reviews`).
  `aplicar_guardrail_entrada` roda antes do loop, sobre a mensagem de usuário mais recente —
  `block` substitui por aviso fixo e devolve `StopReason::Done` com zero turnos, **sem nunca
  chamar o provider** (zero egresso); `redact` mascara a mensagem antes de `build_request`.
  `aplicar_guardrail_saida` roda após `StopReason::Done`, **antes** de `revisar_ou_continuar`
  (Reviewer, ADR-0015) — `block` substitui a resposta e retorna via `ControlFlow::Break`
  (Reviewer nunca chega a rodar sobre conteúdo substituído); `redact` mascara e segue via
  `ControlFlow::Continue` (Reviewer roda em cima do texto já mascarado). `ColetorDuplo`
  (privado) encaminha cada `GuardrailAuditEntry` ao sink real e também acumula localmente,
  populando o novo campo `SessionOutcome::guardrail_hits` (paralelo a `reviews`).
  **Limitação conhecida, documentada no código:** em `run_streaming`, o texto já foi entregue
  a `on_event` (tipicamente exibido ao vivo) antes de chegar à checagem de saída — corrigir
  isso exigiria *buffer* da resposta inteira, o que desfaria o propósito de streaming; fora
  de escopo deste ticket. 5 testes novos (bloqueio de entrada nunca chama o provider; redact
  de entrada chega ao provider mascarado; bloqueio de saída pula o Reviewer mesmo habilitado;
  redact de saída mascara a resposta e o Reviewer ainda roda em cima dela — confirmando que
  o próprio Reviewer recebe o texto já mascarado; sessão sem `with_guardrails` nunca aplica
  nada), 257 testes na lib do core + 4 de integração + 23 na CLI, fmt/clippy limpos,
  `cargo build --release` verde. Nenhuma dependência nova (`6d46a51`).
- [x] **MT-47 adicionado** (`docs/roadmap-v0.4.md`) — a pedido do usuário, ao discutir a
  limitação encontrada no MT-45: em `run_streaming`, o texto de saída já é entregue a
  `on_event` em tempo real, turno a turno, antes de `aplicar_guardrail_saida` rodar sobre o
  texto completo. Correção decidida: **buffer condicional** — só quando `guardrails.output`
  tiver ao menos uma regra, `run_streaming` acumula a resposta inteira, roda a checagem, e só
  então emite os eventos (originais/mascarados/aviso fixo, conforme o resultado); sem regras
  de saída, o streaming continua 100% ao vivo, sem nenhuma mudança. Alternativas descartadas
  na discussão: janela deslizante (mais complexa, ainda deixa uma fresta perto da borda do
  buffer) e exigir `run` não-streaming para guardrails de saída (força demais a mão do
  chamador). Depende só do MT-45 (não do MT-46) — pode ser feito antes ou depois dele.
  Nenhum código implementado ainda.
- [x] **MT-46** — `crates/cli/src/main.rs`: `main()` constrói o `GuardrailGate` a partir da
  `Config` resolvida (MT-44) e chama `Session::with_guardrails` (MT-45); `StderrAuditSink`
  ganha `impl GuardrailAuditSink` (`Display` compacto, uma linha, mesma disciplina do `impl
  AuditSink` já existente). **Achado real ao validar o critério de aceite** (regra do
  arquivo precisa bloquear/redigir de ponta a ponta via a `Session` real de `main()`):
  `Config::resolve` em `main()` só recebia a camada `Settings::from_process_env()` — a
  camada do arquivo (`Settings::from_file`, MT-39) nunca era passada, apesar do MT-39/40
  estarem fechados como "consumo real". Na prática, `.agentry/agentry.settings.json` nunca
  chegava a ser lido pelo binário real; as 4 flags de contexto/provider (`repo_map`/
  `semantic_rag`/`lsp_grounding`/`structured_output`) só funcionavam de fato via variável de
  ambiente, nunca via arquivo. Opções discutidas com o usuário: corrigir dentro do MT-46
  (escolhida), ticket separado antes, ou só documentar e seguir. Corrigido extraindo
  `build_config(workspace_root)` — resolve as duas camadas reais na ordem certa (arquivo
  primeiro, ambiente por cima, mesma precedência já documentada em `Settings::from_file`) —
  e reordenando o cálculo de `workspace_root` para antes da resolução de `Config` (antes era
  feito bem depois, tarde demais para essa camada existir). 4 testes novos: leitura real de
  `guardrails.input`/`output` do arquivo via `build_config`; ausência do arquivo preserva
  `GuardrailGate` vazio; regra de entrada `block` bloqueia de ponta a ponta via `Session`
  real (`RegistryToolExecutor`/`ToolRegistry` reais, só o provider é mock — provider nunca
  chamado); regra de saída `redact` mascara a resposta de ponta a ponta. 257 testes na lib
  do core + 4 de integração + 27 na CLI (23 + 4), fmt/clippy limpos, `cargo build --release`
  verde. Nenhuma dependência nova (`ee33219`). **Fecha o segundo dos dois tickets da Fase 9**
  — falta só o MT-47.
- [x] **MT-47** — `crates/core/src/session/mod.rs`: buffer condicional em `run_streaming`
  (achado durante o MT-45 — `on_event` recebia cada `StreamEvent` em tempo real, turno a
  turno, antes de `aplicar_guardrail_saida` rodar sobre o texto completo; bloqueio/redação de
  saída só protegia `self.messages`/turnos seguintes, não o que já tinha sido transmitido ao
  vivo). `buffer_saida = self.guardrails` tem ao menos uma regra em `output`: sem nenhuma,
  zero mudança de comportamento (mesmo código de sempre, evento por evento, ao vivo). Com
  regra de saída, os eventos de cada turno deixam de ser repassados conforme chegam — só
  acumulados (via `StreamAggregator`, como já acontecia, e também guardados em ordem). Um
  turno com tool-calls (não é a resposta final — o Guardrail Gate só audita a resposta final,
  mesma disciplina do MT-45) repassa os eventos originais em lote no fim do turno, sem
  nenhuma checagem. O turno que encerra com `StopReason::Done`, depois de
  `aplicar_guardrail_saida` decidir Allowed/Redacted/Blocked, emite eventos **sintéticos**
  (`MessageStart`/`TextDelta`/`MessageEnd`, nova `emitir_texto_como_eventos`) com o texto já
  resolvido — nunca os eventos brutos originais, que em Redacted/Blocked ainda carregam o
  texto sem máscara. 5 testes novos (guardrail só de entrada não ativa o buffer de saída;
  regra de saída `block` nunca emite o texto original, só o aviso sintético; regra de saída
  `redact` emite só o texto mascarado; turno intermediário com tool-call repassado em lote e
  o turno final via evento sintético; teste de agregação já existente sem guardrails continua
  verde sem nenhuma alteração). 261 testes na lib do core + 4 de integração + 27 na CLI,
  fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova (`f60e5be`).
  **Fecha a Fase 9 inteira** (Guardrail Gate, ADR-0007).
- [x] **Housekeeping de status de ADR** — 16 de 19 ADRs estavam `Proposed`; 13 (0007..0019)
  promovidos a `Accepted` depois de confirmar individualmente (por `grep` contra o símbolo
  real no código — `GuardrailGate`, `CallPreset`, `RepoMapTool`, `CodeSearchSession`,
  `with_structured_output`, `register_lsp_tools`, `RuntimeOverride`, `reviewer`,
  `Session::compact`, `state_dir`, `schemaVersion`, `fetch_profile_settings`) que cada
  decisão está implementada e em vigor — não promoção em lote cega. **ADR-0003 e ADR-0004
  permanecem `Proposed`, verificados como genuinamente incompletos:** ADR-0003 (consumo dos
  artefatos do `profiles`) só teve a fatia `settings-schema:1` implementada (ADR-0018);
  leitura de `AGENTS.md`/`SKILL.md` via *progressive disclosure* ainda não existe no
  `agentry`. ADR-0004 (sinergia OSS) ganhou verificação parcial via `gh repo view
  rtk-ai/rtk` (real, não arquivado, Apache-2.0, ativo, release `v0.43.0` recente) registrada
  no próprio ADR — mas `stargazerCount: 70976` para um repositório com ~6 meses de vida
  reforça a suspeita original de números inflados, e a checagem de telemetria 100%
  desligável (bloqueante para adotar o **binário**, não só o padrão) não foi feita;
  `caveman`/`ponytail`/`OKF` seguem sem identificador de repositório conhecido em nenhum dos
  dois repos do ecossistema, sem verificação possível ainda. `docs/adr/README.md`
  atualizado (`5b8913a`).

- [x] **Site de documentação MkDocs** (a pedido do usuário) — `mkdocs.yml` na raiz,
  `docs_dir: docs`, três trilhas na nav. **Desenvolvimento** reaproveita o `docs/` já
  existente (architecture.md, ADRs, roadmaps, testing.md, interop/, CURRENT-STATE.md) sem
  duplicar nada — decisão confirmada com o usuário antes de escrever qualquer conteúdo.
  **Guia do Usuário** (`docs/usuario/`, novo): instalação, `agentry.settings.json` (schema
  completo — `permissions`/`context`/`providers`/`guardrails`, com exemplo JSON validado),
  flags da CLI + comandos de barra do REPL (verificados contra `Args`/`aplicar_comando` no
  código, não assumidos), guardrails de conteúdo do ponto de vista do operador, FAQ.
  **Governança & Compliance** (`docs/governanca/`, novo) — trilha para times de segurança
  avaliando uso interno: modelo de privacidade/egresso (com a ressalva real de que a CLI
  v0.1 só fala com Ollama local — adapters de nuvem existem na lib mas sem fiação de CLI
  ainda; e a exceção real de `--init --profile` contatar rede para buscar config, corrigida
  depois de uma inconsistência própria entre duas páginas), auditoria (dois sistemas
  independentes — egresso e guardrail —, o que cada um audita, o que nunca loga, e a
  limitação real de hoje só existir sink de stderr, sem persistência estruturada
  embutida), permissões de ferramentas (shell default-deny na CLI, distinto do
  default-allow do mecanismo genérico), guardrails (perspectiva de compliance), postura de
  dependências (ADR-0004, critério de adoção, sem telemetria conhecida hoje), FAQ.
  Framework-agnóstica por decisão do usuário (sem citar SOC2/ISO27001/LGPD) — descreve
  controles técnicos reais, nunca alega certificação; inclui seção "Maturidade e status"
  honesta (projeto pessoal, sem auditoria externa, v0.1) — decisão deliberada para não
  overclaiming numa página que existe para convencer um time de segurança.
  `mkdocs-material` não estava disponível via pip neste ambiente (sem root/apt) — instalado
  num venv isolado via `uv`, sem sudo; `docs-requirements.txt` documenta como reproduzir.
  Build local validado com `mkdocs build --strict`: zero warnings; todos os arquivos da nav
  existem (checado programaticamente); JSON de exemplo validado com `json.loads`. Nenhum
  deploy (GitHub Pages) configurado — decisão do usuário, só montagem local por enquanto.
  `README.md` ganhou seção "Documentação" com o passo a passo; `.gitignore` ignora `/site`
  e `.venv-docs/` (`8a4be44`).
- [x] **Roadmap v0.5** (`docs/roadmap-v0.5.md`, novo — v0.4 permanece fechado/imutável) —
  Fase 10, conexão configurável com LiteLLM (ADR-0006, já `Accepted` desde 2026-07-06 mas
  nunca ligado à CLI), quebrada em MT-48..51 via skill `micro-ticket-planner`. Nenhum ADR
  novo — ADR-0006 já decide o ponto mais sensível (classe de egresso por endpoint sempre
  explícita/configurável, ausência ⇒ `cloud-ok`/bloqueado em perfis restritivos, nunca
  inferida do host); confirmado com o usuário que é exatamente isso que ele queria (classe
  configurável, não hardcoded) antes de começar a implementar (`b18a65c`).
- [x] **MT-48** — `crates/core/src/config/mod.rs`: `LiteLlmSettings`
  (`baseUrl`/`model`/`egressClass`, todos opcionais) em `ProvidersSettings.litellm`,
  `merged_over` escalar (mesmo padrão de `OllamaSettings`). `Config::resolve` expõe
  `litellm: Option<LiteLlmConfig>` — `Some` só quando `baseUrl` **e** `model` estão
  presentes (LiteLLM não configurado não é erro); `egressClass` ausente nesse caso resolve
  `EgressClass::CloudOk` (ADR-0006 "fail-closed invertido para proxies", nunca
  `local-only` por inferência do host). Chave de API deliberadamente fora do schema —
  documentado no próprio doc comment que vem de `AGENTRY_LITELLM_API_KEY` (MT-49), nunca do
  arquivo. 5 testes novos (schema completo; ausência de `egressClass` → `cloud-ok`; só
  `baseUrl` ou só `model` → `None`; ausência do bloco inteiro preserva `None`; camada mais
  específica sobrescreve campo a campo, inclusive parcialmente). 266 testes na lib do core
  (261+5) + 4 de integração + 27 na CLI, fmt/clippy limpos, `cargo build --release` verde.
  Nenhuma dependência nova (`ac28251`).
- [x] **MT-49** — `crates/core/src/transport/mod.rs` ganha `host_from_url(url)` (extrai só
  o host de uma URL completa — mesma extração que `Transport::authorize` já usa
  internamente — exposta para quem monta uma `AllowlistEntry` fora do módulo de transporte
  a partir de uma URL configurada precisar declarar exatamente o mesmo host que será
  checado depois). `crates/cli/src/main.rs` ganha `build_litellm_provider(cfg, api_key)` —
  quando `cfg.litellm` (MT-48) é `Some`, monta uma segunda instância de `Transport`
  dedicada (mesma disciplina do bootstrap `--profile`, ADR-0019) com allowlist restrita ao
  host de `base_url` sob a `egress_class` já resolvida (nunca inferida — ADR-0006); anexa
  `Authorization: Bearer` só quando `api_key` é `Some`. **Decisão de design:** `api_key` é
  parâmetro explícito da função, não lido do ambiente dentro dela — `main()` lê
  `AGENTRY_LITELLM_API_KEY` e repassa, pra não acoplar os testes a variáveis de ambiente
  reais (evita *flakiness* em testes paralelos). Provider registrado no `Router` como
  segundo candidato da `task-class` "chat", depois de Ollama — zero mudança de
  comportamento *default* para quem não configurar `providers.litellm` (confirmado com
  smoke-test manual do binário real). `crates/cli/src/repl.rs`: `set_chat_route` ganha um
  `candidato_extra` opcional — como `Router::set_route` substitui a `RouteEntry` inteira
  (não existe "adicionar candidato"), o candidato LiteLLM precisa ser redeclarado a cada
  `/model`, senão desapareceria silenciosamente na primeira troca de modelo no REPL;
  `run_repl` reagrupou `workspace_root`/`preset_base`/`candidato_extra` num novo struct
  `ReplConfig` (`clippy::too_many_arguments` batia no limite de 7 com o parâmetro novo). 9
  testes novos (`host_from_url` no core; `build_litellm_provider` — ausência preserva
  `None`, configuração completa monta provider+candidato corretos, `baseUrl` inválida é
  erro tratado; `Router` com os dois candidatos resolve o preferencial por *default* e o
  `litellm` quando pedido via `RuntimeOverride.provider`, mesmo mecanismo que o MT-50 vai
  expor por flag). 268 testes na lib do core (266+2) + 4 de integração + 31 na CLI (27+4),
  fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova (`a714182`).
- [x] **ADR-0020** (Proposed) — discussão do usuário sobre uso de contexto: `.claudeignore`
  (já usado por `fs`/`repo_map`/`code_search`) resolve confidencialidade, mas nada resolve
  ruído de contexto por artefatos já cobertos por `.gitignore`. Pesquisa via `WebSearch`
  antes de decidir: `.claudeignore` **não é um recurso real do Claude Code** (convenção
  mal-atribuída, espalhada por documentação gerada por IA); o `OpenCode` real resolve os
  dois lados — `.gitignore` respeitado por padrão nas tools de busca, mais um arquivo
  próprio nativo (`.opencodeignore`) para exclusões do agente. Verificação também revelou
  que `.claudeignore` não é invenção só do `agentry` — é artefato de verdade distribuído
  pelos 3 perfis do `ai-coding-agent-profiles` (arquivo real, referenciado no `SPEC.md`
  canônico, no `setup-profile.sh`, na skill `secrets-guard`), o que elevou "renomear 3
  constantes" para uma mudança de contrato de interop entre dois repositórios — por isso
  virou ADR antes de qualquer código. Decisão: `.agentryignore` como artefato próprio do
  `agentry` (mesmo padrão de posse do `.agentry/`, ADR-0017), *fallback* para
  `.claudeignore` quando `.agentryignore` está ausente (nunca merge — `.agentryignore`
  vence sozinho quando presente); nova opção `context.gitignore.enabled` (*default*
  `false`) para também respeitar `.gitignore`, sempre em união com
  `.agentryignore`/`.claudeignore`, nunca substituindo. Escopo desta rodada é só o lado
  `agentry` — migração dos `.claudeignore` reais do `profiles` fica para uma sessão futura
  naquele repositório. ADR-0003 emendada (ainda `Proposed`) removendo `.claudeignore` da
  lista de artefatos de primeira classe do contrato de interop. **Fase 11 adicionada ao
  roadmap-v0.5.md** (MT-52 renomeia com *fallback*; MT-53 schema + consumo de
  `context.gitignore`; MT-54 documentação do site) — nenhum código implementado ainda
  (`3b851cb`).
- [x] **MT-50** — `crates/cli/src/main.rs`: `Args` ganha `-p, --provider <nome>`,
  encaminhado por `overrides_from_args` (mesmo padrão de `--model`) pro
  `RuntimeOverride.provider` que já existia desde a ADR-0014/MT-33 mas nunca tinha sido
  ligado a nada real. Sem a flag, comportamento atual preservado (Ollama por *default*).
  **Uso esperado:** `--provider litellm` **sem** `--model` junto seleciona o único
  candidato `litellm` declarado (modelo vem de `providers.litellm.model`, MT-48) —
  `resolve_with_override` filtra por provider **e** model quando os dois estão definidos
  (E, não OU), então passar `--model` junto exige bater exatamente com o modelo
  configurado; omitir `--model` é o caminho simples. `crates/cli/src/repl.rs`:
  `aplicar_comando` ganha o braço `"provider"` — troca `overrides.provider` e devolve
  `mudou_model=false` (diferente de `/model`, o candidato `litellm` é estático, não precisa
  redeclarar a rota — `resolve_with_override` já refiltra a cada comando). 6 testes novos
  (`overrides_from_args` mapeia a flag, presente e ausente; REPL com dois candidatos
  registrados troca de verdade via `/provider` sem precisar de `/model`; provider
  desconhecido propaga o erro de resolução do `Router`, sem *panic*). Confirmado com
  smoke-test real do binário: `--provider litellm` ataca o `baseUrl` configurado, sem a
  flag continua indo para o Ollama. 268 testes na lib do core + 4 de integração + 35 na CLI
  (33+2), fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova
  (`4aee255`). **Fecha o penúltimo ticket da Fase 10** — falta só o MT-51.
- [x] **Makefile de distribuição** (a pedido do usuário — precisava testar contra o LiteLLM
  da empresa dele, num computador Windows). `Makefile` na raiz (`make` sem argumento lista
  os alvos): `windows-build` cross-compila (`x86_64-pc-windows-gnu`, reaproveita o
  `.cargo/config.toml` local já resolvido pra pegadinha posix/win32 do `mingw-w64`,
  documentada em `docs/testing.md`); `windows` compila e empacota `agentry.exe` +
  `README.md` + `LICENSE` num zip flat (`zip -j`) em
  `dist/agentry-windows-x86_64-<versão>.zip`; `windows-clean` limpa `dist/`. Rodado de
  ponta a ponta nesta máquina: gera um PE32+ válido, zip de ~83MB — **grande demais para o
  limite de upload do chat (30MB)**; usuário optou por pegar o arquivo direto do
  filesystem, não pediu divisão em partes nem redução de tamanho do binário. `.gitignore`
  ganhou `/dist` e `/.cargo/` (esse último já era documentado como "não versionar" no
  `testing.md`, mas nunca tinha sido de fato ignorado — corrigido). `README.md` ganhou a
  seção "Distribuir para Windows"; `docs/testing.md` referencia o atalho (`0a0897a`).
- [x] **Fix: `agentry.settings.json` gerado por `--init` não tinha exemplo de
  `providers.litellm`** — achado real do usuário testando o MT-49/50: a única forma de
  descobrir a chave certa (`baseUrl`/`model`/`egressClass`, camelCase) era ler o
  código-fonte ou a ADR-0006. Princípio pedido pelo usuário: **tudo que for configurável
  precisa já vir no arquivo, com default ou como campo exemplo.** JSON não tem comentário —
  `GENERIC_SETTINGS_EXAMPLE` passa a usar `null` como o equivalente mais próximo de "campo
  existe, ainda desligado" (a chave fica descobrível sem ativar nada — `Config::resolve` só
  registra o candidato `litellm` quando `baseUrl` **e** `model` estão os dois presentes,
  MT-48). Campos novos no exemplo: `profile`/`model`/`max_tokens` (topo),
  `providers.litellm.{baseUrl,model,egressClass}`, `guardrails.{input,output}` (vazios).
  Teste novo prova que o exemplo é JSON válido do schema real e que nenhum `null` ativa
  nada sozinho. **Achado adicional do smoke-test manual, importante para o teste do
  usuário:** `egressClass: null` fica **bloqueado por padrão** (fail-closed correto,
  ADR-0006 "invertido para proxies") mesmo sob perfil `local-only` — o usuário precisa
  declarar `"egressClass": "local-only"` **explicitamente** no arquivo (caso dele, gateway
  só em VPN interna) para o candidato `litellm` ficar de fato alcançável; confirmado
  também que, com essa declaração explícita, a conexão é tentada de verdade. 36 testes na
  CLI (35+1), 268 na lib do core + 4 de integração, fmt/clippy limpos, `cargo build
  --release` verde. Nenhuma dependência nova (`ed0988c`).
- [x] **Config autoexplicativa via `_comentario`** — usuário pediu exemplos detalhados +
  comentários e perguntou sobre migrar o formato inteiro pra TOML (nativo em comentários).
  Investigado antes de decidir (mesmo protocolo do `.agentryignore`): o
  `ai-coding-agent-profiles` já distribui `.agentry/agentry.settings.json` real em JSON,
  com ferramenta de merge não-destrutivo própria pra JSON
  (`update_json_settings()`/`hybrid_json` em `scripts/setup-profile.sh`) — trocar de
  formato quebraria essa ferramenta e criaria dois formatos coexistindo (`--init` genérico
  em TOML vs. `--init --profile`, que continuaria vindo em JSON de lá). **Achado
  decisivo:** os arquivos reais dos 3 perfis já usam uma chave `_comentario` (prefixo `_`,
  ignorada pelo parser real — `Settings` não usa `deny_unknown_fields`) exatamente pra
  esse propósito — convenção já estabelecida no ecossistema, não uma invenção nova.
  Decisão do usuário: manter JSON, estender essa convenção a cada bloco do
  `GENERIC_SETTINGS_EXAMPLE` (topo, `permissions`, `context`, `providers`,
  `providers.litellm`, `guardrails`). Zero mudança de comportamento (chaves `_` já eram
  ignoradas antes), nenhum ADR novo (consumo de convenção já registrada do lado
  `profiles`, não decisão arquitetural nova). Suíte inalterada — o teste que valida o
  exemplo como JSON válido/todo campo `null` inerte continua verde. 36 testes na CLI, 268
  na lib do core + 4 de integração, fmt/clippy limpos, `cargo build --release` verde.
  Pacote Windows (`dist/`) regenerado com o exemplo atualizado (`95406c1`).
- [x] **MT-51** — `docs/usuario/configuracao.md` ganha a seção `providers.litellm`
  completa (`baseUrl`/`model`/`egressClass`, `AGENTRY_LITELLM_API_KEY`, fail-closed quando
  `egressClass` ausente) + exemplo JSON atualizado + nota sobre `_comentario`. A afirmação
  "nenhum destino de rede além do Ollama local"/"não há caminho de configuração" — que
  deixou de ser verdade a partir do MT-49 — foi corrigida **em toda parte onde aparecia**:
  além de `docs/governanca/privacidade-e-egresso.md` (o único arquivo listado no ticket),
  um `grep` encontrou mais 3 ocorrências (`docs/governanca/index.md`,
  `docs/governanca/faq.md`, `docs/usuario/faq.md`), todas corrigidas também — reafirma o
  que continua verdade (Ollama local por padrão sem nenhuma configuração; LiteLLM é opt-in,
  exige escolha explícita via `--provider`/`/provider`; classe de egresso sempre declarada,
  nunca inferida do host; Anthropic ainda sem fiação de CLI). **Achado adicional durante a
  revisão:** `mkdocs.yml` já estava com `--strict` quebrado antes desta sessão —
  `roadmap-v0.5.md` e `adr/0020` não estavam na `nav` (corrigido); `docs/testing.md` tinha
  um link relativo pra `README.md` que fica fora do `docs_dir` (trocado por URL real do
  GitHub). `mkdocs build --strict` validado limpo depois de cada edição; anchors novos
  (`#providerslitellm`, `#flags-de-invocacao-one-shot`) conferidos direto no HTML gerado,
  não por suposição (`9c5e495`). **Fecha a Fase 10 inteira** (MT-48..51) — roadmap marcado
  concluído (`3f908bf`).
- [x] **Planejamento de longo prazo** (a pedido do usuário — roadmap rumo à paridade com
  Claude Code CLI/OpenCode, englobando a Fase 11 e os temas tools/TUI + os que levantei).
  Feito em **plan mode** (aprovado). Decisões do usuário (via `AskUserQuestion`): profundidade
  = roadmap-mestre + stubs de ADR + títulos de ticket, 1ª fase detalhada; sequência = Config →
  AGENTS.md/Skills → Tools → TUI → MCP → 2ª onda; SearXNG = desabilitado até o usuário
  configurar a URL. Requisitos específicos incorporados: tool **AskUser** (Fase 14, precedente
  = trait `Confirmer`), **web search anônimo via SearXNG configurável** (Fase 14, passa pelo
  `Transport`), **config de task-class completa** (Fase 12 — hoje o `Router` suporta mas a CLI
  hardcoda; `compact`/`guardrail-compliance` nem são registradas), **todo config com
  default+comentário+exemplos** (ADR-0022). Novos: `docs/roadmap-longo-prazo.md` (mapa Fases
  11–17+, supersede o esboço v0.2/v0.3 de `architecture.md`), `docs/roadmap-v0.6.md` (Fase 12
  detalhada, MT-55..58), `docs/adr/0021` (schema task-class, Proposed), `docs/adr/0022`
  (convenção autoexplicativa, Proposed). `adr/README.md` + `architecture.md` + `mkdocs.yml`
  atualizados; `mkdocs build --strict` limpo. Faixa ADR-0023..0028 reservada (arquivos
  escritos ao iniciar cada fase). Nenhum código — planejamento (`de46792`). Plan file:
  `~/.claude/plans/majestic-gathering-codd.md`.
- [x] **Infraestrutura de execução autônoma** (`.claude/commands/implementar-roadmap.md` +
  `docs/decisoes-autonomas.md`) — ver detalhes no turno anterior do handoff/histórico
  (`c8cf8a8`).
- [x] **MT-52** (execução autônoma via `/loop`, modelo Sonnet 5) — `resolve_ignore_file_name`
  centralizada em `crates/core/src/tools/mod.rs`: `.agentryignore` checado primeiro,
  *fallback* para `.claudeignore` quando ausente; se os dois existirem, `.agentryignore`
  vence **sozinho** (nunca merge, ADR-0020 §2). As três tools (`fs`/`repo_map`/
  `code_search`) — que tinham a mesma constante `.claudeignore` triplicada — passam a
  chamar a função compartilhada. 7 testes novos/renomeados (`resolve_ignore_file_name` nos
  4 cenários de precedência; `fs`/`repo_map` ganham o caso `.agentryignore` sozinho e o
  caso "os dois presentes"; o teste antigo virou o caso de *fallback* legado). 275 testes
  na lib do core (268+7) + 4 de integração + 36 na CLI, fmt/clippy limpos, `cargo build
  --release` verde. Smoke-test do binário real sem regressão (sem Ollama disponível pra
  exercitar uma tool-call completa; cobertura de unidade já exercita o caminho de produção
  diretamente). Nenhuma decisão-sob-dúvida neste ticket — escopo objetivo, sem registro em
  `decisoes-autonomas.md` (`d742265`).
- [x] **MT-53** — `ContextSettings` (`crates/core/src/config/mod.rs`) ganha `gitignore:
  FeatureToggle` (`context.gitignore.enabled`, mesmo padrão de
  `repoMap`/`semanticRag`/`lspGrounding`); `Config.respect_gitignore: bool`, *default*
  `false` — **opt-in**, diferente das outras flags de `context.*` (default `true`): reduzir
  ruído de contexto nunca muda o comportamento de quem não configurou nada. As três tools
  ganham o parâmetro: `fs.rs` soma `.gitignore` ao `GitignoreBuilder` já existente (união
  real, um só matcher); `repo_map`/`code_search`/`FsSearchTool` ganham
  `.git_ignore(respect_gitignore)` no `WalkBuilder`. **Achado real ao testar:** a crate
  `ignore` só respeita `.gitignore` dentro de um repo git de verdade por padrão
  (`WalkBuilder::require_git`, `true`) — duas suítes falharam até eu descobrir isso;
  corrigido com `.require_git(false)` nos três `WalkBuilder` (não é decisão-sob-dúvida, é
  correção de comportamento real da dependência — não entra em `decisoes-autonomas.md`).
  `crates/cli/src/main.rs`: as 4 tools de `fs` + `RepoMapTool` + `CodeSearchSession` passam
  a receber `cfg.respect_gitignore` na construção real. **Autocorreção:** o commit de código
  (`3bbd934`) alegou "8 testes novos" incluindo cobertura de schema que na verdade não tinha
  sido escrita — faltavam 2 testes de `config/mod.rs` (parsing/merge/resolução de
  `context.gitignore.enabled`); corrigido num commit separado e honesto (`6151e26`), sem
  `--amend`. 282 testes na lib do core (280+2, mais os das tools) + 4 de integração + 36 na
  CLI, fmt/clippy limpos, `cargo build --release` verde. Smoke-test do binário real confirma
  que o novo bloco `context.gitignore` parseia sem erro. Nenhuma dependência nova.
  **Fecha o penúltimo ticket da Fase 11** — falta só o MT-54.
- [x] **MT-54** — `docs/usuario/configuracao.md`: exemplo JSON ganha `context.gitignore`;
  `### context` documenta `gitignore.enabled` (default `false`, opt-in — diferente das
  outras três flags de `context.*`, default `true`); nova seção "Arquivo de ignore do
  `agentry` (`.agentryignore`)" explicando o mecanismo (sintaxe `.gitignore`, independente
  de versionamento, *fallback* pra `.claudeignore`, precedência sem merge).
  `docs/governanca/permissoes.md`: a seção final estava **desatualizada** — dizia que
  granularidade por conteúdo de arquivo "fica para configuração futura", mas
  `.agentryignore` já é esse mecanismo, existe desde o MT-52; reescrita explicando pro
  público de compliance e deixando explícito o ponto pedido: `.agentryignore`
  (confidencialidade, independente do Git) e `context.gitignore.enabled` (ruído de
  contexto, opt-in, zero efeito de confidencialidade) são mecanismos distintos. Varredura
  por `grep` confirmando nenhuma outra menção desatualizada. `mkdocs build --strict` limpo,
  validado duas vezes; anchor novo conferido direto no HTML gerado. JSON do exemplo
  validado (`json.loads`). **Fecha a Fase 11 inteira** (MT-52..54) — roadmap marcado
  concluído (`a13eb98`).

- [x] **MT-55** — `crates/core/src/config/mod.rs`: bloco `taskClasses` (mapa `nome →
  { candidates: [{ provider, model, egressClass }], preset: { temperature, topP, maxTokens,
  systemPrompt, reasoning } }`) via `TaskClassCandidateSettings`/`TaskClassPresetSettings`/
  `TaskClassSettings`, com `merged_over` por nome (`merge_task_classes`/
  `merge_candidatos_de_task_class` — candidato mais específico vence por par
  `(provider, model)`, egresso **nunca afrouxa**, mesma disciplina de `Permissions::union`).
  `Config::resolve` expõe `task_classes: HashMap<String, RouteEntry>`, reaproveitando
  `RouteEntry`/`RouteTarget`/`CallPreset` do `Router` (ADR-0008/0014) — sem tipo novo de
  roteamento. **Desvio do texto original do ticket, registrado em
  `docs/decisoes-autonomas.md`:** `Config` não sintetiza os defaults `chat`/`compact`/
  `guardrail-compliance` quando ausentes — ausência resolve em mapa vazio; a síntese de
  defaults concretos de provider/modelo (que exigiria `crates/core` conhecer `"ollama"` como
  escolha de produto) fica deferida à CLI, MT-56, que já é o ponto que hoje hardcoda essa
  escolha via `set_chat_route`. 5 testes novos (schema completo resolve `RouteEntry` exato;
  ausência resolve mapa vazio; camada mais específica sobrescreve preset por nome; merge por
  nome soma task-class nova sem apagar herdada; mesmo candidato em duas camadas nunca afrouxa
  a classe de egresso, nas duas ordens), 287 testes na lib do core (282+5) + 4 de integração +
  36 na CLI, fmt/clippy limpos, `cargo build --release` verde. Nenhuma dependência nova.

- [x] **MT-56** — `crates/cli/src/main.rs`/`repl.rs`: `register_declared_task_classes`
  (main.rs) registra no `Router` toda task-class declarada em `cfg.task_classes` (MT-55) e
  sintetiza os defaults `compact`/`guardrail-compliance` quando ausentes (Ollama
  `local-only` + preset default) — responsabilidade herdada do desvio do MT-55; `chat`
  continua sintetizada por `repl::set_chat_route` (chamada antes, para que uma task-class
  `chat` declarada no arquivo possa sobrescrevê-la depois). Nova flag `--task-class <nome>`
  (one-shot) e comando `/task-class <nome>` (REPL) escolhem entre as task-classes já
  registradas para a invocação — mesmo padrão vetado de `--provider`/`--model` (ADR-0014):
  nunca introduz um alvo não declarado, nome desconhecido/candidato indisponível é o mesmo
  erro tratado de `Router::resolve_with_override`, sem *panic*. `/model` continua
  redeclarando especificamente `chat` (documentado, decisão de escopo — não um desvio: evita
  assumir Ollama como provider de uma task-class customizada que pode apontar só para
  LiteLLM). `/compact` (ADR-0016) e o Reviewer (ADR-0015) passam a ter rota real na CLI
  distribuída pela primeira vez. 7 testes novos (4 em `main.rs`, 3 em `repl.rs`), 43 testes na
  CLI (36+7) + 287 no core, fmt/clippy limpos, `cargo build --release` verde. Smoke-test
  manual contra Ollama real confirma `--task-class`/`/task-class` ponta a ponta (config
  custom → resposta real do modelo, nos dois modos). Nenhuma dependência nova.

- [x] **MT-57** — `crates/cli/src/main.rs`: `GENERIC_SETTINGS_EXAMPLE` ganha o bloco
  `taskClasses` — `chat` com o mesmo par (Ollama, `DEFAULT_MODEL`, `local-only`) do
  comportamento zero-config (declará-lo não muda nada observável) e dois exemplos extras
  comentados (`revisao-em-nuvem` cloud-ok via litellm, `dados-sensiveis` local-only), inertes
  até escolhidos via `--task-class`/`/task-class`. Como `taskClasses` é
  `HashMap<String, TaskClassSettings>` sem *wrapper*, uma chave `_comentario` solta no bloco
  quebraria o parse — a explicação do mecanismo entra dentro do `_comentario` da própria
  `chat`. Auditoria dos demais blocos (ADR-0022) encontrou um gap real:
  `context.gitignore.enabled` nunca tinha sido adicionado ao exemplo real gerado por `--init`
  desde o MT-53/54 (só a doc do site tinha o campo) — corrigido junto; `permissions`/
  `guardrails` ganharam exemplos **textuais** no `_comentario` (nunca como entradas reais, que
  mudariam o comportamento default). Teste
  `generic_settings_example_e_json_valido_e_todo_campo_null_fica_inerte` estendido: resolve
  exatamente os 3 nomes de `taskClasses` declarados, sem sintetizar `compact`/
  `guardrail-compliance` (responsabilidade da CLI, MT-56); `context.gitignore.enabled=false`
  preserva `respect_gitignore=false`. 43 testes na CLI (extensão de teste existente, sem
  testes novos) + 287 no core, fmt/clippy limpos, `cargo build --release` verde. Smoke-test
  manual do `--init` real confirma JSON válido e uma tarefa *one-shot* contra Ollama real
  idêntica com o arquivo gerado presente. Nenhuma dependência nova.

- [x] **MT-58** — `docs/usuario/configuracao.md`: nova seção `### taskClasses` (candidatos,
  preset, defaults sintetizados, seleção via `--task-class`/`/task-class`, merge por nome sem
  afrouxar egresso) e nova seção `## Convenção: todo bloco vem com exemplo` (ADR-0022);
  exemplo JSON de "Estrutura do arquivo" ganha o bloco `taskClasses`. `docs/usuario/uso.md`
  documenta `--task-class`/`/task-class` e a nota de que `/model` sempre atua sobre `chat`,
  independente da task-class ativa (MT-56). Releitura ("nada ficou desatualizado") encontrou
  um gap pré-existente desde o MT-50: `--provider`/`-p` e `/provider` nunca tinham sido
  documentados nas tabelas de flags/comandos, apesar de já existirem no binário e de
  `configuracao.md` já linkar para eles — corrigido junto. `mkdocs build --strict` limpo;
  *anchors* de todos os *cross-links* novos conferidos direto no HTML gerado; JSON de exemplo
  validado. Nenhuma mudança de código. **Fecha a Fase 12 inteira (MT-55..58)** — o tema mais
  enfatizado pelo usuário no planejamento original.

- [x] **Preparação da Fase 13** — ADR-0023 (`Proposed`) decide: `AGENTS.md` primário /
  `CLAUDE.md` *fallback* (nunca merge, mesma precedência do ADR-0020); concatenados numa única
  mensagem de sistema junto do preset da `task-class` ativa; leitura sempre respeita
  `.agentryignore`/`.claudeignore`; `.claude/skills/*/SKILL.md` reaproveitado verbatim
  (compatibilidade direta com a convenção já existente do Claude Code, inclusive a deste
  próprio repositório); skill completa carregada só sob demanda via nova tool `skill` (mesmo
  padrão `Tool`/`ToolRegistry` do MT-11). Decisão-sob-dúvida registrada: parser de
  frontmatter de `SKILL.md` **próprio** (só `name`/`description`, incluindo bloco dobrado
  `>-`), não uma dependência YAML — decidir isso na ADR evita o gatilho de parada dura do loop
  para dependência nova. `docs/roadmap-v0.7.md` detalha MT-59 (loader AGENTS.md/CLAUDE.md),
  MT-60 (descoberta de SKILL.md), MT-61 (tool `skill`), MT-62 (documentação + ADR-0003 →
  `Accepted`), sequência estritamente linear. Housekeeping: ADR-0020/0021/0022 promovidas a
  `Accepted` (gap de status desatualizado, mesma categoria dos gaps corrigidos no MT-57/58).
  `mkdocs build --strict` limpo. Nenhuma mudança de código.

- [x] **MT-59** — `crates/core/src/project_instructions.rs` (novo):
  `load_project_instructions(root, ignore)` lê `AGENTS.md` (primário) ou `CLAUDE.md`
  (*fallback*, nunca os dois, mesma precedência do ADR-0020), pulando caminho coberto por
  `.agentryignore`/`.claudeignore`. `tools::fs::load_ignore` promovida de privada para **`pub`**
  (não `pub(crate)` — a CLI, crate diferente, precisa montar o mesmo `Gitignore`). `Session`
  ganha `with_project_instructions(String)`; `ensure_system_prompt` concatena instruções de
  projeto + `system_prompt` do preset numa única mensagem de sistema (projeto primeiro).
  `context.agentsFile.enabled` (*default* `true`, diferente do opt-in de `gitignore`) liga/
  desliga. 11 testes novos (6+3+2), 298 testes no core (287+11) + 43 na CLI, fmt/clippy
  limpos, `cargo build --release` verde. Smoke-test manual contra Ollama real confirma
  `AGENTS.md` influenciando a resposta de fato (instrução seguida) e o *opt-out* funcionando.
  Nenhuma dependência nova.

- [x] **MT-60** — `crates/core/src/skills.rs` (novo): `discover_skills(root, ignore)` varre
  `<root>/.claude/skills/*/SKILL.md` (um nível, sem recursão) e extrai `name`/`description`
  via parser de frontmatter próprio (decisão da ADR-0023 — cobre `chave: valor` de uma linha e
  o bloco dobrado `chave: >-`); `SKILL.md` malformado ou coberto por
  `.agentryignore`/`.claudeignore` é pulado silenciosamente, sem interromper a descoberta das
  demais. `render_skills_list` formata a lista compacta. `Session` ganha `with_skills_list`;
  `ensure_system_prompt` concatena, nesta ordem, instruções de projeto + preset + lista de
  skills (por último). `main.rs` descobre as skills sem *opt-out* próprio (custo desprezível).
  **Achado durante o teste da *fixture* real:** literal Rust com continuação de linha (`\`)
  remove a indentação da linha seguinte, destruindo o bloco dobrado do teste — corrigido com
  *raw string* (`r#"..."#`); bug do dado de teste, não do parser. 8 testes novos (6+2), 306
  testes no core (298+8) + 43 na CLI, fmt/clippy limpos, `cargo build --release` verde.
  Smoke-test manual contra Ollama real, rodado neste próprio repositório (5 skills reais em
  `.claude/skills/`): o modelo listou as 5 corretamente a partir do *system prompt* injetado.
  Nenhuma dependência nova.

- [x] **MT-61** — `crates/core/src/tools/skill.rs` (novo): `SkillTool` implementa `Tool`
  sobre o `Vec<SkillDescriptor>` do MT-60 — `{"name": "<skill>"}` devolve o corpo do
  `SKILL.md` correspondente (tudo após o `---` de fechamento, nunca os metadados); nome
  desconhecido/argumento ausente é erro tratado. Registrada como qualquer outra tool, sob o
  mesmo `PermissionGate`, sem *default-deny* especial. `main.rs`: descoberta de skills e
  `context_ignore` subiram para antes da montagem do `ToolRegistry` (a tool precisa do
  `Vec<SkillDescriptor>` no momento do registro). 6 testes novos, 312 testes no core (306+6) +
  43 na CLI, fmt/clippy limpos, `cargo build --release` verde. **Smoke-test:** tentativa de
  invocação via linguagem natural não confirmou o *round-trip* completo — modelos locais
  disponíveis não chamaram a tool de fato mesmo para `fs_read` (já madura), simulando resposta
  em vez de *tool-call* real — limitação de confiabilidade de *tool-calling* de modelos locais
  pequenos neste ambiente, não regressão do ticket; correção coberta com confiança pelos
  testes de integração via `ToolRegistry::execute` real (inclusive o gate de permissão).
  Nenhuma dependência nova. **Fecha o mecanismo de *progressive disclosure* (MT-59..61).**

- [x] **MT-62** — `docs/usuario/configuracao.md`: nova seção "Memória de projeto
  (`AGENTS.md`/`CLAUDE.md`)" (precedência sem merge, ordem de concatenação com o preset da
  `task-class`, relação com `.agentryignore`) + campo `agentsFile.enabled` na lista de
  `context`. Novo `docs/usuario/skills.md`: convenção `.claude/skills/<nome>/SKILL.md`
  (frontmatter obrigatório, corpo, subconjunto de YAML suportado pelo parser mínimo do MT-60),
  descoberta automática + carregamento sob demanda via a tool `skill` (MT-61). **ADR-0003**
  (`Proposed` desde o MT-04) promovida a `Accepted` — emenda registra que
  `.claude/settings.json` nunca foi consumido (artefato próprio, ADR-0018) e que os demais
  artefatos previstos estão todos implementados. **ADR-0023 também promovida a `Accepted`**
  (MT-59..62 concluídos). Achado de *anchor* do mkdocs (barra entre `AGENTS.md`/`CLAUDE.md` no
  título vira *slug* sem separador — `agentsmdclaudemd`, não `agentsmd-claudemd`) pego pelo
  próprio `mkdocs build --strict`, corrigido nos 2 *cross-links* que usavam. Nenhuma mudança
  de código. **Fecha a Fase 13 inteira (MT-59..62).**

- [x] **Preparação da Fase 14** — ADR-0024 (`Proposed`) decide: `trait Prompter` no `core`
  (padrão `AuditSink`, não `Confirmer`), `AskUserTool` mínima (texto livre + sugestões).
  ADR-0025 (`Proposed`) decide: coringa `"*"` novo na `Allowlist` para `WebFetch` (host
  arbitrário), liberado só sob `EgressClass::CloudOk` **e** `tools.webFetch.enabled` (*opt-in*
  explícito, *default* `false`); `WebSearch` via SearXNG usa o modelo de allowlist já
  existente (host único); anonimato como requisito de código (sem cookies, `User-Agent`
  genérico, sem `Referer`); HTML→Markdown fora de escopo (dependência nova, registrada para
  não ser decidida silenciosamente depois). ADR-0026 (`Proposed`) decide: `Glob` via
  `ignore::overrides` (zero dependência nova); shell em background como extensão de
  `ShellPolicy`/MT-13, nunca uma política paralela. `docs/roadmap-v0.8.md` detalha os 7
  tickets (MT-63..69), 4 trilhas independentes (AskUser, web, glob, shell background)
  convergindo em MT-69 (documentação). Nenhuma dependência nova proposta — nenhum gatilho de
  parada dura acionado. `mkdocs build --strict` limpo. Nenhuma mudança de código.

- [x] **MT-63** — `crates/core/src/tools/ask_user.rs` (novo): `trait Prompter`
  (dyn-compatible via `BoxFuture`) definido no core — padrão `AuditSink` (interface no core,
  implementação concreta de quem consome), não o padrão `Confirmer` (tipo só da CLI), já que
  `AskUserTool` implementa `Tool` e toda `Tool` vive em `agentry_core::tools`.
  `AskUserTool::new(Arc<dyn Prompter>)`; `execute()` lê `question` (obrigatório)/`options`
  (opcional) e devolve a resposta do `Prompter`; `question` ausente é erro tratado. 5 testes
  novos, 317 testes no core (312+5) + 43 na CLI (fiação real fica para o MT-64), fmt/clippy
  limpos, `cargo build --release` verde. Nenhuma dependência nova; sem mudança de
  comportamento observável da CLI ainda.

- [x] **MT-64** — `crates/cli/src/tool_executor.rs`: `InteractivePrompter` implementa
  `Prompter` (imprime a pergunta + sugestões numeradas via `formata_pergunta`, testável sem
  I/O real; lê uma linha de `stdin`, sem *parsing*/validação — mesmo padrão de
  `InteractiveConfirmer`). `crates/cli/src/main.rs` registra
  `AskUserTool::new(Arc::new(InteractivePrompter))` no `ToolRegistry`, junto das demais tools
  sempre ativas. 2 testes novos, 45 testes na CLI (43+2) + 317 no core, fmt/clippy limpos,
  `cargo build --release` verde. Smoke-test manual reproduziu a mesma limitação do MT-61 —
  modelo local não emitiu uma *tool-call* real para `ask_user` (não é regressão desta ticket);
  correção coberta pela equivalência estrutural com `InteractiveConfirmer` (já em produção
  desde o MT-14) + os testes de `AskUserTool`/formatação. Nenhuma dependência nova.
  **Fecha a trilha `AskUser` (MT-63/64, ADR-0024).**

- [x] **MT-65** — `crates/core/src/egress/allowlist.rs`: `ANY_HOST` (`"*"`), terceiro padrão
  de `AllowlistEntry::matches` (casa qualquer host, precisa ser adicionado explicitamente,
  continua fail-closed). Novo `crates/core/src/tools/web_fetch.rs`: `WebFetchTool` via
  `Transport::get_text`, `User-Agent` genérico fixo, corpo truncado a 20k caracteres. Novo
  `tools.webFetch.enabled` (*default* `false`) em `Settings`/`Config`. `main.rs`:
  `build_web_fetch_tool` só registra a tool quando `tools.webFetch.enabled=true` **e**
  `cfg.egress_class == CloudOk`. 14 testes novos, 327 testes no core (317+10) + 49 na CLI
  (45+4), fmt/clippy limpos, `cargo build --release` verde. Smoke-test confirma a fiação real
  (perfil `pessoal` resolve `cloud-ok`), reproduz a mesma limitação de *tool-calling* dos
  modelos locais já registrada (MT-61/64) — não regressão, coberta pelos testes automatizados.
  Nenhuma dependência nova.

- [x] **MT-66** — `tools.webSearch` (`searxngUrl`/`searxngEgressClass`) em `Settings`/
  `Config`, mesmo padrão de `providers.litellm` (ausência ⇒ não registrada; classe ausente ⇒
  `cloud-ok`, mas *self-hosted* pode declarar `local-only`, diferente do coringa fixo do
  `web_fetch`). `transport/mod.rs` ganha `build_searxng_search_url` (percent-*encoding*
  correto via `reqwest::Url::query_pairs_mut`) — mantém `reqwest` confinado ao módulo de
  transporte, guard test preservado. Novo `crates/core/src/tools/web_search.rs`:
  `WebSearchTool` consulta a API JSON do SearXNG via `Transport::get_text` (host único, sem
  coringa), resultados formatados (título/URL/resumo, capados a 8). `main.rs`:
  `build_web_search_tool` só registra quando `searxngUrl` declarada. 24 testes novos, 339
  testes no core (327+12) + 52 na CLI (49+3), fmt/clippy limpos, `cargo build --release`
  verde. Smoke-test confirma a fiação real, reproduz a mesma limitação de *tool-calling* já
  registrada (MT-61/64/65) — não regressão. Nenhuma dependência nova. **Fecha as duas trilhas
  de web tools da ADR-0025 (MT-65/66).**

- [x] **MT-67** — `crates/core/src/tools/glob.rs` (novo): `GlobTool` busca por padrão de
  nome/caminho (`"**/*.rs"`) via `ignore::overrides::OverrideBuilder` + `WalkBuilder` (mesma
  configuração já estabelecida em `fs.rs`/`repo_map.rs` — `standard_filters(false)` +
  `add_custom_ignore_filename` + `git_ignore` + `require_git(false)`), respeitando
  `.agentryignore`/`.claudeignore`/`context.gitignore.enabled`; resultado capado a 200 itens;
  registrada sempre ativa (sem *toggle* próprio). 5 testes novos, 344 testes no core (339+5) +
  52 na CLI, fmt/clippy limpos, `cargo build --release` verde. Smoke-test reproduz a mesma
  limitação de *tool-calling* já registrada — não regressão, coberta pelos testes. Nenhuma
  dependência nova.

- [x] **MT-68** — `crates/core/src/tools/shell.rs`: `ShellBackgroundTool` (`shell_background`,
  ação `start`/`output`/`stop`), extensão de `ShellPolicy`/MT-13 (mesma política *default-deny*,
  nunca uma paralela). `start` spawna via `tokio::process` sem esperar terminar
  (`kill_on_drop(true)` como rede de segurança, mesmo espírito do `Drop` do `LspClient`);
  `stdout`/`stderr` acumulados em buffer truncado a 50k caracteres (`aplica_teto`, testada
  isoladamente); `output` drena o buffer sem tocar o `Child`; `stop` mata de fato
  (`Child::kill`). 10 testes novos, incluindo verificação real de *spawn*/*kill* via `kill -0`
  (mesmo padrão do `LspClient`, MT-23). 354 testes no core (344+10) + 52 na CLI, fmt/clippy
  limpos, `cargo build --release` verde. Smoke-test reproduz a mesma limitação de
  *tool-calling* já registrada; a tool também fica bloqueada por padrão (*allow-list* vazia,
  mesmo comportamento do `shell_exec`). Nenhuma dependência nova.
- [x] **MT-69** — `docs/usuario/configuracao.md`: seções `tools.webFetch` (as duas condições
  exigidas — *opt-in* + perfil `cloud-ok` — e por quê) e `tools.webSearch`
  (`searxngUrl`/`searxngEgressClass`, sem instância pública pré-configurada). `docs/usuario/uso.md`:
  seção "Ferramentas do agente" (`ask_user`/`glob`/`shell_background`/`web_fetch`/`web_search`).
  `docs/governanca/privacidade-e-egresso.md`: seção "Egresso via ferramentas de web" para o
  público de *compliance* — por que `web_fetch` exige as duas condições (não uma *allowlist*
  de host, já que o destino não é conhecido de antemão); modelo de anonimato como requisito de
  código. Corrigida uma afirmação desatualizada ("os dois são os únicos caminhos de rede") —
  achado de releitura, mesma categoria dos gaps do MT-57/58/62. ADR-0024/0025/0026 promovidas
  a `Accepted`. `mkdocs build --strict` limpo; *anchors* conferidos no HTML. Nenhuma mudança de
  código. **Fecha a Fase 14 inteira (MT-63..69).**

- [x] **Preparação da Fase 15** — ADR-0027 (`Proposed`) decide: `ratatui`+`crossterm` (MIT,
  maturidade verificada via `crates.io/api/v1/crates/ratatui`: 37,9M *downloads* totais/14,2M
  em 90 dias, ativo desde 2023, repositório da própria organização) só em `crates/cli`, nunca
  no `core`; TUI é modo **opt-in** (`--tui`), nunca substitui o REPL de texto; `Session::run_streaming`
  (*callback* já genérico, MT-10) roda numa *task* separada enviando `StreamEvent`s por canal
  ao laço de eventos, **zero mudança no `core`**; `TuiConfirmer`/`TuiPrompter` implementam as
  *traits* já existentes (`Confirmer`/`Prompter`, ADR-0024); *toggle* de permissão `auto`/
  `normal` nunca contorna um `deny`. Fora de escopo deliberado (YAGNI): *widget* de lista de
  tarefas (`agentry` não tem esse conceito no `core` hoje). `docs/roadmap-v0.9.md` detalha os
  7 tickets (MT-70..76), estritamente sequenciais. `mkdocs build --strict` limpo. Nenhuma
  mudança de código.

- [x] **MT-70** — `Cargo.toml` (raiz): `ratatui = { version = "0.30", default-features = false,
  features = ["crossterm"] }` em `[workspace.dependencies]` (evita `all-widgets`/`macros`/
  `palette` do *default* — árvore mínima, ADR-0004); `crates/cli/Cargo.toml`: `ratatui = {
  workspace = true }`. Nova flag `--tui` (`crates/cli/src/main.rs`, `conflicts_with_all =
  ["init", "tarefa"]`) despacha para `crates/cli/src/tui/run()` em vez do REPL de texto; sem a
  flag, caminho existente inalterado byte a byte. `crates/cli/src/tui/mod.rs` (novo): usa
  `ratatui::try_init`/`ratatui::restore` (já instalam o *panic hook* que restaura o terminal
  antes de propagar — dispensa implementar isso na mão) para telas alternativa/modo bruto;
  laço mínimo desenha um `Paragraph` estático (título + "pressione 'q' para sair") e resolve
  cada tecla via `action_for_key` (função pura, testável sem terminal real) — `q` ou `Ctrl+C`
  saem, qualquer outra tecla é ignorada (mesmo padrão de "comando desconhecido não derruba o
  REPL", MT-14); filtra `KeyEventKind::Press` explicitamente (terminais que emitem eventos de
  *release* dobrariam a ação sem esse filtro). 5 testes novos cobrindo `action_for_key`.
  Smoke-test manual do binário `--release` via `tmux` (não há TTY interativo neste ambiente):
  tela renderiza corretamente; `q` e `Ctrl+C` cada um sai com código 0, janela `tmux` fecha
  sozinha (processo não trava, sem *escape sequence* vazando). `cargo build --release` limpo
  com a dependência nova.

- [x] **MT-71** — `crates/cli/src/tui/keybind.rs` (novo): tabela única `DEFINITIONS`
  (ação→tecla *default*+descrição, mesmo espírito de
  `packages/tui/src/config/keybind.ts` do OpenCode); `resolve()` traduz `KeyEvent` para
  `Option<Action>` consultando a tabela (tecla sem ação mapeada é `None`, não erro — mesmo
  padrão do MT-14); `legenda()` monta o rodapé de ajuda direto da tabela (dedupe por ação) — o
  campo `description` fica de fato usado, não morto (clippy `dead_code` pego na primeira
  rodada, corrigido assim em vez de `#[allow]`). `crates/cli/src/tui/mod.rs`: laço de eventos
  passa a chamar `keybind::resolve` em vez de inspecionar `KeyCode` direto (a mudança de escopo
  do ticket: widgets nunca leem tecla bruta); histórico de mensagens **mock** (`MENSAGENS_MOCK`
  — troca pelo histórico real da `Session` fica para o MT-72) fica rolável via
  `Estado::aplicar` (função pura, `ScrollUp`/`ScrollDown` saturam nos limites, `Quit` não
  altera o estado). 9 testes novos (tabela sem conflito de tecla *default*, resolução cobre
  todas as entradas, tecla desconhecida não é erro, evento de *release* ignorado, legenda sem
  duplicata; navegação: topo/fim saturam, scroll para cima/baixo, `Quit` não muda o estado).
  Smoke-test manual do binário `--release` via `tmux`: histórico e rodapé (legenda gerada pela
  tabela) renderizam certo, `j` desce duas linhas visíveis, `q` sai com código 0 e terminal
  restaurado.

- [x] **MT-72** — `crates/cli/src/tui/mod.rs`: `tui::run(session, router)` recebe a mesma
  `Session`/`Router` de `main()` (reaproveitados, não duplicados). `Session::run_streaming`
  roda numa *task* separada (`tokio::spawn`); o *callback* já genérico (MT-10) envia cada
  `StreamEvent` por canal ao laço principal, que faz `tokio::select!` entre eventos de
  terminal (lidos numa *thread* dedicada — `crossterm::event::read` bloqueia) e eventos de
  *stream* — **zero mudança em `crates/core`**. Novo `crates/cli/src/tui/chat.rs`:
  `ChatState` traduz `StreamEvent` em histórico de mensagens (`TextDelta` cresce o turno
  aberto, `MessageEnd` conclui, `marcar_erro` fecha o turno em falha), pura e testável sem
  terminal real. Caixa de entrada de texto real (Enter envia, Backspace edita) substitui o
  histórico mock do MT-71. 19 testes novos (10 em `mod.rs`, 9 em `chat.rs`).

  **Dois achados do smoke-test manual com Ollama real, ambos corrigidos e registrados em
  `docs/decisoes-autonomas.md`:** (1) os atalhos de letra do MT-71 (`q`/`k`/`j`) colidiam com
  a digitação real — tabela revisada para só `Ctrl+C` (sair, convenção universal) e setas
  (rolar); (2) `StderrAuditSink` (`eprintln!` a cada chamada de rede) corrompia visualmente a
  tela alternativa do `crossterm` (`ratatui` não sabe da escrita, não a repõe no próximo
  `draw`) — `NoopAuditSink` (novo) descarta auditoria só sob `--tui`, preservando stderr
  normal no REPL/one-shot; *widget* de log de auditoria fica candidato a ticket futuro
  (YAGNI, não pedido por nenhum ticket da Fase 15). Smoke-test real (llama3.1:8b local):
  mensagem enviada, resposta chega incrementalmente sem corromper a tela, scroll responde
  enquanto o modelo ainda está respondendo, `Ctrl+C` sai limpo com código 0.

- [x] **MT-73** — novo `crates/cli/src/tui/model_picker.rs`: `CandidatoExibicao` + `buscar()`
  (casamento de subsequência simples, sem diferenciar maiúsculas/minúsculas, ordena pelo
  trecho mais compacto — não uma dependência de *fuzzy-matching*, mesma disciplina de
  MT-06/ADR-0007/MT-60 contra dependência nova para problema estreito). Novo
  `Router::route_entry` (`crates/core/src/router/mod.rs`) — acessor de leitura direto aos
  candidatos declarados de uma `task-class`, extensão do escopo de arquivos do ticket
  registrada em `docs/decisoes-autonomas.md` (evita duplicar a lógica de merge
  declarado+sintetizado de `register_declared_task_classes`, MT-56). `keybind.rs` ganha
  `Action::OpenModelPicker` (`Ctrl+P`) e `Action::Cancel` (`Esc`), reinterpretadas pelo laço de
  eventos conforme o modo ativo (a presença de `Estado::seletor: Option<...>` já é a fonte de
  verdade do modo, nenhum campo redundante). `aplicar_selecao` (`tui/mod.rs`) monta o mesmo
  `RuntimeOverride`/`Router::resolve_with_override` já usados por `/model`/`/provider` do REPL
  (reaproveitado, não duplicado) — candidato inexistente nunca é alcançável pela UI (a lista só
  mostra o que `route_entry` devolve); egresso insuficiente continua *fail-closed* (ADR-0002),
  o seletor nunca contorna a checagem. Modal centralizado (`ratatui::widgets::Clear`) com busca
  + lista filtrada; erro de resolução aparece no título da lista. 23 testes novos (21 em
  `crates/cli`, 2 em `crates/core`).

  Smoke-test manual do binário `--release` via `tmux`, dois modelos Ollama declarados
  (`llama3.1:8b`/`qwen2.5:7b`): `Ctrl+P` abre o modal, digitar filtra em tempo real, `Enter`
  confirma e fecha, `Esc` cancela sem selecionar, a mensagem seguinte à seleção prova que a
  rota mudou de verdade (resposta veio do modelo recém-selecionado, "Eu sou Qwen..."). `Ctrl+C`
  sai limpo com código 0.

- [x] **MT-74** — `crates/cli/src/tool_executor.rs`: `PedidoHumano` (novo) — pedido de
  interação humana enviado por canal ao laço de eventos da TUI, já que `Confirmer`/`Prompter`
  rodam dentro da *task* de streaming (MT-72), não no laço que possui o terminal.
  `TuiConfirmer` (implementa `Confirmer`): *toggle* `auto`/`normal` (`AtomicBool` compartilhado
  via `Arc`, alternado por `Ctrl+A`) — em `auto`, aprova sem passar pelo canal nem mostrar
  modal; em `normal`, envia `PedidoHumano::Confirmacao` e aguarda a resposta por `oneshot`.
  Invariante de segurança central do ticket, com teste dedicado nomeado
  (`modo_auto_do_tui_confirmer_nunca_aprova_uma_tool_sob_deny`): a garantia é **estrutural**,
  `RegistryToolExecutor::execute` nem chama `Confirmer::confirm` para `ExecutionOutcome::Denied`
  — nenhum `TuiConfirmer`, em `auto` ou não, jamais participa dessa decisão. Novo
  `crates/cli/src/tui/ask_user.rs`: `TuiPrompter` (implementa `Prompter`, ADR-0024) — mesmo
  canal `PedidoHumano`, sem *toggle* `auto` (a tool `ask_user` existe para perguntar algo ao
  usuário; pular a pergunta contrariaria o propósito da tool). `tui/mod.rs`: `SolicitacaoAtiva`
  (`Confirmacao`/`Pergunta`) com prioridade sobre o seletor de modelo e o chat normal — `Enter`
  aprova/confirma, `Esc` recusa/cancela, digitação livre na caixa de resposta da pergunta.
  Indicador `[auto]` no título da caixa de mensagem quando o *toggle* está ligado. `main.rs`
  constrói `TuiConfirmer`/`TuiPrompter` (em vez de `Interactive*`) só sob `--tui`. 15 testes
  novos.

  Smoke-test manual: indicador `[auto]` alterna corretamente com `Ctrl+A`, terminal não
  corrompe. **Confirmação de tool via LLM real não pôde ser demonstrada de ponta a ponta** —
  mesmo achado documentado em MT-61/64/65/66/67/68: os modelos locais disponíveis
  (`llama3.1:8b`, `qwen2.5:7b`) narram em prosa em vez de emitir uma *tool-call* real, mesmo
  para tools já testadas e funcionais (não é um defeito do código). A fiação
  `TuiConfirmer`→canal→`oneshot` é coberta por testes automatizados que simulam exatamente esse
  *handshake*.

- [x] **MT-75** — novo `crates/cli/src/tui/diff.rs`: `LinhaDiff`
  (`Removida`/`Adicionada`/`Inalterada`) + `diff_linhas()` — diff clássico por subsequência
  comum máxima (LCS, implementação própria via programação dinâmica; mesma disciplina de
  MT-06/ADR-0007/MT-60/MT-73 contra dependência nova para problema estreito). 7 testes cobrindo
  arquivo novo, conteúdo idêntico, adição/remoção no meio, substituição, dois vazios.
  `tool_executor.rs::montar_diff_se_aplicavel` detecta `fs_write`/`fs_edit` pelo nome da tool e
  monta o diff lendo o conteúdo atual do arquivo via `fs::read_to_string` — nenhuma mudança em
  `FsWriteTool`/`FsEditTool`, só uma leitura adicional do lado da prévia; qualquer outra tool
  devolve `None`. `TuiConfirmer` ganha `workspace_root` (só para resolver o *path* relativo).
  `PedidoHumano`/`SolicitacaoAtiva::Confirmacao` carregam o diff pronto; o modal (agora 70×60%,
  maior para caber diffs reais) renderiza linhas `-`/`+` (vermelho/verde) quando presente,
  caindo nos argumentos brutos para qualquer outra tool ou diff vazio. 25 testes novos no
  total, incluindo 5 com arquivos reais em disco.

  Smoke-test manual: TUI renderiza/responde normalmente. Confirmação de `fs_write` via LLM real
  não pôde ser demonstrada de ponta a ponta — mesmo achado documentado em
  MT-61/64/65/66/67/68/74.

- [x] **MT-76 — fecha a Fase 15 inteira (MT-70..76).** `docs/usuario/uso.md` ganha a seção
  "Modo TUI" (`--tui` opt-in, tabela de *keybindings* *default*, nota de que a trilha de
  governança não muda); `--tui` adicionada à tabela de flags de invocação. **ADR-0027
  promovida de `Proposed` para `Accepted`** (`docs/adr/README.md` atualizado).
  `docs/roadmap-longo-prazo.md` marca a Fase 15 `✅ concluída`. `mkdocs build --strict` limpo,
  *anchors* conferidos no HTML gerado. Nenhuma mudança de código — fmt/clippy/test rodados como
  checagem de sanidade (104+356 testes, tudo verde).

- [x] **Preparação da Fase 16** — ADR-0028 (`Proposed`) decide: `rmcp` só com as *features*
  `client`+`transport-child-process` em produção (maturidade verificada via
  `crates.io/api/v1/crates/rmcp`: Apache-2.0, 15,9M *downloads* totais/8,1M em 90 dias,
  repositório oficial `modelcontextprotocol/rust-sdk`, atualizado em 2026-07-08); **v1 só
  suporta servidores MCP locais** (subprocesso, `stdio`) — servidores remotos exigiriam o
  cliente HTTP embutido do `rmcp`, que bypassaria o `Transport` único do projeto (ADR-0001)
  sem `Allowlist`/auditoria, uma questão de *fail-closed* (ADR-0002) explicitamente adiada
  para uma fase dedicada, nunca resolvida via atalho; `rmcp` vive em `crates/core` (mesmo
  lugar de `lsp-types`, ADR-0013); tools MCP entram no `ToolRegistry` com nome prefixado pelo
  servidor (`"<servidor>__<tool>"`), sob o mesmo `PermissionGate` de sempre. `docs/roadmap-v0.10.md`
  detalha os 5 tickets (MT-77..81 — numeração retoma do MT-77, livre desde que o *widget* de
  lista de tarefas foi descartado na preparação da Fase 15). `mkdocs build --strict` limpo.
  Nenhuma mudança de código.

- [x] **MT-77** — `rmcp` adicionado a `crates/core/Cargo.toml` (só *features*
  `client`+`transport-child-process`, `default-features = false`), ainda não usado em código
  Rust nesta ticket (mesmo padrão de MT-55/56: schema antes de consumo). Novo bloco
  `mcpServers` em `agentry.settings.json`: `McpServerSettings { command, args, egressClass }`
  (`crates/core/src/config/mod.rs`) — `command` obrigatório, `args` *default* vazio,
  `egressClass` sempre obrigatória (nunca inferida, ADR-0002), validada como `local-only` já
  em `Settings::from_json_str` (novo `ConfigError::McpServerEgressNotSupported`, rejeitado
  antes do merge entre camadas, nunca conectado). `merge_mcp_servers` substitui a entrada
  inteira por nome (não mescla campo a campo como `taskClasses`). `GENERIC_SETTINGS_EXAMPLE`
  ganha o bloco com um servidor de exemplo usando `echo` como comando inerte — decisão
  registrada em `docs/decisoes-autonomas.md` (`mcpServers` não tem a camada de seleção
  explícita que torna os exemplos reais de `taskClasses` seguros; um comando MCP real como
  `npx` teria efeito colateral assim que um ticket futuro conectar a servidores declarados).
  6 testes novos + teste do exemplo `--init` estendido.

  Smoke-test manual do binário `--release`: `--init` gera o bloco `mcpServers` corretamente
  (JSON válido, `echo` como comando de exemplo); carregar a config gerada e rodar uma tarefa
  real não falha (bloco presente mas inerte, nada ainda o consome).

- [x] **MT-78** — `crates/core/src/mcp/mod.rs` (novo): `McpClient` spawna um servidor MCP via
  `rmcp::transport::child_process::TokioChildProcess` (subprocesso local, `stdio`), completa o
  *handshake* (`ServiceExt::serve`) e lista as tools via `list_all_tools()` (paginação
  resolvida pelo próprio `rmcp`). Nenhum `Drop` manual necessário — o `TokioChildProcess` do
  `rmcp` já mata o subprocesso quando descartado (`ChildWithCleanup::drop`, dentro do próprio
  SDK), validado empiricamente pelo teste de integração. Mesmo modelo de confiança do
  `LspClient` (ADR-0013): subprocesso local, IPC via `pipe`, nunca uma chamada de rede mediada
  pelo `agentry`.

  **Achado técnico registrado em `docs/decisoes-autonomas.md`:** a primeira tentativa de
  fixture de teste (`fake_mcp_server`) usou a *feature* `server` do `rmcp` em
  `[dev-dependencies]` — compilou e passou com `cargo build -p agentry-core --bins --tests`,
  mas falhou em `cargo build --release` real: um alvo `[[bin]]` de `crates/core` (descoberto
  em `src/bin/`) só recebe *features* de `[dependencies]`, nunca as de `[dev-dependencies]`
  (Cargo só estende `dev-dependencies` para alvos `tests`/`examples`, não `[[bin]]`). Resolvido
  implementando o protocolo MCP na mão em `fake_mcp_server.rs` — JSON-RPC 2.0
  *newline-delimited* sobre `stdio` (mais simples que o `Content-Length` do LSP, confirmado no
  código-fonte do `rmcp`), usando os tipos de `rmcp::model` (módulo sem *feature gate*,
  disponível só com `client`) para montar respostas corretas sem hand-typing nomes de campo.
  Isso evita o problema pela raiz sem violar a proibição da própria ADR-0028 contra habilitar
  `server` em produção.

  3 testes de integração (`crates/core/tests/mcp_client.rs`: ciclo de vida completo
  *handshake*→`list_tools`→`shutdown`; `Drop` sem `shutdown()` explícito não deixa processo
  órfão, mesmo teste já existente para `LspClient`; comando inexistente é erro tratado) + 1
  teste unitário. `cargo build --release` limpo — confirma que a superfície de produção do
  `rmcp` continua só `client`+`transport-child-process`, sem vazamento de `server`/`macros`.

- [x] **MT-79** — `crates/core/src/tools/mcp.rs` (novo): cada tool descoberta por
  `McpClient::list_tools()` (MT-78) vira uma `McpTool`, implementando a *trait* `Tool`
  (MT-11) — nome de registro prefixado pelo servidor (`"<servidor>__<tool>"`, ADR-0028) para
  nunca colidir entre servidores; a chamada de verdade ao servidor usa o nome **original**,
  sem prefixo. `execute()` encaminha para `McpClient::call_tool` (novo em
  `crates/core/src/mcp/mod.rs`) — sob o **mesmo** `ToolRegistry`/`PermissionGate` de qualquer
  outra tool, nenhum mecanismo paralelo de confirmação/bloqueio. `register_mcp_tools`
  (`crates/cli/src/main.rs`) conecta a cada servidor de `cfg.mcp_servers` e registra suas
  tools; falha de conexão/listagem de **um** servidor é não-fatal (`eprintln!` + `continue`) —
  problema de ambiente/*runtime*, distinto de config inválida (que já é fatal desde o MT-77),
  e um servidor fora do ar não deve travar a CLI nem os demais servidores configurados.

  **Achado corrigido durante o teste de ponta a ponta (bug, não decisão de projeto — não
  gerou entrada em `docs/decisoes-autonomas.md`):** o teste de execução real
  (`execucao_via_registry_chega_ao_servidor_real_e_devolve_pong`) travou indefinidamente. Causa: o
  `fake_mcp_server` (fixture do MT-78) ignorava em silêncio qualquer método não tratado,
  incluindo `tools/call` — o cliente `rmcp` real não tem *timeout* próprio por requisição e
  ficava esperando para sempre por uma resposta que nunca viria. Corrigido: `fake_mcp_server`
  agora responde `tools/call` de verdade (`CallToolResult::success` com `"pong"`) e devolve um
  erro JSON-RPC `-32601` ("Method not found") para qualquer outro método com `id`, em vez de
  ignorar — fecha essa classe de trava por completo, não só o caso observado.

  366 testes em `agentry-core` (+8: 3 unitários em `tools/mcp.rs`, 5 de integração em
  `tests/mcp_tool.rs`) e 104 em `agentry`, `cargo build --release` limpo. Smoke-test manual do
  binário `--release`: `--init` gera `mcpServers.exemplo` (comando `echo`, não fala MCP de
  verdade); rodar uma tarefa real com essa config presente imprime
  `erro ao conectar ao servidor MCP 'exemplo': ...` em `stderr` e segue normalmente até
  completar a tarefa — confirma que uma falha de conexão de um servidor nunca trava a CLI.

- [x] **MT-80** — `McpClient::start_from_settings` (novo, `crates/core/src/mcp/mod.rs`):
  defesa em profundidade além do `Settings::from_json_str` (MT-77) — checa
  `egress_class == LocalOnly` **antes** de tocar em `Command`/`TokioChildProcess`, devolvendo
  `McpError::EgressNotSupported` sem spawnar nenhum subprocesso caso contrário. Garante que
  nenhum caminho de código, inclusive um `McpServerSettings` montado direto em Rust sem passar
  pelo parser, chega a conectar um servidor com classe de egresso diferente de `local-only`.
  `register_mcp_tools` (`crates/cli/src/main.rs`) passa a usar esse ponto de entrada em vez de
  extrair `command`/`args` manualmente. 2 testes novos (egresso remoto rejeitado sem spawnar;
  `local-only` com comando inexistente ainda falha ao spawnar, comportamento preservado). 368
  testes em `agentry-core`, 104 em `agentry`, `cargo build --release` limpo. Nenhuma mudança de
  comportamento observável — o `Settings::from_json_str` já bloqueava esse caso antes de
  chegar aqui.

- [x] **MT-81** — documentação (usuário + governança), **fecha a Fase 16 inteira**.
  `docs/usuario/configuracao.md` ganha a seção `mcpServers` (schema, exemplo real com
  `npx`/`@modelcontextprotocol/server-filesystem`, nome de tool sempre prefixado pelo
  servidor, servidor fora do ar não trava a CLI). `docs/usuario/uso.md` ganha uma nota curta
  em "Ferramentas do agente". `docs/governanca/privacidade-e-egresso.md` ganha a seção "MCP e
  egresso" para o público de *compliance*. **ADR-0028 promovida de `Proposed` para
  `Accepted`** (`docs/adr/README.md` atualizado); `docs/roadmap-longo-prazo.md` marca a
  Fase 16 `✅ concluída`. `mkdocs build --strict` limpo, *anchors* conferidos no HTML gerado.
  Nenhuma mudança de código.

- [x] **Preparação da Fase 17** — decisão de qual das cinco frentes de "segunda onda"
  preparar primeiro registrada em `docs/decisoes-autonomas.md` (2026-07-16): **custo/uso
  visível** (as outras quatro — memória entre sessões, subagentes/orquestração, multimodal,
  checkpoints/*undo* — passam a viver na Fase 18+, sem ordem decidida entre elas ainda).
  **ADR-0029** (`Proposed`, `docs/adr/0029-uso-de-tokens-visivel-na-sessao.md`): `Session`
  acumula `Usage` por sessão; exposto via resumo em `stderr` no modo *one-shot*, comando
  `/usage` no REPL, rodapé da TUI; sem persistência entre sessões; custo em dinheiro fora de
  escopo. `docs/roadmap-v0.11.md` detalha MT-82..85. `docs/adr/README.md`/`mkdocs.yml`
  atualizados (novo ADR + novo roadmap na `nav`). `mkdocs build --strict` limpo. Nenhuma
  mudança de código — esta iteração só prepara a fase (skill `adr-writer`/
  `micro-ticket-planner`), não implementa.

- [x] **MT-82** — `Session` (`crates/core/src/session/mod.rs`) ganha `usage_total` (`Usage`),
  somado a cada turno concluído (`after_response`) e a cada `Session::compact` — distinto do
  `consumed` local já existente (só vive durante uma chamada a `run`/`run_streaming`, decide
  estouro de `TokenBudget`). Novo método `usage_total()` expõe o total acumulado; `compact()`
  também soma seu próprio uso (decisão tomada durante a implementação, ver
  `docs/CURRENT-STATE.md` acima para a justificativa). 4 testes novos. 372 testes em
  `agentry-core` (+4), 104 em `agentry`, `cargo build --release` limpo. Nenhuma mudança de
  comportamento observável da CLI ainda.

**Em andamento:** nada pendente — árvore de trabalho limpa, tudo commitado. **Fase 16
concluída inteira (MT-77..81)**; **Fase 17 preparada** (ADR-0029 `Proposed`,
`docs/roadmap-v0.11.md`, MT-82..85); **MT-82 concluído**.

**Próximo passo:** **MT-83** (`docs/roadmap-v0.11.md`, `crates/cli/src/main.rs`,
`crates/cli/src/repl.rs`) — exposição do uso de tokens no modo *one-shot* (resumo em
`stderr` ao final da tarefa) e comando `/usage` no REPL (mesmo padrão de `/compact`).
Segundo ticket de implementação da Fase 17. Outros itens em aberto, sem ticket: deploy do
site MkDocs (GitHub Pages) — decisão explícita do usuário de não fazer ainda; CI multi-SO
ainda não observado verde (falta um push que dispare a matriz); backlog independente do
`ai-coding-agent-profiles` (ADRs 0001-0005 — RTK/OKF pendentes de reanálise de maturidade,
perfis base+overlay/skills executáveis/config de serviços pendentes de validação de
implementação).

## Impedimentos de ambiente (não são bugs do código)

- **`protoc` não vem pré-instalado por padrão** (nem, presumivelmente, nos runners padrão do GitHub Actions) — exigido pelo build script de `lance-encoding` (transitiva do `lancedb`, MT-27). CI já corrigido; ambientes de desenvolvimento locais precisam instalar `protobuf-compiler` (Debian/Ubuntu), `protobuf` (Homebrew) ou equivalente antes de rodar `cargo build`/`cargo test` neste crate — ver `docs/testing.md`. **Nesta máquina de desenvolvimento, já resolvido**: `protobuf-compiler` instalado via `apt` pelo usuário (precisa de `sudo` — funciona só com terminal interativo; o agente não deve tentar rodar `sudo` sozinho, sempre pedir para o usuário rodar). Um binário `protoc` *standalone* baixado manualmente mais cedo na sessão (`~/.local/bin/protoc`, contornando a falta de `sudo` interativo) foi removido para não sombrear o `/usr/bin/protoc` do pacote no `PATH` — `cargo build`/`test`/`clippy` voltaram a funcionar sem nenhuma variável de ambiente extra (`PROTOC`/`PROTOC_INCLUDE`).

## Impedimentos abertos

- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido pela empresa; adapter adiado.
- **CI multi-SO ainda não observado verde:** a matriz do ADR-0005 (`2feed85`) precisa de um push ao GitHub para confirmar Windows/macOS verdes.
- **Verificação de "processo não órfão" do MT-23 é Unix-only de fato:** `processo_existe` (`crates/core/tests/lsp_client.rs`) usa `kill -0`; no branch `#[cfg(not(unix))]` sempre devolve `false`, então em Windows os testes `ciclo_de_vida_completo_start_initialize_shutdown`/`drop_sem_shutdown_explicito_nao_deixa_processo_orfao` passam vacuamente (não verificam nada de verdade) — o `Child::wait()`/`kill()` internos do `LspClient` continuam corretos, só falta uma verificação real de ausência de processo em Windows (ex.: via `tasklist`) quando a matriz de CI (ADR-0005) rodar de verdade.

---

## Histórico (mais recente no topo)

| Data | Commit | Resumo | MT |
|------|--------|--------|----|
| 2026-07-15 | `7a68941` | MT-78: cliente MCP -- conecta, handshake, descobre tools | MT-78 |
| 2026-07-15 | `9fcbaaf` | MT-77: adoção rmcp + schema mcpServers na configuração | MT-77 |
| 2026-07-15 | `82c4785` | ADR-0028: cliente MCP via rmcp (autorizado pelo mantenedor); prepara a Fase 16 | — |
| 2026-07-15 | `eeae714` | MT-76: documentação (usuário) — ADR-0027 -> Accepted (fecha a Fase 15) | MT-76 |
| 2026-07-15 | `ba11489` | MT-75: visualizador de diff (modal) para fs_write/fs_edit sob ask | MT-75 |
| 2026-07-15 | `b4e9935` | MT-74: widgets de permissão (TuiConfirmer) e pergunta (TuiPrompter) | MT-74 |
| 2026-07-15 | `7d3da53` | MT-73: seletor de modelo/provider com busca difusa (Ctrl+P) | MT-73 |
| 2026-07-15 | `04db36e` | MT-72: view de chat com streaming real (integração com Session/Router) | MT-72 |
| 2026-07-15 | `fb39a2a` | MT-71: tabela de keybindings (mapa único) + navegação básica | MT-71 |
| 2026-07-15 | `5b18d80` | MT-70: scaffold ratatui/crossterm + flag --tui + laço de eventos mínimo | MT-70 |
| 2026-07-15 | `2e3916a` | ADR-0027: TUI via ratatui (autorizada pelo mantenedor); prepara a Fase 15 | — |
| 2026-07-15 | `c87d458` | docs(handoff): fecha a Fase 14 inteira; loop autônomo parado (dependência nova exigida) | — |
| 2026-07-15 | `5304914` | docs(roadmap): marca MT-69 concluído; fecha a Fase 14 inteira | — |
| 2026-07-15 | `e375095` | MT-69: documentação tools essenciais + ADR-0024/0025/0026 -> Accepted | MT-69 |
| 2026-07-15 | `4e3f5ee` | MT-68: tool shell_background -- start/output/stop (ADR-0026) | MT-68 |
| 2026-07-15 | `1e666ca` | MT-67: tool glob (ADR-0026) | MT-67 |
| 2026-07-15 | `b23b184` | MT-66: tool web_search via SearXNG configurável (ADR-0025) | MT-66 |
| 2026-07-15 | `733fa63` | MT-65: tool web_fetch + coringa ANY_HOST na Allowlist (ADR-0025) | MT-65 |
| 2026-07-15 | `ebfdb5d` | MT-64: InteractivePrompter + registro real da tool ask_user (ADR-0024) | MT-64 |
| 2026-07-15 | `721b2bd` | MT-63: trait Prompter + tool ask_user no core (ADR-0024) | MT-63 |
| 2026-07-15 | `a0da724` | ADR-0024/0025/0026: tools essenciais (AskUser, web/SearXNG, Glob+shell background); prepara a Fase 14 | — |
| 2026-07-15 | `24f2bdd` | MT-62: documentação AGENTS.md/skills; ADR-0003/0023 -> Accepted (fecha a Fase 13) | MT-62 |
| 2026-07-15 | `38f8bcb` | MT-61: tool skill — carrega o corpo completo sob demanda (ADR-0023) | MT-61 |
| 2026-07-15 | `af2c3d8` | MT-60: descoberta de SKILL.md + lista compacta no system prompt (ADR-0023) | MT-60 |
| 2026-07-15 | `eb9c518` | MT-59: loader de AGENTS.md/CLAUDE.md; injeção como mensagem de sistema (ADR-0023) | MT-59 |
| 2026-07-15 | `384899b` | ADR-0023: memória de projeto (AGENTS.md + Skills); prepara a Fase 13 (MT-59..62) | — |
| 2026-07-15 | `5457f18` | MT-58: documentação do site — taskClasses + convenção autoexplicativa (fecha a Fase 12) | MT-58 |
| 2026-07-15 | `efca5dd` | MT-57: exemplo --init enriquecido (taskClasses + auditoria de blocos, ADR-0022) | MT-57 |
| 2026-07-15 | `45d56db` | MT-56: CLI consome task-classes reais + --task-class/`/task-class` (ADR-0021) | MT-56 |
| 2026-07-15 | `8f0ba55` | MT-55: schema taskClasses em Config (ADR-0021) | MT-55 |
| 2026-07-15 | `a13eb98` | MT-54: documentação do site — context.gitignore + .agentryignore (fecha a Fase 11) | MT-54 |
| 2026-07-15 | `6151e26` | test: cobre o schema context.gitignore em config/mod.rs (MT-53) | MT-53 |
| 2026-07-15 | `3bbd934` | MT-53: respeito opcional a .gitignore (ADR-0020 §3) | MT-53 |
| 2026-07-15 | `d742265` | MT-52: renomeia para .agentryignore com fallback de compatibilidade | MT-52 |
| 2026-07-15 | `c8cf8a8` | chore(loop): infraestrutura de execução autônoma do roadmap | — |
| 2026-07-14 | `de46792` | docs(roadmap): planejamento de longo prazo (Fases 11–17+); ADR-0021/0022 | — |
| 2026-07-14 | `3f908bf` | docs(roadmap): marca MT-51 concluído; Fase 10 completa | — |
| 2026-07-14 | `9c5e495` | MT-51: documentação do site reflete o LiteLLM (fecha a Fase 10) | MT-51 |
| 2026-07-14 | `95406c1` | docs: exemplo gerado por --init ganha _comentario por bloco | — |
| 2026-07-14 | `ed0988c` | fix: agentry.settings.json gerado por --init mostra todo campo configurável | — |
| 2026-07-14 | `0a0897a` | build: Makefile para cross-compile Windows + empacotamento em zip | — |
| 2026-07-14 | `4aee255` | MT-50: flag --provider e comando /provider (ADR-0014/MT-49) | MT-50 |
| 2026-07-14 | `3b851cb` | ADR-0020: .agentryignore (renomeando .claudeignore) + gitignore opcional | — |
| 2026-07-14 | `a714182` | MT-49: consumo real do provider LiteLLM na CLI (ADR-0006) | MT-49 |
| 2026-07-14 | `ac28251` | MT-48: schema providers.litellm em Settings/Config (ADR-0006) | MT-48 |
| 2026-07-14 | `b18a65c` | docs(roadmap): conexão configurável com LiteLLM (Fase 10, roadmap-v0.5.md) | — |
| 2026-07-14 | `8a4be44` | docs: site MkDocs com três trilhas (usuário, governança/compliance, dev) | — |
| 2026-07-14 | `5b8913a` | docs(adr): housekeeping de status — 13 ADRs promovidos a Accepted | — |
| 2026-07-14 | `f60e5be` | MT-47: buffer condicional em run_streaming quando há guardrails de saída; fecha a Fase 9 | MT-47 |
| 2026-07-14 | `ee33219` | MT-46: consumo real do Guardrail Gate na CLI; corrige Settings::from_file nunca lido em main() | MT-46 |
| 2026-07-13 | `794a3cc` | docs(roadmap): adiciona MT-47 (buffer condicional em run_streaming) | — |
| 2026-07-13 | `6d46a51` | MT-45: Session aplica o Guardrail Gate na entrada e na saída | MT-45 |
| 2026-07-13 | `3039554` | MT-44: GuardrailSettings — schema mínimo em Config | MT-44 |
| 2026-07-13 | `7627c53` | MT-43: módulo guardrail — tipos, correspondência, auditoria | MT-43 |
| 2026-07-13 | `53c4c6a` | docs(roadmap): ADR-0007 quebrada em MT-43..46 (Fase 9, roadmap-v0.4.md) | — |
| 2026-07-13 | `a7db76d` | ADR-0007: fecha o schema mínimo do Guardrail Gate | — |
| 2026-07-13 | `4f54169` | MT-42: --init --profile — bootstrap via rede, referência pinada; fecha a Fase 8 | MT-42 |
| 2026-07-13 | `3a2075b` | MT-41: --init/`/init` sem --profile — bootstrap local, zero rede | MT-41 |
| 2026-07-13 | `362696f` | docs(roadmap): ADR-0019 quebrada em MT-41/42 (Fase 8, roadmap-v0.3.md) | — |
| 2026-07-13 | `4e24a52` | ADR-0019: bootstrap de agentry.settings.json via --init/`/init` | — |
| 2026-07-13 | `35362f6` | MT-40: consome as 4 flags de contexto/provider na CLI real; fecha a Fase 7 | MT-40 |
| 2026-07-13 | `b3357a6` | MT-39: Settings::from_file — carrega agentry.settings.json (ADR-0018) | MT-39 |
| 2026-07-12 | `fb99c02` | fix: .agentry/.gitignore não podia se autoignorar | — |
| 2026-07-12 | `be4f000` | ADR-0018 (settings-schema) + emenda ADR-0017; roadmap-v0.2.md (Fase 7) | — |
| 2026-07-12 | `4bd6ee6` | fix: audit log em stderr — Display compacto em vez de dump de Debug | — |
| 2026-07-10 | `0791411` | docs: README real + teste de usabilidade (primeira config/uso) | — |
| 2026-07-10 | `a4f1efd` | docs(testing): guia de testes Linux/Windows + scripts de automação | — |
| 2026-07-10 | `254b139` | MT-35: Reviewer integrado ao agent loop; ADR-0015 completa, fecha o roadmap v0.1 | MT-35 |
| 2026-07-10 | `edffd28` | MT-34: Reviewer — auditoria semântica via task-class (ADR-0015) | MT-34 |
| 2026-07-10 | `ef9caf5` | MT-30: tool code_search; fecha o RAG semântico (ADR-0011) e a Fase 6 inteira | MT-30 |
| 2026-07-10 | `38c18e1` | MT-29: indexação incremental (manifesto hash+chunks) (ADR-0011) | MT-29 |
| 2026-07-10 | `33ed4c0` | MT-38: diretório de estado local (.agentry/) + auto-exclusão do git; ADR-0017 completa | MT-38 |
| 2026-07-10 | `6968663` | MT-28: busca híbrida (RRF) + reranking via LlmProvider::chat (ADR-0011) | MT-28 |
| 2026-07-10 | `49e79f9` | ADR-0017: diretório de estado local (.agentry/) para memória/histórico/índices; MT-38 adicionado | — |
| 2026-07-10 | `a518c9e` | MT-27: índice semântico (embeddings + lancedb) sobre os chunks (ADR-0011) | MT-27 |
| 2026-07-09 | `93f7ccd` | MT-26: índice lexical (tantivy/BM25) sobre os chunks (ADR-0011) | MT-26 |
| 2026-07-09 | `00b9460` | MT-25: chunking AST-aware para RAG (ADR-0011) | MT-25 |
| 2026-07-09 | `7b3777d` | MT-24: tools lsp_hover/lsp_definition; fecha a trilha LSP (ADR-0013) | MT-24 |
| 2026-07-09 | `39ffd55` | MT-23: cliente LSP mínimo, spawn + JSON-RPC stdio (ADR-0013) | MT-23 |
| 2026-07-09 | `889d4e8` | MT-22: saída estruturada para tool-calling no Ollama (ADR-0012) | MT-22 |
| 2026-07-09 | `2d11628` | MT-21: tool repo_map exposta ao agent loop; fecha a trilha repo-map (ADR-0010) | MT-21 |
| 2026-07-09 | `6ad4f6d` | MT-20: ranking de relevância estilo PageRank (ADR-0010) | MT-20 |
| 2026-07-09 | `5b7a48e` | MT-19: grafo de referências entre arquivos (ADR-0010) | MT-19 |
| 2026-07-09 | `06ea5d8` | MT-18: extração de símbolos AST-aware via tree-sitter (ADR-0010) | MT-18 |
| 2026-07-09 | `c07cf81` | MT-17: timeout adaptativo + keep_alive (ADR-0009) | MT-17 |
| 2026-07-09 | `f932e41` | MT-37: comando /compact no REPL; ADR-0016 totalmente implementado | MT-37 |
| 2026-07-09 | `7e217c4` | MT-36: Session::compact (mecanismo de compactação de histórico) | MT-36 |
| 2026-07-09 | `80f7a81` | ADR-0016: compactação de histórico de sessão; MT-36/37 adicionados | — |
| 2026-07-09 | `16bbe0b` | CI: scan de segredos (gitleaks) no pipeline | — |
| 2026-07-08 | `f62851d` | MT-16: adapter Anthropic (Messages API); fecha a Fase 5 | MT-16 |
| 2026-07-08 | `0951111` | MT-15: adapter OpenAI-compatible (vLLM/OpenRouter/LiteLLM); Transport ganha with_api_key | MT-15 |
| 2026-07-08 | `c226f3f` | MT-14: CLI one-shot + REPL com override de parâmetros; fecha a Fase 4 | MT-14 |
| 2026-07-08 | `3244dbc` | MT-33: RuntimeOverride no Router; ADR-0014 totalmente implementado | MT-33 |
| 2026-07-08 | `0decd45` | MT-32: reasoning/thinking como parâmetro de chamada (campo think no Ollama) | MT-32 |
| 2026-07-08 | `39211bc` | MT-13: tool de shell default-deny (ShellPolicy + CommandRunner como gancho de sandbox) | MT-13 |
| 2026-07-08 | `814ba2f` | MT-12: tools de filesystem read/write/edit/search (crate `ignore` p/ .claudeignore) | MT-12 |
| 2026-07-08 | `5b5ee37` | ADR-0015: Reviewer (auditoria semântica por task-class); MT-34/35 adicionados | — |
| 2026-07-08 | `a31382a` | MT-31: Session consome CallPreset via ResolvedRoute (fecha lacuna do ADR-0008) | MT-31 |
| 2026-07-08 | `4775f33` | ADR-0014: override runtime de parâmetros (reasoning + model/temperature/etc.); MT-31..MT-33 | — |
| 2026-07-08 | `70c0470` | ADR-0010..0013: RAG/repo-map/saída estruturada/LSP-grounding; Fase 6 + MT-18..MT-30 | — |
| 2026-07-07 | `cf21f6f` | MT-11: Tool Registry + gate de permissão allow\|ask\|deny + testes | MT-11 |
| 2026-07-07 | `ef69785` | ADR-0009: timeout adaptativo + keep_alive para troca de modelo local; MT-17 adicionado | — |
| 2026-07-07 | `cdd4fc6` | MT-10: agent loop ReAct mínimo (run + run_streaming); abre a Fase 4 | MT-10 |
| 2026-07-07 | `e23390b` | MT-09: Router/Policy Engine (task-class → provider/modelo/classe); fecha a Fase 3 | MT-09 |
| 2026-07-07 | `3ae5054` | ADR-0007/0008: guardrails de conteúdo + presets de chamada por task-class | — |
| 2026-07-07 | `4d961eb` | MT-08: adapter Ollama (chat+stream) sobre o Transporte; abre a Fase 3 | MT-08 |
| 2026-07-07 | `1723c31` | MT-07: transporte HTTP único sobre reqwest; fecha a Fase 2 (egresso) | MT-07 |
| 2026-07-07 | `9a89679` | MT-06: audit log de egresso + redação de segredos (sem regex) + testes | MT-06 |
| 2026-07-07 | `a2120b7` | MT-05: allowlist de endpoints + `rank`/`permits` de `EgressClass` + testes | MT-05 |
| 2026-07-07 | `b63fe6b` | MT-04: config em camadas + classe de privacidade fail-closed + testes | MT-04 |
| 2026-07-06 | `26b370e` | MT-03: `trait LlmProvider` + `MockProvider` roteirizado + testes | MT-03 |
| 2026-07-06 | `f03c1ef` | MT-02: tipos de domínio de mensagens/LLM + testes round-trip serde | MT-02 |
| 2026-07-06 | `ab69934` | ADR-0006: LiteLLM via adapter OpenAI-compatible; roadmap MT-15 e arquitetura atualizados | — |
| 2026-07-06 | `2feed85` | ADR-0005 fechado: matriz de CI em 3 SOs + `.gitattributes` (LF) | — |
| 2026-06-19 | `ba74200` | MT-01: scaffold do workspace Cargo + CI + lint + `git init`; validação local verde | MT-01 |
| 2026-06-19 | — | Planejamento: ADR-0001..0004, interop v1, `architecture.md`, `roadmap-v0.1.md` | — |
