<!-- Caminho relativo: docs/governanca/privacidade-e-egresso.md -->

# Modelo de privacidade e egresso

Base técnica: [ADR-0001](../adr/0001-fundacao-camada-llm.md) e
[ADR-0002](../adr/0002-modelo-privacidade-egresso.md).

## Um único ponto de saída de rede

O código do `agentry` tem exatamente **um** módulo autorizado a abrir uma conexão de rede.
Todo o resto do código — ferramentas, sessão, roteamento de modelo — passa por ele; não há
caminho alternativo de acesso à rede em nenhuma outra parte do código.

Isso não é só uma convenção documentada: é verificado por um teste automatizado que lê o
próprio código-fonte do projeto e falha a compilação se qualquer outro módulo importar a
biblioteca de rede diretamente. Uma regressão nesse ponto quebra o processo de build, não
depende de revisão manual pegar.

## O que passa por esse ponto único, antes de qualquer conexão

1. **Allowlist** — decide se o host de destino é alcançável, dado a classe de egresso ativa
   da sessão. Uma tentativa fora da allowlist **aborta antes de abrir qualquer conexão TCP**
   — nenhum byte trafega.
2. **Log de auditoria** — toda tentativa, permitida ou bloqueada, gera uma entrada de
   auditoria (ver [Auditoria e rastreabilidade](auditoria.md)).

## Classes de egresso

| Classe | Significado |
|---|---|
| `local-only` | Egresso para nuvem **proibido**; só endpoints locais/aprovados explicitamente. |
| `cloud-opt-out` | Nuvem permitida só com opt-out de retenção de dados comprovado, sob allowlist. |
| `cloud-ok` | APIs de nuvem livres, dentro de bom senso de custo. |

A classe ativa numa sessão vem do **perfil** configurado (`empresa` /
`externo-confidencial` / `pessoal`), mapeado pela política do projeto irmão
[`ai-coding-agent-profiles`](https://github.com/psiagoleal/ai-coding-agent-profiles):
`empresa` → `local-only`; `externo-confidencial` → `cloud-opt-out`; `pessoal` → `cloud-ok`.
O `agentry` **impõe** esse mapa; não o define — a política vive no repositório de política,
o `agentry` é a camada de execução.

## Fail-closed: o que acontece sem configuração clara

Perfil ausente, desconhecido, ou com grafia inesperada resolve **sempre** para a classe mais
restritiva (`local-only`) — nunca para a mais permissiva. Configuração incompleta ou
ambígua nunca é interpretada como "liberar por padrão".

## O que isso significa na prática, hoje (v0.1)

A CLI distribuída hoje só tem um provedor de modelo **efetivamente conectável**: um servidor
[Ollama](https://ollama.com/) rodando **localmente**, na mesma máquina ou rede que você
controla. Adapters para provedores de nuvem (compatíveis com a API da OpenAI, e um adapter
nativo para a API da Anthropic) já existem como código na biblioteca — mas não há, ainda,
caminho de configuração pela CLI para ativá-los. Ou seja: **nenhum código-fonte, prompt ou
resposta sai da máquina onde o `agentry` roda**, independentemente da classe de egresso
configurada, porque não há hoje nenhum destino de rede, além do Ollama local, para o qual
esse tipo de conteúdo possa ser enviado. O modelo de classes de egresso descrito acima é a
base arquitetural que já existe em código para quando adapters de nuvem forem conectados à
CLI — vale a pena entender antes desse momento chegar, não depois.

**Exceção, fora do escopo de conteúdo de tarefa:** o comando `--init --profile <nome>` (ou
`/init <perfil>` no REPL) contata a rede para buscar um artefato de configuração público do
repositório de perfis — nunca conteúdo de código, prompt ou resposta, só o arquivo de
política a aplicar localmente. É a única operação de rede além do Ollama local disponível
hoje na CLI, e é opcional (`--init` sem `--profile` não toca a rede).

## O que audita e o que não sabe

O `agentry` audita **tentativas de rede** (host, permitida ou não) — não decide sozinho o
que um provedor de nuvem já conectado faz com o conteúdo depois de recebê-lo (retenção,
treinamento, etc.). Essa é uma responsabilidade do contrato com cada provedor, fora do
controle técnico do `agentry` — a classe `cloud-opt-out` existe justamente para expressar
"só me conecte a provedores com opt-out de retenção comprovado", mas comprovar esse opt-out
por provedor é um processo de avaliação de fornecedor, não algo que o software verifica
sozinho.
