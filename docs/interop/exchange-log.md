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

---

## 2026-07-08 — Sexta extensão ao `settings-schema:1`: Reviewer (auditoria semântica)

- **Origem:** `agentry`.
- **Contexto:** o ADR-0007 (Guardrail Gate) já tinha adiado moderação semântica por modelo
  para "v0.2, se necessária, sob novo ADR" — este é esse ADR, motivado pela discussão sobre
  ter um modelo "reviewer" para auditorias diversas (correção, segurança, conformidade,
  cumprimento da tarefa).
- **ADR criado (`agentry`):**
  - **ADR-0015** (Proposed) — Reviewer: cada tipo de auditoria é uma `task-class` própria
    (ex.: `review-security`), resolvida pelo Router (MT-09) como qualquer outra — sem
    infraestrutura nova, reaproveita Router + `ChatRequest` + saída estruturada (ADR-0012)
    inteiramente. Disparo pós-`Done` (v0.1); dois modos — `advisory` (anexa veredito ao
    `SessionOutcome`) e `blocking` (retry corretivo limitado por teto, falha persistente
    sempre exposta). **Default desligado** (diferente do pacote ADR-0010..0013): é uma
    segunda chamada completa de modelo por tarefa, custo/latência reais, não uma capacidade
    local barata.
- **Pendências (rascunho a ratificar por ADR de esquema específico, quando implementado):**
  - Chaves de habilitação/modo por tipo de auditoria e por `task-class` no `settings-schema`
    (ADR-0015).
- **Status:** ✅ ADR de direção criado no `agentry`; micro-tickets **MT-34/35** adicionados à
  Fase 4 do roadmap. Formato definitivo fica para a implementação, a confirmar com o
  `profiles` antes de congelar.

---

## 2026-07-12 — Sétima extensão ao `settings-schema:1`: fechando o loop, de vez

- **Origem:** `agentry`.
- **Contexto:** revisão do roadmap pós-v0.1 (completo, MT-01..38): todas as seis extensões
  anteriores deste log ficaram com o formato de esquema deliberadamente adiado — nenhuma foi
  de fato confirmada. Investigação do lado `profiles` (repositório lido diretamente) revelou
  que o artefato hoje existente (`.claude/settings.json` por perfil) é o formato **nativo do
  Claude Code** — não o `settings-schema:1` que a ADR-0003 supõe consumir. Domínios
  incompatíveis por design: `agentry::config::Permissions.deny`/`ask` espera nomes exatos de
  tool; o Claude Code usa padrões `"Bash(git push*)"`. Não havia nada ali sobre roteamento por
  `task-class`, seleção de provider, ou as flags de contexto (RAG/repo-map/LSP)/Reviewer.
- **Decisão:** em vez de reinterpretar o artefato nativo do Claude Code, o `agentry` passa a
  ter um **artefato próprio**: `.agentry/agentry.settings.json` (não `.claude/`) — mesma
  pasta reservada pela ADR-0017 (MT-38), com uma exceção nomeada na auto-exclusão do
  `.gitignore` (a ADR-0017 foi emendada em 2026-07-12 para registrar isso). Primeira fatia de
  schema congelada (permissões + as 4 *flags* booleanas do pacote ADR-0010..0013, hoje
  hardcoded `true`); as demais extensões pendentes (`task-class` presets, timeout/
  `keep_alive`, `reasoning`, Reviewer, `guardrails`) continuam adiadas, uma por vez, para
  quando cada ticket de consumo for implementado — mesmo padrão desta sessão inteira.
- **ADR criado (`agentry`):**
  - **ADR-0018** (Proposed) — artefato/local/descoberta/precedência de camadas + primeira
    fatia de schema. Ver `docs/adr/0018-artefato-e-schema-minimo-de-configuracao-do-agentry.md`.
- **Trabalho do lado `profiles` (não é pendência — feito na mesma sessão, repos em
  paralelo):** os três perfis (`empresa`/`externo-confidencial`/`pessoal`) ganham um
  `.agentry/agentry.settings.json` + `.agentry/.gitignore` *default* próprios;
  `scripts/setup-profile.sh` ganha uma entrada em `bucket_for()` classificando o arquivo
  novo como `hybrid_json` (mesma disciplina de `--update` não-destrutivo já usada para
  `.claude/settings.json`); `docs/interop/SPEC.md` (canônico naquele repo) ganha uma linha
  na tabela de artefatos. Ver ADR local do `profiles` (`docs/adr/0006-*.md`).
- **Micro-tickets adicionados:** **MT-39** (`Settings::from_file`, descoberta+parsing do
  arquivo) e **MT-40** (consumo real das 4 flags em `crates/cli/src/main.rs`) — novo
  `docs/roadmap-v0.2.md` (v0.1 permanece fechado/imutável como registro histórico).
- **Status:** ✅ ADR-0018 criada no `agentry`; ADR local + arquivos *default* + script
  atualizados no `profiles`, mesma sessão. Implementação de MT-39/MT-40 fica para o próximo
  turno.

---

## 2026-07-13 — Oitava troca: bootstrap de `agentry.settings.json` lê do `profiles` sem clone

- **Origem:** `agentry`.
- **Contexto:** MT-39/MT-40 fecharam a leitura do artefato; faltava um jeito de **criar**
  `.agentry/agentry.settings.json` sem exigir que o usuário clone o `ai-coding-agent-profiles`
  ao lado do `agentry`. `curl <script> | sh` foi considerado e descartado (execução de código
  remoto sem *pinning*/revisão); buscar só o JSON diretamente também foi considerado e
  corrigido em revisão — violava literalmente a Diretriz de Conformidade da ADR-0002
  ("proibido qualquer chamada de rede fora do módulo de transporte central"), resolvido
  roteando a busca pelo próprio `Transport`, numa instância dedicada ao bootstrap (allowlist
  restrita a um host fixo, `EgressClass::CloudOk`) — sem abrir exceção à ADR-0002.
- **ADR criado (`agentry`):**
  - **ADR-0019** (Proposed) — `--init`/`/init` materializam `.agentry/agentry.settings.json`;
    sem `--profile`, só o exemplo genérico local (zero rede); com `--profile`, um único GET
    HTTPS (via `Transport`) do `agentry.settings.json` daquele perfil no `ai-coding-agent-
    profiles`, numa **referência (tag/commit) fixa gravada no código do `agentry`** — nunca
    "latest" dinâmico. Sempre imprime o comando manual equivalente (`setup-profile.sh`) como
    alternativa. Falha de rede com `--profile` explícito é erro tratado, nunca *fallback*
    silencioso para o exemplo genérico.
- **Efeito no lado `profiles`:** nenhum arquivo novo é necessário — os `.agentry/
  agentry.settings.json` por perfil já existem desde a sétima troca (ADR-0006 daquele
  repo). O único acoplamento novo é o `agentry` passar a conhecer, como constante pinada no
  próprio código, uma referência (tag/commit) daquele repositório público — atualizada
  manualmente a cada *bump* deliberado, nunca automática.
- **Status:** ✅ ADR-0019 criada no `agentry` (ainda `Proposed`, sem micro-ticket de
  implementação aberto). Nenhuma ação pendente do lado `profiles`.
