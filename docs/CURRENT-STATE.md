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
- **Commits:** `2f359f2`, `95e0966`, `d80d78c`, `e0b85bb`
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

## Rodada 9 (2026-09-07) — build Linux, higiene de disco e ADR-0044

**`target/` com 125 GB.** Causa raiz: o perfil `dev` gera binários de teste de ~1,5 GB **cada**
e o Cargo **não remove** os de hash antigo quando o código muda — `target/debug/deps` sozinho
tinha 97 GB em 9.205 arquivos, acumulados em três semanas (nada com mais de 30 dias).
`cargo clean --profile dev` liberou 113 GB preservando `release` e o cross-compile Windows.
Os alvos de limpeza no `Makefile` existem para isso não voltar: **é higiene recorrente**, não
correção pontual.

**O `Makefile` só tinha Windows.** O pacote Linux vinha sendo montado à mão com `tar` a cada
release (ver `handoff-arquivo.md`, rodada 5). Fechado com `make linux`, verificado ponta a
ponta. O build Linux é **nativo, sem `--target`**: passar `x86_64-unknown-linux-gnu`
explicitamente cria uma segunda árvore de artefatos para o mesmo triplo (é o que explica os
654 MB soltos em `target/x86_64-unknown-linux-gnu/`), forçando recompilação sem ganho.

**Binário de 281,8 MB.** `strip = "debuginfo"` foi adotado e é **no-op** — binário
byte-idêntico, zero seções `.debug`, porque o perfil `release` já usa `debug = false`; fica
como guarda. O corte real seria `strip = "symbols"` (281,8 → 204,3 MB, **−27%**), **decidido
pelo mantenedor para releases estáveis apenas**: apaga a tabela de símbolos e o backtrace de
panic perde os nomes de função, justo enquanto a `v0.1.0-usertest` existe para colher relato
de bug. O piso de 204 MB é a árvore de dependências (`datafusion`, `lancedb`, `tantivy`,
`arrow`); a alavanca real seria feature-gate no stack de RAG.

**ADR-0044** — rejeita navegador embutido como autenticação/transporte de provider e adota
providers por assinatura via *spawn* do CLI oficial (Codex, Gemini), no molde do `claude-cli`
da ADR-0040, mais providers por API no molde do `anthropic`. O impedimento decisivo **não** é
contratual e sim estrutural: navegador embutido é superfície de egresso **não enumerável**,
então nenhuma `AuditEntry` descreveria com honestidade o que saiu, e o *fail-closed* da
ADR-0002 não teria como ser imposto. Navegador *headless* segue admissível como **tool** de
leitura de página (extensão da ADR-0025), atrás do `Transport` — sob ADR próprio.

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
   **MT-140** (ADR-0045), que fixa as três escolhas estruturais — provider falso local em vez
   de rede/credencial em CI, subir o **binário real** como subprocesso, e escopo na fiação e
   não nas unidades. O roadmap lista três pontos **não confirmados no código** que mudam o
   escopo dos tickets seguintes; verificá-los é a primeira tarefa do MT-140.
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

