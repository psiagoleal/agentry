<!-- Caminho relativo: docs/roadmap-v0.14.md -->

# Roadmap v0.14 — Micro-tickets

O roadmap v0.13 (`docs/roadmap-v0.13.md`) cobre a Fase 19 (subagentes/orquestração,
ADR-0031, **concluída**). Este documento detalha a **Fase 20** do roadmap de longo prazo
(`docs/roadmap-longo-prazo.md`): memória de projeto explícita entre sessões (ADR-0032) — a
única frente de "segunda onda" pronta para virar ADR agora (multimodal continua bloqueada
por um pré-requisito próprio, o *guardrail* de imagem, ver `docs/roadmap-longo-prazo.md`
§Fase 21+).

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), skill
`micro-ticket-planner` para granularidade. **Nenhuma dependência nova nesta fase** —
persistência via `serde_json` + `std::fs` (já dependências existentes), mesmo padrão de
`.agentry/` (ADR-0017).

---

## Fase 20 — Memória de projeto explícita entre sessões (ADR-0032)

### MT-93: `MemoryStore` — grava e carrega fatos ✅ concluído (0c47121)
- **Objetivo:** novo `crates/core/src/memory/mod.rs`: `MemoryStore` persiste um array de
  *strings* em `.agentry/memory.json` (via `state_dir::ensure_state_dir`, ADR-0017 —
  auto-excluído do git). `remember(fato)` acrescenta uma entrada (sem teto — fatos são
  curados manualmente, decisão registrada na ADR-0032); `load()` devolve todas as entradas já
  gravadas, em ordem. `render_memoria(&fatos)` (função pura) formata a lista para injeção no
  *system prompt* (mesmo padrão de `render_skills_list`, ADR-0023).
- **Arquivos no escopo:** `crates/core/src/memory/mod.rs` (novo), `crates/core/src/lib.rs`
  (novo `pub mod memory`).
- **Critério de aceite:** testes — `remember` seguido de `load` devolve o fato gravado;
  múltiplos `remember` acumulam em ordem (nunca sobrescrevem); `load` sem nenhum arquivo
  ainda gravado devolve lista vazia, não erro; `render_memoria` de lista vazia devolve string
  vazia (mesmo padrão de `render_skills_list`); persistência confirmada em
  `.agentry/memory.json` com auto-exclusão do git (mesmo teste já usado para
  `checkpoints.json`, MT-86).
- **Fora de escopo:** comando `/remember`/flag `--remember` (MT-94); injeção no *system
  prompt* de uma `Session` real (MT-94); `/forget`/remoção de entrada (fora de escopo da
  ADR-0032 inteira, YAGNI).
- **Depende de:** ADR-0032.

### MT-94: Comando `/remember`, flag `--remember`, `Session::with_memoria` ✅ concluído (b6c4e22)
- **Objetivo:** `Session` (`crates/core/src/session/mod.rs`) ganha `with_memoria(texto)`
  (mesmo padrão *builder* de `with_project_instructions`/`with_skills_list`);
  `ensure_system_prompt` concatena instruções de projeto, memória, preset da *task-class*,
  lista de skills, nessa ordem. `crates/cli/src/main.rs`: nova flag `--remember <fato>`
  (mutuamente exclusiva com `--init`/`--tui`/tarefa, mesmo padrão de `--undo`) grava o fato e
  sai, sem rodar tarefa; ao montar a sessão real (qualquer modo), carrega
  `.agentry/memory.json` (se houver conteúdo) e chama `with_memoria`. `crates/cli/src/repl.rs`
  ganha o comando `/remember <fato>` (mesmo padrão de `/compact`) — grava e confirma, sem
  *side-effect* na conversa além disso.
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`, `crates/cli/src/main.rs`,
  `crates/cli/src/repl.rs`.
- **Critério de aceite:** testes — `/remember` grava e confirma; fato gravado numa invocação
  anterior aparece no *system prompt* de uma sessão nova (mesmo padrão de teste já usado para
  `project_instructions`); `--remember` funciona como *one-shot* (grava e sai, sem rodar
  tarefa); sessão sem nenhum fato gravado não insere bloco de memória vazio no *system
  prompt* (mesmo comportamento de `skills_list` vazio hoje).
- **Fora de escopo:** *keybinding*/comando na TUI (YAGNI — `/remember` já cobre REPL/*one-shot*,
  os dois modos mais comuns; TUI pode reaproveitar o mesmo `MemoryStore` numa extensão futura
  se houver demanda, mas não é um terceiro ponto de exposição obrigatório como as Fases
  17/18 tiveram, já que memória de projeto — diferente de uso de tokens/checkpoints — não é
  um dado que muda a cada turno).
- **Depende de:** MT-93.

### MT-95: Documentação
- **Objetivo:** `docs/usuario/uso.md` ganha uma seção sobre `/remember`/`--remember` — o que
  fica gravado, onde (`.agentry/memory.json`), que fica disponível em sessões futuras, que
  não existe `/forget` nesta versão (editar o arquivo é o caminho).
  `docs/governanca/privacidade-e-egresso.md` ganha uma nota curta sobre memória de projeto
  ser sempre um ato explícito do usuário, nunca uma decisão do agente, e local ao projeto
  (mesma garantia de `.agentry/`). ADR-0032 promovida de `Proposed` para `Accepted` (MT-93/94
  concluídos); `docs/adr/README.md` e `docs/roadmap-longo-prazo.md` atualizados — Fase 20
  marcada concluída.
- **Arquivos no escopo:** `docs/usuario/uso.md`, `docs/governanca/privacidade-e-egresso.md`,
  `docs/adr/0032-memoria-de-projeto-explicita.md` (status), `docs/adr/README.md`,
  `docs/roadmap-longo-prazo.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo.
- **Fora de escopo:** nenhuma mudança de código.
- **Depende de:** MT-93/94 (todos).

---

## Sequência crítica

```
MT-93 → MT-94 → MT-95
```

Sequência estritamente linear — MT-94 depende do núcleo do MT-93 já existir; MT-95 documenta
o comportamento final depois de ambos.
