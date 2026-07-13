<!-- Caminho relativo: docs/roadmap-v0.3.md -->

# Roadmap v0.3 — Micro-tickets

O roadmap v0.2 (`docs/roadmap-v0.2.md`, MT-39/40) está **fechado e imutável** como registro
histórico — settings-schema:1 lido de verdade e as 4 flags de contexto/provider consumidas
pela CLI real (Fase 7 concluída). Este documento começa uma nova fase: implementar o
bootstrap de `.agentry/agentry.settings.json` decidido na ADR-0019
(`docs/adr/0019-bootstrap-de-agentry-settings-json-via-init.md`).

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), dependência nova exige
ADR (ADR-0004), skill `micro-ticket-planner` para granularidade.

---

## Fase 8 — Bootstrap de configuração (`--init`/`/init`, ADR-0019)

### MT-41: `--init`/`/init` sem `--profile` — bootstrap local, zero rede — ✅ concluído (`3a2075b`)
- **Objetivo:** `agentry --init` (flag CLI) e `/init` (comando REPL), chamados sem
  `--profile`, criam `.agentry/agentry.settings.json` com o exemplo genérico já documentado
  na ADR-0018 §5 (`schemaVersion`, `permissions` vazias, as 4 flags de contexto/provider em
  `true`), reaproveitando `state_dir::ensure_state_dir`/`agentry_settings_path` (MT-38/39)
  para localizar a raiz e criar o `.gitignore`. Se o arquivo já existir, não sobrescreve —
  emite aviso e sai sem erro. Em ambos os casos (criou ou já existia), imprime também o
  comando manual equivalente (`setup-profile.sh` do `ai-coding-agent-profiles`) como
  alternativa para quem quiser os valores diferenciados por perfil.
- **Arquivos no escopo:** `crates/cli/src/main.rs` (flag `--init`), `crates/cli/src/repl.rs`
  (comando `/init`) — mesmo padrão de módulo compartilhado já usado por
  `overrides_from_args`/`parse_bool_toggle` entre os dois pontos de entrada.
- **Critério de aceite:** testes — arquivo ausente é criado com o conteúdo exato do exemplo
  da ADR-0018 §5; arquivo já presente não é sobrescrito (conteúdo original preservado,
  aviso emitido, sem erro); a saída sempre inclui o comando manual (`setup-profile.sh`),
  independente de ter criado ou não; `/init` e `--init` produzem o mesmo arquivo a partir da
  mesma função compartilhada (sem lógica duplicada entre CLI e REPL).
- **Fora de escopo:** `--profile`/qualquer chamada de rede (MT-42); flag `--force` de
  sobrescrita explícita (deliberadamente adiada pela própria ADR-0019 §6).
- **Depende de:** MT-38, MT-39 · ADR-0017, ADR-0018, ADR-0019.

### MT-42: `--init --profile <nome>` — bootstrap via rede, referência pinada
- **Objetivo:** quando `--profile <empresa|externo-confidencial|pessoal>` for informado
  (na flag CLI ou no comando REPL), buscar o `agentry.settings.json` real daquele perfil no
  `ai-coding-agent-profiles`, através de uma **instância de `Transport` dedicada ao
  bootstrap** (`Allowlist` restrita a um único host fixo de conteúdo bruto do GitHub,
  `EgressClass::CloudOk` — nunca a classe de egresso do perfil-alvo nem de nenhuma sessão),
  numa referência (tag ou commit) **fixa, gravada como constante no código-fonte** (nunca
  resolvida contra "latest"). O artefato obtido é validado com `Settings::from_json_str`
  (checagem de `schemaVersion`) antes de qualquer gravação em disco. Reaproveita a função de
  escrita/idempotência do MT-41 (não sobrescreve arquivo já existente; imprime o comando
  manual como alternativa).
- **Arquivos no escopo:** `crates/cli/src/main.rs`, novo módulo `crates/cli/src/init.rs`
  (lógica de fetch + validação, mantendo `main.rs` como só orquestração — mesmo padrão de
  `streaming.rs`/`tool_executor.rs`), `crates/cli/src/repl.rs`.
- **Critério de aceite:** testes — nome de perfil desconhecido é erro tratado **antes** de
  qualquer chamada de rede; servidor local (mock via `tokio::net`, mesma técnica já usada em
  `transport::tests`) respondendo um JSON válido resulta no arquivo gravado corretamente;
  servidor respondendo `schemaVersion` incompatível não grava nada e devolve erro tratado;
  host inalcançável/timeout é erro tratado, **nunca** cai silenciosamente no exemplo
  genérico do MT-41; a chamada de rede efetivamente passa pelo `Transport` (prova por
  reaproveitar o teste-guarda do MT-07 ou verificação equivalente de que não há chamada de
  rede fora dele).
- **Fora de escopo:** `--force`/sobrescrita explícita; resolução dinâmica de "latest" em vez
  da referência pinada (proibido pela ADR-0019); qualquer execução de conteúdo obtido pela
  rede (a ADR já proíbe — não há ambiguidade a implementar aqui).
- **Depende de:** MT-41 · ADR-0002, ADR-0019.

---

## Sequência crítica

```
MT-41 → MT-42
```
