<!-- Caminho relativo: docs/pesquisa-tui-referencias.md -->

# Pesquisa de referência: UX/TUI de agentes de codificação (MT-103)

> Primeira etapa do aprimoramento mais amplo da TUI do `agentry`, pedido pelo mantenedor após a
> rodada 4 de teste manual (`docs/roadmap-v0.15.md`). Objetivo: mapear como ferramentas
> comparáveis resolvem os mesmos problemas de design antes de desenhar qualquer mudança de
> código — a Fase C+ (redesenho) só é detalhada depois deste documento existir e ser revisado
> pelo mantenedor, mesma disciplina de "ADR com contexto fresco" já usada no resto do projeto.

## Método

Cinco ferramentas de codificação agêntica com interface de terminal interativa, escolhidas por
serem as referências mais diretas do gênero (CLIs em volta de um *agent loop* + LLM, não IDEs):

| Ferramenta | Fornecedor | Stack |
|---|---|---|
| **Claude Code CLI** | Anthropic | TypeScript, próprio (o ambiente rodando esta pesquisa) |
| **OpenCode** | anomalyco (ex-sst) | TypeScript/Bun, `@opentui`/SolidJS — já mapeado em memória prévia |
| **Aider** | Aider-AI (open-source) | Python |
| **Gemini CLI** | Google | TypeScript/Node |
| **Codex CLI** | OpenAI | Rust (o mais próximo do `agentry` em stack) |

Fontes: documentação oficial de cada projeto (linkada por seção) + repositórios públicos
(GitHub). Nenhuma implementação foi lida linha a linha — o nível de detalhe é o de
documentação pública, suficiente para orientar decisões de design, não uma auditoria de
código-fonte.

## Comparação por eixo

### 1. Sistema de *keybindings*

| Ferramenta | Modelo |
|---|---|
| **Claude Code** | Tabela padrão + arquivo de override do usuário (`~/.claude/keybindings.json`, schema JSON tipo VS Code); `Shift+Tab` cicla **modo de permissão** (não *keybindings* arbitrários). |
| **OpenCode** | Tecla *leader* configurável (`ctrl+x` por padrão, `leader_timeout` de 2s) para evitar colisão com sequências de terminal — todo comando "secundário" passa por `leader` + tecla, vim-like. Paleta de comandos (`ctrl+p`) e autocomplete de `/`. |
| **Aider** | Modo `--vim` (Esc/i/h-j-k-l) opcional; `Ctrl-X Ctrl-E` abre o buffer de entrada no editor externo do usuário (`$EDITOR`). |
| **Gemini CLI** | Tabela fixa + arquivo `~/.gemini/keybindings.json` (schema tipo VS Code, adicionar/remover bindings). `Shift+Tab` cicla modo de aprovação; achado de bug real: em PowerShell/CMD do Windows, `Shift+Tab` frequentemente não funciona (`ESC O Z` em vez da sequência esperada) — **mesma classe de problema de portabilidade Windows que o `agentry` já enfrentou** (ex.: `echo` no MCP placeholder, rodada 1). |
| **Codex CLI** | `~/.codex/config.toml`, seções por contexto (`global`, `composer`, `approval`, `list`, `pager`) — granularidade maior que um mapa único. `/keymap` abre um editor interativo. |

**`agentry` hoje:** tabela fixa em `crates/cli/src/tui/keybind.rs` (`KeyBinding` estático),
**sem arquivo de override do usuário** — a única forma de descoberta é a legenda compacta no
rodapé (`keybind::legenda()`).

### 2. Seleção de modelo/*provider*

| Ferramenta | UX |
|---|---|
| **Claude Code** | `/model` com autocomplete; seletor no app desktop/VS Code. |
| **OpenCode** | Modal de busca difusa (`fuzzysort`), categorizado em Favoritos/Recentes/por-*provider*, com metadados (custo, data de lançamento, filtro de modelos descontinuados) — já documentado em memória prévia do projeto. |
| **Aider** | `/model <nome>` ou flag `--model`; sem seletor visual dedicado. |
| **Gemini CLI** | Sem atalho de teclado dedicado documentado; troca via configuração/flag. |
| **Codex CLI** | `/model` abre um seletor com nível de "reasoning effort" combinado (ex.: "gpt-5.6-sol medium"). |

**`agentry` hoje:** seletor por busca difusa (`Ctrl+P`, MT-73) — já no mesmo espírito do
OpenCode, mas **sem metadados** (custo/data/favoritos/recentes) e **restrito aos candidatos já
declarados** em `taskClasses` (decisão deliberada, ADR-0014 — nunca introduz um alvo não
vetado). `/model <nome>` livre só existe no REPL de texto, não na TUI (decisão da rodada 3,
`docs/CURRENT-STATE.md`).

### 3. UX de confirmação/permissão

| Ferramenta | Modelo |
|---|---|
| **Claude Code** | **Cinco modos** (`default`/Manual, `acceptEdits`, `plan`, `auto`, `dontAsk`, `bypassPermissions`) — ciclo via `Shift+Tab`, indicador no *status bar* (`⏵⏵ accept edits on` etc.), classificador de segurança dedicado (`auto`) que audita ações antes de rodar, nunca um "modo mágico" sem *fallback*: 3 recusas seguidas ou 20 no total pausam `auto` e voltam a perguntar. |
| **OpenCode** | *Toggle* de dois estados (`"auto"`/`"normal"`) — já documentado em memória prévia; mais simples que o Claude Code, mais próximo do que o `agentry` já tem. |
| **Aider** | `--auto-accept-architect` (aceita edições do modelo "arquiteto" automaticamente); `/ok` como atalho pontual. |
| **Gemini CLI** | Três modos (`default`/`auto_edit`/`plan`) + YOLO (`--yolo`, aprova tudo) só por linha de comando — nunca um atalho de teclado pra entrar em YOLO current, `Ctrl+Y` alterna. |
| **Codex CLI** | Modos de sandbox (`read-only`/`workspace-write`/`danger-full-access`) **separados** de política de aprovação (`untrusted`/`on-request`/`never`) — dois eixos independentes, não um só "nível". `/permissions` abre o seletor. |

**`agentry` hoje:** gate binário `deny`/`ask`/*(implícito)* `allow` por nome de tool
(`PermissionGate`, ADR-0007) + *toggle* `auto`/`normal` (`Action::ToggleAuto`, MT-74) só afeta
tools sob `ask` — **nunca afeta `deny`** (mesmo invariante do Claude Code auto mode: a
classificação/automação nunca contorna uma negação explícita). `shell_exec`/`shell_background`
são **sempre negados** por padrão nesta versão (sem *allowlist* configurável ainda) — mais
parecido com o par sandbox/aprovação do Codex CLI (dois eixos) do que com um "nível único".

### 4. Revisão de *diff*

| Ferramenta | UX |
|---|---|
| **Claude Code** | `/diff` dedicado; app desktop com revisão visual de *diff* standalone. |
| **OpenCode** | *Diff viewer* como modal de primeira classe — *hunks*, árvore de arquivos, visão *split*/unificada — não só argumentos brutos antes de um `ask` (já documentado em memória prévia). |
| **Aider** | Formato de *diff* unificado no próprio fluxo de edição (não um modal separado) — `git diff`/`/undo` cobrem revisão via *tooling* de `git`, já que toda edição é commitada automaticamente. |
| **Gemini CLI** | Integração com *diff viewer* nativo do editor (extensão de IDE) — no terminal puro, prompt (Y/n) com o *diff*/comando mostrado inline. |
| **Codex CLI** | `/diff` mostra o `git diff` navegável por rolagem; `codex review` roda uma auditoria dedicada (achados priorizados) sem tocar a árvore de trabalho, separado do fluxo de aprovação linha-a-linha. |

**`agentry` hoje:** modal de confirmação já mostra o *diff* colorido (`+`/`-`/` `, MT-75) antes de
aprovar `fs_write`/`fs_edit` sob `ask` — no mesmo espírito do Gemini CLI/Aider (inline, não um
modal de navegação por *hunks* como o OpenCode). Não há um comando `/diff`/`review` dedicado
fora do fluxo de aprovação.

### 5. *Streaming* e formatação (Markdown)

| Ferramenta | UX |
|---|---|
| **Claude Code** | *Rendering* de Markdown com destaque de sintaxe (servidor, `mistune`); modo *fullscreen* (`CLAUDE_CODE_NO_FLICKER=1`) elimina *flicker* durante *streaming*. |
| **OpenCode** | *Rendering* rico via `@opentui`/SolidJS (reativo) — não documentado em detalhe nesta pesquisa, mas a stack em si (renderer dedicado) sugere suporte completo a Markdown. |
| **Aider** | *Streaming* de Markdown com `rich`, incluindo destaque de código inline; cores configuráveis por papel (usuário/ferramenta/erro/aviso). |
| **Gemini CLI** | *Streaming* nativo; sem detalhe público específico de *rendering* de Markdown além do padrão do terminal. |
| **Codex CLI** | *Streaming* com destaque de sintaxe no `/theme` (pré-visualização ao vivo trocando de tema); modo de *scrollback* bruto (texto não renderizado) pra copiar literal. |

**`agentry` hoje (achado da rodada 2/3):** **sem** *rendering* de Markdown de verdade — só cor
por autor (usuário/agente) e um estilo distinto pro marcador de tool em uso; `**bold**` e
` ```blocos de código``` ` aparecem como texto literal com os símbolos. Decisão de escopo já
registrada (rodada 2): fora desta rodada, candidato natural pra Fase C+.

### 6. Rolagem/paginação do histórico

| Ferramenta | UX |
|---|---|
| **Claude Code** | Modo *fullscreen* com atalhos de navegação de transcript (`{`/`}` pra pular entre *prompts*). |
| **OpenCode** | `pageup` rola mensagens; setas navegam entre sessões pai/filha (não rolagem simples) — modelo de sessão em árvore, fora do escopo do `agentry` (single-session). |
| **Aider** | Sem documentação pública de rolagem dedicada — depende do *scrollback* nativo do terminal. |
| **Gemini CLI** | `Shift+Up`/`Shift+Down`, `Ctrl+Home`/`Ctrl+End`, `Page Up`/`Page Down` — o conjunto mais completo encontrado nesta pesquisa. |
| **Codex CLI** | Rolagem de `/diff` e um modo de *scrollback* bruto separado; *issue* pública aberta pedindo desacoplar rolagem de *transcript* do foco da caixa de entrada (tema ainda não resolvido nem lá). |

**`agentry` hoje (rodada 2/4):** `Up`/`Down` rolam o histórico; ancoragem no fim (auto-*follow*)
corrigida na rodada 4 (achado: conversa curta não grudava embaixo). Sem `Page Up`/`Page Down`
dedicados, sem pular entre mensagens/*prompts*.

### 7. *Onboarding*/tela vazia

| Ferramenta | UX |
|---|---|
| **Claude Code** | Sem "logo" — abre direto no *prompt*, com dicas contextuais na primeira execução. |
| **OpenCode** | Não documentado nesta pesquisa. |
| **Aider** | Mensagem de texto simples ao iniciar (versão, modelo ativo, arquivos no *chat*). |
| **Gemini CLI** | Cabeçalho ASCII de marca ("Gemini") antes do primeiro *prompt*. |
| **Codex CLI** | Cabeçalho com modelo/diretório/contexto disponível + dica "? for shortcuts". |

**`agentry` hoje (rodada 2):** logo de abertura estilo *8-bit* (robô em *box-drawing* +
*wordmark*), some para sempre após a primeira mensagem — já no mesmo espírito do Gemini
CLI/Codex CLI (marca + dica), mais elaborado visualmente que a maioria.

### 8. Temas/cores

| Ferramenta | UX |
|---|---|
| **Claude Code** | Tema claro/escuro detectado automaticamente pelo terminal + paleta própria. |
| **OpenCode** | Configuração separada em `tui.json` (preferências visuais + *keybindings*). |
| **Aider** | Cores configuráveis por papel via *flags*/config (`--user-input-color`, etc.), padrão sensível para terminal claro/escuro. |
| **Gemini CLI** | Seletor de tema na configuração da UI (sem atalho de teclado dedicado documentado). |
| **Codex CLI** | `/theme` com **pré-visualização ao vivo** (destaque de sintaxe atualiza enquanto navega os temas) — a UX de troca de tema mais polida encontrada nesta pesquisa. |

**`agentry` hoje:** cores fixas (`Color::Cyan` usuário, `Color::White` agente,
`Color::DarkGray` marcador de tool) — **sem tema configurável**, sem detecção clara/escuro,
sem paridade com o resto do ecossistema `ratatui`/terminal do usuário.

### 9. *Widget* de lista de tarefas (*todo*)

| Ferramenta | UX |
|---|---|
| **Claude Code** | `TodoWrite`/`Task*` — *tool* que mantém uma lista de subtarefas (pendente/em andamento/concluída) visível no *transcript*, atualizada ao vivo pelo próprio modelo. |
| **OpenCode** | Componente `todo-item` — glifos `[ ]`/`[•]`/`[✓]`, mesmo conceito do Claude Code (já documentado em memória prévia). |
| **Aider** | Sem *widget* de tarefas — não documentado. |
| **Gemini CLI** | `Ctrl+T` mostra a lista *TODO* completa (`app.showFullTodos`) — o modelo mantém uma lista, visível sob demanda. |
| **Codex CLI** | Não encontrado nesta pesquisa. |

**`agentry` hoje:** **nenhum** *widget*/*tool* de lista de tarefas — o agente não tem como
comunicar "estou numa tarefa de N passos, aqui está o progresso" de forma estruturada; só texto
livre na resposta. **Gap mais consistente entre as ferramentas pesquisadas** (3 de 5 têm algo
equivalente).

### 10. Descoberta de atalhos/ajuda

| Ferramenta | UX |
|---|---|
| **Claude Code** | `?` abre o painel completo de atalhos (modo *fullscreen*). |
| **OpenCode** | Paleta de comandos (`ctrl+p`) + `/help` — múltiplos caminhos de descoberta a partir da mesma fonte de verdade (`Definitions`). |
| **Aider** | `/help <pergunta>` (modo dedicado que responde sobre o próprio Aider, usando um *chat-mode* específico). |
| **Gemini CLI** | `?` num *prompt* vazio alterna o painel de atalhos. |
| **Codex CLI** | Dica fixa "? for shortcuts" no cabeçalho; `/keymap` e `/theme` como pontos de entrada interativos que já ensinam o que existe. |

**`agentry` hoje:** só a legenda compacta no rodapé (`keybind::legenda()`) — **sem painel
expandido**, sem comando `/help` dentro da TUI (existe `/help`? não — comando desconhecido hoje
seria erro "comando desconhecido: /help", já que não está entre os tratados por
`processar_comando_de_texto`).

## Oportunidades priorizadas para o `agentry`

Lista ordenada por impacto vs. esforço estimado, cada uma já esboçada como candidata a
micro-ticket — **nenhuma detalhada ainda**; a Fase C+ (`docs/roadmap-v0.15.md`) só ganha
tickets de verdade depois deste documento ser revisado pelo mantenedor.

1. **Painel de ajuda expandido (`?`)** — hoje só a legenda compacta do rodapé existe; todas as
   5 ferramentas pesquisadas têm algum painel dedicado. Esforço baixo (reaproveita
   `keybind::legenda()` como fonte, só muda a apresentação — modal de tela cheia em vez de uma
   linha).
2. **Comando `/help` dentro da TUI** — hoje `processar_comando_de_texto` (rodada 3) não trata
   `/help`; cai em "comando desconhecido". Natural companheiro do item 1.
3. ***Widget*/*tool* de lista de tarefas** — gap mais consistente entre as ferramentas
   pesquisadas (3 de 5 têm). Exigiria uma nova *tool* (ex.: `todo_write`, espelhando o padrão já
   usado por este próprio ambiente) + um componente de renderização na TUI — escopo maior,
   possivelmente merece sua própria ADR (mexe em `crates/core` como *tool* nova, não só na TUI).
4. **Arquivo de *keybindings* do usuário** (`~/.agentry/keybindings.json` ou
   `.agentry/keybindings.json`, mesmo padrão de `.agentry/agentry.settings.json`, ADR-0017) —
   hoje a tabela é 100% hardcoded em `keybind.rs`. Presente em Claude Code, Gemini CLI, Codex
   CLI e OpenCode — o único eixo em que o `agentry` está atrás de **todas as 5** referências.
5. **Metadados no seletor de modelo** (favoritos/recentes, ao menos) — o seletor já existe
   (MT-73), só falta a camada de metadados/persistência (`~/.agentry` ou `.agentry/model.json`,
   mesmo padrão de estado local já estabelecido).
6. **Tema configurável** (claro/escuro detectado do terminal, ou `/theme` com pré-visualização
   como o Codex CLI) — hoje cores são fixas no código. Risco baixo, mas escopo de "quantas cores
   configuráveis" precisa de uma decisão de design antes de começar (não um *ticket* trivial).
7. ***Rendering* de Markdown de verdade** (negrito/blocos de código estilizados) — já
   identificado desde a rodada 2, decisão deliberada de adiar; permanece candidato, mas é o mais
   caro dos itens desta lista (parser + interação com o *wrap* manual já implementado).
8. **`Page Up`/`Page Down` no histórico** — Gemini CLI tem o conjunto mais completo de atalhos
   de rolagem pesquisado; o `agentry` só tem `Up`/`Down` linha-a-linha. Esforço baixo.
9. **Editor externo para a caixa de entrada** (`$EDITOR`, como o `Ctrl-X Ctrl-E` do Aider) — útil
   pra mensagens longas/multi-linha complexas; a caixa de entrada já cresce dinamicamente
   (MT-97), então o caso de uso é mais estreito para o `agentry` do que para o Aider (que não
   tinha altura dinâmica antes disso).
10. **Comando/`review` dedicado fora do fluxo de aprovação** (como `codex review`/`/diff` do
    Codex CLI) — permitiria auditar mudanças já feitas sem estar no meio de um `ask`. Menor
    prioridade: o `agentry` já tem checkpoints/`undo` (ADR-0030) cobrindo parte do mesmo
    problema (desfazer, não revisar).

**Não recomendado adotar como está:**
- Modelo de sessões em árvore do OpenCode (pai/filho) — fora do escopo do `agentry`
  (sessão única, `subagent` já cobre delegação sem essa complexidade de navegação, ADR-0031).
- YOLO/*bypass* de um atalho só (Gemini CLI `Ctrl+Y`, Claude Code `bypassPermissions`) — o
  `agentry` já decidiu deliberadamente que `deny` nunca é contornável por automação (mesmo
  invariante do próprio *auto mode* do Claude Code); um atalho de "aprovar tudo" teria que
  preservar isso explicitamente, não uma cópia direta do padrão de nenhuma das duas
  ferramentas.

## Fontes

- Claude Code — [Choose a permission mode](https://code.claude.com/docs/en/permission-modes),
  [Todo Lists](https://code.claude.com/docs/en/agent-sdk/todo-tracking).
- OpenCode — [TUI Commands & Keybindings (DeepWiki)](https://deepwiki.com/anomalyco/opencode/9.2-tui-commands-and-keybindings),
  memória prévia do projeto (`opencode-tui-reference`, pesquisa de sessão anterior).
- Aider — [Documentação oficial](https://aider.chat/docs/), [Usage](https://aider.chat/docs/usage.html),
  [Options reference](https://aider.chat/docs/config/options.html).
- Gemini CLI — [Keyboard shortcuts](https://geminicli.com/docs/reference/keyboard-shortcuts/),
  [IDE Integration](https://google-gemini.github.io/gemini-cli/docs/ide-integration/).
- Codex CLI — [Codex CLI docs](https://developers.openai.com/codex/cli),
  [Agent approvals & security](https://developers.openai.com/codex/agent-approvals-security),
  [Sandbox](https://developers.openai.com/codex/concepts/sandboxing),
  [Codex CLI TUI Shortcuts and Slash Commands](https://codex.danielvaughan.com/2026/04/08/codex-cli-tui-shortcuts-slash-commands/).
