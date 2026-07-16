<!-- Caminho relativo: docs/roadmap-v0.10.md -->

# Roadmap v0.10 — Micro-tickets

O roadmap v0.9 (`docs/roadmap-v0.9.md`) cobre a Fase 15 (TUI via `ratatui`, ADR-0027,
**concluída**). Este documento detalha a **Fase 16** do roadmap de longo prazo
(`docs/roadmap-longo-prazo.md`): cliente MCP via `rmcp` (ADR-0028) — só servidores locais
(`stdio`) nesta fase; servidores remotos (HTTP/SSE) ficam explicitamente fora de escopo até
uma fase dedicada resolver como esse tráfego se integra ao `Transport` único do projeto.

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), skill
`micro-ticket-planner` para granularidade. **Uma dependência nova nesta fase**, já vetada e
autorizada pelo mantenedor: `rmcp` (só as *features* `client` + `transport-child-process` nas
dependências de produção — ver ADR-0028 para a verificação de maturidade completa e a
justificativa do escopo). Nenhuma outra dependência nova de produção; a *feature* `server` do
`rmcp` pode ser habilitada só em `[dev-dependencies]`, se necessária para montar um servidor
MCP mínimo de teste (mesmo espírito do `fake_lsp_server`, MT-23) — decisão de implementação do
MT-78, registrada em `docs/decisoes-autonomas.md` se exigir escolha.

**Numeração:** a Fase 15 fechou em MT-76; o MT-77 (*widget* de lista de tarefas) foi
descartado ainda em preparação (YAGNI, ADR-0027) e nunca chegou a ser usado — esta fase
retoma a partir dele, sem deixar o número como lacuna permanente.

---

## Fase 16 — Cliente MCP via `rmcp` (ADR-0028)

### MT-77: Adoção `rmcp` + schema `mcpServers` na configuração ✅ concluído (9fcbaaf)
- **Objetivo:** `rmcp` adicionado a `crates/core/Cargo.toml` (`[workspace.dependencies]` no
  `Cargo.toml` raiz), só com as *features* `client` + `transport-child-process` (ADR-0028 —
  nunca `server`/`reqwest`/transporte HTTP em dependência de produção). Novo bloco
  `mcpServers` em `agentry.settings.json`: mapa nome → `{ command: string, args: [string],
  egressClass: string }` — `Settings`/`Config` (`crates/core/src/config/mod.rs`) ganham o
  schema e o *merge* por camada (mesmo padrão de `taskClasses`, MT-55/ADR-0021); vazio por
  padrão (zero-config = zero servidores conectados). `egressClass` diferente de `local-only`
  é `ConfigError` tratado (nunca conecta, nunca infere) — mensagem explica que servidores
  remotos ainda não são suportados nesta versão. `GENERIC_SETTINGS_EXAMPLE`
  (`crates/cli/src/main.rs`) ganha o bloco comentado (ADR-0022), com um exemplo inerte
  (servidor de exemplo com `_comentario`, sem ativar nada).
- **Arquivos no escopo:** `Cargo.toml` (raiz), `crates/core/Cargo.toml`,
  `crates/core/src/config/mod.rs`, `crates/cli/src/main.rs`.
- **Critério de aceite:** testes — parsing do bloco `mcpServers` completo; ausência do bloco
  preserva zero-config (nenhum servidor); `egressClass` fora de `local-only` é erro tratado
  com mensagem clara; exemplo `--init` continua JSON válido com todo campo de exemplo inerte
  (mesmo teste do MT-49/57, estendido).
- **Fora de escopo:** conectar de fato a um servidor (MT-78); registrar tools no
  `ToolRegistry` (MT-79).
- **Depende de:** ADR-0028.

### MT-78: Cliente MCP — conecta, *handshake*, descobre tools ✅ concluído (7a68941)
- **Objetivo:** novo `crates/core/src/mcp/mod.rs`: `McpClient` (ou nome equivalente) spawna
  cada servidor declarado em `mcpServers` via `rmcp::transport::child_process::
  TokioChildProcess` (subprocesso local, `stdio`), completa o *handshake* MCP
  (`ServiceExt::serve`) e lista as tools disponíveis (`list_tools()`). Mesmo padrão de
  gerenciamento de ciclo de vida de `LspClient` (`crates/core/src/context/lsp`, ADR-0013):
  `Drop` mata o subprocesso como rede de segurança se nenhum encerramento explícito
  aconteceu — mesma limitação já aceita (não cobre `std::process::exit()`).
- **Arquivos no escopo:** `crates/core/src/mcp/mod.rs` (novo).
- **Critério de aceite:** teste de integração de ponta a ponta contra um servidor MCP real de
  teste (mesmo espírito do `fake_lsp_server`, MT-23 — servidor mínimo próprio, ou a *feature*
  `server` do `rmcp` só em `[dev-dependencies]`, decisão de implementação a registrar se
  exigir escolha): *handshake* completo, `list_tools()` devolve as tools esperadas; `Drop` sem
  encerramento explícito não deixa processo órfão (mesmo teste já existente para
  `LspClient::drop_sem_shutdown_explicito_nao_deixa_processo_orfao`).
- **Fora de escopo:** registro no `ToolRegistry` (MT-79); qualquer transporte que não seja
  subprocesso local.
- **Depende de:** MT-77.

### MT-79: Tools MCP no `ToolRegistry` sob o gate de permissão ✅ concluído (f758b2d)
- **Objetivo:** cada tool descoberta por `McpClient` (MT-78) vira uma entrada no
  `ToolRegistry` já existente, implementando a *trait* `Tool` (MT-11) — `execute()` encaminha
  a chamada para `peer.call_tool(...)` do `rmcp`. Nome registrado sempre prefixado pelo nome
  do servidor (`"<servidor>__<tool>"`, ADR-0028) — nunca colide entre dois servidores com uma
  tool de mesmo nome. Sob o **mesmo** `PermissionGate` de qualquer outra tool — nenhum
  mecanismo paralelo de confirmação/bloqueio.
- **Arquivos no escopo:** `crates/core/src/tools/mcp.rs` (novo), `crates/cli/src/main.rs`
  (registra uma tool por servidor+tool descoberta, condicional a `mcpServers` não vazio).
- **Critério de aceite:** testes — tool MCP registrada aparece em `ToolRegistry::specs()` com
  o nome prefixado corretamente; execução respeita `deny`/`ask` como qualquer outra tool
  (mesmo padrão dos testes já existentes do MT-11 aplicado a uma tool MCP, com um servidor de
  teste); duas tools de mesmo nome em servidores diferentes não colidem no registro.
- **Fora de escopo:** validação adicional de classe de egresso além do que o MT-77/80 já
  cobrem na configuração.
- **Depende de:** MT-78.

### MT-80: Classe de egresso declarada por servidor MCP (ADR-0002) ✅ concluído (6f3b9b5)
- **Objetivo:** formaliza e testa de ponta a ponta o que o MT-77 já começou no *parsing*: um
  servidor MCP só é spawnado/conectado (`McpClient`, MT-78) se sua `egressClass` declarada for
  `local-only` — qualquer outro valor já falhou antes, na resolução da configuração (MT-77),
  então esta ticket garante que **nenhum caminho** (inclusive um bloco `mcpServers` montado
  manualmente por código, não só pelo arquivo) chega a spawnar um servidor com classe
  diferente. Documenta explicitamente, em código e no teste, que suporte a servidor
  remoto/HTTP é trabalho futuro deliberadamente adiado (ADR-0028), não um `TODO` esquecido.
- **Arquivos no escopo:** `crates/core/src/config/mod.rs`, `crates/core/src/mcp/mod.rs`.
- **Critério de aceite:** teste — tentar construir/rodar `McpClient` para uma entrada com
  `egressClass` diferente de `local-only` (contornando o *parsing* do MT-77, direto na
  construção) ainda falha de forma tratada, nunca silenciosa; nenhuma chamada de processo é
  feita nesse caso (o subprocesso nunca chega a ser spawnado).
- **Fora de escopo:** qualquer implementação de transporte remoto.
- **Depende de:** MT-77.

### MT-81: Documentação (usuário + governança)
- **Objetivo:** `docs/usuario/configuracao.md` ganha a seção `mcpServers` (schema, exemplo,
  nota de que só `local-only` é aceito nesta versão e por quê). `docs/usuario/uso.md` ganha
  uma nota curta sobre tools MCP aparecerem dinamicamente (nome prefixado pelo servidor,
  mesma disciplina `ask`/`deny` de qualquer tool). `docs/governanca/privacidade-e-egresso.md`
  ganha a seção "MCP e egresso" para o público de *compliance*: por que só servidores locais
  são suportados agora (nenhuma segunda via de rede não auditada), o que isso implica para
  quem avalia o produto, e que servidores remotos ficam fora até uma fase dedicada. ADR-0028
  promovida de `Proposed` para `Accepted` (MT-77..80 concluídos).
- **Arquivos no escopo:** `docs/usuario/configuracao.md`, `docs/usuario/uso.md`,
  `docs/governanca/privacidade-e-egresso.md`, `docs/adr/0028-mcp-client-via-rmcp.md` (status),
  `docs/adr/README.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo; releitura confirmando que nada nas
  trilhas de usuário/governança ficou desatualizado.
- **Fora de escopo:** nenhuma mudança de código.
- **Depende de:** MT-77..80 (todos).

---

## Sequência crítica

```
MT-77 → MT-78 → MT-79 → MT-81
   └──────────→ MT-80 ──────↗
```

MT-80 depende só do MT-77 (schema/config) e pode rodar em paralelo ao MT-78/79 em termos de
planejamento, mas como o loop autônomo processa um ticket por vez, a ordem numérica
(MT-77 → 78 → 79 → 80 → 81) é seguida à risca — mais simples do que reordenar por dependência
mínima, sem custo real (nenhum ticket fica bloqueado esperando por um irmão que já terminou).
