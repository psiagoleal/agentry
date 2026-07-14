<!-- Caminho relativo: docs/usuario/faq.md -->

# Perguntas frequentes

**Preciso de conta em algum serviço de nuvem para usar o `agentry`?**

Não, na v0.1. A CLI fala só com um servidor [Ollama](https://ollama.com/) local — nenhuma
tarefa sai da sua máquina. Adapters de nuvem existem na biblioteca, mas ainda não há flag
nem configuração para escolhê-los pela CLI.

**O `agentry` funciona sem internet?**

Sim, contanto que o Ollama esteja rodando localmente com o modelo já puxado. `--init
--profile <nome>` é a única operação que precisa de rede (busca a configuração do perfil no
repositório `ai-coding-agent-profiles`) — e é opcional; `--init` sem `--profile` não toca a
rede.

**Como troco de modelo no meio de uma conversa (REPL)?**

`/model <nome>` — vale a partir da próxima mensagem, sem perder o histórico da conversa
(ver [Uso da CLI e do REPL](uso.md)).

**A conversa fica muito longa e lenta — o que fazer?**

`/compact` no REPL resume o histórico inteiro numa única mensagem, reduzindo o consumo de
tokens dos turnos seguintes.

**Como impeço o agente de rodar comandos de shell?**

Por padrão, a tool de shell já vem bloqueada (nenhum padrão de comando liberado). Para
bloquear qualquer outra tool explicitamente, use `permissions.deny` no
`agentry.settings.json` — ver [Configuração](configuracao.md#permissions).

**Diferença entre `permissions` e `guardrails`?**

`permissions` decide **quais ferramentas** o agente pode executar (ler arquivo, rodar
shell...). `guardrails` filtra **conteúdo** de texto (bloqueia/mascara padrões nas
mensagens), independente de qual tool é chamada. Mecanismos distintos, sobre dimensões
diferentes — ver [Guardrails de conteúdo](guardrails.md).

**Onde fica o histórico/estado local do projeto?**

Em `.agentry/`, na raiz do projeto (mesmo diretório onde fica `agentry.settings.json`) —
adicionado automaticamente a um `.gitignore` próprio para não ser versionado por engano.

**Encontrei um bug ou tenho uma sugestão.**

Abra uma *issue* em [github.com/psiagoleal/agentry](https://github.com/psiagoleal/agentry).
