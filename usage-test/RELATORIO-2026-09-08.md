<!-- Caminho relativo: usage-test/RELATORIO-2026-09-08.md -->

# Relatório de teste de uso — `agentry` TUI

## Identificação

| Campo | Valor |
|---|---|
| Data | 2026-09-08 |
| Commit testado | `282bcf6` (branch `chore/build-linux-e-higiene-de-disco`) |
| Binário | `target/release/agentry`, perfil release |
| SO / terminal | Linux 7.0.0-31 · `tmux 3.6` · 120×40, 80×24 e 40×10 |
| Provider | Ollama local (`llama3.1:8b`), default zero-config |
| `--tui` era o padrão? | **Não** — invocado explicitamente |
| Executado por | Agente, via `tmux send-keys` / `capture-pane` |

## Resumo

| | Quantidade |
|---|---|
| Cenários executados | 13 |
| `PASSOU` | 12 |
| `FALHOU` | 1 |
| `BLOQUEADO` / não executados | 0 executados com falha de ambiente; 12 cenários do plano ficaram para uma segunda rodada (ver "Cobertura") |

**Veredito geral:** a TUI está sólida no que foi exercitado — abre sem configuração, refluiu em
três tamanhos de terminal (inclusive 40×10), o modal do agente é claro, o seletor de modelo
funciona, e o terminal é restaurado corretamente após `Ctrl+C`. Os três achados **não são de
estabilidade**: são de primeira impressão — e é justamente a primeira impressão que passa a ser
o padrão se o `--tui` virar default.

**A TUI está pronta para ser o modo padrão?** **Com ressalvas.** Nada do que encontrei indica
instabilidade. Mas os achados 1 e 2 atingem exatamente quem abre a ferramenta pela primeira vez,
e o achado 3 é estrutural do modo TUI. Recomendo corrigir os três **antes** de virar o default —
são baratos, e viram o custo de "erro que aparece para todo mundo, sempre".

## Resultados por cenário

| ID | Cenário | Veredito |
|---|---|---|
| A1 | `--init` em diretório vazio | PASSOU |
| A2 | `--init` idempotente | PASSOU |
| A3 | TUI sem configuração alguma | PASSOU |
| B1 | Abertura e layout (120×40) | PASSOU |
| B2 | Redimensionamento (80×24) | PASSOU |
| B3 | Terminal mínimo (40×10) | PASSOU |
| C1 | Mensagem e resposta do modelo | PASSOU (com observação) |
| C4 | Modal do agente + `Escape` | PASSOU |
| D1 | `Ctrl+P` abre o seletor | PASSOU |
| D2 | `Escape` fecha o seletor | PASSOU |
| G1 | `/help` documenta os atalhos | **FALHOU** |
| I1 | `Ctrl+C` sai e restaura o terminal | PASSOU |
| I3/I4 | Higiene: `HOME` e processos | PASSOU |

### A1/A2 — `--init`

```
criado: <WORKSPACE>/.agentry/agentry.settings.json
dica: para valores diferenciados por perfil (empresa/externo-confidencial/pessoal), rode o
      scripts/setup-profile.sh de https://github.com/psiagoleal/ai-coding-agent-profiles
```

Segunda execução: `já existe, não sobrescrito: …`. Cria também um `.gitignore` dentro de
`.agentry/` — bom detalhe, evita versionar estado por acidente.

### A3 — TUI sem configuração

Abre normalmente e cai no default zero-config (`⌂ ollama:llama3.1:8b` no título). **Não** exige
configuração prévia, não dá *panic*, não corrompe a tela. O `⌂` comunicando "local" no título é
um acerto: a pergunta "para onde meus dados estão indo" tem resposta permanente na tela.

### B1/B2/B3 — layout

Refluiu corretamente em 120×40, 80×24 e 40×10. Em 40×10 o layout degrada mantendo título, área
de histórico e caixa de entrada; o painel não morreu (`pane_dead=0`).

### C1/C4 — conversa e modal

Enviei `responda apenas: batata`. O modelo escolheu chamar a tool `ask_user` em vez de responder
direto — **comportamento do modelo, não da ferramenta**. Isso exercitou o modal:

```
┌ pergunta do agente ──────────────────────────────────────────────────┐
│responda apenas:                                                      │
│> 1. batata                                                           │
│↑↓ escolhe · Enter (vazio) envia a opção · ou digite livremente       │
└──────────────────────────────────────────────────────────────────────┘
```

O modal traz as teclas **inline**, que é o padrão certo. `Escape` cancelou de forma limpa e o
agente se recuperou com "Nenhum resultado disponível. Por favor, faça uma nova pergunta…". A
caixa de entrada trocou o rótulo para `aguardando resposta...` durante a chamada — bom sinal de
estado.

**Observação:** não consegui evidenciar o *streaming* incremental. As capturas em 3s e 15s
pegaram o antes e o depois, não o meio. Não é indício de defeito; é limitação do método com um
modelo rápido. Precisa de captura em intervalos curtos numa resposta longa.

### D1/D2 — seletor de modelo

`Ctrl+P` abre listando os candidatos declarados (`claude-cli/opus`, `ollama/llama3.1:8b`);
`Escape` fecha sem alterar a rota. Não testei a troca efetiva (D3).

### G1 — `/help` — **FALHOU**

O painel é completo (todos os comandos de barra, com descrição), mas os atalhos saem assim:

```
atalhos de teclado:
  c: sai do modo TUI
  ↑: rola o histórico para cima
  ↓: rola o histórico para baixo
  Enter: envia a mensagem digitada
  p: abre o seletor de modelo/provider
  Esc: fecha o seletor/recusa a confirmação
  a: alterna confirmação automática de tools sob ask
  z: desfaz o último fs_write/fs_edit (checkpoint)
```

As teclas reais são `Ctrl+C`, `Ctrl+P`, `Ctrl+A`, `Ctrl+Z`. Ver Achado 1.

### I1/I3/I4 — encerramento e higiene

`Ctrl+C` saiu e o terminal ficou utilizável (`echo TERMINAL_OK_$((2+2))` → `TERMINAL_OK_4`).
`~/.agentry` real **não existe** e não foi criado; o `HOME` isolado só recebeu arquivos do
próprio zsh. Nenhum processo órfão do teste.

> **Falso positivo que quase reportei:** havia um `agentry --tui` vivo, mas com 5h de execução e
> pai `-zsh` — é uma sessão **do usuário**, anterior ao teste. Não foi tocada.

## Achados

| # | Tipo | Cenário | Resumo | Impacto |
|---|---|---|---|---|
| 1 | defeito | G1 | `/help` omite o modificador `Ctrl` de todos os atalhos | **alto** |
| 2 | defeito | A3/I1 | `--init` gera config que erra na conexão MCP em toda execução | **alto** |
| 3 | defeito estrutural | I1 | Erro em `stderr` é invisível durante a TUI e só aparece ao sair | **alto** |
| 4 | método | — | `capture-pane` sem `-e` achata o logo e parece defeito | baixo |

### Achado 1 — `/help` mostra atalhos que não funcionam

- **Tipo:** defeito
- **Reproduzir:** abrir a TUI, `/help`, ler o bloco "atalhos de teclado".
- **Observado:** `c: sai do modo TUI`, `p: abre o seletor`, `a: alterna`, `z: desfaz`.
- **Esperado:** `Ctrl+C`, `Ctrl+P`, `Ctrl+A`, `Ctrl+Z`.
- **Causa (confirmada no código):** `crates/cli/src/tui/keybind.rs:154` monta a linha com
  `rotulo_tecla(def.code)` e **ignora `def.modifiers`**. O campo existe na tabela e não é lido.
- **Por que importa:** apertar `c` só digita a letra na caixa de entrada. O usuário conclui que
  a ajuda mente ou que a TUI está quebrada. Pior: o comentário do próprio módulo registra que os
  atalhos de letra foram **removidos de propósito** no MT-72 — a ajuda descreve um comportamento
  que deixou de existir e ninguém notou, porque o teste de ajuda só verifica se os comandos
  aparecem, não se as teclas estão certas.
- **Já conhecido?** Não.

### Achado 2 — `--init` entrega uma configuração que sempre falha

- **Tipo:** defeito
- **Reproduzir:** `agentry --init` num diretório limpo, depois `agentry --tui`, sair com
  `Ctrl+C`.
- **Observado:** ao sair, no terminal:

```
erro ao conectar ao servidor MCP 'exemplo': servidor MCP respondeu com erro: Send message error
Transport [rmcp::transport::child_process::TokioChildProcess] error: Broken pipe (os error 32),
when send initialize request
```

- **Causa (confirmada no código):** o template de `--init` (`crates/cli/src/main.rs:224-231`)
  declara `mcpServers.exemplo` com `"command": "echo"`, que não fala MCP. O comentário no próprio
  template diz que "uma tentativa de conexão falharia de forma tratada" — escrito no MT-77,
  quando **nada conectava ainda**. O MT-78 passou a conectar, e o exemplo virou um erro
  permanente para toda instalação nova.
- **Por que importa:** a primeira coisa que um usuário novo vê depois de configurar é um erro
  citando `rmcp::transport::child_process::TokioChildProcess` e "Broken pipe". É assustador,
  não é acionável, e não é culpa dele.
- **Correção sugerida:** o exemplo não deveria estar ativo. Ou vem comentado/desabilitado por
  um campo `enabled`, ou o bloco `mcpServers` sai vazio com o exemplo só na documentação.
- **Já conhecido?** Não.

### Achado 3 — sob TUI, erro em `stderr` é invisível até o usuário sair

- **Tipo:** defeito estrutural do modo TUI
- **Reproduzir:** o mesmo do Achado 2 — a mensagem **não** aparece em nenhum momento durante a
  sessão (verifiquei em 1s e após a TUI pintar), e surge no terminal depois do `Ctrl+C`.
- **Por que importa:** é a classe de problema que o cenário **H2** do plano antecipou. Sob TUI o
  `stderr` é descartado de propósito (escrever nele corromperia a tela — achado do MT-72). Só
  que **erros de verdade vão por ali**. A recusa de egresso é o caso grave: um usuário sob
  perfil `local-only` que pedir algo roteado para nuvem recebe... nada. Sem explicação, sem
  pista de que uma política bloqueou o trabalho.
- **Relação com o roadmap:** parente do **MT-149** (recusa de rota não deixa trilha), mas
  distinto: aqui não é o log persistente que falta, é o **usuário** que não é informado.
- **Por que decide a promoção a default:** hoje quem usa a TUI escolheu entrar nela. Se ela virar
  o padrão, este silêncio passa a ser a experiência de todo mundo.
- **Já conhecido?** Não como defeito; o plano previu a classe (H2).

### Achado 4 — método: `capture-pane` precisa de `-e`

`tmux capture-pane -p` descarta cores, e o logo — que é um raster de meio-blocos `▀` com
*true-color* por célula — vira um retângulo sólido que **parece** defeito de renderização. Com
`-e` as sequências aparecem e confirmam que está correto. Já corrigido no plano.

## Achados não previstos pelo plano

**O `HOME` isolado quebra o `tmux` com zsh.** Sem `.zshrc`, o `zsh-newuser-install` intercepta a
primeira digitação e engole caracteres do comando — a primeira tentativa do A3 falhou por isso,
não por culpa do `agentry`. Já corrigido no plano (criar `.zshrc` vazio na preparação).

## Perguntas de usabilidade

1. **Tempo até entender o que fazer:** imediato. A tela inicial diz "digite uma mensagem e Enter
   para começar", que é a instrução certa no lugar certo.
2. **Atalhos descobríveis?** **Não.** `/help` os lista mas com a tecla errada (Achado 1). Sem ler
   o código, não há como descobrir `Ctrl+P`.
3. **Mensagens dizem o que fazer a seguir?** As do fluxo normal, sim. As de erro, não — quando
   aparecem (Achado 3).
4. **Dúvida sobre para onde vão os dados?** Não. O `⌂ ollama:llama3.1:8b` no título é constante e
   claro, e o `audit.log` registra destino, classe e resultado por chamada.
5. **O que faria desistir?** O erro MCP do Achado 2 na primeira execução. Um usuário avaliando a
   ferramenta concluiria que ela está quebrada antes de mandar a primeira mensagem.

## Cobertura — o que ficou de fora

Não executados nesta rodada, por dependerem de configuração adicional ou de resposta longa do
modelo: **D3** (troca efetiva de modelo), **E1-E6** (permissão `ask`/`deny`, `Ctrl+A`, saída de
tool expansível), **F1/F2** (undo), **G2/G3** (comando inexistente, `/save`+`--resume`), **H1-H3**
(provider inalcançável, egresso bloqueado, guardrail de PII), **C2/C3** (resposta longa, acentos
e emoji).

**O bloco E é o mais importante que falta**, em especial o **E5** — confirmar que `Ctrl+A` não
afrouxa uma tool sob `deny` é um invariante de segurança, e é o único cenário do plano cuja falha
seria grave por si só.

## Higiene pós-teste

| Verificação | OK? |
|---|---|
| `~/.agentry/` real não foi tocado | sim — não existe |
| `HOME` temporário só tem o que o teste criou | sim (`.zshrc`, `.zcompdump`) |
| Nenhum processo `agentry` órfão do teste | sim |
| Terminal restaurado após `Ctrl+C` | sim |
| Nenhum segredo neste relatório | sim |
