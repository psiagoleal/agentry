<!-- Caminho relativo: docs/interop/exchange-log.md -->

# Exchange Log — `agentry` ⇄ `ai-coding-agent-profiles`

Registro **append-only** das trocas entre os dois projetos. Regras: anexar ao final; nunca
reescrever entradas; decisões vinculantes viram ADR (referenciar o ADR aqui).

---

## 2026-06-19 — Bootstrap do ecossistema

- **Origem:** `agentry`.
- **Contexto:** definição inicial dos dois projetos como ecossistema (política + execução).
- **Decisões:**
  - Estabelecido o contrato de interoperabilidade **v1** (canônico em `profiles/docs/interop/SPEC.md`).
  - Divisão de responsabilidades (charter) ratificada: `profiles` = política; `agentry` = execução/imposição.
  - Provedores da v0.1 do `agentry`: **Ollama**, **vLLM** e **Anthropic**. GitHub Copilot/GitHub Enterprise **adiado** (caminho oficial — GitHub Models ou API Enterprise — ainda indefinido pela empresa).
  - Privacidade/egresso é **requisito** de arquitetura (não feature): router com classes de egresso desde a v0.1.
- **Pendências (rascunho a ratificar por ADR no `agentry`):**
  - `settings-schema:1` — quais chaves de `settings.json` o `agentry` lê.
  - `privacy-taxonomy:1` — mapa perfil → classe de egresso (`empresa`→`local-only`, `externo-confidencial`→`cloud-opt-out`, `pessoal`→`cloud-ok`).
- **Sinergia OSS avaliada (maturidade a verificar via `gh repo view`):** `rtk` (Rust, compressão de tool-output — candidato a dependência, auditar telemetria), `caveman`/`ponytail` (skills consumíveis), `LLM-Wiki` (padrão da camada de memória), `OKF` (vigiar — imaturo).
- **Status:** ✅ contrato v1 criado; ADRs do `agentry` pendentes.

---

## 2026-06-19 — Pacote de ADRs do `agentry`

- **Origem:** `agentry`.
- **Contexto:** ratificação das decisões estruturais da v0.1 (sem código ainda).
- **ADRs criados:**
  - **ADR-0001** (Accepted) — fundação da camada LLM: abstração própria sobre `reqwest`, sem framework (`rig`/`genai` fora do runtime).
  - **ADR-0002** (Accepted) — privacidade/egresso: transporte único auditável + allowlist + *fail-closed*; **ratifica `privacy-taxonomy:1`** (empresa→local-only, externo-confidencial→cloud-opt-out, pessoal→cloud-ok).
  - **ADR-0003** (Proposed) — consumo dos artefatos do `profiles`; `settings-schema:1` mínimo, extensível por novos ADRs.
  - **ADR-0004** (Proposed) — sinergia OSS: padrão antes de dependência (rtk/caveman/ponytail/LLM-Wiki/OKF); telemetria barrada por ADR-0002.
- **Efeito no contrato:** `privacy-taxonomy:1` passa de *(rascunho)* a **ratificado** no SPEC; `settings-schema:1` segue *(rascunho)* vinculado ao ADR-0003.
- **Documentação:** criado `agentry/docs/architecture.md` (módulos + fluxo de egresso).
- **Status:** ✅ ADRs e arquitetura criados. Próximo: roadmap de micro-tickets da v0.1.

---

## 2026-06-19 — Nome da CLI definido: `agentry`

- **Origem:** `agentry`.
- **Decisão:** o repositório/crate/binário da CLI passa a se chamar **`agentry`** (confirmado livre na crates.io), substituindo o placeholder `ai-cli` em toda a documentação dos dois repos.
- **Pareamento:** `ai-coding-agent-profiles` (política) + `agentry` (execução).
- **Ressalvas:** colisão leve de marca com "SAP Agentry" (plataforma legada de mobilidade empresarial, domínio distinto) — sem conflito de crate. A **pasta local** continua `~/dev/ai-cli` até renomeação manual (afeta CWD da sessão e o caminho da auto-memória).
- **Status:** ✅ documentação renomeada nos dois repos.

---

## 2026-07-07 — Duas extensões propostas ao `settings-schema:1`

- **Origem:** `agentry`.
- **Contexto:** discussão sobre (1) guardrails de conteúdo configuráveis, distintos do gate
  de tools do MT-11 e da allowlist de egresso do MT-05; e (2) parâmetros de chamada de LLM
  (`temperature`, `top_p`, *system prompt* padrão) configuráveis por tipo de tarefa, inspirado
  no conceito de Modelfile do Ollama mas com formato próprio e portável entre providers.
- **ADRs criados (`agentry`):**
  - **ADR-0007** (Proposed) — Guardrail Gate de conteúdo; regras vêm de uma futura chave
    `guardrails` no `settings-schema`; camada mais específica só pode reforçar, nunca
    afrouxar regra herdada (mesma semântica de `Permissions::union` do MT-04).
  - **ADR-0008** (Proposed) — Presets de modelo por `task-class` (`temperature`/`top_p`/
    `system_prompt`/`max_tokens`); consumidos pelo Router (MT-09); `system_prompt` continua
    sendo uma `Message::system(...)` comum, não um campo novo.
- **Pendências (rascunho a ratificar por ADR de esquema específico, quando implementado):**
  - Chave `guardrails` no `settings-schema` (ADR-0007).
  - Chave de presets de modelo por `task-class` no `settings-schema` (ADR-0008).
- **Status:** ✅ ADRs de direção criados no `agentry`. Formato definitivo do esquema fica para
  quando os respectivos micro-tickets forem implementados — a confirmar com o `profiles`
  antes de congelar, já que `settings-schema` é artefato de sua posse.

---

## 2026-07-07 — Terceira extensão proposta ao `settings-schema:1`: timeout/keep_alive

- **Origem:** `agentry`.
- **Contexto:** discussão sobre confiabilidade do uso 100% local com Ollama — o
  carregamento/descarregamento de modelos pode causar timeout espúrio numa troca de
  `task-class` que implica troca de modelo no mesmo provider.
- **ADR criado (`agentry`):**
  - **ADR-0009** (Proposed) — Router passa a rastrear o último modelo por provider e
    sinaliza troca de modelo (`is_model_switch`) em `ResolvedRoute`; Transporte ganha
    timeout por chamada (API nativa do `reqwest`); adapter Ollama usa o sinal para
    timeout frio/quente e envia `keep_alive`. Timeout frio/quente e `keep_alive`
    configuráveis pelo usuário.
- **Pendências (rascunho a ratificar por ADR de esquema específico, quando implementado):**
  - Chaves de timeout frio/quente e `keep_alive` por provider no `settings-schema` (ADR-0009).
- **Status:** ✅ ADR de direção criado no `agentry`; micro-ticket **MT-17** adicionado ao
  roadmap (Fase 3). Formato definitivo do esquema fica para a implementação, a confirmar
  com o `profiles` antes de congelar.

---

## 2026-07-08 — Quarta extensão ao `settings-schema:1`: especialização de modelos sem fine-tuning

- **Origem:** `agentry`.
- **Contexto:** o `agentry` tem como alvo de uso local modelos open-source pequenos (8B–30B,
  ex.: Qwen) via Ollama — mais fracos que modelos de fronteira em busca agenteica iterativa e
  propensos a alucinar API/produzir tool-call malformada. Quatro capacidades foram desenhadas
  para compensar isso sem fine-tuning, todas **ativadas por padrão e desabilitáveis pelo
  usuário**.
- **ADRs criados (`agentry`):**
  - **ADR-0010** (Proposed) — Repo map (estilo Aider) via `tree-sitter`: grafo de referências +
    ranking de relevância, sem vector DB. Maturidade do crate `tree-sitter` verificada
    (`gh repo view`/crates.io): MIT, 26,9M downloads, ativo.
  - **ADR-0011** (Proposed) — RAG semântico local: chunking AST-aware (reaproveita ADR-0010) +
    índice lexical (`tantivy`) + índice semântico (`lancedb`, via `LlmProvider::embeddings`
    já existente) + busca híbrida + reranker + indexação incremental. Maturidade verificada:
    `tantivy` (MIT, 15M downloads), `lancedb` (Apache-2.0, 639k downloads), ambos nativos em
    Rust, sem servidor.
  - **ADR-0012** (Proposed) — Saída estruturada (*constrained decoding*) para tool-calling no
    `OllamaProvider`, via o campo `format` já existente na API do Ollama (sem dependência
    nova).
  - **ADR-0013** (Proposed) — Tool de *grounding* via LSP (`lsp-types`+`lsp-server`), só
    leitura (hover/definição), falando com *language server* já instalado pelo usuário — o
    `agentry` não empacota nenhum. Nota de maturidade: `lsp-types` sem *push* há mais de um
    ano, mitigado por ser dependência direta do `rust-analyzer` (ativo); registrado para
    reverificação futura.
- **Pendências (rascunho a ratificar por ADR de esquema específico, quando implementado):**
  - `context.repo_map.enabled` (ADR-0010).
  - `context.semantic_rag.enabled` (ADR-0011).
  - `providers.ollama.structured_output` (ADR-0012).
  - `context.lsp_grounding.enabled` (ADR-0013).
  - Todas com *default* `true` — convenção "ativado por padrão, desabilitável pelo usuário"
    definida nesta troca.
- **Status:** ✅ 4 ADRs de direção criados no `agentry`; nova **Fase 6** e micro-tickets
  **MT-18..MT-30** adicionados ao roadmap. Formato definitivo das chaves de esquema fica para
  a implementação, a confirmar com o `profiles` antes de congelar.

---

## 2026-07-08 — Quinta extensão ao `settings-schema:1`: override runtime de parâmetros

- **Origem:** `agentry`.
- **Contexto:** discussão sobre configurar `reasoning`/`thinking` (e, mais amplamente, todo o
  conjunto de parâmetros de chamada) tanto como *default* em camadas quanto ajustável em
  tempo real (flag de CLI para invocação única; comando REPL para a sessão), no estilo do
  `/model` do Claude Code. Descoberta no processo: o `CallPreset` do ADR-0008/MT-09 já existe
  no código mas não era consumido por `Session` — lacuna pré-existente, fechada pelo mesmo
  ADR.
- **ADR criado (`agentry`):**
  - **ADR-0014** (Proposed) — `CallPreset` ganha campo `reasoning`; novo tipo
    `RuntimeOverride` (model/provider/temperature/top_p/system_prompt/max_tokens/reasoning)
    com precedência chamada-única > sessão > `task-class` > `settings-schema` > default do
    provider. **Fronteira de segurança explícita:** `RuntimeOverride` nunca contém classe de
    egresso nem permissões — essas continuam fixas pela resolução de `Config` (MT-04) feita
    na inicialização; override de `model`/`provider` continua sujeito à checagem de
    allowlist/classe do Router (nunca contorna o *fail-closed* do ADR-0002); override só vem
    de comando explícito do usuário, nunca inferido de conteúdo de mensagem/tool-output
    (fecha superfície de *prompt injection*).
- **Pendências (rascunho a ratificar por ADR de esquema específico, quando implementado):**
  - Formato de `reasoning` no `settings-schema` (ADR-0014, estende ADR-0008).
- **Status:** ✅ ADR de direção criado no `agentry`; micro-tickets **MT-31/32/33** adicionados
  à Fase 4 do roadmap (antes do MT-14, que passa a expor as duas superfícies de override).
  Formato definitivo fica para a implementação, a confirmar com o `profiles` antes de
  congelar.
