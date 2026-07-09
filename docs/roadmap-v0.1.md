<!-- Caminho relativo: docs/roadmap-v0.1.md -->

# Roadmap v0.1 — Micro-tickets

Backlog autocontido para a v0.1 do `agentry`. Cada micro-ticket cabe em um ciclo limpo de
contexto (ver skill `micro-ticket-planner`). Ordem pensada para que o **transporte/egresso**
(ADR-0002) exista **antes** de qualquer adapter tocar a rede.

## Convenções

- **Critério de aceite padrão (DoD):** `cargo fmt --check`, `cargo clippy -- -D warnings` e
  `cargo test` passam (no CI). Critérios específicos abaixo somam-se a este.
- **Crates convencionais** (`tokio`, `clap`, `serde`, `toml`, `reqwest`) seguem o stack de
  `docs/architecture.md`. Qualquer dependência **consequente** nova exige ADR (regra do
  ADR-0004). Bibliotecas de teste (mock HTTP, etc.) são *dev-dependencies* e seguem a
  verificação de maturidade/licença do ADR-0004 — a escolha exata fica a confirmar no ticket.
- Decisões estruturais já registradas: ver índice completo em [`docs/adr/README.md`](./adr/README.md)
  (ADR-0001..0015 — fundação LLM, egresso, consumo de profiles, sinergia OSS, portabilidade,
  LiteLLM, guardrails, presets de chamada, timeout/keep_alive, repo-map, RAG semântico, saída
  estruturada, LSP-grounding, override runtime de parâmetros, Reviewer).

---

## Fase 0 — Bootstrap

### MT-01: Inicializar workspace Cargo + CI + lint
- **Objetivo:** esqueleto Rust que compila e passa no CI, com `cli` (binário) e `core` (lib).
- **Arquivos no escopo:** `Cargo.toml`, `crates/cli/Cargo.toml`, `crates/cli/src/main.rs`, `crates/core/Cargo.toml`, `crates/core/src/lib.rs`, `rustfmt.toml`, `.github/workflows/ci.yml`, `.gitignore`, `AGENTS.md` (bloco `USER:BEGIN id=comandos-exatos`).
- **Critério de aceite:** `cargo build` ok; CI roda fmt+clippy+test e fica verde; comandos exatos do `AGENTS.md` atualizados para Rust.
- **Fora de escopo:** qualquer lógica de LLM, rede ou CLI real (só `main` "hello").
- **Depende de:** nenhum · ADR-0001.

---

## Fase 1 — Núcleo de tipos e configuração

### MT-02: Tipos de domínio de mensagens/LLM (sem rede)
- **Objetivo:** tipos serializáveis: `Message`, `Role`, `ContentBlock`, `ToolCall`, `ToolResult`, `Usage`, `StreamEvent`.
- **Arquivos no escopo:** `crates/core/src/model/mod.rs`.
- **Critério de aceite:** testes de *round-trip* serde (serialize→deserialize) passam.
- **Fora de escopo:** nenhum formato específico de provider; nenhuma chamada de rede.
- **Depende de:** MT-01 · ADR-0001.

### MT-03: `trait LlmProvider` + provider mock de teste
- **Objetivo:** definir a `trait LlmProvider` (`chat`, `chat_stream`, *tool-calling*, `embeddings`) e um `MockProvider` para testes.
- **Arquivos no escopo:** `crates/core/src/provider/mod.rs`, `crates/core/src/provider/mock.rs`.
- **Critério de aceite:** compila; teste exercita `MockProvider` retornando mensagem e stream.
- **Fora de escopo:** qualquer adapter real (Ollama/OpenAI/Anthropic).
- **Depende de:** MT-02 · ADR-0001.

### MT-04: Configuração em camadas + resolução de classe de privacidade
- **Objetivo:** carregar config (perfil + projeto + env) e resolver perfil → classe de egresso.
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`, `crates/core/src/config/privacy.rs`.
- **Critério de aceite:** testes — `empresa`→`local-only`, `pessoal`→`cloud-ok`, classe ausente/ambígua→`local-only` (*fail-closed*).
- **Fora de escopo:** parsear todo o `settings-schema` (só o mínimo do ADR-0003); ler skills.
- **Depende de:** MT-02 · ADR-0002, ADR-0003.

---

## Fase 2 — Transporte e egresso (coração da confidencialidade)

### MT-05: Allowlist de endpoints + verificação de classe (lógica pura)
- **Objetivo:** decidir, sem rede, se um destino é permitido para a classe/perfil ativo.
- **Arquivos no escopo:** `crates/core/src/egress/allowlist.rs`.
- **Critério de aceite:** testes — endpoint fora da allowlist ⇒ erro; `local-only` rejeita host de nuvem; *fail-closed* no caso ambíguo.
- **Fora de escopo:** I/O HTTP real; audit log; redação.
- **Depende de:** MT-04 · ADR-0002.

### MT-06: Audit log de egresso + redação de segredos (lógica pura)
- **Objetivo:** produzir entrada de auditoria estruturada (destino, perfil, classe, tarefa) e redigir segredos antes de logar.
- **Arquivos no escopo:** `crates/core/src/egress/audit.rs`, `crates/core/src/egress/redact.rs`.
- **Critério de aceite:** testes — segredo nunca aparece no log; entrada contém os campos exigidos.
- **Fora de escopo:** transporte HTTP; persistência do log (só a estrutura/emissão).
- **Depende de:** MT-05 · ADR-0002.

### MT-07: Transporte HTTP único sobre `reqwest`
- **Objetivo:** ponto **único** de saída de rede, integrando allowlist + audit + redação; nenhuma outra parte do código chama `reqwest` diretamente.
- **Arquivos no escopo:** `crates/core/src/transport/mod.rs`.
- **Critério de aceite:** teste com servidor HTTP mock — chamada permitida passa; bloqueada **aborta**; teste/lint garante ausência de uso de `reqwest` fora deste módulo.
- **Fora de escopo:** lógica de qualquer provider; streaming específico de provider.
- **Depende de:** MT-05, MT-06 · ADR-0002.

---

## Fase 3 — Primeiro provider + router

### MT-08: Adapter Ollama (chat + stream) sobre o Transporte
- **Objetivo:** primeiro provider real (local), usando exclusivamente o Transporte (MT-07).
- **Arquivos no escopo:** `crates/core/src/provider/ollama.rs`.
- **Critério de aceite:** teste com mock do endpoint Ollama — chat e stream funcionam via Transporte; respeita `local-only`.
- **Fora de escopo:** router; outros providers.
- **Depende de:** MT-03, MT-07 · ADR-0001, ADR-0002.

### MT-09: Router / Policy Engine
- **Objetivo:** mapear `task-class → (provider, modelo, classe)` com fallback por disponibilidade; resolver também os presets de parâmetros de chamada por `task-class` do ADR-0008 (`temperature`/`top_p`/`system_prompt`/`max_tokens`).
- **Arquivos no escopo:** `crates/core/src/router/mod.rs`.
- **Critério de aceite:** testes — roteia por classe; tarefa sensível **nunca** roteia para provider de nuvem; fallback funciona; preset de `task-class` aplica os parâmetros de chamada esperados.
- **Fora de escopo:** UI de configuração; providers de nuvem (ainda só Ollama/mock).
- **Depende de:** MT-04, MT-08 · ADR-0002, ADR-0003, ADR-0008.

### MT-17: Timeout adaptativo + `keep_alive` configurável (troca de modelo local)
- **Objetivo:** Router rastreia o último modelo resolvido por provider e sinaliza troca de modelo (`is_model_switch`) em `ResolvedRoute`; Transporte aceita timeout por chamada; adapter Ollama usa o sinal para escolher timeout frio/quente e envia `keep_alive`. Tudo configurável via `settings-schema` (ADR-0009).
- **Arquivos no escopo:** `crates/core/src/router/mod.rs`, `crates/core/src/transport/mod.rs`, `crates/core/src/provider/ollama.rs`.
- **Critério de aceite:** testes — Router sinaliza `is_model_switch` corretamente entre resoluções consecutivas no mesmo/diferente modelo; Transporte aplica o timeout por chamada passado (mock lento confirma abort no timeout curto e sucesso no longo); `OllamaProvider` envia `keep_alive` na requisição.
- **Fora de escopo:** UI de configuração; providers de nuvem (não sofrem esse problema).
- **Depende de:** MT-04, MT-08, MT-09 · ADR-0009.

---

## Fase 4 — Loop, tools, permissão, CLI

### MT-10: Agent loop ReAct mínimo
- **Objetivo:** laço mensagem→tool-call→observação, com streaming e orçamento de tokens, sobre `MockProvider`/Ollama.
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`.
- **Critério de aceite:** teste — loop completa um ciclo de tool-call com provider mock e encerra no orçamento.
- **Fora de escopo:** tools reais; CLI; skills/MCP.
- **Depende de:** MT-03, MT-09 · ADR-0001.

### MT-11: Tool Registry + gate de permissão `allow|ask|deny`
- **Objetivo:** `trait Tool`, registro e portão de permissão.
- **Arquivos no escopo:** `crates/core/src/tools/mod.rs`, `crates/core/src/tools/permission.rs`.
- **Critério de aceite:** testes — `deny` bloqueia; `ask` sinaliza; `allow` executa (tool dummy).
- **Fora de escopo:** implementações concretas de fs/shell; Guardrail Gate de conteúdo (ADR-0007 — mecanismo distinto, ver micro-ticket a criar).
- **Depende de:** MT-10 · ADR-0002.

### MT-12: Tools de filesystem (read, write/edit, search)
- **Objetivo:** operações de arquivo respeitando `.claudeignore` e o gate de permissão.
- **Arquivos no escopo:** `crates/core/src/tools/fs.rs`.
- **Critério de aceite:** testes em diretório temporário — read/edit/search; respeita ignore; sob permissão.
- **Fora de escopo:** shell; rede.
- **Depende de:** MT-11.

### MT-13: Tool de shell sob permissão
- **Objetivo:** execução de comando com `deny` por padrão e ganchos de sandbox.
- **Arquivos no escopo:** `crates/core/src/tools/shell.rs`.
- **Critério de aceite:** testes — comando só roda sob `allow`; `deny` por *default*; nada executa sem aprovação.
- **Fora de escopo:** sandbox completo de SO (só os ganchos/política).
- **Depende de:** MT-11 · ADR-0002.

### MT-31: Consumir `CallPreset` no `Session` (fecha a lacuna do ADR-0008)
- **Objetivo:** `Session` passa a resolver a rota via `Router::resolve()` (em vez de receber provider/modelo fixos) e aplicar o `CallPreset` resolvido — `temperature`/`top_p`/`max_tokens` no `ChatRequest`; `system_prompt` antepõe uma `Message::system(...)` ao histórico se ainda não houver uma.
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`, `crates/core/src/provider/mod.rs` (`ChatRequest` ganha `temperature`/`top_p`).
- **Critério de aceite:** testes — preset de `task-class` (temperature/top_p/system_prompt/max_tokens) chega de fato ao `ChatRequest` enviado ao provider mock; `system_prompt` não duplica se já houver mensagem de sistema.
- **Fora de escopo:** override em runtime (MT-32/33); CLI (MT-14).
- **Depende de:** MT-09, MT-10 · ADR-0008.

### MT-32: `reasoning`/`thinking` como parâmetro de chamada
- **Objetivo:** `CallPreset` ganha campo `reasoning` (representação abstrata); `OllamaProvider` traduz para o mecanismo nativo do Ollama (`think`, para modelos que suportam raciocínio).
- **Arquivos no escopo:** `crates/core/src/router/mod.rs`, `crates/core/src/provider/ollama.rs`.
- **Critério de aceite:** teste com mock do endpoint Ollama — `think` presente na requisição quando `reasoning` está definido no preset.
- **Fora de escopo:** granularidade específica de outros providers (fica para quando forem implementados, MT-15/16).
- **Depende de:** MT-31 · ADR-0014.

### MT-33: Camada de override em runtime (`RuntimeOverride`)
- **Objetivo:** tipo `RuntimeOverride` (model/provider/temperature/top_p/system_prompt/max_tokens/reasoning) aplicado com a precedência do ADR-0014 (chamada única > sessão > preset de `task-class` > `settings-schema` > default do provider); override de `model`/`provider` continua sujeito à checagem de classe de egresso do Router — nunca a contorna.
- **Arquivos no escopo:** `crates/core/src/router/mod.rs` (ou novo `crates/core/src/router/override.rs`).
- **Critério de aceite:** testes — precedência de camadas resolve corretamente; override de `model`/`provider` que viole a classe de egresso ativa é bloqueado, não aplicado.
- **Fora de escopo:** superfícies de interação (flags/REPL — MT-14).
- **Depende de:** MT-31 · ADR-0014.

### MT-34: `Reviewer` — auditoria semântica por tipo, via `task-class` dedicada
- **Objetivo:** componente que monta a requisição de auditoria (prompt por tipo — `correctness`/`security`/`guardrail-compliance`/`task-completion` — + artefato a revisar + contexto original) e resolve via `Router::resolve("review-<tipo>")`, reaproveitando saída estruturada (ADR-0012) para obter o veredito (`pass`/`fail` + notas) em formato previsível.
- **Arquivos no escopo:** `crates/core/src/session/reviewer.rs`.
- **Critério de aceite:** teste — Reviewer monta a requisição certa por tipo de auditoria e interpreta corretamente o veredito estruturado devolvido por um provider mock.
- **Fora de escopo:** disparo automático dentro do `Session` (MT-35); UI/CLI de configuração.
- **Depende de:** MT-09, MT-31 · ADR-0015.

### MT-35: Reviewer integrado ao agent loop (pós-`Done`, com retry limitado)
- **Objetivo:** `Session::run()`/`run_streaming()` disparam o `Reviewer` (MT-34) após `StopReason::Done`, conforme tipos de auditoria habilitados para a `task-class`; modo `advisory` anexa o veredito a `SessionOutcome`; modo `blocking` com veredito `fail` gera turno corretivo (notas como observação) até um teto de retentativas, após o qual a falha persistente é exposta, nunca suprimida.
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`.
- **Critério de aceite:** testes — modo `advisory` não bloqueia a resposta; modo `blocking` reprovado dispara retry até o teto e depois desiste reportando a falha; nenhuma auditoria roda se não habilitada (*default* desligado, ADR-0015).
- **Fora de escopo:** revisão pré-execução (antes de tool-call); UI/CLI.
- **Depende de:** MT-34 · ADR-0015.

### MT-36: `Session::compact` — mecanismo de compactação de histórico
- **Objetivo:** `Session` ganha `compact` (assinatura exata a definir na implementação), que resolve a `task-class` `"compact"` via Router, faz uma chamada de chat simples (sem tools, sem streaming) pedindo um resumo do histórico atual, e substitui `self.messages` inteiro por uma única mensagem de sistema com o resumo; falha do provider preserva o histórico original intacto (tudo-ou-nada).
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`.
- **Critério de aceite:** testes — compactação bem-sucedida (mock) substitui o histórico por uma única mensagem de sistema; falha do provider preserva `messages` original intocado; a chamada usa a `task-class` `"compact"` resolvida pelo Router, não um caminho de chamada próprio.
- **Fora de escopo:** superfície de interação (MT-37); disparo automático por limiar; compactação parcial (preservar últimas mensagens verbatim).
- **Depende de:** MT-09, MT-31 · ADR-0016.

### MT-37: Comando `/compact` no REPL
- **Objetivo:** novo comando de barra `/compact` que chama `Session::compact` (MT-36) e ecoa confirmação (ou erro) ao usuário, no mesmo estilo dos comandos existentes (`/model`, `/temperature` etc., MT-14).
- **Arquivos no escopo:** `crates/cli/src/repl.rs`.
- **Critério de aceite:** teste de integração — `/compact` reduz o histórico a uma mensagem de sistema (mock devolvendo o resumo); histórico vazio ou erro do provider durante a compactação não derruba o REPL.
- **Fora de escopo:** TUI (v0.3); disparo automático.
- **Depende de:** MT-36 · ADR-0016.

### MT-14: CLI streaming (one-shot + REPL) com override de parâmetros
- **Objetivo:** interface de linha que roda o loop, exibe stream/diffs e prompts de permissão; expõe `RuntimeOverride` (MT-33) por **flags na invocação one-shot** (ex.: `--model`, `--temperature`, `--reasoning`) e por **comandos no REPL** (ex.: `/model`, `/temperature`, `/reasoning`, no estilo do `/model` do Claude Code), com eco de confirmação da mudança.
- **Arquivos no escopo:** `crates/cli/src/main.rs`, `crates/cli/src/repl.rs`.
- **Critério de aceite:** teste de integração — `agentry "<tarefa>"` roda o loop com Ollama local/mock e trata permissão interativa; flag de override muda o parâmetro só daquela invocação; comando REPL muda o parâmetro para os turnos seguintes da sessão até ser trocado de novo.
- **Fora de escopo:** TUI (v0.3); skills/MCP (v0.2).
- **Depende de:** MT-10, MT-12, MT-13, MT-33 · ADR-0014.

---

## Fase 5 — Demais providers

### MT-15: Adapter OpenAI-compatible (vLLM/OpenRouter/LiteLLM)
- **Objetivo:** adapter para a API OpenAI-compatible sobre o Transporte (cobre vLLM, OpenRouter e gateways LiteLLM).
- **Arquivos no escopo:** `crates/core/src/provider/openai_compat.rs`.
- **Critério de aceite:** teste com mock — chat/stream/tool-calling; classe de egresso respeitada (`cloud-opt-out`/allowlist); caso LiteLLM — endpoint de proxy com classe declarada funciona e **sem** classe declarada é bloqueado em perfil restritivo (fail-closed do ADR-0006).
- **Fora de escopo:** Anthropic; UI.
- **Depende de:** MT-07, MT-08 (mesmo padrão) · ADR-0001, ADR-0002, ADR-0006.

### MT-16: Adapter Anthropic (Messages API)
- **Objetivo:** adapter Anthropic (Messages API, *tool use*, streaming SSE) sobre o Transporte.
- **Arquivos no escopo:** `crates/core/src/provider/anthropic.rs`.
- **Critério de aceite:** teste com mock SSE — blocos `tool_use`; egresso só sob classe de nuvem permitida. *(Ao implementar, consultar a skill `claude-api` para o surface correto da API.)*
- **Fora de escopo:** *prompt caching* (v0.3); UI.
- **Depende de:** MT-03, MT-07 · ADR-0001, ADR-0002.

---

## Fase 6 — Especialização de modelos open-source sem fine-tuning

Motivação: o `agentry` tem como alvo de uso local modelos open-source pequenos (8B–30B, ex.:
família Qwen) servidos via Ollama — bem mais fracos que modelos de fronteira em busca
agenteica iterativa e propensos a alucinar assinatura/API e a produzir tool-call malformada.
As quatro capacidades abaixo (ADR-0010..0013) atacam essas fraquezas **sem fine-tuning**,
melhorando contexto e confiabilidade em vez de depender de raciocínio que o modelo pequeno
não tem. Todas vêm **ativadas por padrão**, com flag de desativação no `settings-schema`
(convenção herdada do ADR-0007/0008/0009: mudança de fronteira registrada no `exchange-log`).

Ordem de construção: repo-map primeiro (mais barato, sem infra pesada), saída estruturada e
LSP-grounding em paralelo (independentes entre si e do repo-map), RAG semântico por último
(reaproveita a extração de símbolos do repo-map para o chunking).

### MT-18: Parsing AST-aware via `tree-sitter` (extração de símbolos)
- **Objetivo:** dado um arquivo-fonte e sua linguagem, extrair os símbolos de nível função/classe/método com *range* de bytes.
- **Arquivos no escopo:** `crates/core/src/context/mod.rs`, `crates/core/src/context/ast.rs`.
- **Critério de aceite:** testes — extrai símbolos corretos (nome + *range*) de um arquivo Rust e de um arquivo Python de exemplo.
- **Fora de escopo:** repo-map (MT-19/20); chunking para RAG (MT-25) — só a extração de símbolos em si.
- **Depende de:** nenhum · ADR-0010.

### MT-19: Grafo de referências entre símbolos/arquivos
- **Objetivo:** construir um grafo dirigido de referências (import/uso) a partir dos símbolos extraídos (MT-18).
- **Arquivos no escopo:** `crates/core/src/context/repo_map/graph.rs`.
- **Critério de aceite:** teste — grafo construído sobre um mini-repositório de exemplo reflete as referências esperadas (arestas corretas).
- **Fora de escopo:** ranking (MT-20); tool exposta ao agent loop (MT-21).
- **Depende de:** MT-18 · ADR-0010.

### MT-20: Ranking de relevância (estilo PageRank) sobre o grafo
- **Objetivo:** dado um conjunto de arquivos/símbolos "semente", ranquear os demais por relevância usando o grafo (MT-19).
- **Arquivos no escopo:** `crates/core/src/context/repo_map/rank.rs`.
- **Critério de aceite:** teste — símbolos mais referenciados a partir da semente ficam no topo do ranking em um grafo de exemplo conhecido.
- **Fora de escopo:** tool exposta ao agent loop (MT-21).
- **Depende de:** MT-19 · ADR-0010.

### MT-21: Tool `repo_map` exposta ao agent loop
- **Objetivo:** expor o repo-map (MT-19/20) como `Tool` (MT-11) — dada uma consulta/tarefa, devolve os arquivos/símbolos mais relevantes.
- **Arquivos no escopo:** `crates/core/src/tools/repo_map.rs`.
- **Critério de aceite:** testes — tool respeita o gate de permissão (MT-11); respeita a flag `context.repo_map.enabled` (*default* `true`) — desligada, a tool não é registrada.
- **Fora de escopo:** UI/CLI de configuração.
- **Depende de:** MT-11, MT-20 · ADR-0010.

### MT-22: Saída estruturada (*constrained decoding*) no `OllamaProvider`
- **Objetivo:** `OllamaProvider` envia o campo `format` (JSON Schema das tools) quando `ChatRequest.tools` não está vazio.
- **Arquivos no escopo:** `crates/core/src/provider/ollama.rs`.
- **Critério de aceite:** teste com mock do endpoint Ollama — corpo da requisição contém `format` correspondente ao schema das tools quando presentes; ausente quando não há tools; respeita a flag `providers.ollama.structured_output` (*default* `true`).
- **Fora de escopo:** outros providers (OpenAI-compatible/Anthropic têm mecanismos próprios; não generalizar aqui).
- **Depende de:** MT-08 · ADR-0012.

### MT-23: Cliente LSP mínimo (spawn + JSON-RPC stdio)
- **Objetivo:** cliente LSP capaz de iniciar um *language server* já instalado, fazer `initialize`/`didOpen`, e encerrar limpo ao final.
- **Arquivos no escopo:** `crates/core/src/context/lsp/client.rs`.
- **Critério de aceite:** teste — ciclo de vida completo (start → initialize → shutdown) contra um *language server* de teste/mock; nenhum processo órfão após o teste.
- **Fora de escopo:** operações de leitura específicas (hover/definição — MT-24).
- **Depende de:** nenhum · ADR-0013.

### MT-24: Tool `lsp_hover`/`lsp_definition`
- **Objetivo:** expor hover/*go-to-definition*/referências do cliente LSP (MT-23) como `Tool` (MT-11).
- **Arquivos no escopo:** `crates/core/src/tools/lsp.rs`.
- **Critério de aceite:** testes — tool respeita o gate de permissão (MT-11); respeita a flag `context.lsp_grounding.enabled` (*default* `true`); ausência do *language server* no ambiente é erro tratado (não trava o agent loop).
- **Fora de escopo:** operações de escrita/refatoração via LSP.
- **Depende de:** MT-11, MT-23 · ADR-0013.

### MT-25: Chunking AST-aware para RAG
- **Objetivo:** gerar chunks de função/classe/método (reaproveitando MT-18) com metadados (arquivo, símbolo, *range*) prontos para indexação.
- **Arquivos no escopo:** `crates/core/src/context/rag/chunk.rs`.
- **Critério de aceite:** teste — chunks gerados não quebram uma função no meio; metadados corretos.
- **Fora de escopo:** geração de embeddings (MT-27); índice lexical (MT-26).
- **Depende de:** MT-18 · ADR-0011.

### MT-26: Índice lexical (`tantivy`/BM25) sobre os chunks
- **Objetivo:** indexar os chunks (MT-25) num índice `tantivy` para busca lexical (BM25).
- **Arquivos no escopo:** `crates/core/src/context/rag/lexical_index.rs`.
- **Critério de aceite:** teste — consulta por identificador exato devolve o chunk esperado no topo.
- **Fora de escopo:** busca híbrida (MT-28).
- **Depende de:** MT-25 · ADR-0011.

### MT-27: Índice semântico (embeddings + `lancedb`) sobre os chunks
- **Objetivo:** gerar embeddings dos chunks (MT-25) via `LlmProvider::embeddings` (MT-03) e indexá-los em `lancedb`.
- **Arquivos no escopo:** `crates/core/src/context/rag/semantic_index.rs`.
- **Critério de aceite:** teste com provider mock de embeddings — consulta por vetor devolve os chunks mais próximos esperados.
- **Fora de escopo:** busca híbrida (MT-28); reranking.
- **Depende de:** MT-03, MT-25 · ADR-0011.

### MT-28: Busca híbrida + *reranking*
- **Objetivo:** combinar os índices lexical (MT-26) e semântico (MT-27) e reordenar o top-K com um *reranker* cross-encoder.
- **Arquivos no escopo:** `crates/core/src/context/rag/hybrid_search.rs`.
- **Critério de aceite:** teste — resultado combinado reflete tanto *match* lexical exato quanto proximidade semântica; *reranking* reordena corretamente um caso conhecido.
- **Fora de escopo:** indexação incremental (MT-29); tool exposta ao agent loop (MT-30).
- **Depende de:** MT-26, MT-27 · ADR-0011.

### MT-29: Indexação incremental
- **Objetivo:** reembedar/reindexar só arquivos alterados (via `git diff` ou observação de *filesystem*), nunca o repositório inteiro.
- **Arquivos no escopo:** `crates/core/src/context/rag/incremental.rs`.
- **Critério de aceite:** teste — alterar um arquivo dispara reindexação só dele; arquivos não alterados não são reprocessados.
- **Fora de escopo:** tool exposta ao agent loop (MT-30).
- **Depende de:** MT-26, MT-27 · ADR-0011.

### MT-30: Tool `code_search` (RAG semântico) exposta ao agent loop
- **Objetivo:** expor a busca híbrida (MT-28) como `Tool` (MT-11).
- **Arquivos no escopo:** `crates/core/src/tools/code_search.rs`.
- **Critério de aceite:** testes — tool respeita o gate de permissão (MT-11); respeita a flag `context.semantic_rag.enabled` (*default* `true`) — desligada, a tool não é registrada nem a indexação roda.
- **Fora de escopo:** UI/CLI de configuração.
- **Depende de:** MT-11, MT-28, MT-29 · ADR-0011.

---

## Sequência crítica

```
MT-01 → MT-02 → MT-03 ─┐
            └ MT-04 → MT-05 → MT-06 → MT-07 → MT-08 → MT-09 → MT-10 → MT-11 → MT-12,13 → MT-31 → MT-32,33 → MT-14
                                                  └ MT-15, MT-16 (após MT-07/08)
                                                  └ MT-17 (após MT-04/08/09, independente)
                                                  └ Fase 6 (após MT-11, independente):
                                                       MT-18 → MT-19 → MT-20 → MT-21
                                                       MT-18 → MT-25 → MT-26 ┐
                                                                    → MT-27 ┴→ MT-28 → MT-29 → MT-30
                                                       MT-22 (após MT-08, independente)
                                                       MT-23 → MT-24 (independente)
                                                  └ MT-34 → MT-35 (após MT-09/31, independente do MT-14)
                                                  └ MT-36 → MT-37 (após MT-09/31, independente do MT-14)
```
