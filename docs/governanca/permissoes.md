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

## O que este controle não cobre

Permissões decidem **se** uma ferramenta roda. Não analisam o conteúdo dos argumentos com
que ela é chamada (ex.: *qual* arquivo é lido, *qual* comando de shell é pedido) — esse tipo
de granularidade fica para configuração futura ou integração customizada. Também não
substitui o controle de rede: uma ferramenta local (leitura de arquivo, por exemplo) não
passa pelo módulo de transporte nem pela allowlist — ver [Modelo de privacidade e
egresso](privacidade-e-egresso.md) para o que efetivamente controla saída de dados da
máquina.
