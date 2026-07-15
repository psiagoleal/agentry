<!-- Caminho relativo: docs/roadmap-longo-prazo.md -->

# Roadmap de longo prazo — rumo à paridade com Claude Code CLI / OpenCode

Este é o **mapa de visão** do `agentry` a médio/longo prazo. Ele **supersede** o esboço de
roadmap (v0.2/v0.3) que estava em `docs/architecture.md` §Roadmap — aquele previa Skills
Loader, MCP, TUI e memória mas nunca foi detalhado, e o roadmap real (Fases 1–10) foi
capturado por necessidades concretas que surgiram (RAG, LSP, Reviewer, guardrails, LiteLLM).

Cada fase abaixo lista **objetivo**, **ADR(s) necessária(s)** e a **primeira leva de
micro-tickets** (título + objetivo de uma linha). Seguindo a disciplina do projeto
(`skill adr-writer` / `micro-ticket-planner`): a **ADR completa e os tickets detalhados de
cada fase são escritos quando a fase começa**, promovidos para um `roadmap-vX.Y.md`
versionado. **Fases 12, 13 e 14** estão concluídas (`docs/roadmap-v0.6.md`,
`docs/roadmap-v0.7.md`, `docs/roadmap-v0.8.md`). **Fases 15 e 16** (a próximas) exigem
dependência de runtime nova (`ratatui`/`rmcp`) — parada dura do comando de loop; preparar
essas fases (mesmo só para escrever a ADR, que decidiria adotar a dependência) exige decisão
do mantenedor antes de qualquer trabalho autônomo continuar.

> Convenções de DoD, granularidade e "dependência nova exige ADR (ADR-0004)": iguais às dos
> roadmaps versionados (`docs/roadmap-v0.1.md` §Convenções).

## Execução autônoma (loop)

Este roadmap pode ser executado de forma **autônoma** pelo comando
`.claude/commands/implementar-roadmap.md` (via `/loop`): uma unidade de trabalho por
iteração, retomável após interrupção, com **paradas de segurança** (dependência nova,
repo irmão, qualquer afrouxamento de egresso ⇒ escala ao usuário) e **registro de toda
decisão-sob-dúvida** em [`docs/decisoes-autonomas.md`](./decisoes-autonomas.md) para revisão
posterior. Fases sem tickets detalhados (13+) são primeiro **preparadas** (ADR + quebra em
micro-tickets) antes de implementadas.

## Sequência das fases

```
Fase 11 → Fase 12 → Fase 13 → Fase 14 → Fase 15 → Fase 16 → Fase 17+
(ignore)  (config)   (memória)  (tools)   (TUI)     (MCP)     (2ª onda)
```

Prioridade escolhida: **fundamentos antes das vitrines** — configuração e memória de projeto
maduras por baixo antes de investir em Tools de destaque e TUI.

---

## Fase 11 — `.agentryignore` + `.gitignore` opcional (ADR-0020)

**Já planejada** em `docs/roadmap-v0.5.md` (MT-52/53/54). Renomeia `.claudeignore` →
`.agentryignore` (com *fallback* de compatibilidade) e adiciona `context.gitignore.enabled`
(opt-in) para reduzir ruído de contexto. Fora do escopo de re-detalhamento aqui.

## Fase 12 — Configuração completa e autoexplicativa (ADR-0021, ADR-0022) ✅ concluída

**Objetivo:** tornar o roteamento por **task-class** configurável de ponta a ponta (hoje
hardcoded na CLI, apesar de o `Router` já suportar tudo — ADR-0008/0014) e instituir a
convenção "todo config vem com default + comentário + exemplos".

**ADRs:** ADR-0021 (schema de task-class), ADR-0022 (convenção autoexplicativa) — **escritas**.

**Detalhamento completo:** `docs/roadmap-v0.6.md` (MT-55..58, **concluídos**).

## Fase 13 — Memória de projeto: AGENTS.md + Skills (ADR-0023) ✅ concluída

**Objetivo:** o `agentry` passa a **ler `AGENTS.md`/`CLAUDE.md`** da raiz do projeto como
contexto de sistema (o papel do `CLAUDE.md` no Claude Code) e a carregar **`SKILL.md` por
*progressive disclosure*** (só `name`+`description` no contexto até um gatilho acionar).
Fecha o item que mantém a **ADR-0003 `Proposed`** (consumo dos artefatos do `profiles`) e dá
ao agente memória de projeto persistente.

**ADR:** ADR-0023 (leitura de AGENTS.md + progressive disclosure de SKILL.md) — **escrita**.
Decisões centrais: `AGENTS.md` primário / `CLAUDE.md` *fallback* (nunca merge, mesma
precedência do ADR-0020); ambos concatenados numa única mensagem de sistema junto do preset
da `task-class`; `.claude/skills/*/SKILL.md` reaproveitado verbatim (compatibilidade direta
com convenção já existente do Claude Code); frontmatter via **parser próprio, sem dependência
YAML nova** (decisão registrada em `docs/decisoes-autonomas.md`); skill completa carregada só
sob demanda via nova tool `skill`. Ao concluir, ADR-0003 → `Accepted`.

**Detalhamento completo:** `docs/roadmap-v0.7.md` (MT-59..62, **concluídos**). ADR-0003 e
ADR-0023 ambas `Accepted`.

## Fase 14 — Tools essenciais (ADR-0024, ADR-0025, ADR-0026) ✅ concluída

**Objetivo:** aproximar o conjunto de ferramentas do Claude Code/OpenCode. Inclui o pedido
explícito do usuário por uma **tool de pergunta ao usuário** e por **web search anônimo via
SearXNG configurável**.

**ADRs** — todas **escritas** (`Proposed`):
- **ADR-0024** — Tool **AskUser**: `trait Prompter` definido no `core` (padrão `AuditSink`,
  não o padrão `Confirmer` — que é tipo só da CLI; ver a ADR para o racional), implementação
  concreta (`InteractivePrompter`) na CLI. Escopo mínimo: texto livre + sugestões opcionais,
  sem seleção múltipla/*preview*.
- **ADR-0025** — **Web tools**: `WebFetch` (URL arbitrária) exige um coringa novo (`"*"`) na
  `Allowlist` (MT-05), liberado só sob `EgressClass::CloudOk` **e** `tools.webFetch.enabled`
  (*opt-in* explícito, *default* `false`); `WebSearch` via SearXNG usa o modelo de allowlist
  já existente (host único, como o LiteLLM) — `tools.webSearch.searxngUrl`/
  `searxngEgressClass`, **desabilitado até o usuário informar a URL**, sem instância pública
  *hardcoded*. Anonimato como requisito de código: sem cookies (já garantido pela config atual
  do `reqwest`), `User-Agent` genérico fixo, sem `Referer`/parâmetro de rastreio.
  HTML→Markdown fica fora de escopo (exigiria *parser* de HTML, dependência nova).
- **ADR-0026** — Tools **Glob** (via `ignore::overrides`, já dependência — zero nova) e
  **shell em background/streaming** (extensão de `ShellPolicy`/MT-13, nunca uma política
  paralela; `tokio::process`, já dependência).

**Nenhuma dependência nova nesta fase** (as três ADRs decidem isso explicitamente).

**Detalhamento completo:** `docs/roadmap-v0.8.md` (MT-63..69, **concluídos**). ADR-0024/0025/
0026 todas `Accepted`.

## Fase 15 — TUI (ADR-0027) — *tema 2 do usuário*

**Objetivo:** substituir o REPL de texto puro por uma TUI com `ratatui`. Referência de UX é o
OpenCode (pointers concretos anotados em memória — keybind flat map, model picker fuzzy, diff
modal, permission toggle, todo widget), **não** seu modelo de segurança (o `agentry` é mais
estrito, ADR-0002).

**ADR necessária:** ADR-0027 (adoção de `ratatui` — vetada por maturidade/licença, ADR-0004 —
+ arquitetura da TUI) — *stub reservado*.

- **MT-70:** scaffold `ratatui` + event loop.
- **MT-71:** tabela de keybindings (flat map, estilo OpenCode).
- **MT-72:** view de chat com streaming.
- **MT-73:** seletor de modelo/provider (fuzzy).
- **MT-74:** widgets de permissão + AskUser (implementação TUI do `Prompter` da Fase 14).
- **MT-75:** visualizador de diff modal (para confirmação de `fs_write`/`fs_edit`).
- **MT-76:** todo widget (lista de tarefas visível).
- **MT-77:** documentação.

## Fase 16 — MCP client (ADR-0028)

**Objetivo:** interoperar com o ecossistema MCP — qualquer servidor MCP existente passa a
funcionar no `agentry` (o maior efeito de rede possível). Via `rmcp` (SDK oficial Rust).

**ADR necessária:** ADR-0028 (MCP client via `rmcp` — dependência sob ADR-0004; servidores
configuráveis; tools MCP sob o mesmo gate de permissão + classe de egresso; progressive
disclosure de tools) — *stub reservado*.

- **MT-78:** adoção `rmcp` + config de servidores MCP.
- **MT-79:** tools MCP no `ToolRegistry` sob o gate de permissão.
- **MT-80:** classificação de egresso por servidor MCP (ADR-0002).
- **MT-81:** documentação.

## Fase 17+ — Segunda onda (ADRs 0029+ quando alcançadas)

Enumeradas; *stubs de ADR adiados* — cada uma ganha ADR e detalhamento quando chegar a vez:

- **Memória entre sessões** (padrão LLM-Wiki/OKF, ADR-0004(c)) — hoje só há compactação
  *dentro* de uma sessão (ADR-0016); nada persiste conhecimento entre sessões/dias.
- **Subagentes / orquestração** dentro do `agentry` (equivalente ao `Task` do Claude Code /
  árvore de sessão do OpenCode). **Decisão-chave da futura ADR:** um subagente herda a classe
  de egresso da sessão-mãe ou pode ter a própria? (implicação direta em ADR-0002).
- **Multimodal** — `ContentBlock::Image` (`crates/core/src/model/mod.rs` só tem
  Text/ToolCall/ToolResult hoje); aceitar screenshot/imagem como entrada.
- **Checkpoints / undo** de mudanças de arquivo feitas pelo agente (equivalente ao "rewind").
- **Custo / uso visível** durante a sessão — `Usage` já é rastreado internamente
  (`crates/core/src/model/mod.rs`), falta expor consumo/custo ao usuário.

---

## Faixa de ADRs reservada

ADR-0021 e ADR-0022 **escritas** (Fase 12). ADR-0023..0028 **reservadas** (números fixados
aqui; arquivo de cada uma escrito ao iniciar sua fase, com contexto fresco). ADR-0029+
para a segunda onda (Fase 17+), sem número fixado ainda.
