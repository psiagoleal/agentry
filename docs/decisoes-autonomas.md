<!-- Caminho relativo: docs/decisoes-autonomas.md -->

# Log de decisões autônomas (loop de implementação)

Registro **append-only** de toda decisão tomada pelo agente durante a execução autônoma em
loop (`/loop /implementar-roadmap`) quando ele se depara com uma dúvida e escolhe a **opção
recomendada** sem parar para perguntar. Existe para que o mantenedor **revise depois** cada
escolha feita sozinho.

> Regra do loop: diante de uma dúvida com opção recomendada clara, o agente **segue a
> recomendada, registra aqui, e continua**. Diante de uma dúvida **sem** recomendação clara,
> ou de uma parada dura (dependência nova, repo irmão, afrouxar segurança), o agente **para e
> escala ao usuário** — ver `.claude/commands/implementar-roadmap.md`.

## Formato de cada entrada

```
### AAAA-MM-DD — <ticket/fase> — <título curto da decisão>
- **Contexto:** onde/por quê a decisão apareceu.
- **Opções consideradas:** (a) …; (b) …; (c) …
- **Escolha (recomendada):** <a opção adotada>.
- **Justificativa:** por que é a mais alinhada ao objetivo do agentry, à segurança/
  governança/confidencialidade (ADR-0002 fail-closed) e a um design mínimo (não
  over-engineered).
- **Commit:** `<hash>`.
```

## Entradas (mais recente no topo)

### 2026-07-16 — ADR-0031 (Fase 19, subagentes) — interpretação de "classe da sessão-mãe" e prevenção de recursão
- **Contexto:** o mantenedor respondeu explicitamente (não uma decisão autônoma) a pergunta-
  chave da ADR-0031 — um subagente pode declarar sua própria classe de egresso, mas só igual
  ou mais restrita que a da sessão-mãe (opção B, escalada em turno anterior). Duas questões de
  design **de implementação** ficaram abertas ao transformar essa resposta em arquitetura
  concreta, ambas com opção recomendada clara:
  1. **O que exatamente é "a classe da sessão-mãe"?** O `Router` (`crates/core/src/router/mod.rs`)
     é construído uma vez por processo com **uma** `EgressClass` (o teto do perfil ativo,
     ADR-0002) — `resolve()`/`resolve_with_override()` já recusam qualquer candidato mais
     permissivo que esse teto, para **qualquer** chamador. Uma leitura mais estrita seria "a
     classe da rota especificamente ativa na sessão-mãe agora" (que pode ser mais restrita que
     o teto do perfil, se o usuário trocou de task-class via `/task-class`) — isso exigiria
     rastrear e repassar esse estado mutável (hoje só existe como `String` na CLI, não no
     `Router`/`Session`) até o `SubagentTool`.
  2. **Como impedir um subagente de criar outro subagente (recursão)?** Um contador/*flag*
     compartilhado (`Arc<AtomicBool>`) checado em `execute()` funcionaria, mas exige estado
     mutável só para essa checagem.
- **Opções consideradas (questão 1):** (a) o subagente usa o **mesmo** `Arc<Router>` da sessão-
  mãe — o teto do perfil já é enforced estruturalmente por `resolve()`, sem código novo; (b)
  rastrear a rota especificamente ativa da sessão-mãe (mais preciso, mas exige um mecanismo de
  estado compartilhado mutável novo, hoje inexistente).
- **Opções consideradas (questão 2):** (a) o executor interno do subagente **não inclui a tool
  `subagent` no próprio registro** — recursão impossível estruturalmente (o modelo nem enxerga
  a tool), sem estado compartilhado; (b) *flag* atômico compartilhado, checado em tempo de
  execução.
- **Escolha (recomendada):** (1a) mesmo `Arc<Router>` compartilhado; (2a) executor do subagente
  sem a própria tool registrada.
- **Justificativa:** ambas são a opção **estruturalmente mais forte** (a garantia vem da
  construção do objeto, não de uma checagem que poderia ser esquecida ou contornada) e a de
  **menor código novo** (zero estado mutável compartilhado adicional) — mesma disciplina de
  design mínimo do projeto. (1a) ainda cumpre a letra da resposta do mantenedor: um subagente
  nunca resolve um candidato mais permissivo que o teto que a própria sessão-mãe já respeita.
  Caso o mantenedor prefira a leitura mais estrita (1b) depois de ver a implementação, é uma
  extensão futura clara (rastrear a rota ativa da mãe), não uma reversão.
- **Commit:** preparação da Fase 19 nesta mesma iteração (ver `docs/adr/0031-*.md` e
  `docs/roadmap-v0.13.md`).

### 2026-07-16 — Preparação da Fase 18 — qual frente restante da "segunda onda" preparar
- **Contexto:** a Fase 17 (uso de tokens visível) fechou inteira (MT-82..85). Restam quatro
  frentes de "segunda onda" em `docs/roadmap-longo-prazo.md` §Fase 18+ (à época), sem ordem
  declarada: memória entre sessões, subagentes/orquestração, multimodal, checkpoints/*undo*.
- **Opções consideradas:** (a) **checkpoints/*undo*** de mudanças de arquivo — escopo
  autocontido (só o workspace local, nenhum caminho de rede/egresso), local de
  armazenamento já resolvido pela ADR-0017 (`.agentry/`, auto-excluído do git), design
  mínimo claro (snapshot do conteúdo **antes** de `fs_write`/`fs_edit` escrever, pilha
  *LIFO* com teto, `/undo` no REPL); (b) **multimodal** (`ContentBlock::Image`) — levanta uma
  pergunta de confidencialidade real (imagem pode conter informação sensível que os
  *guardrails* de texto atuais não enxergam — precisa de decisão de design própria antes de
  implementar, não só uma extensão mecânica do tipo); (c) **memória entre sessões**
  (LLM-Wiki/OKF) — maior escopo entre as quatro, persistir conteúdo de conversa **entre**
  sessões levanta pergunta de retenção/confidencialidade que a compactação intra-sessão
  (ADR-0016) nunca precisou responder; (d) **subagentes/orquestração** — o próprio roadmap já
  sinaliza uma "decisão-chave" com implicação direta na ADR-0002 (egresso), mais alinhada a
  uma escalada ao mantenedor do que a uma decisão autônoma (mesmo motivo já registrado na
  entrada de preparação da Fase 17, abaixo).
- **Escolha (recomendada):** (a) **checkpoints/*undo*** de mudanças de arquivo.
- **Justificativa:** é a única das quatro sem nenhuma pergunta de segurança/confidencialidade/
  egresso em aberto — puramente local ao workspace, reaproveita a decisão de diretório de
  estado já tomada (ADR-0017), sem exigir decisão de design nova sobre dado sensível
  (diferente de multimodal/memória entre sessões) nem sobre modelo de privacidade
  (diferente de subagentes). Design mínimo: sem dependência nova, sem novo caminho de rede.
  As outras três ficam para as próximas preparações de fase; multimodal e memória entre
  sessões precisam de mais reflexão de design antes de virarem ADR (registrado aqui para a
  próxima iteração não repetir a análise do zero); subagentes continua a candidata mais
  provável a exigir escalar ao mantenedor.
- **Commit:** preparação da Fase 18 nesta mesma iteração (ver `docs/adr/0030-*.md` e
  `docs/roadmap-v0.12.md`).

### 2026-07-16 — Preparação da Fase 17 — qual frente da "segunda onda" preparar primeiro
- **Contexto:** a Fase 16 (MCP) fechou inteira (MT-77..81). `docs/roadmap-longo-prazo.md`
  §Fase 17+ (nome do bloco antes desta decisão — hoje dividido em Fase 17, esta escolha, e
  Fase 18+, as quatro restantes) enumerava cinco frentes da "segunda onda" **sem ordem
  declarada entre elas**:
  memória entre sessões, subagentes/orquestração, multimodal, checkpoints/*undo*, custo/uso
  visível. O comando `/loop /implementar-roadmap` §1 exige que, numa fase sem tickets
  detalhados, a unidade de trabalho da iteração seja **preparar** a próxima fase (ADR +
  quebra em micro-tickets) — o que exige escolher qual das cinco vem primeiro.
- **Opções consideradas:** (a) **custo/uso visível** — `Usage` (tokens de entrada/saída) já é
  rastreado por chamada em `crates/core/src/model/mod.rs`/`session/mod.rs`, só não é
  acumulado por sessão nem exposto ao usuário; escopo pequeno, nenhuma dependência nova,
  nenhuma superfície de rede/egresso nova, nenhuma pergunta de segurança em aberto — é
  essencialmente uma camada de observabilidade sobre dado que já existe; (b) **checkpoints/
  undo** — escopo moderado (precisa decidir onde/como versionar snapshots de arquivo), sem
  implicação de egresso, mas maior superfície de design (formato de snapshot, política de
  retenção em disco); (c) **multimodal** (`ContentBlock::Image`) — exige decidir como uma
  imagem entra no *pipeline* de mensagens e se algum provider já suportado (Ollama/LiteLLM)
  aceita entrada multimodal de verdade, além de uma pergunta de confidencialidade nova
  (imagem pode conter informação sensível que os *guardrails* de texto atuais não enxergam);
  (d) **memória entre sessões** (LLM-Wiki/OKF) — maior escopo entre as cinco, e persistir
  conteúdo de conversa **entre** sessões levanta uma pergunta de retenção/confidencialidade
  que hoje não existe (a compactação da ADR-0016 já é *dentro* de uma sessão, nunca grava em
  disco entre sessões) — merece uma ADR com mais contexto de design antes de começar; (e)
  **subagentes/orquestração** — o próprio roadmap já sinaliza uma "decisão-chave" com
  implicação direta na ADR-0002 (um subagente herda a classe de egresso da sessão-mãe ou tem
  a própria?) — questão de modelo de privacidade, não só de implementação, mais alinhada ao
  tipo de decisão que o comando pede para **escalar**, não decidir sozinho.
- **Escolha (recomendada):** (a) **custo/uso visível**.
- **Justificativa:** é a única das cinco sem nenhuma pergunta de segurança/confidencialidade/
  egresso em aberto — só expõe, de forma legível, um dado que o `core` já calcula e descarta
  hoje. Menor escopo (design mínimo, sem over-engineering: nenhuma dependência nova, nenhum
  novo caminho de rede, nenhuma mudança de schema de configuração provavelmente necessária) e
  maior valor imediato de transparência para o usuário (saber quantos tokens uma sessão
  consumiu). As outras quatro ficam no roadmap para as próximas preparações de fase, na ordem
  que a próxima iteração escolher; **subagentes**, em particular, provavelmente merece
  escalar ao mantenedor quando chegar a vez, dado que o próprio roadmap já assinala uma
  decisão de modelo de privacidade em aberto.
- **Commit:** preparação da Fase 17 nesta mesma iteração (ver `docs/adr/0029-*.md` e
  `docs/roadmap-v0.11.md`).

### 2026-07-15 — MT-78 (Fase 16, MCP) — `fake_mcp_server` implementa o protocolo MCP na mão, sem a *feature* `server` do `rmcp`
- **Contexto:** o MT-78 (`docs/roadmap-v0.10.md`) previa, como uma das opções, reaproveitar a
  *feature* `server` do `rmcp` (só em `[dev-dependencies]`) para montar o `fake_mcp_server` —
  mesmo espírito do `fake_lsp_server` (MT-23), mas usando as macros de servidor do próprio SDK
  em vez de implementar o protocolo na mão. Essa abordagem foi tentada primeiro (`rmcp = {
  workspace = true, features = ["server", "macros", "transport-io"] }` em
  `[dev-dependencies]`) e **compilou e passou nos testes** com `cargo build -p agentry-core
  --bins --tests` — mas falhou em `cargo build --release` (o comando real de release do
  projeto): um alvo `[[bin]]` de `crates/core` (como `fake_mcp_server`, descoberto
  automaticamente em `src/bin/`) só recebe as *features* declaradas em `[dependencies]`,
  **nunca** as de `[dev-dependencies]` — Cargo só estende `dev-dependencies` para alvos
  `tests`/`examples`/`benches`, não para `[[bin]]`. O comando combinado `--bins --tests` unifica
  os dois conjuntos de *features* e mascarou o problema até o `cargo build --release` real
  (sem `--tests`) revelar o erro de import.
- **Opções consideradas:**
  (a) mover `fake_mcp_server` de `src/bin/` para `examples/` (Cargo estende
  `dev-dependencies` para alvos `examples` também) — mas perde a conveniência de
  `env!("CARGO_BIN_EXE_fake_mcp_server")` (só definida para `[[bin]]`), exigindo descoberta
  manual e frágil do caminho do binário compilado;
  (b) promover `server`/`macros`/`transport-io` de `[dev-dependencies]` para `[dependencies]`
  (produção) — mas a própria ADR-0028 proíbe explicitamente habilitar `server` em dependência
  de produção (o `agentry` é consumidor de MCP, não expõe a si mesmo como servidor);
  (c) implementar o protocolo MCP na mão no `fake_mcp_server` (JSON-RPC 2.0 delimitado por
  linha sobre `stdio` — verificado no código-fonte do `rmcp`,
  `transport/async_rw.rs::JsonRpcMessageCodec`: é *newline-delimited*, ao contrário do LSP, que
  usa cabeçalhos `Content-Length`), usando os tipos de `rmcp::model` (módulo **sem** *feature
  gate* — disponível só com `client`, já a dependência de produção) para montar respostas
  corretas sem hand-typing os nomes de campo JSON.
- **Escolha (recomendada):** (c).
- **Justificativa:** (a) resolveria o problema de escopo de *feature*, mas trocaria uma
  fragilidade por outra (descoberta manual de caminho de binário, em vez do mecanismo já
  testado e usado por `fake_lsp_server`) sem necessidade. (b) violaria a própria ADR-0028 que
  este ticket implementa — desviar da diretriz de conformidade que acabei de escrever exigiria
  no mínimo revisar a ADR, não uma decisão silenciosa dentro de um ticket de implementação. (c)
  resolve o problema na raiz sem abrir mão de nada: o formato de mensagem do MCP acabou sendo
  **mais simples** que o do LSP (sem cabeçalho de tamanho), e os tipos de dados do `rmcp::model`
  (já disponíveis com a *feature* `client` sozinha) eliminam o risco de errar nomes de campo à
  mão — o resultado final não precisa de nenhuma *feature* extra do `rmcp` em nenhuma camada,
  produção ou teste, mantendo a árvore de dependências exatamente como a ADR-0028 já
  descrevia. Validado de ponta a ponta: os 3 testes de integração (*handshake* real,
  `list_tools()` devolvendo a tool esperada, `Drop` sem `shutdown()` não deixa processo órfão)
  passam contra o cliente `rmcp` real sem nenhuma modificação no cliente de produção.
- **Commit:** `7a68941`.

### 2026-07-15 — MT-77 (Fase 16, MCP) — exemplo de `mcpServers` no `--init` usa `echo` como comando inerte
- **Contexto:** o MT-77 (`docs/roadmap-v0.10.md`) pede que `GENERIC_SETTINGS_EXAMPLE`
  (`crates/cli/src/main.rs`) ganhe o bloco `mcpServers` com um exemplo comentado, mesma
  convenção autoexplicativa (ADR-0022) já usada em todo outro bloco do arquivo. Mas
  `mcpServers` é um `HashMap<String, McpServerSettings>` **sem** *struct* de embrulho — o
  mesmo problema já encontrado no MT-57 para `taskClasses`: uma chave `_comentario` solta no
  nível do mapa falha ao desserializar como `McpServerSettings`. Diferente de `taskClasses`
  (onde o exemplo "chat" é seguro porque replica *exatamente* o comportamento zero-config),
  não existe um "servidor MCP zero-config" natural para replicar — qualquer comando de exemplo
  real (ex.: `npx -y @pacote/algum-servidor`) seria uma entrada de verdade, syntaticamente
  válida, presente por padrão em todo projeto recém-inicializado.
- **Opções consideradas:**
  (a) exemplo com um comando MCP real plausível (ex.: `npx -y
  @modelcontextprotocol/server-filesystem`) — mesmo padrão do `taskClasses`/`revisao-em-nuvem`;
  (b) exemplo usando `echo` (presente em todo sistema, sem efeito colateral, não fala o
  protocolo MCP) como comando — comentário explicativo dentro da própria entrada de exemplo,
  mesma técnica do MT-57, mas escolhendo um comando deliberadamente inerte mesmo se um ticket
  futuro (MT-78) vier a conectar automaticamente a todo servidor declarado.
- **Escolha (recomendada):** (b).
- **Justificativa:** (a) teria efeito colateral real assim que o MT-78 (ainda não implementado)
  passar a conectar a servidores declarados — `npx` tentaria baixar/rodar um pacote não
  verificado por padrão em todo projeto recém-inicializado, um resultado surpreendente e
  potencialmente custoso/lento sem o usuário pedir. `taskClasses`' exemplo real é seguro
  porque exige seleção explícita (`--task-class`/`/task-class`) antes de qualquer efeito;
  `mcpServers`, como desenhado até agora (ADR-0028), não tem essa camada de seleção — a
  suposição mais segura é que declarar um servidor basta para ele ser usado. `echo` resolve o
  problema de mostrar o formato real (comando + args + egressClass) sem nenhum risco: não fala
  o protocolo MCP, então mesmo uma tentativa de conexão falharia de forma tratada e óbvia
  (*handshake* nunca completa), nunca silenciosa nem com efeito colateral de rede/disco.
- **Commit:** `9fcbaaf`.

### 2026-07-15 — MT-73 (Fase 15, TUI) — novo acessor `Router::route_entry` em `crates/core` (fora da lista original de arquivos do ticket)
- **Contexto:** o MT-73 (`docs/roadmap-v0.9.md`) lista só `crates/cli/src/tui/model_picker.rs`
  (novo) e `crates/cli/src/tui/mod.rs` como arquivos no escopo. Para popular o seletor com os
  candidatos já declarados na `task-class` ativa (`RouteEntry.candidates`, o objetivo central
  do ticket), a TUI precisa ler essa lista bruta — mas `Router` (`crates/core/src/router/mod.rs`)
  só expunha `resolve`/`resolve_with_override` (que devolvem **um** candidato já escolhido, não
  a lista inteira); o campo `routes: HashMap<String, RouteEntry>` é privado.
- **Opções consideradas:**
  (a) reconstruir a lista de candidatos de forma independente em `main()`, a partir de
  `cfg.task_classes` + a lógica de síntese de defaults já em `register_declared_task_classes`
  (`crates/cli/src/main.rs`, MT-56), sem tocar `crates/core`;
  (b) adicionar um acessor de leitura mínimo `Router::route_entry(&self, task_class: &str) ->
  Option<&RouteEntry>` em `crates/core/src/router/mod.rs` — só devolve o que `self.routes` já
  tem, sem resolver egresso nem aplicar overrides.
- **Escolha (recomendada):** (b).
- **Justificativa:** (a) duplicaria a lógica de merge declarado+sintetizado que já vive em
  `register_declared_task_classes` — o Router, depois que essa função roda, é a **única** fonte
  de verdade sobre quais candidatos existem de fato para uma `task-class`; reconstruir a lista
  em paralelo arriscaria os dois lugarem divergirem silenciosamente (ex.: um bug futuro em só um
  dos dois). (b) é um acessor de leitura direto (não uma decisão de roteamento nova, não
  contorna egresso/overrides — `resolve_with_override` continua sendo a única forma de
  *escolher* um candidato, exigido pela ADR-0027/ADR-0014), simétrico ao padrão já usado por
  `ChatState::mensagens()` (getter só-leitura) escrito nesta mesma fase. Adicionar um método a
  um tipo existente do `core` não é "reimplementar lógica de domínio na TUI" (proibido pela
  Diretriz de Conformidade da ADR-0027) — é o oposto: expõe a lógica já centralizada em vez de
  duplicá-la fora dela. A lista de "Arquivos no escopo" de um micro-ticket é escrita antes da
  implementação (disciplina `micro-ticket-planner`) e não antecipa toda necessidade de acessor;
  o ticket permanece de tamanho mínimo (um método getter de poucas linhas + um teste no `core`,
  não uma feature nova).
- **Commit:** `7d3da53`.

### 2026-07-15 — MT-72 (Fase 15, TUI) — auditoria descartada sob `--tui`, não redirecionada para um widget
- **Contexto:** o smoke-test manual do MT-72 (`agentry --tui`, mensagem real via Ollama)
  revelou que `StderrAuditSink`/o `impl GuardrailAuditSink` (ambos em `crates/cli/src/main.rs`,
  MT-05/46) escrevem via `eprintln!` diretamente no terminal a cada chamada de rede — sob o
  modo bruto/tela-alternativa do `crossterm` (ADR-0027), essa escrita cai por cima do buffer
  que o `ratatui` está desenhando (ele não sabe da escrita, então não a repõe no próximo
  `draw`), corrompendo a tela a cada turno. Não é um problema no REPL/one-shot (stderr
  simplesmente intercala com a saída normal na mesma tty, sem "buffer" a violar).
- **Opções consideradas:**
  (a) redirecionar a auditoria para um *widget* de log dentro da própria TUI (painel dedicado,
  rolável) quando `--tui` está ativo;
  (b) descartar silenciosamente a auditoria (`NoopAuditSink`, novo tipo unitário implementando
  `AuditSink`/`GuardrailAuditSink` como no-op) enquanto o modo TUI estiver ativo, preservando o
  comportamento atual (stderr) para REPL/one-shot.
- **Escolha (recomendada):** (b).
- **Justificativa:** um *widget* de log é uma peça de UI nova, não pedida por nenhum ticket da
  Fase 15 (MT-70..76) nem pela ADR-0027 — construí-la agora seria escopo além do objetivo do
  MT-72 ("view de chat com streaming real"), violando a disciplina de não introduzir
  funcionalidade além do *Objetivo* do ticket. Descartar é o comportamento correto enquanto não
  existe onde mostrar a auditoria sem corromper a tela; a auditoria em si (rastreabilidade de
  chamadas de rede, ADR-0002) continua ativa e correta no REPL/one-shot, os modos usados hoje
  para qualquer fluxo que dependa de auditoria de verdade. Um *widget* de log fica anotado como
  candidato de ticket futuro, condicionado a demanda real (YAGNI) — não uma lacuna esquecida.
  Não afrouxa nenhuma garantia de segurança/egresso: a decisão de **permitir** ou **negar** uma
  chamada de rede continua inteiramente no `Transport`/`Allowlist` (MT-05/07, ADR-0002),
  inalterados; só o **registro** posterior da chamada já permitida deixa de ser impresso.
- **Commit:** `04db36e`.

### 2026-07-15 — MT-72 (Fase 15, TUI) — revisão dos *keybindings* de letra do MT-71 (`q`/`k`/`j`) para liberar a digitação
- **Contexto:** o MT-71 (`docs/roadmap-v0.9.md`) havia fixado `q` (sair), `k`/`j` (rolar,
  estilo vim) como alternativas às setas na tabela única de `crates/cli/src/tui/keybind.rs` —
  nesse momento a TUI ainda não tinha nenhuma caixa de entrada de texto real, então não havia
  ambiguidade. O MT-72 introduz a digitação de mensagens de verdade, e uma letra solta não pode
  significar simultaneamente "ação fixa" (sair/rolar) e "caractere digitado" sem um modo
  explícito (insert/normal, à la vim) — fora do escopo mínimo desta ticket.
- **Opções consideradas:**
  (a) introduzir um modo explícito (ex.: `Tab` alterna entre "navegação" e "digitação"), preservando os atalhos de letra do MT-71 dentro do modo de navegação;
  (b) remover os atalhos de letra (`q`, `k`, `j`) da tabela `DEFINITIONS`, mantendo só teclas
  que nunca colidem com texto digitado (`Ctrl+C` para sair — convenção universal de terminal,
  inambígua mesmo com o campo de texto focado; setas para rolar).
- **Escolha (recomendada):** (b).
- **Justificativa:** um sistema de modos (a) é a escolha certa para um editor modal completo,
  mas over-engineering para o escopo do MT-72 — nenhum ticket da Fase 15 pede navegação modal
  estilo vim, e introduzir um conceito de modo agora obrigaria também a expor visualmente qual
  modo está ativo (mais uma peça de UI não pedida). (b) resolve a ambiguidade com a mudança
  mínima: `Ctrl+C` já é a convenção universal e inambígua de "sair" em qualquer aplicação de
  terminal (funciona igual estando o campo de texto focado ou não), e setas nunca colidem com
  texto digitado. Nenhuma regressão de segurança — a tabela de *keybindings* continua sem
  conflito de tecla (mesmo teste do MT-71, `tabela_nao_tem_duas_acoes_para_a_mesma_tecla_default`,
  ainda passa) e a garantia "tecla sem ação mapeada não é erro" (MT-71) se estende naturalmente
  para "vira caractere digitado", não um estado de erro.
- **Commit:** `04db36e`.

### 2026-07-15 — ADR-0023 (preparação da Fase 13) — parser de frontmatter de `SKILL.md` próprio, sem dependência YAML
- **Contexto:** ADR-0023 (memória de projeto: `AGENTS.md`/`CLAUDE.md` + *progressive
  disclosure* de `SKILL.md`) precisa extrair `name`/`description` do frontmatter YAML de cada
  `SKILL.md` descoberto (delimitado por `---`), incluindo o estilo de bloco dobrado (`>-`)
  usado nos `SKILL.md` reais deste projeto (ex.: `.claude/skills/adr-writer/SKILL.md`).
- **Opções consideradas:**
  (a) adotar uma dependência de parser YAML (ex.: `serde_yaml` ou `saphyr`) para interpretar o
  frontmatter de forma genérica e robusta a qualquer sintaxe YAML válida;
  (b) escrever um parser próprio, mínimo, cobrindo só o subconjunto realmente usado por
  `SKILL.md` neste ecossistema — duas chaves de string fixas (`name`/`description`), incluindo
  o bloco dobrado `>-` — sem tentar cobrir YAML arbitrário (listas, mapas aninhados, âncoras,
  tipos numéricos/booleanos).
- **Escolha (recomendada):** (b).
- **Justificativa:** o schema do frontmatter de `SKILL.md` é fixo, pequeno e conhecido —
  trazer um parser YAML genérico traria uma superfície de API/manutenção desproporcional ao
  problema, além de ser uma **dependência de runtime nova**, que o próprio comando de loop
  trata como gatilho de parada dura quando decidida durante *implementação* (não durante
  preparação de fase). Decidir agora, na ADR, por um parser mínimo evita completamente esse
  gatilho — mesmo espírito de MT-06 (redação de segredos sem regex) e do casamento de
  guardrail por substring (ADR-0007), que já evitam dependência nova para problemas estreitos
  e bem definidos deste projeto. O trade-off aceito (frontmatter fora do subconjunto suportado
  falha de forma tratada, não silenciosa) é proporcional ao ganho de continuar com árvore de
  dependências auditável (ADR-0001).
- **Commit:** `384899b`.

### 2026-07-15 — MT-55 (Fase 12, `taskClasses`) — `Config` não sintetiza defaults de task-class
- **Contexto:** o ticket MT-55 (`docs/roadmap-v0.6.md`) pedia que, quando `taskClasses` não
  declarar `chat`/`compact`/`guardrail-compliance`, o `Config` (`crates/core/src/config/mod.rs`)
  sintetize internamente esses defaults hoje hardcoded na CLI, para "zero-config idêntico" e
  para `/compact`/Reviewer terem rota mesmo sem configuração explícita.
- **Opções consideradas:**
  (a) `Config::resolve` sintetiza os três defaults concretos (provider `"ollama"`, modelos e
  presets fixos) quando ausentes do mapa declarado — como o texto do ticket propunha;
  (b) `Config.task_classes` expõe só o que foi declarado pelo usuário (mapa vazio quando
  nada é configurado), e a síntese de defaults concretos de provider/modelo passa a ser
  responsabilidade da CLI (MT-56), que já é o ponto que hoje hardcoda `set_chat_route`
  (Ollama, `local-only`).
- **Escolha (recomendada):** (b).
- **Justificativa:** `crates/core` é a camada de domínio (rotas, presets, egresso) e não deve
  conhecer qual provider é o produto usa como fallback — isso é uma decisão de *produto* da
  CLI de referência, não do modelo de dados. Colocar `"ollama"` hardcoded dentro do `core`
  quebraria a separação já estabelecida (o `core` não hardcoda nenhum provider concreto hoje)
  e tornaria a lib reutilizável menos genérica sem necessidade — não há teste ou consumidor de
  `agentry-core` fora da CLI que precise desse comportamento embutido no tipo de config. A
  CLI (MT-56) é o lugar correto para registrar `chat`/`compact`/`guardrail-compliance` como
  rotas concretas quando `task_classes` resolvido vier vazio, preservando o resultado
  observável do ticket (zero-config idêntico) sem violar a fronteira de camadas. Não afrouxa
  segurança/egresso — quando a CLI sintetizar, o candidato de fallback continua `local-only`
  (Ollama), igual ao comportamento anterior a esta mudança.
- **Commit:** `8f0ba55`.
