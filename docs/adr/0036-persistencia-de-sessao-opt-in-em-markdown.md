<!-- Caminho relativo: docs/adr/0036-persistencia-de-sessao-opt-in-em-markdown.md -->

# ADR 0036: Persistência de sessão opt-in, em Markdown (`--resume`)

- **Status:** Accepted
- **Data:** 2026-07-24
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** persistência, confidencialidade, UX, retomada de sessão

## Contexto

Pedido do mantenedor: poder fechar o `agentry` no meio de uma conversa e retomá-la depois
(`--resume`), comportamento equivalente ao do Claude Code CLI. Hoje `Session::messages` é um
`Vec<Message>` puramente em memória — nenhum modo (REPL/*one-shot*/TUI) persiste histórico de
conversa em disco.

**Conflito real com a ADR-0032, levantado antes de qualquer código:** a ADR-0032 (memória de
projeto explícita, `/remember`) já registrou, em 2026-07-16, a decisão de **nunca** persistir
"o conteúdo integral de uma conversa entre sessões" — motivo declarado: a pergunta de
retenção/confidencialidade que isso levanta, incompatível com o objetivo de homologação
corporativa do projeto ([[project-purpose-and-origin]]). Escalado ao mantenedor antes de
prosseguir (não contornado silenciosamente, disciplina da skill `adr-writer`). Resposta: **a
decisão da ADR-0032 continua válida como padrão** — nada é persistido automaticamente, nunca.
Esta ADR abre uma **exceção estreita e opt-in**: o usuário pode pedir explicitamente para
salvar *esta* conversa específica, com aviso claro do que isso implica. Não é uma reversão da
ADR-0032, é uma exceção deliberada, documentada, e acionada apenas por vontade explícita do
usuário — nunca por um resumo automático de cada sessão (o padrão LLM-Wiki/OKF que a ADR-0032
já descartou continua descartado).

O `agentry` já tem o lugar certo pra isso: `.agentry/session/` já está **reservado** desde a
ADR-0017 ("histórico de sessão persistido, se/quando um ticket futuro decidir implementar
retomada de sessão") — esta é exatamente essa decisão.

## Decisão

### 1. Estritamente opt-in, nunca automático

Salvar uma sessão exige um ato explícito do usuário — mesmo espírito do `/remember` (ADR-0032):
nenhuma sessão é salva sozinha ao sair do `agentry`, em nenhum modo. Comandos novos:

- **`/save [nome]`** (REPL e TUI) — salva a sessão corrente em
  `.agentry/session/<id>.md`. `id` é um *timestamp* ISO 8601 compacto
  (`AAAAMMDD-HHMMSS`) quando `nome` não é dado; com `nome`, vira `<id>-<nome>` (nome
  sanitizado: só `[a-z0-9-]`, resto descartado) — sempre prefixado pelo *timestamp*, pra
  listagem em ordem cronológica nunca depender do usuário ter nomeado de forma ordenável.
- **`--resume [id-ou-nome]`** (flag de invocação, os três modos) — sem argumento, retoma a
  sessão salva mais recente; com um `id`/`nome` (ou prefixo único), retoma aquela. Carrega o
  histórico salvo **antes** do primeiro turno, current a sessão continua exatamente como uma
  nova, só que com `Session::messages` pré-populado.
- **`/sessions`** (REPL e TUI) — lista as sessões salvas (`id`, data, primeiras palavras da
  primeira mensagem do usuário, como um título) — sem isso, `--resume` sem argumento é um
  tiro no escuro.

Ao salvar, o `agentry` sempre imprime um aviso (stderr, também na TUI como mensagem de
sistema): *"sessão salva em `.agentry/session/<id>.md` — pode conter informação sensível da
conversa; o diretório já está fora do controle de versão (`.agentry/.gitignore`), mas o
arquivo continua no disco até você apagá-lo"*.

### 2. Formato: Markdown com *front matter* YAML

Legível por humano (o mantenedor pode abrir e ler/editar uma sessão salva num editor de
texto qualquer) **e** reconstruível sem ambiguidade de volta para `Vec<Message>`:

````markdown
---
id: 20260724-183000
criado_em: 2026-07-24T18:30:00Z
provider: litellm
model: gpt-4o
task_class: chat
usage:
  input_tokens: 1234
  output_tokens: 567
---

## Sistema

<texto do system prompt, se houver -- Role::System>

## Usuário

<texto -- ContentBlock::Text>

## Agente

<texto de resposta, se houver>

```tool-call
{"id":"call_1","name":"fs_write","arguments":{"path":"a.txt","content":"..."}}
```

## Tool

```tool-result
{"call_id":"call_1","content":"arquivo criado","is_error":false}
```

## Agente

<texto final>
````

Cada `## <Papel>` inicia uma nova `Message` (`Sistema`→`Role::System`, `Usuário`→`Role::User`,
`Agente`→`Role::Assistant`, `Tool`→`Role::Tool`); dentro da seção, texto corrido vira
`ContentBlock::Text`, e blocos cercados com info-string `tool-call`/`tool-result` viram
`ContentBlock::ToolCall`/`ToolResult` (JSON de uma linha, `serde_json` — `ToolCall`/`ToolResult`
já são `Serialize`/`Deserialize`, nenhum formato novo inventado). Várias seções do mesmo papel
em sequência são permitidas (ex.: duas chamadas de tool seguidas viram duas seções `## Agente`)
— o *parser* nunca precisa adivinhar limite de mensagem por heurística de conteúdo, só pelos
cabeçalhos.

### 3. Local: `.agentry/session/<id>.md`

Confirma o caminho já reservado pela ADR-0017 — nenhum diretório novo, nenhuma exceção nova
de `.gitignore` (já auto-excluído pelo `.agentry/.gitignore` existente).

## Consequências

- **Positivas:** paridade com o `--resume`/`--continue` do Claude Code CLI; formato legível e
  editável por humano (dá pra revisar o que será retomado antes de rodar, ou até editar uma
  sessão salva à mão); reaproveita serialização já existente de `ToolCall`/`ToolResult`.
- **Negativas/riscos aceitos:** uma sessão salva é, por definição, um registro em disco do que
  foi discutido — inclusive qualquer conteúdo sensível que tenha entrado na conversa (nome de
  arquivo, trecho de código, credencial colada por engano). O aviso ao salvar existe
  exatamente para isso: o usuário decide, ciente do que implica, cada vez.
- **Fora de escopo:** qualquer resumo/persistência automática (a ADR-0032 continua proibindo
  isso como padrão); sincronizar sessões salvas para fora da máquina local (ADR-0002/ADR-0017);
  RAG sobre sessões salvas (frente separada, depende desta existir primeiro — ver
  `docs/roadmap-v0.16.md` Fase I).

## Diretriz de Conformidade de Código

- **Proibido:** salvar qualquer sessão sem um ato explícito do usuário (`/save`) — nenhum
  gancho automático em `Drop`, saída do processo, ou `MessageEnd`.
- **Obrigatório:** o aviso de retenção/confidencialidade é impresso **toda vez** que uma
  sessão é salva, sem *flag* pra silenciar — não é um aviso "só na primeira vez".
  `.agentry/session/` segue as mesmas regras de auto-exclusão/proibição de sincronização
  externa já estabelecidas pela ADR-0017 (nenhuma exceção nova).
- **Obrigatório:** o *parser* Markdown→`Vec<Message>` nunca falha silenciosamente — um arquivo
  editado à mão de forma inválida (JSON malformado num bloco `tool-call`, cabeçalho de papel
  desconhecido) é erro tratado e reportado ao usuário, nunca uma sessão retomada com histórico
  truncado/incorreto sem aviso.
