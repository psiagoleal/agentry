<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> **Fonte central de sincronização — contém apenas o estado corrente.**
> Atualizado a cada commit. Não inclua segredos. Mantido conforme a skill `handoff-updater`.
>
> **Histórico completo:** [`docs/handoff-arquivo.md`](./handoff-arquivo.md) — rodadas
> anteriores, fases fechadas e a tabela de commits. Consulte sob demanda; não é necessário
> ler para retomar o trabalho.

## Último turno

- **Data:** 2026-09-07
- **Branch:** `chore/build-linux-e-higiene-de-disco` (não mesclada em `main`)
- **Commits:** `2f359f2`, `95e0966`, `d80d78c`, `e0b85bb`, `ad6606a`, `89b7e94`, `6f41eb2`, `d5832d7`, `579184f`
- **Estado da árvore:** `AGENTS.md`, `skills/README.md` e os diretórios de skills não
  rastreados seguem modificados de **outra frente**, anteriores a esta rodada e intocados.
- **DoD:** `cargo fmt --check` e `cargo clippy --all-targets -- -D warnings` com saída **0**;
  suíte completa **823 testes verdes** (814 + 9 novos). Pacote Linux verificado a partir do
  arquivo extraído (`agentry --version` → `0.1.0`).

## Metas cumpridas neste turno

- [x] **`2f359f2`** — `build`: alvos `build`/`linux-build`/`linux` (pacote `tar.gz`) e
      `disk`/`clean-incremental`/`clean-debug`/`clean-all` no `Makefile`; `[profile.release]`
      explícito no `Cargo.toml`.
- [x] **`95e0966`** — `docs(adr)`: **ADR-0044**, assinatura por CLI oficial; navegador
      embutido rejeitado.
- [x] **`d80d78c`** — `docs(handoff)`: rodada 9; rodada 8f arquivada.
- [x] **`e0b85bb`** — `docs(roadmap)`: **v0.18**, Fase K quebrada em MT-140 a MT-146
      (teste de integração ponta a ponta).
- [x] **`89b7e94`** — **MT-140**: ADR-0045 (estratégia de teste ponta a ponta) + os três
      pontos do roadmap verificados no código.
- [x] **`6f41eb2`** — **MT-141** (guarda de hermetismo) e **MT-142** (`fake_provider`).
- [x] **`d5832d7`** — **MT-143**: harness ponta a ponta; fronteira `main()` → provider
      fechada pela primeira vez.
- [x] **`579184f`** — **MT-144** (parcial): laço de tool-calling ponta a ponta e conjunto de
      tools anunciado.

## Rodada 9 (2026-09-07) — build Linux, higiene de disco e ADR-0044

Relato completo arquivado em [`handoff-arquivo.md`](./handoff-arquivo.md). O que continua
valendo no dia a dia: `target/` cresce ~100 GB em poucas semanas porque o Cargo não remove
binários de teste de hash antigo (~1,5 GB cada) — `make clean-debug` é **higiene recorrente**,
não correção pontual; `make disk` mostra o estado. `strip = "symbols"` (−27% no binário) fica
para releases estáveis, por decisão do mantenedor: apaga a tabela de símbolos e o backtrace de
panic perde os nomes de função enquanto a `v0.1.0-usertest` colhe relato de bug.

### Fase K — MT-140 a MT-144 concluídos

Detalhe por ticket em [`docs/roadmap-v0.18.md`](./roadmap-v0.18.md). A fronteira `main()` →
provider está **fechada**: os casos sobem o binário de verdade contra o `fake_provider` e
afirmam nos dois sentidos — resposta chega ao `stdout`, e o que o binário **enviou** é lido do
registro de requisições. Cobertos: conversa simples, laço completo de tool-calling (executa sob
`PermissionGate`, resultado volta ao modelo) e o **conjunto inteiro** de tools anunciado.

Quatro coisas não óbvias para quem retoma:

1. **Hermetismo é `HOME` + `cwd`**, sem mecanismo novo — `global_dir.rs` é o único resolvedor de
   *home* do workspace, propriedade protegida pela guarda estática do MT-141 (verificada por
   mutação).
2. **`CARGO_BIN_EXE_fake_provider` não existe nos testes de `agentry`** (o bin é de
   `agentry-core`): o caminho é derivado do diretório do `CARGO_BIN_EXE_agentry`. Mover a
   fixture para `crates/cli` faria `cargo install` levá-la ao usuário.
3. **As fixtures desligam todas as funcionalidades de contexto**, não só as caras: senão
   `session_search` entra no conjunto anunciado (default `true`) e a asserção quebra por
   mudança de *default*, não por regressão.
4. **O caminho one-shot é `chat_stream`** — roteiro precisa ser SSE.

**Dois achados aguardando decisão do mantenedor** (nenhum virou asserção, para não congelar
comportamento antes da decisão):

- **MT-147** — corpo **não-SSE** respondido a uma requisição de *stream* produz `stdout` vazio,
  `0 tokens` e **código de saída 0**. Indistinguível de "o modelo não teve o que responder", e é
  o que acontece com endpoint mal configurado ou proxy que intercepta.
- **MT-148** — a regressão do `providers.ollama.structuredOutput` (o defeito mais grave já
  encontrado, nenhuma tool executando no *default*) **segue sem rede de proteção**: o *flag* é do
  `OllamaProvider` e o `fake_provider` só fala OpenAI-compatible. O MT-144 cobriu a *classe* da
  falha, não aquele defeito.

## Em andamento

Nada em execução. A branch `chore/build-linux-e-higiene-de-disco` tem dois commits **não
mesclados em `main`**.

**Cota:** janela de 7 dias em 90% em 2026-09-07, com reset previsto para ~1h28m depois dessa
medição. Frente longa nova deve começar **depois** do reset; o que couber antes precisa ser
incremento commitável.

## Próximo passo sugerido

Decisões do mantenedor, em ordem de impacto:

1. **Teste de integração ponta a ponta em CI** — frente escolhida pelo mantenedor e **já
   planejada**: ver `docs/roadmap-v0.18.md`, Fase K, MT-140 a MT-146. Começar pelo
   **MT-145** (egresso, auditoria e guardrail de PII), que o harness já suporta. Depois, o
   **MT-146** (fiar no CI) fecha a fase. Dois itens rastreados fora da fila principal, ambos
   dependendo de decisão do mantenedor: **MT-147** (o achado de *stream* vazio com código 0) e
   **MT-148** (protocolo nativo do Ollama, para fechar a regressão do `structuredOutput`).
   Causa da frente: todos os bugs de produção do projeto vivem na fronteira `main()` →
   provider real, que os 814 testes unitários não cobrem.
2. **Implementar a ADR-0044** — providers por assinatura via CLI oficial (Codex para conta
   ChatGPT, Gemini CLI para conta Google) e por API (OpenAI, Google). **Pré-requisito duro,
   ainda não verificado:** confirmar que cada CLI autoriza consumo programático sob a
   assinatura e qual o modo *headless* suportado — a ADR-0044 torna essa verificação
   bloqueante para a adoção de cada provider. Quebrar com `micro-ticket-planner`: são pelo
   menos quatro providers, e o molde da ADR-0040 exige `AuditEntry` por invocação e
   `EgressClass` verificada antes do *spawn* em cada um.
3. **Detector de nome de pessoa** — a lacuna que a ADR-0043 declara e não resolve: nome não
   tem formato reconhecível. Exigiria abordagem diferente (dicionário, modelo local de NER) e
   tem custo de falso positivo alto; decisão do mantenedor se vale a pena.
4. **Guardrail de imagem** (bloqueia a frente multimodal) — ver *Impedimentos abertos*.

## Impedimentos de ambiente (não são bugs do código)

- **`protoc` é pré-requisito de build** (build script de `lance-encoding`, transitiva do
  `lancedb`) — sem ele nem o `clippy` compila. **Já resolvido** nesta máquina
  (`protobuf-compiler` via `apt`) e no CI (passo por SO em `ci.yml`); ambiente novo precisa
  instalar antes de `cargo build`/`test`. Detalhes em `docs/testing.md`. Instalar exige
  `sudo`, que não é interativo aqui — pedir ao usuário, nunca tentar sozinho.
- **Lacuna de ~140 GB entre `df` e arquivos visíveis (não é bug do projeto).** Assinatura de
  arquivo deletado preso em processo de **root**: nenhuma varredura acha o consumidor e
  `lsof +L1` sem privilégio não o vê. Pendente de `sudo lsof -nP +L1` pelo usuário, ou reboot.
  Consumidores legítimos mapeados, que não são a causa: Steam 162 GB, containers 49 GB,
  gnome-boxes 23 GB. `make disk` mostra o estado de `target/`.

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

