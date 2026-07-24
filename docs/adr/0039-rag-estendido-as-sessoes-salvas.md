<!-- Caminho relativo: docs/adr/0039-rag-estendido-as-sessoes-salvas.md -->

# ADR 0039: RAG estendido às sessões salvas

- **Status:** Accepted
- **Data:** 2026-07-24
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** rag, embeddings, privacidade, egresso, sessão

## Contexto

A Fase G (ADR-0036) deu ao `agentry` sessões salvas em Markdown (`.agentry/session/*.md`,
opt-in via `/save`). O roadmap já previa, desde então, uma Fase I para estender o pipeline
híbrido de RAG do ADR-0011 (chunking + `tantivy`/BM25 + `lancedb`/embeddings + *reciprocal
rank fusion* + *reranking* via chat) a esse novo corpus — deliberadamente adiada até a Fase G
estar rodando de verdade (sem sessões salvas reais, não há o que indexar). Com Fase G/H/J
completas e uma release atualizada, o mantenedor pediu para seguir.

Duas questões de design ficaram deliberadamente em aberto no texto original da Fase I e
foram levantadas ao mantenedor via `AskUserQuestion` antes de qualquer código:

1. **Exposição:** o agente deveria poder buscar sozinho em conversas passadas (tool
   autônoma, mesmo padrão do `code_search`) ou só o usuário deveria decidir quando puxar
   esse contexto (comando explícito)? Sessões salvas já passaram por um opt-in explícito
   (ADR-0036) justamente por serem potencialmente sensíveis — dar ao modelo acesso
   automático a conversas antigas é uma escolha de política diferente de dar acesso
   automático a código-fonte.
2. **Egresso de embeddings:** o conteúdo de sessão deveria sempre virar embedding via um
   provider local (Ollama), independente da task-class de chat configurada, ou seguir a
   mesma classe de egresso já resolvida (permitindo, em tese, um provider de embeddings na
   nuvem se o projeto estiver configurado como `cloud-ok`)?

**Resposta do mantenedor (ambas as perguntas, 2026-07-24):**

1. **Os dois** — comando do usuário (`/recall <busca>`) e tool para o agente
   (`session_search`), sob o **mesmo** `PermissionGate` (`allow`/`ask`/`deny`) que já rege
   qualquer outra tool — quem quiser só o comando manual pode configurar
   `permissions.deny: ["session_search"]` (ou `ask`) sem perder o comando.
2. **Sempre local-only, hardcoded** — nunca segue a classe de egresso configurada para chat.

## Decisão

### 1. Módulos paralelos, não generalização do RAG de código

`Chunk`/`LexicalIndex`/`SemanticIndex`/`hybrid_search`/`IncrementalIndexer` (ADR-0011) são
todos tipados diretamente para o domínio de código (`file`/`symbol`/`kind: SymbolKind`/
`range: Range<usize>`) — `kind` em particular não tem equivalente sensato numa mensagem de
conversa. Generalizar essas structs (torná-las genéricas sobre um tipo de metadado) tocaria
código já estável e testado (usado hoje por `code_search`) por um benefício que não se
paga: os dois domínios têm forma de dado genuinamente diferente, e a técnica (schema
`tantivy`, tabela `lancedb`, RRF, *reranking* via chat) é o que se repete, não o *schema* dos
dados. Fica decidido reaproveitar a **técnica**, não o **tipo**, via um conjunto paralelo de
módulos em `crates/core/src/context/rag/`:

- `session_chunk.rs` — `SessionChunk` (id da sessão, índice da mensagem, papel, texto) +
  `chunk_session`, chunking **por mensagem** (não por sessão inteira, nem por tamanho fixo
  de token) — reaproveita `session::persist::desserializar_de_markdown` (MT-120) para obter
  o `Vec<Message>` de cada sessão; mensagens de `Role::System`/`Role::Tool` são ignoradas
  (prompt de sistema não é conteúdo de conversa buscável; resultado de tool é JSON, sem
  valor semântico de busca) — só `Role::User`/`Role::Assistant` viram chunk, um por mensagem
  (texto completo da mensagem, nunca truncado, mesmo espírito de "chunk nunca quebra a
  unidade original" do MT-25).
- `session_lexical_index.rs` — `SessionLexicalIndex`, mesmo desenho de `lexical_index.rs`
  (schema `tantivy` próprio, campos `session_id`/`indice_mensagem`/`papel`/`text`).
- `session_semantic_index.rs` — `SessionSemanticIndex`, mesmo desenho de
  `semantic_index.rs` (tabela `lancedb` própria).
- `session_hybrid_search.rs` — `fuse`/`rerank`/`hybrid_search` próprios, mesma fórmula RRF
  (constante `RRF_K = 60.0`, idêntica) e mesmo protocolo de *reranking* via
  `LlmProvider::chat` (prompt adaptado ao formato de `SessionChunk`).
- `session_incremental.rs` — indexação incremental, **mais simples** que a de código:
  arquivos de sessão são **write-once** (`sessao::salvar` sempre gera um `id` novo com
  *timestamp*, nunca sobrescreve um arquivo existente — confirmado em
  `crates/cli/src/sessao.rs`) — o manifesto só precisa detectar **arquivos novos** desde a
  última chamada, não *diff* de conteúdo mutável. Mesmo formato de manifesto
  (`<estado>/index/session_manifest.json`) e mesma filosofia de "manifesto ausente/corrompido
  não é erro, reprocessa tudo" do MT-29.

### 2. Embeddings/reranking sempre via Ollama local, hardcoded

Resposta 2 do mantenedor. Reaproveita literalmente o mesmo `Arc<dyn LlmProvider>` do Ollama
já usado hoje por `register_context_tools` (`crates/cli/src/main.rs`) para o `code_search` —
**não** um mecanismo novo: o `code_search` de hoje **já** ignora a task-class/provider de
chat configurada e usa sempre o cliente Ollama para embeddings/reranking (ver
`register_context_tools`, que recebe `ollama_provider` fixo). A Fase I só estende esse
mesmo padrão já em vigor ao novo par de índices — nenhuma bifurcação de comportamento entre
"RAG de código" e "RAG de sessão" quanto a isso. Se o Ollama não estiver disponível/
configurado, a indexação/busca de sessão falha com erro tratado (mesmo padrão de qualquer
chamada ao provider), nunca cai silenciosamente para outro provider.

### 3. Exposição dupla, mesmo `PermissionGate`

Resposta 1 do mantenedor.

- **Tool `session_search`** (`crates/core/src/tools/session_search.rs`) — mesmo padrão do
  `code_search` (MT-30): registrada condicionalmente por uma flag nova
  (`context.sessionSearch.enabled`, *default* `true`, mesmo padrão de
  `semantic_rag`/`repoMap`), sob o `PermissionGate` comum a qualquer tool — `allow` (padrão),
  `ask` ou `deny` continuam funcionando sem nenhuma exceção especial só para esta tool.
- **Comando `/recall <busca>`** (REPL + TUI) — chama a **mesma** função de busca que a tool
  usa (extraída para não duplicar a lógica entre os dois caminhos), formatando o resultado
  como texto simples (REPL) ou mensagem de sistema (TUI, mesmo padrão de `/sessions`). Não
  passa pelo `PermissionGate` (nenhum comando de barra passa — `/save`/`/remember`/`/sessions`
  também não), já que é um ato explícito do usuário, não uma decisão autônoma do agente.

### 4. Corpus e escopo

Só sessões já salvas em `.agentry/session/*.md` (nunca a conversa em andamento, que ainda
não foi persistida — ADR-0032/0036 continuam regendo isso). Sem sessões salvas, a busca
devolve lista vazia (não erro) — mesmo espírito de "sem conteúdo, sem falha" já usado pelo
`code_search` num repositório sem arquivos indexáveis.

## Consequências

- **Positivas:** conversas antigas viram contexto recuperável, tanto para o usuário
  (`/recall`) quanto para o agente (`session_search`, sob controle de permissão);
  reaproveita uma técnica já validada (ADR-0011) em vez de inventar uma nova; a garantia de
  egresso local para conteúdo de sessão nunca depende de o usuário lembrar de configurar
  algo — é o comportamento *default*, hardcoded, mesmo padrão já em vigor para `code_search`.
- **Negativas/riscos aceitos:** duplicação de código entre os módulos de RAG de código e de
  sessão (schema `tantivy` parecido, mesma fórmula RRF) — aceito deliberadamente em troca de
  não arriscar o caminho estável de `code_search` com uma generalização prematura.
  `session_search` como tool dá ao modelo acesso a conteúdo potencialmente sensível de
  conversas passadas — mitigado pelo mesmo `PermissionGate` de qualquer tool (o usuário pode
  restringir a `ask`/`deny` no `agentry.settings.json` sem perder o comando manual).
- **Fora de escopo:** rotação/expiração de sessões antigas do índice; UI dedicada para
  navegar resultados (texto simples, mesmo padrão de `/sessions`); busca cruzando múltiplos
  workspaces (`state_dir` já restringe ao projeto atual, ADR-0017, sem mudança aqui).

## Diretriz de Conformidade de Código

- **Obrigatório:** chunking de sessão sempre por mensagem individual (nunca a sessão
  inteira como um chunk só, nunca por tamanho fixo de token); toda chamada de
  embeddings/reranking sobre conteúdo de sessão usa o provider Ollama local, nunca a
  task-class/provider de chat configurada nem qualquer candidato que dependa de resolução
  de egresso além de `local-only`; `session_search` (tool) passa pelo mesmo
  `PermissionGate` de qualquer outra tool, sem exceção; `/recall` e `session_search`
  chamam a mesma função de busca subjacente, nunca duas implementações divergentes.
- **Proibido:** generalizar `Chunk`/`LexicalIndex`/`SemanticIndex`/`hybrid_search`/
  `IncrementalIndexer` (ADR-0011) para servir os dois domínios — módulos de sessão são
  paralelos, próprios; indexar a conversa em andamento (ainda não salva) — só
  `.agentry/session/*.md` já materializado é corpus válido; qualquer chamada de
  embeddings/reranking de conteúdo de sessão sair da máquina por um canal que não seja o
  provider Ollama local.
