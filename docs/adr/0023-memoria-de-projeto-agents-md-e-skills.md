<!-- Caminho relativo: docs/adr/0023-memoria-de-projeto-agents-md-e-skills.md -->

# ADR 0023: Memória de projeto — leitura de `AGENTS.md`/`CLAUDE.md` + *progressive disclosure* de `SKILL.md`

- **Status:** Proposed
- **Data:** 2026-07-15
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** contexto, memória de projeto, tools, configuração

## Contexto

O `agentry` hoje **não lê nenhuma instrução de projeto**: cada sessão começa só com o
`system_prompt` do preset de `task-class` (ADR-0008/0021), se houver algum configurado. Isso
deixa o agente sem memória do que já foi decidido sobre convenções, arquitetura ou
restrições do repositório em que está rodando — exatamente o papel que `CLAUDE.md` cumpre no
Claude Code CLI e que este próprio repositório já usa na prática: `AGENTS.md`
(`/home/psiagoleal/dev/agentry/AGENTS.md`) é a fonte única de verdade do projeto, e
`CLAUDE.md` é deliberadamente um arquivo-ponteiro de uma linha ("leia `AGENTS.md`, ele
prevalece") — o padrão real de convenção multi-agente
([agents.md](https://agents.md)), não específico do Claude.

Isso mantém em aberto a **ADR-0003** (`Proposed` desde o MT-04): "Consumo dos artefatos de
política do `ai-coding-agent-profiles`" previa que o `agentry` consumisse instruções de
projeto, mas nunca foi implementado — o schema de config (ADR-0018/0021) cobriu parâmetros
estruturados (permissões, rotas, guardrails), nunca texto livre de convenções.

Também não existe nenhum mecanismo de **skills** — capacidades sob demanda, carregadas só
quando o modelo decide usá-las (*progressive disclosure*: `name`+`description` sempre no
contexto, o corpo completo só quando acionado). Este próprio repositório já usa a convenção
`.claude/skills/<nome>/SKILL.md` (frontmatter YAML com `name`/`description`, corpo em
Markdown) — ver `.claude/skills/adr-writer/SKILL.md` como exemplo real. Reaproveitar essa
convenção **verbatim** (em vez de inventar um formato próprio do `agentry`) dá compatibilidade
imediata com qualquer projeto que já tenha skills do Claude Code definidas — sem migração,
sem duplicação de arquivos — e é o que mais aproxima o `agentry` do Claude Code CLI/OpenCode,
o objetivo explícito do roadmap de longo prazo (`docs/roadmap-longo-prazo.md`).

Uma decisão é necessária agora porque a Fase 13 é a próxima do roadmap-mestre e nenhuma das
duas capacidades (leitura de instruções, skills) tem hoje um desenho concreto — só o esboço
de uma linha em `docs/roadmap-longo-prazo.md`.

## Decisão

### 1. Leitura de instruções de projeto (`AGENTS.md`/`CLAUDE.md`)

Fica acordada a leitura automática de um arquivo de instruções na raiz do projeto
(`workspace_root`, já resolvido por `state_dir::resolve_root`, MT-38), com a **mesma
disciplina de precedência sem merge** já usada por `.agentryignore`/`.claudeignore`
(ADR-0020): `AGENTS.md` é o **primário**; na ausência dele, `CLAUDE.md` é lido como
*fallback*; se nenhum dos dois existir, nenhuma mensagem é inserida (comportamento atual
preservado). Os dois nunca são combinados — evita um arquivo pisar silenciosamente no
conteúdo do outro.

O conteúdo lido é injetado como parte da **mensagem de sistema** da sessão, concatenado
**antes** do `system_prompt` do preset da `task-class` ativa (instruções de projeto primeiro,
por serem mais gerais; o preset da tarefa depois, por ser mais específico) — mesma mensagem
única já gerenciada por `Session::ensure_system_prompt` (MT-14), sem introduzir um segundo
slot de mensagem de sistema.

**Confidencialidade:** o carregador consulta `.agentryignore`/`.claudeignore` (o mesmo
mecanismo do ADR-0020) antes de ler `AGENTS.md`/`CLAUDE.md` — se o arquivo estiver coberto
pelo ignore do projeto, ele é pulado silenciosamente, exatamente como qualquer outro arquivo.
Não existe um segundo controle de confidencialidade paralelo: quem já usa `.agentryignore`
para esconder algo do agente continua com uma única fonte de verdade.

**Configurável:** novo bloco `context.agentsFile.enabled` (mesmo tipo `FeatureToggle` já
usado por `repoMap`/`semanticRag`/`lspGrounding`/`gitignore`, ADR-0018/0020) — **`true` por
padrão** (mesma categoria de custo baixo/benefício alto das três primeiras flags de
`context.*`: leitura local de um arquivo pequeno, sem chamada de rede nem indexação).

### 2. Descoberta de skills (`SKILL.md`) + *progressive disclosure*

Fica acordada a descoberta de skills em `<workspace_root>/.claude/skills/<nome>/SKILL.md` —
**um nível** de subdiretórios, sem recursão, **reaproveitando literalmente** a convenção do
Claude Code (não um formato próprio do `agentry`), pelos motivos de compatibilidade descritos
no Contexto. Formas mais avançadas do harness do Claude Code (namespace `plugin:skill`,
descoberta em múltiplos diretórios) ficam **fora de escopo** desta ADR — YAGNI até haver
demanda real.

Cada `SKILL.md` descoberto vira um `SkillDescriptor { name, description, path }`, extraído do
frontmatter YAML entre delimitadores `---`. **Parser de frontmatter próprio, sem dependência
de YAML nova** — decisão explícita, registrada como decisão-sob-dúvida (ver
`docs/decisoes-autonomas.md`): o schema é fixo e conhecido (só duas chaves de string,
`name`/`description`, incluindo o estilo de bloco dobrado `>-` usado nos `SKILL.md` reais
deste repositório), e um parser geral de YAML traria uma superfície de API/manutenção muito
maior do que o problema exige — mesmo espírito de MT-06 (redação de segredos sem regex) e do
casamento de guardrail por substring (ADR-0007), que evitam dependência nova para um problema
estreito e bem definido. Path coberto por `.agentryignore` é pulado, mesma disciplina da
seção 1.

**Lista compacta sempre no contexto:** `name` + `description` de cada skill descoberta
entram na mesma mensagem de sistema da seção 1 (concatenados por último), formatados como uma
lista simples — custo desprezível de tokens, mesmo padrão de "sempre visível" do Claude Code.

**Corpo completo só sob demanda:** o corpo de um `SKILL.md` (tudo após o frontmatter) só é
lido quando o **modelo** decide usá-lo, via uma nova tool `skill` (`SkillTool`,
`crates/core/src/tools/skill.rs`) — reaproveita a `trait Tool`/`ToolRegistry` já existentes
(ADR/MT-11) **sem** nenhum mecanismo novo de acionamento: é uma tool-call comum, sob o mesmo
`PermissionGate` de qualquer outra tool (sem tratamento especial de *default-deny*, já que é
leitura local sem efeito colateral, diferente da tool de shell).

## Consequências

- **Impacto positivo:** o agente ganha memória de convenções/arquitetura do projeto sem
  nenhuma configuração adicional (zero-config: se `AGENTS.md`/`CLAUDE.md` existir, já
  funciona); reaproveitar `.claude/skills/` dá compatibilidade imediata com qualquer projeto
  que já tenha skills do Claude Code, incluindo o próprio `agentry`; fecha a **ADR-0003**
  (`Proposed` desde o MT-04); nenhuma dependência nova.
- **Impacto negativo:** toda chamada de chat passa a incluir potencialmente mais tokens no
  *system prompt* (conteúdo de `AGENTS.md` + lista de skills) — mitigado por ser *opt-out*
  via `context.agentsFile.enabled` e por a lista de skills ser só nome+descrição, não o corpo
  inteiro; parser de frontmatter próprio não cobre YAML arbitrário (só o subconjunto usado
  pelos `SKILL.md` reais) — se um projeto usar uma sintaxe de frontmatter mais exótica
  (listas, mapas aninhados, âncoras), a descoberta daquela skill falha de forma tratada (nunca
  *panic*), não silenciosa.
- **Trade-offs aceitos:** compatibilidade total com a convenção do Claude Code em troca de
  não inventar um formato "mais correto" ou mais rico para o `agentry`; parser mínimo em troca
  de zero dependência nova, aceitando que frontmatter fora do subconjunto suportado não
  funcione (tratado, não silencioso).

## Diretriz de Conformidade de Código

- **Proibido:** combinar (fazer merge de) `AGENTS.md` e `CLAUDE.md` — só um dos dois é lido,
  nunca os dois; ignorar `.agentryignore`/`.claudeignore` ao carregar instruções de projeto ou
  ao descobrir skills — é o mesmo controle de confidencialidade do ADR-0020, sem exceção para
  este novo caminho de leitura; introduzir uma dependência de parser YAML genérico para o
  frontmatter de `SKILL.md` sem uma nova ADR que reavalie esta decisão; dar à tool `skill`
  qualquer efeito colateral além de leitura local (sem rede, sem escrita).
- **Obrigatório:** ausência de `AGENTS.md`/`CLAUDE.md`/`.claude/skills/` preserva o
  comportamento atual (nenhuma mensagem de sistema extra, nenhuma tool `skill` com skills
  vazias — a tool pode continuar registrada, só sem nada para carregar); toda falha de
  descoberta/parse de um `SKILL.md` individual é tratada (nunca derruba a sessão nem impede a
  descoberta dos demais); `context.agentsFile.enabled` controla exclusivamente a leitura de
  `AGENTS.md`/`CLAUDE.md` — nunca a descoberta de skills, que não tem *opt-out* próprio nesta
  ADR (mesmo espírito de custo desprezível das três primeiras flags de `context.*`).

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
