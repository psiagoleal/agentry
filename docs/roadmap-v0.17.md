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

### MT-134: `session_hybrid_search.rs` — fusão RRF + *reranking* sobre `SessionChunk`
- **Objetivo:** `fuse`/`rerank`/`hybrid_search` próprios, mesma fórmula RRF (`RRF_K = 60.0`,
  idêntica ao MT-28) e mesmo protocolo de *reranking* via `LlmProvider::chat` (prompt
  adaptado ao formato de `SessionChunk` — session_id/papel/texto em vez de file/symbol).
- **Arquivos no escopo:** `crates/core/src/context/rag/session_hybrid_search.rs` (novo),
  `mod.rs`.
- **Critério de aceite:** testes — mesma cobertura de `hybrid_search.rs` (fusão reflete os
  dois sinais, *rerank* reordena um caso conhecido, resposta malformada é erro tratado, 0/1
  chunk não chama o provider, pipeline completo funde e reordena).
- **Depende de:** MT-132, MT-133.

### MT-135: `session_incremental.rs` — indexação incremental (sessões são *write-once*)
- **Objetivo:** manifesto `<estado>/index/session_manifest.json`, mais simples que o de
  código (MT-29): sessões nunca são editadas depois de salvas (`sessao::salvar` sempre gera
  um `id` novo com *timestamp*) — só precisa detectar **arquivos novos** desde a última
  chamada, não *diff* de conteúdo. Manifesto ausente/corrompido não é erro (reprocessa tudo).
- **Arquivos no escopo:** `crates/core/src/context/rag/session_incremental.rs` (novo),
  `mod.rs`.
- **Critério de aceite:** testes — primeira chamada indexa todas as sessões existentes;
  chamada seguinte sem sessão nova não reprocessa nada; sessão nova é detectada e indexada
  sem reprocessar as já indexadas; manifesto corrompido reprocessa tudo sem falhar.
- **Depende de:** MT-131.

### MT-136: Tool `session_search` — exposta ao agente, mesmo `PermissionGate` de sempre
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

### MT-137: Comando `/recall <busca>` — REPL e TUI
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

### MT-138: Documentação de usuário + *settings-schema*
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
- **Depende de:** MT-136, MT-137.

---

## Sequência crítica

```
MT-130                                            (ADR)
      └→ MT-131 → MT-132 ┐
               └→ MT-133 ┴→ MT-134 → MT-136 → MT-137 → MT-138
               └→ MT-135 ───────────↗
```

Nenhuma dependência cruzada com as Fases G/H/J (já concluídas) além de reaproveitar
`session::persist::desserializar_de_markdown` (MT-120) e `sessao::salvar` (MT-121), ambos
estáveis.
