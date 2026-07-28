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

## Achados em aberto da rodada 8 (confidencialidade)

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


> Relato completo das rodadas 8, 8b e 8c (testes de vazamento, `llama3.1:8b`, arquitetura
> invertida e ADR-0041) em [`docs/handoff-arquivo.md`](./handoff-arquivo.md).

## Rodada 8d — ADR-0042: servidor MCP, assinatura Pro/Max como agente principal

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

### Rodada 8e — default de delegação, correção de doc e ponte com o `profiles`

**(1) Correção de documentação, não de código.** A primeira versão da ADR-0042 afirmou, sem
verificar, que guardrails e compactação "não se aplicam" com `mcpTools`. **Aplicam-se** —
testado. O `claude -p` é invocado de dentro de `Session::run_streaming`, então continuam
valendo histórico/`/save`/`--resume`, guardrails de entrada **e** saída, guardrails herdados
pelo **subagente**, permissões/`readAllow`/*checkpoints*/audit log e `/compact`. A fronteira
real é entre **o que a `Session` observa** e **o que roda dentro do subprocesso**: os
tool-calls que o Claude Code executa via MCP não aparecem no histórico salvo (com o provider
`anthropic`, aparecem) nem são inspecionados. ADR, doc do módulo e doc de usuário corrigidas.

**(2) Novo default do `--init`:** `chat` com dois candidatos — `claude-cli` (cloud-ok) e
`ollama` (local-only) — mais a task-class `local` para delegação. Duas salvaguardas, porque um
default que quebra é pior que nenhum: sob perfil sem nuvem o candidato remoto é filtrado pela
classe de egresso e o Ollama assume; e `build_claude_cli_provider` **não registra** o provider
quando o binário `claude` está ausente, com aviso. Verificado nos três casos.

**(3)/(4) — encaminhados no `ai-coding-agent-profiles`** (commit `828c978` lá):
- **ADR-0007 daquele repo:** os três `profiles/<perfil>/.agentry/agentry.settings.json`
  distribuem a arquitetura completa. `profile` passa a ser declarado (lacuna real: sem ele
  `--init --profile pessoal` não liberava nuvem); `chat` declara os **mesmos** dois candidatos
  nos três perfis, e a diferenciação vem da classe de egresso — sem lógica condicional para
  manter; `structuredOutput` corrigido para `false` (distribuíam `true`, que quebra
  tool-calling).
- **Skill `delegacao-a-subagentes`:** critérios de escolha de modelo em vez de nomes (lista de
  modelo envelhece em silêncio), com observações de campo num bloco datado e perecível.
  Declara explicitamente o que a ferramenta **não** garante.
- **Nenhuma mudança de schema** — a divisão da ADR-0006 (perfis distribuem valores, `agentry`
  é dono do schema) segue intacta.
- **Publicado e fiado:** `profiles` no commit `828c978`; `PROFILES_REPO_REF`
  (`crates/cli/src/init.rs`) atualizado para ele. `--init --profile` verificado buscando da
  rede de verdade — o arquivo entregue é byte-a-byte o publicado.
- **Achado de UX no caminho:** `--init --profile` imprimia a dica "rode o `setup-profile.sh`
  para valores diferenciados" **depois** de já ter entregado exatamente esses valores. A dica
  passou a sair só no caminho do exemplo genérico.

Verificado com o binário `release` consumindo os três arquivos: `empresa` e
`externo-confidencial` → `⌂ ollama:llama3.1:8b`; `pessoal` → `↗ claude-cli:opus`, e nele o
agente de nuvem lê `README.md` e é bloqueado em `clientes.csv`.

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

