<!-- Caminho relativo: docs/adr/0001-fundacao-camada-llm.md -->

# ADR 0001: Fundação da camada LLM por abstração própria sobre `reqwest`

- **Status:** Accepted
- **Data:** 2026-06-19
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** dependências, arquitetura, llm, segurança

## Contexto

O `agentry` precisa falar com múltiplos provedores: **Anthropic** (API nativa), serviços
**OpenAI-compatible** (vLLM, OpenRouter, LM Studio) e **Ollama** local. Há três caminhos:

1. **Framework de agente** (ex.: `rig`) — entrega agent loop, abstrações e RAG prontos.
2. **Cliente multi-provider** (ex.: `genai`) — camada de transporte; o loop é seu.
3. **Abstração própria fina** sobre `reqwest`.

Forças em jogo: o alvo é **homologação corporativa**, que exige **árvore de dependências
auditável** e **controle total da fronteira de rede** (egresso — ver ADR-0002). A prioridade
declarada do projeto é **confiabilidade/base bem-feita acima de velocidade**. Frameworks
grandes ampliam a superfície de auditoria e amarram o design; o controle de egresso é
justamente o que **não** se deve terceirizar. A decisão precisa ser tomada agora porque toda
a estrutura de providers, router e agent loop depende dela.

## Decisão

Fica acordada a construção de uma **abstração própria fina** — uma `trait LlmProvider`
(chat, streaming, *tool-calling*, embeddings) sobre **`reqwest`** — **sem** framework de
agente. Os adaptadores da v0.1 são: **Anthropic** (Messages API, *tool use*, streaming SSE,
*prompt caching*), **OpenAI-compatible** (cobre vLLM, OpenRouter, LM Studio) e **Ollama**.
`rig` e `genai` ficam **excluídos** como dependência de runtime da v0.1 (podem servir só de
referência de estudo).

## Consequências

- **Impacto positivo:** controle total da borda de rede (essencial para egresso/privacidade);
  árvore de dependências pequena e auditável; binário enxuto; sem *lock-in* de design.
- **Impacto negativo:** mais código de base para escrever e manter (agent loop e *tool-calling*
  próprios); responsabilidade de acompanhar mudanças de API dos provedores.
- **Trade-offs aceitos:** maior esforço inicial em troca de confiabilidade e auditabilidade.

## Diretriz de Conformidade de Código

- **Proibido:** introduzir framework de agente (`rig` ou equivalente) ou qualquer cliente que
  oculte/realize chamadas de rede fora da `trait LlmProvider`, sem um novo ADR que o autorize.
- **Proibido:** dependências com grande superfície transitiva sem justificativa registrada em ADR.
- **Obrigatório:** toda chamada a um LLM passa pela `trait LlmProvider`; toda saída de rede
  passa pelo módulo de transporte único e auditável definido no ADR-0002.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
