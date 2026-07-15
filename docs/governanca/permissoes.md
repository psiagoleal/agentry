<!-- Caminho relativo: docs/governanca/permissoes.md -->

# Controle de permissões de ferramentas

O agente pode pedir para executar **ferramentas** sobre o sistema onde roda — ler/escrever
arquivos, buscar no código, rodar comandos de shell. Esse controle é independente do modelo
de privacidade/egresso (que decide para onde dados podem trafegar) e do sistema de
guardrails (que filtra conteúdo de texto) — aqui a pergunta é *"o agente pode executar esta
ação sobre este sistema?"*.

## Três decisões possíveis, por nome de ferramenta

- **Permitir** — executa sem pedir confirmação.
- **Perguntar** — pede confirmação explícita ao operador antes de executar.
- **Negar** — nunca executa, sob nenhuma circunstância.

A decisão é configurável por lista (`permissions.deny`/`permissions.ask` no arquivo de
configuração do projeto) — ver [Configuração](../usuario/configuracao.md#permissions) na
trilha de usuário para a sintaxe exata.

## Precedência: negar sempre vence

Se um mesmo nome aparecer, por erro de configuração, tanto em `deny` quanto em `ask`, a
decisão é sempre a mais restritiva (nega). Entre camadas de configuração (arquivo →
ambiente), as listas só **crescem** — uma restrição herdada de uma camada nunca é removida
por uma camada mais específica. Não existe caminho de configuração para "afrouxar" uma
negação herdada.

## Default: permissivo por nome, exceto o shell

Um nome de ferramenta fora das duas listas roda sem pedir confirmação, por padrão — o
mecanismo genérico é "lista de exceções sobre um padrão permissivo", não uma allowlist
fechada por padrão. **Exceção deliberada:** a ferramenta de execução de shell, na CLI de
referência distribuída, é conectada sem nenhum padrão de comando pré-liberado — na prática,
shell fica **bloqueado por padrão**, mesmo sem nenhuma entrada explícita em `deny`. Isso é
uma decisão de postura conservadora tomada no ponto de integração (a CLI), não uma
propriedade do mecanismo genérico de permissões em si — vale a pena confirmar essa
configuração ao avaliar uma versão futura ou uma integração customizada.

## Granularidade por conteúdo: `.agentryignore`

Permissões decidem **se** uma ferramenta roda, não *sobre qual conteúdo* — controlar "o
agente pode usar `fs_read`" é diferente de controlar "o agente pode ler *este* arquivo".
Essa segunda camada existe, mas é um mecanismo **separado**: um arquivo `.agentryignore` na
raiz do projeto (sintaxe `.gitignore`) — arquivos/diretórios listados ali ficam
inacessíveis às tools de sistema de arquivos e busca, **independente** de estarem
versionados no Git ou não. Nome legado `.claudeignore` continua funcionando como
*fallback* de compatibilidade. Ver [Configuração — Arquivo de ignore do
`agentry`](../usuario/configuracao.md#arquivo-de-ignore-do-agentry-agentryignore) para a
sintaxe.

Não confundir com `context.gitignore.enabled` (também documentado ali): esse segundo é só
uma otimização de ruído de contexto (evita reprocessar artefatos de build já cobertos por
`.gitignore`), *opt-in*, **sem nenhum efeito de confidencialidade** — um arquivo fora do
`.agentryignore` continua acessível ao agente esteja `context.gitignore.enabled` ligado ou
não. Quem precisa esconder algo do agente usa `.agentryignore`, nunca depende de
`.gitignore`/`context.gitignore.enabled` para isso.

## O que nenhum dos dois controles cobre

Nem permissões nem `.agentryignore` substituem o controle de rede: uma ferramenta local
(leitura de arquivo, por exemplo) não passa pelo módulo de transporte nem pela allowlist —
ver [Modelo de privacidade e egresso](privacidade-e-egresso.md) para o que efetivamente
controla saída de dados da máquina.
