<!-- Caminho relativo: docs/roadmap-v0.2.md -->

# Roadmap v0.2 — Micro-tickets

O roadmap v0.1 (`docs/roadmap-v0.1.md`, MT-01..38) está **fechado e imutável** como registro
histórico — todo ele implementado e validado (build Linux/Windows, teste de usabilidade
contra Ollama real). Este documento começa uma nova fase: fechar o loop do
`settings-schema:1` com o `ai-coding-agent-profiles`, formalizado na ADR-0018
(`docs/adr/0018-artefato-e-schema-minimo-de-configuracao-do-agentry.md`).

**Fase 7 concluída** (MT-39 + MT-40, `b3357a6`/`35362f6`) — este documento fica, a partir
daqui, fechado/imutável como registro histórico, mesmo padrão do `roadmap-v0.1.md`.

## Convenções

Mesmas do v0.1 (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão (`cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test`), dependência nova exige ADR (ADR-0004), skill
`micro-ticket-planner` para granularidade.

---

## Fase 7 — Configuração real via `agentry.settings.json`

### MT-39: `Settings::from_file` — carregamento do artefato de configuração — ✅ concluído (`b3357a6`)
- **Objetivo:** localizar e parsear `.agentry/agentry.settings.json` a partir da raiz
  resolvida por `state_dir::resolve_root` (MT-38); inserir como camada de precedência entre
  o *default* do perfil e as variáveis de ambiente em `Config::resolve`.
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`.
- **Critério de aceite:** testes — arquivo ausente não é erro (usa *defaults*); arquivo
  presente e válido é lido corretamente; arquivo presente mas JSON inválido é erro tratado
  (nunca *panic*); variável de ambiente sobrescreve o arquivo quando ambos definem o mesmo
  campo (mesma convenção de precedência já implícita em `Settings::resolve`).
- **Fora de escopo:** consumo das flags pelos pontos de registro de tools/provider (MT-40);
  schema além da primeira fatia (ADR-0018 §5).
- **Depende de:** MT-04, MT-38 · ADR-0018.

### MT-40: Consumo real das 4 flags já mecanicamente prontas — ✅ concluído (`35362f6`)
- **Objetivo:** `crates/cli/src/main.rs` para de hardcodar `true` para
  `structured_output`/`context.repo_map.enabled`/`context.semantic_rag.enabled`/
  `context.lsp_grounding.enabled` — passa a ler da `Config` resolvida (MT-39).
- **Arquivos no escopo:** `crates/cli/src/main.rs`.
- **Critério de aceite:** teste — `agentry.settings.json` com uma flag em `false` desativa a
  capacidade correspondente de ponta a ponta (tool não registrada / `structured_output`
  desligado no `OllamaProvider`); ausência do arquivo preserva o comportamento atual
  (todas `true`).
- **Fora de escopo:** UI/CLI de configuração (edição interativa do arquivo); schema além da
  primeira fatia.
- **Depende de:** MT-39 · ADR-0018.

---

## Sequência crítica

```
MT-39 → MT-40
```
