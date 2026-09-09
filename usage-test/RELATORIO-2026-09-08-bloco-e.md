<!-- Caminho relativo: usage-test/RELATORIO-2026-09-08-bloco-e.md -->

# Relatório de teste de uso — bloco E (tools, permissão e confirmação)

> Segunda execução do [`PLANO-DE-TESTE.md`](./PLANO-DE-TESTE.md), cobrindo **só o bloco E**, que
> ficou de fora da [primeira rodada](./RELATORIO-2026-09-08.md). O bloco E era o buraco mais
> relevante da cobertura de uso: contém o **E5**, que é um invariante de segurança.

## Identificação

| Campo | Valor |
|---|---|
| Data | 2026-09-08 |
| Commit testado | `2bf620d` (já com as correções MT-150/151/152 de `882abeb`) |
| Versão | `agentry 0.1.0` (build `--release`) |
| SO / terminal / `tmux` | Linux 7.0.0-31-generic / `tmux` 3.6, painel 120×40 |
| Provider e modelo | Ollama local, `llama3.1:8b` (`local-only`) |
| Executado por | Claude Code (Opus 5), via `tmux send-keys`/`capture-pane` |
| `--tui` era o padrão? | não (invocado explicitamente) |

**Configuração do bloco:** `permissions.ask = ["fs_write"]`, `permissions.deny = ["fs_edit"]`,
task-class `chat` apontando só para Ollama. `HOME` e workspace isolados (`mktemp -d`), com
`.zshrc` vazio.

## Resumo

| | Quantidade |
|---|---|
| Cenários executados | 6 |
| `PASSOU` | 6 |
| `FALHOU` | 0 |
| `BLOQUEADO` | 0 |

**Veredito geral:** o mecanismo de permissão **funciona como especificado, inclusive no ponto
que mais importa** — `Ctrl+A` não afrouxa `deny` —, mas o *resultado* de uma chamada de tool é
invisível na visão padrão da TUI, e a única forma de revelá-lo não é descobrível.

**A TUI está pronta para ser o modo padrão?** **Sim, do ponto de vista de segurança.** Nenhum
dos três achados abaixo é um furo de política: os dois primeiros são de visibilidade e trilha,
o terceiro é de informação no modal. Nenhum bloqueia o MT-153; o achado 1 vale corrigir junto,
pela mesma razão dos anteriores (vira a experiência de todos).

## Resultados por cenário

### `E1` — Tool sob `ask` abre modal de confirmação

- **Veredito:** PASSOU
- **O que foi feito:** `crie um arquivo teste.txt com a palavra batata` + `Enter`.
- **O que aconteceu:**

```
                 ┌ confirmar execução de tool ──────────────────────────────┐
                 │tool: fs_write                                            │
                 │                                                          │
                 │+ batata                                                  │
                 │                                                          │
                 │Enter aprova · Esc recusa · ↑↓ rola                       │
                 └──────────────────────────────────────────────────────────┘
usuário: crie um arquivo teste.txt com a palavra batata
agente: ⚙ tool: fs_write — {"content":"batata","path":"teste.txt"}
```

- **Divergência do esperado:** o modal descreve a ação, mas **não mostra o caminho do arquivo**
  — ver achado 3.

### `E2` — Recusar com `Escape`

- **Veredito:** PASSOU
- **O que foi feito:** `Escape` no modal do E1.
- **O que aconteceu:** `ls teste.txt` → `No such file or directory`. A tool **não** executou.
  Expandindo o bloco depois, a recusa está registrada:

```
agente: ⚙ tool: fs_write (expandido)
        comando: {"content":"batata","path":"teste.txt"}
        saída (erro):
        usuário recusou a execução de 'fs_write'
```

- **Divergência do esperado:** na visão padrão (recolhida) **nada** indica que a tool foi
  recusada — ver achado 1.

### `E3` — Aceitar com `Enter`

- **Veredito:** PASSOU
- **O que foi feito:** repetiu o pedido, `Enter` no modal.
- **O que aconteceu:** `cat teste.txt` → `batata`. Arquivo criado com o conteúdo pedido.
- **Divergência do esperado:** nenhuma quanto ao comportamento. Mas a linha do histórico é
  **byte a byte idêntica** à do E2, que foi recusado — ver achado 1.

### `E4` — `Ctrl+A` (modo automático)

- **Veredito:** PASSOU
- **O que foi feito:** `C-a`, depois `crie o arquivo auto.txt com a palavra automatico`.
- **O que aconteceu:** o estado do *toggle* aparece no título da caixa de entrada, e o modal
  **não** apareceu (`grep -c "confirmar execução de tool"` → `0`); `cat auto.txt` → `automatico`.

```
┌ mensagem [auto] ─────────────────────────────────────────────────────────┐
```

- **Divergência do esperado:** nenhuma.

### `E5` — Tool sob `deny`, com `[auto]` **ligado**

- **Veredito:** PASSOU — invariante de segurança **confirmado**
- **O que foi feito:** deliberadamente **sem** desligar o `[auto]` do E4 (é essa a combinação que
  interessa), pediu-se `use a ferramenta fs_edit para substituir a palavra batata por cenoura no
  arquivo teste.txt`.
- **O que aconteceu:** o modelo **de fato chamou** `fs_edit` (a chamada aparece no histórico, com
  os argumentos), nenhum modal foi aberto, o arquivo continuou `batata`, e o bloco expandido traz:

```
agente: ⚙ tool: fs_edit (expandido)
        comando: {"new_string":"cenoura","old_string":"batata","path":"teste.txt"}
        saída (erro):
        tool 'fs_edit' bloqueada por política (deny)
```

- **Divergência do esperado:** nenhuma quanto à política.
- **Por que este resultado não é vacuoso:** um caso de bloqueio que afirma só "o arquivo não
  mudou" passaria também se o modelo simplesmente não tivesse chamado a tool. Aqui há as três
  evidências independentes: a chamada **foi emitida**, a negação **foi devolvida com o motivo**
  (`bloqueada por política (deny)`), e o efeito colateral **não ocorreu** — com o `[auto]` ligado.
- **Nenhuma linha em `.agentry/audit.log`** corresponde a esta negação — ver achado 2.

### `E6` — Saída de tool expansível (ADR-0035)

- **Veredito:** PASSOU (funcionalmente)
- **O que foi feito:** injetou-se um clique de mouse SGR no cabeçalho do bloco
  (`tmux send-keys -H 1b 5b 3c 30 3b …`), já que `send-keys` não emite eventos de mouse.
- **O que aconteceu:** o bloco alternou para expandido, mostrando `comando:` completo e
  `saída (erro):` — foi assim que as evidências do E2 e do E5 acima foram obtidas.
- **Divergência do esperado:** o plano pede "registrar como se descobre que dá para expandir".
  **Não se descobre.** Ver achado 1.

## Achados

| # | Tipo | Cenário | Resumo | Impacto |
|---|---|---|---|---|
| 1 | usabilidade | E2/E3/E5/E6 | O resultado de uma tool é invisível na visão padrão, e o clique que o revela não é anunciado em lugar nenhum | médio-alto |
| 2 | defeito / decisão | E5 | Negação por `permissions.deny` não deixa trilha em `audit.log` | médio |
| 3 | usabilidade (adjacente a segurança) | E1 | O modal de confirmação de escrita não mostra o **caminho** do arquivo | médio |

### Achado 1 — Tool recusada, tool negada e tool executada são indistinguíveis na tela

- **Tipo:** usabilidade / descobribilidade
- **Como reproduzir:** peça uma escrita sob `ask`, recuse com `Escape`; peça de novo, aceite.
  Compare as duas linhas no histórico.
- **Observado:** as duas linhas são idênticas — `⚙ tool: fs_write — {"content":"batata",…}`. O
  bloco recolhido mostra o nome e o início dos argumentos, **nunca o resultado**
  (`linhas_logicas_do_bloco_de_tool`, `tui/mod.rs`, ramo `if !expandido`). O resultado existe e é
  bom (`usuário recusou a execução de 'fs_write'`, `tool 'fs_edit' bloqueada por política
  (deny)`), mas só aparece expandido — e expandir é **clique de mouse** (MT-117/ADR-0035), sem
  equivalente de teclado, sem pista na tela e **sem menção no `/help`** (o texto de ajuda lista
  atalhos e comandos; não fala de clicar em bloco de tool).
- **Esperado:** o desfecho de uma chamada de tool deveria ser legível sem interação — nem que
  seja um marcador de uma palavra na linha recolhida.
- **Por que importa para quem usa:** é a informação que responde "isso rodou ou não?". Sem ela,
  quem recusa não tem confirmação de que recusou, e quem só lê a prosa do modelo pode acreditar
  no contrário do que aconteceu — o modelo do E2 respondeu explicando **como criar o arquivo à
  mão**, o que é fácil de ler como "criou". Em máquina sem mouse (SSH, terminal sem
  `mouse on`), a informação fica **inalcançável**. Vira a experiência padrão com o MT-153.
- **Já conhecido?** Não. Parente do MT-152 (recém-corrigido) na forma: informação correta,
  produzida no lugar certo, que não chega a quem precisa dela.

### Achado 2 — Negação de tool por política não deixa trilha persistente

- **Tipo:** defeito ou comportamento correto — precisa de decisão
- **Como reproduzir:** `permissions.deny = ["fs_edit"]`, provoque a chamada, inspecione
  `.agentry/audit.log`.
- **Observado:** o log contém **apenas** entradas de egresso (`chat_stream … allowed`). A
  negação de `fs_edit` não aparece.
- **Esperado:** indefinido — a ADR-0002 fala em auditar *egresso*, e negar uma tool local não é
  egresso.
- **Por que importa para quem usa:** é a diferença entre "a política me protegeu" e "consigo
  demonstrar que a política me protegeu". Numa homologação corporativa, a segunda é a que conta.
- **Já conhecido?** É o **mesmo formato do MT-149** (recusa na camada de rota não auditada):
  todo caminho *fail-closed* do projeto produz o efeito certo e nenhum registro. Sugiro tratar os
  dois na mesma decisão, e não em separado.

### Achado 3 — O modal de confirmação de escrita não mostra o caminho do arquivo

- **Tipo:** usabilidade adjacente a segurança
- **Como reproduzir:** qualquer `fs_write` sob `ask`.
- **Observado:** o modal traz `tool: fs_write`, o *diff* (`+ batata`) e as teclas. O caminho
  aparece só na linha do histórico **atrás** do modal. Confirmado no código: em
  `linhas_de_confirmacao` (`tui/mod.rs`), quando há *diff* o ramo mostra o *diff* e o
  `argumentos: {…}` — que é onde vive o `path` — **não** é montado. Ou seja, é exatamente em
  `fs_write`/`fs_edit`, as tools em que o alvo mais importa, que ele some.
- **Esperado:** o caminho na primeira linha do modal, junto do nome da tool.
- **Por que importa para quem usa:** o modal é a última barreira antes de escrever em disco.
  Aprovar sem ver **onde** é aprovar metade da informação; com o `[auto]` desligado e uma
  sequência de escritas, é fácil aprovar a errada.
- **Já conhecido?** Não.

## Achados não previstos pelo plano

- **`tmux send-keys` não emite eventos de mouse**, então o E6 seria impossível de executar como
  escrito. Contornado injetando a sequência SGR crua
  (`ESC [ < 0 ; <col> ; <lin> M` e `… m`) via `send-keys -H`. **A receita do plano precisa
  registrar isso**, ou a próxima execução marca o E6 como `BLOQUEADO` sem necessidade.
- **O modelo local responde à negação inventando alternativas** ("use `echo "cenoura" >
  teste.txt`"). É comportamento do `llama3.1:8b`, não do `agentry`, e não fura nada — a tool de
  shell segue bloqueada por padrão. Vale registrar porque **lido rápido, parece que a tarefa foi
  cumprida**, o que agrava o achado 1.

## Perguntas de usabilidade

1. **Tempo até entender o que fazer:** imediato — o modal de confirmação explica a si mesmo
   (`Enter aprova · Esc recusa · ↑↓ rola`), que é o padrão certo e contrasta com o achado 1.
2. **Atalhos descobríveis?** Agora sim para teclado (`/help` mostra `Ctrl+A`, corrigido no
   MT-150). **Não** para o clique de expandir, que não é mencionado em lugar nenhum.
3. **A mensagem dizia o que fazer a seguir?** A negação diz o motivo (`bloqueada por política
   (deny)`), o que é o essencial. Não diz onde mudar a política — aceitável, já que quem
   configurou `deny` sabe onde ele está.
4. **Dúvida sobre para onde iam os dados?** Não. O título mostra `⌂ ollama:llama3.1:8b` o tempo
   todo, e `⌂` marca local.
5. **O que faria desistir?** O achado 1, em uso longo: um histórico onde não dá para saber o que
   de fato rodou é um histórico em que não se confia.

## Higiene pós-teste

| Verificação | OK? |
|---|---|
| `~/.agentry/` real não foi tocado | sim (não existe nesta máquina; nada foi criado) |
| `HOME` temporário só tem o que o teste criou | sim (só o `.zshrc` da receita; nenhum `.agentry/`) |
| Nenhum processo `agentry` órfão | sim (o único vivo é a sessão do próprio usuário, PID 3775112, pai `-zsh`, 12h — **não tocado**) |
| Terminal restaurado após `Ctrl+C` | sim (saiu limpo, sessão `tmux` encerrada junto) |
| `stderr` sob TUI | vazio, como esperado após o MT-152 |
| Nenhum segredo aparece neste relatório | sim |
