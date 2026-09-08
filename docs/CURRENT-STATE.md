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
- **Commits:** `2f359f2`, `95e0966`, `d80d78c`, `e0b85bb`, `ad6606a`, `89b7e94`
- **Estado da árvore:** `AGENTS.md`, `skills/README.md` e os diretórios de skills não
  rastreados seguem modificados de **outra frente**, anteriores a esta rodada e intocados.
- **DoD:** build `release` OK e pacote Linux verificado a partir do arquivo extraído
  (`agentry --version` → `0.1.0`). **Suíte não re-executada** nesta rodada — as mudanças não
  tocam código Rust (`Makefile`, `[profile.release]` sem efeito prático, e documentação).

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

## Rodada 9 (2026-09-07) — build Linux, higiene de disco e ADR-0044

Relato completo arquivado em [`handoff-arquivo.md`](./handoff-arquivo.md). O que continua
valendo no dia a dia: `target/` cresce ~100 GB em poucas semanas porque o Cargo não remove
binários de teste de hash antigo (~1,5 GB cada) — `make clean-debug` é **higiene recorrente**,
não correção pontual; `make disk` mostra o estado. `strip = "symbols"` (−27% no binário) fica
para releases estáveis, por decisão do mantenedor: apaga a tabela de símbolos e o backtrace de
panic perde os nomes de função enquanto a `v0.1.0-usertest` colhe relato de bug.

### Fase K iniciada — MT-140 concluído

**ADR-0045** fixa: provider falso local (não gravação/replay, não provider real em CI), o
**binário real** subido como subprocesso (a fronteira que os bugs atravessaram não é
alcançável por função interna), escopo na fiação, e a saída declarada de restringir a Linux
caso a matriz de 3 SOs se mostre instável — teste intermitente destrói a confiança em todos os
outros. Estabilidade exige **duas** execuções verdes seguidas.

**A verificação derrubou trabalho previsto.** O MT-141 era "tornar a raiz de configuração
redirecionável", com risco declarado de emenda à ADR-0038. `global_dir.rs:21` já resolve o
home por `env::var_os("HOME")`/`USERPROFILE` e é o **único** leitor dessas variáveis no
workspace: definir `HOME` no processo filho basta. Nenhuma variável nova, nenhuma flag,
nenhuma mudança de produção. O MT-141 virou **teste-guarda** contra alguém introduzir outro
resolvedor de home — que quebraria o isolamento em silêncio, com os testes ainda verdes
escrevendo no `~/.agentry/` real.

Confirmado também que não há `--config`: `state_dir::agentry_settings_path(start)` sobe a
árvore, então o `cwd` do filho seleciona a configuração. Ressalva registrada no ADR: o
diretório temporário não pode estar aninhado sob um que contenha `.agentry/`.

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
   **MT-141** (teste-guarda de hermetismo) e **MT-142** (`fake_provider`), que não dependem um
   do outro e podem ir em qualquer ordem — o MT-140 (ADR-0045) está concluído e os três pontos
   em aberto do plano foram verificados no código.
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

- **`protoc` não vem pré-instalado por padrão** (nem, presumivelmente, nos runners padrão do GitHub Actions) — exigido pelo build script de `lance-encoding` (transitiva do `lancedb`, MT-27). CI já corrigido; ambientes de desenvolvimento locais precisam instalar `protobuf-compiler` (Debian/Ubuntu), `protobuf` (Homebrew) ou equivalente antes de rodar `cargo build`/`cargo test` neste crate — ver `docs/testing.md`. **Nesta máquina de desenvolvimento, já resolvido**: `protobuf-compiler` instalado via `apt` pelo usuário (precisa de `sudo` — funciona só com terminal interativo; o agente não deve tentar rodar `sudo` sozinho, sempre pedir para o usuário rodar). Um binário `protoc` *standalone* baixado manualmente mais cedo na sessão (`~/.local/bin/protoc`, contornando a falta de `sudo` interativo) foi removido para não sombrear o `/usr/bin/protoc` do pacote no `PATH` — `cargo build`/`test`/`clippy` voltaram a funcionar sem nenhuma variável de ambiente extra (`PROTOC`/`PROTOC_INCLUDE`).

- **Lacuna de ~140 GB no `df` sem arquivo correspondente (não é bug do projeto).** Em
  2026-09-07 o disco caiu 121 GB durante uma janela de ~10 min em que `target/` cresceu só
  3 GB; a soma do que é visível (`/home` 439 GB + `/var` 21 GB + swap 17 GB + `/usr` 13 GB +
  resto ≈ 495 GB) não fecha com os 638 GB que o `df` reporta. Varredura por arquivos >500 MB
  modificados em 4h não achou nada, e `lsof +L1` sem privilégio não vê descritor retido —
  assinatura de arquivo deletado preso em processo de **root**. Consumidores reais mapeados
  (não são a causa do pico): Steam 162 GB, containers 49 GB, gnome-boxes 23 GB, Trash 5,5 GB.
  **Pendente:** `sudo lsof -nP +L1` pelo usuário, ou um reboot. `make disk` mostra o estado.

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

