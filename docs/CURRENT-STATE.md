<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> Opcional em projetos solo; recomendado em colaborações. Atualizado a cada commit.
> Não inclua segredos. Mantido conforme a skill `handoff-updater`.

## Último turno

- **Data:** 2026-07-06
- **Branch:** `main`
- **Commit:** `2feed85`
- **Fase:** fundação — ADRs consolidados; iniciando MT-02.

## Metas cumpridas / Em andamento / Próximo passo

**Cumpridas — planejamento:**
- [x] Ecossistema de 2 repositórios: `ai-coding-agent-profiles` (política) ⇄ `agentry` (execução).
- [x] **ADR-0001..0005** + **contrato de interop v1** + `architecture.md` + `roadmap-v0.1.md` (MT-01..MT-16).
- [x] Provedores da v0.1: **Ollama, vLLM, Anthropic** (Copilot/Enterprise adiado).

**Cumpridas — implementação:**
- [x] **MT-01** — scaffold do workspace Cargo (`crates/cli` = bin `agentry`, `crates/core` = lib `agentry_core`), CI, lints, validação local verde (`ba74200`).
- [x] **ADR-0005 fechado** — portabilidade cross-platform: CI em matriz `ubuntu/windows/macos` (fmt/clippy em um SO), `.gitattributes` com LF normalizado (`2feed85`).

**Em andamento:** decisão de incluir **LiteLLM** como fonte de modelos (via adapter OpenAI-compatible) — ADR em elaboração; MT-02 na sequência.

**Próximo passo:** implementar **MT-02** (tipos de domínio de mensagens/LLM em `crates/core/src/model/`).

## Impedimentos abertos

- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido pela empresa; adapter adiado.

---

## Histórico (mais recente no topo)

| Data | Commit | Resumo | MT |
|------|--------|--------|----|
| 2026-07-06 | `2feed85` | ADR-0005 fechado: matriz de CI em 3 SOs + `.gitattributes` (LF) | — |
| 2026-06-19 | `ba74200` | MT-01: scaffold do workspace Cargo + CI + lint + `git init`; validação local verde | MT-01 |
| 2026-06-19 | — | Planejamento: ADR-0001..0004, interop v1, `architecture.md`, `roadmap-v0.1.md` | — |
