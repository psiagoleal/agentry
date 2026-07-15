<!-- Caminho relativo: docs/usuario/configuracao.md -->

# Configuração — `agentry.settings.json`

O `agentry` lê configuração de duas camadas, nesta ordem (a segunda sobrescreve a
primeira campo a campo):

1. **Arquivo** `.agentry/agentry.settings.json`, na raiz do projeto (procurado a partir do
   diretório atual, subindo até encontrar um `.git`).
2. **Variáveis de ambiente**, prefixo `AGENTRY_`: `AGENTRY_PROFILE`, `AGENTRY_MODEL`,
   `AGENTRY_MAX_TOKENS`.

Nenhum dos dois é obrigatório — sem arquivo e sem variáveis, a CLI roda com os *defaults*
descritos abaixo.

Uma terceira variável, `AGENTRY_LITELLM_API_KEY`, é tratada à parte — nunca é uma camada de
configuração (não define nenhum campo do arquivo); é só a chave de API do gateway LiteLLM,
lida diretamente no momento de montar a conexão (ver [`providers.litellm`](#providerslitellm)
abaixo). Segredo nunca fica no arquivo de configuração.

## Criar o arquivo (`--init` / `/init`)

```bash
agentry --init
```

Cria `.agentry/agentry.settings.json` com um exemplo genérico (todas as capacidades
ligadas, permissões vazias) e imprime um comando manual alternativo. Não sobrescreve um
arquivo já existente. **Todo campo do schema já vem no arquivo gerado** — os que ficam
inertes até você preencher (`profile`, `model`, `max_tokens`, `providers.litellm.*`)
aparecem como `null` (JSON não tem comentário; `null` é o jeito de mostrar "a chave existe,
ainda desligada"); cada bloco tem uma chave `_comentario` explicando o que fazer ali —
ignorada pelo `agentry`, só para leitura humana.

Se você tem um perfil do
[`ai-coding-agent-profiles`](https://github.com/psiagoleal/ai-coding-agent-profiles)
(`empresa` / `externo-confidencial` / `pessoal`):

```bash
agentry --init --profile empresa
```

Busca o `agentry.settings.json` real daquele perfil numa referência fixa do repositório
público (nunca "latest" dinâmico) e grava localmente. Falha de rede aqui é erro tratado —
nunca cai silenciosamente no exemplo genérico. O mesmo comando existe dentro do REPL:
`/init` ou `/init <perfil>`.

## Estrutura do arquivo

```json
{
  "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
  "schemaVersion": 1,
  "profile": "empresa",
  "model": "llama3.1:8b",
  "max_tokens": 4096,
  "permissions": {
    "deny": ["shell_execute"],
    "ask": ["fs_write"]
  },
  "context": {
    "repoMap": { "enabled": true },
    "semanticRag": { "enabled": true },
    "lspGrounding": { "enabled": true },
    "gitignore": { "enabled": false },
    "agentsFile": { "enabled": true }
  },
  "providers": {
    "ollama": { "structuredOutput": true },
    "litellm": {
      "baseUrl": "https://litellm.minhaempresa.com",
      "model": "empresa/gpt-30b",
      "egressClass": "local-only"
    }
  },
  "guardrails": {
    "input": [
      { "id": "bloqueia-chave-aws", "match": "AKIA", "action": "block" }
    ],
    "output": [
      { "id": "mascara-email-interno", "match": "@minhaempresa.com", "action": "redact" }
    ]
  },
  "taskClasses": {
    "chat": {
      "candidates": [
        { "provider": "ollama", "model": "llama3.1:8b", "egressClass": "local-only" }
      ]
    },
    "revisao-em-nuvem": {
      "candidates": [
        { "provider": "litellm", "model": "empresa/gpt-30b", "egressClass": "cloud-ok" }
      ],
      "preset": { "temperature": 0.2 }
    }
  },
  "tools": {
    "webFetch": { "enabled": false },
    "webSearch": {
      "searxngUrl": "https://searx.minhaempresa.com",
      "searxngEgressClass": "local-only"
    }
  }
}
```

Todo campo é opcional — uma camada só sobrescreve o que declara.

`schemaVersion` divergente da versão suportada (**1**, hoje) é erro tratado: a CLI aborta
com mensagem explícita em vez de tentar interpretar um schema que não conhece.

### `permissions`

Controla quais **ferramentas** (não conteúdo) o agente pode executar:

- `deny` — nomes de tool sempre bloqueados, sem pedir confirmação.
- `ask` — nomes de tool que pedem confirmação interativa antes de rodar.
- Qualquer tool fora das duas listas roda sem confirmação — **exceto** a tool de shell, que
  vem bloqueada por padrão nesta versão da CLI (nenhum padrão de comando pré-liberado).

Entre camadas (arquivo → ambiente), as listas só **crescem**: uma permissão herdada nunca é
removida por uma camada mais específica.

### `context`

Liga/desliga as capacidades de contexto do agente:

- `repoMap.enabled` — mapa do repositório (símbolos relevantes via `tree-sitter`). `true`
  por padrão.
- `semanticRag.enabled` — busca semântica local no código (tool `code_search`). `true` por
  padrão.
- `lspGrounding.enabled` — consulta a um *language server* real (`lsp_hover`/`lsp_definition`).
  `true` por padrão.
- `gitignore.enabled` — respeito **opcional** a `.gitignore`, reduzindo o que o agente vê
  (arquivos de build, `node_modules`, etc.) sem precisar duplicar cada padrão manualmente.
  **`false` por padrão** — diferente das três acima, é *opt-in*: ligar nunca é necessário
  para o comportamento atual continuar igual, e configurar isso não afeta confidencialidade
  (ver [Arquivo de ignore do `agentry`](#arquivo-de-ignore-do-agentry-agentryignore) abaixo
  para o mecanismo que de fato controla isso).
- `agentsFile.enabled` — leitura de `AGENTS.md`/`CLAUDE.md` como instruções de projeto (ver
  [Memória de projeto](#memoria-de-projeto-agentsmdclaudemd) abaixo). `true` por padrão —
  mesma categoria das três primeiras (custo baixo: leitura local de um arquivo pequeno).

### Memória de projeto (`AGENTS.md`/`CLAUDE.md`)

Se a raiz do projeto tiver um `AGENTS.md` (fonte única de convenções — [convenção
`agents.md`](https://agents.md), a mesma usada por este próprio repositório) ou, na ausência
dele, um `CLAUDE.md` (*fallback* — comum em projetos que só têm convenções para o Claude
Code), o conteúdo é lido automaticamente e passa a fazer parte do *system prompt* de toda
sessão — o agente já chega sabendo convenções, arquitetura ou restrições do repositório, sem
nenhuma configuração extra.

- **`AGENTS.md` é sempre primário; `CLAUDE.md` só é lido na ausência dele — nunca os dois
  juntos.** Se o seu `CLAUDE.md` só aponta para `AGENTS.md` (como o deste próprio
  repositório), isso é o comportamento esperado: com `AGENTS.md` presente, `CLAUDE.md` nunca
  chega a ser lido.
- O texto lido entra **antes** do `system_prompt` de uma eventual `task-class` (ver
  [`taskClasses`](#taskclasses)) — instruções de projeto primeiro (mais gerais), preset da
  tarefa depois (mais específico) — numa única mensagem de sistema.
- **Respeita `.agentryignore`/`.claudeignore`**: se `AGENTS.md`/`CLAUDE.md` estiver coberto
  pelo seu arquivo de ignore, ele é pulado como qualquer outro arquivo escondido do agente —
  não existe um segundo controle de confidencialidade paralelo (ver [Arquivo de ignore do
  `agentry`](#arquivo-de-ignore-do-agentry-agentryignore) abaixo).
- Desligue com `context.agentsFile.enabled: false` se não quiser esse comportamento.

Skills (`.claude/skills/<nome>/SKILL.md`) são um mecanismo relacionado, mas
separado — ver [Skills](skills.md).

### Arquivo de ignore do `agentry` (`.agentryignore`)

Distinto de `context.gitignore.enabled` acima — **dois mecanismos diferentes, para dois
objetivos diferentes**, não confundir um pelo outro:

- **`.agentryignore`** (arquivo próprio na raiz do projeto, sintaxe `.gitignore`) —
  controla o que o agente **nunca vê**, independente de estar versionado ou não. Um arquivo
  pode estar no Git e fora do contexto do agente (liste em `.agentryignore`); ou fora do Git
  e ainda assim visível ao agente (comportamento *default* — `.gitignore` não é olhado a
  menos que você ligue `context.gitignore.enabled`). Esse é o mecanismo de
  **confidencialidade**.
- **`context.gitignore.enabled`** — só reduz **ruído de contexto** (evita reprocessar
  artefatos de build já listados em `.gitignore`), *opt-in*, sem nenhuma relação com o que é
  ou não confidencial.

`.agentryignore` é sempre checado primeiro; se ausente, a CLI cai para `.claudeignore` (nome
legado, mantido só por compatibilidade — se os dois arquivos existirem no mesmo projeto,
`.agentryignore` vence sozinho, nunca um merge dos dois). Exemplo:

```
# .agentryignore — sintaxe idêntica a .gitignore
segredos/
*.pem
.env*
```

### `providers.ollama.structuredOutput`

Liga (`true`, padrão) ou desliga saída estruturada (*constrained decoding*) nas chamadas ao
Ollama, usada para tornar as chamadas de tool mais confiáveis.

### `providers.litellm`

Conecta a CLI a um gateway [LiteLLM](https://www.litellm.ai/) (comum em ambientes
corporativos, na frente de modelos maiores/de nuvem) como um **segundo provider**,
selecionável via [`--provider litellm` / `/provider litellm`](uso.md#flags-de-invocacao-one-shot)
— por padrão, sem essa flag, o Ollama local continua sendo usado.

- `baseUrl` — URL base do gateway (ex.: `https://litellm.minhaempresa.com`).
- `model` — identificador do modelo nesse gateway.
- `egressClass` — `"local-only"`, `"cloud-opt-out"` ou `"cloud-ok"` (ver [Modelo de
  privacidade e egresso](../governanca/privacidade-e-egresso.md) para o que cada uma
  significa). **Sempre declare explicitamente** — ausente (`null`) é tratado como
  `"cloud-ok"` (a mais restritiva para liberar), então um gateway só acessível pela rede
  interna/VPN da empresa, por exemplo, precisa de `"egressClass": "local-only"` para ficar
  de fato alcançável; sem essa declaração, o candidato fica registrado mas nunca resolve
  (nenhum erro fatal — só cai de volta para o Ollama, ou falha de rota tratada se nem o
  Ollama estiver disponível).

**`baseUrl` e `model` precisam estar os dois preenchidos** para este provider ativar —
qualquer um ausente (ou `null`) e a CLI se comporta exatamente como se `providers.litellm`
não existisse.

A chave de API do gateway (se ele exigir uma) **não vai neste arquivo** — vem da variável
de ambiente `AGENTRY_LITELLM_API_KEY`, lida só no momento de montar a conexão. Ausente, a
CLI simplesmente não anexa nenhum cabeçalho de autorização (gateways internos sem
autenticação continuam funcionando normalmente).

### `guardrails`

Ver [Guardrails de conteúdo](guardrails.md) — regras de bloqueio/mascaramento determinístico
aplicadas antes de qualquer mensagem ir ao modelo, e sobre a resposta antes dela voltar para
você.

### `taskClasses`

O `agentry` roteia cada tipo de tarefa por uma **task-class**: um nome (`chat`, `compact`,
ou qualquer nome que você declarar) que mapeia para uma lista ordenada de **candidatos**
(provider + modelo + classe de egresso) e um **preset** de parâmetros de chamada. Este é o
mecanismo central de roteamento multi-modelo por privacidade do projeto — antes desta versão,
só existia uma rota fixa (`chat`, sempre Ollama); agora qualquer task-class é configurável.

```jsonc
"taskClasses": {
  "revisao-em-nuvem": {
    "candidates": [
      { "provider": "litellm", "model": "empresa/gpt-30b", "egressClass": "cloud-ok" },
      { "provider": "ollama", "model": "llama3.1:8b", "egressClass": "local-only" }
    ],
    "preset": { "temperature": 0.2, "maxTokens": 4096 }
  }
}
```

- **`candidates`** — lista **ordenada** por preferência; o Router escolhe o primeiro cujo
  candidato tem `egressClass` permitida pelo perfil ativo **e** cujo provider está registrado
  (`ollama` sempre está; `litellm` só se [`providers.litellm`](#providerslitellm) estiver
  configurado). Um candidato indisponível (provider não registrado, ou classe de egresso
  insuficiente) é **pulado silenciosamente** — só falha se nenhum candidato da lista servir.
- **`preset`** — mesmos campos das flags de override (`temperature`, `topP`, `maxTokens`,
  `systemPrompt`, `reasoning`), aplicados por padrão a qualquer chamada nessa task-class.

**Defaults sintetizados (zero-config idêntico):** se você não declara `taskClasses` no
arquivo, a CLI sintetiza internamente `chat` (Ollama local), `compact` (usada por
[`/compact`](uso.md#comandos-de-barra-repl)) e `guardrail-compliance` (reservada para
auditoria semântica) — o comportamento continua exatamente igual ao de uma instalação sem
este bloco. Declarar `chat` no arquivo **sobrescreve** o default sintetizado desse nome;
declarar qualquer outro nome (como `revisao-em-nuvem` acima) **adiciona** uma task-class nova,
sem remover as demais.

**Seleção por invocação:** `chat` é a task-class usada por padrão (modo *one-shot* e REPL). A
flag [`--task-class <nome>`](uso.md#flags-de-invocacao-one-shot) e o comando
[`/task-class <nome>`](uso.md#comandos-de-barra-repl) escolhem outra task-class **já
declarada** para aquela invocação — mesmo padrão de override de `--provider`/`--model`: nunca
introduz um candidato novo, só escolhe entre os já configurados; nome desconhecido ou
candidato indisponível é erro tratado, nunca *panic*.

**Merge entre camadas:** por nome de task-class — uma task-class nova é adicionada; o mesmo
nome em duas camadas resolve com a camada mais específica vencendo campo a campo em
`candidates`/`preset`. A classe de egresso de um candidato **nunca é afrouxada** por merge
(mesma disciplina fail-closed das demais listas de configuração do `agentry`).

### `tools.webFetch`

Liga a tool `web_fetch` — busca o conteúdo de uma URL (qualquer URL que o agente peça) e
devolve como texto puro (sem conversão para Markdown). **Desligada por padrão**
(`enabled: false`) e só funciona quando **duas** condições valem ao mesmo tempo:

1. `tools.webFetch.enabled: true` neste arquivo (*opt-in* explícito).
2. O perfil ativo resolve para a classe de egresso mais permissiva (`cloud-ok` — ver [Modelo
   de privacidade e egresso](../governanca/privacidade-e-egresso.md)).

Falta qualquer uma das duas e a tool **nem aparece** para o agente — não é um erro em tempo de
chamada, é a tool simplesmente não sendo oferecida. Isso é deliberado: acessar qualquer host da
internet é uma capacidade de risco real, diferente das capacidades de contexto local (`repoMap`,
`agentsFile` etc.), que vêm ligadas por padrão.

```json
"tools": {
  "webFetch": { "enabled": true }
}
```

### `tools.webSearch`

Liga a tool `web_search` — pesquisa um termo via uma instância
[SearXNG](https://docs.searxng.org/) configurada por você. **Sem instância pública
pré-configurada**: a tool só é registrada quando você informa `searxngUrl`.

- `searxngUrl` — URL base da sua instância SearXNG (própria ou de confiança). Ausente ⇒
  `web_search` não é registrada.
- `searxngEgressClass` — classe de egresso mínima desse endpoint. **Sempre declare
  explicitamente** — ausente é tratado como `"cloud-ok"` (a mais restritiva para liberar,
  mesmo *default* de [`providers.litellm`](#providerslitellm)); uma instância SearXNG
  *self-hosted* na sua rede interna pode legitimamente declarar `"local-only"`.

```json
"tools": {
  "webSearch": {
    "searxngUrl": "https://searx.minhaempresa.com",
    "searxngEgressClass": "local-only"
  }
}
```

Diferente de `web_fetch` (que mira qualquer host, por isso exige o perfil mais permissivo),
o endpoint do SearXNG é **um host único e conhecido** — cabe no mesmo modelo de allowlist já
usado por `providers.litellm`, sem precisar do perfil mais permissivo por si só (a classe
exigida é a que você declarar em `searxngEgressClass`).

## Convenção: todo bloco vem com exemplo

O arquivo gerado por `--init`/`/init` segue uma regra simples: **todo campo configurável já
aparece no arquivo**, com um valor padrão funcional (ou `null` explícito, se ficar inerte até
você preencher) e uma chave `_comentario` explicando o bloco — geralmente com um ou mais
exemplos de valores alternativos, prontos para copiar e ajustar. Você nunca precisa ler o
código-fonte para descobrir que campo existe ou qual é a sintaxe esperada. `_comentario` é
ignorada pelo `agentry` (qualquer chave começando com `_` é) — existe só para leitura humana,
e é seguro apagá-la do seu arquivo depois de configurar.

## Flags de linha de comando

Flags por invocação sobrescrevem tudo o que está no arquivo/ambiente, só para aquela
chamada — ver [Uso da CLI e do REPL](uso.md).
