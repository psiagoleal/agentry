<!-- Caminho relativo: docs/adr/0020-agentryignore-com-respeito-opcional-a-gitignore.md -->

# ADR 0020: Arquivo `.agentryignore` (renomeando `.claudeignore`) com respeito opcional a `.gitignore`

- **Status:** Accepted
- **Data:** 2026-07-14
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** contexto, privacidade, interop, configuração

## Contexto

O `agentry` já filtra o que três tools (`fs` — leitura/escrita/edição/busca —, `repo_map`,
`code_search`) enxergam via um arquivo `.claudeignore` na raiz do projeto (sintaxe
gitignore, via crate `ignore`) — deliberadamente **independente** de `.gitignore`: um
arquivo pode estar versionado e fora do contexto do agente (`.claudeignore` cobre), ou fora
do versionamento e ainda assim visível ao agente (comportamento *default* hoje, já que
`.gitignore` não é olhado). Esse mecanismo foi herdado do contrato de interop v1
(ADR-0003) como um dos artefatos consumidos do `ai-coding-agent-profiles` — os três perfis
daquele repositório (`empresa`/`externo-confidencial`/`pessoal`) de fato distribuem seu
próprio `.claudeignore`, referenciado no `docs/interop/SPEC.md` canônico, no
`scripts/setup-profile.sh` e na skill `secrets-guard`.

Duas forças levaram a revisar isso agora:

1. **A premissa do nome está errada.** `.claudeignore` **não é um recurso real do Claude
   Code** — é um caso documentado de alucinação de IA que virou "documentação" e se
   espalhou pela internet (o próprio Claude explicou a existência de um recurso que nunca
   implementou). O Claude Code real só respeita `.gitignore`, e de forma imperfeita
   (`.env` listado em `.gitignore` ainda podia vazar para o console em versões
   documentadas). Manter o nome `.claudeignore` no `agentry` propaga essa premissa
   equivocada, mesmo que o mecanismo em si seja genuinamente útil.
2. **Falta o outro lado do problema: ruído de contexto.** `.claudeignore`/`.agentryignore`
   resolve confidencialidade (esconder algo do agente independente do versionamento), mas
   não resolve o desperdício de janela de contexto com artefatos já listados em
   `.gitignore` (builds, `node_modules`, etc.) sem duplicar cada padrão manualmente.
   O projeto de referência `OpenCode` (`github.com/anomalyco/opencode`) resolve os dois
   lados de forma real e verificável: respeita `.gitignore` por padrão nas tools de
   busca/listagem, **e** tem um arquivo próprio nativo, `.opencodeignore`, para exclusões
   específicas do agente — precedente direto para a mesma estratégia de duas camadas aqui.

## Decisão

Fica acordado:

1. **Novo nome canônico: `.agentryignore`** — mesma raiz do projeto, mesma sintaxe
   gitignore, mesma técnica de implementação (crate `ignore`, já usada). Deixa de ser
   artefato consumido do contrato de interop v1 (ADR-0003 é emendada — ver abaixo) e passa
   a ser um artefato **próprio do `agentry`**, mesmo padrão de posse do `.agentry/`
   (ADR-0017).
2. **Fallback de compatibilidade:** se `.agentryignore` estiver ausente, as tools continuam
   lendo `.claudeignore` (sem erro, comportamento atual preservado). Se **os dois**
   arquivos existirem, `.agentryignore` vence **sozinho** — nunca um merge dos dois; a
   presença de `.agentryignore` é o sinal explícito de migração completa.
3. **Nova opção configurável, `context.gitignore.enabled`** (bloco `context.*` do schema
   mínimo, ADR-0018, mesmo padrão `FeatureToggle` de `repoMap`/`semanticRag`/`lspGrounding`)
   — *default* `false`. Quando `true`, as tools passam a excluir **também** o que
   `.gitignore` cobre — em **união** com `.agentryignore`/`.claudeignore`, nunca em
   substituição. Nunca ligado por padrão: reduzir ruído de contexto é opt-in, não uma
   mudança silenciosa de comportamento para quem já depende do *default* atual (agente vê
   tudo que não estiver em `.agentryignore`/`.claudeignore`, gitignored ou não).
4. **Fidelidade assimétrica, aceita e documentada:** `repo_map`/`code_search` (que já
   percorrem a árvore via `WalkBuilder`) ganham suporte **completo** a `.gitignore`
   aninhado por subdiretório — comportamento nativo da crate `ignore` ao ligar seu filtro
   padrão de git. As tools de `fs` (resolução de caminho único via `GitignoreBuilder`, não
   um passeio de árvore) só enxergam o `.gitignore` da **raiz** — mesma limitação que já
   existe hoje para `.agentryignore`/`.claudeignore` nessas tools; não é uma regressão nova,
   é a mesma assimetria pré-existente estendida ao `.gitignore`.
5. **Escopo desta ADR é só o lado `agentry`.** O `ai-coding-agent-profiles` continua
   distribuindo `.claudeignore` em seus três perfis sem nenhuma mudança nesta rodada — o
   fallback do item 2 garante que nada quebra para quem já usa um perfil daquele
   repositório. Migração do lado `profiles` (renomear os três arquivos, atualizar
   `SPEC.md`/`setup-profile.sh`/`secrets-guard`) é item futuro, tratado numa sessão própria
   naquele repositório, registrado no `exchange-log` quando acontecer — decisão deliberada
   de não bloquear o lado `agentry` numa migração coordenada de dois repositórios de uma vez.
6. **ADR-0003 é emendada:** a lista de artefatos consumidos do `profiles` deixa de incluir
   `.claudeignore` como artefato de primeira classe do contrato de interop v1 — vira só o
   alvo do fallback de compatibilidade do item 2, com uma nota apontando para esta ADR.

## Consequências

- **Impacto positivo:** identidade própria do `agentry`, sem depender de uma convenção
  mal-atribuída ao Claude Code; resolve o lado de ruído de contexto sem duplicar padrões
  (opt-in via `context.gitignore.enabled`); migração sem quebra para quem já tem
  `.claudeignore` num projeto ou vem de um perfil do `profiles`.
- **Impacto negativo:** dois nomes de arquivo coexistindo por tempo indeterminado (até o
  lado `profiles` migrar e/ou usuários migrarem projetos existentes); assimetria de
  fidelidade de `.gitignore` aninhado entre os dois grupos de tools; os dois repositórios
  ficam temporariamente desalinhados quanto ao nome do artefato (mitigado pelo fallback,
  mas é dívida técnica real, registrada aqui e a resolver do lado `profiles` depois).
- **Trade-offs aceitos:** aceitar divergência temporária entre os dois repositórios em vez
  de bloquear a decisão do lado `agentry` numa migração coordenada de uma vez só; aceitar
  fidelidade parcial de `.gitignore` aninhado nas tools de `fs` em vez de reescrevê-las como
  passeio de árvore (fora de escopo — mudaria a natureza dessas tools).

## Diretriz de Conformidade de Código

- **Proibido:** qualquer tool nova de leitura/busca/listagem de arquivo ignorar
  `.agentryignore`/o fallback `.claudeignore` sem justificativa registrada em ADR; fazer
  merge implícito de `.agentryignore` e `.claudeignore` quando os dois existirem no mesmo
  projeto (precedência é exclusiva — `.agentryignore` vence sozinho); ligar
  `context.gitignore.enabled` por padrão em qualquer camada de configuração sem que o
  usuário tenha optado explicitamente.
- **Obrigatório:** `.agentryignore` é sempre verificado primeiro; fallback para
  `.claudeignore` só quando `.agentryignore` está ausente; toda tool que hoje reconhece
  `.claudeignore` reconhece `.agentryignore` da mesma forma, sem exceção seletiva por tool.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
