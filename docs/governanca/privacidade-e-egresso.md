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

## O que isso significa na prática, hoje

**Por padrão, sem nenhuma configuração adicional**, a CLI só fala com um servidor
[Ollama](https://ollama.com/) rodando **localmente**, na mesma máquina ou rede que você
controla — nenhum código-fonte, prompt ou resposta sai da máquina, porque não há nenhum
outro destino de rede configurado.

**Um segundo provider já é conectável de fato, de forma opcional:** um gateway
[LiteLLM](https://www.litellm.ai/) (`providers.litellm` no `agentry.settings.json` — ver
[Configuração](../usuario/configuracao.md#providerslitellm)), comum em ambientes
corporativos na frente de modelos maiores/de nuvem. Duas coisas tornam isso seguro por
design, não por convenção:

- **A classe de egresso desse endpoint é sempre explícita**, nunca inferida do host — um
  gateway hospedado "localmente" (na rede da empresa) não é automaticamente tratado como
  `local-only` só por isso; ele pode encaminhar para *backends* de nuvem por trás, e tratá-lo
  como local por inferência degradaria a confidencialidade em silêncio. Se a classe não for
  declarada, o *default* é o mais restritivo para liberar (`cloud-ok` do ponto de vista de
  risco — ver a tabela de classes acima) — na prática, isso bloqueia o endpoint sob qualquer
  perfil que não seja explicitamente permissivo, até alguém declarar a classe de propósito.
- **Selecionar esse provider é sempre um ato explícito do operador** — via `--provider
  litellm`/`/provider litellm` (ver [Uso da CLI e do REPL](../usuario/uso.md)). Sem essa
  escolha, o Ollama local continua sendo o candidato preferencial; configurar
  `providers.litellm` no arquivo não muda, sozinho, para onde uma tarefa é enviada.

Um adapter nativo para a API da Anthropic também já existe como código na biblioteca, mas
sem nenhum caminho de configuração pela CLI para ativá-lo ainda — diferente do LiteLLM, que
já está conectado de ponta a ponta.

**Além disso, fora do escopo de conteúdo de tarefa:** o comando `--init --profile <nome>`
(ou `/init <perfil>` no REPL) contata a rede para buscar um artefato de configuração
público do repositório de perfis — nunca conteúdo de código, prompt ou resposta, só o
arquivo de política a aplicar localmente. É opcional (`--init` sem `--profile` não toca a
rede) e independente do LiteLLM.

## Egresso via ferramentas de web (`web_fetch`/`web_search`)

Duas tools dão ao agente acesso à web — a diferença entre elas importa para avaliar o
caminho de egresso de cada uma:

- **`web_fetch`** busca o conteúdo de **qualquer URL** que o agente decida pedir — o destino
  não é conhecido de antemão, então não cabe no modelo de allowlist por host único usado por
  todo outro provider/endpoint deste documento. Por isso, essa tool só é registrada quando
  **duas** condições valem ao mesmo tempo: (1) `tools.webFetch.enabled: true`, um *opt-in*
  explícito no arquivo de configuração — desligado por padrão; **e** (2) a sessão já resolve
  para a classe de egresso mais permissiva (`cloud-ok`). Sob `local-only`/`cloud-opt-out`, a
  tool nem aparece para o agente, mesmo que `tools.webFetch.enabled` esteja ligado — ela exige
  a combinação das duas, nunca uma sozinha. Isso é uma restrição de código, verificada por
  teste automatizado, não uma recomendação de uso.
- **`web_search`** consulta uma instância [SearXNG](https://docs.searxng.org/) — **um host
  único e conhecido**, informado por você (`tools.webSearch.searxngUrl`; ver
  [Configuração](../usuario/configuracao.md#toolswebsearch)). Cabe no mesmo modelo de
  allowlist do LiteLLM: classe de egresso sempre explícita (ausente ⇒ `cloud-ok`, o mais
  restritivo para liberar), e a tool só é registrada quando a URL é de fato configurada —
  **nenhuma instância pública vem pré-configurada** no `agentry`, por dois motivos: risco de
  disponibilidade (a instância pode sair do ar) e de cadeia de suprimentos (uma instância de
  terceiros poderia registrar ou manipular consultas).

**Modelo de anonimato, como requisito de código, não de melhor esforço:** as duas tools
nunca enviam cookies (a biblioteca de rede do `agentry` não tem *cookie jar* — não há nada
para desligar, a capacidade simplesmente não existe no binário), sempre usam um
*User-Agent* genérico fixo (nunca identifica a máquina, o usuário ou a versão do SO), e
nunca anexam cabeçalho `Referer` nem qualquer parâmetro de rastreio próprio às consultas.
Reduzir rastreabilidade nessas duas tools foi tratado como requisito desde o desenho, não
como ajuste posterior.

Com essas duas, os caminhos de rede disponíveis hoje na CLI são: Ollama local (sempre),
LiteLLM (opcional, um segundo provider de modelo), busca do artefato de `--init --profile`
(opcional, só configuração), e as duas tools de web acima (opcionais, cada uma com seu
próprio *opt-in*).

## O que audita e o que não sabe

O `agentry` audita **tentativas de rede** (host, permitida ou não) — não decide sozinho o
que um provedor de nuvem já conectado faz com o conteúdo depois de recebê-lo (retenção,
treinamento, etc.). Essa é uma responsabilidade do contrato com cada provedor, fora do
controle técnico do `agentry` — a classe `cloud-opt-out` existe justamente para expressar
"só me conecte a provedores com opt-out de retenção comprovado", mas comprovar esse opt-out
por provedor é um processo de avaliação de fornecedor, não algo que o software verifica
sozinho.
