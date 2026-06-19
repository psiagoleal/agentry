<!-- Caminho relativo: docs/roadmap-v0.1.md -->

# Roadmap v0.1 — Micro-tickets

Backlog autocontido para a v0.1 do `agentry`. Cada micro-ticket cabe em um ciclo limpo de
contexto (ver skill `micro-ticket-planner`). Ordem pensada para que o **transporte/egresso**
(ADR-0002) exista **antes** de qualquer adapter tocar a rede.

## Convenções

- **Critério de aceite padrão (DoD):** `cargo fmt --check`, `cargo clippy -- -D warnings` e
  `cargo test` passam (no CI). Critérios específicos abaixo somam-se a este.
- **Crates convencionais** (`tokio`, `clap`, `serde`, `toml`, `reqwest`) seguem o stack de
  `docs/architecture.md`. Qualquer dependência **consequente** nova exige ADR (regra do
  ADR-0004). Bibliotecas de teste (mock HTTP, etc.) são *dev-dependencies* e seguem a
  verificação de maturidade/licença do ADR-0004 — a escolha exata fica a confirmar no ticket.
- Decisões estruturais já registradas: ADR-0001 (fundação LLM), ADR-0002 (egresso),
  ADR-0003 (consumo de profiles), ADR-0004 (sinergia OSS).

---

## Fase 0 — Bootstrap

### MT-01: Inicializar workspace Cargo + CI + lint
- **Objetivo:** esqueleto Rust que compila e passa no CI, com `cli` (binário) e `core` (lib).
- **Arquivos no escopo:** `Cargo.toml`, `crates/cli/Cargo.toml`, `crates/cli/src/main.rs`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `rustfmt.toml`, `.github/workflows/ci.yml`, `.gitignore`, `AGENTS.md` (bloco `USER:BEGIN id=comandos-exatos`).
- **Critério de aceite:** `cargo build` ok; CI roda fmt+clippy+test e fica verde; comandos exatos do `AGENTS.md` atualizados para Rust.
- **Fora de escopo:** qualquer lógica de LLM, rede ou CLI real (só `main` "hello").
- **Depende de:** nenhum · ADR-0001.

---

## Fase 1 — Núcleo de tipos e configuração

### MT-02: Tipos de domínio de mensagens/LLM (sem rede)
- **Objetivo:** tipos serializáveis: `Message`, `Role`, `ContentBlock`, `ToolCall`, `ToolResult`, `Usage`, `StreamEvent`.
- **Arquivos no escopo:** `crates/core/src/model/mod.rs`.
- **Critério de aceite:** testes de *round-trip* serde (serialize→deserialize) passam.
- **Fora de escopo:** nenhum formato específico de provider; nenhuma chamada de rede.
- **Depende de:** MT-01 · ADR-0001.

### MT-03: `trait LlmProvider` + provider mock de teste
- **Objetivo:** definir a `trait LlmProvider` (`chat`, `chat_stream`, *tool-calling*, `embeddings`) e um `MockProvider` para testes.
- **Arquivos no escopo:** `crates/core/src/provider/mod.rs`, `crates/core/src/provider/mock.rs`.
- **Critério de aceite:** compila; teste exercita `MockProvider` retornando mensagem e stream.
- **Fora de escopo:** qualquer adapter real (Ollama/OpenAI/Anthropic).
- **Depende de:** MT-02 · ADR-0001.

### MT-04: Configuração em camadas + resolução de classe de privacidade
- **Objetivo:** carregar config (perfil + projeto + env) e resolver perfil → classe de egresso.
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`, `crates/core/src/config/privacy.rs`.
- **Critério de aceite:** testes — `empresa`→`local-only`, `pessoal`→`cloud-ok`, classe ausente/ambígua→`local-only` (*fail-closed*).
- **Fora de escopo:** parsear todo o `settings-schema` (só o mínimo do ADR-0003); ler skills.
- **Depende de:** MT-02 · ADR-0002, ADR-0003.

---

## Fase 2 — Transporte e egresso (coração da confidencialidade)

### MT-05: Allowlist de endpoints + verificação de classe (lógica pura)
- **Objetivo:** decidir, sem rede, se um destino é permitido para a classe/perfil ativo.
- **Arquivos no escopo:** `crates/core/src/egress/allowlist.rs`.
- **Critério de aceite:** testes — endpoint fora da allowlist ⇒ erro; `local-only` rejeita host de nuvem; *fail-closed* no caso ambíguo.
- **Fora de escopo:** I/O HTTP real; audit log; redação.
- **Depende de:** MT-04 · ADR-0002.

### MT-06: Audit log de egresso + redação de segredos (lógica pura)
- **Objetivo:** produzir entrada de auditoria estruturada (destino, perfil, classe, tarefa) e redigir segredos antes de logar.
- **Arquivos no escopo:** `crates/core/src/egress/audit.rs`, `crates/core/src/egress/redact.rs`.
- **Critério de aceite:** testes — segredo nunca aparece no log; entrada contém os campos exigidos.
- **Fora de escopo:** transporte HTTP; persistência do log (só a estrutura/emissão).
- **Depende de:** MT-05 · ADR-0002.

### MT-07: Transporte HTTP único sobre `reqwest`
- **Objetivo:** ponto **único** de saída de rede, integrando allowlist + audit + redação; nenhuma outra parte do código chama `reqwest` diretamente.
- **Arquivos no escopo:** `crates/core/src/transport/mod.rs`.
- **Critério de aceite:** teste com servidor HTTP mock — chamada permitida passa; bloqueada **aborta**; teste/lint garante ausência de uso de `reqwest` fora deste módulo.
- **Fora de escopo:** lógica de qualquer provider; streaming específico de provider.
- **Depende de:** MT-05, MT-06 · ADR-0002.

---

## Fase 3 — Primeiro provider + router

### MT-08: Adapter Ollama (chat + stream) sobre o Transporte
- **Objetivo:** primeiro provider real (local), usando exclusivamente o Transporte (MT-07).
- **Arquivos no escopo:** `crates/core/src/provider/ollama.rs`.
- **Critério de aceite:** teste com mock do endpoint Ollama — chat e stream funcionam via Transporte; respeita `local-only`.
- **Fora de escopo:** router; outros providers.
- **Depende de:** MT-03, MT-07 · ADR-0001, ADR-0002.

### MT-09: Router / Policy Engine
- **Objetivo:** mapear `task-class → (provider, modelo, classe)` com fallback por disponibilidade.
- **Arquivos no escopo:** `crates/core/src/router/mod.rs`.
- **Critério de aceite:** testes — roteia por classe; tarefa sensível **nunca** roteia para provider de nuvem; fallback funciona.
- **Fora de escopo:** UI de configuração; providers de nuvem (ainda só Ollama/mock).
- **Depende de:** MT-04, MT-08 · ADR-0002, ADR-0003.

---

## Fase 4 — Loop, tools, permissão, CLI

### MT-10: Agent loop ReAct mínimo
- **Objetivo:** laço mensagem→tool-call→observação, com streaming e orçamento de tokens, sobre `MockProvider`/Ollama.
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`.
- **Critério de aceite:** teste — loop completa um ciclo de tool-call com provider mock e encerra no orçamento.
- **Fora de escopo:** tools reais; CLI; skills/MCP.
- **Depende de:** MT-03, MT-09 · ADR-0001.

### MT-11: Tool Registry + gate de permissão `allow|ask|deny`
- **Objetivo:** `trait Tool`, registro e portão de permissão.
- **Arquivos no escopo:** `crates/core/src/tools/mod.rs`, `crates/core/src/tools/permission.rs`.
- **Critério de aceite:** testes — `deny` bloqueia; `ask` sinaliza; `allow` executa (tool dummy).
- **Fora de escopo:** implementações concretas de fs/shell.
- **Depende de:** MT-10 · ADR-0002.

### MT-12: Tools de filesystem (read, write/edit, search)
- **Objetivo:** operações de arquivo respeitando `.claudeignore` e o gate de permissão.
- **Arquivos no escopo:** `crates/core/src/tools/fs.rs`.
- **Critério de aceite:** testes em diretório temporário — read/edit/search; respeita ignore; sob permissão.
- **Fora de escopo:** shell; rede.
- **Depende de:** MT-11.

### MT-13: Tool de shell sob permissão
- **Objetivo:** execução de comando com `deny` por padrão e ganchos de sandbox.
- **Arquivos no escopo:** `crates/core/src/tools/shell.rs`.
- **Critério de aceite:** testes — comando só roda sob `allow`; `deny` por *default*; nada executa sem aprovação.
- **Fora de escopo:** sandbox completo de SO (só os ganchos/política).
- **Depende de:** MT-11 · ADR-0002.

### MT-14: CLI streaming (one-shot + REPL)
- **Objetivo:** interface de linha que roda o loop, exibe stream/diffs e prompts de permissão.
- **Arquivos no escopo:** `crates/cli/src/main.rs`, `crates/cli/src/repl.rs`.
- **Critério de aceite:** teste de integração — `agentry "<tarefa>"` roda o loop com Ollama local/mock e trata permissão interativa.
- **Fora de escopo:** TUI (v0.3); skills/MCP (v0.2).
- **Depende de:** MT-10, MT-12, MT-13.

---

## Fase 5 — Demais providers

### MT-15: Adapter OpenAI-compatible (vLLM/OpenRouter)
- **Objetivo:** adapter para a API OpenAI-compatible sobre o Transporte (cobre vLLM e OpenRouter).
- **Arquivos no escopo:** `crates/core/src/provider/openai_compat.rs`.
- **Critério de aceite:** teste com mock — chat/stream/tool-calling; classe de egresso respeitada (`cloud-opt-out`/allowlist).
- **Fora de escopo:** Anthropic; UI.
- **Depende de:** MT-07, MT-08 (mesmo padrão) · ADR-0001, ADR-0002.

### MT-16: Adapter Anthropic (Messages API)
- **Objetivo:** adapter Anthropic (Messages API, *tool use*, streaming SSE) sobre o Transporte.
- **Arquivos no escopo:** `crates/core/src/provider/anthropic.rs`.
- **Critério de aceite:** teste com mock SSE — blocos `tool_use`; egresso só sob classe de nuvem permitida. *(Ao implementar, consultar a skill `claude-api` para o surface correto da API.)*
- **Fora de escopo:** *prompt caching* (v0.3); UI.
- **Depende de:** MT-03, MT-07 · ADR-0001, ADR-0002.

---

## Sequência crítica

```
MT-01 → MT-02 → MT-03 ─┐
            └ MT-04 → MT-05 → MT-06 → MT-07 → MT-08 → MT-09 → MT-10 → MT-11 → MT-12,13 → MT-14
                                                  └ MT-15, MT-16 (após MT-07/08)
```
