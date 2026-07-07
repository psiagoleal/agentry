<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> Opcional em projetos solo; recomendado em colaborações. Atualizado a cada commit.
> Não inclua segredos. Mantido conforme a skill `handoff-updater`.

## Último turno

- **Data:** 2026-07-07
- **Branch:** `main`
- **Commit:** `1723c31`
- **Fase:** Fase 2 do roadmap (egresso) **concluída** (MT-05..MT-07); próxima é a Fase 3 (primeiro provider real + router).

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

**Em andamento:** nada pendente no turno.

**Próximo passo:** **MT-08** — adapter Ollama (chat + stream) sobre o Transporte (`crates/core/src/provider/ollama.rs`), o primeiro provider real, local, respeitando `local-only` (depende de MT-03/MT-07, feitos; ADR-0001/0002). Abre a Fase 3 (primeiro provider + router).

## Impedimentos abertos

- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido pela empresa; adapter adiado.
- **CI multi-SO ainda não observado verde:** a matriz do ADR-0005 (`2feed85`) precisa de um push ao GitHub para confirmar Windows/macOS verdes.

---

## Histórico (mais recente no topo)

| Data | Commit | Resumo | MT |
|------|--------|--------|----|
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
