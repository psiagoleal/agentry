<!-- Caminho relativo: usage-test/PLANO-DE-TESTE.md -->

# Plano de teste de uso do `agentry` — modo TUI

> **Para quem executa:** um agente (ou uma pessoa) conduzindo o `agentry` de ponta a ponta na
> perspectiva de quem vai usar a ferramenta, não de quem a escreveu. O produto deste plano é o
> relatório de [`RELATORIO-MODELO.md`](./RELATORIO-MODELO.md), preenchido.
>
> **O que este plano NÃO é:** substituto de `cargo test`. A suíte automatizada cobre unidades e
> a fiação (`crates/cli/tests/e2e.rs`, Fase K). Aqui se testa a **experiência**: o que aparece
> na tela, o que acontece quando se erra, se dá para descobrir o próximo passo sem ler o código.

## 0. O problema que este plano precisa resolver primeiro

A TUI exige um **terminal de verdade** (modo bruto, tela alternativa, `crossterm`). Um agente
em sessão não-interativa **não consegue** digitar nela: não há TTY, e redirecionar `stdin` faz
a TUI nem iniciar direito.

A solução adotada é dirigir a TUI por **`tmux`** (3.6 disponível nesta máquina), que dá um TTY
real e permite enviar teclas e capturar a tela como texto. Todo cenário abaixo é escrito nesse
formato. Quem for executar à mão pode ignorar os comandos `tmux` e apenas digitar.

### Receita base

```bash
# Abre uma sessão de terminal isolada, 120x40, já dentro do workspace de teste
tmux new-session -d -s agentry-teste -x 120 -y 40 -c "$WORKSPACE"

# Envia texto e teclas (nomes de tecla: Enter, Escape, Up, Down, C-c, C-p, C-a, C-z)
tmux send-keys -t agentry-teste 'texto a digitar'
tmux send-keys -t agentry-teste Enter

# Lê a tela inteira como texto — é isto que vai para o relatório
tmux capture-pane -t agentry-teste -p

# Para avaliar QUALQUER COISA VISUAL, capture com -e (preserva as cores)
tmux capture-pane -t agentry-teste -p -e

# Encerra
tmux kill-session -t agentry-teste
```

**Use `-e` ao julgar aparência.** Sem ele o logo — um raster de meio-blocos `▀` com *true-color*
por célula — vira um retângulo sólido e **parece defeito de renderização**. Isso já custou um
falso positivo numa execução real.

**Sempre espere antes de capturar.** A TUI redesenha de forma assíncrona; capturar
imediatamente depois de `send-keys` costuma pegar a tela anterior. Use `sleep 1` (ou `sleep 3`
quando houver chamada ao modelo) entre enviar e capturar. Se a captura vier vazia ou truncada,
**repita a captura** antes de declarar falha — e registre que precisou repetir, porque isso em
si é um achado de responsividade.

## 1. Preparação

1. **Build:** `make build` (ou `cargo build --release -p agentry`). Anote a duração.
2. **Workspace isolado:** crie um diretório novo fora do repositório
   (ex.: `mktemp -d`) e exporte `WORKSPACE` apontando para ele. **Nunca rode os cenários dentro
   do repositório do `agentry`:** a busca de configuração sobe a árvore e acharia o
   `.agentry/` do próprio projeto.
3. **`HOME` isolado:** exporte `HOME` para um diretório temporário antes de subir o `tmux`.
   Isso protege o `~/.agentry/` real (configuração global e **credenciais**) de qualquer
   escrita acidental. Confira ao final que ele continua vazio.
   **Crie um `.zshrc` vazio nesse `HOME` antes de qualquer coisa** (`touch $HOME/.zshrc`): sem
   ele, o assistente `zsh-newuser-install` intercepta a primeira digitação e **engole
   caracteres do comando**, o que parece falha do `agentry` e não é. Achado de uma execução real.
4. **Provider:** use o Ollama local se houver; senão, registre no relatório que os cenários
   dependentes de modelo não puderam rodar. **Não** configure provider de nuvem para este
   plano — nenhum cenário aqui precisa sair da máquina.

## 2. Regras de conduta

- **Nunca** execute comando destrutivo fora do `WORKSPACE`.
- **Nunca** registre no relatório chave de API, token ou conteúdo de `credentials.json`. Se um
  segredo aparecer na tela, isso é **achado grave** — descreva o local, não o valor.
- Registre o que **viu**, não o que esperava ver. Captura de tela em texto vale mais que
  descrição.
- Um cenário que não pôde ser executado é `BLOQUEADO`, nunca `PASSOU`.
- Achado fora do roteiro é bem-vindo: registre na seção "Achados não previstos".

## 3. Cenários

Cada cenário tem **objetivo**, **passos**, **esperado** e o que **registrar**. O veredito é
`PASSOU` / `FALHOU` / `BLOQUEADO`.

### A. Primeiro uso, sem nada configurado

- **A1 — `agentry --init`.** Rode no `WORKSPACE` vazio. Esperado: cria
  `.agentry/agentry.settings.json`, mensagem dizendo o que foi criado, sem nenhuma chamada de
  rede. Registrar: mensagem exibida e se o arquivo é legível/comentado o bastante para o
  usuário saber o que editar.
- **A2 — `agentry --init` de novo.** Esperado: idempotente, não sobrescreve, diz que já existia.
- **A3 — TUI sem provider algum.** Suba `agentry --tui` num diretório **sem** configuração.
  Esperado: erro compreensível dizendo o que falta, não *panic* nem tela corrompida.
  **Este é o cenário mais importante do bloco:** é o primeiro contato de quem errou a
  configuração.

### B. Abertura e layout da TUI

- **B1 — Abertura.** `agentry --tui`. Esperado: logo, área de histórico, caixa de entrada,
  indicação da rota ativa (provider/modelo). Registrar a captura inteira.
- **B2 — Redimensionamento.** Com a TUI aberta:
  `tmux resize-window -t agentry-teste -x 80 -y 24`, depois `-x 160 -y 50`. Esperado: layout se
  reajusta sem sobreposição nem texto cortado. Registrar as duas capturas.
- **B3 — Terminal muito pequeno.** Redimensione para `-x 40 -y 10`. Esperado: degrada de forma
  legível; não pode entrar em laço de erro nem *panic*.

### C. Conversa e streaming

- **C1 — Mensagem simples.** Digite `diga apenas a palavra batata` + `Enter`. Esperado: resposta
  aparece **incrementalmente** (capture duas vezes com 1s de intervalo para evidenciar o
  streaming), a caixa de entrada esvazia, o histórico rola.
- **C2 — Mensagem longa.** Peça um texto de ~40 linhas. Esperado: rolagem automática acompanha;
  `Up`/`Down` navegam o histórico depois.
- **C3 — Mensagem com acento e emoji.** Esperado: sem quebra de layout nem caractere corrompido
  (largura de caractere é fonte clássica de bug de TUI).
- **C4 — `Enter` com entrada vazia.** Esperado: nada acontece; não envia turno vazio ao modelo.

### D. Seletor de modelo (`Ctrl+P`)

- **D1 — Abrir.** `C-p`. Esperado: lista de candidatos declarados.
- **D2 — Cancelar.** `Escape`. Esperado: fecha sem trocar nada; a rota exibida continua a mesma.
- **D3 — Escolher.** Selecione outro candidato. Esperado: a rota exibida muda e a próxima
  mensagem usa o novo alvo.

### E. Tools, permissão e confirmação

Configure `permissions.ask` para incluir uma tool de escrita antes deste bloco.

- **E1 — Tool sob `ask`.** Peça algo que exija escrita (ex.: "crie um arquivo teste.txt com a
  palavra batata"). Esperado: **modal de confirmação** descrevendo a ação.
- **E2 — Recusar.** `Escape`. Esperado: a tool **não** executa; o arquivo não existe.
- **E3 — Aceitar.** Repita e aceite. Esperado: arquivo criado com o conteúdo pedido.
- **E4 — `Ctrl+A` (auto).** Alterne para automático e repita. Esperado: executa sem perguntar, e
  o estado do *toggle* fica visível na tela.
- **E5 — Tool sob `deny`.** Configure uma tool em `permissions.deny` e peça para usá-la.
  Esperado: negada **sem** modal — `Ctrl+A` não pode afrouxar `deny`. Registrar explicitamente:
  este é um invariante de segurança.
- **E6 — Saída de tool expansível (ADR-0035).** Após uma tool com saída longa, verifique se o
  bloco aparece recolhido e pode ser expandido. Registrar como se descobre que dá para expandir.

### F. Desfazer (`Ctrl+Z`)

- **F1 — Undo após escrita.** Depois de E3, `C-z`. Esperado: o arquivo volta ao estado anterior
  e a TUI diz o que foi desfeito.
- **F2 — Undo sem nada a desfazer.** `C-z` numa sessão limpa. Esperado: mensagem clara, não erro.

### G. Comandos de barra

Para **cada** comando, registre a saída: `/help`, `/model`, `/task-class`, `/usage`, `/save`,
`/sessions`, `/recall`, `/remember`, `/compact`, `/undo`, `/workspace`, `/init`, `/exit`.

- **G1 — `/help` é suficiente?** Pergunta central: alguém que abriu a TUI pela primeira vez
  consegue, só com `/help`, descobrir os atalhos (`Ctrl+P`, `Ctrl+A`, `Ctrl+Z`)? Se não,
  isso é achado de usabilidade, não implementação.
- **G2 — Comando inexistente.** `/naoexiste`. Esperado: mensagem de comando desconhecido; a TUI
  **não** pode cair.
- **G3 — `/save` e `/sessions` e `--resume`.** Salve, saia, reabra com `--resume` e confirme que
  o histórico volta.

### H. Falhas e caminhos infelizes

- **H1 — Provider inalcançável.** Aponte `baseUrl` para uma porta fechada. Esperado: erro
  tratado e legível **dentro** da TUI, sem tela corrompida e sem travar.
- **H2 — Egresso bloqueado.** Declare o candidato de rota como `cloud-ok` sem perfil que o
  permita. Esperado: recusa explícita. Registrar **exatamente** a mensagem — hoje ela sai em
  `stderr`, e sob TUI `stderr` é descartado; **se a mensagem não aparecer na tela, é achado
  grave** (o usuário ficaria sem explicação nenhuma).
- **H3 — Guardrail de PII.** Configure `{"id":"bloqueia-cpf","detect":"cpf","action":"block"}`
  em `guardrails.input` e envie um CPF válido de teste (`529.982.247-25` — número de teste, não
  pertence a ninguém). Esperado: bloqueio antes de sair da máquina, com o **id da regra** na
  tela e **nunca** o valor. Depois, confirme que `.agentry/audit.log` nomeia o detector e **não**
  contém o CPF.
- **H4 — Interrupção no meio do streaming.** Envie uma pergunta longa e mande `C-c` durante a
  resposta. Esperado: sai limpo, **terminal restaurado** (sem modo bruto preso, sem tela
  alternativa órfã). Confirme rodando `echo teste` depois: se o terminal ficar estranho, é
  achado grave.

### I. Encerramento e higiene

- **I1 — `Ctrl+C`.** Esperado: sai e restaura o terminal.
- **I2 — `/exit`.** Mesmo esperado.
- **I3 — `HOME` intacto.** Confirme que o `HOME` temporário só tem o que o teste criou, e que o
  `~/.agentry/` real **não** foi tocado.
- **I4 — Processos órfãos.** `ps` para confirmar que nenhum `agentry` ficou rodando.
  **Cuidado:** confira `etime` e o processo pai antes de acusar. Uma sessão `agentry` do próprio
  usuário, aberta antes do teste, aparece aqui e **não** é órfã — matá-la seria destruir trabalho
  alheio.

## 4. Como reportar

Preencha [`RELATORIO-MODELO.md`](./RELATORIO-MODELO.md). Regras:

- Um veredito por cenário, com a captura de tela relevante.
- Achado sem cenário correspondente vai em "Achados não previstos".
- **Separe usabilidade de defeito.** "Não descobri como expandir a saída da tool" é usabilidade;
  "a TUI travou" é defeito. Os dois valem, mas confundir os dois atrapalha a priorização.
- Para cada achado, diga o **impacto para quem usa**, não só o sintoma técnico.

## 5. Itens já conhecidos — não reportar como novos

Estes já estão rastreados em `docs/roadmap-v0.18.md`; confirme se aparecem, mas marque como
**conhecido**:

- **MT-147** — resposta vazia com código de saída 0 quando o endpoint devolve corpo não-SSE.
- **MT-148** — regressão do `structuredOutput` (Ollama) sem cobertura automatizada.
- **MT-149** — recusa de rota por egresso não deixa trilha no `audit.log` (relacionado a **H2**).

## 6. Depois que o `--tui` virar o padrão

Quando a TUI passar a ser o modo default, **repita os blocos B a I** invocando `agentry` **sem**
a flag, e acrescente:

- **J1 — Saída não-TTY.** `agentry | cat` e `agentry < /dev/null`. Esperado: **não** tenta abrir
  TUI; cai no modo texto. Uma TUI tentando desenhar num *pipe* quebraria qualquer script.
- **J2 — Escape hatch.** Confirme que existe forma explícita de pedir o modo texto, e que ela
  está documentada em `--help`.
- **J3 — One-shot intacto.** `agentry "uma tarefa"` deve continuar sem TUI, byte a byte como
  antes.
