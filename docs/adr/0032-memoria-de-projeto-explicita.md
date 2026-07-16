<!-- Caminho relativo: docs/adr/0032-memoria-de-projeto-explicita.md -->

# ADR 0032: Memória de projeto explícita entre sessões (`/remember`)

- **Status:** Accepted
- **Data:** 2026-07-16
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** ferramentas, persistência, confidencialidade

## Contexto

A Fase 19 (subagentes) fechou inteira (MT-90..92, ADR-0031 `Accepted`). Restavam duas frentes
de "segunda onda" — memória entre sessões, multimodal. **Multimodal continua fora de
cogitação**: a própria resposta do mantenedor a adia até existir um *guardrail* de imagem
(OCR), pré-requisito ainda não construído (`docs/roadmap-longo-prazo.md` §Fase 21+). Memória
entre sessões é, portanto, a única frente pronta para virar ADR agora — sem exigir escolher
entre candidatas, já que a outra está bloqueada por um pré-requisito próprio.

**Decisão-chave do mantenedor** (2026-07-16, `docs/decisoes-autonomas.md`): só memória
**explícita** — um comando tipo `/remember` que grava um fato pontual **aprovado pelo
usuário**, nunca persistência automática do conteúdo integral de uma conversa entre sessões.
Isso descarta, de saída, o padrão LLM-Wiki/OKF original cogitado no roadmap de longo prazo
(resumo automático de cada sessão) — o motivo declarado foi a pergunta de retenção/
confidencialidade que persistir conversa inteira levantaria, incompatível com o objetivo de
homologação corporativa do projeto.

O `agentry` já tem: o diretório de estado local (`.agentry/`, ADR-0017, auto-excluído do git
por padrão) — mesmo padrão já usado por `checkpoints.json` (MT-86, ADR-0030) e pelos índices
RAG; `Session::ensure_system_prompt` já concatena `project_instructions` (ADR-0023, MT-59) e
`skills_list` (ADR-0023, MT-60) via o mesmo padrão *builder* `with_*`. Memória de projeto
explícita reaproveita ambos, sem inventar mecanismo novo.

## Decisão

### `/remember <fato>` — comando, nunca uma tool que o agente chama sozinho

Novo comando `/remember <fato>` no REPL (mesmo padrão de `/compact`/`/usage`/`/undo`) e flag
`--remember <fato>` no modo *one-shot* (desfaz/sai sem rodar tarefa, mesmo padrão de
`--undo`) — grava `<fato>` em `.agentry/memory.json` e sai/continua. **Deliberadamente não
existe uma tool `remember` no `ToolRegistry`**: o texto da resposta do mantenedor ("comando...
que o usuário aprova") descreve um ato do usuário, não uma decisão do modelo sobre o que vale
lembrar — introduzir uma tool (mesmo sob `ask`) reintroduziria exatamente a automação que a
resposta pretendia evitar. Decisão de implementação registrada em
`docs/decisoes-autonomas.md` (2026-07-16).

### Armazenamento: `.agentry/memory.json`, array de *strings*, sem teto

Um array JSON de *strings* simples — um fato por entrada, sem *id*/*timestamp*/estrutura
extra (mais fácil de editar à mão; sem comando `/forget` nesta versão — remover uma entrada é
editar o arquivo diretamente). **Sem teto de entradas** (diferente de `checkpoints.json`,
MT-86) — fatos são curados manualmente pelo usuário, um por comando explícito, não gerados
automaticamente a cada chamada de tool, então o risco de crescimento descontrolado é muito
menor; se isso mudar, um teto é extensão futura simples.

### Carregada no início da sessão, mesmo mecanismo de `project_instructions`/`skills_list`

`Session` ganha `with_memoria(texto)` (mesmo padrão *builder* de `with_project_instructions`/
`with_skills_list`). `ensure_system_prompt` concatena, nesta ordem: instruções de projeto
(`AGENTS.md`/`CLAUDE.md`, mais geral), **memória de projeto** (mesma categoria de contexto
durável específico do projeto, mas curado pelo usuário em vez de commitado no repo),
`system_prompt` do preset da *task-class* ativa (mais específico), lista de skills (índice,
não contexto em si). Nenhum mecanismo paralelo de injeção de contexto.

### Confidencialidade: opt-in por natureza, local ao projeto

Cada entrada exige um ato explícito do usuário (digitar o comando) — não há nenhum caminho
onde conteúdo de conversa vira memória sem essa ação. `.agentry/memory.json` é local ao
projeto e auto-excluído do git por padrão (ADR-0017) — mesma garantia de confidencialidade já
aceita para checkpoints e índices RAG.

## Consequências

- **Impacto positivo:** contexto de projeto persiste entre sessões (algo que hoje só existe
  *dentro* de uma sessão via compactação, ADR-0016) sem levantar a pergunta de retenção que
  persistência automática levantaria — o usuário decide exatamente o que fica gravado.
- **Impacto negativo:** exige disciplina do usuário (nada é lembrado a menos que ele
  explicitamente peça) — menos "mágico" que memória automática, aceito deliberadamente pelo
  mantenedor.
- **Trade-offs aceitos:** sem `/forget`/edição assistida nesta versão (editar o arquivo à
  mão é o caminho); sem teto de entradas (extensão futura se necessário).

## Diretriz de Conformidade de Código

- **Proibido:** qualquer caminho que grave conteúdo de conversa em `.agentry/memory.json`
  sem um comando explícito do usuário (`/remember`/`--remember`); registrar uma tool
  `remember`/equivalente no `ToolRegistry` — memória de projeto explícita é sempre um ato do
  usuário, nunca uma decisão do modelo.
- **Obrigatório:** armazenamento de memória só em `.agentry/memory.json` (ADR-0017, nunca um
  diretório global do usuário); `/remember` (REPL) e `--remember` (*one-shot*) chamam a
  **mesma** função de gravação, nunca duas implementações divergentes.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
