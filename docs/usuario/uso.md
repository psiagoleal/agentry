<!-- Caminho relativo: docs/usuario/uso.md -->

# Uso da CLI e do REPL

## Modo one-shot

```bash
agentry "liste os arquivos .rs deste projeto"
```

Roda uma única tarefa (com o loop interno de tool-calls até chegar numa resposta final) e
sai. A resposta é exibida incrementalmente (*streaming*) conforme o modelo gera texto.

Ao final, uma linha `[uso] ...` em `stderr` (nunca em `stdout` — não afeta um `agentry "..."
| algum-comando` que capture só a resposta) mostra o total de tokens consumidos pela tarefa.

## Modo REPL

```bash
agentry
```

Sem tarefa na invocação, entra em modo interativo: histórico de conversa persiste entre
mensagens, até `/exit`, `/quit` ou EOF (Ctrl+D).

```
> qual a versão do Rust usada neste projeto?
[resposta do modelo, com streaming]
> /model llama3.1:70b
modelo alterado para: llama3.1:70b
> agora refaça a análise com mais detalhe
[resposta com o novo modelo]
> /exit
```

## Modo TUI

```bash
agentry --tui
```

Terceiro modo de invocação, opcional (*opt-in* — sem `--tui`, o comportamento *one-shot*/REPL
de texto acima continua idêntico, byte a byte). Roda sobre a **mesma** `Session`/`Router` do
REPL de texto — histórico de chat rolável numa área própria, com uma caixa de entrada de
mensagem fixa embaixo, em vez de um prompt linear.

*Keybindings* padrão (não customizáveis nesta versão):

| Tecla | Ação |
|---|---|
| `Enter` | Envia a mensagem digitada (ou confirma um modal aberto — seletor de modelo, confirmação de tool, pergunta do agente). |
| `↑` / `↓` | Rola o histórico de mensagens (ou navega a lista do seletor de modelo, quando aberto). |
| `Ctrl+P` | Abre o seletor de modelo/*provider* — busca difusa sobre os candidatos já declarados na *task-class* ativa (nunca introduz um candidato novo, mesma disciplina de `/model`/`/provider` do REPL). |
| `Ctrl+A` | Alterna confirmação automática (`auto`/`normal`) de tools sob `ask` — só acelera a aprovação; nunca aprova uma tool em `deny`. |
| `Ctrl+Z` | Desfaz o checkpoint mais recente de `fs_write`/`fs_edit` (ver [Checkpoints e *undo*](#checkpoints-e-undo-de-mudancas-de-arquivo) abaixo) — resultado aparece como uma mensagem no histórico de chat. |
| `Esc` | Fecha o seletor de modelo aberto, ou recusa/cancela uma confirmação/pergunta pendente. |
| `Ctrl+C` | Sai do modo TUI a qualquer momento (mesmo com um modal aberto). |

Tools sob `ask` ([`permissions`](configuracao.md#permissions)) abrem um modal de confirmação —
para `fs_write`/`fs_edit`, o modal mostra o **diff** de verdade (linhas removidas/adicionadas)
em vez dos argumentos brutos da chamada. A tool `ask_user` (ver
[Ferramentas do agente](#ferramentas-do-agente) abaixo) também abre um modal, com a pergunta
(e sugestões, se houver) e uma caixa de resposta em texto livre.

A trilha de governança não muda com o modo TUI: nenhum caminho de rede/egresso novo é
introduzido — é só uma apresentação diferente sobre a mesma `Session`/`Router`/`ToolRegistry`
já documentados no restante deste guia.

O rodapé da caixa de entrada, além da legenda de *keybindings*, mostra o total de tokens
consumidos pela sessão até o último turno concluído — mesmo dado do resumo do modo *one-shot*
e do comando `/usage` do REPL, atualizado automaticamente a cada resposta.

## Flags de invocação (one-shot)

| Flag | Efeito |
|---|---|
| `-m, --model <nome>` | Modelo a usar nesta invocação (sobrescreve o *default*). |
| `-p, --provider <nome>` | Provider a usar nesta invocação — `ollama` (padrão) ou `litellm`, se [`providers.litellm`](configuracao.md#providerslitellm) estiver configurado. Restringe a escolha aos candidatos já declarados na rota; nome fora dela é erro tratado. |
| `--temperature <n>` | Temperatura de amostragem. |
| `--top-p <n>` | *Top-p* (*nucleus sampling*). |
| `--max-tokens <n>` | Limite de tokens de saída. |
| `--system <texto>` | *System prompt* desta invocação. |
| `--reasoning on\|off` | Raciocínio estendido, se o modelo suportar. |
| `--task-class <nome>` | Task-class a usar nesta invocação — ver [`taskClasses`](configuracao.md#taskclasses). *Default*: `chat`. |
| `--ollama-host <host:porta>` | Servidor Ollama a usar (*default*: `127.0.0.1:11434`). |
| `--tui` | Entra no [modo TUI](#modo-tui) em vez do REPL de texto. Incompatível com `--init` e com uma tarefa *one-shot*. |
| `--init` | Cria `.agentry/agentry.settings.json` e sai (ver [Configuração](configuracao.md)). |
| `--profile <nome>` | Com `--init`: busca a configuração real daquele perfil. |
| `--undo` | Desfaz o checkpoint mais recente de `fs_write`/`fs_edit` (ver [Checkpoints e *undo*](#checkpoints-e-undo-de-mudancas-de-arquivo) abaixo) e sai, sem rodar tarefa. Incompatível com `--init`/`--tui`/tarefa. |
| `--remember <fato>` | Grava `<fato>` como memória de projeto (ver [Memória de projeto](#memoria-de-projeto-remember) abaixo) e sai, sem rodar tarefa. Incompatível com `--init`/`--tui`/tarefa. |

```bash
agentry --model llama3.1:70b --temperature 0.2 "revise este diff"
agentry --ollama-host 127.0.0.1:11435 "..."   # outra porta/instância do Ollama
agentry --task-class revisao-em-nuvem "revise a segurança deste diff"
```

## Comandos de barra (REPL)

Equivalentes interativos das flags acima — o valor passa a valer para as mensagens
seguintes, até ser trocado de novo:

| Comando | Efeito |
|---|---|
| `/model <nome>` | Troca de modelo a partir da próxima mensagem — sempre na task-class `chat`, mesmo que `/task-class` tenha trocado a task-class ativa para outra (ver nota abaixo). |
| `/provider <nome>` | Restringe a task-class ativa ao candidato deste provider (`ollama`/`litellm`). |
| `/temperature <n>` | Ajusta a temperatura. |
| `/top_p <n>` (ou `/top-p`) | Ajusta o *top-p*. |
| `/max_tokens <n>` (ou `/max-tokens`) | Ajusta o limite de tokens de saída. |
| `/system <texto>` | Atualiza o *system prompt* a partir da próxima mensagem. |
| `/reasoning on\|off` | Liga/desliga raciocínio estendido. |
| `/task-class <nome>` | Troca a task-class ativa (rota + preset) a partir da próxima mensagem — ver [`taskClasses`](configuracao.md#taskclasses). |
| `/compact` | Resume o histórico da sessão numa única mensagem — reduz o consumo de tokens em conversas longas. |
| `/usage` | Mostra o total de tokens consumidos pela sessão até aquele ponto — sem *side-effect* na conversa. |
| `/undo` | Desfaz o checkpoint mais recente de `fs_write`/`fs_edit` (ver [Checkpoints e *undo*](#checkpoints-e-undo-de-mudancas-de-arquivo) abaixo). |
| `/remember <fato>` | Grava `<fato>` como memória de projeto (ver [Memória de projeto](#memoria-de-projeto-remember) abaixo) — disponível em sessões futuras. |
| `/init` (ou `/init <perfil>`) | Cria `.agentry/agentry.settings.json` sem sair do REPL. |
| `/exit` (ou `/quit`) | Encerra o REPL. |

Qualquer outra linha é tratada como mensagem de usuário.

**`/model` e `/task-class` são independentes:** `/model` sempre redeclara a task-class `chat`
com o modelo pedido (via Ollama), mesmo que você tenha trocado para outra task-class com
`/task-class` — trocar de modelo dentro de uma task-class customizada (ex.: uma que só usa
LiteLLM) não é suportado nesta versão. Se você está numa task-class diferente de `chat` e
quer voltar a ajustar o modelo Ollama, use `/task-class chat` primeiro.

**`/usage` não zera com `/compact`:** o total de tokens mostrado é o consumo real desde o
início da sessão, incluindo a própria chamada de compactação — resumir o histórico reduz o
que vai para o modelo nas próximas mensagens, mas não desfaz o que já foi consumido até ali.

## Checkpoints e *undo* de mudanças de arquivo

Toda chamada bem-sucedida de `fs_write`/`fs_edit` grava um checkpoint (conteúdo do arquivo
**antes** da mudança) numa pilha — `--undo` (*one-shot*), `/undo` (REPL) e `Ctrl+Z` (TUI)
desfazem o **mais recente**, restaurando o conteúdo anterior (ou removendo o arquivo, se ele
não existia antes da mudança desfeita). Chamar de novo desfaz o passo anterior a esse — sem
seleção de checkpoint específico nesta versão, sempre o topo da pilha.

```bash
agentry --undo
```

```
> /undo
'src/main.rs' restaurado ao conteúdo anterior
```

**Importante — só `fs_write`/`fs_edit` são desfazíveis.** Mudanças feitas por `shell_exec`/
`shell_background` (ex.: um comando que sobrescreve um arquivo) **não** geram checkpoint e
**não** podem ser desfeitas pelo `agentry` — o efeito de um comando de shell não é
determinável de antemão da mesma forma que uma escrita de arquivo pela própria tool. Não
assuma que "existe *undo*" significa "toda mudança é reversível".

Checkpoints persistem em `.agentry/checkpoints.json` (mesmo diretório de estado local que
guarda índices e configuração — auto-excluído do git por padrão), então `--undo` desfaz o
mais recente de **qualquer** invocação anterior, não só da sessão atual. Um teto fixo (não
configurável nesta versão) limita quantos checkpoints ficam retidos — o mais antigo é
descartado silenciosamente ao ultrapassar.

## Memória de projeto (`/remember`)

`--remember <fato>` (*one-shot*) e `/remember <fato>` (REPL) gravam um fato pontual em
`.agentry/memory.json` — disponível no *system prompt* de **toda sessão futura** desse
projeto, sem precisar repetir a informação a cada conversa.

```bash
agentry --remember "o endpoint de staging é https://staging.exemplo.internal"
```

```
> /remember o time prefere PRs pequenos, um commit por ticket
lembrado: o time prefere PRs pequenos, um commit por ticket
```

**Sempre um ato explícito seu — nunca uma decisão do agente.** Diferente de outras
ferramentas, **não existe** uma tool que o agente possa chamar sozinho para gravar memória:
um fato só entra em `.agentry/memory.json` porque você digitou o comando. Isso é deliberado —
memória de projeto nesta versão não tenta resumir conversas automaticamente, só registra o
que você decide explicitamente que vale lembrar.

Memória persiste em `.agentry/memory.json` (mesmo diretório de estado local auto-excluído do
git que guarda checkpoints/índices/configuração) como uma lista simples de fatos, sem teto de
entradas. **Não existe `/forget` nesta versão** — para remover um fato, edite o arquivo
diretamente (é só uma lista de texto).

## O que esperar da resposta

O agente pode, no meio de uma tarefa, decidir chamar ferramentas (ler arquivo, editar
arquivo, buscar no código, rodar comando de shell). Tools na lista `ask` de
[`permissions`](configuracao.md#permissions) pedem confirmação interativa antes de rodar;
tools em `deny` nunca rodam. Ver [Guardrails de conteúdo](guardrails.md) para o mecanismo
separado que filtra o **conteúdo** das mensagens (independente de qual tool é chamada).

## Ferramentas do agente

Nenhuma das tools abaixo muda flags/comandos de invocação — o agente decide sozinho, no meio
de uma tarefa, quando usar cada uma; o comportamento observável é só a interação que ela
gera:

- **`ask_user`** — o agente pode perguntar algo diretamente a você, no meio de uma tarefa
  (esclarecer uma ambiguidade, confirmar uma decisão). A pergunta (e sugestões, se houver)
  aparece no terminal; sua resposta em texto livre volta para o agente. Diferente de uma tool
  em `ask` (que pede para *aprovar a execução* de outra tool), aqui é o próprio agente pedindo
  uma informação.
- **`glob`** — busca arquivos por padrão de nome/caminho (`"**/*.rs"`), sem olhar conteúdo.
  Respeita [`.agentryignore`](configuracao.md#arquivo-de-ignore-do-agentry-agentryignore) como
  qualquer tool de arquivo.
- **`shell_background`** — roda um comando de shell sem bloquear o agente (ex.: um `dev
  server`), consulta a saída depois e finaliza quando quiser. Sob a **mesma** política de
  `permissions`/comando bloqueado por padrão do `shell_exec` — rodar em segundo plano não é
  um jeito de contornar essa política.
- **`web_fetch`**/**`web_search`** — acesso à web, **desligados por padrão**; ver
  [`tools.webFetch`](configuracao.md#toolswebfetch)/[`tools.webSearch`](configuracao.md#toolswebsearch)
  para como habilitar e o [Modelo de privacidade e
  egresso](../governanca/privacidade-e-egresso.md) para o que cada um implica.
- **`subagent`** — o agente pode delegar uma subtarefa a uma sessão interna (um "subagente"),
  que roda até completar e devolve só a resposta final — sem aparecer incrementalmente na
  conversa principal, como uma resposta normal apareceria. Um subagente **nunca pode criar
  outro subagente** (sem aninhamento). A classe de egresso do subagente **nunca é mais
  permissiva** que a da sessão principal — reaproveita a mesma configuração de
  *providers*/*task-classes* resolvida no início da CLI (ver [Subagentes e
  egresso](../governanca/privacidade-e-egresso.md#subagentes-e-egresso) para o detalhe).

Além das tools acima, cada servidor MCP declarado em
[`mcpServers`](configuracao.md#mcpservers) adiciona suas próprias tools dinamicamente, uma
por tool exposta pelo servidor — nome sempre prefixado pelo servidor
(`"<servidor>__<tool>"`), sob a mesma disciplina `ask`/`deny` de qualquer tool acima. Nenhuma
vem habilitada por padrão (`mcpServers` é vazio até você declarar um).
