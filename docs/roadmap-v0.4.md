<!-- Caminho relativo: docs/roadmap-v0.4.md -->

# Roadmap v0.4 — Micro-tickets

O roadmap v0.3 (`docs/roadmap-v0.3.md`, MT-41/42) está **fechado e imutável** como registro
histórico — bootstrap de `.agentry/agentry.settings.json` via `--init`/`/init`, local e via
rede (Fase 8 concluída). Este documento começa uma nova fase: implementar o Guardrail Gate
decidido na ADR-0007 (emendada em 2026-07-13 —
`docs/adr/0007-guardrails-configuraveis-de-conteudo.md`).

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), dependência nova exige
ADR (ADR-0004), skill `micro-ticket-planner` para granularidade.

---

## Fase 9 — Guardrail Gate (ADR-0007)

### MT-43: Módulo `guardrail` — tipos, correspondência e auditoria — ✅ concluído (`7627c53`)
- **Objetivo:** novo módulo de topo `crates/core/src/guardrail/mod.rs`, paralelo a
  `egress`/`tools` — `GuardrailAction` (`Block`/`Redact`, com `rank()` análogo ao
  `EgressClass::rank()` do ADR-0002, `Block` > `Redact`), `GuardrailRule` (`id`/`match_text`/
  `action`), `GuardrailDirection` (`Input`/`Output`), `GuardrailCheckResult`
  (`Allowed`/`Redacted(String)`/`Blocked(String)` — texto final ou `id` da regra que
  bloqueou), `GuardrailGate` (`input: Vec<GuardrailRule>`, `output: Vec<GuardrailRule>`) com
  um método `check(direction, texto) -> GuardrailCheckResult` — substring *case-insensitive*
  (sem `regex`, ADR-0007 §1); múltiplas regras `redact` casando aplicam todas as máscaras
  (não só a primeira); qualquer regra `block` que case vence sobre `redact` no mesmo texto.
  `GuardrailAuditEntry` (`direction`/`rule_id`/`action`/`task`) + `trait GuardrailAuditSink`
  (`fn record`) — par novo, não `AuditEntry`/`AuditSink` literais (ADR-0007 §6); só emitido
  quando uma regra efetivamente age (nunca para `Allowed`).
- **Arquivos no escopo:** `crates/core/src/guardrail/mod.rs` (novo), `crates/core/src/lib.rs`
  (`pub mod guardrail;`).
- **Critério de aceite:** testes — substring casa *case-insensitive*; `block` vence `redact`
  quando ambos casam no mesmo texto; múltiplos `redact` mascaram todas as ocorrências, não só
  a primeira; nenhuma regra casando devolve `Allowed` sem gerar `GuardrailAuditEntry`; regra
  que efetivamente age gera exatamente uma entrada, nunca com o texto casado dentro dela.
- **Fora de escopo:** extensão do `settings-schema`/`Config` (MT-44); integração com
  `Session` (MT-45); qualquer motor de correspondência além de substring (proibido pela
  ADR-0007 sem uma ADR própria de dependência).
- **Depende de:** ADR-0007 · nenhum micro-ticket anterior.

### MT-44: `GuardrailSettings` — schema mínimo em `Config` — ✅ concluído (`3039554`)
- **Objetivo:** `Settings` (`crates/core/src/config/mod.rs`) ganha `guardrails:
  GuardrailSettings` (`input: Vec<GuardrailRuleSettings>`, `output: Vec<GuardrailRuleSettings>`
  — mesma forma do JSON da ADR-0007 §2: `{ id, match, action }`), com `merged_over` que une
  por `id` entre camadas, vencendo a ação mais severa em caso de colisão (`block` > `redact`,
  ADR-0007 §3) — mesmo padrão de `ContextSettings::merged_over`/`FeatureToggle` (MT-39).
  `Config` expõe os dois vetores resolvidos (ou um `GuardrailGate` já pronto, reaproveitando
  o tipo do MT-43 — decisão de design fica para a implementação, não muda o comportamento
  observável).
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`.
- **Critério de aceite:** testes — JSON com `guardrails.input`/`guardrails.output` carrega
  corretamente; camada mais específica **adiciona** uma regra de `id` novo sem apagar as
  herdadas; duas camadas com o mesmo `id` e ações diferentes resolvem para a mais severa,
  nas duas ordens (`block` depois de `redact` e vice-versa); ausência da chave `guardrails`
  não é erro e resulta em nenhuma regra.
- **Fora de escopo:** integração com `Session` (MT-45); consumo real na CLI (MT-46).
- **Depende de:** MT-43 · ADR-0007.

### MT-45: `Session` aplica o Guardrail Gate (entrada e saída) — ✅ concluído (`6d46a51`)
- **Objetivo:** `Session::with_guardrails(gate: Arc<GuardrailGate>, sink: Arc<dyn
  GuardrailAuditSink>) -> Self` (*default*: nenhum gate — mesmo "desligado por padrão até
  configurado" de `with_reviews`, MT-35). `run`/`run_streaming` checam a mensagem de usuário
  mais recente contra `guardrails.input` **antes** do loop começar — `Blocked` substitui por
  aviso fixo e devolve `SessionOutcome` com `StopReason::Done` **sem chamar o provider**
  (ADR-0007 §4); `Redacted` mascara a mensagem antes de montar a `ChatRequest`. Após
  `StopReason::Done`, checa a última mensagem (resposta do turno) contra `guardrails.output`
  **antes** de `revisar_ou_continuar` (Reviewer, ADR-0015) — `Blocked` substitui a resposta
  pelo aviso fixo e retorna imediatamente (nunca chega a rodar o Reviewer sobre um conteúdo
  substituído); `Redacted` mascara a mensagem e segue o fluxo normal (Reviewer roda em cima
  do texto já mascarado). `SessionOutcome` ganha `guardrail_hits: Vec<GuardrailHit>`
  (`direction`/`rule_id`/`action`) para observabilidade em teste, paralelo a `reviews`.
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`.
- **Critério de aceite:** testes — regra de entrada `block` nunca chama o provider mock
  (zero `chat_requests()`); regra de entrada `redact` chega ao provider já mascarada; regra
  de saída `block` substitui a resposta pelo aviso e pula o Reviewer mesmo com reviews
  habilitadas; regra de saída `redact` mascara a resposta e o Reviewer ainda roda em cima
  dela; sessão sem `with_guardrails` (*default*) nunca aplica nenhuma checagem, mesmo padrão
  do "reviews vazio nunca chama o Reviewer" (MT-35).
- **Fora de escopo:** carregar `GuardrailSettings` de `Config` (MT-44, já pronto);
  construção do `Arc<GuardrailGate>` a partir da CLI real (MT-46).
- **Depende de:** MT-43 · ADR-0007, ADR-0015 (ordem em relação ao Reviewer).

### MT-46: Consumo real na CLI
- **Objetivo:** `crates/cli/src/main.rs` constrói o `GuardrailGate` a partir do
  `GuardrailSettings` resolvido pela `Config` (MT-44) e chama `Session::with_guardrails`
  (MT-45) — mesmo padrão de "consumo real" já usado pelo MT-40/42 para as outras flags.
  `StderrAuditSink` (já existente) ganha `impl GuardrailAuditSink` — mesma disciplina de
  `Display` compacto já usada para `AuditEntry` (fix de usabilidade anterior), uma linha por
  entrada, nunca o `Debug` dump.
- **Arquivos no escopo:** `crates/cli/src/main.rs`.
- **Critério de aceite:** teste — `agentry.settings.json` com uma regra `guardrails.input`/
  `guardrails.output` de fato bloqueia/redige de ponta a ponta via a `Session` real construída
  em `main()` (mesmo padrão de prova do MT-40: registry/config real, não só unitário isolado);
  ausência do arquivo preserva o comportamento atual (nenhuma checagem, `with_guardrails`
  nunca chamado ou chamado com gate vazio).
- **Fora de escopo:** UI/CLI de configuração; `--force`/edição interativa de regras.
- **Depende de:** MT-44, MT-45 · ADR-0007.

---

## Sequência crítica

```
MT-43 → MT-44 → MT-46
MT-43 → MT-45 → MT-46
```
