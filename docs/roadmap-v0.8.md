<!-- Caminho relativo: docs/roadmap-v0.8.md -->

# Roadmap v0.8 — Micro-tickets

O roadmap v0.7 (`docs/roadmap-v0.7.md`) cobre a Fase 13 (memória de projeto — `AGENTS.md` +
skills, ADR-0023, **concluída**). Este documento detalha a **Fase 14** do roadmap de longo
prazo (`docs/roadmap-longo-prazo.md`): tools essenciais — `AskUser` (ADR-0024), `WebFetch`/
`WebSearch` via SearXNG (ADR-0025), `Glob` e shell em background (ADR-0026).

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), dependência nova exige
ADR (ADR-0004), skill `micro-ticket-planner` para granularidade. **Nenhuma dependência nova
nesta fase** — decisão explícita das três ADRs (reaproveitam `ignore`/`tokio::process`/
`reqwest`/`serde_json`, todos já presentes).

---

## Fase 14 — Tools essenciais (ADR-0024, ADR-0025, ADR-0026)

### MT-63: `trait Prompter` + tool `ask_user` no core ✅ concluído
- **Objetivo:** `crates/core/src/tools/ask_user.rs` (novo): `trait Prompter` (dyn-compatible
  via `BoxFuture`, `fn ask(&self, question: &str, options: &[String]) -> BoxFuture<'_, String>`)
  + `AskUserTool` implementando `Tool` (mesma trait do MT-11) sobre um `Arc<dyn Prompter>`
  injetado — `execute()` lê `question`/`options` dos argumentos e devolve a resposta do
  `Prompter` como `ToolOutput::ok`. `question` ausente é `ToolOutput::error` tratado.
- **Arquivos no escopo:** `crates/core/src/tools/ask_user.rs` (novo),
  `crates/core/src/tools/mod.rs` (declaração do módulo).
- **Critério de aceite:** testes — `Prompter` de teste (resposta fixa) faz `execute()` devolver
  exatamente essa resposta; `options` vazio funciona (só `question` obrigatório); argumento
  `question` ausente é erro tratado, sem *panic*; tool respeita `deny`/`ask` do
  `PermissionGate` como qualquer outra (teste via `ToolRegistry::execute`, mesmo padrão do
  MT-61).
- **Fora de escopo:** implementação real do `Prompter` (lê `stdin`, MT-64); `Prompter` de TUI
  (Fase 15).
- **Depende de:** ADR-0024 · ADR/MT-11 (`trait Tool`).

### MT-64: `InteractivePrompter` (CLI) + registro real ✅ concluído
- **Objetivo:** `crates/cli/src/tool_executor.rs` ganha `InteractivePrompter` implementando
  `Prompter` (imprime a pergunta e, se houver, as sugestões numeradas; lê uma linha de
  `stdin`; devolve o texto como veio — mesmo padrão síncrono de `InteractiveConfirmer`, sem
  parsing/validação). `crates/cli/src/main.rs` registra `AskUserTool::new(Arc::new(
  InteractivePrompter))` no `ToolRegistry`, junto das demais tools sempre registradas
  (`fs_read`/`fs_write`/.../`skill`).
- **Arquivos no escopo:** `crates/cli/src/tool_executor.rs`, `crates/cli/src/main.rs`.
- **Critério de aceite:** smoke-test manual do binário real — tarefa que induz o modelo a usar
  `ask_user`, resposta digitada via `stdin` chega de volta ao agente (confirmar via a resposta
  final da tarefa referenciando o que foi digitado). Testes automatizados cobrem só a
  formatação da pergunta/sugestões (sem depender de `stdin` real em CI, mesmo padrão de
  `InteractiveConfirmer`, que também não tem teste automatizado de E/S real).
- **Depende de:** MT-63.
- **Nota de implementação:** o smoke-test de indução via linguagem natural reproduziu a mesma
  limitação já registrada no MT-61 — o modelo local disponível (`llama3.1:8b`) não emitiu uma
  *tool-call* real para `ask_user`, respondendo em texto solto em vez de rodar a tool. Não é
  regressão desta ticket (o mesmo já foi observado com `fs_read`, tool madura); a correção do
  `InteractivePrompter` fica coberta por ser estruturalmente idêntica ao
  `InteractiveConfirmer` (mesmo padrão `std::io::stdin().read_line()`, já em produção desde o
  MT-14) somada aos 5 testes de `AskUserTool` no core (MT-63) + os 2 novos de formatação aqui.

### MT-65: Tool `WebFetch` + coringa `"*"` na `Allowlist` ✅ concluído
- **Objetivo:** `crates/core/src/egress/allowlist.rs`: `AllowlistEntry::matches` reconhece o
  padrão coringa `"*"` (casa qualquer host — terceiro caso, distinto de exato e `*.sufixo`).
  Novo `crates/core/src/tools/web_fetch.rs`: `WebFetchTool` faz `GET` via `Transport` (nunca
  `reqwest` direto), com `User-Agent` genérico fixo (`Transport::with_header`, nunca o
  *default* da *crate*), corpo da resposta devolvido como texto truncado a um teto (sem
  conversão HTML→Markdown, ADR-0025). Novo campo `tools.webFetch.enabled` (`FeatureToggle`,
  ADR-0018, *default* `false`) em `Settings`/`Config`. `crates/cli/src/main.rs`: `Transport`
  dedicado com a entrada coringa exigindo `EgressClass::CloudOk` (mesmo padrão de instância
  própria do LiteLLM/`--profile`); tool só registrada quando `tools.webFetch.enabled=true`
  **e** `cfg.egress_class == CloudOk`.
- **Arquivos no escopo:** `crates/core/src/egress/allowlist.rs`,
  `crates/core/src/tools/web_fetch.rs` (novo), `crates/core/src/config/mod.rs`,
  `crates/cli/src/main.rs`.
- **Critério de aceite:** testes — entrada `"*"` casa qualquer host, mas só libera sob a
  classe declarada (`CloudOk`) — classe insuficiente continua bloqueando mesmo com o coringa
  presente (fail-closed preservado); `WebFetchTool` busca via `Transport` mock e trunca corpo
  grande a um teto; `User-Agent` correto chega à requisição; tool **não** registrada sob
  `local-only`/`cloud-opt-out` mesmo com `tools.webFetch.enabled=true`; tool **não**
  registrada com a flag desligada mesmo sob `cloud-ok`.
- **Fora de escopo:** `WebSearch`/SearXNG (MT-66); conversão HTML→Markdown (dependência nova,
  fora de escopo desta ADR).
- **Depende de:** ADR-0025.
- **Nota de implementação:** smoke-test manual confirmou a fiação real (perfil `pessoal`
  resolve `cloud-ok`, audit log mostra `(cloud-ok, allowed)`), mas reproduziu a mesma
  limitação de confiabilidade de *tool-calling* dos modelos locais já registrada no MT-61/64 —
  não é regressão desta ticket; correção coberta com confiança pelos 14 testes automatizados.

### MT-66: Tool `WebSearch` via SearXNG configurável
- **Objetivo:** novo bloco `tools.webSearch` (`searxngUrl`, `searxngEgressClass`) em
  `Settings`/`Config`, mesmo padrão de `providers.litellm` (ausência de `searxngUrl` ⇒ tool
  não registrada; `searxngEgressClass` ausente ⇒ `cloud-ok`, ADR-0002 fail-closed). Novo
  `crates/core/src/tools/web_search.rs`: `WebSearchTool` consulta
  `<searxngUrl>/search?q=<query>&format=json` via `Transport` (host único, `AllowlistEntry`
  normal — **sem** o coringa do MT-65), `User-Agent` genérico igual ao `WebFetch`, resposta
  JSON (`serde_json`) reformatada em texto (título/URL/resumo por resultado, capado a um
  teto). `crates/cli/src/main.rs`: `Transport` dedicado (mesmo padrão de
  `build_litellm_provider`), tool registrada só quando `searxngUrl` presente.
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`,
  `crates/core/src/tools/web_search.rs` (novo), `crates/cli/src/main.rs`.
- **Critério de aceite:** testes — ausência de `searxngUrl` preserva comportamento atual
  (tool não registrada); `searxngEgressClass` ausente resolve `cloud-ok`; resposta JSON válida
  do SearXNG (fixture) formatada corretamente; resposta malformada é erro tratado, sem
  *panic*; `User-Agent` correto; query string carrega só `q`/`format` (sem parâmetro de
  rastreio).
- **Fora de escopo:** UI de configuração interativa; *ranking*/reordenação dos resultados
  (devolvidos na ordem que o SearXNG já devolve).
- **Depende de:** ADR-0025 · MT-65 (reaproveita o padrão de `User-Agent`/`Transport`
  dedicado, mesmo sem depender do coringa).

### MT-67: Tool `glob`
- **Objetivo:** novo `crates/core/src/tools/glob.rs`: `GlobTool` recebe um padrão glob
  (`"**/*.rs"`) e devolve os caminhos que casam, via `ignore::overrides::OverrideBuilder` +
  `ignore::WalkBuilder` (mesma *crate* já usada por `tools::fs`/`repo_map`/`code_search` —
  **nenhuma dependência nova**), respeitando `.agentryignore`/`.claudeignore`/
  `context.gitignore.enabled` como qualquer tool de filesystem; resultado capado a um teto de
  itens (mesmo espírito de `MAX_RESULTADOS`, `repo_map.rs`, MT-21).
- **Arquivos no escopo:** `crates/core/src/tools/glob.rs` (novo),
  `crates/core/src/tools/mod.rs`, `crates/cli/src/main.rs` (registro, sempre ativa — mesma
  categoria de custo baixo de `fs_read`, sem *toggle* próprio).
- **Critério de aceite:** testes — padrão casa exatamente os arquivos esperados num diretório
  de teste; arquivo coberto por `.agentryignore` nunca aparece no resultado; padrão sem
  nenhuma correspondência não é erro (lista vazia); resultado capado ao teto configurado.
- **Fora de escopo:** busca por conteúdo (já existe, `fs_search`/MT-12); *ranking* de
  relevância (já existe, `repo_map`/MT-21).
- **Depende de:** ADR-0026.

### MT-68: Shell em background/streaming (`shell_background`)
- **Objetivo:** extensão de `crates/core/src/tools/shell.rs` (MT-13, não uma política
  paralela): nova tool `shell_background` com campo `action` (`"start"`/`"output"`/`"stop"`),
  sob a **mesma** `ShellPolicy`/checagem *default-deny* de comando já usada por `ShellTool` —
  rodar em segundo plano nunca contorna a política. `start` spawna via `tokio::process` (já
  dependência) sem esperar terminar, devolve um `id`; uma tarefa em segundo plano acumula
  `stdout`/`stderr` num buffer compartilhado **truncado** a um teto de tamanho; `output` lê o
  acumulado desde a última consulta, sem bloquear esperando o processo terminar; `stop` mata o
  processo (mesmo padrão de `kill`/`wait` do `Drop` do `LspClient`, MT-23). Processos
  esquecidos são finalizados quando o processo `agentry` termina (mesma rede de segurança do
  `LspClient`).
- **Arquivos no escopo:** `crates/core/src/tools/shell.rs`, `crates/cli/src/main.rs`
  (registro).
- **Critério de aceite:** testes — `start` de um comando bloqueado por `ShellPolicy` continua
  negado, sem sequer spawnar; `start` de um comando permitido devolve um `id`; `output`
  devolve o texto acumulado sem bloquear (comando de longa duração real em teste, ex.: um
  `sleep` curto + `echo` periódico); `stop` mata o processo de fato (verificação real, mesmo
  padrão do `processo_existe`/`kill -0` usado nos testes do `LspClient`); buffer respeita o
  teto de tamanho com saída maior que ele.
- **Fora de escopo:** múltiplos processos nomeados/agrupados (um `id` por `start`, suficiente
  para o caso de uso — `dev server`/`watch` único por vez).
- **Depende de:** ADR-0026 · MT-13 (`ShellPolicy`/`CommandRunner`).

### MT-69: Documentação (usuário + governança)
- **Objetivo:** `docs/usuario/configuracao.md` ganha as seções `tools.webFetch`/
  `tools.webSearch` (o que fazem, *defaults*, como habilitar) e uma nota sobre `ask_user`/
  `glob`/`shell_background` em `docs/usuario/uso.md` (comportamento observável, não muda
  flags/comandos de invocação). `docs/governanca/privacidade-e-egresso.md` ganha uma seção
  sobre o novo caminho de egresso web (`WebFetch` só sob `cloud-ok` + *opt-in*; `WebSearch`
  como qualquer outro endpoint externo configurado) e o modelo de anonimato (sem cookies,
  `User-Agent` genérico, sem `Referer`/rastreio) — relevante para quem avalia o software para
  aceite interno em empresa. ADR-0024/0025/0026 promovidas a `Accepted` (MT-63..68
  concluídos).
- **Arquivos no escopo:** `docs/usuario/configuracao.md`, `docs/usuario/uso.md`,
  `docs/governanca/privacidade-e-egresso.md`, `docs/adr/0024-*.md`/`0025-*.md`/`0026-*.md`
  (status), `docs/adr/README.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo; releitura confirmando que nada nas
  trilhas de usuário/governança ficou desatualizado.
- **Fora de escopo:** nenhum código novo.
- **Depende de:** MT-63..68 (todos).

---

## Sequência crítica

```
MT-63 → MT-64 ─┐
MT-65 → MT-66 ─┼→ MT-69
MT-67 ─────────┤
MT-68 ─────────┘
```

MT-63/64 (AskUser), MT-65/66 (web), MT-67 (glob) e MT-68 (shell background) são independentes
entre si — podem ser feitos em qualquer ordem relativa (a numeração reflete só a ordem em que
foram detalhados). MT-69 depende de todos os seis anteriores estarem concluídos.
