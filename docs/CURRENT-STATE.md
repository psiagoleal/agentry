<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> **Fonte central de sincronização — contém apenas o estado corrente.**
> Atualizado a cada commit. Não inclua segredos. Mantido conforme a skill `handoff-updater`.
>
> **Histórico completo:** [`docs/handoff-arquivo.md`](./handoff-arquivo.md) — rodadas
> anteriores, fases fechadas e a tabela de commits. Consulte sob demanda; não é necessário
> ler para retomar o trabalho.

## Último turno

- **Data:** 2026-09-12
- **Branch:** `chore/build-linux-e-higiene-de-disco` — **enviada ao remoto** em 2026-09-12
  (39 commits), ainda **não mesclada em `main`**
- **Commits desta rodada:** `825a765`, `5027a83`, `bee6c74`, `d743d64`, `d4e7151`,
  `f23474c`, `dd4ce27`, `fb08f6b` — rodada anterior (MT-150 a MT-157)
  arquivada em [`handoff-arquivo.md`](./handoff-arquivo.md) como Rodada 11
- **Estado da árvore:** `AGENTS.md`, `skills/README.md` e os diretórios de skills não rastreados
  seguem modificados de **outra frente**, intocados. Há um binário `agentry` de ~280 MB solto na
  raiz (não rastreado, fora do `.gitignore`) — cópia de build; remover é decisão do dono.
- **DoD:** `cargo fmt` aplicado, `cargo clippy --workspace --all-targets -- -D warnings` com
  saída **0**, suíte completa **858 testes verdes**.

## Metas cumpridas neste turno

- [x] **`825a765`** — avaliação do `agentry` como *harness*
      ([`analise-harness-engineering.md`](./analise-harness-engineering.md)) e abertura da frente
      **MT-159 a MT-163**. O mapeamento é majoritariamente favorável; as cinco lacunas têm o
      mesmo eixo — **o projeto sabe impedir e não sabe contar o que aconteceu**.
- [x] **`5027a83`** — **MT-158**: `AGENTRY_LITELLM_BASE_URL`/`_MODEL` na camada de ambiente, o
      que torna o exemplo versionado utilizável sem edição. `egressClass` **não** ganhou
      variável: endereço é conveniência, classe de egresso é política.
- [x] **`bee6c74`** — **MT-164** registrado.
- [x] **`d4e7151`** — gateway interno passa a ser **`cloud-opt-out`**, não `cloud-ok` (decisão do
      mantenedor). A taxonomia descreve a **fronteira de confiança**, não a distância de rede;
      declarar um gateway interno `cloud-ok` forçaria afrouxar a sessão inteira para falar com um
      endpoint mais confiável que a média.
- [x] **`f23474c`** — **ADR-0046 (Proposed)**: hooks de ciclo, e avaliação do núcleo `cepia`.
- [x] **`dd4ce27`** — **MT-159**: `StopReason::Impasse`, e `mensagem_de_parada` passa a cobrir
      **todos** os motivos — parar em silêncio era o mesmo defeito de atribuição que o ticket
      corrige.
- [x] **`fb08f6b`** — **MT-146**: o primeiro CI da vida do projeto reprovou, e reprovou
      **exatamente o que a Fase K existia para descobrir**. Três testes de montagem do provider
      `claude-cli` falharam nos **três** SOs: o *runner* não tem `claude` no `PATH`. Estavam
      verdes havia meses porque toda máquina de desenvolvimento do projeto tem o Claude Code
      instalado — não testavam a montagem, testavam a estação de trabalho. Terceiro membro da
      mesma família (`HOME`/MT-141, `AGENTRY_*`/MT-158, agora `PATH`), com a guarda estática
      junto das outras duas. **A hipótese da ADR-0045 §5 não se confirmou:** os casos ponta a
      ponta passaram nos três SOs; a fragilidade não estava em subir processo e abrir socket.
- [x] **`d743d64`** — teste de uso da TUI contra o **gateway real**
      ([`RELATORIO-2026-09-10-litellm.md`](../usage-test/RELATORIO-2026-09-10-litellm.md)):
      10 cenários, os quatro marcadores do **MT-154** confirmados na tela, e a negação por
      `deny` exercitada com `[auto]` **ligado**. Evidência conferida contra o estado em disco e
      contra as 15 chamadas do `audit.log`. **Leia com a ressalva registrada no próprio
      relatório:** ambiente pré-verificado remove por construção a classe de achado de primeira
      configuração — que foi exatamente onde os MT-150/151/152 apareceram.

Dois defeitos encontrados **ao verificar** o MT-158 contra o gateway real, corrigidos junto:

- **Variável exportada vazia contava como valor declarado.** A camada de ambiente é a **última**
  de `Config::resolve`, então um `export AGENTRY_LITELLM_BASE_URL=` de espaço reservado apagaria
  em silêncio o que o arquivo do projeto declarou. Vazio agora conta como ausência.
- **`build_config` lia o ambiente real dentro da função que os testes usam.** Dois testes de
  precedência quebraram quando `AGENTRY_MODEL` foi exportada no `.zshrc` — não por regressão:
  eles ficaram verdes por dois anos só porque nenhuma máquina tinha `AGENTRY_*` exportada. Não
  verificavam precedência, verificavam o `.zshrc` de quem rodava. Camada agora é injetada, com
  guarda estática irmã da guarda de `HOME` do MT-141.

## Em andamento

**MT-146 — falta confirmar o CI verde.** A branch está no remoto e o CI **roda de verdade** (o
repositório é público, então Actions é gratuito e ilimitado). As duas primeiras execuções
reprovaram, a causa foi corrigida em `fb08f6b`, e o critério de aceite — **duas execuções verdes
seguidas** — ainda não foi observado. É o primeiro passo de quem retomar.

Contexto anterior do ticket: O código está pronto: o CI já rodava
`cargo test --all`, então os casos ponta a ponta **já entram** na matriz de três SOs — não
faltava fiação. O que faltava era portabilidade real, e havia um defeito concreto: o caso do
MT-157 redirecionava o diretório temporário só por `TMPDIR`, que o Windows ignora, então ele
teria passado **por vacuidade** lá (afirmando sobre um diretório que o binário nunca usou).
Corrigido; o resto do e2e foi auditado e não precisa de recorte por SO.

Falta o critério de aceite: **CI verde em duas execuções seguidas**. Isso exige `push` da
branch `chore/build-linux-e-higiene-de-disco`, que nunca foi enviada — **não fiz por conta
própria**. A ADR-0045 §5 **não** foi emendada de propósito: a saída acordada lá (restringir o
e2e a Linux) depende de instabilidade **observada**, e ainda não houve execução de CI.

**Gateway LiteLLM alcançável desta máquina** — endereço e chave ficam **fora do repositório**.
`qwen3-coder:30b` faz *tool-calling*. **Nunca ecoar a chave:** `agentry --set-credential litellm`
lê de `stdin` (grava `0600`, mantém o valor fora de `argv`). Desde o MT-158 o endereço vem de
`AGENTRY_LITELLM_BASE_URL`, então `usage-test/exemplos/02-gateway-litellm.json` roda sem edição.
Existe a skill `delegacao-litellm` para mandar trabalho volumoso ao gateway em vez de gastar
cota da assinatura.

**Decisão pendente de política, agora com consequência prática.** O `.zshrc` do mantenedor
exporta `AGENTRY_PROFILE=externo-confidencial` (⇒ `cloud-opt-out`), e o gateway está declarado
`cloud-ok` — logo o `Router` **recusa** a rota e cai para o candidato local. Funcionou como
projetado, mas bloqueou uma rodada inteira de teste de uso antes de alguém desconfiar do
ambiente. Escolha uma: declarar o gateway `cloud-opt-out` (leitura: "é da empresa, não treina
com o dado") ou usar `pessoal`. Note que `empresa` mapeia para `local-only`
(`config/privacy.rs`), então sob o perfil corporativo um gateway interno só é alcançável se for
declarado `local-only` — aqui `local-only` significa "não sai da fronteira de confiança", não
"não sai da máquina".

## Próximo passo sugerido

1. **MT-159** — `StopReason::Impasse`. O item de maior retorno da frente nova e o mais barato:
   hoje um agente preso queima os 25 turnos e reporta `MaxTurnsExceeded`, que **atribui a causa
   errada** — quem lê conclui que o teto está baixo e o aumenta.
2. **MT-164** — mostrar a camada de origem de cada valor da configuração. Custou uma rodada de
   teste de uso; é diagnóstico que qualquer pessoa vai precisar.
3. **MT-146** — fiar os testes ponta a ponta no CI; é o **único** ticket restante da Fase K.
   Só falta o `push` (ver *Em andamento*).
4. **MT-153** — promover a TUI a modo padrão, com `--repl` como escape hatch e *fallback* para
   texto quando não houver TTY (`std::io::IsTerminal`). **Desbloqueado** por `882abeb`; exige
   ADR nova, citando o relatório de uso como a evidência que a ADR-0027 pedia.
5. **Implementar a ADR-0044** — providers por assinatura via CLI oficial (Codex/Gemini) e por
   API. **Pré-requisito duro, ainda não verificado:** confirmar que cada CLI autoriza consumo
   programático sob a assinatura e qual o modo *headless* suportado. Quebrar com
   `micro-ticket-planner`: o molde da ADR-0040 exige `AuditEntry` por invocação e
   `EgressClass` verificada antes do *spawn* em cada provider.
6. **Detector de nome de pessoa** — lacuna que a ADR-0043 declara e não resolve; custo de falso
   positivo alto, decisão do mantenedor.

## Decisões pendentes do mantenedor

Nenhuma virou asserção, para não congelar comportamento antes da decisão:

- **MT-147** — corpo **não-SSE** numa requisição de *stream* produz `stdout` vazio, `0 tokens` e
  **código 0** — indistinguível de "o modelo não teve o que responder", e é o que acontece com
  endpoint mal configurado ou proxy que intercepta.
- **MT-148** — a regressão do `providers.ollama.structuredOutput` (nenhuma tool executando no
  *default*) **segue sem rede de proteção**: o *flag* é do `OllamaProvider` e o `fake_provider`
  só fala OpenAI-compatible. O MT-144 cobriu a *classe* da falha, não aquele defeito.
- **MT-149 + MT-155** — **decidir juntos**: nem a recusa na camada de rota (MT-149) nem a
  negação de tool por `permissions.deny` (MT-155) deixam trilha persistente, enquanto o bloqueio
  no `Transport` deixa. É o mesmo formato: todo caminho *fail-closed* do projeto acerta o efeito
  e não registra nada. Ou a auditoria cobre só egresso — e isso vira texto explícito na ADR-0002
  —, ou passa a cobrir decisão de política, e os dois entram pela mesma porta.

## O que saber antes de mexer nos testes ponta a ponta

Sete armadilhas já pagas (hermetismo, `CARGO_BIN_EXE`, *defaults* de contexto, SSE, as duas
camadas do *fail-closed*, o invariante do `Ctrl+A` e o método de `tmux`) estão registradas em
[`docs/roadmap-v0.18.md`](./roadmap-v0.18.md), junto do MT-146. **Leia antes de escrever
qualquer caso novo** — cada uma custou um teste que passava sem verificar nada.

## Impedimentos de ambiente (não são bugs do código)

- **`protoc` é pré-requisito de build** (build script de `lance-encoding`, transitiva do
  `lancedb`) — sem ele nem o `clippy` compila. **Já resolvido** nesta máquina e no CI; ambiente
  novo precisa instalar antes de `cargo build`/`test` (`docs/testing.md`). Exige `sudo`, que não
  é interativo aqui — pedir ao usuário, nunca tentar sozinho.
- **Lacuna de ~140 GB entre `df` e arquivos visíveis (não é bug do projeto).** Assinatura de
  arquivo deletado preso em processo de **root**: nenhuma varredura acha o consumidor e
  `lsof +L1` sem privilégio não o vê. Pendente de `sudo lsof -nP +L1` pelo usuário, ou reboot.
  Consumidores legítimos mapeados, que não são a causa: Steam 162 GB, containers 49 GB,
  gnome-boxes 23 GB. `make disk` mostra o estado de `target/`; `make clean-debug` é **higiene
  recorrente**, não correção pontual.

## Impedimentos abertos

- **Roadmap de longo prazo original esgotado — só resta multimodal, bloqueada.** Fases 11 a 20
  concluídas. A Fase 21+ está bloqueada pela resposta do mantenedor (2026-07-16,
  `docs/decisoes-autonomas.md`): adiada até existir um *guardrail* de imagem, já que os
  guardrails hoje só inspecionam texto e multimodal sem isso abriria um canal não auditado.
  Provavelmente exige **dependência nova** (OCR). **Decisão pendente:** qual caminho seguir, ou
  apontar uma frente nova.
- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada
  via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido
  pela empresa; adapter adiado.
- **CI multi-SO ainda não observado verde:** a matriz do ADR-0005 (`2feed85`) precisa de um
  push ao GitHub para confirmar Windows/macOS verdes.
- **Verificação de "processo não órfão" do MT-23 é Unix-only de fato:** `processo_existe`
  (`crates/core/tests/lsp_client.rs`) usa `kill -0` e devolve `false` em `#[cfg(not(unix))]`,
  então em Windows os dois testes de ciclo de vida do `LspClient` passam **vacuamente**. Falta
  uma verificação real (ex.: `tasklist`) quando a matriz de CI rodar.
