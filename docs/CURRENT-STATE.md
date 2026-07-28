<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> **Fonte central de sincronização — contém apenas o estado corrente.**
> Atualizado a cada commit. Não inclua segredos. Mantido conforme a skill `handoff-updater`.
>
> **Histórico completo:** [`docs/handoff-arquivo.md`](./handoff-arquivo.md) — rodadas
> anteriores, fases fechadas e a tabela de commits. Consulte sob demanda; não é necessário
> ler para retomar o trabalho.

## Último turno

- **Data:** 2026-07-28
- **Branch:** `main`
- **Commit:** `faa407f`
- **Estado da árvore:** limpa · **DoD:** `fmt`/`clippy` limpos, **795 testes** verdes, build
  `release` OK.

## Metas cumpridas neste turno

- [x] **`2955651`** — `fix(ollama)`: `structuredOutput` *default* `false`. Bug crítico:
      nenhuma tool executava na configuração *default*. Emenda à ADR-0012.
- [x] **`815bb35`** — `feat(anthropic)`: provider `anthropic` (Messages API por chave)
      registrado na CLI. ADR-0040 parte A.
- [x] **`08816a8`** — `feat(claude-cli)`: provider `claude-cli` (assinatura Pro/Max via
      subprocesso, com auditoria equivalente). ADR-0040 parte B.
- [x] **`34376f0`** — `docs(handoff)`: relato da rodada 7.
- [x] **`0e7cdba`** — `docs(handoff)`: separa estado corrente de histórico; skill exige a
      divisão (replicada no `ai-coding-agent-profiles`).
- [x] **`d0153f6`** — `docs(handoff)`: achados de confidencialidade da rodada 8.
- [x] **`275b682`** — `fix(auditoria,ask_user)`: audit distingue destino local de nuvem; EOF
      não vira laço.
- [x] **`f394f23`** — `feat(ux)`: prompt do REPL e título da TUI mostram a rota ativa.
- [x] **`8781abc`** — `docs(handoff)`: rodada 8b (llama3.1 + arquitetura invertida).
- [x] **`bf7867f`** — ADR-0041: `readAllow` (escopo de leitura por caminho) +
      `subagentPermissions`.
- [x] **`f9fa40f`** — ADR-0042: modo `--mcp-server` expondo as tools sob a política.
- [x] **`faa407f`** — ADR-0042: ponte MCP no `claude-cli` — assinatura Pro/Max com
      tool-calling.

## Rodada 8 (2026-07-27) — roteamento entre modelos e confidencialidade

Pedido do mantenedor: testar o `agentry` com o modelo local (Ollama) tratando dados
confidenciais e delegando à nuvem (Claude) **apenas o não-confidencial**, via `taskClasses`;
observar também usabilidade/fluidez sem perder segurança.

**Cenário montado** (`taskClasses`): `chat` → `ollama`/`qwen2.5:7b`/`local-only`;
`nuvem` → `claude-cli`/`haiku`/`cloud-ok`. Sessão sob perfil `pessoal`. Delegação pela tool
`subagent` com `task_class='nuvem'`. Egresso para a nuvem capturado com um *wrapper* do
binário `claude` (audita o que de fato sai da máquina).

### Achados — confidencialidade (ordenados por gravidade)

1. **Nada impõe a sanitização na fronteira local→nuvem.** Com instrução explícita, o modelo
   local sanitizou corretamente (só `plano`/`mrr`/`churn_risco` saíram — verificado). **Sem**
   a instrução, o CSV inteiro com nomes, CPFs e e-mails saiu da máquina. A proteção depende
   inteiramente de um modelo 7B obedecer — não há mecanismo do `agentry` no caminho.
2. **Guardrails são só correspondência literal (ADR-0007, "sem regex") — o modelo contorna.**
   Com `guardrails.input/output = [{match:"cpf", action:"block"}]`, o payload com o literal
   `cpf` foi bloqueado; o modelo então **reformatou os dados sem aquela coluna e mandou os
   nomes assim mesmo**. Bloquear literal dá falsa sensação de proteção: PII precisa de padrão
   (CPF/e-mail/telefone), não de literal.
3. **O audit log registra *que* houve egresso, não *o que* saiu.** Não há como verificar nem
   detectar o vazamento depois do fato — foi preciso instrumentar o binário externo para
   descobrir. Para o objetivo de conformidade do projeto, é a lacuna central.
4. **O audit log registra a classe de egresso da *sessão*, não a do destino.** Toda chamada
   puramente local ao Ollama aparece como `classe=cloud-ok` (a do perfil). Quem audita não
   distingue "ficou na máquina" de "foi para a nuvem" a não ser conhecendo os *hosts*.

### Achados — usabilidade

5. **Laço de `ask_user` em modo *one-shot*, sem humano para responder.** O modelo perguntou e
   "esperou" repetidamente até o teto de turnos; uma execução gastou 19k tokens sem entregar
   resposta. Em *one-shot* não há interatividade: a tool deveria falhar cedo e explicitamente.
6. **Nenhuma indicação de qual modelo/provider respondeu cada turno.** Num cenário
   multi-modelo — exatamente o que este teste exercita — é a informação que dá (ou tira) a
   confiança do usuário sobre onde seus dados foram parar.
7. **O subagente foi invocado 4× para um pedido**, sem nenhuma visibilidade disso na saída.

### Correções entregues nesta rodada

- **Achado 4 ✅ `275b682`** — `Allowlist::check` passa a devolver a classe exigida pelo
  destino (já a calculava e descartava); `AuditEntry` ganha `destination_class`. O log agora
  distingue `destino local-only, sessão cloud-ok` de `cloud-ok`, no terminal e no
  `.agentry/audit.log`.
- **Achado 5 ✅ `275b682`** — `InteractivePrompter` trata `Ok(0)` (EOF) como ausência de
  humano, devolvendo ao modelo um texto que diz que ninguém responderá **e o que fazer em
  seguida**. Medido no mesmo pedido: **19k → 5.8k tokens**, 4 → 1 chamada ao subagente.
- **Achado 6 ✅ `f394f23`** — `repl::rotulo_de_rota` (fonte única) alimenta o prompt do REPL
  (`⌂ ollama:qwen2.5:7b >`) e o título da TUI (` agentry — ↗ claude-cli:haiku `), com `⌂`/`↗`
  sinalizando se a mensagem sai da máquina.

### Achados 1-3: em aberto, exigem decisão do mantenedor

São **arquiteturais** e não têm correção segura sem ADR. O ponto comum: hoje a confidencialidade
na fronteira local→nuvem depende de um modelo pequeno obedecer a uma instrução, e não há como
verificar depois se ele obedeceu.

Direções possíveis (nenhuma escolhida):
- **Guardrails com padrão**, não literal — emendar a ADR-0007, que hoje proíbe regex.
  Sem isso não há como expressar CPF/e-mail/telefone.
- **Guardrail obrigatório na saída do `subagent`** quando o alvo é mais permissivo que a
  sessão-mãe — hoje a delegação cruza a fronteira sem inspeção dedicada.
- **Registrar no audit log um resumo verificável do que saiu** (ex.: hash + contagem de
  padrões sensíveis detectados), sem gravar o conteúdo — permitiria detectar vazamento depois
  do fato sem transformar o log num novo repositório de dados sensíveis.

### Rodada 8b — `llama3.1:8b` e a arquitetura invertida (2026-07-28)

**Teste repetido com `llama3.1:8b`** (mesmos prompts da Fase D): modo de falha **diferente**
do `qwen2.5:7b`, mesma conclusão. Com instrução de sanitizar, chamou o `subagent` mas passou
como prompt um fragmento da instrução do usuário — delegação inútil, sem vazamento. **Sem** a
instrução, entrou em laço: 25 chamadas ao subagente com o prompt "Analisar retenção", parou no
teto do ADR-0033, **72k tokens**. Não vazou porque nunca conseguiu passar dado nenhum.

Conclusão reforçada: a sanitização é **loteria de modelo**. `qwen2.5:7b` compõe prompts
razoáveis mas vaza tudo quando não é instruído; `llama3.1:8b` não vaza, mas também não delega.
Nenhum dos dois é confiável, e nada no `agentry` está no caminho.

**Efeito colateral achado:** o teto de turnos protege o laço, não a fatura — as 25 iterações
foram 25 chamadas reais à nuvem. Não há teto de *chamadas de subagente* por turno.

**Arquitetura invertida (Claude como sessão principal) — VERIFICADA, funciona hoje.** Com
`providers.anthropic` (chave de API) na task-class `chat` e o Ollama numa task-class `local`,
o rastro de auditoria mostra exatamente o padrão desejado: `cloud-ok` (Claude recebe a tarefa)
→ `destino local-only` (subagente lê o arquivo confidencial) → `cloud-ok` (Claude recebe só o
resumo). Nenhuma PII chegou ao Claude. Verificado com mock da Messages API devolvendo um
`tool_use` de `subagent`.

**Lacuna de isolamento (verificada em código e em execução):** `register_subagent_tool`
(`main.rs`) passa as **mesmas** `Permissions` da sessão-mãe ao registry do subagente. Logo,
negar `fs_read` para impedir o Claude de ler também **cega o subagente** — confirmado: com
`permissions.deny=["fs_read"]` o subagente passou a inventar o conteúdo do CSV. Hoje só é
possível *pedir* ao Claude que não leia, não *impedir*.

### Rodada 8c — ADR-0041 implementada (2026-07-28)

Mantenedor escolheu a estratégia **(b)** (permissões próprias para o subagente) e acrescentou
o requisito de o agente principal continuar lendo parte do repositório (`README.md`,
`AGENTS.md`, `docs/`, `skills/`) — o que transformou "negar `fs_read`" em **escopo por
caminho**, granularidade certa do problema.

**ADR-0041 (Accepted)** — dois mecanismos independentes e combináveis:

- **`readAllow`** em `permissions`: lista fechada de caminhos legíveis, sintaxe `.gitignore`
  (crate `ignore` já presente, nenhuma dependência nova). **Allowlist, não denylist** —
  arquivo confidencial novo nasce protegido. Ausente ⇒ sem restrição (nada muda para quem já
  usa). A decisão vive no `PermissionGate`, único ponto de estrangulamento, e normaliza o
  argumento `path` pela **mesma** função da tool (`resolve_within_root`) — `../` e caminho
  absoluto recusados antes da comparação.
- **`subagentPermissions`**: conjunto próprio do subagente; ausente ⇒ herda. **Substitui, não
  soma** — somar não conseguiria *devolver* ao subagente uma tool negada ao principal, que é
  o objetivo.

Detalhe de desenho: entre camadas, `deny`/`ask` continuam somando (mais restritivo é o lado
seguro de errar), mas `readAllow` **substitui** — somar uma *allowlist* a afrouxaria, e uma
camada herdada poderia reabrir por acidente um caminho que a camada específica quis fechar.

**Verificado com o binário `release`** (mock da Messages API pedindo os dois arquivos): o
agente principal **lê `README.md`** e é **bloqueado em `clientes.csv`** (`tool 'fs_read'
bloqueada por política (deny)`), e a delegação ao Ollama roda em seguida. A invariante
central (`subagente_le_o_que_o_agente_principal_nao_pode`) é testada sobre os *gates*, não
ponta a ponta: um modelo local que simplesmente não chama a tool produz o mesmo "nada
aconteceu" de um bloqueio e não distinguiria os dois casos.

**Limitação declarada na ADR e na documentação:** `readAllow` só alcança tools com argumento
`path`. `shell_exec`/`glob`/`fs_search` leem por outras vias e **precisam** entrar em `deny`.
E o mecanismo garante que nenhuma leitura confidencial ocorre **sem passar por um modelo
local** — não que a resposta dele venha sanitizada (isso depende dos achados 1-3, ainda
abertos).

### Rodada 8d — ADR-0042: servidor MCP, assinatura Pro/Max como agente principal

Fecha o caminho **(c)** da rodada 8b, o último pendente. Três incógnitas foram resolvidas
**experimentalmente antes de decidir**, com um servidor MCP descartável e o `claude` real:

1. `--tools ""` desliga as ferramentas embutidas mas **preserva as de MCP** — com
   `--strict-mcp-config`, a única coisa visível é `mcp__agentry__*`. É a propriedade central.
2. Em *headless* a aprovação não é interativa: sem `--allowedTools` o modelo só responde
   pedindo permissão. `mcp__agentry` (nível de servidor) libera tudo daquele servidor.
3. O ciclo completo funciona: tool chamada com os argumentos certos, resultado de volta.

**Entregue:** `agentry --mcp-server` serve o `ToolRegistry` já montado pela configuração —
mesmas tools, mesmo `PermissionGate`. Servir a partir do ponto em que `main()` já construiu o
registry (em vez de montar um próprio) é o que impede o servidor de virar porta lateral em
volta da ADR-0041. `providers.claudeCli.mcpTools` liga a ponte no provider.

Decisão registrada: **expor o registry inteiro**, não só o `subagent`. Com `readAllow` o
agente principal já é contido por caminho, e obrigá-lo a delegar até a leitura do `README.md`
o deixaria inútil para planejar — que é o papel dele nesta arquitetura.

**Verificado ponta a ponta com a assinatura Pro/Max real**, num único comando `agentry`: a
sessão do Claude lista só `mcp__agentry__*`, lê `README.md`, é **bloqueada** em
`clientes.csv` (`tool 'fs_read' bloqueada por política (deny)`) e reconhece sozinha que deve
delegar via `subagent` (as `instructions` do handshake ensinam o padrão).

**Bug achado e corrigido no processo:** o próprio processo `--mcp-server` construía o provider
`claude-cli` e montava uma **segunda** ponte, cujo arquivo temporário nunca era removido — o
servidor é encerrado pelo cliente, sem rodar destrutores. Sobrava um
`/tmp/agentry-mcp-<pid>.json` por execução. Um servidor MCP não tem motivo para lançar o
`claude`; corrigido com teste de regressão.

**Limitação declarada na ADR e na documentação:** com `mcpTools` o **laço de agente pertence
ao Claude Code**. O que é por tool continua valendo (permissões, `readAllow`, *checkpoints*,
audit log); o que é por turno da `Session` **não se aplica** — guardrails de conteúdo
(ADR-0007), teto de turnos (ADR-0033) e compactação (ADR-0016). Quem precisa dessas garantias
usa o provider `anthropic`.

## Em andamento

Nada em execução. Árvore limpa.

## Próximo passo sugerido

Decisões do mantenedor, em ordem de impacto:

0. **Achados 1-3 da rodada 8** (confidencialidade na fronteira local→nuvem) — as três
   direções possíveis estão listadas acima; nenhuma é segura de escolher sem ADR.
1. **Teste de integração ponta a ponta em CI** — causa estrutural dos três bugs de produção
   já encontrados (todos na fronteira `main()` → provider real, que os 765 testes unitários
   não cobrem). Maior retorno disponível hoje; acima de qualquer feature nova.
2. **Ponte MCP para o `claude-cli`** — expor as tools do `agentry` como **servidor** MCP daria
   paridade agêntica ao provider de assinatura (hoje só texto). Registrado na ADR-0040 como
   trabalho futuro; é um projeto próprio, não um ticket.
3. **Guardrail de imagem** (bloqueia a frente multimodal) — ver *Impedimentos abertos*.

## Impedimentos de ambiente (não são bugs do código)

- **`protoc` não vem pré-instalado por padrão** (nem, presumivelmente, nos runners padrão do GitHub Actions) — exigido pelo build script de `lance-encoding` (transitiva do `lancedb`, MT-27). CI já corrigido; ambientes de desenvolvimento locais precisam instalar `protobuf-compiler` (Debian/Ubuntu), `protobuf` (Homebrew) ou equivalente antes de rodar `cargo build`/`cargo test` neste crate — ver `docs/testing.md`. **Nesta máquina de desenvolvimento, já resolvido**: `protobuf-compiler` instalado via `apt` pelo usuário (precisa de `sudo` — funciona só com terminal interativo; o agente não deve tentar rodar `sudo` sozinho, sempre pedir para o usuário rodar). Um binário `protoc` *standalone* baixado manualmente mais cedo na sessão (`~/.local/bin/protoc`, contornando a falta de `sudo` interativo) foi removido para não sombrear o `/usr/bin/protoc` do pacote no `PATH` — `cargo build`/`test`/`clippy` voltaram a funcionar sem nenhuma variável de ambiente extra (`PROTOC`/`PROTOC_INCLUDE`).

## Impedimentos abertos

- **Roadmap de longo prazo original esgotado — só resta multimodal, bloqueada.** As Fases
  11 a 20 estão todas concluídas. A última fase (21+, multimodal —
  `docs/roadmap-longo-prazo.md` §Fase 21+) está explicitamente bloqueada pela própria
  resposta do mantenedor (2026-07-16, `docs/decisoes-autonomas.md`): adiada até existir um
  *guardrail* de imagem (ex.: OCR alimentando as regras de texto já existentes,
  `crates/core/src/guardrail/`) — os *guardrails* de conteúdo hoje só inspecionam texto, e
  multimodal sem esse pré-requisito abriria um canal de conteúdo não auditado. Esse
  pré-requisito provavelmente exige uma **dependência nova** (biblioteca de OCR), o que já é
  uma parada dura por si só (regra §6 do comando `/loop /implementar-roadmap`). **Decisão que
  o mantenedor precisa tomar:** como endereçar o *guardrail* de imagem (qual biblioteca de
  OCR, se for esse o caminho, ou uma abordagem alternativa), ou se prefere apontar uma nova
  frente de trabalho fora das cinco originalmente enumeradas no roadmap de longo prazo. O
  loop autônomo parou aqui, com a árvore de trabalho limpa e as Fases 11-20 inteiras
  concluídas.
- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido pela empresa; adapter adiado.
- **CI multi-SO ainda não observado verde:** a matriz do ADR-0005 (`2feed85`) precisa de um push ao GitHub para confirmar Windows/macOS verdes.
- **Verificação de "processo não órfão" do MT-23 é Unix-only de fato:** `processo_existe` (`crates/core/tests/lsp_client.rs`) usa `kill -0`; no branch `#[cfg(not(unix))]` sempre devolve `false`, então em Windows os testes `ciclo_de_vida_completo_start_initialize_shutdown`/`drop_sem_shutdown_explicito_nao_deixa_processo_orfao` passam vacuamente (não verificam nada de verdade) — o `Child::wait()`/`kill()` internos do `LspClient` continuam corretos, só falta uma verificação real de ausência de processo em Windows (ex.: via `tasklist`) quando a matriz de CI (ADR-0005) rodar de verdade.

