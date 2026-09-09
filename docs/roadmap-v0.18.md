<!-- Caminho relativo: docs/roadmap-v0.18.md -->

# Roadmap v0.18 — Micro-tickets

Fase K: **teste de integração ponta a ponta**. É o item 1 do handoff desde a rodada 8, e a
justificativa é estrutural, não uma preferência de cobertura: **todo bug de produção
encontrado até hoje neste projeto vive na fronteira `main()` → provider real**, que os 814
testes unitários não alcançam. Só na rodada 8, três defeitos apareceram exclusivamente ao
rodar o binário de verdade (`shell_background` exposto, config MCP temporária órfã, pontuação
consecutiva no detector de PII), e o mais grave de todos — nenhuma tool executava no Ollama
por causa do `structuredOutput` *default* — passou por toda a suíte verde.

A causa é conhecida: o que os testes cobrem são **unidades**; o que quebra é a **fiação**
entre elas — parsing de argumento, carga de configuração, registro no `Router`, laço do
agente, `PermissionGate`, guardrails e audit log, nessa ordem, no processo real.

## Convenções

Mesmas de sempre (`docs/roadmap-v0.1.md` §Convenções): DoD padrão (`cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`), *smoke-test* real do
binário para toda mudança observável, skill `micro-ticket-planner` para granularidade.

**Nenhuma dependência nova.** O provider falso é servidor HTTP sobre `std::net::TcpListener`
com parsing mínimo — introduzir `axum`/`hyper` como dependência direta seria parada dura por
ADR-0004, e um servidor de teste não justifica isso. Precedente direto no repositório:
`fake_mcp_server` e `fake_lsp_server` (`crates/core/src/bin/`), consumidos por
`env!("CARGO_BIN_EXE_<nome>")` a partir de testes de integração.

---

## Fase K — Teste de integração ponta a ponta (MT-140 a MT-146)

### MT-140: ADR-0045 — estratégia de teste ponta a ponta ✅ concluído
- **Objetivo:** registrar a decisão antes de qualquer código, porque ela fixa três escolhas
  que depois custam caro para reverter: (a) **provider falso local** em vez de gravação/
  *replay* de tráfego ou de bater em provider real em CI — CI não pode depender de rede,
  credencial ou cota de terceiro, e a matriz de 3 SOs (ADR-0005) multiplicaria qualquer
  flakiness por três; (b) o teste **sobe o binário real** (`CARGO_BIN_EXE_agentry`) como
  subprocesso, e não uma função `run()` interna, porque é exatamente a fronteira do processo
  que os bugs atravessaram; (c) o escopo é a **fiação**, não a lógica de cada unidade, que já
  tem cobertura.
- **Arquivos no escopo:** `docs/adr/0045-*.md` (novo), `docs/adr/README.md`, `mkdocs.yml`,
  `docs/roadmap-v0.18.md` (este arquivo).
- **Critério de aceite:** ADR com os cinco blocos do template; índice atualizado.
- **Fora de escopo:** qualquer código de teste.
- **Depende de:** nenhum.

### MT-141: teste-guarda de hermetismo (`HOME` + `cwd`) ✅ concluído
- **Objetivo:** o escopo original deste ticket — "tornar a raiz de configuração
  redirecionável" — **deixou de existir**. A verificação do MT-140 mostrou que
  `crates/core/src/global_dir.rs` já resolve o home por `std::env::var_os("HOME")`
  (`USERPROFILE` como *fallback*) e é o **único** ponto do workspace que lê essas variáveis:
  definir `HOME` no processo filho já redireciona `~/.agentry/` por inteiro. Não há variável
  nova, flag nova, mudança em código de produção nem emenda à ADR-0038.
  O que resta é o que protege essa propriedade: um **teste-guarda** que falhe se alguém
  introduzir resolução de home fora do `global_dir` (via `dirs`/`directories`, `env::var`
  direto ou caminho hardcoded), porque isso quebraria o isolamento **em silêncio** — os testes
  ponta a ponta continuariam passando enquanto escrevessem no `~/.agentry/` real.
- **Arquivos no escopo:** `crates/core/tests/hermetismo.rs` (novo).
- **Critério de aceite:** com `HOME` apontando para um diretório temporário, nenhuma leitura
  ou escrita ocorre em `~/.agentry/`; o teste falha se surgir outro resolvedor de home no
  workspace.
- **Fora de escopo:** mudar precedência de configuração, formato ou schema.
- **Depende de:** MT-140.
- **Execução:** a guarda foi verificada **por mutação** — introduzido um
  `std::env::var_os("HOME")` num arquivo temporário, o teste falhou nomeando arquivo e linha;
  arquivo removido em seguida. Teste-guarda que nunca ficou vermelho pelo motivo certo não
  prova nada. A segunda asserção (a que protege contra varredura vacuante) pegou um erro real
  na primeira execução: ela afirmava que `global_dir.rs` contém a string `var_os("HOME")`, e o
  arquivo usa indireção (`home_dir_de(|nome| std::env::var_os(nome))` com `buscar_var("HOME")`)
  — a asserção foi corrigida para a forma que o código de fato tem.

### MT-142: `fake_provider` — provider HTTP falso, roteirizado ✅ concluído
- **Objetivo:** binário de teste que responde ao protocolo OpenAI-compatible (o que cobre
  vLLM/LiteLLM/LM Studio pela ADR-0001) com **roteiro determinístico** — a sequência de
  respostas vem de um arquivo JSON passado por argumento, incluindo respostas com
  `tool_calls` e resposta em *streaming* (SSE). Registra as requisições recebidas para o
  teste poder afirmar **o que o `agentry` enviou**, não só o que aceitou de volta.
- **Arquivos no escopo:** `crates/core/src/bin/fake_provider.rs` (novo).
- **Critério de aceite:** teste que sobe o binário, faz uma requisição com `reqwest` (já em
  `dev-dependencies`) e recebe a resposta roteirizada; porta atribuída pelo SO (porta 0) e
  informada em `stdout`, nunca fixa — porta fixa é flaky em CI paralelo.
- **Fora de escopo:** protocolo Anthropic e protocolo Ollama nativo (MT-146).
- **Depende de:** MT-140.

### MT-143: harness ponta a ponta + primeiro caso (conversa sem tool) ✅ concluído
- **Objetivo:** primeiro teste que sobe o **binário real** com `HOME`/raiz de config/cwd
  temporários e um `agentry.settings.json` apontando para o `fake_provider`, e afirma sobre
  `stdout` e código de saída. Fecha a fronteira `main()` → provider pela primeira vez.
- **Arquivos no escopo:** `crates/cli/tests/e2e.rs` (novo — o pacote `agentry` **não tem**
  diretório `tests/` hoje), `crates/cli/tests/fixtures/` (novo).
- **Critério de aceite:** `cargo test -p agentry` executa o caso sem rede e sem tocar
  `~/.agentry/`; falha se o binário responder erro de configuração.
- **Fora de escopo:** tool-calling, guardrails, auditoria (MT-144 a MT-146).
- **Depende de:** MT-141, MT-142.
- **Execução:** o `fake_provider` é `[[bin]]` de `agentry-core`, então
  `CARGO_BIN_EXE_fake_provider` **não** existe nos testes de `agentry` — só o pacote que
  declara o binário ganha a variável. O caminho é derivado do diretório de
  `CARGO_BIN_EXE_agentry` (os dois caem no mesmo lugar), com erro explícito se o binário não
  estiver compilado. Mover a fixture para `crates/cli` resolveria a variável, mas
  `cargo install` passaria a instalar o `fake_provider` na máquina do usuário — inaceitável.

### Achado do MT-143 — resposta vazia com código de saída 0

Primeiro defeito candidato encontrado pela própria suíte, antes mesmo de ela cobrir o que foi
planejada para cobrir. O caminho *one-shot* usa `chat_stream`; quando o endpoint devolve um
corpo que **não** é SSE (o que acontece com endpoint mal configurado, proxy que intercepta, ou
provider que ignora `stream: true`), o binário imprime **nada** em `stdout`, registra
`0 tokens` e sai com **código 0**.

Do ponto de vista de quem usa, é indistinguível de "o modelo não teve o que responder". É
exatamente a classe de falha silenciosa que motivou a Fase K.

**Não virou teste ainda de propósito:** transformar o comportamento atual em asserção o
congelaria. Precisa de decisão do mantenedor sobre qual é o comportamento correto — provável
candidato a **MT-147**: *stream* que termina sem nenhum evento de conteúdo deve produzir erro
tratado e código de saída diferente de zero, não silêncio.

### MT-144: caso de tool-calling — regressão do bug do Ollama ✅ concluído (parcial)
- **Objetivo:** o roteiro faz o modelo pedir uma tool; o teste afirma que o `agentry`
  **executou** a tool sob `PermissionGate` e devolveu o resultado ao modelo. É a regressão
  direta do bug mais grave já encontrado (`structuredOutput` *default* fazendo o modelo
  devolver a tool-call como texto, com nenhuma tool executando). Inclui o caso da tool
  indevidamente exposta (`shell_background`): afirmar **qual é** o conjunto de tools
  oferecido ao modelo, não só que a chamada funciona.
- **Arquivos no escopo:** `crates/cli/tests/e2e.rs`, fixtures.
- **Critério de aceite:** com `structuredOutput` no *default* atual, a tool executa; o
  conjunto de tools enviado ao provider confere com o esperado.
- **Depende de:** MT-143.
- **Execução — o que entrou:** o laço completo de tool-calling ponta a ponta (o modelo pede
  `fs_read`, o `agentry` executa sob `PermissionGate`, o conteúdo lido volta ao modelo na
  segunda ida ao provider, a resposta final chega ao `stdout`) e a afirmação do **conjunto
  inteiro** de tools anunciado.
- **Execução — o que NÃO entrou, e por quê:** a regressão específica do
  `providers.ollama.structuredOutput` **não está coberta**. Esse *flag* é do `OllamaProvider`,
  e o caminho exercitado aqui é o OpenAI-compatible (`litellm`) — o `fake_provider` não fala o
  protocolo nativo do Ollama, que o MT-142 deixou explicitamente fora de escopo. O que ficou
  coberto é a **classe** da falha (tool-call que não executa), não aquele defeito. Fechar de
  verdade exige o **MT-148** abaixo.
- **Nota de fixture:** as fixtures desligam **todas** as funcionalidades de contexto. Fosse só
  as caras, `session_search` continuaria anunciada (o *default* de `sessionSearch` é `true`) e
  a asserção do conjunto quebraria por mudança de *default*, não por regressão.

### MT-148 (proposto): `fake_provider` fala o protocolo nativo do Ollama
- **Objetivo:** fechar de verdade a regressão do defeito mais grave já encontrado. Hoje o
  `fake_provider` só fala OpenAI-compatible, e `structuredOutput` é do `OllamaProvider` — o
  bug pelo qual **nenhuma tool executava na configuração default** continua sem rede de
  proteção ponta a ponta.
- **Arquivos no escopo:** `crates/core/src/bin/fake_provider.rs`, `crates/cli/tests/e2e.rs`,
  fixtures.
- **Critério de aceite:** um caso que roteia pelo provider `ollama` com `structuredOutput` no
  *default* e afirma que a tool **executa**; e um caso que, com `structuredOutput: true`,
  documenta o comportamento degradado — é o que torna a regressão detectável se o *default*
  for revertido por engano.
- **Depende de:** MT-144.

### MT-145: casos de egresso, auditoria e guardrail ✅ concluído
- **Objetivo:** três afirmações que hoje só existem em teste unitário e nunca ponta a ponta —
  (a) `.agentry/audit.log` registra destino e classe de egresso a cada chamada (ADR-0037);
  (b) classe `local-only` **bloqueia** egresso para nuvem, *fail-closed* (ADR-0002); (c) um
  CPF no prompt é bloqueado/redigido antes de sair, e o log registra nome do detector e
  contagem, **nunca o valor** (ADR-0043) — incluindo o caso de pontuação consecutiva, que foi
  defeito real.
- **Arquivos no escopo:** `crates/cli/tests/e2e.rs`, fixtures.
- **Critério de aceite:** os três casos passam; o teste **grep**a o `audit.log` para garantir
  ausência do valor sensível, não só presença do nome do detector.
- **Depende de:** MT-143.
- **Execução:** virou **quatro** casos, porque o *fail-closed* tem **duas camadas** com
  comportamentos diferentes, e cobrir só uma daria falsa segurança:
  1. `audit.log` registra destino e classe (ADR-0037);
  2. **recusa de rota** — candidato mais permissivo que a sessão: o `Router` recusa *antes* de
     qualquer chamada, com código 1 e mensagem explícita;
  3. **bloqueio no `Transport`** — candidato permitido com endpoint mais permissivo: a barreira
     cai na camada que audita, e a entrada `blocked` aparece no log;
  4. CPF bloqueado, com o log nomeando o detector e **sem** o valor, em duas grafias.
- **Achado do processo:** as duas primeiras versões dos casos de bloqueio passaram **por
  vacuidade** — afirmavam só "nenhuma requisição chegou ao provider", o que também seria
  verdade se o binário tivesse falhado por motivo alheio (foi o que aconteceu: o `audit.log`
  estava vazio). Cada caso de bloqueio precisa afirmar **o mecanismo**, não só o efeito.

### MT-149 (proposto): recusa de rota por egresso não deixa trilha persistente
- **Objetivo:** decidir se a recusa **na camada de rota** deve ser auditada. Hoje ela sai só em
  `stderr` com código 1; nada é gravado em `.agentry/audit.log`. O bloqueio no `Transport`, sim,
  grava `blocked`. Descoberto pelo MT-145 ao exigir que o caso provasse o mecanismo.
- **A tensão:** a ADR-0002 manda auditar *cada egresso*, e aqui **não houve egresso** — a rota
  nem foi selecionada, então formalmente não há o que registrar. Do lado de quem audita, porém,
  "o sistema recusou trabalho por política de egresso" é exatamente o evento que se quer ver, e
  hoje ele não sobrevive ao fim do processo.
- **Decisão do mantenedor**, não do implementador: pode ser lacuna de conformidade ou
  comportamento correto. Se for para auditar, o formato precisa distinguir *recusa de rota* de
  *bloqueio de egresso* — colapsar os dois tornaria o log ambíguo.
- **Arquivos prováveis:** `crates/cli/src/main.rs` (resolução de rota),
  `crates/core/src/egress/audit.rs`, `docs/adr/0002-*.md` (emenda).
- **Depende de:** MT-145.

### MT-150: `/help` mostra os atalhos sem o modificador `Ctrl` ✅ concluído (`882abeb`)
- **Como ficou:** o rótulo passou a ser formado a partir da **entrada inteira**
  (`rotulo_atalho(def)`), não de `def.code` isolado — é o que impede a divergência de voltar
  quando um modificador for acrescentado à tabela. A guarda fecha a volta em vez de afirmar
  sobre a forma do texto: o rótulo exibido é **reconstruído em `KeyEvent`** e tem de resolver
  para a ação que ele descreve. Verificada por mutação (3 testes falham se `modifiers` voltar
  a ser ignorado). Conferido na TUI real via `tmux`: `Ctrl+C`, `Ctrl+P`, `Ctrl+A`, `Ctrl+Z`.
- **Objetivo:** `keybind.rs:154` monta cada linha com `rotulo_tecla(def.code)` e **ignora**
  `def.modifiers`, então `Ctrl+C`/`Ctrl+P`/`Ctrl+A`/`Ctrl+Z` aparecem como `c`/`p`/`a`/`z`. A
  ajuda descreve atalhos de letra que o **MT-72 removeu de propósito** — apertar `c` só digita a
  letra. Achado da primeira execução do plano de uso.
- **Arquivos no escopo:** `crates/cli/src/tui/keybind.rs`, testes do módulo.
- **Critério de aceite:** `/help` exibe o modificador; um teste afirma o rótulo **completo** de
  uma tecla com `Ctrl` — o teste atual só verifica se os comandos aparecem, não se a tecla está
  certa, e foi por isso que a divergência passou.
- **Depende de:** nenhum.

### MT-151: `--init` gera configuração que sempre erra na conexão MCP ✅ concluído (`882abeb`)
- **Como ficou:** o exemplo **saiu do mapa** — `"mcpServers": {}`, com o formato num
  `_comentario_mcpServers` irmão (mesma convenção já usada por `subagentPermissions`). Regra
  que fica registrada no teste: bloco cujo valor de exemplo é **executado** não pode ter
  exemplo inerte. Conferido: `--init` numa árvore limpa abre sessão sem erro nenhum.
- **Objetivo:** o template (`main.rs:224-231`) declara `mcpServers.exemplo` com
  `"command": "echo"`, que não fala MCP. Toda execução termina com erro citando
  `rmcp::transport` e "Broken pipe". O comentário do template afirma que a falha seria "tratada,
  não silenciosa" — foi escrito no MT-77, quando **nada conectava**; o MT-78 passou a conectar.
- **Critério de aceite:** instalação nova não produz erro nenhum sem o usuário ter configurado
  nada. Ou o exemplo sai do arquivo (fica só na documentação), ou ganha forma inerte.
- **Depende de:** nenhum.

### MT-152: sob TUI, erro em `stderr` nunca chega ao usuário ✅ concluído (`882abeb`)
- **Como ficou:** a correção não foi escrever em `stderr` mais cedo — foi **não escolher o
  canal no ponto de origem**. Quem produz o aviso registra em `DiagnosticosDeInicializacao`;
  `main` decide a superfície (`stderr` nos modos de texto, mensagem de sistema no histórico
  sob `--tui`). Três avisos cobertos: MCP que não conectou, binário `claude` fora do `PATH`,
  ponte MCP que não montou. O risco da mudança é o oposto do defeito (um coletor nunca
  escoado engoliria o aviso em **todos** os modos), e é o que o caso ponta a ponta prende.
- **Correção ao achado original:** a recusa de rota sob `local-only` **não** estava invisível
  — ocorre antes de a TUI subir e sai em `stderr` com código 1 (verificado). Erro de turno já
  aparecia no chat via `marcar_erro`. O que estava invisível era só a classe de diagnóstico de
  partida.
- **Objetivo:** o `stderr` é descartado sob TUI de propósito (escrever nele corrompe a tela,
  achado do MT-72), mas **erros reais vão por ali** — só aparecem depois que o usuário sai. O caso
  grave é a recusa de egresso: sob `local-only`, quem pede algo roteado para nuvem não recebe
  explicação nenhuma.
- **Relação:** parente do **MT-149**, e distinto — lá falta trilha persistente, aqui falta
  informar **quem está usando**.
- **Critério de aceite:** erro tratado aparece **dentro** da TUI, sem corromper a tela.
- **Bloqueante para o MT-153:** promover a TUI a padrão sem isto tornaria o silêncio a
  experiência de todos, não só de quem optou pela TUI.
- **Depende de:** nenhum.

### MT-153 (proposto): promover a TUI a modo padrão, com `--repl` como saída
- **Objetivo:** o que a ADR-0027 adiou ("só depois dela provar estabilidade"). Decidido pelo
  mantenedor em 2026-09-08, com a ordem também decidida: **rodar o plano de uso primeiro**, o que
  já foi feito (`usage-test/RELATORIO-2026-09-08.md`).
- **Escape hatch decidido:** **`--repl`** — nomeia o modo pelo que ele é, e é o termo que a
  própria ADR-0027 usa. `--tui` continua aceito como *no-op* explícito, para não quebrar script
  nem documentação existente.
- **Requisitos:** *fallback* automático para texto quando `stdout`/`stdin` não forem TTY
  (`std::io::IsTerminal`, sem dependência nova); `agentry "tarefa"` segue sem TUI, byte a byte;
  ADR nova registrando a promoção e citando o relatório como a evidência que a ADR-0027 exigiu.
- **Depende de:** **MT-150, MT-151 e MT-152** — os três atingem a primeira impressão, que é
  exatamente o que vira padrão. O relatório recomenda corrigi-los antes de virar a chave.
  **Desbloqueado em 2026-09-08** (`882abeb`).

### MT-154: o desfecho de uma chamada de tool é invisível na visão padrão ✅ concluído
- **Como ficou:** a linha recolhida ganhou um **marcador de desfecho** (`⚙` em curso, `✓`
  concluída, `✗` falhou/recusada) e, só no caso de erro, o motivo truncado depois de `→`. Os
  três estados são exatamente os três casos de `resultado` (`None` / `false` / `true`), não uma
  classificação nova. O *preview* dos argumentos foi preservado (não regride o MT-116), e a
  saída de **sucesso** continua fora da linha recolhida — ela pode ser enorme, e o marcador já
  diz o que precisa ser dito. A ajuda passou a explicar os três marcadores e o clique de
  expansão, que não era documentado em lugar nenhum.
- **Verificado na TUI real** (via LiteLLM, `qwen3-coder:30b`), os quatro estados no mesmo
  histórico: `⚙` enquanto o modal está aberto, `✗ … → usuário recusou a execução de 'fs_write'`,
  `✓ tool: fs_write — {…}` e `✗ … → tool 'fs_edit' bloqueada por política (deny)`.

### MT-157 (proposto): `/tmp/agentry-mcp-<pid>.json` vaza em toda saída por `std::process::exit`
- **Objetivo:** o bloco recolhido mostra nome + início dos argumentos e **nunca o resultado**
  (`linhas_logicas_do_bloco_de_tool`, ramo `if !expandido`). Tool recusada, tool negada por
  `deny` e tool executada com sucesso renderizam **linhas idênticas**. O resultado existe e é
  bom (`usuário recusou a execução de 'fs_write'`, `tool 'fs_edit' bloqueada por política
  (deny)`), mas só aparece expandido — e expandir é **clique de mouse** (MT-117/ADR-0035), sem
  equivalente de teclado, sem pista na tela e sem menção no `/help`. Em terminal sem mouse, a
  informação é inalcançável. Achado do bloco E do plano de uso (`RELATORIO-2026-09-08-bloco-e`).
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs` (bloco recolhido e texto de ajuda).
- **Critério de aceite:** sem nenhuma interação, dá para distinguir na tela uma tool que rodou
  de uma que não rodou; um teste afirma que as duas formas recolhidas **diferem**. Que a ajuda
  passe a mencionar a expansão é parte do mesmo ticket.
- **Fora de escopo:** trocar o mecanismo de expansão por teclado (é decisão de UX à parte).
- **Depende de:** nenhum.

- **Objetivo:** a ponte MCP do `claudeCli` (ADR-0042) escreve
  `/tmp/agentry-mcp-<pid>.json` e conta com o `Drop` de `PonteMcp` para removê-lo. `Drop` **não
  roda** quando o processo termina por `std::process::exit`, que é como `main` encerra **todo**
  caminho de erro (rota irresolúvel, falha do provider, credencial ilegível). Como
  `claudeCli.mcpTools` vem ligado no exemplo do `--init`, qualquer pessoa que erre a
  configuração acumula um arquivo por tentativa.
- **Como foi isolado (2026-09-09):** cinco arquivos encontrados em `/tmp`, todos de PIDs mortos.
  Instrumentando o `Drop`: numa execução **bem-sucedida** (one-shot contra LiteLLM) ele roda e o
  arquivo some; numa saída limpa do **`--tui`** (`Ctrl+C`) ele também roda; numa execução que
  termina em erro, **não roda**. Ou seja: não é a TUI nem o caminho feliz — é `process::exit`.
- **Escolha real:** (a) remover o arquivo explicitamente antes de cada `exit`, que espalha a
  limpeza por ~15 pontos e volta a quebrar no próximo `exit` novo; ou (b) `main` passar a
  devolver código de saída em vez de chamar `process::exit`, deixando o desempilhamento rodar —
  corrige esta e qualquer limpeza futura que dependa de `Drop`. **(b) é o conserto; (a) é o
  curativo.**
- **Critério de aceite:** teste ponta a ponta que roda o binário num caminho de **erro** e
  afirma que nenhum `agentry-mcp-*.json` sobra. O harness da Fase K já suporta.
- **Depende de:** nenhum. Era um item embutido no MT-146; virou ticket próprio ao ganhar causa
  raiz e por não ter relação com CI.

### MT-155 (proposto): negação de tool por política não deixa trilha persistente
- **Objetivo:** com `permissions.deny`, a negação produz o efeito certo (verificado no E5, com
  `[auto]` ligado) e **nenhuma linha** em `.agentry/audit.log`, que só registra egresso.
- **Relação:** é o **mesmo formato do MT-149** (recusa na camada de rota não auditada) — todo
  caminho *fail-closed* do projeto acerta o efeito e não deixa registro. Decidir os dois juntos:
  ou a auditoria cobre só egresso (e isso vira texto explícito na ADR-0002), ou passa a cobrir
  decisão de política, e aí os dois casos entram pela mesma porta.
- **Critério de aceite:** decisão registrada em ADR; se for auditar, entrada persistente com o
  motivo, e caso ponta a ponta afirmando-a.
- **Depende de:** decisão do mantenedor (junto do MT-149).

### MT-156 (proposto): o modal de confirmação de escrita não mostra o caminho do arquivo
- **Objetivo:** em `linhas_de_confirmacao`, quando há *diff* o modal mostra `tool: <nome>` + o
  *diff*, e o ramo que monta `argumentos: {…}` — onde vive o `path` — não roda. Ou seja: é
  exatamente em `fs_write`/`fs_edit`, as tools em que o alvo mais importa, que o caminho some.
  O modal é a última barreira antes de escrever em disco.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`, testes do módulo.
- **Critério de aceite:** o caminho aparece no modal junto do nome da tool; um teste afirma que
  o `path` da chamada está nas linhas de confirmação **também** no caso com *diff*.
- **Depende de:** nenhum.

### MT-146: fiar no CI (e o que fica de fora)
- **Objetivo:** rodar os testes ponta a ponta na matriz de 3 SOs, ou decidir e **registrar**
  um recorte menor. Ponto de decisão real: o `fake_provider` sobe processo e abre socket, o
  que é a parte mais frágil em Windows e macOS; se a matriz completa se mostrar instável, a
  saída honesta é rodar ponta a ponta só em Linux e declarar isso no ADR-0045, não deixar
  teste intermitente na matriz. Inclui a regressão da config MCP temporária órfã (arquivo
  removido ao fim do processo — **movida para o MT-157**, que tem a causa raiz).
- **Arquivos no escopo:** `.github/workflows/ci.yml`, `docs/testing.md`, `docs/adr/0045-*.md`
  (se o recorte mudar).
- **Critério de aceite:** CI verde nos SOs escolhidos, em duas execuções seguidas — uma só
  não distingue teste estável de teste com sorte.
- **Depende de:** MT-143, MT-144, MT-145.

---

## Pontos verificados no MT-140 (2026-09-07)

Os três pontos levantados no planejamento foram confirmados **no código** antes de escrever o
ADR-0045. Resultado:

1. **Nomes das chaves de configuração — confirmados**, o plano já usava os corretos:
   `structuredOutput` (`config/mod.rs:216`), `baseUrl` (`:237`, `:281`), `egressClass`
   (`:246`, `:286`), `readAllow` (`:108`).
2. **Redirecionar `~/.agentry/` — já é possível, sem mudar produção.** `global_dir.rs:21`
   resolve o home por `env::var_os("HOME")`/`USERPROFILE` e é o único leitor dessas variáveis
   no workspace. **Consequência:** o MT-141 encolheu de "mudança em código de produção" para
   "teste-guarda", e o risco de emenda à ADR-0038 desapareceu.
3. **Caminho de configuração por argumento — não existe**, e não passa a existir.
   `state_dir::agentry_settings_path(start)` sobe a árvore a partir de um caminho dado, então
   o `cwd` do processo filho seleciona a configuração de projeto. A presunção do MT-143 se
   confirma, com uma ressalva registrada no ADR-0045: o diretório temporário **não pode**
   estar aninhado sob um diretório que contenha `.agentry/`, ou a busca acharia a configuração
   do próprio repositório.
