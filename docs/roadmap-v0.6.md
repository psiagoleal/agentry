<!-- Caminho relativo: docs/roadmap-v0.6.md -->

# Roadmap v0.6 — Micro-tickets

O roadmap v0.5 (`docs/roadmap-v0.5.md`) cobre a Fase 10 (LiteLLM, **concluída**) e a Fase 11
(`.agentryignore` + `.gitignore` opcional, ADR-0020, **pendente** — MT-52/53/54). Este
documento detalha a **Fase 12** do roadmap de longo prazo (`docs/roadmap-longo-prazo.md`):
tornar o roteamento por task-class configurável de ponta a ponta (ADR-0021) e instituir a
convenção de configuração autoexplicativa (ADR-0022).

> A Fase 11 permanece em `roadmap-v0.5.md`; ela e a Fase 12 são independentes (mexem em
> arquivos diferentes) e podem ser feitas em qualquer ordem.

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), dependência nova exige
ADR (ADR-0004), skill `micro-ticket-planner` para granularidade. Nenhuma dependência nova
nesta fase — só camada de configuração sobre o `Router` já existente.

---

## Fase 12 — Configuração completa e autoexplicativa (ADR-0021, ADR-0022) ✅ concluída (MT-55..58)

### MT-55: `TaskClassSettings` — schema de task-class em `Config` ✅ concluído
- **Objetivo:** `Settings` (`crates/core/src/config/mod.rs`) ganha um bloco `taskClasses`
  (mapa `nome → { candidates: [{ provider, model, egressClass }], preset: { temperature,
  topP, maxTokens, systemPrompt, reasoning } }`), com `merged_over` por nome de task-class
  (candidato/preset da camada mais específica vence, egresso **nunca afrouxa** — mesma
  disciplina de `Permissions::union`/MT-44). `Config::resolve` expõe as task-classes
  resolvidas como `RouteEntry` prontos (reaproveita `RouteEntry`/`RouteTarget`/`CallPreset`
  do `Router`, ADR-0008/0014 — **sem tipo novo de roteamento**).
  **Desvio registrado (decisão autônoma):** `Config` **não** sintetiza os defaults
  `chat`/`compact`/`guardrail-compliance` — ausência do bloco resolve em mapa vazio. A síntese
  de defaults concretos (que exige conhecer `"ollama"` como provider de produto) fica a cargo
  da CLI (MT-56), preservando a fronteira `core` = domínio / CLI = produto. Justificativa
  completa em `docs/decisoes-autonomas.md` (entrada MT-55).
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`.
- **Critério de aceite:** testes — `taskClasses` completo resolve `RouteEntry` com os
  candidatos/preset exatos; ausência do bloco resolve em mapa **vazio** (sem síntese — desvio
  acima); task-class declarada sobrescreve o default de mesmo nome; merge por nome adiciona
  task-class nova sem apagar herdadas; camada mais específica **não** afrouxa a classe de
  egresso de um candidato.
- **Fora de escopo:** consumo pela CLI (MT-56); flag `--task-class` (MT-56); exemplo gerado
  (MT-57); síntese de defaults concretos de provider/modelo (deferida ao MT-56, ver desvio
  acima).
- **Depende de:** ADR-0021 · ADR-0008/0014 (tipos do Router) · ADR-0018 (padrão de schema).

### MT-56: CLI consome task-classes reais + flag `--task-class`/comando `/task-class` ✅ concluído
- **Objetivo:** `crates/cli/src/main.rs`/`repl.rs` param de hardcodar `set_chat_route`:
  montam o `Router` a partir das task-classes resolvidas (MT-55), registrando **todas** as
  rotas declaradas — inclusive `compact`/`guardrail-compliance`, que passam a ter rota de
  fato (hoje `/compact` e o Reviewer não são configurados na CLI real). **Síntese de
  defaults (herdada do desvio do MT-55):** quando `cfg.task_classes` resolvido não declara
  `chat`/`compact`/`guardrail-compliance`, a CLI sintetiza aqui os defaults concretos hoje
  hardcoded em `set_chat_route` (Ollama, `local-only`), para zero-config idêntico ao
  comportamento atual — a CLI é o lugar certo por já hardcodar esse provider hoje. Nova flag
  `--task-class <nome>` (one-shot) e comando `/task-class <nome>` (REPL) escolhem entre as
  task-classes **declaradas** para a invocação (mesmo padrão vetado de `--provider`/`--model`,
  ADR-0014; `chat` é o default de usuário).
- **Arquivos no escopo:** `crates/cli/src/main.rs`, `crates/cli/src/repl.rs`.
- **Critério de aceite:** teste de ponta a ponta — `agentry.settings.json` com uma task-class
  custom (ex.: `revisao`, apontando para outro modelo) resolve a rota nela via
  `--task-class revisao`; ausência do arquivo preserva o comportamento atual (rota `chat` →
  Ollama); `/compact` num REPL com config real não falha mais por falta de rota
  (`compact` registrada); provider/modelo inexistente na task-class escolhida é o mesmo erro
  tratado de `Router::resolve_with_override` (reaproveitado, sem *panic*).
- **Fora de escopo:** exemplo gerado por `--init` (MT-57); UI de configuração interativa.
- **Depende de:** MT-55.
- **Nota de implementação:** `/model` continua redeclarando especificamente a task-class
  `chat` (comportamento pré-existente, documentado explicitamente no código) —
  independentemente de qual task-class está ativa via `/task-class`. Generalizar `/model`
  para redeclarar a task-class ativa exigiria assumir Ollama como provider também para
  task-classes customizadas (que podem apontar só para LiteLLM, por exemplo), o que
  contrariaria a config declarada pelo usuário; deixado fora de escopo desta ticket (UI de
  configuração interativa já estava explicitamente fora de escopo).

### MT-57: Exemplo `--init` enriquecido — convenção autoexplicativa (ADR-0022) ✅ concluído
- **Objetivo:** `GENERIC_SETTINGS_EXAMPLE` (`crates/cli/src/main.rs`) ganha o bloco
  `taskClasses` com a task-class `chat` default (Ollama/`local-only`) **mais exemplos
  comentados** de alternativas (ex.: uma task-class de nuvem `cloud-ok`; uma de dados
  sensíveis `local-only` com outro modelo) via `_comentario`. O bloco `guardrails` (hoje só
  `input`/`output` vazios) ganha **regras de exemplo comentadas** (`block` e `redact`).
  Auditoria de **todos** os blocos do exemplo segundo a ADR-0022 (default + `_comentario` +
  exemplos) — permissions/context/providers/guardrails/taskClasses.
- **Arquivos no escopo:** `crates/cli/src/main.rs`.
- **Critério de aceite:** teste — o exemplo gerado é JSON válido do schema real
  (`Settings::from_json_str`) **e** todo campo de exemplo fica inerte
  (`Config::resolve` sobre ele não ativa nada indevido — extensão do teste
  `generic_settings_example_e_json_valido_e_todo_campo_null_fica_inerte` do commit `ed0988c`,
  agora cobrindo também `taskClasses`); `--init` continua gravando o arquivo (smoke-test).
- **Fora de escopo:** documentação do site (MT-58).
- **Depende de:** MT-55 (schema existe) · ADR-0022.
- **Notas de implementação:** `taskClasses` é `HashMap<String, TaskClassSettings>` sem
  *wrapper* — uma chave `_comentario` solta dentro do bloco quebraria o parse (toda chave
  vira uma tentativa de `TaskClassSettings` de verdade); a explicação do mecanismo entra
  dentro do `_comentario` da própria task-class `chat`, que é garantida presente. Auditoria
  encontrou um gap real: `context.gitignore.enabled` (ADR-0020 §3, desde o MT-53/54) nunca
  tinha sido adicionado ao `GENERIC_SETTINGS_EXAMPLE` — só a documentação do site
  (`docs/usuario/configuracao.md`) mostrava o campo; corrigido junto.

### MT-58: Documentação do site (task-class + convenção) ✅ concluído
- **Objetivo:** `docs/usuario/configuracao.md` ganha a seção `taskClasses` (candidatos,
  preset, seleção via `--task-class`/`/task-class`, defaults sintetizados) e uma nota sobre a
  convenção autoexplicativa (ADR-0022 — por que todo bloco vem com `_comentario` + exemplos).
  `docs/usuario/uso.md` documenta a flag/comando novos.
- **Arquivos no escopo:** `docs/usuario/configuracao.md`, `docs/usuario/uso.md`.
- **Critério de aceite:** `mkdocs build --strict` continua sem warnings; releitura confirmando
  que nada na trilha de usuário ficou desatualizado.
- **Fora de escopo:** trilha de governança (nenhuma afirmação de egresso muda — task-class só
  usa candidatos com classe declarada, ADR-0002 preservado).
- **Depende de:** MT-56.
- **Notas de implementação:** a releitura ("nada na trilha de usuário ficou desatualizado")
  encontrou um gap pré-existente desde o MT-50: `--provider`/`-p` e `/provider` nunca tinham
  sido documentados nas tabelas de flags/comandos de `uso.md`, apesar de já existirem no
  binário e de `configuracao.md` já linkar para eles — corrigido junto, mesmo fora da lista
  literal de campos novos do MT-58, por estar dentro do arquivo já em escopo e diretamente
  coberto pelo próprio critério de aceite.

---

## Sequência crítica

```
MT-55 → MT-56 → MT-58
MT-55 → MT-57
```
