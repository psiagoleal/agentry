<!-- Caminho relativo: docs/adr/0031-subagentes-com-egresso-restrito.md -->

# ADR 0031: Subagentes com classe de egresso restrita à sessão-mãe

- **Status:** Accepted
- **Data:** 2026-07-16
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** ferramentas, egresso, arquitetura

> **Nota de implementação (MT-91, 2026-07-16):** o texto original desta ADR (ver "Classe de
> egresso" abaixo) previa o subagente usando literalmente o **mesmo** objeto `Arc<Router>` da
> sessão-mãe. Na fiação real, isso esbarrou num obstáculo concreto: `Router` não implementa
> `Clone` e é **mutável em tempo de execução** (`/model`/`/task-class`), então compartilhar o
> objeto exigiria `Arc<Mutex<Router>>` tocando três pontos de entrada da CLI. Implementado, em
> vez disso, com **duas instâncias de `Router` construídas de forma idêntica** (mesmos
> providers, mesmas *task-classes* declaradas, mesma classe de egresso) a partir dos mesmos
> insumos — decisão registrada em `docs/decisoes-autonomas.md`. A garantia de egresso desta
> ADR continua **integralmente válida** (a classe de egresso nunca muda em tempo de execução,
> só as rotas declaradas podem); a única consequência é que o subagente não reflete uma troca
> de modelo/task-class feita **depois** que a CLI já inicializou.

## Contexto

A Fase 18 (checkpoints/*undo*) fechou inteira (MT-86..89, ADR-0030 `Accepted`). Restam três
frentes de "segunda onda" (`docs/roadmap-longo-prazo.md` §Fase 19+, à época) — subagentes/
orquestração, memória entre sessões, multimodal —, todas com uma pergunta de design/segurança
sem opção recomendada óbvia o bastante para decidir sozinho. O mantenedor foi consultado
diretamente (não uma decisão autônoma) e respondeu às três; esta ADR trata a de subagentes,
escolhida para vir primeiro (`docs/decisoes-autonomas.md`, 2026-07-16).

**Decisão-chave do mantenedor:** um subagente pode declarar sua própria classe de egresso,
mas só igual ou mais restrita que a da sessão-mãe — nunca mais permissiva (opção B das três
consideradas: herdar sempre / declarar mais restrita ou igual / classe independente).

O `agentry` já tem: um `ToolRegistry` (MT-11) com gate de permissão uniforme; um `Router`
(`crates/core/src/router/mod.rs`, ADR-0008) construído **uma vez por processo** com **uma**
`EgressClass` — o teto do perfil ativo (ADR-0002) — que `resolve()`/`resolve_with_override()`
já recusam qualquer candidato mais permissivo que esse teto, para **qualquer** chamador, sem
exceção; um `TokenBudget`/`Session::usage_total` (ADR-0029); `GuardrailGate` (ADR-0007),
todos já `Arc`-compartilháveis. Uma tool de subagente pode reaproveitar 100% dessa
infraestrutura, sem inventar um segundo modelo de egresso/permissão.

## Decisão

### Subagente = nova `Session` interna, mesma tool `subagent`

Nova tool `subagent` (`crates/core/src/tools/subagent.rs`) — o agente principal delega uma
subtarefa (`description`, texto livre) e, opcionalmente, uma `task_class` já declarada
(mesmo padrão de `--task-class`/`/task-class`, ADR-0021: nunca introduz um candidato não
vetado). A tool constrói uma `Session` nova, roda até completar (`Session::run`, **sem
streaming** — a resposta do subagente não aparece incrementalmente na conversa principal,
só o resultado final) e devolve o texto da resposta final como `ToolOutput`.

### Classe de egresso: o subagente usa o **mesmo** `Router` da sessão-mãe

A tool `subagent` guarda o **mesmo** `Arc<Router>` já construído para a sessão-mãe (não um
`Router` próprio, não uma cópia). Como `Router::resolve`/`resolve_with_override` já recusam
qualquer candidato mais permissivo que o teto de egresso do perfil ativo — para **qualquer**
chamador —, essa garantia vale automaticamente para o subagente, **sem nenhum código novo de
imposição**: ele estrutural e literalmente não consegue resolver um candidato que a própria
sessão-mãe não pudesse também resolver. Decisão de implementação registrada em
`docs/decisoes-autonomas.md` (2026-07-16): esta é a leitura de "classe da sessão-mãe" como
"o teto do perfil que a sessão-mãe já respeita" (garantida pelo `Router` compartilhado), não
"a rota especificamente ativa agora" (que exigiria rastrear estado mutável hoje inexistente
fora da CLI) — extensão futura clara se o mantenedor preferir a leitura mais estrita depois
de ver a implementação.

### Sem recursão: o executor do subagente não conhece a própria tool

Um subagente **nunca pode criar outro subagente**. Implementado estruturalmente: o
`Arc<dyn ToolExecutor>` injetado em `SubagentTool` vem de um `ToolRegistry` **que nunca
registra a própria tool `subagent`** — o modelo dentro do subagente nem enxerga essa tool
existir, muito menos consegue chamá-la. Preferido a um *flag*/contador em tempo de execução
(mesma decisão registrada em `docs/decisoes-autonomas.md`): a garantia vem da construção do
objeto, não de uma checagem que poderia ser esquecida.

Isso exige `crates/cli/src/main.rs` construir a lista de tools reais **uma vez**, registrá-la
em **dois** `ToolRegistry` (um sem `subagent`, usado pelo executor do subagente; um com
`subagent` incluída, usado pela sessão principal de verdade) — mesmas instâncias `Arc<dyn
Tool>`, só duas tabelas de registro diferentes, sem duplicar estado real de nenhuma tool.

### Reaproveitamento total: mesmo `PermissionGate`, mesmo `Confirmer`, mesmos `Guardrails`

O subagente usa o **mesmo** `Arc<dyn ToolExecutor>` (logo, o mesmo `PermissionGate` e o
mesmo `Confirmer`/`Prompter` já injetados) e o **mesmo** `Arc<GuardrailGate>`/*sink* da
sessão-mãe. Uma tool sob `ask` chamada de dentro de um subagente aciona a **mesma**
confirmação interativa de qualquer outra chamada — nenhum mecanismo paralelo, nenhuma
exceção.

### Fora de escopo (v1)

- **Uso do subagente não soma automaticamente ao `usage_total` (ADR-0029) da sessão-mãe** —
  a `Session` da sessão-mãe é dona exclusiva (`&mut`) de si mesma enquanto roda seu próprio
  loop; somar de dentro de uma tool chamada por esse mesmo loop exigiria um canal de retorno
  que hoje não existe (mesmo problema estrutural que a TUI resolve só para o próprio turno
  via canal, MT-72). Em vez disso, o uso do subagente aparece no próprio texto devolvido
  (transparência ao agente/usuário) — acumulação de verdade fica para uma extensão futura, se
  houver demanda.
- **Sem `AGENTS.md`/skills no contexto do subagente** — só a `description` recebida vira a
  mensagem inicial; reaproveitar `project_instructions`/`skills_list` (ADR-0023) é uma
  extensão mecânica futura, cortada aqui por escopo mínimo.
- **Um nível de aninhamento, sequencial, sem *streaming*** — sem subagentes concorrentes
  nem recursivos (ver acima); a resposta do subagente só aparece de uma vez, ao final.

## Consequências

- **Impacto positivo:** delega subtarefas sem duplicar nenhuma infraestrutura de segurança
  (egresso, permissão, *guardrails*) — a restrição de egresso do mantenedor sai "de graça" do
  `Router` compartilhado; prova, mais uma vez, que a fronteira `Tool` (MT-11) generaliza bem.
- **Impacto negativo:** uso de tokens do subagente não é somado automaticamente ao total
  visível da sessão (ADR-0029) — só aparece embutido no texto de resposta da tool.
- **Trade-offs aceitos:** um nível de aninhamento só, sem contexto de projeto herdado, sem
  *streaming* — escopo mínimo deliberado; todos são extensões futuras claras, não decisões
  fechadas em pedra.

## Diretriz de Conformidade de Código

- **Proibido:** um subagente resolver um candidato de `provider`/`model` fora do `Arc<Router>`
  compartilhado com a sessão-mãe (nunca um `Router` próprio/mais permissivo); registrar a
  tool `subagent` no `ToolRegistry` que o próprio subagente usa (recursão); qualquer tool
  chamada de dentro de um subagente contornar o `PermissionGate`/`Confirmer` já em uso pela
  sessão-mãe.
- **Obrigatório:** `subagent` só em `crates/core/src/tools/subagent.rs`; a resolução de rota
  do subagente passa sempre por `Router::resolve`/`resolve_with_override` (nunca um caminho
  de resolução paralelo); executor do subagente e da sessão principal continuam a mesma
  `Arc<dyn ToolExecutor>` subjacente exceto pela ausência da tool `subagent` no registro
  interno.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
