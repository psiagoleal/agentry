<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> Opcional em projetos solo; recomendado em colaborações. Atualizado a cada commit.
> Não inclua segredos. Mantido conforme a skill `handoff-updater`.

## Último turno

- **Data:** 2026-07-08
- **Branch:** `main`
- **Commit:** `4775f33`
- **Fase:** Fase 4 (MT-11 concluído) + ADR-0010..0014 registrados; Fase 6 (especialização sem fine-tuning) e MT-31/32/33 (override runtime) adicionados ao roadmap.

## Metas cumpridas / Em andamento / Próximo passo

**Cumpridas — planejamento:**
- [x] Ecossistema de 2 repositórios: `ai-coding-agent-profiles` (política) ⇄ `agentry` (execução).
- [x] **ADR-0001..0006** + **contrato de interop v1** + `architecture.md` + `roadmap-v0.1.md` (MT-01..MT-16).
- [x] Fontes de modelos da v0.1: **Ollama, vLLM, Anthropic, LiteLLM** (ADR-0006; Copilot/Enterprise adiado).

**Cumpridas — implementação:**
- [x] **MT-01** — scaffold do workspace Cargo (`crates/cli` = bin `agentry`, `crates/core` = lib `agentry_core`), CI, lints (`ba74200`).
- [x] **ADR-0005 fechado** — CI em matriz `ubuntu/windows/macos` (fmt/clippy em um SO), `.gitattributes` com LF (`2feed85`).
- [x] **ADR-0006** — LiteLLM como fonte de modelos via adapter OpenAI-compatible (MT-15); endpoints de proxy exigem classe de egresso declarada, ausência ⇒ tratado como nuvem (`ab69934`).
- [x] **MT-02** — tipos de domínio em `crates/core/src/model/`: `Message`, `Role`, `ContentBlock`, `ToolCall`, `ToolResult`, `Usage`, `StreamEvent`; round-trip serde testado; validação local verde (fmt+clippy+test) (`f03c1ef`).
- [x] **MT-03** — `trait LlmProvider` (chat, chat_stream, tool-calling, embeddings) + `MockProvider` roteirizado em `crates/core/src/provider/`. Trait dyn-compatible via `BoxFuture` (sem `async-trait`); streaming por canal `tokio::sync::mpsc` (tokio só com feature `sync`); 6 testes novos, 14 no total, validação verde (`26b370e`).
- [x] **MT-04** — `crates/core/src/config/`: `Settings` (mínimo do `settings-schema:1`, ADR-0003) com merge perfil→projeto→env (permissões são união; `deny` nunca encolhe) e `privacy.rs` com perfil→classe de egresso (`privacy-taxonomy:1`). Fail-closed: perfil ausente/desconhecido ⇒ `local-only`; schema divergente ⇒ erro. 32 testes no total, validação verde (`b63fe6b`).
- [x] **MT-05** — `crates/core/src/egress/allowlist.rs`: decisão em memória (sem I/O) se um host é alcançável sob a classe de egresso ativa. Host fora da allowlist ou classe insuficiente ⇒ erro; entradas conflitantes para o mesmo host resolvem para a mais restritiva (fail-closed); suporta host exato e wildcard `*.sufixo` (sem casar domínio nu). `EgressClass` ganhou `rank()`/`permits()` em `config/privacy.rs`. 40 testes no total, validação verde (`a2120b7`).
- [x] **MT-06** — `crates/core/src/egress/redact.rs` (redação sem regex, via tokenizador próprio que isola segredos colados em `chave=`/`?token=` etc.) e `audit.rs` (`AuditEntry` estruturada com destino/perfil/classe/tarefa/outcome, redigindo automaticamente todo campo textual). 54 testes no total, validação verde (`9a89679`).
- [x] **MT-07** — `crates/core/src/transport/mod.rs`: único ponto do crate autorizado a fazer rede (via `reqwest`, com `rustls-tls` em vez de `native-tls`). Integra allowlist (MT-05) + audit log (MT-06): chamada bloqueada aborta **antes** de abrir conexão TCP; toda tentativa emite `AuditEntry`. Teste com servidor HTTP mock feito só com `tokio::net` (sem lib de mock nova) + teste-guarda que varre o código-fonte do crate confirmando que `reqwest::` só aparece em `transport/mod.rs`. 58 testes no total, `cargo build --release` verde (`1723c31`). **Fecha a Fase 2 (egresso).**
- [x] **MT-08** — `crates/core/src/provider/ollama.rs`: primeiro provider real (local), implementando `LlmProvider::chat`/`chat_stream` exclusivamente via `Transport` (nunca importa `reqwest`), herdando allowlist+audit automaticamente. `Transport` ganhou `post_json_lines` (streaming genérico por linhas, agnóstico de formato de provider) e `tokio` ganhou a feature `rt` em `[dependencies]` (não só dev). Durante o desenvolvimento, o teste-guarda do MT-07 pegou uma falha de design própria: `Transport::new` recebia `reqwest::Client` por parâmetro, obrigando quem construísse um `Transport` a importar `reqwest` também — corrigido fazendo `Transport::new` construir o client internamente, sem expor o tipo na API pública. 63 testes no total, `cargo build --release` verde (`4d961eb`).
- [x] **ADR-0007** (Proposed) — Guardrail Gate de conteúdo (entrada/saída de LLM), distinto do gate de tools (MT-11) e da allowlist de egresso (MT-05); regras via extensão do `settings-schema`, camada mais específica só reforça, nunca afrouxa.
- [x] **ADR-0008** (Proposed) — parâmetros de chamada de LLM (`temperature`/`top_p`) e presets de modelo por `task-class`, resolvidos pelo Router (MT-09); rejeita o Modelfile do Ollama como mecanismo de configuração (acopla a um provider). Ambos mudam a fronteira do `settings-schema` (posse do `profiles`) — pedido registrado em `docs/interop/exchange-log.md`; roadmap (MT-09/MT-11) aponta para os ADRs (`3ae5054`).
- [x] **MT-09** — `crates/core/src/router/mod.rs`: mapeia `task-class → (provider, modelo, classe de egresso)` com fallback por disponibilidade e resolve os presets de chamada do ADR-0008. `resolve()` descarta candidato que exige mais do que a classe ativa **antes** de checar disponibilidade — tarefa sensível nunca alcança provider de nuvem mesmo que ele esteja registrado; provider indisponível cai no próximo candidato. Esta é a peça que cobre a ideia de "orquestrador multi-modelo" discutida com o usuário (ver [[no-separate-orchestrator-project]]). 6 testes novos, 69 no total, `cargo build --release` verde (`e23390b`). **Fecha a Fase 3.**
- [x] **MT-10** — `crates/core/src/session/mod.rs`: `Session` com `run()` (chat agregado) e `run_streaming()` (chat_stream + `StreamAggregator` reconstruindo a mensagem final a partir dos eventos), ambos partilhando `after_response()` (soma uso, decide orçamento, executa tool-calls). Execução real de tools ainda não existe — o loop consome só o contrato `ToolExecutor` (dyn-compatible via `BoxFuture`, mesmo padrão do `LlmProvider`); implementações reais (fs/shell) chegam no MT-11+. Orçamento checado logo após cada resposta, **antes** de executar qualquer tool-call pendente. 5 testes novos, 74 no total, `cargo build --release` verde (`cdd4fc6`). **Abre a Fase 4.**
- [x] **ADR-0009** (Proposed) — timeout adaptativo + `keep_alive` configurável para troca de modelo em provider local: Router sinaliza `is_model_switch` em `ResolvedRoute` (rastreando o último modelo por provider); Transporte ganha timeout por chamada; `OllamaProvider` usa o sinal para timeout frio/quente e envia `keep_alive`. Motivado por uma lacuna real auditada: `Transport::new` hoje constrói `reqwest::Client::new()` sem nenhum timeout configurado. Muda a fronteira do `settings-schema` — registrado em `docs/interop/exchange-log.md`; micro-ticket **MT-17** adicionado à Fase 3 do roadmap (`ef69785`).
- [x] **MT-11** — `crates/core/src/tools/{mod.rs,permission.rs}`: `trait Tool` dyn-compatible via `BoxFuture` (mesmo padrão de `LlmProvider`/`ToolExecutor`) + `ToolRegistry` + `PermissionGate` reaproveitando `config::Permissions` (deny/ask do MT-04) em vez de inventar novo formato de política. `deny` (explícito ou tool não registrada) bloqueia sem executar; `ask` **sinaliza** devolvendo a `ToolCall` pendente (`ExecutionOutcome::NeedsConfirmation`) — nunca bloqueia esperando confirmação humana, isso fica para a CLI (MT-14); `allow` executa. Precedência fail-closed: `deny` checado antes de `ask` no mesmo nome. 10 testes novos, 84 no total, `cargo build --release` verde (`cf21f6f`).
- [x] **ADR-0010..0013** (Proposed) — pacote de 4 ADRs para "especialização de modelos open-source sem fine-tuning" (alvo: Qwen 8B-30B local via Ollama). **ADR-0010:** repo-map estilo Aider via `tree-sitter` (grafo de referências + ranking), sem vector DB — construído primeiro por ser mais barato. **ADR-0011:** RAG semântico local — chunking AST-aware (reaproveita ADR-0010) + índice lexical `tantivy` + índice semântico `lancedb` (via `LlmProvider::embeddings` já existente) + busca híbrida + reranker + indexação incremental; `tantivy`/`lancedb` escolhidos por serem nativos em Rust (sem ponte Python/FFI). **ADR-0012:** saída estruturada (constrained decoding) para tool-calling via o campo `format` já existente na API do Ollama — sem dependência nova. **ADR-0013:** tool de grounding via LSP (`lsp-types`+`lsp-server`), só leitura, falando com language server já instalado pelo usuário. Maturidade das 4 dependências novas verificada via `gh repo view`+crates.io antes de fechar os ADRs (todas MIT/Apache-2.0, ativas; `lsp-types` sem push há >1 ano, mitigado por ser dependência direta do `rust-analyzer` ativo — registrado para reverificação). Todas ativadas por padrão, desabilitáveis via `settings-schema` — mudança de fronteira registrada no `exchange-log.md`. Nova **Fase 6** + micro-tickets **MT-18..MT-30** adicionados ao roadmap via skill `micro-ticket-planner` (`70c0470`).
- [x] **ADR-0014** (Proposed) — override runtime de parâmetros de chamada: `CallPreset` (ADR-0008/MT-09) ganha campo `reasoning`; novo tipo `RuntimeOverride` (model/provider/temperature/top_p/system_prompt/max_tokens/reasoning) com precedência chamada-única (flag de CLI) > sessão (comando REPL, estilo `/model` do Claude Code) > preset de `task-class` > `settings-schema` > default do provider. **Fronteira de segurança:** `RuntimeOverride` nunca contém classe de egresso/permissões (continuam fixas pela resolução de `Config` na inicialização); override de model/provider continua sujeito à checagem de allowlist/classe do Router — nunca contorna o fail-closed do ADR-0002; override só vem de comando explícito, nunca inferido de conteúdo de mensagem/tool-output. **Lacuna descoberta e registrada:** `CallPreset` já existe no código desde o MT-09 mas `Session` nunca o consumia — o MT-31 fecha isso independentemente do reasoning. Micro-tickets **MT-31/32/33** adicionados à Fase 4, antes do MT-14 (`4775f33`).

**Em andamento:** nada pendente no turno. **Nota:** um commit anterior neste turno (`31e4fef`) teve um erro de citação de shell (backtick de `` `format` `` expandido como comando) corrompendo um trecho da mensagem — corrigido via `--amend` no mesmo turno, antes de qualquer push.

**Próximo passo:** **MT-12** — Tools de filesystem (read, write/edit, search) em `crates/core/src/tools/fs.rs`, respeitando `.claudeignore` e o gate de permissão do MT-11 (depende de MT-11, feito) — dá sequência à Fase 4 já em andamento. **Pendências independentes ainda abertas (sem bloquear a Fase 4):** MT-17 (ADR-0009, timeout/keep_alive); MT-18..MT-30 (Fase 6, ADR-0010..0013); MT-31/32/33 (ADR-0014, override runtime — MT-31 é o mais urgente dos três por fechar uma lacuna já existente do ADR-0008/MT-09, não só habilitar o `reasoning`).

## Impedimentos abertos

- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido pela empresa; adapter adiado.
- **CI multi-SO ainda não observado verde:** a matriz do ADR-0005 (`2feed85`) precisa de um push ao GitHub para confirmar Windows/macOS verdes.

---

## Histórico (mais recente no topo)

| Data | Commit | Resumo | MT |
|------|--------|--------|----|
| 2026-07-08 | `4775f33` | ADR-0014: override runtime de parâmetros (reasoning + model/temperature/etc.); MT-31..MT-33 | — |
| 2026-07-08 | `70c0470` | ADR-0010..0013: RAG/repo-map/saída estruturada/LSP-grounding; Fase 6 + MT-18..MT-30 | — |
| 2026-07-07 | `cf21f6f` | MT-11: Tool Registry + gate de permissão allow\|ask\|deny + testes | MT-11 |
| 2026-07-07 | `ef69785` | ADR-0009: timeout adaptativo + keep_alive para troca de modelo local; MT-17 adicionado | — |
| 2026-07-07 | `cdd4fc6` | MT-10: agent loop ReAct mínimo (run + run_streaming); abre a Fase 4 | MT-10 |
| 2026-07-07 | `e23390b` | MT-09: Router/Policy Engine (task-class → provider/modelo/classe); fecha a Fase 3 | MT-09 |
| 2026-07-07 | `3ae5054` | ADR-0007/0008: guardrails de conteúdo + presets de chamada por task-class | — |
| 2026-07-07 | `4d961eb` | MT-08: adapter Ollama (chat+stream) sobre o Transporte; abre a Fase 3 | MT-08 |
| 2026-07-07 | `1723c31` | MT-07: transporte HTTP único sobre reqwest; fecha a Fase 2 (egresso) | MT-07 |
| 2026-07-07 | `9a89679` | MT-06: audit log de egresso + redação de segredos (sem regex) + testes | MT-06 |
| 2026-07-07 | `a2120b7` | MT-05: allowlist de endpoints + `rank`/`permits` de `EgressClass` + testes | MT-05 |
| 2026-07-07 | `b63fe6b` | MT-04: config em camadas + classe de privacidade fail-closed + testes | MT-04 |
| 2026-07-06 | `26b370e` | MT-03: `trait LlmProvider` + `MockProvider` roteirizado + testes | MT-03 |
| 2026-07-06 | `f03c1ef` | MT-02: tipos de domínio de mensagens/LLM + testes round-trip serde | MT-02 |
| 2026-07-06 | `ab69934` | ADR-0006: LiteLLM via adapter OpenAI-compatible; roadmap MT-15 e arquitetura atualizados | — |
| 2026-07-06 | `2feed85` | ADR-0005 fechado: matriz de CI em 3 SOs + `.gitattributes` (LF) | — |
| 2026-06-19 | `ba74200` | MT-01: scaffold do workspace Cargo + CI + lint + `git init`; validação local verde | MT-01 |
| 2026-06-19 | — | Planejamento: ADR-0001..0004, interop v1, `architecture.md`, `roadmap-v0.1.md` | — |
