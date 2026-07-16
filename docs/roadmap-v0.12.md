<!-- Caminho relativo: docs/roadmap-v0.12.md -->

# Roadmap v0.12 — Micro-tickets

O roadmap v0.11 (`docs/roadmap-v0.11.md`) cobre a Fase 17 (uso de tokens visível, ADR-0029,
**concluída**). Este documento detalha a **Fase 18** do roadmap de longo prazo
(`docs/roadmap-longo-prazo.md`): checkpoints e *undo* de mudanças de arquivo (ADR-0030) —
segunda das cinco frentes de "segunda onda" a ser preparada (decisão registrada em
`docs/decisoes-autonomas.md`, 2026-07-16).

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), skill
`micro-ticket-planner` para granularidade. **Nenhuma dependência nova nesta fase** —
persistência via `serde_json` + `std::fs` (já dependências existentes), mesmo padrão de
`.agentry/` (ADR-0017).

---

## Fase 18 — Checkpoints e *undo* de mudanças de arquivo (ADR-0030)

### MT-86: `CheckpointStore` — registra e desfaz checkpoints ✅ concluído (7e7f608)
- **Objetivo:** novo `crates/core/src/checkpoint/mod.rs`: `CheckpointStore` persiste uma
  pilha *LIFO* de checkpoints em `.agentry/checkpoints.json` (via `state_dir::ensure_state_dir`,
  ADR-0017) — `record(path, conteudo_antes)` acrescenta uma entrada (descarta a mais antiga se
  ultrapassar um teto fixo); `undo()` desempilha a última, restaura o arquivo ao conteúdo
  anterior (ou remove, se `conteudo_antes` for `None`) e devolve o que foi desfeito.
- **Arquivos no escopo:** `crates/core/src/checkpoint/mod.rs` (novo), `crates/core/src/lib.rs`
  (novo `pub mod checkpoint`).
- **Critério de aceite:** testes — `record` seguido de `undo` restaura o conteúdo anterior;
  `undo` de um checkpoint de arquivo novo (`conteudo_antes: None`) remove o arquivo; `undo`
  sem nenhum checkpoint registrado é erro tratado, não *panic*; teto descarta o checkpoint mais
  antigo quando excedido; dois `record`/`undo` em sequência desfazem na ordem inversa (LIFO).
- **Fora de escopo:** integração com as tools (`fs_write`/`fs_edit`, MT-87); qualquer
  exposição via CLI/REPL/TUI (MT-87/88).
- **Depende de:** ADR-0030.

### MT-87: `CheckpointingTool` envolve `fs_write`/`fs_edit`; flag `--undo` e comando `/undo` ✅ concluído (84e86bd)
- **Objetivo:** novo `crates/core/src/tools/checkpoint.rs`: `CheckpointingTool` decora uma
  `Arc<dyn Tool>`, lê `arguments["path"]`, lê o conteúdo atual do arquivo antes de delegar a
  chamada de verdade, grava um checkpoint (`CheckpointStore::record`) só se o resultado
  delegado não for erro. `crates/cli/src/main.rs` envolve `FsWriteTool`/`FsEditTool` com essa
  decoração ao registrar; nova flag `--undo` (mutuamente exclusiva com `--init`/`--tui`/tarefa,
  mesmo padrão de `--init`) desfaz o checkpoint mais recente e sai. REPL
  (`crates/cli/src/repl.rs`) ganha o comando `/undo` (mesmo padrão de `/compact`).
- **Arquivos no escopo:** `crates/core/src/tools/checkpoint.rs` (novo),
  `crates/core/src/tools/mod.rs` (novo `pub mod checkpoint`), `crates/cli/src/main.rs`,
  `crates/cli/src/repl.rs`.
- **Critério de aceite:** testes — `CheckpointingTool` grava checkpoint só em chamada
  bem-sucedida (chamada com erro da tool interna não grava nada); `/undo` restaura o arquivo e
  reporta o que foi desfeito; `/undo` sem checkpoint disponível é erro tratado, REPL continua
  rodando; `--undo` funciona como *one-shot* (desfaz e sai, sem rodar tarefa).
- **Fora de escopo:** *keybinding* na TUI (MT-88).
- **Depende de:** MT-86.

### MT-88: *Keybinding* de *undo* na TUI
- **Objetivo:** `crates/cli/src/tui/keybind.rs` ganha `Ctrl+Z` → `Action::Undo` (único
  modificador livre na tabela); laço de eventos (`crates/cli/src/tui/mod.rs`) chama a mesma
  `CheckpointStore::undo()` do MT-87 e mostra o resultado como uma mensagem do sistema no
  histórico de chat (mesmo padrão de `estado.chat.marcar_erro`, mas para sucesso/erro de
  *undo*).
- **Arquivos no escopo:** `crates/cli/src/tui/keybind.rs`, `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** teste — `Ctrl+Z` resolve para `Action::Undo`, nenhuma tecla já
  mapeada colide; simulação de `Action::Undo` no laço de eventos (sem terminal real) chama
  `undo()` e reflete o resultado no estado do chat. Smoke-test manual: `Ctrl+Z` após uma
  edição real desfaz de fato, mensagem aparece no histórico.
- **Fora de escopo:** modal dedicado de *undo* (YAGNI — mensagem no histórico já é suficiente,
  mesmo espírito de `/compact` no REPL).
- **Depende de:** MT-87.

### MT-89: Documentação
- **Objetivo:** `docs/usuario/uso.md` ganha uma nota sobre `--undo`/`/undo`/`Ctrl+Z` — deixando
  explícito que só `fs_write`/`fs_edit` geram checkpoint (mudanças de `shell_exec`/
  `shell_background` não são desfeitas pelo `agentry`, para não criar expectativa equivocada,
  ADR-0030). ADR-0030 promovida de `Proposed` para `Accepted` (MT-86..88 concluídos);
  `docs/adr/README.md` e `docs/roadmap-longo-prazo.md` atualizados — Fase 18 marcada
  concluída.
- **Arquivos no escopo:** `docs/usuario/uso.md`, `docs/adr/0030-checkpoints-e-undo-de-mudancas-de-arquivo.md`
  (status), `docs/adr/README.md`, `docs/roadmap-longo-prazo.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo.
- **Fora de escopo:** nenhuma mudança de código.
- **Depende de:** MT-86..88 (todos).

---

## Sequência crítica

```
MT-86 → MT-87 → MT-88 → MT-89
```

Sequência estritamente linear (diferente da Fase 17, onde MT-83/84 podiam ser preparados em
paralelo) — MT-88 depende do `CheckpointStore::undo()` já exposto pelo MT-87 na CLI/REPL
antes de ganhar um terceiro consumidor (TUI).
