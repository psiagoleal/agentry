<!-- Caminho relativo: docs/usuario/guardrails.md -->

# Guardrails de conteúdo

Guardrails são regras de correspondência de texto — **determinísticas** (substring
literal, sem diferenciar maiúsculas/minúsculas; não é um modelo de IA analisando o
conteúdo) — aplicadas em dois pontos de cada chamada ao modelo:

- **Entrada** (`guardrails.input`): checada contra a sua mensagem, **antes** de qualquer
  chamada ao provider.
- **Saída** (`guardrails.output`): checada contra a resposta do modelo, antes dela chegar
  até você.

Distinto do controle de `permissions` (que decide **quais ferramentas** o agente pode
executar) — guardrails filtram **conteúdo**, independente de qual tool é chamada.

## Configurar

No `agentry.settings.json` (ver [Configuração](configuracao.md#guardrails)):

```json
{
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

Cada regra tem:

- `id` — identificador único; aparece em avisos e no log de auditoria (**nunca** o texto que
  casou).
- `match` — substring a procurar.
- `action` — `"block"` ou `"redact"`.

## As duas ações

- **`block`** — substitui a mensagem inteira por um aviso fixo. Se a regra é de entrada, o
  modelo **nunca é chamado** para aquela mensagem (nenhum dado sai da máquina, se o provider
  ativo for de nuvem). Se é de saída, a resposta que chegaria até você é substituída antes
  de aparecer na tela.
- **`redact`** — mascara só o trecho que casou (substituído por um marcador de redação) e a
  conversa continua normalmente. Múltiplas regras de `redact` que casarem no mesmo texto são
  todas aplicadas.

Se uma regra `block` e uma `redact` casarem no mesmo texto, `block` sempre vence.

## Efeito no streaming

Sem nenhuma regra em `guardrails.output`, a resposta do modelo aparece na tela em tempo
real, palavra por palavra, como de costume. **Assim que você adiciona qualquer regra de
saída**, o comportamento muda: a resposta do turno é acumulada inteira, checada, e só então
aparece na tela de uma vez (já mascarada ou substituída pelo aviso, se alguma regra casou) —
o preço de garantir que nenhum texto que deveria ter sido bloqueado/mascarado chegue a ser
exibido antes da checagem rodar. Guardrails de entrada não têm esse efeito colateral (a
checagem já acontece antes de qualquer streaming começar).

## Quando usar

Guardrails são um filtro **mecânico e auditável**, não uma análise semântica — bons para
padrões conhecidos e fixos (ex.: prefixos de chave de API, domínios de e-mail internos,
identificadores sensíveis). Para revisão semântica de tarefas inteiras contra critérios mais
amplos, veja o Reviewer (trilha de Desenvolvimento,
[ADR-0015](../adr/0015-reviewer-auditoria-semantica-por-task-class.md)) — mecanismo
complementar, não sobreposto.
