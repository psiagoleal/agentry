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
versionado. Só a **Fase 12** (a próxima) já está detalhada — ver `docs/roadmap-v0.6.md`.

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

## Fase 12 — Configuração completa e autoexplicativa (ADR-0021, ADR-0022)

**Objetivo:** tornar o roteamento por **task-class** configurável de ponta a ponta (hoje
hardcoded na CLI, apesar de o `Router` já suportar tudo — ADR-0008/0014) e instituir a
convenção "todo config vem com default + comentário + exemplos".

**ADRs:** ADR-0021 (schema de task-class), ADR-0022 (convenção autoexplicativa) — **escritas**.

**Detalhamento completo:** `docs/roadmap-v0.6.md` (MT-55..58).

## Fase 13 — Memória de projeto: AGENTS.md + Skills (ADR-0023)

**Objetivo:** o `agentry` passa a **ler `AGENTS.md`/`CLAUDE.md`** da raiz do projeto como
contexto de sistema (o papel do `CLAUDE.md` no Claude Code) e a carregar **`SKILL.md` por
*progressive disclosure*** (só `name`+`description` no contexto até um gatilho acionar).
Fecha o item que mantém a **ADR-0003 `Proposed`** (consumo dos artefatos do `profiles`) e dá
ao agente memória de projeto persistente.

**ADR necessária:** ADR-0023 (leitura de AGENTS.md + progressive disclosure de SKILL.md) —
*stub reservado, arquivo escrito ao iniciar a fase*. Ao concluir, ADR-0003 → `Accepted`.

- **MT-59:** loader de `AGENTS.md`/`CLAUDE.md` → mensagem de sistema no início da sessão.
- **MT-60:** descoberta de `SKILL.md` (frontmatter) + progressive disclosure.
- **MT-61:** gatilho/ativação de skill dentro do agent loop.
- **MT-62:** documentação; ADR-0003 promovida a `Accepted`.

## Fase 14 — Tools essenciais (ADR-0024, ADR-0025, ADR-0026)

**Objetivo:** aproximar o conjunto de ferramentas do Claude Code/OpenCode. Inclui o pedido
explícito do usuário por uma **tool de pergunta ao usuário** e por **web search anônimo via
SearXNG configurável**.

**ADRs necessárias** (*stubs reservados*):
- **ADR-0024** — Tool **AskUser** (pergunta/confirmação): modelo de interação via um canal
  `Prompter` injetado (mesmo padrão do `Confirmer`, `crates/cli/src/tool_executor.rs`), sem
  mudar a `trait Tool`. REPL implementa via texto; a TUI (Fase 15) via widget.
- **ADR-0025** — **Web tools** (WebFetch + WebSearch): tudo passa pelo `Transport` único
  (allowlist + auditoria + classe de egresso, ADR-0002); web é **nuvem por natureza**
  (bloqueado sob perfis restritivos sem allowlist explícita). **WebSearch via SearXNG**
  configurável (`tools.webSearch.searxngUrl`), **desabilitado até o usuário informar a URL**
  — sem instância pública hardcoded (risco de disponibilidade/cadeia de suprimentos).
  **Anonimato/segurança:** sem User-Agent/Referer identificáveis, sem cookies, sem
  parâmetros de rastreio — reduzir rastreabilidade é requisito, não opção.
- **ADR-0026** — Tools **Glob** (busca por padrão de arquivo) e **shell em background/
  streaming** (dev server/watch rodando enquanto o agente segue).

- **MT-63:** `trait Prompter` + tool **AskUser** no core.
- **MT-64:** implementação REPL/CLI do `Prompter` (texto).
- **MT-65:** tool **WebFetch** via `Transport` (classe de egresso + anonimato).
- **MT-66:** tool **WebSearch** via SearXNG + schema `tools.webSearch.searxngUrl` (desabilitado
  até configurado) + anonimato.
- **MT-67:** tool **Glob**.
- **MT-68:** **shell em background/streaming**.
- **MT-69:** documentação (usuário + governança: novo caminho de egresso web, modelo de
  anonimato).

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
