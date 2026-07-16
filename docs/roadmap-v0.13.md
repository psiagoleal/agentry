<!-- Caminho relativo: docs/roadmap-v0.13.md -->

# Roadmap v0.13 — Micro-tickets

O roadmap v0.12 (`docs/roadmap-v0.12.md`) cobre a Fase 18 (checkpoints e *undo* de mudanças
de arquivo, ADR-0030, **concluída**). Este documento detalha a **Fase 19** do roadmap de
longo prazo (`docs/roadmap-longo-prazo.md`): subagentes/orquestração (ADR-0031) — escolhida
pelo mantenedor entre as três frentes restantes de "segunda onda" (decisão de
2026-07-16, registrada em `docs/decisoes-autonomas.md`).

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), skill
`micro-ticket-planner` para granularidade. **Nenhuma dependência nova nesta fase** — reaproveita
100% da infraestrutura existente (`Router`, `ToolRegistry`, `Session`, `GuardrailGate`).

---

## Fase 19 — Subagentes/orquestração (ADR-0031)

### MT-90: `SubagentTool` — núcleo (constrói e roda uma `Session` interna) ✅ concluído (a4f8470)
- **Objetivo:** novo `crates/core/src/tools/subagent.rs`: `SubagentTool` guarda `Arc<Router>`
  (o **mesmo** da sessão-mãe), `Arc<dyn ToolExecutor>` (cujo `ToolRegistry` interno **não**
  inclui a própria tool `subagent` — recursão impossível estruturalmente, ADR-0031),
  `Arc<GuardrailGate>` + *sink*. `execute()` lê `arguments["description"]` (texto livre da
  subtarefa) e `arguments["task_class"]` (opcional, *default* `"chat"`), resolve via
  `Router::resolve`/`resolve_with_override` (nunca um caminho de resolução paralelo — mesma
  disciplina de vetar candidato, ADR-0014), constrói uma `Session` nova
  (`Session::new(rota, executor, TokenBudget::new(DEFAULT_TOKEN_BUDGET))`
  `.with_guardrails(...)`), roda até completar (`Session::run`, sem *streaming*) e devolve o
  texto da resposta final como `ToolOutput` — erro de `Router`/`Session` vira
  `ToolOutput::error` tratado, nunca `panic`.
- **Arquivos no escopo:** `crates/core/src/tools/subagent.rs` (novo),
  `crates/core/src/tools/mod.rs` (novo `pub mod subagent`).
- **Critério de aceite:** testes — subtarefa simples completa e devolve o texto esperado;
  `task_class` desconhecida/candidato indisponível é erro tratado (mesmo padrão de
  `Router::resolve_with_override`); um `Router` com teto de egresso restritivo nunca deixa o
  subagente resolver um candidato mais permissivo (mesmo teste de fail-closed já usado para a
  sessão principal, aplicado ao subagente); o executor interno do subagente, ao listar suas
  `ToolSpec`s, nunca inclui `"subagent"` (recursão comprovadamente impossível, não só não
  testada).
- **Fora de escopo:** fiação em `crates/cli/src/main.rs` (MT-91); documentação (MT-92).
- **Depende de:** ADR-0031.

### MT-91: Fiação na CLI — dois registros de tools, um sem `subagent` ✅ concluído (0d2c4cf)
- **Objetivo:** `crates/cli/src/main.rs` refatora a construção de tools para uma lista
  reutilizável de `Arc<dyn Tool>` (mesmas instâncias, sem duplicar estado real de nenhuma
  tool), registrada em **dois** `ToolRegistry`: um sem `SubagentTool` (vira o
  `Arc<dyn ToolExecutor>` interno de `SubagentTool`), outro com `SubagentTool` incluída (vira
  o executor da sessão principal de verdade, exposta ao usuário).
- **Arquivos no escopo:** `crates/cli/src/main.rs`.
- **Critério de aceite:** teste de ponta a ponta — a sessão principal enxerga a tool
  `subagent` em `ToolRegistry::specs()`; uma chamada real à tool `subagent` (com um
  `MockProvider`/executor de teste) completa e devolve resultado; nenhuma regressão nos
  testes já existentes de registro de tools (`cfg_com_flags`/`register_context_tools`).
- **Fora de escopo:** qualquer mudança de comportamento das tools já existentes.
- **Depende de:** MT-90.

### MT-92: Documentação ✅ concluído (66747d4) — fecha a Fase 19
- **Objetivo:** `docs/usuario/uso.md` ganha uma nota sobre a tool `subagent` — o que ela faz,
  que a resposta só aparece de uma vez (sem *streaming*), que um subagente nunca cria outro.
  `docs/governanca/privacidade-e-egresso.md` ganha uma seção "Subagentes e egresso"
  explicando, para o público de *compliance*, que um subagente nunca resolve um candidato mais
  permissivo que o teto de egresso do perfil ativo (mesmo `Router` compartilhado, garantia
  estrutural, não uma checagem que poderia ser esquecida). ADR-0031 promovida de `Proposed`
  para `Accepted` (MT-90/91 concluídos); `docs/adr/README.md` e
  `docs/roadmap-longo-prazo.md` atualizados — Fase 19 marcada concluída.
- **Arquivos no escopo:** `docs/usuario/uso.md`, `docs/governanca/privacidade-e-egresso.md`,
  `docs/adr/0031-subagentes-com-egresso-restrito.md` (status), `docs/adr/README.md`,
  `docs/roadmap-longo-prazo.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo.
- **Fora de escopo:** nenhuma mudança de código.
- **Depende de:** MT-90/91 (todos).

---

## Sequência crítica

```
MT-90 → MT-91 → MT-92
```

Sequência estritamente linear — MT-91 depende do núcleo do MT-90 já existir; MT-92 documenta
o comportamento final depois de ambos.
