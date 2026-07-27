<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> **Fonte central de sincronização — contém apenas o estado corrente.**
> Atualizado a cada commit. Não inclua segredos. Mantido conforme a skill `handoff-updater`.
>
> **Histórico completo:** [`docs/handoff-arquivo.md`](./handoff-arquivo.md) — rodadas
> anteriores, fases fechadas e a tabela de commits. Consulte sob demanda; não é necessário
> ler para retomar o trabalho.

## Último turno

- **Data:** 2026-07-25
- **Branch:** `main`
- **Commit:** `34376f0`
- **Estado da árvore:** limpa · **DoD:** `fmt`/`clippy` limpos, **765 testes** verdes, build
  `release` OK.

## Metas cumpridas neste turno

- [x] **`2955651`** — `fix(ollama)`: `structuredOutput` *default* `false`. Bug crítico:
      nenhuma tool executava na configuração *default*. Emenda à ADR-0012.
- [x] **`815bb35`** — `feat(anthropic)`: provider `anthropic` (Messages API por chave)
      registrado na CLI. ADR-0040 parte A.
- [x] **`08816a8`** — `feat(claude-cli)`: provider `claude-cli` (assinatura Pro/Max via
      subprocesso, com auditoria equivalente). ADR-0040 parte B.
- [x] **`34376f0`** — `docs(handoff)`: relato da rodada 7.

## Rodada 7 (2026-07-25) — bug crítico do Ollama + provider Anthropic (ADR-0040)

Pedido do mantenedor: avaliar código/planejamento, adicionar a Anthropic como provider (API
**e** assinatura Pro/Max) e testar o `agentry` de verdade contra Ollama local e contra a
própria conta do Claude Code.

### 1. Bug crítico achado pelo teste com Ollama — `2955651`

Rodar o binário `release` contra Ollama real (`qwen2.5:7b`) mostrou que **nenhuma tool
executava na configuração default**: o agente imprimia `{"arguments":{…},"name":"fs_read"}`
como texto e encerrava no primeiro turno.

Causa raiz: o campo `format` do Ollama restringe a geração do **texto**, não das
`tool_calls`. Com `providers.ollama.structuredOutput` no default `true` (ADR-0012/MT-22), um
modelo com tool-calling nativo devolve a chamada como string JSON em `content` e
`tool_calls` volta vazio — `ollama_message_to_domain` só lê `tool_calls`, então o JSON vira
mensagem comum e o laço para com `StopReason::Done`. Confirmado por `curl` direto nos dois
sentidos. Default invertido para `false` (emenda à ADR-0012); a flag continua como escape
para modelos antigos sem suporte nativo.

**Este é o terceiro bug da mesma família** (junto com `Session::with_tools` nunca chamado e o
`stream_options` ausente): todos vivem na fronteira `main()` → provider real, que os testes
unitários não cobrem porque cada um monta sua própria `Session` com `MockProvider`.
**Recomendação registrada:** um teste de integração em CI que suba o binário real contra um
mock HTTP fiel (ou o Ollama do container) e verifique efeito colateral em disco vale mais que
a próxima feature.

### 2. ADR-0040 — provider Anthropic, dois caminhos (`815bb35`, `08816a8`)

Achado: o `AnthropicProvider` existia **completo e testado** desde o MT-16, mas **nunca era
registrado no `Router` da CLI** — código morto, com um teste usando `anthropic` como exemplo
de "provider inexistente". A metade "API" era fiação, não código novo.

- **Parte A — `anthropic`** (`815bb35`): `providers.anthropic` no schema (`model` ativa;
  `baseUrl`/`egressClass` têm default), `build_anthropic_provider` no molde do LiteLLM,
  headers `x-api-key` + `anthropic-version` (não `Bearer`), credencial via
  `~/.agentry/credentials.json` (MT-128) — obrigatória, já que a Messages API não tem modo
  anônimo. **Correção acoplada:** o campo `thinking` emitia
  `{"type":"enabled","budget_tokens":N}`, formato **rejeitado com HTTP 400** nos modelos
  atuais; passa a emitir `{"type":"adaptive"}`.
- **Parte B — `claude-cli`** (`08816a8`): `ClaudeCliProvider` faz *spawn* de
  `claude -p --output-format stream-json`, usando a assinatura Pro/Max sem nunca tocar no
  token OAuth. Como não passa pelo `Transport`, emite ele próprio uma `AuditEntry` por
  invocação no mesmo `.agentry/audit.log`, e recusa o *spawn* (auditando o bloqueio) quando a
  classe de egresso não permite nuvem.

**Achado que mudou o desenho da parte B:** `claude -p` **não é um endpoint de modelo — é um
agente completo**, com laço, ferramentas e permissões próprios. Delegar por inteiro faria as
edições escaparem do `PermissionGate`/*checkpoints*/auditoria do `agentry`. Decisão: desligar
as ferramentas embutidas (`--tools ""`) e usar o subprocesso como gerador de **texto puro** —
`claude-cli` serve para conversa/`/compact`/revisão, **não** para o laço agêntico. Paridade
completa exigiria expor as tools do `agentry` como **servidor** MCP (só existe cliente hoje,
ADR-0028): **trabalho futuro registrado na ADR-0040, não assumido.**

### Verificação real (não só testes unitários)

- Ollama, sem nenhum arquivo de configuração: laço completo (`fs_read` + `fs_edit`), arquivo
  corrigido de fato no disco.
- Anthropic contra mock da Messages API: `POST /v1/messages`, `x-api-key` presente e
  `Authorization` ausente, 16 tools no corpo, `thinking:{"type":"adaptive"}` sem
  `budget_tokens`; `--set-credential` grava com `0600` e a chave do arquivo chega ao servidor
  sem variável de ambiente.
- `claude-cli` contra a assinatura **real**: resposta correta, uso de tokens, entrada gravada
  em `.agentry/audit.log`. Sob `egressClass: local-only`, a rota resolve mas o provider recusa
  o *spawn* e registra o bloqueio com motivo.

### 3. Handoff reestruturado (dívida apontada e resolvida na mesma rodada)

Este arquivo tinha ~2550 linhas (~114k tokens): virou log append-only, não handoff — um
agente novo não conseguia lê-lo dentro do contexto, o oposto do propósito da skill
`handoff-updater`. A pedido do mantenedor, o histórico foi movido para
[`docs/handoff-arquivo.md`](./handoff-arquivo.md) e este arquivo passou a conter **só o
estado corrente**. A skill (neste repositório e no `ai-coding-agent-profiles`) foi atualizada
para tornar essa separação a forma de trabalho padrão, com teto explícito de tamanho.

## Em andamento

Nada em execução. Árvore limpa.

## Próximo passo sugerido

Decisões do mantenedor, em ordem de impacto:

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

