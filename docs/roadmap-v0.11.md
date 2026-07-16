<!-- Caminho relativo: docs/roadmap-v0.11.md -->

# Roadmap v0.11 — Micro-tickets

O roadmap v0.10 (`docs/roadmap-v0.10.md`) cobre a Fase 16 (cliente MCP via `rmcp`,
ADR-0028, **concluída**). Este documento detalha a **Fase 17** do roadmap de longo prazo
(`docs/roadmap-longo-prazo.md`): uso de tokens visível durante a sessão (ADR-0029) — a
primeira das cinco frentes de "segunda onda", escolhida por ser a única sem nenhuma pergunta
de segurança/confidencialidade/egresso em aberto (decisão registrada em
`docs/decisoes-autonomas.md`, 2026-07-16).

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), skill
`micro-ticket-planner` para granularidade. **Nenhuma dependência nova nesta fase** — todo o
dado necessário (`Usage`) já existe em `crates/core/src/model/mod.rs`, só falta acumular e
expor.

---

## Fase 17 — Uso de tokens visível durante a sessão (ADR-0029)

### MT-82: `Session` acumula `Usage` ao longo da sessão ✅ concluído (60b1b41)
- **Objetivo:** `Session` (`crates/core/src/session/mod.rs`) ganha um campo interno de uso
  acumulado (`Usage`), somado ao final de cada turno concluído (mesmo ponto onde o `Usage` do
  turno já é calculado hoje) — nunca influencia roteamento nem `TokenBudget` (responsabilidade
  distinta: truncar histórico, não relatar consumo). Novo método de leitura (`usage_total()`
  ou nome equivalente) expõe o total acumulado; `Session::compact` (ADR-0016) não reseta o
  contador (compactar histórico não é "começar de novo" do ponto de vista de uso consumido).
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`.
- **Critério de aceite:** testes — um turno soma corretamente ao total; múltiplos turnos
  acumulam (não sobrescrevem); sessão recém-criada começa em `Usage::default()`; `/compact`
  não zera o total acumulado.
- **Fora de escopo:** qualquer exposição pela CLI (MT-83/84); persistência entre sessões
  (fora de escopo da ADR-0029 inteira).
- **Depende de:** ADR-0029.

### MT-83: Exposição no modo *one-shot* e comando `/usage` no REPL ✅ concluído (5e54de9)
- **Objetivo:** modo *one-shot* (`agentry "tarefa"`, `crates/cli/src/main.rs`) imprime uma
  linha de resumo do uso total em `stderr` ao final da tarefa (mesma classe de saída de
  `[audit] ...`, nunca em `stdout`). REPL (`crates/cli/src/repl.rs`) ganha o comando `/usage`
  (mesmo padrão de `/compact`): imprime o total acumulado da sessão até aquele ponto, sem
  side-effect na conversa.
- **Arquivos no escopo:** `crates/cli/src/main.rs`, `crates/cli/src/repl.rs`.
- **Critério de aceite:** testes — `/usage` imprime o total corrente sem alterar histórico
  nem preset; resumo do modo *one-shot* aparece em `stderr`, nunca em `stdout` (teste que
  verifica os dois fluxos separadamente, mesmo padrão dos testes de streaming já existentes).
- **Fora de escopo:** exposição na TUI (MT-84).
- **Depende de:** MT-82.

### MT-84: Exposição na TUI (rodapé) ✅ concluído (4cd7f5e)
- **Objetivo:** modo TUI (`--tui`, `crates/cli/src/tui/mod.rs`) mostra o uso total acumulado
  na barra de rodapé já existente (mesmo lugar da legenda de *keybindings*,
  `keybind::legenda()`), atualizado a cada turno concluído — sem modal novo, sem tecla nova.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** teste — o texto do rodapé inclui o total de uso corrente após um
  turno simulado (mesmo padrão dos testes de `Estado`/`aplicar` já existentes, função pura
  testável sem terminal real). Smoke-test manual: rodapé atualiza visivelmente após uma
  mensagem real.
- **Fora de escopo:** qualquer widget/modal dedicado a uso (YAGNI — cabe no rodapé).
- **Depende de:** MT-82.

### MT-85: Documentação ✅ concluído (4cc49df) — fecha a Fase 17
- **Objetivo:** `docs/usuario/uso.md` ganha uma nota sobre o comando `/usage`, o resumo do
  modo *one-shot* e o rodapé da TUI — todos apontando para a mesma fonte de dado (`Usage`
  acumulado da sessão). ADR-0029 promovida de `Proposed` para `Accepted` (MT-82..84
  concluídos); `docs/adr/README.md` e `docs/roadmap-longo-prazo.md` atualizados — Fase 17
  marcada concluída.
- **Arquivos no escopo:** `docs/usuario/uso.md`, `docs/adr/0029-uso-de-tokens-visivel-na-sessao.md`
  (status), `docs/adr/README.md`, `docs/roadmap-longo-prazo.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo.
- **Fora de escopo:** nenhuma mudança de código.
- **Depende de:** MT-82..84 (todos).

---

## Sequência crítica

```
MT-82 → MT-83 → MT-85
   └──→ MT-84 ──↗
```

MT-83 e MT-84 dependem só do MT-82 e podem ser preparados em paralelo em termos de
planejamento, mas como o loop autônomo processa um ticket por vez, a ordem numérica
(MT-82 → 83 → 84 → 85) é seguida à risca — mesma disciplina já usada nas Fases 15/16.
