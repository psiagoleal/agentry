<!-- Caminho relativo: docs/adr/0008-parametros-de-chamada-e-presets-por-task-class.md -->

# ADR 0008: Parâmetros de chamada de LLM e presets de modelo por task-class

- **Status:** Proposed
- **Data:** 2026-07-07
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** configuração, providers, router, interop

## Contexto

Hoje `ChatRequest` (MT-03) só carrega `model`/`messages`/`tools`/`max_tokens`, e `Settings`
(MT-04) só carrega `model`/`max_tokens`. Não há como o usuário configurar `temperature`,
`top_p` ou um *system prompt* padrão, nem variar esses parâmetros conforme o tipo de tarefa
— algo que o **Router / Policy Engine** (MT-09, ainda não implementado) precisará fazer ao
mapear `task-class → (provider, modelo, classe de egresso)`. A motivação concreta citada foi
o conceito de **Modelfile** do Ollama (arquivo que empacota modelo + parâmetros + *template*
de sistema) — mas um Modelfile é formato **proprietário de um provider**; adotá-lo como
mecanismo de configuração do `agentry` acoplaria o núcleo a um único provider, o que o
ADR-0001 já rejeita para a camada de providers em geral.

## Decisão

Fica acordada a extensão do `settings-schema` com uma seção de **presets de modelo por
task-class** (nome de chave a definir no ADR de esquema específico, quando implementado):
cada preset associa uma `task-class` a `{ provider?, model, temperature?, top_p?,
system_prompt?, max_tokens? }`. O Router (MT-09) resolve, para cada tarefa, não só
`(provider, modelo, classe de egresso)` como já previsto, mas também os parâmetros de
chamada default daquela classe de tarefa.

`system_prompt` **não** vira um campo novo em `ChatRequest` — continua sendo uma
`Message::system(...)` comum (MT-02); o preset só define *qual* texto usar por padrão, e é o
Router/Session quem a antepõe à conversa. `temperature`/`top_p`, que não têm equivalente em
`Message`, ganham campos opcionais em `ChatRequest` (MT-03), com ausência caindo no *default*
de cada provider — nenhum campo é obrigatório.

O Modelfile do Ollama fica **fora de escopo** como mecanismo de configuração: ao integrar o
adapter Ollama (MT-08, já implementado) com presets, o preset do `agentry` é quem manda e é
traduzido para o campo `options` da API do Ollama — nunca lido de um Modelfile externo.

## Consequências

- **Impacto positivo:** parâmetros de chamada portáveis entre providers (Ollama,
  OpenAI-compatible, Anthropic), configuráveis por perfil/projeto via o mesmo mecanismo de
  camadas do MT-04; comportamento determinístico por tipo de tarefa (ex.: temperatura baixa
  para geração de código, mais alta para *brainstorm*).
- **Impacto negativo:** mais uma seção do `settings-schema` para manter e documentar; exige
  coordenação com o `profiles` (dono do esquema) antes de congelar o formato definitivo.
- **Trade-offs aceitos:** overhead de configuração em troca de previsibilidade e
  auditabilidade por classe de tarefa; adiar suporte a formatos nativos de provider (ex.:
  Modelfile) em favor de um formato próprio e portável.

## Diretriz de Conformidade de Código

- **Proibido:** adapters aceitarem parâmetros de chamada fora do `ChatRequest` padronizado
  (parâmetro solto ou *hardcoded* por provider); usar o Modelfile nativo do Ollama (ou
  formato equivalente de outro provider) como fonte de verdade de configuração do `agentry`.
- **Obrigatório:** presets vivem na mesma camada perfil→projeto→env do MT-04 (união/override
  consistente com `Settings::merged_over`); campo de preset ausente cai no *default* do
  provider, nunca em pânico ou erro; mudança de esquema correspondente registrada no
  `exchange-log` (`docs/interop/exchange-log.md`).

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
