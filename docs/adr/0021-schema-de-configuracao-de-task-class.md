<!-- Caminho relativo: docs/adr/0021-schema-de-configuracao-de-task-class.md -->

# ADR 0021: Schema de configuração de task-class (rotas e presets configuráveis)

- **Status:** Proposed
- **Data:** 2026-07-14
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** configuração, roteamento, egresso, task-class

## Contexto

O diferencial central do `agentry` (ver `docs/architecture.md`) é o **roteamento por
task-class**: cada tipo de tarefa mapeia para `(provider, modelo, classe de egresso)` + um
preset de parâmetros. O `Router` (ADR-0008/0014) já implementa isso por completo —
`RouteEntry` (`candidates: Vec<RouteTarget>` + `preset: CallPreset`), `RouteTarget`
(`provider`/`model`/`egress_class`), `CallPreset` (`temperature`/`top_p`/`system_prompt`/
`max_tokens`/`reasoning`), e `resolve`/`resolve_with_override`. O código de fato **usa** três
task-classes: `chat` (a principal), `compact` (compactação de histórico, ADR-0016) e a do
Reviewer (`guardrail-compliance`, ADR-0015).

O que **não existe** é a camada de configuração que permita ao usuário declarar essas
task-classes. Hoje a CLI (`crates/cli/src/main.rs`/`repl.rs`) hardcoda uma única rota `chat`
apontando para o Ollama local (`repl::set_chat_route`), e as task-classes `compact`/
`guardrail-compliance` **não são sequer registradas no Router real** — na prática, `/compact`
e o Reviewer não têm rota configurada na CLI distribuída. Ou seja: o recurso que mais
distingue o projeto (roteamento multi-modelo por privacidade) é inacessível ao usuário final,
que só consegue trocar de modelo dentro da mesma task-class `chat` via `--model`/`--provider`
(ADR-0006/MT-49/50).

Uma decisão é necessária agora porque, sem esse schema, todo o investimento no `Router`, no
Reviewer e na compactação fica represado atrás de uma fiação hardcoded — e o usuário pediu
explicitamente para "completar as configurações acerca das task-class".

## Decisão

Fica acordada a introdução de um bloco **`taskClasses`** no `agentry.settings.json`
(`agentry-settings-schema:1`, extensão sob ADR-0018), que popula os tipos já existentes do
`Router` — **sem criar nenhum tipo novo de roteamento**, só a camada de configuração:

```jsonc
"taskClasses": {
  "chat": {
    "candidates": [
      { "provider": "ollama", "model": "llama3.1:8b", "egressClass": "local-only" }
    ],
    "preset": { "temperature": 0.2, "maxTokens": 4096 }
  }
}
```

- **`candidates`** é uma lista **ordenada** por preferência; o `Router` já escolhe o primeiro
  cuja classe de egresso é permitida pela sessão **e** cujo provider está registrado
  (ADR-0002 fail-closed preservado).
- **`preset`** mapeia 1:1 para `CallPreset` (chaves em `camelCase`: `topP`, `maxTokens`,
  `systemPrompt`).

**Defaults obrigatórios (fail-safe, zero-config idêntico):** quando o usuário **não** declara
uma task-class, o `Config` **sintetiza** as três internas hoje hardcoded — `chat` (Ollama,
`local-only`), `compact` e `guardrail-compliance` — de modo que:
1. um projeto sem `taskClasses` no arquivo se comporta exatamente como hoje;
2. `/compact` (ADR-0016) e o Reviewer (ADR-0015) passam a ter rota configurada de fato na
   CLI, coisa que hoje falta.
Uma task-class declarada pelo usuário **sobrescreve** o default sintetizado de mesmo nome.

**Merge entre camadas** (perfil → arquivo → ambiente): por **nome** de task-class — uma
task-class nova é adicionada; o mesmo nome em duas camadas resolve com a camada mais
específica vencendo em `candidates`/`preset`. A classe de egresso de um candidato **nunca é
afrouxada** por merge (mesma disciplina fail-closed de `Permissions::union`, MT-44).

**Seleção da task-class por invocação:** `chat` é a task-class default voltada ao usuário
(interativo e one-shot). Uma flag `--task-class <nome>` (e comando `/task-class <nome>` no
REPL) permite escolher outra task-class **declarada** para uma invocação — mesmo padrão de
override já vetado do `--provider`/`--model` (ADR-0014): nunca introduz um alvo não declarado.
As task-classes internas (`compact`, `guardrail-compliance`) são consumidas pelos seus
subsistemas, não normalmente selecionadas à mão.

## Consequências

- **Impacto positivo:** o roteamento multi-modelo por privacidade — o diferencial do projeto
  — passa a ser configurável de ponta a ponta pelo usuário; `/compact` e Reviewer ganham
  rota real na CLI; reaproveita 100% do `Router` existente, sem tipo novo; zero mudança para
  quem não configura (defaults sintetizados).
- **Impacto negativo:** aumenta a superfície do schema (mitigado pela convenção da ADR-0022 —
  bloco vem com default + comentário + exemplos); a flag `--task-class` adiciona uma dimensão
  de seleção a mais na CLI.
- **Trade-offs aceitos:** sintetizar defaults em código (em vez de exigir declaração
  explícita) em troca de compatibilidade total com projetos existentes.

## Diretriz de Conformidade de Código

- **Proibido:** hardcodar rota de task-class na CLI quando houver configuração resolvida
  disponível; permitir que um merge de camadas **afrouxe** a classe de egresso de um
  candidato; deixar `compact`/`guardrail-compliance` sem rota quando a sessão os usa;
  introduzir, via `--task-class`/`/task-class`, um alvo não declarado na configuração.
- **Obrigatório:** ausência de `taskClasses` sintetiza os defaults internos (`chat`/`compact`/
  `guardrail-compliance`) preservando o comportamento atual; a seleção por invocação escolhe
  apenas entre task-classes declaradas; todo candidato carrega classe de egresso explícita
  (ADR-0002).

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
