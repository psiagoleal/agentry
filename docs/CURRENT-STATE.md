<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> Opcional em projetos solo; recomendado em colaborações. Atualizado a cada commit.
> Não inclua segredos. Mantido conforme a skill `handoff-updater`.

## Último turno

- **Data:** 2026-07-06
- **Branch:** `main`
- **Commit:** `26b370e`
- **Fase:** Fase 1 do roadmap — MT-02 e MT-03 concluídos.

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

**Em andamento:** nada pendente no turno.

**Próximo passo:** **MT-04** — configuração em camadas + resolução de classe de privacidade em `crates/core/src/config/` (depende de MT-02, feito; ADR-0002/0003).

## Impedimentos abertos

- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido pela empresa; adapter adiado.
- **CI multi-SO ainda não observado verde:** a matriz do ADR-0005 (`2feed85`) precisa de um push ao GitHub para confirmar Windows/macOS verdes.

---

## Histórico (mais recente no topo)

| Data | Commit | Resumo | MT |
|------|--------|--------|----|
| 2026-07-06 | `26b370e` | MT-03: `trait LlmProvider` + `MockProvider` roteirizado + testes | MT-03 |
| 2026-07-06 | `f03c1ef` | MT-02: tipos de domínio de mensagens/LLM + testes round-trip serde | MT-02 |
| 2026-07-06 | `ab69934` | ADR-0006: LiteLLM via adapter OpenAI-compatible; roadmap MT-15 e arquitetura atualizados | — |
| 2026-07-06 | `2feed85` | ADR-0005 fechado: matriz de CI em 3 SOs + `.gitattributes` (LF) | — |
| 2026-06-19 | `ba74200` | MT-01: scaffold do workspace Cargo + CI + lint + `git init`; validação local verde | MT-01 |
| 2026-06-19 | — | Planejamento: ADR-0001..0004, interop v1, `architecture.md`, `roadmap-v0.1.md` | — |
