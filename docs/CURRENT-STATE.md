<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> **Fonte central de sincronização — contém apenas o estado corrente.**
> Atualizado a cada commit. Não inclua segredos. Mantido conforme a skill `handoff-updater`.
>
> **Histórico completo:** [`docs/handoff-arquivo.md`](./handoff-arquivo.md) — rodadas
> anteriores, fases fechadas e a tabela de commits. Consulte sob demanda; não é necessário
> ler para retomar o trabalho.

## Último turno

- **Data:** 2026-09-09
- **Branch:** `chore/build-linux-e-higiene-de-disco` (**não mesclada em `main`**)
- **Commits desta rodada:** `882abeb`, `2bf620d`, `67b1e77`, `bcc7788`, `f521dbe`, `5c311ca`,
  `f1ffdde`, `f00b730`, `c4bfda0`, `29420cf`, `0ce65a0`
- **Estado da árvore:** `AGENTS.md`, `skills/README.md` e os diretórios de skills não
  rastreados seguem modificados de **outra frente**, anteriores a esta rodada e intocados.
  Há um binário `agentry` de ~280 MB **solto na raiz** (não rastreado, fora do `.gitignore`) —
  cópia de build; remover é decisão do dono.
- **DoD:** `cargo fmt` aplicado, `cargo clippy --workspace --all-targets -- -D warnings` com
  saída **0**, suíte completa **835 testes verdes**.

## Metas cumpridas neste turno

- [x] **`882abeb`** — **MT-150**, **MT-151** e **MT-152**: os três achados do teste de uso da
      TUI, corrigidos e verificados na TUI real (via `tmux`), além dos testes.
- [x] **bloco E do plano de uso** executado (`usage-test/RELATORIO-2026-09-08-bloco-e.md`):
      6 cenários, **6 passaram**, 3 achados novos (MT-154/155/156).
- [x] **`f521dbe`** — **MT-154**: marcador de desfecho (`⚙`/`✓`/`✗`) na linha recolhida de tool,
      mais o motivo do erro; a ajuda passou a explicar os marcadores e o clique de expansão.
- [x] **`f1ffdde`** — **MT-157**: saída do processo concentrada na `main`; os 25
      `std::process::exit` viraram `Encerramento(n)` propagado por `?`.
- [x] **exemplos de configuração** em `usage-test/exemplos/`, versionados e com duas guardas:
      todo exemplo tem de **carregar**, e nenhum pode apontar para host real.
- [~] **`c4bfda0`** — **MT-146** (parcial): e2e auditado e tornado portável para a matriz; falta
      **só** observar o CI verde duas vezes, o que depende de um `push` ainda não autorizado.

Causa raiz de cada um (detalhe em `docs/roadmap-v0.18.md`); as três correções compartilham a
mesma forma — **informação correta produzida no lugar certo, que não chegava a quem precisa**:

- **MT-150** — a legenda vinha só de `def.code`; desde o MT-72 letra solta **não resolve** para
  ação nenhuma, então a ajuda anunciava teclas que não funcionam.
- **MT-151** — regra que fica: bloco de configuração cujo valor de exemplo é **executado** não
  pode ter exemplo inerte. O `echo` do MT-77 era seguro porque nada conectava; o MT-78 passou
  a conectar e o exemplo virou erro fixo.
- **MT-152** — a correção não foi escrever em `stderr` mais cedo: foi **não escolher o canal na
  origem**. Quem produz registra em `DiagnosticosDeInicializacao`; `main` decide a superfície.
- **MT-154** — o marcador (`⚙`/`✓`/`✗`) responde "isso rodou ou não?" sem interação.
- **MT-157** — `std::process::exit` **não roda destrutores**, então todo `Drop` de limpeza era
  condicional ao caminho feliz. A saída passou a acontecer num ponto só, depois de `executar`
  ter devolvido. Vale como regra geral daqui em diante: **limpeza por `Drop` só é confiável
  porque o processo agora desempilha** — a guarda estática impede o próximo `exit` de aparecer.

**Correção ao relatório de 2026-09-08:** a recusa de rota sob `local-only` **não** estava
invisível — ocorre antes de a TUI subir e sai em `stderr` com código 1 (verificado).

## Em andamento

**MT-146, parado num ponto que exige decisão sua.** O código está pronto: o CI já rodava
`cargo test --all`, então os casos ponta a ponta **já entram** na matriz de três SOs — não
faltava fiação. O que faltava era portabilidade real, e havia um defeito concreto: o caso do
MT-157 redirecionava o diretório temporário só por `TMPDIR`, que o Windows ignora, então ele
teria passado **por vacuidade** lá (afirmando sobre um diretório que o binário nunca usou).
Corrigido; o resto do e2e foi auditado e não precisa de recorte por SO.

Falta o critério de aceite: **CI verde em duas execuções seguidas**. Isso exige `push` da
branch `chore/build-linux-e-higiene-de-disco`, que nunca foi enviada — **não fiz por conta
própria**. A ADR-0045 §5 **não** foi emendada de propósito: a saída acordada lá (restringir o
e2e a Linux) depende de instabilidade **observada**, e ainda não houve execução de CI.

**Pedido em aberto do mantenedor (2026-09-09):** usar `usage-test/` para exemplos de uso e de
configuração — feito para a parte de **configuração**; falta **executar** um teste de uso contra
o gateway e deixar o relatório. Parou por cota, não por dificuldade.

**Gateway LiteLLM alcançável desta máquina desde 2026-09-09** — endpoint e chave ficam **fora
do repositório**. Quatro modelos, dos quais `qwen3-coder:30b` faz *tool-calling*. **Nunca ecoar
a chave:** use `agentry --set-credential litellm` lendo de `stdin` (grava `0600`, mantém o valor
fora de `argv`). Primeira validação ponta a ponta contra um gateway **real**: one-shot e TUI,
com *tool-calling* completo e auditoria `cloud-ok, allowed`. Existe a skill `delegacao-litellm`
para mandar trabalho volumoso ao gateway em vez de gastar cota da assinatura.

**Decisão pendente de política:** que `egressClass` o gateway interno deve declarar. Os testes
usaram `profile: pessoal` + `cloud-ok`, que é o rótulo conservador. Note que `empresa` mapeia
para `local-only` (`config/privacy.rs`), então sob o perfil corporativo um gateway interno só é
alcançável se for declarado `local-only` — ou seja, `local-only` aqui significa "não sai da
fronteira de confiança", não "não sai da máquina".

## Próximo passo sugerido

1. **MT-146** — fiar os testes ponta a ponta no CI; é o **único** ticket restante da Fase K.
   Inclui a regressão da configuração MCP temporária órfã.
2. **MT-153** — promover a TUI a modo padrão, com `--repl` como escape hatch e *fallback* para
   texto quando não houver TTY (`std::io::IsTerminal`). **Desbloqueado** por `882abeb`; exige
   ADR nova, citando o relatório de uso como a evidência que a ADR-0027 pedia.
3. **Implementar a ADR-0044** — providers por assinatura via CLI oficial (Codex/Gemini) e por
   API. **Pré-requisito duro, ainda não verificado:** confirmar que cada CLI autoriza consumo
   programático sob a assinatura e qual o modo *headless* suportado. Quebrar com
   `micro-ticket-planner`: o molde da ADR-0040 exige `AuditEntry` por invocação e
   `EgressClass` verificada antes do *spawn* em cada provider.
4. **Detector de nome de pessoa** — lacuna que a ADR-0043 declara e não resolve; custo de falso
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
