<!-- Caminho relativo: docs/adr/README.md -->

# Índice de ADRs

Registros de decisões estruturais (ver skill `adr-writer`). Use
`skills/adr-writer/templates/adr-template.md` para criar o primeiro.

| ADR | Título | Status |
|-----|--------|--------|
| [0001](0001-fundacao-camada-llm.md) | Fundação da camada LLM por abstração própria sobre `reqwest` | Accepted |
| [0002](0002-modelo-privacidade-egresso.md) | Modelo de privacidade/egresso e taxonomia de classes | Accepted |
| [0003](0003-consumo-artefatos-profiles.md) | Consumo dos artefatos de política do `ai-coding-agent-profiles` | Accepted |
| [0004](0004-postura-sinergia-open-source.md) | Postura de sinergia com projetos open-source | Proposed |
| [0005](0005-portabilidade-cross-platform.md) | Portabilidade cross-platform (Linux, Windows, macOS) | Accepted |
| [0006](0006-litellm-fonte-de-modelos.md) | LiteLLM como fonte de modelos via adapter OpenAI-compatible | Accepted |
| [0007](0007-guardrails-configuraveis-de-conteudo.md) | Guardrails configuráveis de conteúdo (gate distinto do Tool Registry) | Accepted |
| [0008](0008-parametros-de-chamada-e-presets-por-task-class.md) | Parâmetros de chamada de LLM e presets de modelo por task-class | Accepted |
| [0009](0009-timeout-adaptativo-e-keep-alive-para-troca-de-modelo.md) | Timeout adaptativo e `keep_alive` configurável para troca de modelo em provider local | Accepted |
| [0010](0010-repo-map-tree-sitter.md) | Repo map (estilo Aider) via `tree-sitter`, sem vector DB | Accepted |
| [0011](0011-rag-semantico-local-para-codigo.md) | RAG semântico local para código (chunking + busca híbrida + reranker) | Accepted |
| [0012](0012-saida-estruturada-para-tool-calling.md) | Saída estruturada (*constrained decoding*) para tool-calling | Accepted |
| [0013](0013-tool-de-grounding-via-lsp.md) | Tool de *grounding* via LSP (Language Server Protocol) | Accepted |
| [0014](0014-override-runtime-de-parametros-de-chamada.md) | Override runtime de parâmetros de chamada (sessão + invocação única) | Accepted |
| [0015](0015-reviewer-auditoria-semantica-por-task-class.md) | Reviewer — auditoria semântica de tarefas via `task-class` dedicada | Accepted |
| [0016](0016-compactacao-de-historico-de-sessao.md) | Compactação de histórico de sessão (`Session::compact`) | Accepted |
| [0017](0017-diretorio-de-estado-local-do-agente.md) | Diretório de estado local por projeto (`.agentry/`) para memória, histórico e índices | Accepted |
| [0018](0018-artefato-e-schema-minimo-de-configuracao-do-agentry.md) | Artefato e schema mínimo de configuração do `agentry` (`agentry.settings.json`) | Accepted |
| [0019](0019-bootstrap-de-agentry-settings-json-via-init.md) | Bootstrap de `.agentry/agentry.settings.json` via `--init`/`/init` | Accepted |
| [0020](0020-agentryignore-com-respeito-opcional-a-gitignore.md) | Arquivo `.agentryignore` (renomeando `.claudeignore`) com respeito opcional a `.gitignore` | Accepted |
| [0021](0021-schema-de-configuracao-de-task-class.md) | Schema de configuração de task-class (rotas e presets configuráveis) | Accepted |
| [0022](0022-convencao-de-configuracao-autoexplicativa.md) | Convenção de configuração autoexplicativa (`_comentario` obrigatório) | Accepted |
| [0023](0023-memoria-de-projeto-agents-md-e-skills.md) | Memória de projeto: leitura de `AGENTS.md`/`CLAUDE.md` + *progressive disclosure* de `SKILL.md` | Accepted |
| [0024](0024-tool-askuser.md) | Tool `AskUser` — pergunta/confirmação entre agente e usuário | Accepted |
| [0025](0025-web-tools-webfetch-websearch-searxng.md) | Web tools — `WebFetch` e `WebSearch` via SearXNG configurável | Accepted |
| [0026](0026-tool-glob-e-shell-em-background.md) | Tool `Glob` (busca por padrão de arquivo) e shell em background/streaming | Accepted |
| [0027](0027-tui-via-ratatui.md) | TUI via `ratatui` — modo interativo opt-in, sem substituir o REPL | Accepted |
| [0028](0028-mcp-client-via-rmcp.md) | Cliente MCP via `rmcp` — só servidores locais (`stdio`) na v1 | Accepted |
| [0029](0029-uso-de-tokens-visivel-na-sessao.md) | Uso de tokens visível durante a sessão (`Session::usage_total`) | Accepted |
| [0030](0030-checkpoints-e-undo-de-mudancas-de-arquivo.md) | Checkpoints e *undo* de mudanças de arquivo (`fs_write`/`fs_edit`) | Accepted |
| [0031](0031-subagentes-com-egresso-restrito.md) | Subagentes com classe de egresso restrita à sessão-mãe | Accepted |
| [0032](0032-memoria-de-projeto-explicita.md) | Memória de projeto explícita entre sessões (`/remember`) | Accepted |
| [0033](0033-teto-de-turnos-consecutivos-com-tool-call.md) | Teto de turnos consecutivos com tool-call, independente do orçamento de tokens | Accepted |
