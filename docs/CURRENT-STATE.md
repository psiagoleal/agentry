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
- **Estado da árvore:** limpa · **DoD:** `fmt`/`clippy` limpos, **814 testes** verdes, build
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

## Rodada 8f — achados 1-3 resolvidos (ADR-0043)

Os três achados de confidencialidade da rodada 8 tinham uma raiz comum: o `agentry` sabia
**que** dado cruzava a fronteira, e não **o quê**. Fechados com um mecanismo só.

- **Achado 2** (guardrail literal é contornado) — `GuardrailRule` ganha `detect`, alternativo
  a `match`: `cpf`, `cnpj`, `email`, `telefone-br`, `cartao-credito`. Reconhecem por
  **formato**, então reformatar os dados (a evasão observada) deixa de funcionar. Detectores
  **nomeados**, não regex do usuário — a ADR-0043 registra os três motivos (regex de PII é
  difícil de escrever e erra em silêncio; padrão vindo de configuração é superfície de ReDoS;
  nome é auditável, padrão não). A ADR-0007 §"sem regex" continua valendo.
- **Achado 3** (log não diz o que saiu) — `GuardrailAuditEntry` passa a registrar o **nome** do
  detector e a **contagem**, nunca o valor. Vazamento vira detectável depois do fato sem o log
  virar um novo repositório de dado sensível.
- **Achado 1** (nada impõe sanitização) — com os dois acima, a fronteira passa a ter inspeção
  por formato nos dois sentidos, inclusive no subagente (que já herdava os guardrails).

Dígito verificador é obrigatório onde existe (módulo 11, Luhn): sem isso qualquer
identificador numérico do mesmo tamanho viraria achado e um `redact` corromperia dado legítimo
em silêncio.

**Limitação declarada:** nome de pessoa **não tem formato** — "Ana Ribeiro" continua passando.
`readAllow` (ADR-0041) segue sendo a proteção primária; detectores são a segunda linha.

Verificado com o binário `release`: bloqueio por CPF num texto **sem** a palavra `cpf`;
`redact` entregando ao modelo o texto já mascarado; `audit.log` com `"deteccao":["cpf",1]` e
nenhum dos valores presente no arquivo.

## Em andamento

Nada em execução. Árvore limpa.

## Próximo passo sugerido

Decisões do mantenedor, em ordem de impacto:

1. **Teste de integração ponta a ponta em CI** — causa estrutural dos bugs de produção já
   encontrados (todos na fronteira `main()` → provider real, que os 814 testes unitários não
   cobrem). **Maior retorno disponível hoje**, acima de qualquer feature nova: nesta rodada
   sozinha, três defeitos apareceram só ao rodar o binário de verdade (`shell_background`
   exposto, config MCP temporária órfã, pontuação consecutiva no detector).
2. **Detector de nome de pessoa** — a lacuna que a ADR-0043 declara e não resolve: nome não
   tem formato reconhecível. Exigiria abordagem diferente (dicionário, modelo local de NER) e
   tem custo de falso positivo alto; decisão do mantenedor se vale a pena.
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

