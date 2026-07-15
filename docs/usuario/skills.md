<!-- Caminho relativo: docs/usuario/skills.md -->

# Skills

Skills são capacidades sob demanda: instruções detalhadas para uma tarefa específica
(escrever um ADR, revisar um PR, planejar micro-tickets), carregadas pelo agente só quando
ele decide usá-las — em vez de todo esse texto ocupar contexto em toda mensagem trocada.

O `agentry` reaproveita **literalmente** a convenção `.claude/skills/<nome>/SKILL.md` já
usada pelo Claude Code — o mesmo formato, no mesmo caminho. Se o seu projeto já tem skills
definidas para o Claude Code (como este próprio repositório, em `.claude/skills/`), o
`agentry` já enxerga todas elas, sem nenhuma migração.

## Formato de um `SKILL.md`

```markdown
---
name: adr-writer
description: >-
  Cria e atualiza Registros de Decisão de Arquitetura (ADRs). Aciona ao decidir
  bibliotecas, padrões arquiteturais, ou quando o usuário pedir para documentar
  uma decisão técnica.
---

# adr-writer — Registros de Decisão de Arquitetura

Corpo da skill: instruções completas, exemplos, template — tudo o que o agente
precisa para executar a tarefa depois de carregar esta skill.
```

- **Frontmatter** (entre os dois `---`): duas chaves obrigatórias, `name` e `description`.
  `description` deve deixar claro **quando** a skill se aplica — é só esse texto que fica
  sempre visível ao agente (ver abaixo); uma descrição vaga reduz a chance do agente escolher
  a skill certa na hora certa.
- **Corpo** (tudo depois do segundo `---`): as instruções completas, só carregadas quando o
  agente decide usar a skill.

**Parser de frontmatter mínimo, não YAML genérico:** o `agentry` reconhece `chave: valor`
numa linha e o bloco dobrado `chave: >-` (como no `description` do exemplo acima — várias
linhas indentadas viram uma só, com espaço entre elas). Sintaxes YAML mais elaboradas (listas,
mapas aninhados, âncoras) **não são suportadas** — um `SKILL.md` fora desse formato mínimo é
descoberto, mas sem `name`/`description`, e é ignorado silenciosamente (não trava a
descoberta das demais skills).

## Como o agente descobre e usa

1. **Descoberta automática:** toda sessão varre `.claude/skills/*/SKILL.md` (um nível de
   subdiretórios, sem recursão) e monta uma lista compacta (`nome: descrição`) — inserida no
   *system prompt*, junto de eventuais instruções de projeto (ver [Memória de
   projeto](configuracao.md#memoria-de-projeto-agentsmdclaudemd)). Sem opção de desligar —
   o custo é desprezível (só nome+descrição de cada skill, nunca o corpo inteiro).
2. **Carregamento sob demanda:** o agente tem acesso a uma tool `skill`; quando decide que uma
   skill se aplica, ele a chama pelo nome e recebe o corpo completo daquele `SKILL.md` — só
   nesse momento, nunca antes.

Um `SKILL.md` coberto pelo seu `.agentryignore`/`.claudeignore` é pulado — mesma checagem de
confidencialidade usada em todo o resto do projeto (ver [Arquivo de ignore do
`agentry`](configuracao.md#arquivo-de-ignore-do-agentry-agentryignore)).

## Nada a configurar

Diferente de outros mecanismos deste guia, skills não têm bloco próprio em
`agentry.settings.json` — o diretório `.claude/skills/` é a única fonte, sempre a mesma para
qualquer projeto. Para adicionar uma skill, basta criar
`.claude/skills/<nome-da-sua-skill>/SKILL.md` seguindo o formato acima.
