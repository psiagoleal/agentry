<!-- Caminho relativo: docs/analise-harness-engineering.md -->

# O `agentry` como *harness*: mapeamento e lacunas

> **Natureza deste documento:** não é ADR (não decide nada) nem roadmap (não sequencia
> trabalho). É uma avaliação do `agentry` contra o vocabulário corrente de *harness
> engineering*, feita para responder uma pergunta: **onde o projeto já está maduro e onde tem
> lacuna real?** As decisões que ele sugere estão registradas como tickets em
> [`roadmap-v0.18.md`](./roadmap-v0.18.md); as que exigem ADR estão marcadas como tal.

## O enquadramento

A ideia central é que **agente = modelo + harness**, e que o harness — não o modelo — é onde
uma equipe acumula vantagem: tools, contexto, permissões, verificação, observabilidade e
memória. O corolário prático, atribuído a Mitchell Hashimoto, é o que mais interessa aqui:

> Quando o agente repete um erro, a correção mais durável costuma estar **no ambiente ao redor
> dele** — não em mais uma advertência no prompt.

Isso separa dois tipos de controle que o `agentry` já trata de forma diferente, mas nunca
nomeou junto: **instrução é probabilística** (o modelo lê e decide seguir) e **fronteira é
determinística** (o runtime impede, independente do que o modelo concluiu). O projeto já
escolheu o lado determinístico nos pontos que importam — egresso, permissão, guardrail — e é
por isso que o mapeamento abaixo é majoritariamente favorável.

## Mapeamento: os componentes de um harness robusto

| Componente | No `agentry` hoje | Situação |
|---|---|---|
| **Ferramentas** — ações pequenas, previsíveis, com falha observável | `ToolRegistry` + tools nativas + MCP (ADR-0028) | maduro |
| **Contexto** — base fixa curta + conteúdo sob demanda | `repoMap`, RAG semântico, *grounding* por LSP, `session_search`; skills com *frontmatter* e corpo sob demanda (ADR-0023) | maduro |
| **Memória** — decisão útil sobrevive à sessão | `/remember` explícito (ADR-0032); `/save`+`/recall` | maduro |
| **Permissões** — política vira limite executável | `deny`/`ask`, `readAllow`, `subagentPermissions` (ADR-0041); `shell_exec` bloqueada por padrão | **lacuna parcial** |
| **Verificação** — o que conta como correto | guardrails de conteúdo (ADR-0007), detectores de PII (ADR-0043), Reviewer (ADR-0015) | **lacuna**: nenhum sensor determinístico (teste/lint/tipos) no ciclo |
| **Observabilidade** — decisões, tools, tentativas, motivo de parada | `audit.log` de **egresso** (ADR-0002) | **lacuna central** |
| **Atribuição de falha** — a falha veio do modelo, do contexto, da tool ou da política? | — | **inexistente** |

Duas leituras saltam da tabela. A primeira: o `agentry` é forte exatamente onde o modelo
público é mais vago — ele trata privacidade e egresso como eixo de primeira classe, o que
nenhum dos componentes acima cobre (ver "O que o modelo não cobre", no fim). A segunda: as
lacunas se concentram do lado **depois da ação** — o projeto sabe impedir, e não sabe contar o
que aconteceu.

## Lacunas, em ordem de retorno

### 1. O loop não sabe dizer que travou

`StopReason` tem hoje três valores: `Done`, `BudgetExceeded`, `MaxTurnsExceeded`. Falta o que a
literatura de *loop engineering* chama de **impasse**: o agente repete a mesma ação — ou o
mesmo par alternado — sem que o observado mude.

Isso não é preciosismo taxonômico. Um agente preso hoje queima os 25 turnos do teto (ADR-0033)
e reporta `MaxTurnsExceeded`, que **atribui a causa errada**: diz "orçamento" quando a verdade é
"travou no passo 3". Quem lê o relato conclui que o teto está baixo e o aumenta — piorando o
problema. Detecção conhecida e barata: *hash* de `tool + argumentos` repetido N vezes sem
mudança no resultado observado; alternância `A, B, A, B` conta como impasse igual.

→ **MT-159**.

### 2. Observabilidade cobre egresso e mais nada

O `audit.log` responde "o que saiu da máquina". Não responde "o que o agente fez": qual tool
foi chamada, qual permissão foi aplicada, quantas tentativas houve, por que o loop parou.

Esta é a lacuna que **absorve o MT-149 e o MT-155**, hoje soltos como pendências de
conformidade. Os dois são o mesmo sintoma — todo caminho *fail-closed* do projeto acerta o
efeito e não deixa rastro — e o enquadramento certo não é "a ADR-0002 exige?", é: *dá para
dizer se a falha veio do modelo, do contexto, da ferramenta ou da política?* Hoje não dá. Num
projeto cujo alvo declarado é homologação corporativa, a diferença entre "a política me
protegeu" e "consigo demonstrar que me protegeu" é o produto inteiro.

→ **MT-160** (e fecha MT-149 + MT-155 na mesma decisão).

### 3. Permissão de shell é tudo-ou-nada

`shell_exec` bloqueada por padrão é o lado certo de errar, mas quem precisa dela **abre por
inteiro**. Falta granularidade por padrão de comando, com a regra usual de precedência
(*wildcard* primeiro, exceções depois, última correspondência vence): `"*": "ask"` com
`"git push*": "deny"` é o caso canônico — o risco não pode depender de o modelo lembrar da
convenção.

→ **MT-161**.

### 4. Orçamento mede tokens e turnos, não tempo nem custo

Turnos e tokens o próprio loop controla. Tempo de parede e custo acumulado têm dono diferente —
exigem contador no orquestrador — e hoje simplesmente não existem. Uma sessão pode gastar meia
hora e uma fatura sem cruzar nenhum teto declarado.

→ **MT-162**.

### 5. Erro transitório e determinístico recebem o mesmo tratamento

*Rate limit*, *timeout* e 5xx podem funcionar na tentativa seguinte e merecem recuo
exponencial; argumento inválido, arquivo inexistente e permissão negada não melhoram com
repetição — repetir só gasta turno. Hoje os dois derrubam o turno da mesma forma. A regra
operacional é conhecida: *retry* é do transporte; falha de conteúdo volta como observação para
o modelo corrigir.

→ **MT-163**.

## O que **não** adotar sem decisão explícita

- **Hooks** (`PreToolUse`/`PostToolUse`/`Stop`) são o mecanismo mais poderoso do modelo — trocam
  instrução probabilística por automação garantida. Mas o `agentry` já tem duas camadas
  determinísticas **antes** da ação (permissões e guardrails); o que hooks acrescentariam de
  fato é o lado **depois** (rodar teste/lint sobre o que acabou de ser editado) e o **gate de
  saída** (só encerra se o critério de aceite passar). O ganho é real e o custo também: um hook
  é execução arbitrária declarada em arquivo de configuração, que é precisamente o tipo de
  capacidade que um agente mirando homologação corporativa não pode ganhar por impulso.
  **Exige ADR** — e a ADR precisa responder quem pode declarar hook e sob qual classe de
  egresso, não só qual o formato.
- **Grafo / *fan-out* em *worktrees***. O ganho é de **relógio, não de token** — cada fatia
  paga o próprio *prefill* —, e só compensa quando as fatias não compartilham contexto. O
  subagente de um nível (ADR-0031) é adequado ao tamanho atual do projeto. Reabrir quando
  houver caso concreto de onda com fatias verticais independentes.
- **SDD/TDD, *completion gates* e Gauntlet Loop** são disciplina de **processo de quem usa** o
  `agentry`, não features do binário. Boa parte já é praticada aqui (spec → teste que nasce
  falhando → verificação com comando de prova; achado vira ticket, não comentário solto).

## O que o modelo público não cobre — e o `agentry` sim

O vocabulário de harness trata permissão como *"o que a ferramenta pode fazer"*. O `agentry`
acrescenta um eixo que nenhum dos componentes canônicos nomeia: ***para onde o dado pode ir***
— classe de egresso por perfil, recusa de rota **antes** da chamada, bloqueio no transporte com
auditoria, e roteamento que permite ao modelo de nuvem planejar enquanto o modelo local lê o
que é confidencial (ADR-0041).

Vale registrar isso explicitamente por dois motivos. Primeiro, porque é o diferencial do
projeto e ele não aparece de graça em nenhuma comparação com ferramenta genérica. Segundo,
porque é o critério que decide as lacunas acima: cada uma delas foi priorizada por **quanto
reforça a capacidade de prestar contas**, que é a promessa que o `agentry` faz.
