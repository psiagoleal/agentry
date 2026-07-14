<!-- Caminho relativo: docs/usuario/uso.md -->

# Uso da CLI e do REPL

## Modo one-shot

```bash
agentry "liste os arquivos .rs deste projeto"
```

Roda uma única tarefa (com o loop interno de tool-calls até chegar numa resposta final) e
sai. A resposta é exibida incrementalmente (*streaming*) conforme o modelo gera texto.

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

## Flags de invocação (one-shot)

| Flag | Efeito |
|---|---|
| `-m, --model <nome>` | Modelo a usar nesta invocação (sobrescreve o *default*). |
| `--temperature <n>` | Temperatura de amostragem. |
| `--top-p <n>` | *Top-p* (*nucleus sampling*). |
| `--max-tokens <n>` | Limite de tokens de saída. |
| `--system <texto>` | *System prompt* desta invocação. |
| `--reasoning on\|off` | Raciocínio estendido, se o modelo suportar. |
| `--ollama-host <host:porta>` | Servidor Ollama a usar (*default*: `127.0.0.1:11434`). |
| `--init` | Cria `.agentry/agentry.settings.json` e sai (ver [Configuração](configuracao.md)). |
| `--profile <nome>` | Com `--init`: busca a configuração real daquele perfil. |

```bash
agentry --model llama3.1:70b --temperature 0.2 "revise este diff"
agentry --ollama-host 127.0.0.1:11435 "..."   # outra porta/instância do Ollama
```

## Comandos de barra (REPL)

Equivalentes interativos das flags acima — o valor passa a valer para as mensagens
seguintes, até ser trocado de novo:

| Comando | Efeito |
|---|---|
| `/model <nome>` | Troca de modelo a partir da próxima mensagem. |
| `/temperature <n>` | Ajusta a temperatura. |
| `/top_p <n>` (ou `/top-p`) | Ajusta o *top-p*. |
| `/max_tokens <n>` (ou `/max-tokens`) | Ajusta o limite de tokens de saída. |
| `/system <texto>` | Atualiza o *system prompt* a partir da próxima mensagem. |
| `/reasoning on\|off` | Liga/desliga raciocínio estendido. |
| `/compact` | Resume o histórico da sessão numa única mensagem — reduz o consumo de tokens em conversas longas. |
| `/init` (ou `/init <perfil>`) | Cria `.agentry/agentry.settings.json` sem sair do REPL. |
| `/exit` (ou `/quit`) | Encerra o REPL. |

Qualquer outra linha é tratada como mensagem de usuário.

## O que esperar da resposta

O agente pode, no meio de uma tarefa, decidir chamar ferramentas (ler arquivo, editar
arquivo, buscar no código, rodar comando de shell). Tools na lista `ask` de
[`permissions`](configuracao.md#permissions) pedem confirmação interativa antes de rodar;
tools em `deny` nunca rodam. Ver [Guardrails de conteúdo](guardrails.md) para o mecanismo
separado que filtra o **conteúdo** das mensagens (independente de qual tool é chamada).
