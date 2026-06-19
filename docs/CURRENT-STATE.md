<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> Opcional em projetos solo; recomendado em colaborações. Atualizado a cada commit.
> Não inclua segredos. Mantido conforme a skill `handoff-updater`.

## Último turno

- **Data:** 2026-06-19
- **Branch:** `main`
- **Commit:** — (pendente; **MT-01** implementado e validado, aguardando autorização para o commit inicial)
- **Fase:** implementação iniciada — scaffold (MT-01) pronto.

## Metas cumpridas / Em andamento / Próximo passo

**Cumpridas — planejamento:**
- [x] Ecossistema de 2 repositórios: `ai-coding-agent-profiles` (política) ⇄ `agentry` (execução).
- [x] **ADR-0001..0004** + **contrato de interop v1** + `architecture.md` + `roadmap-v0.1.md` (MT-01..MT-16).
- [x] Provedores da v0.1: **Ollama, vLLM, Anthropic** (Copilot/Enterprise adiado).

**Cumpridas — implementação:**
- [x] **MT-01** — scaffold do workspace Cargo (`crates/cli` = bin `agentry`, `crates/core` = lib `agentry_core`), `rustfmt.toml`, lints clippy de workspace, `.github/workflows/ci.yml`, `.gitignore`, comandos exatos do `AGENTS.md` atualizados para Rust. `git init -b main` feito.
- [x] **Validação local verde:** `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --all` (1 teste), `cargo build --all --release`. Binário imprime `agentry 0.1.0`.

**Em andamento:** commit inicial do MT-01 (aguardando autorização).

**Próximo passo:** commitar o MT-01 e seguir para **MT-02** (tipos de domínio de mensagens/LLM em `crates/core/src/model/`).

## Impedimentos abertos

- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido pela empresa; adapter adiado.

---

## Histórico (mais recente no topo)

| Data | Commit | Resumo | MT |
|------|--------|--------|----|
| 2026-06-19 | (pendente) | MT-01: scaffold do workspace Cargo + CI + lint + `git init`; validação local verde | MT-01 |
| 2026-06-19 | — | Planejamento: ADR-0001..0004, interop v1, `architecture.md`, `roadmap-v0.1.md` | — |
