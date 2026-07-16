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
versionado. **Fases 12, 13, 14, 15, 16, 17 e 18** estão concluídas (`docs/roadmap-v0.6.md`,
`docs/roadmap-v0.7.md`, `docs/roadmap-v0.8.md`, `docs/roadmap-v0.9.md` — `ratatui` autorizado
pelo mantenedor em 2026-07-15, após a parada dura do comando de loop por dependência nova;
ADR-0027 `Accepted` — `docs/roadmap-v0.10.md`, `rmcp` pré-autorizado pelo mantenedor junto de
`ratatui`; ADR-0028 `Accepted`, escopo v1 restrito a servidores MCP locais — `docs/roadmap-v0.11.md`,
ADR-0029 `Accepted`: uso de tokens visível durante a sessão, primeira das cinco frentes de
"segunda onda" — e `docs/roadmap-v0.12.md`, ADR-0030 `Accepted`: checkpoints e *undo* de
mudanças de arquivo, segunda das cinco frentes de "segunda onda"). Das três frentes
restantes, todas levantavam pergunta de design/segurança sem opção recomendada óbvia — o
mantenedor foi consultado diretamente (2026-07-16, ver `docs/decisoes-autonomas.md`) e
escolheu **subagentes/orquestração** para vir primeiro. **Fase 19** já está detalhada — ver
`docs/roadmap-v0.13.md` (ADR-0031, `Proposed`). Pronta para começar a implementação a partir
do MT-90. As outras duas frentes (memória entre sessões, multimodal) seguem sem tickets
detalhados, mas já com a pergunta de design de cada uma respondida (ver Fase 20+ abaixo).

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
Fase 11 → Fase 12 → Fase 13 → Fase 14 → Fase 15 → Fase 16 → Fase 17 → Fase 18       → Fase 19        → Fase 20+
(ignore)  (config)   (memória)  (tools)   (TUI)     (MCP)     (uso)     (checkpoints)  (subagentes)     (2ª onda, restante)
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

## Fase 15 — TUI via `ratatui` (ADR-0027) — *tema 2 do usuário* ✅ concluída

**Objetivo:** modo TUI **opt-in** (`--tui`) — não substitui o REPL de texto (aditivo, zero
risco de regressão no caminho existente). Referência de UX é o OpenCode (*pointers* concretos
anotados em memória — *keybind* mapa único, seletor de modelo *fuzzy*, diff modal, *toggle*
de permissão), **não** seu modelo de segurança (o `agentry` é mais estrito, ADR-0002).

**ADR:** ADR-0027 (adoção de `ratatui`+`crossterm`, autorizada pelo mantenedor — maturidade
verificada: MIT, 37,9M downloads, ativo desde 2023 — + arquitetura da TUI) — **escrita**,
`Accepted`. Decisões centrais: `ratatui`/`crossterm` só em `crates/cli` (nunca no `core`);
`Session::run_streaming` (*callback* já genérico, MT-10) roda numa *task* separada enviando
`StreamEvent`s por canal ao laço de eventos — **zero mudança no `core`**; `TuiConfirmer`/
`TuiPrompter` implementam as *traits* já existentes (`Confirmer`/`Prompter`, ADR-0024) — prova
que a fronteira de trait desenhada na Fase 14 generaliza de verdade; *toggle* de permissão
`auto`/`normal` nunca contorna um `deny`, só acelera a confirmação de um `ask`.

**Detalhamento completo:** `docs/roadmap-v0.9.md` (MT-70..76, **concluídos** — *widget* de
lista de tarefas deliberadamente fora de escopo, YAGNI: `agentry` não tem esse conceito no
`core` hoje). Scaffold `ratatui`/*keybindings*/*streaming* real/seletor de modelo/widgets de
permissão e pergunta/diff modal, todos entregues; documentação de usuário fechando a fase.

## Fase 16 — Cliente MCP via `rmcp` (ADR-0028) ✅ concluída

**Objetivo:** interoperar com o ecossistema MCP — qualquer servidor MCP **local** (subprocesso,
`stdio`) existente passa a funcionar no `agentry` como um conjunto de tools comuns, sob o
mesmo `ToolRegistry`/`PermissionGate` de sempre. Via `rmcp` (SDK oficial Rust, mantido pela
própria organização do protocolo).

**ADR:** ADR-0028 (adoção de `rmcp` — só as *features* `client`+`transport-child-process` em
produção, maturidade verificada: Apache-2.0, 15,9M downloads, repositório oficial
`modelcontextprotocol/rust-sdk`, ativo) — **escrita**, `Accepted`. Decisão central: **v1 só
suporta servidores MCP locais** — servidores remotos (HTTP/SSE) exigiriam o cliente HTTP
embutido do `rmcp`, que faria chamadas de rede **fora** do `Transport` único do projeto
(ADR-0001), sem `Allowlist`/`EgressClass`/auditoria — ficam explicitamente fora de escopo até
uma fase dedicada resolver essa integração, nunca implementados via atalho. Servidor local é o
mesmo modelo de confiança já aceito para `LspClient` (subprocesso, IPC via `pipe`, não é uma
chamada de rede mediada pelo `agentry`). Tools MCP entram no `ToolRegistry` com nome prefixado
pelo servidor de origem (`"<servidor>__<tool>"`), sob o mesmo gate de permissão de qualquer
outra tool — nenhum mecanismo paralelo.

**Detalhamento completo:** `docs/roadmap-v0.10.md` (MT-77..81, **concluídos** — a numeração
retoma do MT-77, que ficou livre quando o *widget* de lista de tarefas foi descartado ainda na
preparação da Fase 15, YAGNI/ADR-0027). Cliente MCP (`McpClient`, `crates/core/src/mcp/`),
tools MCP no `ToolRegistry` (`crates/core/src/tools/mcp.rs`) com defesa em profundidade de
egresso (`McpClient::start_from_settings`), documentação de usuário e governança fechando a
fase.

## Fase 17 — Uso de tokens visível durante a sessão (ADR-0029) ✅ concluída

**Objetivo:** expor ao usuário quantos tokens uma sessão consumiu — `Usage`
(`crates/core/src/model/mod.rs`) já é calculado por turno, só não é acumulado nem exibido.
Primeira das cinco frentes de "segunda onda" a ser preparada (ordem escolhida e registrada em
`docs/decisoes-autonomas.md`, 2026-07-16, por ser a única sem pergunta de segurança/
confidencialidade/egresso em aberto).

**ADR:** ADR-0029 — **escrita**, `Accepted`. Decisão central: `Session` acumula `Usage` ao
longo da sessão (nenhum tipo novo, só soma o que já existe por turno); exposto em três
pontos — resumo em `stderr` no modo *one-shot*, comando `/usage` no REPL, rodapé da TUI.
Contador **não persiste entre sessões** (fora de escopo — pertence à frente "memória entre
sessões" abaixo, se/quando essa decidir como persistência funciona). Custo em dinheiro fica
deliberadamente fora de escopo (exigiria tabela de preço configurável, não é dado intrínseco
ao provider como tokens são).

**Detalhamento completo:** `docs/roadmap-v0.11.md` (MT-82..85, **concluídos**).
`Session::usage_total` acumula por turno e por `Session::compact`; `formatar_uso()`
(`crates/cli/src/main.rs`) é a única fonte da string de formatação, reaproveitada pelos três
pontos de exposição sem nenhuma divergência entre eles; documentação de usuário fechando a
fase.

## Fase 18 — Checkpoints e *undo* de mudanças de arquivo (ADR-0030) ✅ concluída

**Objetivo:** tornar reversível uma mudança de `fs_write`/`fs_edit` feita pelo agente
(equivalente ao "rewind" do Claude Code CLI/OpenCode) — segunda das cinco frentes de "segunda
onda" a ser preparada (ordem escolhida e registrada em `docs/decisoes-autonomas.md`,
2026-07-16, por ser a única, entre as quatro restantes, sem pergunta de segurança/
confidencialidade/egresso em aberto).

**ADR:** ADR-0030 — **escrita**, `Accepted`. Decisão central: `CheckpointStore` persiste uma
pilha *LIFO* de checkpoints em `.agentry/checkpoints.json` (mesmo diretório de estado local da
ADR-0017); só `fs_write`/`fs_edit` geram checkpoint (nunca `shell_exec`/`shell_background` —
efeito colateral de comando fica fora de escopo, mesmo nível de confiança já aceito,
ADR-0026); exposto em três pontos — flag `--undo` (*one-shot*), comando `/undo` (REPL),
*keybinding* `Ctrl+Z` (TUI) — todos chamando a **mesma** `CheckpointStore::undo()`. Um nível
de desfazer por vez (pilha, sem seleção de checkpoint específico); teto fixo de checkpoints
retidos, sem configuração nova (YAGNI).

**Detalhamento completo:** `docs/roadmap-v0.12.md` (MT-86..89, **concluídos**).
`CheckpointStore` (`crates/core/src/checkpoint/`) + `CheckpointingTool`
(`crates/core/src/tools/checkpoint.rs`) decorando `fs_write`/`fs_edit`; `formatar_undo()`
como única fonte de formatação, reaproveitada pelos três pontos de exposição; documentação de
usuário fechando a fase.

## Fase 19 — Subagentes/orquestração (ADR-0031)

**Objetivo:** delegar subtarefas a uma `Session` interna (equivalente ao `Task` do Claude
Code / árvore de sessão do OpenCode) — escolhida pelo mantenedor entre as três frentes
restantes de "segunda onda" (`docs/decisoes-autonomas.md`, 2026-07-16), depois de responder
diretamente à decisão-chave de design: **um subagente pode declarar sua própria classe de
egresso, mas só igual ou mais restrita que a da sessão-mãe** — nunca mais permissiva.

**ADR:** ADR-0031 — **escrita**, `Proposed`. Decisão central: o subagente usa o **mesmo**
`Arc<Router>` da sessão-mãe — como `Router::resolve` já recusa qualquer candidato mais
permissivo que o teto de egresso do perfil ativo, para **qualquer** chamador, essa garantia
vale automaticamente, sem nenhum código novo de imposição. Recursão (subagente criando
subagente) é impossível **estruturalmente**: o executor interno do subagente nunca registra
a própria tool `subagent`, em vez de uma checagem em tempo de execução. Reaproveita 100% da
infraestrutura existente — mesmo `PermissionGate`/`Confirmer`/`GuardrailGate` da sessão-mãe,
nenhum mecanismo paralelo. Fora de escopo (v1): uso do subagente não soma automaticamente ao
`usage_total` da sessão-mãe (aparece no próprio texto de resposta); sem `AGENTS.md`/skills no
contexto do subagente; um nível de aninhamento só, sequencial, sem *streaming*.

**Detalhamento completo:** `docs/roadmap-v0.13.md` (MT-90..92). Pronta para começar a
implementação a partir do MT-90.

## Fase 20+ — Segunda onda, restante (ADRs 0032+ quando alcançadas)

Enumeradas; *stubs de ADR adiados* — cada uma ganha ADR e detalhamento quando chegar a vez.
O mantenedor já respondeu à pergunta de design de cada uma (2026-07-16), mas a ADR completa
e os micro-tickets só são escritos quando a fase começar, com contexto fresco:

- **Memória entre sessões** (padrão LLM-Wiki/OKF, ADR-0004(c)) — hoje só há compactação
  *dentro* de uma sessão (ADR-0016); nada persiste conhecimento entre sessões/dias.
  **Resposta do mantenedor:** só memória **explícita** — um comando tipo `/remember` que
  grava um fato pontual aprovado pelo usuário, nunca persistência automática do conteúdo
  integral de uma conversa.
- **Multimodal** — `ContentBlock::Image` (`crates/core/src/model/mod.rs` só tem
  Text/ToolCall/ToolResult hoje); aceitar screenshot/imagem como entrada. **Resposta do
  mantenedor:** adiada até existir um *guardrail* de imagem (ex.: OCR alimentando as regras de
  texto já existentes, `crates/core/src/guardrail/`) — os *guardrails* de conteúdo hoje só
  inspecionam texto; multimodal sem esse pré-requisito abriria um canal de conteúdo não
  auditado. Um mecanismo de OCR provavelmente exige uma dependência nova (biblioteca de OCR)
  — quando essa frente chegar, a escolha da biblioteca passa pelo mantenedor de novo (ADR-0004).

Ordem entre essas duas ainda não decidida — fica para quando a Fase 19 concluir.

---

## Faixa de ADRs reservada

ADR-0021 e ADR-0022 **escritas** (Fase 12). ADR-0023..0028 **reservadas** (números fixados
aqui; arquivo de cada uma escrito ao iniciar sua fase, com contexto fresco). ADR-0029
**escrita** (Fase 17, `Accepted`). ADR-0030 **escrita** (Fase 18, `Accepted`). ADR-0031
**escrita** (Fase 19, `Proposed`). ADR-0032+ para o restante da segunda onda (Fase 20+), sem
número fixado ainda.
