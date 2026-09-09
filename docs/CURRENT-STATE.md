<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> **Fonte central de sincronização — contém apenas o estado corrente.**
> Atualizado a cada commit. Não inclua segredos. Mantido conforme a skill `handoff-updater`.
>
> **Histórico completo:** [`docs/handoff-arquivo.md`](./handoff-arquivo.md) — rodadas
> anteriores, fases fechadas e a tabela de commits. Consulte sob demanda; não é necessário
> ler para retomar o trabalho.

## Último turno

- **Data:** 2026-09-08
- **Branch:** `chore/build-linux-e-higiene-de-disco` (**não mesclada em `main`**)
- **Commits desta rodada:** `882abeb`
- **Estado da árvore:** `AGENTS.md`, `skills/README.md` e os diretórios de skills não
  rastreados seguem modificados de **outra frente**, anteriores a esta rodada e intocados.
  Há um binário `agentry` de ~280 MB **solto na raiz** (não rastreado, não coberto pelo
  `.gitignore`, de 2026-09-06) — cópia de build, não versionar; remover é decisão do dono.
- **DoD:** `cargo fmt` aplicado, `cargo clippy --workspace --all-targets -- -D warnings` com
  saída **0**, suíte completa **835 testes verdes**.

## Metas cumpridas neste turno

- [x] **`882abeb`** — **MT-150**, **MT-151** e **MT-152**: os três achados do teste de uso da
      TUI, corrigidos e verificados na TUI real (via `tmux`), além dos testes.

Um parágrafo por ticket, porque a causa importa mais que o sintoma:

- **MT-150** — a legenda de atalhos era montada só de `def.code`, ignorando `def.modifiers`:
  `Ctrl+C` aparecia como `c`. Não era cosmético — desde o MT-72 letra solta **não resolve**
  para ação nenhuma (é caractere digitado), então a ajuda anunciava teclas que não funcionam.
  A guarda fecha a volta: o rótulo exibido é reconstruído em `KeyEvent` e tem de resolver para
  a ação que ele descreve (verificada por mutação — 3 testes falham se `modifiers` voltar a
  ser ignorado).
- **MT-151** — o exemplo do `--init` declarava `mcpServers.exemplo` com `command: "echo"`. O
  MT-77 apostou que a falha de conexão seria "tratada, não silenciosa", e era — porque naquele
  momento **nada conectava**; o MT-78 passou a conectar e o exemplo inerte virou erro fixo em
  toda execução. Regra que fica: bloco cujo valor de exemplo é **executado** não pode ter
  exemplo inerte — o formato vai no comentário, o mapa sai vazio.
- **MT-152** — avisos não-fatais de partida (MCP que não conectou, binário `claude` fora do
  `PATH`, ponte MCP que não montou) iam direto a `stderr` na origem; sob `--tui` o `crossterm`
  entra na tela alternativa logo depois e o aviso só reaparece **ao sair**. A correção não é
  escrever mais cedo: é **não escolher o canal na origem**. Quem produz registra em
  `DiagnosticosDeInicializacao`; `main` decide a superfície.

**Correção ao relatório de 2026-09-08:** a recusa de rota sob `local-only` **não** estava
invisível — ela ocorre antes de a TUI subir e sai em `stderr` com código 1 (verificado). O que
estava invisível era só a classe de diagnóstico de partida acima; erro de turno já aparecia no
chat via `marcar_erro`.

## Em andamento

Nada em execução.

## Próximo passo sugerido

1. **Bloco E do plano de uso** (`usage-test/PLANO-DE-TESTE.md`) — cenários de permissão, ainda
   não executados. Em especial o **E5**: confirmar que `Ctrl+A` **não** afrouxa uma tool sob
   `deny` é invariante de segurança, e é o buraco mais relevante da cobertura de uso hoje.
2. **MT-146** — fiar os testes ponta a ponta no CI; é o **único** ticket restante da Fase K.
   Inclui a regressão da configuração MCP temporária órfã.
3. **MT-153** — promover a TUI a modo padrão, com `--repl` como escape hatch e *fallback* para
   texto quando não houver TTY (`std::io::IsTerminal`). **Desbloqueado** por `882abeb`; exige
   ADR nova, citando o relatório de uso como a evidência que a ADR-0027 pedia.
4. **Implementar a ADR-0044** — providers por assinatura via CLI oficial (Codex/Gemini) e por
   API. **Pré-requisito duro, ainda não verificado:** confirmar que cada CLI autoriza consumo
   programático sob a assinatura e qual o modo *headless* suportado. Quebrar com
   `micro-ticket-planner`: o molde da ADR-0040 exige `AuditEntry` por invocação e
   `EgressClass` verificada antes do *spawn* em cada provider.
5. **Detector de nome de pessoa** — lacuna que a ADR-0043 declara e não resolve; custo de falso
   positivo alto, decisão do mantenedor.

## Decisões pendentes do mantenedor

Nenhuma virou asserção, para não congelar comportamento antes da decisão:

- **MT-147** — corpo **não-SSE** respondido a uma requisição de *stream* produz `stdout` vazio,
  `0 tokens` e **código de saída 0**. Indistinguível de "o modelo não teve o que responder" — e
  é o que acontece com endpoint mal configurado ou proxy que intercepta.
- **MT-148** — a regressão do `providers.ollama.structuredOutput` (o defeito mais grave já
  encontrado: nenhuma tool executando no *default*) **segue sem rede de proteção**. O *flag* é
  do `OllamaProvider` e o `fake_provider` só fala OpenAI-compatible; o MT-144 cobriu a *classe*
  da falha, não aquele defeito.
- **MT-149** — a recusa **na camada de rota** não deixa trilha persistente, enquanto o bloqueio
  no `Transport` deixa. A ADR-0002 manda auditar *cada egresso*, e aqui não houve egresso — pode
  ser lacuna de conformidade ou comportamento correto.

## O que saber antes de mexer nos testes ponta a ponta

Detalhe por ticket em [`docs/roadmap-v0.18.md`](./roadmap-v0.18.md); relato completo da Fase K
no [`handoff-arquivo.md`](./handoff-arquivo.md).

1. **Hermetismo é `HOME` + `cwd`**, sem mecanismo novo — `global_dir.rs` é o único resolvedor
   de *home* do workspace, propriedade protegida pela guarda estática do MT-141.
2. **`CARGO_BIN_EXE_fake_provider` não existe nos testes de `agentry`** (o bin é de
   `agentry-core`): o caminho é derivado do diretório do `CARGO_BIN_EXE_agentry`. Mover a
   fixture para `crates/cli` faria `cargo install` levá-la ao usuário.
3. **As fixtures desligam todas as funcionalidades de contexto**, não só as caras: senão
   `session_search` entra no conjunto anunciado (default `true`) e a asserção quebra por
   mudança de *default*, não por regressão.
4. **O caminho one-shot é `chat_stream`** — o roteiro do `fake_provider` precisa ser SSE.
5. **O *fail-closed* tem duas camadas** com comportamento diferente (`Router` recusa sem
   auditar; `Transport` barra e audita `blocked`). Caso de bloqueio afirma o **mecanismo** —
   afirmar só "nada chegou ao provider" passa por vacuidade se o binário falhar por outro
   motivo.
6. **O plano de uso dirige a TUI por `tmux`** (`send-keys` + `capture-pane`), porque a TUI
   exige TTY. Duas armadilhas já corrigidas no plano: capturar sem `-e` achata o logo e
   *parece* defeito; o `HOME` isolado precisa de um `.zshrc` vazio, ou o assistente do zsh
   engole o comando.

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
  gnome-boxes 23 GB. `make disk` mostra o estado de `target/`; `make clean-debug` é **higiene
  recorrente**, não correção pontual.

## Impedimentos abertos

- **Roadmap de longo prazo original esgotado — só resta multimodal, bloqueada.** As Fases 11 a
  20 estão concluídas. A Fase 21+ (multimodal) está bloqueada pela resposta do mantenedor
  (2026-07-16, `docs/decisoes-autonomas.md`): adiada até existir um *guardrail* de imagem — os
  guardrails de conteúdo hoje só inspecionam texto, e multimodal sem esse pré-requisito abriria
  um canal não auditado. Provavelmente exige **dependência nova** (OCR), o que é parada dura
  por si só. **Decisão pendente:** qual caminho para o guardrail de imagem, ou apontar uma
  frente nova fora das cinco do roadmap de longo prazo.
- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada
  via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido
  pela empresa; adapter adiado.
- **CI multi-SO ainda não observado verde:** a matriz do ADR-0005 (`2feed85`) precisa de um
  push ao GitHub para confirmar Windows/macOS verdes.
- **Verificação de "processo não órfão" do MT-23 é Unix-only de fato:** `processo_existe`
  (`crates/core/tests/lsp_client.rs`) usa `kill -0`; no branch `#[cfg(not(unix))]` sempre
  devolve `false`, então em Windows os dois testes de ciclo de vida do `LspClient` passam
  **vacuamente**. O `Child::wait()`/`kill()` internos continuam corretos; falta uma verificação
  real de ausência de processo em Windows (ex.: via `tasklist`) quando a matriz de CI rodar.
