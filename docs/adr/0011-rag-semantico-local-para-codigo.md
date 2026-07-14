<!-- Caminho relativo: docs/adr/0011-rag-semantico-local-para-codigo.md -->

# ADR 0011: RAG semântico local para código (chunking + busca híbrida + reranker)

- **Status:** Accepted
- **Data:** 2026-07-08
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** contexto, especialização-sem-fine-tuning, dependências, rag, embeddings

## Contexto

O repo-map (ADR-0010) é leve, mas limitado a relações estruturais explícitas (imports,
chamadas) — não encontra código semanticamente parecido cujo nome ou localização o modelo
não conhece de antemão. Para os modelos open-source pequenos que o `agentry` tem como alvo
de uso local (8B–30B via Ollama), que também têm dificuldade de sintetizar buscas iterativas
eficazes, recuperação semântica pré-computada faz parte do trabalho que o modelo não
consegue fazer bem sozinho — o inverso do que vale para um modelo de fronteira, que compensa
com raciocínio próprio.

Chunking por tamanho fixo de token quebra funções no meio e perde contexto sintático; deve
ser **AST-aware**, reaproveitando a extração de símbolos já decidida no ADR-0010
(`tree-sitter`) em vez de duplicá-la. Busca puramente semântica erra em identificadores
exatos (nomes de função/variável importam); busca **híbrida** (semântica + lexical) com
*reranking* dá mais precisão, à custa de mais uma passada de latência.

**Maturidade verificada** (`gh repo view` + crates.io, 2026-07-08):

| Crate | Estrelas do repo | Downloads (crates.io) | Versão | Licença | Último push |
|---|---|---|---|---|---|
| `tantivy` | 15.523 | 15.085.040 | 0.26.1 | MIT | dia da verificação |
| `lancedb` | 10.826 | 639.123 | 0.31.0 | Apache-2.0 | dia da verificação |

Ambos escolhidos sobre as alternativas cogitadas inicialmente (Chroma, `rank_bm25`) por
serem **nativos em Rust** — sem ponte FFI/Python — mantendo a árvore de dependências
auditável (ADR-0001) e evitando um processo de servidor externo (ambos embutidos/*in-process*).

## Decisão

Fica acordada a construção de um pipeline de RAG local:

1. **Chunking AST-aware**, reaproveitando a extração de símbolos do ADR-0010 (função/classe/
   método como unidade de chunk) — não se duplica a extração.
2. **Índice lexical** via `tantivy` (BM25).
3. **Índice semântico** via `lancedb`, com vetores gerados através do método já existente
   `LlmProvider::embeddings` (MT-03) — **nenhum adapter novo de embeddings**; reaproveita o
   contrato já estabelecido.
4. **Busca híbrida**, combinando os dois índices (*reciprocal rank fusion* ou equivalente).
5. **Reranking** cross-encoder do top-K antes de montar o prompt final — o modelo de
   reranking também é servido localmente (ex.: via Ollama), reaproveitando a mesma `trait
   LlmProvider`, **não** uma API nova.
6. **Indexação incremental** via `git diff`/observação de *filesystem* — reembeda só
   arquivos alterados; **nunca** reindexa o repositório inteiro a cada mudança.

O resultado é exposto ao agent loop como uma tool (`code_search`), sob o gate de permissão do
MT-11. **Ativada por padrão**, mas **desabilitável pelo usuário** via `settings-schema` (ex.:
`context.semantic_rag.enabled`, *default* `true`) — desligar a tool também deve desligar a
indexação de fundo, para não gastar disco/CPU à toa.

## Consequências

- **Impacto positivo:** ataca diretamente a limitação de recall/precisão de modelos pequenos
  sobre bases de código grandes; reaproveita `tree-sitter` (ADR-0010) e
  `LlmProvider::embeddings` (MT-03) já existentes, minimizando a dependência nova de fato
  introduzida (só `tantivy` + `lancedb`).
- **Impacto negativo:** infraestrutura mais pesada que o repo-map (índices em disco, processo
  de reindexação); duas passadas de busca + reranking aumentam latência; mais uma superfície
  de configuração e risco de desalinhamento (índice desatualizado se a indexação incremental
  falhar silenciosamente).
- **Trade-offs aceitos:** latência/complexidade extra em troca de precisão — mitigado por
  manter o repo-map (ADR-0010) como *fallback* mais barato e por indexação incremental (nunca
  reindexa tudo).

## Diretriz de Conformidade de Código

- **Proibido:** gerar embeddings fora de `LlmProvider::embeddings`; qualquer chamada de rede
  do pipeline de RAG (embeddings ou reranking remotos) fora do Transporte único (ADR-0002) —
  se o modelo for remoto, respeita a allowlist/classe de egresso como qualquer outro
  provider; adicionar dependência de vetor store/busca nova sem verificação de maturidade
  (ADR-0004); indexação falhar silenciosamente sem log observável.
- **Obrigatório:** a tool `code_search` respeita a flag de configuração (*default*: ativada);
  indexação incremental — nunca reindexar tudo a cada mudança; reaproveitar o chunking do
  ADR-0010, não duplicar a extração de símbolos.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
