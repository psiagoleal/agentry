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

## Criar o arquivo (`--init` / `/init`)

```bash
agentry --init
```

Cria `.agentry/agentry.settings.json` com um exemplo genérico (todas as capacidades
ligadas, permissões vazias) e imprime um comando manual alternativo. Não sobrescreve um
arquivo já existente.

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
    "lspGrounding": { "enabled": true }
  },
  "providers": {
    "ollama": { "structuredOutput": true }
  },
  "guardrails": {
    "input": [
      { "id": "bloqueia-chave-aws", "match": "AKIA", "action": "block" }
    ],
    "output": [
      { "id": "mascara-email-interno", "match": "@minhaempresa.com", "action": "redact" }
    ]
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

Liga/desliga as três capacidades de contexto do agente — todas `true` por padrão:

- `repoMap.enabled` — mapa do repositório (símbolos relevantes via `tree-sitter`).
- `semanticRag.enabled` — busca semântica local no código (tool `code_search`).
- `lspGrounding.enabled` — consulta a um *language server* real (`lsp_hover`/`lsp_definition`).

### `providers.ollama.structuredOutput`

Liga (`true`, padrão) ou desliga saída estruturada (*constrained decoding*) nas chamadas ao
Ollama, usada para tornar as chamadas de tool mais confiáveis.

### `guardrails`

Ver [Guardrails de conteúdo](guardrails.md) — regras de bloqueio/mascaramento determinístico
aplicadas antes de qualquer mensagem ir ao modelo, e sobre a resposta antes dela voltar para
você.

## Flags de linha de comando

Flags por invocação sobrescrevem tudo o que está no arquivo/ambiente, só para aquela
chamada — ver [Uso da CLI e do REPL](uso.md).
