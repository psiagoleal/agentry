<!-- Caminho relativo: docs/adr/0012-saida-estruturada-para-tool-calling.md -->

# ADR 0012: Saída estruturada (*constrained decoding*) para tool-calling

- **Status:** Proposed
- **Data:** 2026-07-08
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** confiabilidade, providers, especialização-sem-fine-tuning

## Contexto

Modelos open-source pequenos (8B–30B) produzem JSON de tool-call malformado com frequência
bem maior que modelos de fronteira — argumento com aspas não escapadas, campo faltando, JSON
truncado. Isso quebra o *parse* em `crate::model::ContentBlock::ToolCall`/
`crate::tools::ToolRegistry` e degrada a confiabilidade do agent loop justamente na interface
mais crítica: decidir e executar ações.

O Ollama já suporta restringir a geração a um schema JSON — o campo `format` do corpo da
requisição aceita tanto `"json"` quanto um JSON Schema completo. Usar isso para as tool-calls
elimina a maior parte da malformação **sem fine-tuning e sem dependência nova**: é um
parâmetro de API que o `OllamaProvider` (MT-08) já constrói.

## Decisão

Fica acordado que o `OllamaProvider` passa a enviar o campo `format` da API do Ollama com o
JSON Schema combinado das `tools` da `ChatRequest`, sempre que `tools` não estiver vazio —
restringindo a geração da porção de tool-call ao schema esperado.

**Ativado por padrão**, mas **desabilitável pelo usuário** via `settings-schema` (ex.:
`providers.ollama.structured_output`, *default* `true`) — necessário porque restringir a
saída pode, em alguns modelos/versões do Ollama, custar latência ou qualidade de geração, e o
usuário deve poder desligar para depuração ou modelos que não suportem bem o recurso.

## Consequências

- **Impacto positivo:** reduz drasticamente a taxa de JSON malformado em tool-calling para
  modelos pequenos, sem fine-tuning e sem dependência nova; mudança pequena e cirúrgica
  (só `crates/core/src/provider/ollama.rs`).
- **Impacto negativo:** suporte a `format` varia por modelo/versão do Ollama; em alguns casos
  pode enviesar o texto livre que acompanha a tool-call.
- **Trade-offs aceitos:** aceitar variação de suporte entre modelos — a flag de desativação
  cobre esse caso — em troca de ganho de confiabilidade no caso comum.

## Diretriz de Conformidade de Código

- **Proibido:** enviar `format` sem checar a flag de configuração; aplicar essa restrição de
  schema a outros providers (OpenAI-compatible/Anthropic têm mecanismos próprios) sem ADR
  específico — não generalizar sem verificar.
- **Obrigatório:** a flag é lida pela mesma camada de configuração em três níveis do MT-04;
  ausência de configuração cai no *default* (`true`); se `format` causar erro do provider
  (ex.: modelo sem suporte), o erro é reportado como `ProviderError`, nunca mascarado ou
  ignorado em silêncio.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
