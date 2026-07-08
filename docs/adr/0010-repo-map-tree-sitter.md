<!-- Caminho relativo: docs/adr/0010-repo-map-tree-sitter.md -->

# ADR 0010: Repo map (estilo Aider) via `tree-sitter`, sem vector DB

- **Status:** Proposed
- **Data:** 2026-07-08
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** contexto, especialização-sem-fine-tuning, dependências, tree-sitter

## Contexto

O uso-alvo do `agentry` inclui modelos open-source pequenos (na faixa 8B–30B, ex.: família
Qwen) servidos localmente via Ollama. Esses modelos são bem piores que modelos de fronteira
em decidir sozinhos, de forma iterativa, o que buscar num repositório grande — cometem
buscas ineficientes, alucinam caminhos ou desistem cedo. É preciso apresentar "quais
arquivos/símbolos importam para esta tarefa" sem depender da capacidade de busca agenteica
do próprio modelo.

A abordagem do **Aider** (repo-map: `tree-sitter` extrai símbolos/definições; um grafo de
referências entre eles é construído; um algoritmo de ranking tipo PageRank decide quais
símbolos/arquivos são mais relevantes para a tarefa atual) é leve — não exige vector DB nem
infraestrutura de embeddings — e **complementa**, não substitui, a recuperação semântica mais
pesada do ADR-0011. Por ser mais barata, é construída primeiro.

**Maturidade verificada** (`gh repo view` + crates.io, 2026-07-08): crate `tree-sitter` —
repositório com 26.167 estrelas, push mais recente no dia da verificação, licença **MIT**,
26,9 milhões de downloads acumulados em crates.io, versão estável 0.26.10. Sem sinal de
abandono ou telemetria.

## Decisão

Fica acordada a adoção do crate `tree-sitter` para extrair símbolos (função/classe/método,
com *range* de bytes) de arquivos-fonte. Gramáticas por linguagem (`tree-sitter-rust`,
`tree-sitter-python` etc.) são adotadas **individualmente**, cada uma vetada pela mesma regra
de maturidade/licença do ADR-0004, conforme o suporte a linguagens for sendo ampliado — não
há adoção em lote.

Sobre essa extração, constrói-se um **grafo de referências** (import/uso) entre
símbolos/arquivos, e um **algoritmo de ranking** (PageRank ou equivalente) ordena a relevância
desses símbolos/arquivos dado um conjunto de "arquivos semente" (ex.: mencionados na tarefa
atual, ou já abertos na sessão).

O resultado é exposto ao agent loop como uma tool (`repo_map`), sob o gate de permissão do
MT-11 como qualquer outra tool. **A tool vem ativada por padrão**, mas é **desabilitável pelo
usuário** via extensão do `settings-schema` (chave exata a definir na implementação, ex.:
`context.repo_map.enabled`, *default* `true`).

## Consequências

- **Impacto positivo:** ajuda concreta para modelos fracos, sem infraestrutura pesada (sem
  servidor, sem vector DB, 100% local/CPU); a extração de símbolos é reaproveitada pelo
  chunking do RAG semântico (ADR-0011), evitando duplicar essa peça.
- **Impacto negativo:** o ranking depende de heurísticas estruturais (referências explícitas)
  que não capturam similaridade semântica; cobertura de linguagem limitada às gramáticas
  `tree-sitter` adotadas.
- **Trade-offs aceitos:** menos preciso que busca semântica para "código parecido sem nome
  conhecido" — tratado como complementar (o ADR-0011 cobre esse caso), não como substituto.

## Diretriz de Conformidade de Código

- **Proibido:** adicionar gramática `tree-sitter` nova sem verificação individual de
  maturidade/licença (ADR-0004); a tool `repo_map` ignorar o gate de permissão do MT-11;
  desabilitar a tool silenciosamente sem expor a flag de configuração ao usuário.
- **Obrigatório:** a tool `repo_map` respeita a flag de configuração (*default*: ativada);
  qualquer chamada de rede eventualmente introduzida por esta funcionalidade passa pelo
  Transporte único (ADR-0002) — hoje esta funcionalidade é 100% local, sem egresso.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
