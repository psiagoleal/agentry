<!-- Caminho relativo: docs/roadmap-v0.9.md -->

# Roadmap v0.9 — Micro-tickets

O roadmap v0.8 (`docs/roadmap-v0.8.md`) cobre a Fase 14 (tools essenciais, ADR-0024/0025/0026,
**concluída**). Este documento detalha a **Fase 15** do roadmap de longo prazo
(`docs/roadmap-longo-prazo.md`): TUI via `ratatui` (ADR-0027) — modo interativo opt-in, sem
substituir o REPL de texto existente.

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), skill
`micro-ticket-planner` para granularidade. **Uma dependência nova nesta fase**, já vetada e
autorizada pelo mantenedor: `ratatui` (com a *feature* `crossterm`) — ver ADR-0027 para a
verificação de maturidade completa. Nenhuma outra dependência nova.

---

## Fase 15 — TUI via `ratatui` (ADR-0027)

### MT-70: *Scaffold* `ratatui`/`crossterm` + laço de eventos mínimo
- **Objetivo:** `ratatui` (com a *feature* `crossterm`) adicionada a `crates/cli/Cargo.toml`
  (`[workspace.dependencies]` no `Cargo.toml` raiz, como as demais). Nova flag `--tui`
  (`crates/cli/src/main.rs`) — quando presente, entra num novo módulo `crates/cli/src/tui/mod.rs`
  em vez do REPL de texto (`repl::run_repl`); sem a flag, **nenhuma mudança** no caminho
  existente. Laço de eventos mínimo: inicializa o terminal (modo alternativo, `crossterm`),
  desenha uma tela estática (ex.: título + instrução "pressione 'q' para sair"), processa
  eventos de teclado só o suficiente para sair limpo (`q`/`Ctrl+C`), restaura o terminal ao
  encerrar (inclusive em caso de *panic* — `panic::set_hook` restaurando o terminal antes de
  propagar, mesmo padrão recomendado pela documentação do `ratatui` para não deixar o
  terminal do usuário quebrado).
- **Arquivos no escopo:** `Cargo.toml` (raiz), `crates/cli/Cargo.toml`,
  `crates/cli/src/main.rs`, `crates/cli/src/tui/mod.rs` (novo).
- **Critério de aceite:** `cargo build --release` inclui a nova dependência sem erro;
  smoke-test manual do binário real (`agentry --tui`, tecla `q` sai limpo, terminal restaurado
  — sem sequências de escape vazando para o shell depois); teste automatizado cobre só a
  lógica pura extraível (ex.: mapeamento de tecla → ação de saída), não o laço de eventos
  real (não há como automatizar E/S de terminal real em CI sem uma dependência de teste nova
  — fora de escopo).
- **Fora de escopo:** qualquer integração com `Session`/agente (MT-72); keybinds
  configuráveis (MT-71 só a tabela *default*, customização por usuário é ticket futuro).
- **Depende de:** ADR-0027.

### MT-71: Tabela de *keybindings* (mapa único) + navegação básica
- **Objetivo:** `crates/cli/src/tui/keybind.rs` (novo): uma única tabela `Definitions` (nome
  de ação → tecla *default* + descrição — ex.: `quit` → `q`, `scroll_up` → `↑`/`k`), mesmo
  espírito de `packages/tui/src/config/keybind.ts` do OpenCode (referência de UX, não de
  código). Widgets consultam a ação pelo nome (`Action::Quit`, `Action::ScrollUp`, ...), nunca
  a tecla bruta diretamente — desacopla o mapeamento de tecla da lógica de cada widget. Laço
  de eventos do MT-70 passa a rolar um histórico de mensagens (mesmo que ainda estático/mock,
  sem o agente real) — prova a navegação funcionando antes de acoplar o *streaming* real.
- **Arquivos no escopo:** `crates/cli/src/tui/keybind.rs` (novo), `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** testes — tabela de *keybindings* não tem duas ações para a mesma
  tecla *default* (conflito detectado, não silencioso); resolução de tecla → ação funciona
  para todas as entradas da tabela; tecla sem ação mapeada não é erro (ignorada, mesmo padrão
  de "comando desconhecido não derruba o REPL", MT-14).
- **Fora de escopo:** customização de keybind pelo usuário (`agentry.settings.json` ou
  arquivo próprio) — só a tabela *default* nesta fase.
- **Depende de:** MT-70.

### MT-72: View de chat com *streaming* real (integração com `Session`)
- **Objetivo:** conecta a TUI à `Session`/`Router` reais (mesma construção já feita por
  `main()` para o REPL — reaproveitada, não duplicada). `Session::run_streaming` roda numa
  *task* separada (`tokio::spawn`); o *callback* (`FnMut(&StreamEvent)`, já genérico desde o
  MT-10) envia cada evento (já `Clone`) por um canal (`tokio::sync::mpsc`) de volta ao laço de
  eventos principal, que faz `tokio::select!` entre eventos de terminal (`crossterm::event`) e
  eventos de *stream* do canal — renderizando o texto incrementalmente na view de chat, mesmo
  resultado observável do REPL de texto (MT-14), só que numa área rolável com o restante da
  UI viva ao redor. **Nenhuma mudança em `crates/core`** — a API de *callback* já era
  genérica o suficiente.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`, novo `crates/cli/src/tui/chat.rs`.
- **Critério de aceite:** testes — a função que traduz `StreamEvent` → atualização do estado
  de renderização é pura e testável sem terminal real (dado um `StreamEvent::TextDelta`, o
  texto acumulado cresce; `MessageEnd` marca o turno como concluído); smoke-test manual do
  binário real (`agentry --tui`, mandar uma mensagem, ver o texto chegando incrementalmente,
  terminal continua respondendo a `resize`/`scroll` enquanto o modelo ainda está respondendo).
- **Fora de escopo:** confirmação de tool via widget (MT-74 — nesta ticket, tool-calls sob
  `ask` ainda usam o `Confirmer` de texto simples, se necessário só para não travar; widget de
  verdade é o próximo ticket); seletor de modelo (MT-73).
- **Depende de:** MT-71.

### MT-73: Seletor de modelo/*provider* (busca difusa)
- **Objetivo:** widget de seleção com busca difusa (*fuzzy*) sobre os candidatos já declarados
  na `task-class` ativa (`RouteEntry.candidates`, já resolvido) — evolução do `/model
  <nome>`/`/provider <nome>` de texto exato (MT-14/50). **Nunca** introduz um candidato novo —
  mesma disciplina de override já vetada pelo ADR-0014: só escolhe entre o que já está
  registrado na `RouteEntry`. Atalho de teclado (tabela do MT-71) abre o seletor.
- **Arquivos no escopo:** novo `crates/cli/src/tui/model_picker.rs`, `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** testes — a função de busca difusa (filtra/ordena candidatos por
  substring aproximada do texto digitado) é pura e testável sem terminal real; selecionar um
  candidato aplica o mesmo `RuntimeOverride`/`resolve_with_override` já usado pelos comandos
  `/model`/`/provider` de texto (reaproveitado, não duplicado); candidato inexistente não é
  um estado alcançável pela UI (a lista só mostra o que já está declarado).
- **Fora de escopo:** categorização "Favoritos/Recentes" do OpenCode (metadado que agentry não
  rastreia hoje) — lista simples de candidatos declarados, sem histórico de uso.
- **Depende de:** MT-72.

### MT-74: Widgets de permissão (`TuiConfirmer`) e pergunta (`TuiPrompter`)
- **Objetivo:** `TuiConfirmer` (implementa `Confirmer`, `crates/cli/src/tool_executor.rs`,
  MT-14) e `TuiPrompter` (implementa `Prompter`, `crates/core/src/tools/ask_user.rs`, MT-63) —
  widgets modais em vez de `print!`/`read_line` síncronos. *Toggle* de dois estados
  (`auto`/`normal`, inspirado em `permission.tsx` do OpenCode) muda só a UX de **confirmar
  mais rápido** um `ask` — **nunca** contorna um `deny` do `PermissionGate` (MT-11); essa é a
  invariante de segurança central desta ticket, não uma escolha de design.
- **Arquivos no escopo:** `crates/cli/src/tool_executor.rs` (novo `TuiConfirmer`), novo
  `crates/cli/src/tui/ask_user.rs` (`TuiPrompter`), `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** testes — `TuiConfirmer`/`TuiPrompter` implementam as *traits*
  corretamente (teste de integração como os já existentes para `InteractiveConfirmer`, com um
  dublê de entrada de teclado); **teste explícito e nomeado provando que o modo `auto` nunca
  aprova uma tool sob `deny`** — só afeta `ask` (a invariante de segurança desta ADR, com um
  teste dedicado a ela, não só incidental).
- **Fora de escopo:** paleta de comandos, "stash" de mensagem — não fazem parte desta ticket.
- **Depende de:** MT-72 (reaproveita o laço de eventos/`tokio::select!` já funcionando).

### MT-75: Visualizador de diff (modal)
- **Objetivo:** para confirmações de `fs_write`/`fs_edit` (MT-12) sob `ask`, o `TuiConfirmer`
  (MT-74) mostra um modal com o diff de verdade (linhas removidas/adicionadas) em vez dos
  argumentos brutos da *tool-call*. **Nenhuma mudança** em `FsWriteTool`/`FsEditTool` — os
  argumentos que essas tools já recebem (caminho + conteúdo novo, ou padrão de substituição)
  já são suficientes para montar o diff do lado da CLI, lendo o conteúdo atual do arquivo (via
  `fs::read_to_string`, sem tocar a lógica de escrita em si).
- **Arquivos no escopo:** novo `crates/cli/src/tui/diff.rs`, `crates/cli/src/tool_executor.rs`
  (`TuiConfirmer` passa a detectar `fs_write`/`fs_edit` e montar o diff antes de exibir).
- **Critério de aceite:** testes — função de geração de diff (linha a linha, conteúdo antigo
  vs. novo) é pura e testável sem terminal real, cobrindo adição/remoção/arquivo novo (sem
  conteúdo antigo); confirmação de qualquer outra tool (que não `fs_write`/`fs_edit`) continua
  mostrando o resumo genérico anterior, sem tentar montar um diff que não faz sentido para ela.
- **Fora de escopo:** diff *side-by-side*/navegação por *hunk* (só visualização linear
  unificada nesta ticket — suficiente para o caso de uso, generalização fica para quando
  houver demanda real).
- **Depende de:** MT-74.

### MT-76: Documentação (usuário)
- **Objetivo:** `docs/usuario/uso.md` ganha a seção "Modo TUI" (`--tui`, *keybindings*
  *default*, como sair) — trilha de governança **não** muda (a TUI não introduz nenhum
  caminho de rede/egresso novo, é só apresentação sobre a mesma `Session`/`Router` já
  documentados). ADR-0027 promovida a `Accepted` (MT-70..75 concluídos).
- **Arquivos no escopo:** `docs/usuario/uso.md`, `docs/adr/0027-tui-via-ratatui.md` (status),
  `docs/adr/README.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo; releitura confirmando que nada na
  trilha de usuário ficou desatualizado.
- **Fora de escopo:** trilha de governança (nenhuma afirmação de egresso muda).
- **Depende de:** MT-70..75 (todos).

---

## Sequência crítica

```
MT-70 → MT-71 → MT-72 → MT-73 → MT-74 → MT-75 → MT-76
```

Estritamente sequencial, diferente das fases anteriores — cada ticket constrói sobre o laço
de eventos/integração do anterior (o *scaffold* precisa existir antes das *keybindings*, que
precisam existir antes da view de chat, e assim por diante).
