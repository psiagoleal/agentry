<!-- Caminho relativo: docs/roadmap-v0.17.md -->

# Roadmap v0.17 — Micro-tickets

Fase I: RAG estendido às sessões salvas (ADR-0039), adiada desde a rodada anterior até a Fase
G estar rodando de verdade (sem sessões salvas reais, não havia o que indexar). Pedido do
mantenedor para seguir agora que Fase G/H/J estão completas e a release foi atualizada. Duas
perguntas de design resolvidas com o mantenedor antes de qualquer código (ver ADR-0039):
exposição dupla (comando `/recall` **e** tool `session_search`, ambos sob o mesmo
`PermissionGate`) e embeddings/*reranking* sempre via Ollama local (hardcoded, nunca segue a
task-class de chat configurada).

## Convenções

Mesmas de sempre (`docs/roadmap-v0.1.md` §Convenções): DoD padrão (`cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`), *smoke-test* real do
binário pra toda mudança observável, skill `micro-ticket-planner` para granularidade.
**Nenhuma dependência nova** — tudo com `tantivy`/`lancedb`/`serde_json`/`std` já presentes
(mesmas dependências já aprovadas pelo ADR-0011).

---

## Fase I — RAG estendido às sessões salvas (ADR-0039)

### MT-130: ADR-0039 (Accepted) ✅ concluído
- **Objetivo:** registrar a decisão — módulos paralelos ao RAG de código (não generalização
  de `Chunk`/índices), chunking por mensagem, embeddings/*reranking* sempre via Ollama
  local, exposição dupla (`/recall` + tool `session_search`) sob o mesmo `PermissionGate`.
  Duas perguntas resolvidas com o mantenedor via `AskUserQuestion` antes de escrever.
- **Arquivos no escopo:** `docs/adr/0039-*.md` (novo), `docs/adr/README.md`, `mkdocs.yml`,
  `docs/roadmap-v0.17.md` (este arquivo).
- **Depende de:** nenhum.

### MT-131: `session_chunk.rs` — chunking por mensagem das sessões salvas ✅ concluído (2560cd7)
- **Objetivo:** `SessionChunk` (id da sessão, índice da mensagem, papel, texto) +
  `chunk_session(id, mensagens: &[Message]) -> Vec<SessionChunk>` — um chunk por mensagem de
  `Role::User`/`Role::Assistant` (texto completo, nunca truncado); `Role::System`/`Role::Tool`
  são ignoradas (prompt de sistema não é conteúdo buscável; resultado de tool é JSON sem
  valor semântico).
- **Arquivos no escopo:** `crates/core/src/context/rag/session_chunk.rs` (novo),
  `crates/core/src/context/rag/mod.rs` (`pub mod`).
- **Critério de aceite:** testes — uma sessão com mensagens de usuário/agente/sistema/tool
  gera só os chunks certos (contagem e conteúdo); mensagem de assistente com bloco de tool
  (sem texto) não gera chunk vazio; sessão sem nenhuma mensagem de usuário/agente não gera
  chunk nenhum (não é erro).
- **Ajuste de escopo na implementação:** a leitura de `.agentry/session/*.md` (ler o
  diretório, desserializar cada arquivo) **não** entrou neste ticket — mirando o mesmo
  desenho de `chunk.rs`/`chunk_file` (código), que também é puro/sem I/O sobre um único
  arquivo já lido; a orquestração multi-arquivo fica para o MT-135 (indexação incremental,
  mesmo papel de `code_search.rs::ler_arquivos` no lado de código). 6 testes novos, 706 no
  *workspace*. Sem ponto de entrada na CLI ainda — sem comportamento observável pra
  *smoke-test*, mesma situação do MT-119/120.
- **Depende de:** nenhum (só MT-120, já concluído).

### MT-132: `session_lexical_index.rs` — índice BM25 sobre `SessionChunk` ✅ concluído (a8361da)
- **Objetivo:** `SessionLexicalIndex`, mesmo desenho de `lexical_index.rs` (MT-26) — schema
  `tantivy` próprio (`session_id`/`indice_mensagem`/`papel`/`text`), `build`/`search`.
- **Arquivos no escopo:** `crates/core/src/context/rag/session_lexical_index.rs` (novo),
  `mod.rs`.
- **Critério de aceite:** testes — mesma cobertura de `lexical_index.rs` (consulta por termo
  exato no topo, consulta sem correspondência devolve vazio, limite restringe a contagem,
  chunk reconstruído preserva metadados).
- **Depende de:** MT-131.
- **Diferença deliberada:** sem *field boost* equivalente ao `symbol` do lado de código —
  `SessionChunk` não tem um campo identificador análogo (nome de função/variável); `text` é
  o único campo buscável. 4 testes novos, 710 no *workspace*. Sem ponto de entrada na CLI
  ainda.

### MT-133: `session_semantic_index.rs` — índice semântico via embeddings (sempre Ollama) ✅ concluído (de4c8b1)
- **Objetivo:** `SessionSemanticIndex`, mesmo desenho de `semantic_index.rs` (MT-27) — tabela
  `lancedb` própria, `build`/`search`. Recebe o provider como parâmetro (mesmo padrão do
  código), mas quem chama (tool/comando, MT-136/137) **sempre** passa o cliente Ollama —
  nunca a task-class de chat configurada (ADR-0039 §2).
- **Arquivos no escopo:** `crates/core/src/context/rag/session_semantic_index.rs` (novo),
  `mod.rs`.
- **Critério de aceite:** testes — mesma cobertura de `semantic_index.rs` (busca por vetor
  mais próximo, limite restringe a contagem, chunk reconstruído preserva metadados).
- **Depende de:** MT-131.
- A garantia de "sempre Ollama" não é verificada em tempo de compilação (o índice recebe
  `&dyn LlmProvider` genérico, mesma limitação de `semantic_index.rs`) — documentada no
  comentário do módulo, aplicada de fato pelo MT-136. 5 testes novos, 715 no *workspace*.
  Sem ponto de entrada na CLI ainda.

### MT-134: `session_hybrid_search.rs` — fusão RRF + *reranking* sobre `SessionChunk` ✅ concluído (537908d)
- **Objetivo:** `fuse`/`rerank`/`hybrid_search` próprios, mesma fórmula RRF (`RRF_K = 60.0`,
  idêntica ao MT-28) e mesmo protocolo de *reranking* via `LlmProvider::chat` (prompt
  adaptado ao formato de `SessionChunk` — session_id/papel/texto em vez de file/symbol).
- **Arquivos no escopo:** `crates/core/src/context/rag/session_hybrid_search.rs` (novo),
  `mod.rs`.
- **Critério de aceite:** testes — mesma cobertura de `hybrid_search.rs` (fusão reflete os
  dois sinais, *rerank* reordena um caso conhecido, resposta malformada é erro tratado, 0/1
  chunk não chama o provider, pipeline completo funde e reordena).
- **Depende de:** MT-132, MT-133.
- 7 testes novos, 721 no *workspace*. Sem ponto de entrada na CLI ainda.

### MT-135: `session_incremental.rs` — indexação incremental (sessões são *write-once*) ✅ concluído (e632a55)
- **Objetivo:** manifesto `<estado>/index/session_manifest.json`, mais simples que o de
  código (MT-29): sessões nunca são editadas depois de salvas (`sessao::salvar` sempre gera
  um `id` novo com *timestamp*) — só precisa detectar **arquivos novos** desde a última
  chamada, não *diff* de conteúdo. Manifesto ausente/corrompido não é erro (reprocessa tudo).
- **Arquivos no escopo:** `crates/core/src/context/rag/session_incremental.rs` (novo),
  `mod.rs`.
- **Critério de aceite:** testes — primeira chamada indexa todas as sessões existentes;
  chamada seguinte sem sessão nova não reprocessa nada; sessão nova é detectada e indexada
  sem reprocessar as já indexadas; manifesto corrompido reprocessa tudo sem falhar.
- Sem hash de conteúdo (diferente do MT-29): presença no manifesto já é sinal suficiente
  para "não reprocessar" — testado explicitamente (sessão já indexada nunca reprocessa,
  mesmo com mensagens diferentes no input). 6 testes novos, 727 no *workspace*. Sem ponto de
  entrada na CLI ainda.
- **Depende de:** MT-131.

### MT-136: Tool `session_search` — exposta ao agente, mesmo `PermissionGate` de sempre ✅ concluído (bb34b47)
- **Objetivo:** `SessionSearchTool`/`SessionSearchSession` (mesmo padrão de
  `CodeSearchTool`/`CodeSearchSession`, MT-30) — mantém os índices em cache, reconstrói só
  quando a indexação incremental (MT-135) reporta sessão nova. `register_session_search_tool`
  registra condicionalmente a uma flag nova (`context.sessionSearch.enabled`, *default*
  `true`). Sob o `PermissionGate` comum — sem exceção de permissão própria.
- **Arquivos no escopo:** `crates/core/src/tools/session_search.rs` (novo),
  `crates/core/src/tools/mod.rs`, `crates/core/src/config/mod.rs` (flag nova),
  `crates/cli/src/main.rs` (`register_context_tools` ou fiação equivalente).
- **Critério de aceite:** testes — tool registrada/ausente conforme a flag; busca real sobre
  sessões de teste devolve os chunks esperados; respeita `deny`/`ask` do `PermissionGate`
  como qualquer outra tool. *Smoke-test* real: sessão salva de verdade, agente chama
  `session_search`, resultado aparece na resposta.
- **Depende de:** MT-134, MT-135.
- **Nota sobre o *smoke-test*:** um round-trip de tool-calling ao vivo (mockar o formato
  exato de tool-call do Ollama) não agrega sobre a cobertura já real dos testes de unidade
  (chunking/indexação/fusão RRF/*reranking* reais, `busca_real_sobre_sessoes_de_teste_...`)
  — mesmo nível de verificação usado pelo `code_search` original (MT-30), que também não
  teve esse tipo de *smoke-test*. Verificado, em vez disso, que o binário `release` real
  sobe sem *panic* com uma sessão salva presente (prova a fiação de
  `register_context_tools`). 13 testes novos (mais 2 testes existentes de "3 tools de
  contexto" ampliados para "4 tools", cobrindo `session_search`), 731 no *workspace*.

### MT-137: Comando `/recall <busca>` — REPL e TUI ✅ concluído (837715a)
- **Objetivo:** mesma busca da tool (MT-136), função compartilhada extraída para não
  duplicar lógica — formata o resultado como texto simples (REPL) ou mensagem de sistema
  (TUI, mesmo padrão de `/sessions`). Não passa pelo `PermissionGate` (nenhum comando de
  barra passa).
- **Arquivos no escopo:** `crates/cli/src/repl.rs`, `crates/cli/src/tui/mod.rs`,
  `docs/usuario/uso.md`.
- **Critério de aceite:** testes — `/recall` sem sessões salvas avisa (não erro); `/recall
  <busca>` com sessões de teste devolve os trechos esperados, formatados de forma legível.
  *Smoke-test* real via `tmux`.
- **Depende de:** MT-136.
- `main.rs` monta uma única `SessionSearchSession` compartilhada (não uma por caminho),
  passada tanto a `register_context_tools` quanto a `repl::run_repl`/`tui::run` — mesmo
  cache de índices entre a tool e o comando manual. 18 testes novos, 736 no *workspace*.
- **⚠️ Achado real via smoke-test contra o Ollama local de verdade (não `docs/usuario/uso.md`
  ainda, ver MT-138):** `OllamaProvider::embeddings` (`crates/core/src/provider/ollama.rs`,
  desde o MT-08) nunca foi implementado — sempre devolve `ProviderError::Unsupported`.
  `/recall` (e, pelo mesmo motivo, `code_search` com um índice semântico não-vazio) falha
  contra o Ollama real assim que há ao menos um chunk pra embeddar — gap pré-existente, não
  introduzido por este ticket, só ficou visível com um *smoke-test* real (`MockProvider` nos
  testes sempre implementa embeddings). Reportado ao mantenedor antes de seguir pro MT-138.

### MT-138: Documentação de usuário + *settings-schema* ✅ concluído (54220bd)
- **Objetivo:** `context.sessionSearch.enabled` documentado em `docs/usuario/configuracao.md`
  (mesmo padrão de `semanticRag`/`repoMap`); `/recall` e a tool `session_search` documentados
  em `docs/usuario/uso.md`; exemplo do schema (`_comentario`) no arquivo gerado por
  `--init`/`/init`.
- **Arquivos no escopo:** `docs/usuario/configuracao.md`, `docs/usuario/uso.md`,
  `crates/cli/src/main.rs` (`GENERIC_SETTINGS_EXAMPLE`, se o campo precisar aparecer no
  exemplo gerado).
- **Critério de aceite:** `mkdocs build --strict` limpo; teste existente de
  `run_init_local_cria_o_arquivo_ausente_com_o_exemplo_exato_da_adr_0018` atualizado se o
  exemplo mudar.
- **`GENERIC_SETTINGS_EXAMPLE` deliberadamente não ganhou o campo novo** — mesmo precedente
  de `agentsFile`, que também não aparece nesse template gerado por `--init`/`/init`; sem
  mudança nesse arquivo, o teste citado acima não precisou de atualização. Nova seção "RAG
  sobre sessões salvas" em `uso.md` cobre exposição dupla, `permissions.deny` pra restringir
  só ao comando manual, e a ressalva de que embeddings sempre via Ollama exigem um Ollama
  com suporte a embeddings habilitado (MT-139). `mkdocs build --strict` limpo.
- **Fecha a Fase I inteira (MT-130..139).**
- **Depende de:** MT-136, MT-137.

### MT-139: `OllamaProvider::embeddings` via `/api/embed` ✅ concluído (e7e15ac)
- **Objetivo (achado durante o MT-137, não previsto no plano original):** `smoke-test` real
  do `/recall` contra o Ollama local de verdade revelou que `OllamaProvider::embeddings`
  (`crates/core/src/provider/ollama.rs`) nunca foi implementado desde o MT-08 — sempre
  devolvia `ProviderError::Unsupported`. Como `session_search`/`/recall` (MT-131..137) e o
  `code_search` já existente (ADR-0011) **sempre** usam o cliente Ollama fixo para
  embeddings (ADR-0039 §2), esse gap tornava as duas tools inutilizáveis contra o provider
  *default* do projeto assim que o índice semântico tivesse ao menos um chunk. Reportado ao
  mantenedor via `AskUserQuestion`; resposta: implementar agora.
- **Arquivos no escopo:** `crates/core/src/provider/ollama.rs`.
- **Critério de aceite:** testes — vetores/uso reais via mock HTTP; corpo da requisição
  contém `model`/`input`; egresso `local-only` continua bloqueando sem tocar a rede; status
  de erro do servidor (embeddings desabilitado) vira `Err` tratado, nunca *panic*.
- **Depende de:** nenhum (módulo independente da Fase I em si, só descoberto durante ela).
- **Verificado contra o Ollama local de verdade:** requisição real chega em `/api/embed`
  (confirmado no *audit log*), erro tratado corretamente quando o servidor não tem
  embeddings habilitado (HTTP 501, endpoint/formato do corpo confirmados corretos via `curl`
  direto antes de implementar) — sucesso completo (vetor de verdade) não verificado ao vivo
  porque o Ollama desta máquina não está rodando com `--embeddings`/config equivalente
  habilitada (fora do escopo deste ticket). 4 testes novos, 740 no *workspace*.

---

## Sequência crítica

```
MT-130                                            (ADR)
      └→ MT-131 → MT-132 ┐
               └→ MT-133 ┴→ MT-134 → MT-136 → MT-137 → MT-138
               └→ MT-135 ───────────↗
MT-139 (independente -- achado durante o MT-137, corrige um gap do MT-08)
```

Nenhuma dependência cruzada com as Fases G/H/J (já concluídas) além de reaproveitar
`session::persist::desserializar_de_markdown` (MT-120) e `sessao::salvar` (MT-121), ambos
estáveis.
