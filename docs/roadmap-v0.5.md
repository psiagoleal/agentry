<!-- Caminho relativo: docs/roadmap-v0.5.md -->

# Roadmap v0.5 — Micro-tickets

O roadmap v0.4 (`docs/roadmap-v0.4.md`, MT-43..47) está **fechado e imutável** como registro
histórico — Guardrail Gate completo (Fase 9). Este documento consome a **ADR-0006** (LiteLLM
como fonte de modelos via adapter OpenAI-compatible, já `Accepted` desde 2026-07-06, mas sem
nenhuma fiação de CLI até aqui): o adapter `OpenAiCompatProvider` já existe e é testado em
`agentry_core` (inclusive contra um endpoint simulando LiteLLM), mas `crates/cli/src/main.rs`
só constrói e registra o provider Ollama — nenhuma configuração real conecta a CLI a um
gateway LiteLLM. Motivação concreta: testar modelos maiores (30B+) atrás do gateway LiteLLM
corporativo do usuário, com uso equivalente a uma CLI de codificação de mercado.

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), dependência nova exige
ADR (ADR-0004), skill `micro-ticket-planner` para granularidade. Nenhum ADR novo é exigido
nesta fase — ADR-0006 já decide o ponto mais sensível (classe de egresso por endpoint
LiteLLM sempre explícita, ausência ⇒ tratado como `cloud-ok`/bloqueado em perfis
restritivos, nunca inferida do host); ADR-0014 já decide que `provider` é um campo de
override de rota válido, só nunca exposto por uma flag/comando real.

---

## Fase 10 — Conexão configurável com LiteLLM (ADR-0006) — ✅ concluída (MT-48..51)

### MT-48: Schema `providers.litellm` em `Settings`/`Config`
- **Objetivo:** `Settings` (`crates/core/src/config/mod.rs`) ganha `LiteLlmSettings`
  (`base_url: Option<String>`, `model: Option<String>`, `egress_class: Option<EgressClass>`
  — chave JSON `egressClass`, mesma convenção `camelCase` do resto do ADR-0018 §5) dentro de
  `ProvidersSettings.litellm`, com `merged_over` escalar (mais específico vence), mesmo
  padrão de `OllamaSettings`. `Config::resolve` expõe `litellm: Option<LiteLlmConfig>`
  (`base_url`/`model`/`egress_class` já resolvidos, não mais opcionais) — `Some` só quando
  `base_url` **e** `model` estão presentes na configuração mesclada; `egress_class` ausente
  nesse caso resolve para `EgressClass::CloudOk` (ADR-0006 "fail-closed invertido para
  proxies" — nunca tratado como `local-only` por inferência); `base_url` ou `model` ausente
  (mesmo com o outro presente) resolve `Config.litellm = None` — LiteLLM simplesmente não
  está configurado, não é um erro.
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`.
- **Critério de aceite:** testes — JSON com `providers.litellm.baseUrl`/`model`/`egressClass`
  completos resolve `Config.litellm` com os três campos exatos; mesmo JSON sem `egressClass`
  resolve `egress_class: CloudOk`; JSON com só `baseUrl` (sem `model`) ou só `model` (sem
  `baseUrl`) resolve `Config.litellm = None`; ausência do bloco `providers.litellm` inteiro
  também resolve `None` (comportamento atual preservado); camada mais específica sobrescreve
  campo a campo (mesma convenção de precedência escalar já testada para `OllamaSettings`).
- **Fora de escopo:** leitura da chave de API (nunca fica no arquivo de configuração — vem
  de variável de ambiente, MT-49); qualquer instância real de `Transport`/`Router`/provider
  (MT-49); flag/comando para selecionar o provider em tempo de execução (MT-50).
- **Depende de:** ADR-0006 · ADR-0018 (padrão de schema) · nenhum micro-ticket anterior.

### MT-49: Consumo real na CLI — instancia o provider LiteLLM e registra como candidato
- **Objetivo:** `crates/cli/src/main.rs` — quando `cfg.litellm` (MT-48) é `Some`, monta uma
  segunda instância de `Transport` (mesmo padrão de instância dedicada já usado pelo
  bootstrap `--profile`, ADR-0019) com allowlist restrita ao host de `base_url` sob
  `egress_class` resolvida; anexa `Authorization: Bearer <chave>` via `Transport::with_header`
  **só se** a variável de ambiente `AGENTRY_LITELLM_API_KEY` estiver definida — ausência não é
  erro (gateways internos sem autenticação, como o do usuário, continuam funcionando sem a
  variável); instancia `OpenAiCompatProvider` (nome fixo `"litellm"` no Router) e registra
  como **segundo candidato** da `task-class` `"chat"`, depois de Ollama na ordem de
  preferência — zero mudança de comportamento *default* para quem não configurar
  `providers.litellm`.
- **Arquivos no escopo:** `crates/cli/src/main.rs`.
- **Critério de aceite:** teste — `agentry.settings.json` com `providers.litellm` completo +
  `AGENTRY_LITELLM_API_KEY` no ambiente resolve uma `Session` real (mesmo padrão de prova do
  MT-46: registry/config real, não só unitário isolado) cujo `Router` tem os dois candidatos
  registrados (`ollama` e `litellm`); mesmo cenário sem a variável de ambiente também
  registra o candidato `litellm` (sem header de autorização); ausência do bloco
  `providers.litellm` preserva o comportamento atual (só `ollama` registrado, nenhuma
  tentativa de rede ao LiteLLM); perfil resolvendo para uma classe insuficiente para o
  candidato LiteLLM não é erro fatal na construção da sessão (o candidato só fica inelegível
  na resolução de rota — `Router::resolve_with_override`, já testado no core).
- **Fora de escopo:** seleção de qual candidato usar por padrão além da ordem de preferência
  declarada (MT-50); UI/CLI de configuração interativa; suporte a mais de um endpoint
  LiteLLM simultâneo.
- **Depende de:** MT-48.

### MT-50: Flag `--provider` e comando `/provider`
- **Objetivo:** expõe `RuntimeOverride.provider` (já existente no core desde a ADR-0014/
  MT-33, nunca ligado a uma flag/comando real) via nova flag `-p, --provider <nome>` no modo
  one-shot (`crates/cli/src/main.rs`, `Args`) e novo comando `/provider <nome>` no REPL
  (`crates/cli/src/repl.rs`, `aplicar_comando`) — mesmo padrão de `--model`/`/model`.
- **Arquivos no escopo:** `crates/cli/src/main.rs`, `crates/cli/src/repl.rs`.
- **Critério de aceite:** teste — `agentry --provider litellm --model <modelo-no-litellm>
  "tarefa"`, com os dois candidatos registrados (MT-49), resolve a rota no candidato
  `litellm`, não `ollama`; `/provider <nome>` no REPL troca o candidato ativo a partir da
  próxima mensagem, preservando o histórico da conversa (mesmo padrão de teste já usado para
  `/model`); nome de provider inexistente é o mesmo erro tratado que
  `Router::resolve_with_override` já devolve para candidato inexistente (reaproveitado, não
  duplicado).
- **Fora de escopo:** validação antecipada de que o nome existe antes de tentar resolver a
  rota (o erro de resolução já cobre isso).
- **Depende de:** MT-49.

### MT-51: Atualizar a documentação do site (usuário + governança) — ✅ concluído (`9c5e495`)
- **Objetivo:** revisar `docs/usuario/configuracao.md`/`uso.md` (novo bloco
  `providers.litellm`, flag `--provider`/comando `/provider`) e, principalmente,
  `docs/governanca/privacidade-e-egresso.md` — a afirmação atual ("nenhum destino de rede,
  além do Ollama local, para o qual esse tipo de conteúdo possa ser enviado") deixa de ser
  verdade assim que um endpoint LiteLLM estiver configurado; a seção precisa descrever o
  novo caminho de rede possível, sob qual classe de egresso, e reafirmar que o *default* sem
  configuração continua sendo só o Ollama local. `docs/governanca/dependencias.md` e
  `docs/governanca/auditoria.md` não devem precisar mudar (nenhuma dependência nova; o
  mesmo par allowlist+audit já documentado se aplica ao segundo `Transport`).
- **Arquivos no escopo:** `docs/usuario/configuracao.md`, `docs/usuario/uso.md`,
  `docs/governanca/privacidade-e-egresso.md`.
- **Critério de aceite:** `mkdocs build --strict` continua sem warnings; releitura manual
  confirmando que nenhuma afirmação da trilha de governança ficou desatualizada pela
  mudança.
- **Fora de escopo:** qualquer trilha nova; tradução para outro idioma.
- **Depende de:** MT-50 (para documentar o comportamento final, não um estado intermediário).

---

## Fase 11 — `.agentryignore` e respeito opcional a `.gitignore` (ADR-0020) — ✅ concluída (MT-52..54)

Discussão do usuário durante a Fase 10: `.claudeignore` (já usado por `fs`/`repo_map`/
`code_search`) resolve confidencialidade (independente do versionamento), mas nada resolve
ruído de contexto por artefatos já cobertos por `.gitignore`. Pesquisa de mercado (ver
ADR-0020): `.claudeignore` não é um recurso real do Claude Code (convenção mal-atribuída,
espalhada por documentação gerada por IA); o `OpenCode` real resolve os dois lados —
`.gitignore` respeitado por padrão nas tools de busca, mais um arquivo próprio nativo
(`.opencodeignore`) para exclusões específicas do agente. ADR-0020 decide: renomear para
`.agentryignore` (artefato próprio do `agentry`, não mais do contrato de interop v1) com
*fallback* de compatibilidade para `.claudeignore`; nova opção `context.gitignore.enabled`
(*default* `false`) para também respeitar `.gitignore`, em união com
`.agentryignore`/`.claudeignore`, nunca substituindo. Escopo só do lado `agentry` — o
`ai-coding-agent-profiles` continua distribuindo `.claudeignore` sem mudança por enquanto
(migração daquele lado é item futuro).

### MT-52: Renomear para `.agentryignore` com fallback de compatibilidade — ✅ concluído (`d742265`)
- **Objetivo:** as três tools que já reconhecem `.claudeignore` (`fs`, `repo_map`,
  `code_search`) passam a procurar `.agentryignore` primeiro; se ausente, caem para
  `.claudeignore` (sem erro — comportamento atual preservado). Se os dois existirem no
  mesmo projeto, `.agentryignore` vence **sozinho** — nunca um merge dos dois padrões
  (ADR-0020 §2).
- **Arquivos no escopo:** `crates/core/src/tools/fs.rs`, `crates/core/src/tools/repo_map.rs`,
  `crates/core/src/tools/code_search.rs`.
- **Critério de aceite:** testes — projeto só com `.agentryignore` funciona; projeto só com
  `.claudeignore` (legado) ainda funciona via *fallback*; projeto com os dois presentes usa
  só `.agentryignore` (arquivo coberto só por `.claudeignore` nesse cenário **não** fica
  ignorado); ausência dos dois não é erro, nada ignorado (comportamento atual preservado).
- **Fora de escopo:** opção de respeitar `.gitignore` (MT-53); qualquer mudança no
  `ai-coding-agent-profiles` (ADR-0020 §5, item futuro separado).
- **Depende de:** ADR-0020.

### MT-53: Schema `context.gitignore` — respeito opcional a `.gitignore` — ✅ concluído (`3bbd934`/`6151e26`)
- **Objetivo:** `Settings.context.gitignore: FeatureToggle` (mesmo padrão de
  `repoMap`/`semanticRag`/`lspGrounding`, ADR-0018), *default* `false`; `Config` expõe o
  booleano resolvido. Quando `true`, as três tools passam a excluir **também** o que
  `.gitignore` cobre — `GitignoreBuilder::add(".gitignore")` em `fs.rs` (só a raiz, mesma
  assimetria pré-existente dessas tools com `.agentryignore`); `WalkBuilder::git_ignore(true)`
  em `repo_map`/`code_search` (`.gitignore` aninhado por subdiretório, suporte nativo da
  crate `ignore`) — sempre em **união** com `.agentryignore`/`.claudeignore` (MT-52), nunca
  em substituição.
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`, `crates/core/src/tools/fs.rs`,
  `crates/core/src/tools/repo_map.rs`, `crates/core/src/tools/code_search.rs`,
  `crates/cli/src/main.rs` (fiação da flag resolvida até as tools, se necessário).
- **Critério de aceite:** testes — flag desligada (*default*) preserva o comportamento
  atual (arquivo só coberto por `.gitignore`, não por `.agentryignore`, continua visível);
  flag ligada exclui um arquivo coberto só por `.gitignore`; flag ligada com arquivo coberto
  pelos dois continua excluído (união, sem conflito); `repo_map`/`code_search` respeitam
  `.gitignore` aninhado por subdiretório quando ligada.
- **Fora de escopo:** migração do lado `profiles`; UI/CLI dedicada além do
  `agentry.settings.json`.
- **Depende de:** MT-52.

### MT-54: Documentação do site (usuário + governança) — ✅ concluído (`a13eb98`)
- **Objetivo:** `docs/usuario/configuracao.md` ganha o bloco `context.gitignore` e a
  explicação de `.agentryignore`/*fallback* `.claudeignore`; revisão da trilha de
  governança deixando explícito que confidencialidade (`.agentryignore`, independente do
  versionamento) e redução de ruído de contexto (`context.gitignore.enabled`, opt-in) são
  mecanismos distintos, não confundir um pelo outro.
- **Arquivos no escopo:** `docs/usuario/configuracao.md`, `docs/governanca/*.md` (o que se
  aplicar).
- **Critério de aceite:** `mkdocs build --strict` continua sem warnings.
- **Fora de escopo:** qualquer trilha nova; tradução para outro idioma.
- **Depende de:** MT-53.

---

## Sequência crítica

```
MT-48 → MT-49 → MT-50 → MT-51
MT-52 → MT-53 → MT-54
```
