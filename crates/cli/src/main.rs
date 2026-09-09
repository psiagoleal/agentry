// Caminho relativo: crates/cli/src/main.rs
//! Ponto de entrada da CLI `agentry` (MT-14).
//!
//! Monta configuração (MT-04), transporte+allowlist (MT-05/07), o `Router`
//! com o provider Ollama (MT-08/09), o `ToolRegistry` com as tools de fs
//! (MT-12) e shell (MT-13), e despacha para um dos três modos:
//!
//! - **One-shot** (`agentry "<tarefa>"`): roda um único turno (com o loop de
//!   tool-calls interno de [`agentry_core::session::Session::run_streaming`])
//!   e sai.
//! - **REPL** (sem tarefa na invocação): entra em [`repl::run_repl`], que
//!   aceita mensagens e comandos de barra até `/exit`/`/quit`/EOF.
//! - **TUI** (`--tui`, ADR-0027): entra em [`tui::run`], que recebe a mesma
//!   `Session`/`Router` já montados aqui — nenhuma construção duplicada.
//!
//! Em todos os modos, as flags de override (`--model`, `--temperature`,
//! `--top-p`, `--max-tokens`, `--system`, `--reasoning`) montam o
//! [`RuntimeOverride`] inicial (ADR-0014/MT-33): no one-shot, vale só para
//! aquela invocação (o processo roda uma vez e sai); no REPL, vira o estado
//! de sessão inicial, que os comandos de barra atualizam a partir daí.
//!
//! A flag `--init` (e o comando `/init` do REPL) materializam
//! `.agentry/agentry.settings.json`: sem `--profile`, o exemplo genérico do
//! schema mínimo (ADR-0018 §5), sem nenhuma chamada de rede (ADR-0019 §1,
//! MT-41); com `--profile <nome>`, busca o artefato real daquele perfil no
//! repositório irmão `ai-coding-agent-profiles` (ver [`init`], MT-42).

mod audit_sink;
mod init;
mod mcp_server;
mod repl;
mod sessao;
mod streaming;
mod tool_executor;
mod tui;

use std::io;
use std::sync::Arc;

use clap::Parser;

use agentry_core::config::{Config, Settings};
use agentry_core::egress::allowlist::{Allowlist, AllowlistEntry, ANY_HOST};
use agentry_core::egress::audit::AuditEntry;
use agentry_core::guardrail::{GuardrailAuditEntry, GuardrailAuditSink, GuardrailGate};
use agentry_core::mcp::McpClient;
use agentry_core::provider::anthropic::AnthropicProvider;
use agentry_core::provider::claude_cli::{ClaudeCliProvider, PonteMcp};
use agentry_core::provider::ollama::OllamaProvider;
use agentry_core::provider::openai_compat::OpenAiCompatProvider;
use agentry_core::provider::LlmProvider;
use agentry_core::router::{CallPreset, RouteEntry, RouteTarget, Router, RuntimeOverride};
use agentry_core::session::{Session, TokenBudget, ToolExecutor};
use agentry_core::state_dir;
use agentry_core::tools::ask_user::{AskUserTool, Prompter};
use agentry_core::tools::checkpoint::CheckpointingTool;
use agentry_core::tools::code_search::{register_code_search_tool, CodeSearchSession};
use agentry_core::tools::fs::{FsEditTool, FsReadTool, FsSearchTool, FsWriteTool};
use agentry_core::tools::glob::GlobTool;
use agentry_core::tools::lsp::{register_lsp_tools, LspSession};
use agentry_core::tools::mcp::McpTool;
use agentry_core::tools::permission::PermissionGate;
use agentry_core::tools::repo_map::{register_repo_map_tool, RepoMapTool};
use agentry_core::tools::session_search::{register_session_search_tool, SessionSearchSession};
use agentry_core::tools::shell::{ShellBackgroundTool, ShellPolicy, ShellTool};
use agentry_core::tools::skill::SkillTool;
use agentry_core::tools::subagent::SubagentTool;
use agentry_core::tools::todo::TodoWriteTool;
use agentry_core::tools::web_fetch::{WebFetchTool, WEB_TOOL_USER_AGENT};
use agentry_core::tools::web_search::WebSearchTool;
use agentry_core::tools::ToolRegistry;
use agentry_core::transport::{host_from_url, AuditSink, Transport};

use repl::parse_bool_toggle;
use tool_executor::{Confirmer, InteractiveConfirmer, InteractivePrompter, RegistryToolExecutor};

/// Modelo Ollama usado quando `--model` não é informado.
const DEFAULT_MODEL: &str = "llama3.1:8b";
/// Host:porta padrão do servidor Ollama local.
const DEFAULT_OLLAMA_HOST: &str = "127.0.0.1:11434";
/// Orçamento de tokens usado quando `max_tokens` não está definido em
/// nenhuma camada de configuração.
const DEFAULT_TOKEN_BUDGET: u64 = 100_000;
/// Nome do provider LiteLLM (ADR-0006) no `Router` — fixo, diferente do
/// `OpenAiCompatProvider` genérico (que aceita qualquer nome porque pode
/// apontar para vLLM/OpenRouter/etc.): esta CLI só liga um único endpoint
/// LiteLLM, `providers.litellm` (MT-48).
const LITELLM_PROVIDER_NAME: &str = "litellm";
/// Variável de ambiente com a chave de API do gateway LiteLLM (MT-49) —
/// nunca lida do arquivo de configuração (segredo). Ausente ⇒ nenhum header
/// de autorização é anexado; gateways internos sem autenticação (ex.: só
/// acessíveis via VPN corporativa) continuam funcionando sem ela.
const LITELLM_API_KEY_ENV: &str = "AGENTRY_LITELLM_API_KEY";
/// Nome do provider Anthropic (ADR-0040) no `Router` — Messages API por
/// **chave de API**. Deliberadamente distinto do caminho de assinatura
/// Pro/Max (`claude-cli`): autenticação, custo e auditoria diferentes, e o
/// usuário precisa escolher explicitamente qual está usando.
const ANTHROPIC_PROVIDER_NAME: &str = "anthropic";
/// Variável de ambiente com a chave da Anthropic API (ADR-0040) — nunca lida
/// do arquivo de configuração versionado. Ausente ⇒ `credentials::
/// resolve_api_key` consulta `~/.agentry/credentials.json` (MT-128).
const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
/// Versão da Messages API enviada em toda requisição (header obrigatório).
const ANTHROPIC_VERSION: &str = "2023-06-01";
/// Nome do provider de assinatura Pro/Max (ADR-0040) no `Router` — separado
/// de `anthropic` de propósito: modelo de custo, de autenticação e de
/// auditoria diferentes, e o usuário precisa escolher explicitamente.
const CLAUDE_CLI_PROVIDER_NAME: &str = "claude-cli";
/// *Language server* usado por `lsp_hover`/`lsp_definition` (MT-24, ADR-0013)
/// quando `context.lspGrounding.enabled` está ativo. Seleção por linguagem
/// (detectar o projeto e escolher o LS certo) fica para um ticket futuro —
/// fora de escopo do MT-40 ("UI/CLI de configuração"); `rust-analyzer` é o
/// default razoável hoje porque o próprio `agentry` é um workspace Rust.
/// Ausência do binário no `PATH` já é erro tratado pelo `LspClient` (MT-23),
/// nunca *panic*.
const DEFAULT_LSP_COMMAND: &str = "rust-analyzer";
/// Exemplo genérico do schema mínimo (ADR-0018 §5) — conteúdo exato gravado
/// por `--init`/`/init` quando nenhum `--profile` é informado (ADR-0019 §1,
/// MT-41): todas as flags de contexto/provider em `true`, permissões
/// vazias. Busca de valores diferenciados por perfil fica para o MT-42.
///
/// **Todo campo configurável do schema aparece aqui** (achado real do
/// MT-49/50 — `providers.litellm` não tinha exemplo nenhum no arquivo
/// gerado, só documentado em ADR/roadmap; usuário só descobriu a chave
/// certa lendo o código-fonte). Campos que ficam **inertes até serem
/// preenchidos** (`profile`, `model`, `max_tokens`,
/// `providers.litellm.*`) usam `null` — JSON não tem comentário, `null` é
/// o equivalente mais próximo de "campo existe, ainda desligado": mostra a
/// chave sem ativar nada (`Config::resolve` só liga o candidato LiteLLM
/// quando `baseUrl` **e** `model` estão ambos presentes, MT-48).
///
/// **Comentários explicativos via `_comentario`** (avaliado trocar o
/// formato inteiro para TOML — descartado: o `ai-coding-agent-profiles`
/// já distribui este artefato em JSON real, com uma ferramenta de merge
/// não-destrutivo própria para JSON, `update_json_settings()`/
/// `hybrid_json` em `scripts/setup-profile.sh`; trocar de formato quebraria
/// essa ferramenta e criaria dois formatos coexistindo — `--init` genérico
/// vs. `--init --profile`. Os arquivos reais daquele repositório já usam
/// `_comentario` — chave prefixada com `_`, ignorada pelo parser real
/// (`Settings` não usa `deny_unknown_fields`) — para o mesmo propósito;
/// aqui só se estende essa convenção já estabelecida a cada bloco, em vez
/// de introduzir um formato novo.
const GENERIC_SETTINGS_EXAMPLE: &str = r#"{
  "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
  "_comentario": "Configuração local do agentry para este projeto. Guia completo: docs/usuario/configuracao.md no repositório do agentry. Campos com valor null existem no schema mas ficam desligados até você preencher.",
  "schemaVersion": 1,
  "profile": null,
  "model": null,
  "max_tokens": null,
  "permissions": {
    "_comentario": "deny: nomes de tool sempre bloqueados. ask: nomes de tool que pedem confirmação antes de rodar. Fora das duas listas, a tool roda sem perguntar (exceto a tool de shell, bloqueada por padrão nesta CLI). Vazio por padrão — nenhum nome extra bloqueado/perguntado. Exemplo (não aplicado, só ilustrativo): \"deny\": [\"shell\"] bloquearia a tool de shell mesmo numa build futura sem o default-deny atual; \"ask\": [\"fs_write\"] pediria confirmação antes de qualquer escrita.",
    "deny": [],
    "ask": [],
    "_comentario_readAllow": "readAllow (ADR-0041): lista fechada de caminhos que o agente pode LER, sintaxe do .gitignore. Ausente/vazio = sem restrição de caminho (padrão). Presente = tudo que não casar é bloqueado. ATENÇÃO: só alcança tools com argumento de caminho (fs_read/fs_write/fs_edit) — shell_exec, glob e fs_search leem por outras vias e precisam entrar em deny, senão readAllow é falsa sensação de proteção. Exemplo para o caso 'agente de nuvem planeja, modelo local lê': \"readAllow\": [\"README.md\", \"AGENTS.md\", \"docs/**\"] junto de \"deny\": [\"shell_exec\", \"glob\", \"fs_search\"].",
    "readAllow": []
  },
  "_comentario_subagentPermissions": "subagentPermissions (ADR-0041): mesmo formato de permissions, aplicado só ao subagente. Ausente = herda as do agente principal. Serve para negar dados sensíveis ao agente principal (tipicamente de nuvem) e devolver esse acesso ao subagente, que roda num modelo local. Exemplo: com o readAllow acima em permissions, use \"subagentPermissions\": { \"deny\": [] } para o modelo local continuar lendo tudo. Garante que nenhuma leitura confidencial ocorre sem passar por um modelo local — não garante que a resposta dele venha sanitizada.",
  "context": {
    "_comentario": "repoMap/semanticRag/lspGrounding: as três capacidades de contexto do agente, ligadas por padrão. gitignore.enabled: opcional (default false, diferente das outras três) — quando ligado, o agente também respeita o .gitignore do projeto (em união com .agentryignore, nunca em substituição) para reduzir ruído de contexto; não tem efeito de confidencialidade, quem precisa esconder algo do agente usa .agentryignore.",
    "repoMap": { "enabled": true },
    "semanticRag": { "enabled": true },
    "lspGrounding": { "enabled": true },
    "gitignore": { "enabled": false }
  },
  "providers": {
    "_comentario": "Ollama (local) é o provider padrão desta CLI. litellm é opcional — preencha baseUrl e model no bloco abaixo para ativar um gateway LiteLLM (ex.: corporativo) como segundo provider, selecionável via --provider litellm / comando /provider.",
    "ollama": {
      "_comentario": "structuredOutput restringe a geração ao schema JSON das tools (ADR-0012). Mantenha false (default desde 2026-07-24): com ele ativo, um modelo com tool-calling nativo devolve a chamada como texto e nenhuma tool executa. Ligue apenas para modelos antigos, sem suporte nativo a tools, onde a restrição de schema é a única forma de obter JSON bem-formado.",
      "structuredOutput": false
    },
    "litellm": {
      "_comentario": "baseUrl e model precisam estar os dois preenchidos para este provider ativar. egressClass (local-only / cloud-opt-out / cloud-ok) decide se o endpoint é alcançável sob o perfil ativo — ausente (null) é tratado como cloud-ok, o mais restritivo para liberar; gateways só acessíveis via rede interna/VPN geralmente precisam declarar local-only explicitamente.",
      "baseUrl": null,
      "model": null,
      "egressClass": null
    },
    "anthropic": {
      "_comentario": "Messages API da Anthropic por chave de API (cobrada por token). Preencha model (ex.: claude-opus-5) para ativar; baseUrl ausente usa https://api.anthropic.com e egressClass ausente usa cloud-ok. A chave NUNCA vai neste arquivo: use a variável ANTHROPIC_API_KEY ou `agentry --set-credential anthropic`. Exige o perfil 'pessoal' (cloud-ok) para ser alcançável.",
      "model": null,
      "baseUrl": null,
      "egressClass": null
    },
    "claudeCli": {
      "_comentario": "Usa sua assinatura Claude Pro/Max via o binário `claude` já instalado e autenticado (nenhuma chave de API; o agentry nunca lê seu token). Preencha model (ex.: opus, haiku, claude-opus-5) para ativar. Sem mcpTools é SÓ TEXTO: as ferramentas embutidas do Claude Code ficam desligadas para que nenhuma edição escape do controle de permissão/checkpoints do agentry — serve para conversa, /compact e revisão. Cada invocação é registrada em .agentry/audit.log como subprocess:claude-cli.",
      "model": "opus",
      "egressClass": null,
      "_comentario_mcpTools": "mcpTools (ADR-0042): devolve ao Claude as ferramentas do PRÓPRIO agentry, sob a mesma política (permissions/readAllow) — é o que permite usar a assinatura Pro/Max como agente principal COM tool-calling. As ferramentas embutidas do Claude Code continuam desligadas nos dois modos. Combine com readAllow para o padrão 'o Claude planeja, o modelo local lê o que é sensível'. ATENÇÃO: com mcpTools o laço de agente passa a ser do Claude Code, então guardrails de conteúdo, teto de turnos e compactação (que são por turno da sessão do agentry) NÃO se aplicam — permissões, readAllow, checkpoints e audit log continuam valendo. Precisa dessas garantias? use o provider anthropic.",
      "mcpTools": true
    }
  },
  "guardrails": {
    "_comentario": "Regras de bloqueio/mascaramento de conteúdo, verificadas antes (input) e depois (output) de cada chamada ao modelo. Cada regra tem id (identificador único), match (texto a procurar, sem diferenciar maiúsculas/minúsculas) e action (block ou redact). Vazio por padrão — nenhuma regra ativa. Exemplos para copiar em input/output (não aplicados aqui): {\"id\": \"bloqueia-senha\", \"match\": \"senha:\", \"action\": \"block\"} bloqueia uma entrada que cole uma credencial; {\"id\": \"mascara-segredo\", \"match\": \"segredo-abc\", \"action\": \"redact\"} mascara uma saída que ecoe um segredo conhecido. Guia: docs/usuario/guardrails.md.",
    "input": [],
    "output": []
  },
  "taskClasses": {
    "chat": {
      "_comentario": "Roteamento por task-class (ADR-0021): cada nome mapeia para uma lista ordenada de candidatos (provider/model/egressClass) + um preset de parâmetros — o Router usa o primeiro candidato cuja egressClass é permitida e cujo provider está registrado. Esta ('chat') é a task-class default, usada quando nenhuma outra é escolhida via --task-class/`/task-class` — mesmos provider/modelo/egressClass do comportamento zero-config (sem este bloco); sobrescreva livremente, outras camadas de configuração nunca afrouxam a egressClass declarada aqui. 'compact' (/compact) e 'guardrail-compliance' (Reviewer) são sintetizadas automaticamente com Ollama/local-only quando ausentes deste bloco. Nomes extras (como os dois exemplos abaixo) ficam inertes até serem escolhidos explicitamente via --task-class/`/task-class`. Guia: docs/usuario/configuracao.md.",
      "candidates": [
        { "provider": "claude-cli", "model": "opus", "egressClass": "cloud-ok" },
        { "provider": "ollama", "model": "llama3.1:8b", "egressClass": "local-only" }
      ]
    },
    "local": {
      "_comentario": "Task-class para delegar ao modelo LOCAL via a tool `subagent` (subagent com task_class='local'). É a metade do padrão 'o Claude planeja, o modelo local lê o que é sensível': combinada com permissions.readAllow + subagentPermissions, o agente de nuvem fica estruturalmente impedido de abrir os arquivos confidenciais e precisa delegar essas leituras aqui. Nunca sai da máquina (local-only).",
      "candidates": [
        { "provider": "ollama", "model": "llama3.1:8b", "egressClass": "local-only" }
      ]
    },
    "revisao-em-nuvem": {
      "_comentario": "Exemplo de task-class opcional para tarefas que podem sair da máquina: aponta para o gateway LiteLLM (preencha providers.litellm acima para o candidato ficar de fato disponível) com egressClass cloud-ok. Use via --task-class revisao-em-nuvem ou /task-class revisao-em-nuvem.",
      "candidates": [
        { "provider": "litellm", "model": "modelo-30b-do-seu-gateway", "egressClass": "cloud-ok" }
      ],
      "preset": { "temperature": 0.2 }
    },
    "dados-sensiveis": {
      "_comentario": "Exemplo de task-class que nunca deve sair da máquina, mesmo com providers.litellm configurado: só declara o candidato Ollama, então local-only é garantido pela ausência de qualquer candidato de nuvem, não só pela egressClass.",
      "candidates": [
        { "provider": "ollama", "model": "llama3.1:8b", "egressClass": "local-only" }
      ]
    }
  },
  "_comentario_mcpServers": "mcpServers (Fase 16/ADR-0028): cada nome mapeia para um comando (+ argumentos) rodado como subprocesso, falando o protocolo MCP via stdin/stdout — mesmo modelo de confiança de um language server local (ADR-0013), nunca uma chamada de rede mediada pelo agentry. egressClass é sempre obrigatória e, nesta versão, só aceita 'local-only' — servidores remotos (HTTP/SSE) ainda não são suportados, declarar qualquer outra classe é erro tratado ao carregar a configuração. Vazio por padrão: um servidor declarado é um servidor ao qual o agentry TENTA CONECTAR em toda execução, então um exemplo inerte aqui viraria uma mensagem de erro fixa a cada início de sessão. Exemplo para copiar dentro de mcpServers: \"filesystem\": { \"command\": \"npx\", \"args\": [\"-y\", \"@modelcontextprotocol/server-filesystem\", \"/caminho/do/projeto\"], \"egressClass\": \"local-only\" }. Guia: docs/usuario/configuracao.md.",
  "mcpServers": {}
}
"#;
/// Comando manual equivalente, sempre exibido por `--init`/`/init` (ADR-0019
/// §5) — para quem preferir os valores diferenciados por perfil
/// (`empresa`/`externo-confidencial`/`pessoal`) do `ai-coding-agent-profiles`
/// em vez do exemplo genérico, inspecionando/rodando por conta própria.
const MANUAL_SETUP_HINT: &str = "dica: para valores diferenciados por perfil (empresa/externo-confidencial/pessoal), rode o scripts/setup-profile.sh de https://github.com/psiagoleal/ai-coding-agent-profiles";

/// CLI agêntica de codificação (multi-provedor, roteamento por classe de
/// privacidade) — v0.1 fala só com um servidor Ollama local.
#[derive(Parser, Debug)]
#[command(name = "agentry", version, about)]
struct Args {
    /// Tarefa a rodar em modo one-shot; ausente inicia o REPL interativo.
    tarefa: Option<String>,

    /// Modelo a usar nesta invocação (sobrescreve o padrão).
    #[arg(long, short = 'm')]
    model: Option<String>,

    /// Provider a usar nesta invocação — `ollama` (padrão), `litellm`
    /// (ADR-0006/MT-49), `anthropic` (chave de API) ou `claude-cli`
    /// (assinatura Pro/Max, só texto: não executa ferramentas) — ADR-0040;
    /// cada um disponível se o bloco `providers.*` correspondente estiver
    /// configurado. Restringe a escolha aos candidatos já declarados na rota;
    /// nome desconhecido é o mesmo erro tratado de
    /// `Router::resolve_with_override`.
    #[arg(long, short = 'p')]
    provider: Option<String>,

    /// Temperatura de amostragem desta invocação.
    #[arg(long)]
    temperature: Option<f32>,

    /// *Top-p* (*nucleus sampling*) desta invocação.
    #[arg(long = "top-p")]
    top_p: Option<f32>,

    /// Limite de tokens de saída desta invocação.
    #[arg(long = "max-tokens")]
    max_tokens: Option<u32>,

    /// *System prompt* desta invocação.
    #[arg(long)]
    system: Option<String>,

    /// Raciocínio estendido (`on`/`off`), se o modelo suportar.
    #[arg(long)]
    reasoning: Option<String>,

    /// Host:porta do servidor Ollama local.
    #[arg(long = "ollama-host", default_value = DEFAULT_OLLAMA_HOST)]
    ollama_host: String,

    /// Cria `.agentry/agentry.settings.json` (bootstrap, ADR-0019) e sai —
    /// sem `--profile`, usa o exemplo genérico do schema mínimo (ADR-0018
    /// §5), sem nenhuma chamada de rede.
    #[arg(long, conflicts_with = "tarefa")]
    init: bool,

    /// Com `--init`: busca o `agentry.settings.json` real deste perfil
    /// (`empresa`/`externo-confidencial`/`pessoal`) no `ai-coding-agent-profiles`
    /// público, numa referência fixa (MT-42, ADR-0019 §2-4) — em vez do
    /// exemplo genérico.
    #[arg(long, requires = "init")]
    profile: Option<String>,

    /// Task-class a usar nesta invocação — escolhe entre as task-classes
    /// **declaradas** (`taskClasses`, MT-55/ADR-0021) para esta chamada;
    /// default `chat`. Mesmo padrão de override vetado de
    /// `--provider`/`--model` (ADR-0014): nunca introduz um alvo não
    /// declarado — nome desconhecido ou candidato indisponível é o mesmo
    /// erro tratado de `Router::resolve_with_override`.
    #[arg(long = "task-class")]
    task_class: Option<String>,

    /// Entra no modo TUI (`ratatui`, ADR-0027) em vez do REPL de texto —
    /// mesma `Session`/`Router` da CLI, com *streaming* real (MT-72). Sem
    /// esta flag, o comportamento one-shot/REPL existente continua
    /// inalterado.
    #[arg(long, conflicts_with_all = ["init", "tarefa"])]
    tui: bool,

    /// Desfaz o checkpoint mais recente de `fs_write`/`fs_edit` (MT-87,
    /// ADR-0030) e sai, sem rodar nenhuma tarefa — os checkpoints persistem
    /// em `.agentry/checkpoints.json`, então desfaz o mais recente de
    /// **qualquer** invocação anterior, não só desta sessão.
    #[arg(long, conflicts_with_all = ["init", "tarefa", "tui"])]
    undo: bool,

    /// Grava `<fato>` como memória de projeto explícita (MT-94, ADR-0032)
    /// em `.agentry/memory.json` e sai, sem rodar nenhuma tarefa — fica
    /// disponível para sessões futuras (carregado no *system prompt* de
    /// toda invocação seguinte). Deliberadamente **não** existe uma tool
    /// que o agente possa chamar sozinho para isso — sempre um ato
    /// explícito do usuário.
    #[arg(long, conflicts_with_all = ["init", "tarefa", "tui"])]
    remember: Option<String>,

    /// Retoma uma sessão salva (`/save`, MT-121/ADR-0036) antes de rodar —
    /// sem valor, retoma a mais recente em `.agentry/session/`; com
    /// `--resume=<id-ou-nome>` (ou prefixo único), retoma aquela. Ambíguo ou
    /// inexistente é erro tratado, nunca uma sessão retomada silenciosamente
    /// errada. `require_equals` é deliberado (MT-122): sem isso, `clap`
    /// consome avidamente o próximo argumento posicional como valor de
    /// `--resume` — `agentry --resume "tarefa"` tentaria retomar uma sessão
    /// chamada "tarefa" em vez de rodar "tarefa" como tarefa *one-shot*
    /// (achado real de *smoke-test* manual).
    #[arg(long, num_args = 0..=1, default_missing_value = "", require_equals = true)]
    resume: Option<String>,

    /// Grava a credencial de `<provider>` em `~/.agentry/credentials.json`
    /// (MT-129/ADR-0038) e sai, sem rodar nenhuma tarefa — o valor da
    /// credencial é lido de *stdin* (uma linha), nunca como argumento de
    /// linha de comando (apareceria em `ps`/histórico do shell) nem
    /// ecoado de volta no terminal (mesmo princípio da skill
    /// `secrets-guard`). Preserva qualquer outra credencial já gravada —
    /// só substitui a entrada de `<provider>`.
    #[arg(long, value_name = "provider", conflicts_with_all = ["init", "tarefa", "tui", "undo", "remember", "resume"])]
    set_credential: Option<String>,

    /// Roda como **servidor MCP** sobre *stdio* (ADR-0042) em vez de iniciar
    /// uma sessão: expõe as tools deste projeto — sob as mesmas
    /// `permissions`/`readAllow` (ADR-0041) — para um cliente MCP externo,
    /// tipicamente o `claude -p` lançado pelo provider `claude-cli`.
    ///
    /// Não é feito para ser chamado à mão: o *stdout* é o canal do protocolo
    /// JSON-RPC, então qualquer coisa impressa ali quebraria o handshake (todo
    /// diagnóstico vai para *stderr*).
    #[arg(long, conflicts_with_all = ["init", "tarefa", "tui", "undo", "remember", "resume", "set_credential"])]
    mcp_server: bool,
}

/// Resultado de [`run_init_local`] — usado tanto por `--init` quanto por
/// `/init` (MT-41) para formatar a mesma mensagem sem duplicar a decisão.
enum InitOutcome {
    /// Arquivo criado agora, no caminho dado.
    Created(std::path::PathBuf),
    /// Arquivo já existia — não sobrescrito (mesma idempotência de
    /// `state_dir::ensure_state_dir` para o `.gitignore`, MT-38).
    AlreadyExists(std::path::PathBuf),
}

/// Grava `conteudo` em `.agentry/agentry.settings.json` — reaproveita
/// `state_dir::ensure_state_dir` (cria o diretório de estado e seu
/// `.gitignore`, MT-38) e `state_dir::agentry_settings_path` (MT-39) para
/// resolver o caminho final; nunca sobrescreve um arquivo já existente.
/// Compartilhada entre o bootstrap local (MT-41) e o via rede com
/// `--profile` (MT-42) — a diferença entre os dois é só de onde `conteudo`
/// vem, nunca de como é gravado.
///
/// # Errors
///
/// Devolve o `io::Error` de criar o diretório de estado ou escrever o
/// arquivo, sem tratamento especial.
fn write_settings_if_absent(
    workspace_root: &std::path::Path,
    conteudo: &str,
) -> io::Result<InitOutcome> {
    state_dir::ensure_state_dir(workspace_root)?;
    let caminho = state_dir::agentry_settings_path(workspace_root);
    if caminho.exists() {
        return Ok(InitOutcome::AlreadyExists(caminho));
    }
    std::fs::write(&caminho, conteudo)?;
    Ok(InitOutcome::Created(caminho))
}

/// Materializa `.agentry/agentry.settings.json` com o exemplo genérico do
/// schema mínimo (ADR-0018 §5) — bootstrap local, sem `--profile` (ADR-0019
/// §1), zero rede.
///
/// # Errors
///
/// Ver [`write_settings_if_absent`].
fn run_init_local(workspace_root: &std::path::Path) -> io::Result<InitOutcome> {
    write_settings_if_absent(workspace_root, GENERIC_SETTINGS_EXAMPLE)
}

/// Escreve o resultado de [`run_init_local`] em `output` — usado tanto por
/// `--init` quanto por `/init`, para não duplicar a mensagem entre CLI e REPL.
///
/// A dica de configuração manual (ADR-0019 §5) só sai quando o arquivo veio do
/// exemplo genérico (`veio_de_perfil == false`). Com `--profile`, os valores
/// diferenciados **já foram entregues** — mandar o usuário rodar um script para
/// obtê-los seria contradizer o que acabou de acontecer.
fn escrever_resultado_init(
    outcome: &InitOutcome,
    veio_de_perfil: bool,
    output: &mut impl io::Write,
) -> io::Result<()> {
    match outcome {
        InitOutcome::Created(caminho) => {
            writeln!(output, "criado: {}", caminho.display())?;
        }
        InitOutcome::AlreadyExists(caminho) => {
            writeln!(output, "já existe, não sobrescrito: {}", caminho.display())?;
        }
    }
    if veio_de_perfil {
        return Ok(());
    }
    writeln!(output, "{MANUAL_SETUP_HINT}")
}

/// Formata um [`agentry_core::model::Usage`] acumulado como texto legível
/// (MT-83, ADR-0029) — única fonte da string, usada pelo resumo do modo
/// *one-shot* (`[uso] ...` em `stderr`) e pelo comando `/usage` do REPL
/// (`crates/cli/src/repl.rs`), para nunca haver duas formatações
/// divergentes do mesmo dado.
pub(crate) fn formatar_uso(usage: agentry_core::model::Usage) -> String {
    format!(
        "{} tokens de entrada, {} de saída (total: {})",
        usage.input_tokens,
        usage.output_tokens,
        usage.total()
    )
}

/// Formata um [`agentry_core::checkpoint::UndoOutcome`] como texto legível
/// (MT-87, ADR-0030) — única fonte da string, usada pela flag `--undo`
/// (*one-shot*) e pelo comando `/undo` do REPL (`crates/cli/src/repl.rs`).
pub(crate) fn formatar_undo(outcome: &agentry_core::checkpoint::UndoOutcome) -> String {
    match outcome.acao {
        agentry_core::checkpoint::UndoAcao::Restaurado => {
            format!("'{}' restaurado ao conteúdo anterior", outcome.path)
        }
        agentry_core::checkpoint::UndoAcao::Removido => {
            format!(
                "'{}' removido (não existia antes da mudança desfeita)",
                outcome.path
            )
        }
    }
}

/// Mensagem de aviso quando a sessão para por
/// [`agentry_core::session::StopReason::MaxTurnsExceeded`] (MT-102,
/// ADR-0033) — `None` para qualquer outro motivo de parada. Única fonte da
/// string, usada pelos três pontos de exposição (REPL, *one-shot*, TUI),
/// mesma disciplina de [`formatar_uso`]/[`formatar_undo`].
pub(crate) fn mensagem_de_teto_de_turnos(
    outcome: &agentry_core::session::SessionOutcome,
) -> Option<String> {
    if outcome.reason != agentry_core::session::StopReason::MaxTurnsExceeded {
        return None;
    }
    Some(format!(
        "[aviso] parou após {} turnos consecutivos com uso de ferramenta (ADR-0033) — pode \
         continuar enviando outra mensagem",
        outcome.turns
    ))
}

/// Emite cada [`AuditEntry`] de egresso em stderr — suficiente para a v0.1;
/// persistência estruturada (arquivo/serviço) fica para quando houver
/// demanda concreta. Usa o `Display` de `AuditEntry` (uma linha compacta),
/// não `{:?}` — o *dump* de `Debug` poluía o stderr (achado real do teste
/// de usabilidade, `scripts/usability-test.sh`).
struct StderrAuditSink;

impl AuditSink for StderrAuditSink {
    fn record(&self, entry: AuditEntry) {
        eprintln!("[audit] {entry}");
    }
}

/// Emite cada [`GuardrailAuditEntry`] em stderr (MT-46) — `Display` já
/// compacto, uma linha por entrada, mesma disciplina do `impl AuditSink`
/// acima (nunca o `Debug` dump).
impl GuardrailAuditSink for StderrAuditSink {
    fn record(&self, entry: GuardrailAuditEntry) {
        eprintln!("{entry}");
    }
}

/// Descarta toda entrada de auditoria — usado só em testes desde o MT-125
/// (ADR-0037): o modo TUI (`--tui`, MT-72) trocou este sink por
/// `audit_sink::FileAuditSink` (`stderr` continua descartado sob `--tui`,
/// mas o audit log persistente não tem o mesmo problema de corromper a
/// tela). **Achado do smoke-test manual do MT-72, motivo original de
/// existir:** um `eprintln!` direto no modo bruto/tela-alternativa do
/// `crossterm` escreve por cima do buffer que o `ratatui` está desenhando
/// (ele não sabe da escrita, então não a repõe no próximo `draw`),
/// corrompendo a tela a cada chamada de rede.
#[cfg(test)]
struct NoopAuditSink;

#[cfg(test)]
impl AuditSink for NoopAuditSink {
    fn record(&self, _entry: AuditEntry) {}
}

#[cfg(test)]
impl GuardrailAuditSink for NoopAuditSink {
    fn record(&self, _entry: GuardrailAuditEntry) {}
}

/// Conecta a cada servidor MCP declarado em `cfg.mcp_servers` (Fase 16,
/// ADR-0028), descobre as tools que ele expõe (`McpClient::list_tools`,
/// MT-78) e registra cada uma no `registry` — nome prefixado pelo servidor
/// (`crates/core/src/tools/mcp.rs`, MT-79), sob o mesmo `PermissionGate`
/// de qualquer outra tool.
///
/// Falha ao conectar a um servidor (comando ausente no `PATH`, *handshake*
/// que não completa) é registrada em `diagnosticos` (MT-152 — quem decide o
/// canal de saída é `main`, não este ponto) e **não aborta a CLI** — os
/// demais servidores (e o restante da sessão) continuam normalmente.
/// Diferente de `register_context_tools` (nome/schema de cada tool é
/// estático, erro só aparece por chamada), aqui não há como registrar uma
/// tool "estática": sem uma conexão bem-sucedida não se sabe nem o nome
/// nem o schema dela — por isso a falha acontece na hora do registro, não
/// depois.
async fn register_mcp_tools(
    registry: &mut ToolRegistry,
    cfg: &Config,
    diagnosticos: &mut DiagnosticosDeInicializacao,
) {
    for (nome_servidor, servidor) in &cfg.mcp_servers {
        let cliente = match McpClient::start_from_settings(servidor).await {
            Ok(cliente) => Arc::new(cliente),
            Err(erro) => {
                diagnosticos.registrar(format!(
                    "erro ao conectar ao servidor MCP '{nome_servidor}': {erro}"
                ));
                continue;
            }
        };
        let tools = match cliente.list_tools().await {
            Ok(tools) => tools,
            Err(erro) => {
                diagnosticos.registrar(format!(
                    "erro ao listar tools do servidor MCP '{nome_servidor}': {erro}"
                ));
                continue;
            }
        };
        for tool in &tools {
            registry.register(Arc::new(McpTool::new(
                Arc::clone(&cliente),
                nome_servidor,
                tool,
            )));
        }
    }
}

/// Registra as 4 tools de contexto (`repo_map`, `code_search`,
/// `lsp_hover`/`lsp_definition`, `session_search`) segundo as flags
/// booleanas resolvidas por `Config` (MT-39/ADR-0018) — extraído de
/// `main()` para ser testável sem rodar o binário inteiro (parsing de
/// argv, rede real etc., MT-40). `ollama_provider` é reaproveitado do
/// provider Ollama já registrado no `Router` (embeddings/reranking do RAG
/// semântico, ADR-0011/ADR-0039), não um segundo cliente —
/// `session_search` (MT-136) usa exatamente o mesmo, nunca a task-class de
/// chat configurada (ADR-0039 §2). `session_search_session` já vem pronto
/// (em vez de construído aqui dentro, como as outras 3) porque `/recall`
/// (MT-137, REPL/TUI) reaproveita a **mesma instância** — mesmo cache de
/// índices entre a tool e o comando manual, não dois pipelines
/// independentes.
fn register_context_tools(
    registry: &mut ToolRegistry,
    cfg: &Config,
    workspace_root: &std::path::Path,
    ollama_provider: Arc<dyn LlmProvider>,
    modelo: &str,
    session_search_session: Arc<SessionSearchSession>,
) {
    register_repo_map_tool(
        registry,
        cfg.repo_map_enabled,
        RepoMapTool::new(workspace_root.to_path_buf(), cfg.respect_gitignore),
    );
    register_code_search_tool(
        registry,
        cfg.semantic_rag_enabled,
        Arc::new(CodeSearchSession::new(
            workspace_root.to_path_buf(),
            ollama_provider,
            modelo,
            modelo,
            cfg.respect_gitignore,
        )),
    );
    register_lsp_tools(
        registry,
        cfg.lsp_grounding_enabled,
        Arc::new(LspSession::new(
            DEFAULT_LSP_COMMAND,
            vec![],
            workspace_root.to_path_buf(),
        )),
    );
    register_session_search_tool(registry, cfg.session_search_enabled, session_search_session);
}

/// Resolve a `Config` final a partir das três camadas reais do binário, da
/// menos específica para a mais: preferências pessoais globais
/// (`~/.agentry/agentry.settings.json`, `Settings::from_global_file`,
/// MT-127/ADR-0038) `<` arquivo do projeto (`.agentry/agentry.settings.json`,
/// `Settings::from_file`, MT-39) `<` variáveis de ambiente
/// (`Settings::from_process_env`) — o projeto sempre pode sobrescrever uma
/// preferência pessoal global, e a variável de ambiente continua sendo o
/// *override* mais específico (efêmero, por invocação). Extraída de `main()`
/// para ser testável sem rodar o binário inteiro (MT-46, mesmo padrão do
/// MT-40/`register_context_tools`).
///
/// **Achado do MT-46:** até este ticket, `main()` só montava a `Config` a
/// partir de `Settings::from_process_env()` — a camada do arquivo nunca
/// entrava na chamada, apesar do MT-39/40 estarem fechados como "consumo
/// real"; as 4 flags de contexto/provider só funcionavam de fato via
/// variável de ambiente. Corrigido aqui porque o critério de aceite do
/// MT-46 depende diretamente disso (regra de guardrail no arquivo precisa
/// chegar à `Session` real).
///
/// # Errors
///
/// Propaga o `ConfigError` de qualquer uma das três camadas (arquivo
/// malformado/`schemaVersion` divergente, ou variável de ambiente numérica
/// inválida) — nunca *panic*.
fn build_config(
    workspace_root: &std::path::Path,
) -> Result<Config, agentry_core::config::ConfigError> {
    build_config_com_camada_global(workspace_root, Settings::from_global_file()?)
}

/// Núcleo de [`build_config`], recebendo a camada global já resolvida — os
/// testes deste módulo passam `Settings::default()` explicitamente em vez
/// de deixar `build_config` ler o `$HOME`/`%USERPROFILE%` **real** da
/// máquina que roda os testes (que poderia ter um
/// `~/.agentry/agentry.settings.json` de verdade, tornando o resultado do
/// teste dependente do ambiente de quem/onde ele roda) — mesmo cuidado de
/// hermeticidade já aplicado por `global_dir::home_dir_de`/
/// `Settings::from_env_vars` (injeção em vez de tocar o real).
fn build_config_com_camada_global(
    workspace_root: &std::path::Path,
    camada_global: Settings,
) -> Result<Config, agentry_core::config::ConfigError> {
    Ok(Config::resolve(vec![
        camada_global,
        Settings::from_file(workspace_root)?,
        Settings::from_process_env()?,
    ]))
}

/// Monta o `RuntimeOverride` inicial a partir das flags de invocação.
///
/// # Errors
///
/// Devolve erro se `--reasoning` vier com um valor que não seja `on`/`off`
/// (e variantes) — falha explícita em vez de ignorar a flag em silêncio.
fn overrides_from_args(args: &Args) -> Result<RuntimeOverride, String> {
    let reasoning = args
        .reasoning
        .as_deref()
        .map(parse_bool_toggle)
        .transpose()?;
    Ok(RuntimeOverride {
        provider: args.provider.clone(),
        model: args.model.clone(),
        temperature: args.temperature,
        top_p: args.top_p,
        system_prompt: args.system.clone(),
        max_tokens: args.max_tokens,
        reasoning,
    })
}

/// Provider já pronto para registrar no `Router` + o candidato de rota
/// correspondente — par devolvido por [`build_litellm_provider`].
type RegistroDeProvider = (Arc<dyn LlmProvider>, RouteTarget);

/// Monta o provider LiteLLM e o candidato de rota correspondente, a partir
/// de `cfg.litellm` (`providers.litellm`, MT-48) — `None` se LiteLLM não
/// estiver configurado (comportamento atual preservado: só Ollama).
///
/// Transporte dedicado (mesma disciplina de instância própria já usada pelo
/// bootstrap `--profile`, ADR-0019): allowlist restrita ao host de
/// `base_url` sob a `egress_class` já resolvida por `Config` — nunca
/// inferida aqui, só lida da configuração (ADR-0006: proibido tratar
/// endpoint de proxy como `local-only` por inferência de host). `api_key`
/// (tipicamente de `AGENTRY_LITELLM_API_KEY`, lida por `main` — nunca por
/// esta função, para não acoplar a testes ao ambiente de processo real) é
/// anexada como `Authorization: Bearer` só quando `Some`; gateways sem
/// autenticação continuam funcionando com `None`.
///
/// # Errors
///
/// Devolve erro se `providers.litellm.baseUrl` não puder ser interpretada
/// como URL válida com host.
fn build_litellm_provider(
    cfg: &Config,
    api_key: Option<&str>,
    audit_sink: Arc<dyn AuditSink>,
) -> Result<Option<RegistroDeProvider>, String> {
    let Some(litellm) = &cfg.litellm else {
        return Ok(None);
    };

    let host = host_from_url(&litellm.base_url)
        .map_err(|erro| format!("providers.litellm.baseUrl inválida: {erro}"))?;
    let allowlist = Allowlist::new(vec![AllowlistEntry::new(host, litellm.egress_class)]);
    let mut transport = Transport::new(
        allowlist,
        cfg.egress_class,
        cfg.profile.map(|p| format!("{p:?}")),
        audit_sink,
    );
    if let Some(chave) = api_key {
        transport = transport.with_header("Authorization", format!("Bearer {chave}"));
    }

    let provider: Arc<dyn LlmProvider> = Arc::new(OpenAiCompatProvider::new(
        Arc::new(transport),
        litellm.base_url.clone(),
        LITELLM_PROVIDER_NAME,
    ));
    let candidato = RouteTarget::new(
        LITELLM_PROVIDER_NAME,
        litellm.model.clone(),
        litellm.egress_class,
    );
    Ok(Some((provider, candidato)))
}

/// Monta o provider Anthropic (Messages API por chave, ADR-0040) e o
/// candidato de rota correspondente, a partir de `cfg.anthropic`
/// (`providers.anthropic`) — `None` se não estiver configurado, mesmo padrão
/// de [`build_litellm_provider`].
///
/// `Transport` dedicado com `Allowlist` restrita ao host de `base_url` sob a
/// `egress_class` já resolvida por `Config` — nunca inferida aqui. A chave
/// (de `ANTHROPIC_API_KEY` ou de `~/.agentry/credentials.json`, resolvida por
/// `main` — nunca por esta função, para não acoplar os testes ao ambiente do
/// processo real) vai no header `x-api-key`, **não** em `Authorization:
/// Bearer` como no LiteLLM: a Messages API usa um esquema próprio, junto do
/// header obrigatório `anthropic-version`.
///
/// Diferente do LiteLLM, a chave aqui é **obrigatória** — a Anthropic API não
/// tem modo anônimo, então configurar o provider sem credencial é erro de
/// configuração, reportado, e não uma requisição que falharia com 401 no
/// primeiro uso.
///
/// # Errors
///
/// Devolve erro se `providers.anthropic.baseUrl` não puder ser interpretada
/// como URL válida com host, ou se nenhuma credencial for encontrada.
fn build_anthropic_provider(
    cfg: &Config,
    api_key: Option<&str>,
    audit_sink: Arc<dyn AuditSink>,
) -> Result<Option<RegistroDeProvider>, String> {
    let Some(anthropic) = &cfg.anthropic else {
        return Ok(None);
    };

    let Some(chave) = api_key else {
        return Err(format!(
            "providers.anthropic está configurado (model: {}) mas nenhuma credencial foi \
             encontrada — defina {ANTHROPIC_API_KEY_ENV} ou grave a chave com \
             `agentry --set-credential {ANTHROPIC_PROVIDER_NAME}`",
            anthropic.model
        ));
    };

    let host = host_from_url(&anthropic.base_url)
        .map_err(|erro| format!("providers.anthropic.baseUrl inválida: {erro}"))?;
    let allowlist = Allowlist::new(vec![AllowlistEntry::new(host, anthropic.egress_class)]);
    let transport = Transport::new(
        allowlist,
        cfg.egress_class,
        cfg.profile.map(|p| format!("{p:?}")),
        audit_sink,
    )
    .with_header("x-api-key", chave)
    .with_header("anthropic-version", ANTHROPIC_VERSION);

    let provider: Arc<dyn LlmProvider> = Arc::new(AnthropicProvider::new(
        Arc::new(transport),
        anthropic.base_url.clone(),
    ));
    let candidato = RouteTarget::new(
        ANTHROPIC_PROVIDER_NAME,
        anthropic.model.clone(),
        anthropic.egress_class,
    );
    Ok(Some((provider, candidato)))
}

/// Monta o provider `claude-cli` (assinatura Pro/Max via subprocesso,
/// ADR-0040) e o candidato de rota correspondente — `None` se
/// `providers.claudeCli.model` não estiver declarado.
///
/// **Sem `Transport`**: o egresso sai por um subprocesso, então não há
/// `Allowlist` nem audit log de transporte no caminho. O `ClaudeCliProvider`
/// compensa emitindo ele próprio uma `AuditEntry` por invocação, no **mesmo**
/// `audit_sink` da CLI — por isso o sink é passado aqui, e não um `Transport`.
/// A `egress_class` **da sessão** vai junto para o provider poder recusar o
/// *spawn* antes de criar o processo (ver a ADR-0040 §Decisão).
/// Avisos de inicialização que **não** derrubam a sessão: provider
/// configurado mas indisponível, servidor MCP que não conectou, ponte MCP
/// que não montou. Diferente de todo outro erro de partida, estes seguem
/// adiante — e por isso precisam de um lugar para ser vistos.
///
/// **MT-152.** Até aqui cada um deles ia direto para `stderr` no ponto em
/// que acontecia. Nos modos de texto isso funciona; sob `--tui`, não: o
/// `crossterm` entra na tela alternativa logo depois, e o que já estava
/// escrito no terminal some junto — a mensagem só reaparece quando quem usa
/// **sai** da TUI, momento em que ela não serve mais para nada. O achado
/// concreto do teste de uso de 2026-09-08 foi o erro de servidor MCP:
/// invisível durante toda a sessão, apesar de o `--init` daquele momento
/// garantir que ele aconteceria (MT-151).
///
/// A correção não é escrever em `stderr` mais cedo — é **não escolher o
/// canal no ponto de origem**. Quem produz o aviso só o registra aqui; quem
/// sabe qual é a superfície de saída (`main`) decide onde ele aparece:
/// `stderr` nos modos de texto, mensagem de sistema no histórico da TUI.
#[derive(Debug, Default)]
struct DiagnosticosDeInicializacao {
    mensagens: Vec<String>,
}

impl DiagnosticosDeInicializacao {
    fn registrar(&mut self, mensagem: String) {
        self.mensagens.push(mensagem);
    }

    /// Escoa para `stderr`, na ordem em que os avisos ocorreram — o
    /// comportamento literal de antes do MT-152, para os modos de texto.
    fn escoar_para_stderr(&self) {
        for mensagem in &self.mensagens {
            eprintln!("{mensagem}");
        }
    }

    fn mensagens(&self) -> &[String] {
        &self.mensagens
    }
}

fn build_claude_cli_provider(
    cfg: &Config,
    audit_sink: Arc<dyn AuditSink>,
    montar_ponte_mcp: bool,
    diagnosticos: &mut DiagnosticosDeInicializacao,
) -> Option<RegistroDeProvider> {
    let claude_cli = cfg.claude_cli.as_ref()?;

    // Binário ausente ⇒ provider **não registrado**, em vez de registrado e
    // falhando no primeiro uso. Isso é o que permite a configuração padrão
    // declarar `claude-cli` e `ollama` como candidatos da mesma task-class:
    // quem não tem o Claude Code instalado cai no Ollama sozinho, sem editar
    // nada. O aviso existe para a queda nunca ser silenciosa.
    if !binario_no_path(agentry_core::provider::claude_cli::CLAUDE_BINARY) {
        diagnosticos.registrar(
            "[claude-cli] aviso: providers.claudeCli está configurado mas o binário 'claude' \
             não está no PATH — provider não registrado (instale o Claude Code e rode \
             `claude login` para usá-lo). Os demais candidatos da rota seguem valendo."
                .to_string(),
        );
        return None;
    }

    let mut provider = ClaudeCliProvider::new(
        audit_sink,
        cfg.egress_class,
        cfg.profile.map(|p| format!("{p:?}")),
    );

    // Ponte MCP (ADR-0042): devolve ao subprocesso as tools do `agentry`, sob
    // a mesma política. Falha ao montar a ponte é avisada e **degrada para
    // texto puro** em vez de derrubar a sessão — o provider continua útil sem
    // ela, e um erro fatal aqui seria desproporcional. Nunca o contrário:
    // jamais seguir em silêncio dando a impressão de que as tools chegaram.
    // `montar_ponte_mcp` é `false` quando este processo **é** o servidor MCP:
    // um servidor não tem motivo para lançar o `claude`, e montar a ponte ali
    // criaria um arquivo temporário que nunca seria removido — o processo
    // servidor é encerrado pelo cliente, sem rodar destrutores (achado real:
    // sobrava um `/tmp/agentry-mcp-<pid>.json` por execução).
    if claude_cli.mcp_tools && montar_ponte_mcp {
        match std::env::current_exe()
            .map_err(|e| format!("não foi possível localizar o próprio binário: {e}"))
            .and_then(|exe| {
                PonteMcp::nova(&exe, mcp_server::NOME_DO_SERVIDOR, &std::env::temp_dir())
            }) {
            Ok(ponte) => provider = provider.with_ponte_mcp(ponte),
            Err(erro) => diagnosticos.registrar(format!(
                "[claude-cli] aviso: providers.claudeCli.mcpTools está ligado mas a ponte MCP \
                 não pôde ser montada ({erro}) — seguindo sem tool-calling (só texto)."
            )),
        }
    }

    let provider: Arc<dyn LlmProvider> = Arc::new(provider);
    let candidato = RouteTarget::new(
        CLAUDE_CLI_PROVIDER_NAME,
        claude_cli.model.clone(),
        claude_cli.egress_class,
    );
    Some((provider, candidato))
}

/// `true` se `nome` é encontrado em algum diretório do `PATH`.
///
/// Só consulta o sistema de arquivos — não executa nada (executar para
/// descobrir se existe teria efeito colateral e custo de processo).
fn binario_no_path(nome: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| {
        let candidato = dir.join(nome);
        // No Windows o executável tem extensão; `PATHEXT` completo é exagero
        // aqui, `.exe`/`.cmd` cobrem o instalador do Claude Code.
        candidato.is_file()
            || candidato.with_extension("exe").is_file()
            || candidato.with_extension("cmd").is_file()
    })
}

/// Monta a tool `web_fetch` (MT-65, ADR-0025) só quando as **duas**
/// condições valem: `tools.webFetch.enabled` (*opt-in* explícito) **e**
/// `cfg.egress_class == CloudOk` (acesso amplo à internet é a capacidade
/// mais permissiva da taxonomia, ADR-0002) — ausência de qualquer uma das
/// duas e a tool simplesmente não é registrada, nunca aparece para o
/// modelo. `Transport` dedicado (mesmo padrão de [`build_litellm_provider`])
/// com o coringa [`agentry_core::egress::allowlist::ANY_HOST`] exigindo
/// `CloudOk` e o `User-Agent` genérico fixo (ADR-0025 — nunca o *default*
/// do `reqwest`).
fn build_web_fetch_tool(cfg: &Config, audit_sink: Arc<dyn AuditSink>) -> Option<WebFetchTool> {
    use agentry_core::config::privacy::EgressClass;

    if !cfg.web_fetch_enabled || cfg.egress_class != EgressClass::CloudOk {
        return None;
    }
    let allowlist = Allowlist::new(vec![AllowlistEntry::new(ANY_HOST, EgressClass::CloudOk)]);
    let transport = Transport::new(
        allowlist,
        cfg.egress_class,
        cfg.profile.map(|p| format!("{p:?}")),
        audit_sink,
    )
    .with_header("User-Agent", WEB_TOOL_USER_AGENT);
    Some(WebFetchTool::new(Arc::new(transport)))
}

/// Monta a tool `web_search` (MT-66, ADR-0025) só quando `tools.webSearch.searxngUrl`
/// está declarado (`cfg.web_search`, `Config::resolve`) — mesmo padrão de
/// `providers.litellm` (ausência ⇒ não registrada). `Transport` dedicado
/// (mesmo padrão de [`build_litellm_provider`]) com a `Allowlist` do host
/// único do endpoint (**sem** o coringa `ANY_HOST` do `web_fetch` — o host
/// é conhecido) e o `User-Agent` genérico fixo.
fn build_web_search_tool(
    cfg: &Config,
    audit_sink: Arc<dyn AuditSink>,
) -> Result<Option<WebSearchTool>, String> {
    let Some(web_search) = &cfg.web_search else {
        return Ok(None);
    };

    let host = host_from_url(&web_search.searxng_url)
        .map_err(|erro| format!("tools.webSearch.searxngUrl inválida: {erro}"))?;
    let allowlist = Allowlist::new(vec![AllowlistEntry::new(host, web_search.egress_class)]);
    let transport = Transport::new(
        allowlist,
        cfg.egress_class,
        cfg.profile.map(|p| format!("{p:?}")),
        audit_sink,
    )
    .with_header("User-Agent", WEB_TOOL_USER_AGENT);
    Ok(Some(WebSearchTool::new(
        Arc::new(transport),
        web_search.searxng_url.clone(),
    )))
}

/// Nomes das task-classes internas **auxiliares** — além de `chat`, que já é
/// sintetizada por [`repl::set_chat_route`] antes desta função rodar (MT-14),
/// e que continua sem rota real hoje: `compact` (`/compact`, ADR-0016) e
/// `guardrail-compliance` (Reviewer, ADR-0015). Ambas ficam sem candidato
/// nenhum na CLI distribuída até este ticket — `/compact` falhava com
/// `RouterError::UnknownTaskClass` em qualquer sessão real.
const TASK_CLASSES_AUXILIARES: [&str; 2] = ["compact", "guardrail-compliance"];

/// Registra no `router` toda `task-class` declarada em `cfg.task_classes`
/// (MT-55/ADR-0021) e sintetiza os defaults internos de
/// [`TASK_CLASSES_AUXILIARES`] para os nomes **ausentes** do bloco
/// declarado — mesmo par `(Ollama local-only [+ LiteLLM se configurado],
/// CallPreset::default())` já usado por `chat` via [`repl::set_chat_route`],
/// preservando zero-config idêntico ao comportamento anterior ao MT-56.
///
/// Responsabilidade herdada do desvio registrado no MT-55
/// (`docs/decisoes-autonomas.md`): `crates/core` não sintetiza esses
/// defaults por não dever conhecer `"ollama"` como escolha de produto; a
/// CLI é o lugar certo, por já hardcodar essa escolha em
/// [`repl::set_chat_route`] hoje.
///
/// Task-classes declaradas sempre vencem — inclusive um `chat`/`compact`/
/// `guardrail-compliance` customizado pelo usuário, que substitui o default
/// sintetizado do mesmo nome (`Router::set_route` roda por último para cada
/// entrada declarada).
fn register_declared_task_classes(
    router: &mut Router,
    cfg: &Config,
    modelo_inicial: &str,
    candidatos_extra: &[RouteTarget],
) {
    for nome in TASK_CLASSES_AUXILIARES {
        if !cfg.task_classes.contains_key(nome) {
            let mut candidates = vec![RouteTarget::new(
                repl::PROVIDER,
                modelo_inicial,
                agentry_core::config::privacy::EgressClass::LocalOnly,
            )];
            candidates.extend_from_slice(candidatos_extra);
            router.set_route(
                nome,
                RouteEntry {
                    candidates,
                    preset: CallPreset::default(),
                },
            );
        }
    }
    for (nome, entry) in &cfg.task_classes {
        router.set_route(nome.clone(), entry.clone());
    }
}

/// Monta um `Router` completo (providers registrados, `"chat"` declarada,
/// task-classes auxiliares/configuradas registradas) — função pura dos
/// insumos dados, para poder ser chamada mais de uma vez com o mesmo
/// resultado (MT-91/ADR-0031: o subagente ganha sua própria instância
/// "equivalente" à do laço principal, já que `Router` não é `Clone`/
/// compartilhável sob mutação concorrente).
/// `registros_extra` são os providers de nuvem opcionais, na ordem de
/// preferência em que devem entrar como candidatos depois do Ollama local:
/// LiteLLM (`providers.litellm`, MT-49) e Anthropic (`providers.anthropic`,
/// ADR-0040). Uma lista (e não um `Option` por provider) porque o número de
/// providers opcionais cresce — o `claude-cli` da ADR-0040 é o próximo.
fn montar_router(
    egress_class: agentry_core::config::privacy::EgressClass,
    ollama: Arc<dyn LlmProvider>,
    registros_extra: Vec<RegistroDeProvider>,
    modelo_inicial: &str,
    cfg: &Config,
) -> Router {
    let mut router = Router::new(egress_class);
    router.register_provider(ollama);
    let candidatos_extra: Vec<RouteTarget> = registros_extra
        .into_iter()
        .map(|(provider, candidato)| {
            router.register_provider(provider);
            candidato
        })
        .collect();
    repl::set_chat_route(
        &mut router,
        modelo_inicial,
        &CallPreset::default(),
        &candidatos_extra,
    );
    register_declared_task_classes(&mut router, cfg, modelo_inicial, &candidatos_extra);
    router
}

/// Registra a tool `subagent` (MT-91/ADR-0031) em `registry` — o executor
/// que ela usa internamente vem de um **segundo** `ToolRegistry`, com as
/// MESMAS tools já registradas em `registry` até aqui (mesmas instâncias
/// `Arc<dyn Tool>`, reaproveitadas — nenhuma reconstrução), mas que
/// **nunca** registra a própria `SubagentTool`: recursão (subagente
/// criando subagente) fica impossível **estruturalmente**, não por uma
/// checagem em tempo de execução. `router_subagente` é a instância de
/// `Router` "equivalente" construída por [`montar_router`] — ver a doc
/// daquela função para por que não é literalmente o mesmo objeto
/// compartilhado do laço principal.
fn register_subagent_tool(
    registry: &mut ToolRegistry,
    permissions: agentry_core::config::Permissions,
    workspace_root: &std::path::Path,
    confirmer: Arc<dyn Confirmer>,
    router_subagente: Arc<Router>,
    guardrails: Option<(Arc<GuardrailGate>, Arc<dyn GuardrailAuditSink>)>,
) {
    // `permissions` aqui é `cfg.subagent_permissions` (ADR-0041) — já
    // resolvida por `Config::resolve` para uma cópia de `cfg.permissions`
    // quando nenhuma camada declarou `subagentPermissions`. É o que permite
    // negar `fs_read` ao agente principal sem cegar o subagente.
    let mut registry_subagente =
        ToolRegistry::new(PermissionGate::new(permissions).with_workspace_root(workspace_root));
    for tool in registry.tools() {
        registry_subagente.register(Arc::clone(tool));
    }
    let executor_subagente: Arc<dyn ToolExecutor> =
        Arc::new(RegistryToolExecutor::new(registry_subagente, confirmer));
    registry.register(Arc::new(SubagentTool::new(
        router_subagente,
        executor_subagente,
        guardrails,
    )));
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    if args.init {
        let workspace_root = std::env::current_dir().unwrap_or_else(|erro| {
            eprintln!("erro ao ler diretório de trabalho: {erro}");
            std::process::exit(1)
        });
        let resultado = match &args.profile {
            Some(perfil) => {
                let sink: Arc<dyn AuditSink> = Arc::new(audit_sink::SinksCombinados::new(
                    StderrAuditSink,
                    audit_sink::FileAuditSink::new(workspace_root.clone()),
                ));
                match init::fetch_profile_settings(perfil, sink).await {
                    Ok(conteudo) => write_settings_if_absent(&workspace_root, &conteudo),
                    Err(erro) => {
                        eprintln!("erro ao buscar configuração do perfil: {erro}");
                        std::process::exit(1)
                    }
                }
            }
            None => run_init_local(&workspace_root),
        };
        match resultado {
            Ok(outcome) => {
                escrever_resultado_init(&outcome, args.profile.is_some(), &mut io::stdout())
                    .unwrap_or_else(|erro| {
                        eprintln!("erro: {erro}");
                        std::process::exit(1)
                    });
            }
            Err(erro) => {
                eprintln!("erro ao inicializar configuração: {erro}");
                std::process::exit(1)
            }
        }
        return;
    }

    if args.undo {
        let workspace_root = std::env::current_dir().unwrap_or_else(|erro| {
            eprintln!("erro ao ler diretório de trabalho: {erro}");
            std::process::exit(1)
        });
        let store = agentry_core::checkpoint::CheckpointStore::new(&workspace_root);
        match store.undo() {
            Ok(outcome) => println!("{}", formatar_undo(&outcome)),
            Err(erro) => {
                eprintln!("erro: {erro}");
                std::process::exit(1)
            }
        }
        return;
    }

    if let Some(fato) = &args.remember {
        let workspace_root = std::env::current_dir().unwrap_or_else(|erro| {
            eprintln!("erro ao ler diretório de trabalho: {erro}");
            std::process::exit(1)
        });
        let store = agentry_core::memory::MemoryStore::new(&workspace_root);
        match store.remember(fato.clone()) {
            Ok(()) => println!("lembrado: {fato}"),
            Err(erro) => {
                eprintln!("erro: {erro}");
                std::process::exit(1)
            }
        }
        return;
    }

    if let Some(provider) = &args.set_credential {
        eprint!("valor da credencial para '{provider}': ");
        let mut valor = String::new();
        io::stdin().read_line(&mut valor).unwrap_or_else(|erro| {
            eprintln!("erro ao ler stdin: {erro}");
            std::process::exit(1)
        });
        let valor = valor.trim();
        if valor.is_empty() {
            eprintln!("erro: valor da credencial não pode ser vazio");
            std::process::exit(1);
        }
        match agentry_core::credentials::set_api_key(provider, valor) {
            Ok(caminho) => println!(
                "credencial de '{provider}' gravada em {}",
                caminho.display()
            ),
            Err(erro) => {
                eprintln!("erro: {erro}");
                std::process::exit(1)
            }
        }
        return;
    }

    let overrides = overrides_from_args(&args).unwrap_or_else(|erro| {
        eprintln!("erro: {erro}");
        std::process::exit(2)
    });

    let workspace_root = std::env::current_dir().unwrap_or_else(|erro| {
        eprintln!("erro ao ler diretório de trabalho: {erro}");
        std::process::exit(1)
    });

    let cfg = build_config(&workspace_root).unwrap_or_else(|erro| {
        eprintln!("erro de configuração: {erro}");
        std::process::exit(2)
    });
    // Construído cedo e compartilhado (`Arc::clone`) entre a `Session`
    // principal e o `SubagentTool` (MT-91/ADR-0031) — mesmo `GuardrailGate`,
    // nenhum mecanismo paralelo.
    let guardrail_gate = Arc::new(cfg.guardrails.clone());

    // Sob `--tui`, `eprintln!` corrompe a tela alternativa do `crossterm`
    // (achado do smoke-test manual do MT-72) — o lado `stderr` fica
    // descartado nesse modo (ver doc de `NoopAuditSink`), mas o
    // `FileAuditSink` (MT-125, ADR-0037) grava em `.agentry/audit.log`
    // independente do modo: é auditoria persistente, não uma saída de
    // terminal, então não tem o mesmo problema de corromper a tela.
    let audit_sink: Arc<dyn AuditSink> = if args.tui {
        Arc::new(audit_sink::FileAuditSink::new(workspace_root.clone()))
    } else {
        Arc::new(audit_sink::SinksCombinados::new(
            StderrAuditSink,
            audit_sink::FileAuditSink::new(workspace_root.clone()),
        ))
    };
    let guardrail_audit_sink: Arc<dyn GuardrailAuditSink> = if args.tui {
        Arc::new(audit_sink::FileAuditSink::new(workspace_root.clone()))
    } else {
        Arc::new(audit_sink::SinksCombinados::new(
            StderrAuditSink,
            audit_sink::FileAuditSink::new(workspace_root.clone()),
        ))
    };

    let ollama_ip = args
        .ollama_host
        .split(':')
        .next()
        .unwrap_or(&args.ollama_host)
        .to_string();
    let allowlist = Allowlist::new(vec![AllowlistEntry::new(
        ollama_ip,
        agentry_core::config::privacy::EgressClass::LocalOnly,
    )]);
    let transport = Arc::new(Transport::new(
        allowlist,
        cfg.egress_class,
        cfg.profile.map(|p| format!("{p:?}")),
        Arc::clone(&audit_sink),
    ));
    let ollama = Arc::new(
        OllamaProvider::new(transport, format!("http://{}", args.ollama_host))
            .with_structured_output(cfg.ollama_structured_output),
    );
    // Clonado antes de `register_provider` consumir o Arc — o repo_map/RAG
    // semântico reaproveita o mesmo provider Ollama para embeddings/reranking
    // (ADR-0011), não um segundo cliente.
    let ollama_provider: Arc<dyn LlmProvider> = ollama.clone();

    // Variável de ambiente sempre vence; `~/.agentry/credentials.json`
    // (MT-128, ADR-0038) só é consultado quando `AGENTRY_LITELLM_API_KEY`
    // não está definida -- nunca os dois somados.
    let chave_litellm = agentry_core::credentials::resolve_api_key(
        LITELLM_PROVIDER_NAME,
        std::env::var(LITELLM_API_KEY_ENV).ok(),
    )
    .unwrap_or_else(|erro| {
        eprintln!("erro ao ler credenciais: {erro}");
        std::process::exit(2)
    });
    let litellm_registro: Option<RegistroDeProvider> =
        build_litellm_provider(&cfg, chave_litellm.as_deref(), Arc::clone(&audit_sink))
            .unwrap_or_else(|erro| {
                eprintln!("erro de configuração: {erro}");
                std::process::exit(2)
            });

    // Mesma disciplina do LiteLLM (ADR-0038): variável de ambiente vence e
    // curto-circuita antes de qualquer I/O; `~/.agentry/credentials.json` só
    // é consultado quando ela não está definida.
    let chave_anthropic = agentry_core::credentials::resolve_api_key(
        ANTHROPIC_PROVIDER_NAME,
        std::env::var(ANTHROPIC_API_KEY_ENV).ok(),
    )
    .unwrap_or_else(|erro| {
        eprintln!("erro ao ler credenciais: {erro}");
        std::process::exit(2)
    });
    let anthropic_registro: Option<RegistroDeProvider> =
        build_anthropic_provider(&cfg, chave_anthropic.as_deref(), Arc::clone(&audit_sink))
            .unwrap_or_else(|erro| {
                eprintln!("erro de configuração: {erro}");
                std::process::exit(2)
            });

    // Coletor único dos avisos não-fatais de partida (MT-152) — preenchido
    // aqui e por `register_mcp_tools` logo abaixo, escoado uma vez só,
    // depois que se sabe qual é a superfície de saída desta execução.
    let mut diagnosticos = DiagnosticosDeInicializacao::default();
    let claude_cli_registro = build_claude_cli_provider(
        &cfg,
        Arc::clone(&audit_sink),
        !args.mcp_server,
        &mut diagnosticos,
    );

    // Ordem = preferência de candidato depois do Ollama local.
    let registros_extra: Vec<RegistroDeProvider> = litellm_registro
        .into_iter()
        .chain(anthropic_registro)
        .chain(claude_cli_registro)
        .collect();

    let modelo_inicial = args
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());

    // `Router` não é `Clone` (mutável em tempo de execução via `/model`/
    // `/task-class`, ADR-0014) — para o subagente (MT-91/ADR-0031) ter sua
    // própria instância "equivalente" (mesmos providers, mesmas task-classes
    // declaradas, mesma classe de egresso), monta-se uma segunda vez com os
    // mesmos insumos já computados (`ollama`/`litellm_registro` clonados,
    // nenhuma reconstrução de verdade — nenhum I/O novo). Consequência
    // documentada: o subagente não reflete uma troca de modelo/task-class
    // feita via `/model`/`/task-class` **depois** que a CLI já inicializou
    // — só o que já estava declarado na configuração no arranque.
    let mut router = montar_router(
        cfg.egress_class,
        ollama.clone(),
        registros_extra.clone(),
        &modelo_inicial,
        &cfg,
    );
    // O REPL precisa de **todos** os candidatos extras, não só do primeiro:
    // `/model` redeclara a rota inteira (ver `repl::set_chat_route`), então um
    // candidato ausente daqui sumiria na primeira troca de modelo.
    let candidatos_extra: Vec<RouteTarget> = registros_extra
        .iter()
        .map(|(_, candidato)| candidato.clone())
        .collect();
    let router_subagente = Arc::new(montar_router(
        cfg.egress_class,
        ollama,
        registros_extra,
        &modelo_inicial,
        &cfg,
    ));

    let task_class = args
        .task_class
        .clone()
        .unwrap_or_else(|| repl::TASK_CLASS.to_string());
    let rota = router
        .resolve_with_override(&task_class, &overrides)
        .unwrap_or_else(|erro| {
            eprintln!("erro ao resolver rota: {erro}");
            std::process::exit(1)
        });

    // Único `Gitignore` reaproveitado por instruções de projeto (MT-59),
    // descoberta de skills (MT-60) e a tool `skill` (MT-61) — mesma
    // checagem de confidencialidade (`.agentryignore`/`.claudeignore`,
    // ADR-0020) em todos os três, sem reconstruir o matcher a cada um.
    let context_ignore =
        agentry_core::tools::fs::load_ignore(&workspace_root, cfg.respect_gitignore);
    // Descoberta de skills (MT-60/ADR-0023) não tem *opt-out* próprio —
    // custo desprezível (só nome+descrição na mensagem de sistema; corpo
    // lido sob demanda pela tool `skill`) e listar as skills disponíveis
    // não é uma decisão de confidencialidade, diferente de
    // `context.agentsFile.enabled`.
    let skills_descobertas =
        agentry_core::skills::discover_skills(&workspace_root, &context_ignore);

    // Modo TUI (MT-74/ADR-0027): print!/read_line brigam com o modo bruto do
    // terminal (achado do MT-72) — TuiConfirmer/TuiPrompter enviam o pedido
    // por canal ao laço de eventos da TUI em vez de ler stdin diretamente.
    // `rx_humano`/`auto_confirmacao` só são consumidos no ramo `--tui` do
    // despacho abaixo; fora dele, o par simplesmente não é usado.
    let auto_confirmacao = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (tx_humano, rx_humano) = tokio::sync::mpsc::unbounded_channel();
    let (prompter, confirmer): (Arc<dyn Prompter>, Arc<dyn Confirmer>) = if args.tui {
        (
            Arc::new(tui::TuiPrompter::new(tx_humano.clone())),
            Arc::new(tool_executor::TuiConfirmer::new(
                tx_humano.clone(),
                Arc::clone(&auto_confirmacao),
                workspace_root.clone(),
            )),
        )
    } else {
        (
            Arc::new(InteractivePrompter),
            Arc::new(InteractiveConfirmer),
        )
    };

    let mut registry = ToolRegistry::new(
        PermissionGate::new(cfg.permissions.clone()).with_workspace_root(&workspace_root),
    );
    registry.register(Arc::new(FsReadTool::new(
        workspace_root.clone(),
        cfg.respect_gitignore,
    )));
    let checkpoint_store = Arc::new(agentry_core::checkpoint::CheckpointStore::new(
        workspace_root.clone(),
    ));
    registry.register(Arc::new(CheckpointingTool::new(
        Arc::new(FsWriteTool::new(
            workspace_root.clone(),
            cfg.respect_gitignore,
        )),
        workspace_root.clone(),
        Arc::clone(&checkpoint_store),
    )));
    registry.register(Arc::new(CheckpointingTool::new(
        Arc::new(FsEditTool::new(
            workspace_root.clone(),
            cfg.respect_gitignore,
        )),
        workspace_root.clone(),
        Arc::clone(&checkpoint_store),
    )));
    registry.register(Arc::new(FsSearchTool::new(
        workspace_root.clone(),
        cfg.respect_gitignore,
    )));
    registry.register(Arc::new(GlobTool::new(
        workspace_root.clone(),
        cfg.respect_gitignore,
    )));
    // Sem padrões de `allow` configuráveis ainda (fora de escopo do MT-14):
    // shell fica bloqueado por padrão (default-deny da `ShellPolicy`, MT-13).
    registry.register(Arc::new(ShellTool::new(ShellPolicy::new(vec![]))));
    registry.register(Arc::new(ShellBackgroundTool::new(ShellPolicy::new(vec![]))));
    registry.register(Arc::new(SkillTool::new(skills_descobertas.clone())));
    registry.register(Arc::new(AskUserTool::new(prompter)));
    registry.register(Arc::new(TodoWriteTool::new()));
    if let Some(web_fetch) = build_web_fetch_tool(&cfg, Arc::clone(&audit_sink)) {
        registry.register(Arc::new(web_fetch));
    }
    match build_web_search_tool(&cfg, Arc::clone(&audit_sink)) {
        Ok(Some(web_search)) => registry.register(Arc::new(web_search)),
        Ok(None) => {}
        Err(erro) => {
            eprintln!("erro de configuração: {erro}");
            std::process::exit(2)
        }
    }
    register_mcp_tools(&mut registry, &cfg, &mut diagnosticos).await;
    // Sob `--tui` os avisos vão para o histórico de chat (ver `tui::run`);
    // em qualquer outro modo, para `stderr`, exatamente como antes.
    if !args.tui {
        diagnosticos.escoar_para_stderr();
    }

    // Construído aqui (não dentro de `register_context_tools`) porque
    // `/recall` (MT-137, REPL/TUI) precisa da mesma instância — mesmo
    // cache de índices entre a tool `session_search` e o comando manual.
    let session_search_session = Arc::new(SessionSearchSession::new(
        workspace_root.clone(),
        Arc::clone(&ollama_provider),
        &modelo_inicial,
        &modelo_inicial,
    ));
    register_context_tools(
        &mut registry,
        &cfg,
        &workspace_root,
        ollama_provider,
        &modelo_inicial,
        Arc::clone(&session_search_session),
    );

    register_subagent_tool(
        &mut registry,
        cfg.subagent_permissions.clone(),
        &workspace_root,
        Arc::clone(&confirmer),
        router_subagente,
        Some((
            Arc::clone(&guardrail_gate),
            Arc::clone(&guardrail_audit_sink),
        )),
    );

    // Modo servidor MCP (ADR-0042): o registry está completo aqui — mesmas
    // tools, mesmo `PermissionGate` (com `readAllow`/`subagentPermissions`) e
    // mesmo audit sink do modo normal. Servir daqui é o que garante que o
    // servidor não vira uma porta lateral em volta da política; montar um
    // registry próprio seria a forma óbvia de essa garantia se perder.
    //
    // Nada de `Session`/`Router` daqui pra frente: quem roda o laço de agente
    // é o cliente MCP (o `claude -p`), não o `agentry`.
    if args.mcp_server {
        if let Err(erro) = mcp_server::servir(Arc::new(registry)).await {
            // `stderr`: o `stdout` é o canal do protocolo.
            eprintln!("{erro}");
            std::process::exit(1);
        }
        return;
    }

    // Capturado antes do `registry` ser consumido por `RegistryToolExecutor`
    // logo abaixo — sem isto, `Session::with_tools` nunca era chamado e o
    // modelo nunca recebia nenhuma definição de tool (achado durante teste
    // manual de usabilidade: o modelo respondia com blocos de código Python
    // soltos em vez de chamar `fs_write`/etc., porque a requisição real
    // nunca incluía `tools`).
    let especificacoes_das_tools = registry.specs();
    let executor: Arc<dyn ToolExecutor> = Arc::new(RegistryToolExecutor::new(registry, confirmer));

    let budget = cfg
        .max_tokens
        .map(u64::from)
        .unwrap_or(DEFAULT_TOKEN_BUDGET);
    let mut session = Session::new(rota, executor, TokenBudget::new(budget))
        .with_tools(especificacoes_das_tools)
        .with_guardrails(guardrail_gate, guardrail_audit_sink);
    if cfg.agents_file_enabled {
        if let Some(instrucoes) = agentry_core::project_instructions::load_project_instructions(
            &workspace_root,
            &context_ignore,
        ) {
            session = session.with_project_instructions(instrucoes);
        }
    }
    // Memória de projeto explícita (MT-94, ADR-0032) — carregada sempre que
    // houver algum fato já gravado por `/remember`/`--remember` em
    // invocações anteriores; sem *opt-out* próprio (mesma disciplina de
    // `agentsFile`, ADR-0023 — quem não quiser nada aqui simplesmente nunca
    // usa o comando).
    let fatos_lembrados = agentry_core::memory::MemoryStore::new(&workspace_root)
        .load()
        .unwrap_or_default();
    let memoria = agentry_core::memory::render_memoria(&fatos_lembrados);
    if !memoria.is_empty() {
        session = session.with_memoria(memoria);
    }
    let lista_de_skills = agentry_core::skills::render_skills_list(&skills_descobertas);
    if !lista_de_skills.is_empty() {
        session = session.with_skills_list(lista_de_skills);
    }

    // `--resume` (MT-122, ADR-0036) -- antes do primeiro turno, em
    // qualquer um dos três modos (TUI/one-shot/REPL compartilham esta
    // mesma `session`). Falha (nenhuma sessão salva, id ambíguo/inexistente,
    // arquivo corrompido) é erro fatal e claro, nunca uma sessão vazia
    // silenciosa.
    if let Some(id_ou_nome) = &args.resume {
        match sessao::carregar_sessao(&workspace_root, id_ou_nome) {
            Ok(mensagens) => session = session.with_messages(mensagens),
            Err(erro) => {
                eprintln!("erro ao retomar sessão: {erro}");
                std::process::exit(2);
            }
        }
    }

    if args.tui {
        tui::run(
            session,
            router,
            task_class,
            overrides,
            rx_humano,
            auto_confirmacao,
            workspace_root.clone(),
            session_search_session,
            diagnosticos.mensagens().to_vec(),
        )
        .await
        .unwrap_or_else(|erro| {
            eprintln!("erro: {erro}");
            std::process::exit(1)
        });
    } else if let Some(tarefa) = args.tarefa {
        session.push_user_message(tarefa);
        let outcome = streaming::stream_to_writer(&mut session, io::stdout(), &router)
            .await
            .unwrap_or_else(|erro| {
                eprintln!("erro: {erro}");
                std::process::exit(1)
            });
        if let Some(aviso) = mensagem_de_teto_de_turnos(&outcome) {
            eprintln!("{aviso}");
        }
        eprintln!("[uso] {}", formatar_uso(session.usage_total()));
    } else {
        let stdin = io::stdin();
        repl::run_repl(
            stdin.lock(),
            io::stdout(),
            &mut session,
            &mut router,
            overrides,
            task_class,
            &repl::ReplConfig {
                workspace_root: &workspace_root,
                preset_base: &CallPreset::default(),
                candidatos_extra: &candidatos_extra,
                session_search_session: &session_search_session,
            },
        )
        .await
        .unwrap_or_else(|erro| {
            eprintln!("erro: {erro}");
            std::process::exit(1);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::config::privacy::EgressClass;
    use agentry_core::config::Permissions;
    use agentry_core::provider::mock::MockProvider;

    /// Diretório temporário de teste, removido automaticamente ao sair de
    /// escopo (mesma disciplina de `state_dir`/`config`/`tools::*`, MT-38/39).
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new() -> Self {
            let unico = format!(
                "agentry-cli-main-test-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("relógio do sistema não deve estar antes de 1970")
                    .as_nanos()
            );
            let path = std::env::temp_dir().join(unico);
            std::fs::create_dir_all(&path).expect("deve criar diretório temporário de teste");
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn run_init_local_cria_o_arquivo_ausente_com_o_exemplo_exato_da_adr_0018() {
        let dir = TempDir::new();

        let outcome = run_init_local(dir.path()).expect("deve criar o arquivo");
        let caminho = match outcome {
            InitOutcome::Created(caminho) => caminho,
            InitOutcome::AlreadyExists(_) => panic!("arquivo não deveria existir ainda"),
        };

        let conteudo = std::fs::read_to_string(&caminho).expect("arquivo deve existir");
        assert_eq!(conteudo, GENERIC_SETTINGS_EXAMPLE);
    }

    #[test]
    fn generic_settings_example_e_json_valido_e_todo_campo_null_fica_inerte() {
        let camada = Settings::from_json_str(GENERIC_SETTINGS_EXAMPLE)
            .expect("o exemplo gravado por --init deve ser JSON válido do schema real");
        let cfg = Config::resolve(vec![camada]);

        // Campos mostrados como `null` (achado do MT-49/50: providers.litellm
        // não tinha exemplo nenhum) não devem ativar nada sozinhos.
        assert_eq!(cfg.profile, None);
        assert_eq!(cfg.model, None);
        assert_eq!(cfg.max_tokens, None);
        assert!(
            cfg.litellm.is_none(),
            "baseUrl/model/egressClass como null não deve registrar um candidato litellm"
        );
        assert!(cfg.guardrails.input.is_empty());
        assert!(cfg.guardrails.output.is_empty());

        // As flags que já eram `true` no exemplo continuam.
        assert!(cfg.repo_map_enabled);
        assert!(cfg.semantic_rag_enabled);
        assert!(cfg.lsp_grounding_enabled);
        // Emenda de 2026-07-24 à ADR-0012: o exemplo passa a declarar `false`
        // explicitamente, acompanhando o novo default.
        assert!(!cfg.ollama_structured_output);

        // MT-57: `context.gitignore.enabled` explícito em `false` preserva o
        // default opt-in (ADR-0020 §3) — não liga nada sozinho.
        assert!(!cfg.respect_gitignore);

        // `taskClasses` do exemplo: `chat` traz **dois** candidatos, nesta
        // ordem — `claude-cli` (nuvem) e Ollama (local). É o que faz a mesma
        // configuração servir aos dois cenários sem edição: sob um perfil que
        // permite nuvem, o Claude atende; sob `empresa`/`externo-confidencial`
        // (local-only), o candidato de nuvem é filtrado pela classe de egresso
        // e o Ollama assume sozinho. Se o binário `claude` não estiver
        // instalado, o provider nem é registrado e a queda para o Ollama
        // também acontece (ver `build_claude_cli_provider`).
        //
        // As task-classes extras (`local`, `revisao-em-nuvem`,
        // `dados-sensiveis`) ficam no mapa resolvido sem "ativar" nada —
        // nenhum candidato é escolhido a menos que alguém peça
        // `--task-class`/`/task-class` ou a tool `subagent`.
        assert_eq!(cfg.task_classes.len(), 4);
        let chat = cfg
            .task_classes
            .get("chat")
            .expect("'chat' deve estar declarada no exemplo");
        assert_eq!(chat.candidates.len(), 2);
        assert_eq!(chat.candidates[0].provider, "claude-cli");
        assert_eq!(
            chat.candidates[1].provider, "ollama",
            "o local precisa ser o segundo candidato, para a queda funcionar"
        );
        assert_eq!(chat.candidates[1].model, DEFAULT_MODEL);
        use agentry_core::config::privacy::EgressClass;
        assert_eq!(chat.candidates[0].egress_class, EgressClass::CloudOk);
        assert_eq!(
            chat.candidates[1].egress_class,
            EgressClass::LocalOnly,
            "é a classe do segundo candidato que garante a queda sob perfil restritivo"
        );
        let local = cfg
            .task_classes
            .get("local")
            .expect("'local' é o alvo de delegação do subagente");
        assert_eq!(local.candidates[0].egress_class, EgressClass::LocalOnly);
        assert!(cfg.task_classes.contains_key("revisao-em-nuvem"));
        assert!(cfg.task_classes.contains_key("dados-sensiveis"));
        // 'compact'/'guardrail-compliance' não aparecem no exemplo — quem as
        // sintetiza é `register_declared_task_classes` (MT-56), não `Config`.
        assert!(!cfg.task_classes.contains_key("compact"));
        assert!(!cfg.task_classes.contains_key("guardrail-compliance"));

        // MT-151: 'mcpServers' sai **vazio** do `--init`. O MT-77 declarava
        // aqui um servidor 'exemplo' com `command: "echo"`, apostando que
        // uma conexão falha seria "tratada, não silenciosa" — e era, porque
        // naquele momento nada conectava. O MT-78 passou a conectar, e o
        // exemplo inerte virou uma mensagem de erro fixa
        // (`TokioChildProcess ... Broken pipe`) em **toda** execução de quem
        // rodou `--init`, achado do teste de uso de 2026-09-08. Um bloco de
        // configuração cujo valor de exemplo é executado não pode ter
        // exemplo inerte: o formato vai no comentário, não no mapa.
        assert!(
            cfg.mcp_servers.is_empty(),
            "o exemplo do --init não pode declarar servidor MCP nenhum: todo servidor \
             declarado é conectado no início da sessão (MT-78), então um exemplo aqui é um \
             erro garantido para quem acabou de instalar"
        );
    }

    #[test]
    fn run_init_local_nao_sobrescreve_arquivo_ja_existente() {
        let dir = TempDir::new();
        run_init_local(dir.path()).expect("primeira chamada deve criar");
        let caminho = state_dir::agentry_settings_path(dir.path());
        std::fs::write(&caminho, r#"{"customizado": true}"#)
            .expect("simula customização do usuário");

        let outcome = run_init_local(dir.path()).expect("segunda chamada não deve falhar");

        assert!(matches!(outcome, InitOutcome::AlreadyExists(_)));
        let conteudo = std::fs::read_to_string(&caminho).expect("arquivo deve continuar existindo");
        assert_eq!(
            conteudo, r#"{"customizado": true}"#,
            "customização do usuário não pode ser sobrescrita"
        );
    }

    #[test]
    fn escrever_resultado_init_sempre_inclui_o_comando_manual() {
        let dir = TempDir::new();

        let criado = run_init_local(dir.path()).expect("deve criar");
        let mut saida_criado = Vec::new();
        escrever_resultado_init(&criado, false, &mut saida_criado).expect("deve escrever");
        let texto_criado = String::from_utf8(saida_criado).unwrap();
        assert!(texto_criado.contains("criado:"));
        assert!(texto_criado.contains(MANUAL_SETUP_HINT));

        let ja_existente = run_init_local(dir.path()).expect("segunda chamada");
        let mut saida_existente = Vec::new();
        escrever_resultado_init(&ja_existente, false, &mut saida_existente).expect("deve escrever");
        let texto_existente = String::from_utf8(saida_existente).unwrap();
        assert!(texto_existente.contains("já existe"));
        assert!(texto_existente.contains(MANUAL_SETUP_HINT));
    }

    fn cfg_com_flags(
        repo_map: bool,
        semantic_rag: bool,
        lsp_grounding: bool,
        session_search: bool,
    ) -> Config {
        Config {
            profile: None,
            egress_class: EgressClass::LocalOnly,
            model: None,
            max_tokens: None,
            permissions: Permissions::default(),
            subagent_permissions: Permissions::default(),
            repo_map_enabled: repo_map,
            semantic_rag_enabled: semantic_rag,
            lsp_grounding_enabled: lsp_grounding,
            respect_gitignore: false,
            agents_file_enabled: true,
            session_search_enabled: session_search,
            ollama_structured_output: true,
            guardrails: agentry_core::guardrail::GuardrailGate::default(),
            litellm: None,
            anthropic: None,
            claude_cli: None,
            task_classes: std::collections::HashMap::new(),
            web_fetch_enabled: false,
            web_search: None,
            mcp_servers: std::collections::HashMap::new(),
        }
    }

    fn nomes_registrados(registry: &ToolRegistry) -> Vec<String> {
        registry.specs().into_iter().map(|s| s.name).collect()
    }

    #[test]
    fn flags_true_registra_as_4_tools_de_contexto() {
        let dir = TempDir::new();
        let cfg = cfg_com_flags(true, true, true, true);
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("mock"));
        let session_search_session = Arc::new(SessionSearchSession::new(
            dir.path(),
            Arc::clone(&provider),
            "embed-x",
            "rerank-x",
        ));

        register_context_tools(
            &mut registry,
            &cfg,
            dir.path(),
            provider,
            "modelo-teste",
            session_search_session,
        );

        let nomes = nomes_registrados(&registry);
        assert!(nomes.contains(&"repo_map".to_string()));
        assert!(nomes.contains(&"code_search".to_string()));
        assert!(nomes.contains(&"lsp_hover".to_string()));
        assert!(nomes.contains(&"lsp_definition".to_string()));
        assert!(nomes.contains(&"session_search".to_string()));
    }

    #[test]
    fn flags_false_nao_registra_nenhuma_das_4_tools_de_contexto() {
        let dir = TempDir::new();
        let cfg = cfg_com_flags(false, false, false, false);
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("mock"));
        let session_search_session = Arc::new(SessionSearchSession::new(
            dir.path(),
            Arc::clone(&provider),
            "embed-x",
            "rerank-x",
        ));

        register_context_tools(
            &mut registry,
            &cfg,
            dir.path(),
            provider,
            "modelo-teste",
            session_search_session,
        );

        let nomes = nomes_registrados(&registry);
        assert!(!nomes.contains(&"repo_map".to_string()));
        assert!(!nomes.contains(&"code_search".to_string()));
        assert!(!nomes.contains(&"lsp_hover".to_string()));
        assert!(!nomes.contains(&"lsp_definition".to_string()));
        assert!(!nomes.contains(&"session_search".to_string()));
    }

    // --- MT-91: register_subagent_tool ---

    fn router_subagente_de_teste(mock: Arc<MockProvider>) -> Arc<Router> {
        let mut router = Router::new(EgressClass::LocalOnly);
        router.register_provider(mock.clone());
        router.set_route(
            "chat",
            RouteEntry {
                candidates: vec![RouteTarget::new("mock", "modelo-x", EgressClass::LocalOnly)],
                preset: CallPreset::default(),
            },
        );
        Arc::new(router)
    }

    #[test]
    fn register_subagent_tool_expoe_a_tool_subagent_na_sessao_principal() {
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        registry.register(Arc::new(SkillTool::new(Vec::new())));
        let mock = Arc::new(MockProvider::new("mock"));
        let router_subagente = router_subagente_de_teste(mock);

        register_subagent_tool(
            &mut registry,
            Permissions::default(),
            std::path::Path::new("."),
            Arc::new(InteractiveConfirmer),
            router_subagente,
            None,
        );

        let nomes = nomes_registrados(&registry);
        assert!(nomes.contains(&"subagent".to_string()));
        assert!(
            nomes.contains(&"skill".to_string()),
            "as tools já registradas antes continuam presentes na sessão principal"
        );
    }

    #[tokio::test]
    async fn chamada_real_a_subagent_completa_de_ponta_a_ponta_via_registry_executor() {
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(agentry_core::provider::ChatResponse {
            message: agentry_core::model::Message::assistant("resposta do subagente"),
            usage: agentry_core::model::Usage::default(),
        }));
        let router_subagente = router_subagente_de_teste(mock);

        register_subagent_tool(
            &mut registry,
            Permissions::default(),
            std::path::Path::new("."),
            Arc::new(InteractiveConfirmer),
            router_subagente,
            None,
        );

        let executor: Arc<dyn ToolExecutor> = Arc::new(RegistryToolExecutor::new(
            registry,
            Arc::new(InteractiveConfirmer),
        ));
        let call = agentry_core::model::ToolCall {
            id: "1".into(),
            name: "subagent".into(),
            arguments: serde_json::json!({ "description": "resuma este arquivo" }),
        };

        let resultado = executor.execute(&call).await;

        assert!(!resultado.is_error);
        assert_eq!(resultado.content, "resposta do subagente");
    }

    #[test]
    fn executor_interno_do_subagente_nunca_lista_a_propria_tool_subagent() {
        // Reaproveita a MESMA lógica de register_subagent_tool para provar
        // que o registry_subagente construído internamente (a mesma
        // "forma" do que fica dentro de SubagentTool) nunca inclui
        // "subagent" — recursão impossível estruturalmente (ADR-0031).
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        registry.register(Arc::new(SkillTool::new(Vec::new())));
        let antes = nomes_registrados(&registry);
        assert!(!antes.contains(&"subagent".to_string()));

        let mut registry_subagente = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        for tool in registry.tools() {
            registry_subagente.register(Arc::clone(tool));
        }

        let nomes_internos = nomes_registrados(&registry_subagente);
        assert!(nomes_internos.contains(&"skill".to_string()));
        assert!(!nomes_internos.contains(&"subagent".to_string()));
    }

    #[test]
    fn ausencia_do_arquivo_preserva_o_comportamento_anterior_todas_true() {
        // Mesmo critério de aceite do MT-39: sem `.agentry/agentry.settings.json`,
        // `Config::resolve` cai nos defaults do ADR-0018 (todas `true`) — e,
        // por extensão, as 3 tools de contexto continuam registradas.
        let cfg = Config::resolve(vec![Settings::default()]);
        let dir = TempDir::new();
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let provider: Arc<dyn LlmProvider> = Arc::new(MockProvider::new("mock"));
        let session_search_session = Arc::new(SessionSearchSession::new(
            dir.path(),
            Arc::clone(&provider),
            "embed-x",
            "rerank-x",
        ));

        register_context_tools(
            &mut registry,
            &cfg,
            dir.path(),
            provider,
            "modelo-teste",
            session_search_session,
        );

        let nomes = nomes_registrados(&registry);
        assert!(nomes.contains(&"repo_map".to_string()));
        assert!(nomes.contains(&"code_search".to_string()));
        assert!(nomes.contains(&"lsp_hover".to_string()));
        assert!(nomes.contains(&"lsp_definition".to_string()));
    }

    // --- MT-46: Guardrail Gate consumido de ponta a ponta na CLI real ---

    use agentry_core::router::ResolvedRoute;
    use agentry_core::session::StopReason;

    /// Monta uma `Session` real como `main()` faria (registry vazio,
    /// `RegistryToolExecutor` real, `MockProvider` no lugar do Ollama) — o
    /// suficiente para provar a fiação de `with_guardrails`, sem repetir o
    /// resto do `main()` que não é específico deste ticket.
    fn sessao_de_teste(cfg: &Config, mock: Arc<MockProvider>) -> Session {
        let route = ResolvedRoute::new(mock, "modelo-teste", CallPreset::default());
        let registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let executor: Arc<dyn ToolExecutor> = Arc::new(RegistryToolExecutor::new(
            registry,
            Arc::new(InteractiveConfirmer),
        ));
        Session::new(route, executor, TokenBudget::new(10_000))
            .with_guardrails(Arc::new(cfg.guardrails.clone()), Arc::new(StderrAuditSink))
    }

    /// Regressão: achado num teste manual de usabilidade (build real,
    /// gateway LiteLLM real) — o modelo respondia com texto/código Python
    /// solto em vez de chamar `fs_write`/etc. Causa raiz: `main()` nunca
    /// chamava `Session::with_tools`, então a requisição real ao provider
    /// nunca incluía nenhuma tool, com nenhum provider, em nenhum modo. Este
    /// teste reproduz a sequência exata de `main()` (captura
    /// `registry.specs()` **antes** do `registry` ser consumido por
    /// `RegistryToolExecutor`, só então chama `with_tools`) e observa, via
    /// `MockProvider::chat_requests`, que a tool registrada chega de fato na
    /// requisição — não bastaria inspecionar `ToolRegistry`/`Session`
    /// isoladamente, o bug estava especificamente na fiação entre os dois.
    #[tokio::test]
    async fn sessao_real_leva_as_specs_das_tools_registradas_ao_provider() {
        let dir = TempDir::new();
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        registry.register(Arc::new(FsReadTool::new(dir.path().to_path_buf(), false)));
        let especificacoes_das_tools = registry.specs();
        let executor: Arc<dyn ToolExecutor> = Arc::new(RegistryToolExecutor::new(
            registry,
            Arc::new(InteractiveConfirmer),
        ));

        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(agentry_core::provider::ChatResponse {
            message: agentry_core::model::Message::assistant("ok"),
            usage: agentry_core::model::Usage::default(),
        }));
        let route = ResolvedRoute::new(mock.clone(), "modelo-teste", CallPreset::default());
        let mut session = Session::new(route, executor, TokenBudget::new(10_000))
            .with_tools(especificacoes_das_tools);
        session.push_user_message("oi");

        session
            .run(&router_vazio())
            .await
            .expect("turno simples não deve falhar");

        let requisicoes = mock.chat_requests();
        assert_eq!(requisicoes.len(), 1);
        assert!(
            requisicoes[0].tools.iter().any(|t| t.name == "fs_read"),
            "esperava a tool fs_read na requisição enviada ao provider; tools recebidas: {:?}",
            requisicoes[0]
                .tools
                .iter()
                .map(|t| &t.name)
                .collect::<Vec<_>>()
        );
    }

    fn router_vazio() -> agentry_core::router::Router {
        agentry_core::router::Router::new(EgressClass::LocalOnly)
    }

    fn escreve_settings(dir: &std::path::Path, conteudo: &str) {
        state_dir::ensure_state_dir(dir).expect("cria .agentry de teste");
        std::fs::write(state_dir::agentry_settings_path(dir), conteudo)
            .expect("grava agentry.settings.json de teste");
    }

    #[test]
    fn build_config_le_regras_de_guardrail_do_arquivo_real() {
        let dir = TempDir::new();
        escreve_settings(
            dir.path(),
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "guardrails": {
                "input": [{"id": "bloqueia-senha", "match": "senha:", "action": "block"}],
                "output": [{"id": "mascara-segredo", "match": "segredo-abc", "action": "redact"}]
              }
            }"#,
        );

        let cfg = build_config_com_camada_global(dir.path(), Settings::default())
            .expect("arquivo válido deve resolver");

        assert_eq!(cfg.guardrails.input.len(), 1);
        assert_eq!(cfg.guardrails.input[0].id, "bloqueia-senha");
        assert_eq!(cfg.guardrails.output.len(), 1);
        assert_eq!(cfg.guardrails.output[0].id, "mascara-segredo");
    }

    #[test]
    fn ausencia_do_arquivo_de_settings_preserva_guardrails_vazio() {
        let dir = TempDir::new();

        let cfg = build_config_com_camada_global(dir.path(), Settings::default())
            .expect("ausência do arquivo não é erro");

        assert!(cfg.guardrails.input.is_empty());
        assert!(cfg.guardrails.output.is_empty());
    }

    // --- MT-127: camada global (`~/.agentry/agentry.settings.json`, ADR-0038) ---

    #[test]
    fn camada_global_preenche_uma_preferencia_ausente_do_projeto() {
        let dir = TempDir::new();
        // Sem `.agentry/agentry.settings.json` no "projeto" -- só a camada
        // global declara um modelo padrão.
        let camada_global = Settings::from_json_str(
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "model": "modelo-pessoal-padrao"
            }"#,
        )
        .expect("JSON válido");

        let cfg = build_config_com_camada_global(dir.path(), camada_global)
            .expect("deve resolver com a camada global");

        assert_eq!(cfg.model.as_deref(), Some("modelo-pessoal-padrao"));
    }

    #[test]
    fn arquivo_do_projeto_sobrescreve_a_preferencia_global() {
        let dir = TempDir::new();
        escreve_settings(
            dir.path(),
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "model": "modelo-do-projeto"
            }"#,
        );
        let camada_global = Settings::from_json_str(
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "model": "modelo-pessoal-padrao"
            }"#,
        )
        .expect("JSON válido");

        let cfg = build_config_com_camada_global(dir.path(), camada_global)
            .expect("deve resolver com as duas camadas");

        assert_eq!(
            cfg.model.as_deref(),
            Some("modelo-do-projeto"),
            "o arquivo do projeto deve vencer a preferência pessoal global"
        );
    }

    #[test]
    fn camada_global_ausente_e_regressao_zero_frente_ao_comportamento_de_sempre() {
        let dir = TempDir::new();
        escreve_settings(
            dir.path(),
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "guardrails": {
                "input": [{"id": "bloqueia-senha", "match": "senha:", "action": "block"}],
                "output": []
              }
            }"#,
        );

        let cfg = build_config_com_camada_global(dir.path(), Settings::default())
            .expect("camada global vazia não deve mudar nada");

        assert_eq!(cfg.guardrails.input.len(), 1);
        assert_eq!(cfg.guardrails.input[0].id, "bloqueia-senha");
    }

    #[tokio::test]
    async fn agentry_settings_json_com_regra_de_entrada_block_bloqueia_de_ponta_a_ponta_via_sessao_real(
    ) {
        let dir = TempDir::new();
        escreve_settings(
            dir.path(),
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "guardrails": {
                "input": [{"id": "bloqueia-senha", "match": "senha:", "action": "block"}],
                "output": []
              }
            }"#,
        );
        let cfg = build_config_com_camada_global(dir.path(), Settings::default())
            .expect("arquivo válido deve resolver");
        let mock = Arc::new(MockProvider::new("mock"));
        // Nenhuma resposta enfileirada de propósito: se o provider fosse
        // chamado, o mock devolveria erro de fila vazia.
        let mut session = sessao_de_teste(&cfg, mock.clone());
        session.push_user_message("minha senha: 12345");

        let outcome = session
            .run(&router_vazio())
            .await
            .expect("bloqueio de entrada não deve ser erro");

        assert_eq!(outcome.reason, StopReason::Done);
        assert_eq!(mock.chat_requests().len(), 0, "o provider nunca é chamado");
        assert_eq!(outcome.guardrail_hits.len(), 1);
    }

    #[tokio::test]
    async fn agentry_settings_json_com_regra_de_saida_redact_mascara_a_resposta_de_ponta_a_ponta() {
        let dir = TempDir::new();
        escreve_settings(
            dir.path(),
            r#"{
              "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
              "schemaVersion": 1,
              "guardrails": {
                "input": [],
                "output": [{"id": "mascara-segredo", "match": "segredo-abc", "action": "redact"}]
              }
            }"#,
        );
        let cfg = build_config_com_camada_global(dir.path(), Settings::default())
            .expect("arquivo válido deve resolver");
        let mock = Arc::new(MockProvider::new("mock"));
        mock.enqueue_chat(Ok(agentry_core::provider::ChatResponse {
            message: agentry_core::model::Message::assistant("o valor é segredo-abc, guarde bem"),
            usage: agentry_core::model::Usage::default(),
        }));
        let mut session = sessao_de_teste(&cfg, mock.clone());
        session.push_user_message("qual o valor?");

        let outcome = session.run(&router_vazio()).await.expect("deve completar");

        assert_eq!(outcome.reason, StopReason::Done);
        let ultima = session
            .messages()
            .last()
            .expect("deve ter uma resposta")
            .text_content();
        assert!(!ultima.contains("segredo-abc"));
        assert!(ultima.contains(agentry_core::egress::redact::REDACTED_PLACEHOLDER));
        assert_eq!(outcome.guardrail_hits.len(), 1);
    }

    // --- MT-50: flag --provider ---

    #[test]
    fn flag_provider_chega_ao_runtime_override() {
        let args = Args::parse_from(["agentry", "--provider", "litellm", "tarefa"]);
        let overrides = overrides_from_args(&args).expect("flags válidas não devem falhar");
        assert_eq!(overrides.provider, Some("litellm".to_string()));
    }

    #[test]
    fn ausencia_da_flag_provider_preserva_none() {
        let args = Args::parse_from(["agentry", "tarefa"]);
        let overrides = overrides_from_args(&args).expect("flags válidas não devem falhar");
        assert_eq!(overrides.provider, None);
    }

    // --- MT-87: flag --undo, formatar_undo ---

    #[test]
    fn flag_undo_e_reconhecida_sozinha() {
        let args = Args::parse_from(["agentry", "--undo"]);
        assert!(args.undo);
        assert!(args.tarefa.is_none());
    }

    #[test]
    fn flag_undo_conflita_com_tarefa() {
        let resultado = Args::try_parse_from(["agentry", "--undo", "tarefa"]);
        assert!(
            resultado.is_err(),
            "--undo e uma tarefa one-shot juntos devem ser erro de parsing"
        );
    }

    #[test]
    fn formatar_undo_de_restaurado_menciona_o_caminho_e_a_acao() {
        let outcome = agentry_core::checkpoint::UndoOutcome {
            path: "a.txt".to_string(),
            acao: agentry_core::checkpoint::UndoAcao::Restaurado,
        };
        assert_eq!(
            formatar_undo(&outcome),
            "'a.txt' restaurado ao conteúdo anterior"
        );
    }

    #[test]
    fn formatar_undo_de_removido_menciona_o_caminho_e_a_acao() {
        let outcome = agentry_core::checkpoint::UndoOutcome {
            path: "novo.txt".to_string(),
            acao: agentry_core::checkpoint::UndoAcao::Removido,
        };
        assert_eq!(
            formatar_undo(&outcome),
            "'novo.txt' removido (não existia antes da mudança desfeita)"
        );
    }

    // --- MT-102/ADR-0033: mensagem de aviso ao parar no teto de turnos ---

    fn outcome_de_teste(
        reason: agentry_core::session::StopReason,
        turns: u32,
    ) -> agentry_core::session::SessionOutcome {
        agentry_core::session::SessionOutcome {
            reason,
            usage: agentry_core::model::Usage::default(),
            turns,
            reviews: Vec::new(),
            guardrail_hits: Vec::new(),
        }
    }

    #[test]
    fn mensagem_de_teto_de_turnos_e_none_quando_o_motivo_e_outro() {
        assert_eq!(
            mensagem_de_teto_de_turnos(&outcome_de_teste(
                agentry_core::session::StopReason::Done,
                3
            )),
            None
        );
        assert_eq!(
            mensagem_de_teto_de_turnos(&outcome_de_teste(
                agentry_core::session::StopReason::BudgetExceeded,
                3
            )),
            None
        );
    }

    #[test]
    fn mensagem_de_teto_de_turnos_menciona_a_contagem_quando_e_o_motivo() {
        let mensagem = mensagem_de_teto_de_turnos(&outcome_de_teste(
            agentry_core::session::StopReason::MaxTurnsExceeded,
            25,
        ))
        .expect("deve haver mensagem para MaxTurnsExceeded");

        assert!(mensagem.contains("25"), "mensagem: {mensagem:?}");
        assert!(
            mensagem.contains("turnos consecutivos"),
            "mensagem: {mensagem:?}"
        );
    }

    // --- MT-94: flag --remember, Session::with_memoria carregada no arranque ---

    #[test]
    fn flag_remember_e_reconhecida_com_o_fato() {
        let args = Args::parse_from(["agentry", "--remember", "um fato qualquer"]);
        assert_eq!(args.remember.as_deref(), Some("um fato qualquer"));
        assert!(args.tarefa.is_none());
    }

    #[test]
    fn flag_remember_conflita_com_tarefa() {
        let resultado = Args::try_parse_from(["agentry", "--remember", "fato", "tarefa"]);
        assert!(
            resultado.is_err(),
            "--remember e uma tarefa one-shot juntos devem ser erro de parsing"
        );
    }

    #[test]
    fn flag_remember_ausente_preserva_none() {
        let args = Args::parse_from(["agentry", "tarefa"]);
        assert!(args.remember.is_none());
    }

    #[test]
    fn memory_store_grava_e_render_memoria_reflete_o_fato_gravado() {
        // Prova a mesma sequência que main() executa ao montar a sessão
        // real: MemoryStore::new(workspace_root).load() seguido de
        // render_memoria — sem precisar rodar main() inteiro.
        let dir = TempDir::new();
        let store = agentry_core::memory::MemoryStore::new(dir.path());
        store
            .remember("fato gravado numa invocação anterior")
            .expect("remember deve funcionar");

        let fatos = store.load().expect("load deve funcionar");
        let memoria = agentry_core::memory::render_memoria(&fatos);

        assert!(memoria.contains("fato gravado numa invocação anterior"));
    }

    // --- MT-122: flag --resume, ADR-0036 ---

    #[test]
    fn flag_resume_sozinha_usa_valor_padrao_vazio_mais_recente() {
        let args = Args::parse_from(["agentry", "--resume"]);
        assert_eq!(args.resume.as_deref(), Some(""));
    }

    #[test]
    fn flag_resume_com_igual_e_valor_carrega_o_id_ou_nome() {
        let args = Args::parse_from(["agentry", "--resume=antiga"]);
        assert_eq!(args.resume.as_deref(), Some("antiga"));
    }

    #[test]
    fn flag_resume_ausente_preserva_none() {
        let args = Args::parse_from(["agentry", "tarefa"]);
        assert!(args.resume.is_none());
    }

    #[test]
    fn flag_resume_sem_igual_nao_consome_a_tarefa_one_shot_seguinte() {
        // MT-122: sem `require_equals`, `clap` consumia avidamente "tarefa"
        // como valor de `--resume` (achado real de smoke-test manual) —
        // `--resume` sozinho (sem `=`) deve continuar significando "a mais
        // recente", deixando "tarefa" livre para ser a tarefa *one-shot*.
        let args = Args::parse_from(["agentry", "--resume", "tarefa"]);
        assert_eq!(args.resume.as_deref(), Some(""));
        assert_eq!(args.tarefa.as_deref(), Some("tarefa"));
    }

    // --- MT-49: consumo real do provider LiteLLM na CLI ---

    fn cfg_com_litellm(litellm_json: &str) -> Config {
        let json = format!(r#"{{ "providers": {{ "litellm": {litellm_json} }} }}"#);
        let camada = agentry_core::config::Settings::from_json_str(&json)
            .expect("JSON de teste deve ser válido");
        Config::resolve(vec![camada])
    }

    #[test]
    fn ausencia_de_providers_litellm_preserva_comportamento_atual_none() {
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(build_litellm_provider(&cfg, None, Arc::new(NoopAuditSink))
            .expect("ausência de litellm não é erro")
            .is_none());
    }

    #[test]
    fn litellm_configurado_monta_provider_e_candidato_corretos() {
        let cfg = cfg_com_litellm(
            r#"{ "baseUrl": "https://litellm.minhaempresa.com", "model": "empresa/gpt-30b", "egressClass": "cloud-opt-out" }"#,
        );

        let (provider, candidato) = build_litellm_provider(&cfg, None, Arc::new(NoopAuditSink))
            .expect("configuração válida não é erro")
            .expect("providers.litellm completo deve montar Some");

        assert_eq!(provider.name(), LITELLM_PROVIDER_NAME);
        assert_eq!(candidato.provider, LITELLM_PROVIDER_NAME);
        assert_eq!(candidato.model, "empresa/gpt-30b");
        assert_eq!(
            candidato.egress_class,
            agentry_core::config::privacy::EgressClass::CloudOptOut
        );
    }

    #[test]
    fn litellm_com_base_url_invalida_e_erro_tratado() {
        let cfg = cfg_com_litellm(r#"{ "baseUrl": "não-é-uma-url", "model": "m" }"#);

        match build_litellm_provider(&cfg, None, Arc::new(NoopAuditSink)) {
            Err(erro) => assert!(erro.contains("baseUrl")),
            Ok(_) => panic!("base_url sem host deve ser erro"),
        }
    }

    // ---- build_anthropic_provider (ADR-0040) ----

    fn cfg_com_anthropic(anthropic_json: &str) -> Config {
        let json = format!(r#"{{ "providers": {{ "anthropic": {anthropic_json} }} }}"#);
        let camada = agentry_core::config::Settings::from_json_str(&json)
            .expect("JSON de teste deve ser válido");
        Config::resolve(vec![camada])
    }

    #[test]
    fn ausencia_de_providers_anthropic_nao_e_erro_e_nao_registra() {
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(
            build_anthropic_provider(&cfg, Some("chave"), Arc::new(NoopAuditSink))
                .expect("ausência de anthropic não é erro")
                .is_none()
        );
    }

    #[test]
    fn anthropic_configurado_monta_provider_e_candidato_corretos() {
        let cfg = cfg_com_anthropic(r#"{ "model": "claude-opus-5" }"#);

        let (provider, candidato) =
            build_anthropic_provider(&cfg, Some("chave-de-teste"), Arc::new(NoopAuditSink))
                .expect("configuração válida não é erro")
                .expect("model declarado deve montar Some");

        assert_eq!(provider.name(), ANTHROPIC_PROVIDER_NAME);
        assert_eq!(candidato.provider, ANTHROPIC_PROVIDER_NAME);
        assert_eq!(candidato.model, "claude-opus-5");
        assert_eq!(
            candidato.egress_class,
            agentry_core::config::privacy::EgressClass::CloudOk
        );
    }

    /// Diferente do LiteLLM (gateway interno pode não ter autenticação), a
    /// Messages API não tem modo anônimo — configurar sem credencial é erro
    /// de configuração reportado no arranque, não um 401 no primeiro uso.
    #[test]
    fn anthropic_sem_credencial_e_erro_de_configuracao_tratado() {
        let cfg = cfg_com_anthropic(r#"{ "model": "claude-opus-5" }"#);

        match build_anthropic_provider(&cfg, None, Arc::new(NoopAuditSink)) {
            Err(erro) => {
                assert!(
                    erro.contains("credencial") && erro.contains("--set-credential"),
                    "o erro deve dizer como resolver, não só que faltou: {erro}"
                );
            }
            Ok(_) => panic!("sem credencial deve ser erro, não Some silencioso"),
        }
    }

    #[test]
    fn anthropic_com_base_url_invalida_e_erro_tratado() {
        let cfg = cfg_com_anthropic(r#"{ "model": "m", "baseUrl": "não-é-uma-url" }"#);

        match build_anthropic_provider(&cfg, Some("chave"), Arc::new(NoopAuditSink)) {
            Err(erro) => assert!(erro.contains("baseUrl")),
            Ok(_) => panic!("base_url sem host deve ser erro"),
        }
    }

    /// Regressão da fiação: antes da ADR-0040 o `AnthropicProvider` existia
    /// completo em `crates/core` mas **nunca era registrado no `Router`** —
    /// código morto que nenhum teste pegava, porque cada teste montava seu
    /// próprio router.
    #[tokio::test]
    async fn router_registra_anthropic_junto_do_ollama_e_resolve_via_override() {
        let cfg = cfg_com_anthropic(r#"{ "model": "claude-opus-5" }"#);
        let registro = build_anthropic_provider(&cfg, Some("chave"), Arc::new(NoopAuditSink))
            .expect("configuração válida")
            .expect("deve montar Some");

        let router = montar_router(
            agentry_core::config::privacy::EgressClass::CloudOk,
            Arc::new(MockProvider::new("ollama")),
            vec![registro],
            "modelo-ollama",
            &cfg,
        );

        let override_anthropic = RuntimeOverride {
            provider: Some(ANTHROPIC_PROVIDER_NAME.to_string()),
            ..RuntimeOverride::default()
        };
        let rota = router
            .resolve_with_override("chat", &override_anthropic)
            .expect("anthropic deve estar declarada como candidato de `chat`");

        assert_eq!(rota.provider.name(), ANTHROPIC_PROVIDER_NAME);
        assert_eq!(rota.model, "claude-opus-5");
    }

    // ---- ADR-0041: escopo de caminho + permissões próprias do subagente ----

    /// A invariante que a ADR-0041 existe para garantir, no ponto exato em que
    /// `main()` monta os dois registries: com a mesma `Config`, o gate do
    /// agente **principal** nega o arquivo confidencial e libera a
    /// documentação, enquanto o gate do **subagente** lê os dois.
    ///
    /// Testado sobre os gates (e não ponta a ponta com um modelo real) de
    /// propósito: um modelo local que simplesmente *não chama* a tool produz o
    /// mesmo "nada aconteceu" de um bloqueio, e não distinguiria os dois casos.
    #[test]
    fn subagente_le_o_que_o_agente_principal_nao_pode() {
        use agentry_core::model::ToolCall;
        use agentry_core::tools::permission::{Permission, PermissionGate};

        let camada = agentry_core::config::Settings::from_json_str(
            r#"{
                "permissions": {
                    "readAllow": ["README.md", "docs/**"],
                    "deny": ["shell_exec", "glob", "fs_search"]
                },
                "subagentPermissions": { "deny": [] }
            }"#,
        )
        .expect("JSON válido");
        let cfg = Config::resolve(vec![camada]);

        let raiz = std::path::Path::new("/workspace");
        let gate_principal = PermissionGate::new(cfg.permissions.clone()).with_workspace_root(raiz);
        let gate_subagente =
            PermissionGate::new(cfg.subagent_permissions.clone()).with_workspace_root(raiz);

        let ler = |path: &str| ToolCall {
            id: "c1".into(),
            name: "fs_read".into(),
            arguments: serde_json::json!({ "path": path }),
        };

        assert_eq!(
            gate_principal.decide(&ler("README.md")),
            Permission::Allow,
            "o principal precisa ler a documentação para ser útil"
        );
        assert_eq!(
            gate_principal.decide(&ler("clientes.csv")),
            Permission::Deny,
            "o principal (nuvem) não pode alcançar o confidencial"
        );
        assert_eq!(
            gate_subagente.decide(&ler("clientes.csv")),
            Permission::Allow,
            "sem isto a ADR-0041 não serve para nada: negar ao principal cegaria o subagente"
        );
        assert_eq!(
            gate_principal.decide(&ToolCall {
                id: "c2".into(),
                name: "shell_exec".into(),
                arguments: serde_json::json!({ "command": "cat clientes.csv" }),
            }),
            Permission::Deny,
            "shell_exec não é coberta por readAllow — o deny explícito é obrigatório"
        );
    }

    // ---- DiagnosticosDeInicializacao (MT-152) ----

    /// A falha de conexão a um servidor MCP tem de virar **dado**, não uma
    /// escrita imediata em `stderr`: é o que permite a `main` mostrá-la
    /// dentro da TUI, onde `stderr` está coberto pela tela alternativa.
    ///
    /// O par de asserções é o contrato inteiro: o aviso existe *e* nenhuma
    /// tool foi registrada. Só a primeira passaria também numa versão que
    /// avisasse e registrasse uma tool quebrada; só a segunda passaria numa
    /// versão que falhasse em silêncio — que é exatamente o defeito.
    #[tokio::test]
    async fn servidor_mcp_que_nao_conecta_vira_diagnostico_em_vez_de_escrita_em_stderr() {
        let camada = agentry_core::config::Settings::from_json_str(
            r#"{ "mcpServers": { "inexistente": {
                "command": "agentry-comando-que-nao-existe",
                "args": [],
                "egressClass": "local-only"
            } } }"#,
        )
        .expect("JSON de teste deve ser válido");
        let cfg = Config::resolve(vec![camada]);
        let mut registry = ToolRegistry::new(PermissionGate::new(Permissions::default()));
        let mut diagnosticos = DiagnosticosDeInicializacao::default();

        register_mcp_tools(&mut registry, &cfg, &mut diagnosticos).await;

        assert_eq!(
            diagnosticos.mensagens().len(),
            1,
            "um servidor que não conecta produz exatamente um aviso; veio: {:?}",
            diagnosticos.mensagens()
        );
        assert!(
            diagnosticos.mensagens()[0].contains("inexistente"),
            "o aviso precisa nomear o servidor — quem lê tem de saber qual dos declarados \
             falhou; veio: {:?}",
            diagnosticos.mensagens()
        );
        assert!(
            registry.specs().is_empty(),
            "nenhuma tool pode ser registrada por um servidor que não conectou"
        );
    }

    // ---- build_claude_cli_provider (ADR-0040, assinatura Pro/Max) ----

    fn cfg_com_claude_cli(json_interno: &str) -> Config {
        let json = format!(r#"{{ "providers": {{ "claudeCli": {json_interno} }} }}"#);
        let camada = agentry_core::config::Settings::from_json_str(&json)
            .expect("JSON de teste deve ser válido");
        Config::resolve(vec![camada])
    }

    #[test]
    fn ausencia_de_providers_claude_cli_nao_registra() {
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(build_claude_cli_provider(
            &cfg,
            Arc::new(NoopAuditSink),
            false,
            &mut DiagnosticosDeInicializacao::default(),
        )
        .is_none());
    }

    /// Diferente de `anthropic`, este provider **não** pede credencial: a
    /// autenticação é do binário `claude`, que o agentry nunca lê.
    #[test]
    fn claude_cli_configurado_monta_sem_exigir_credencial() {
        let cfg = cfg_com_claude_cli(r#"{ "model": "claude-opus-5" }"#);

        let (provider, candidato) = build_claude_cli_provider(
            &cfg,
            Arc::new(NoopAuditSink),
            false,
            &mut DiagnosticosDeInicializacao::default(),
        )
        .expect("model declarado deve montar Some");

        assert_eq!(provider.name(), CLAUDE_CLI_PROVIDER_NAME);
        assert_eq!(candidato.provider, CLAUDE_CLI_PROVIDER_NAME);
        assert_eq!(candidato.model, "claude-opus-5");
    }

    /// Regressão da ADR-0042: quando o próprio processo é o servidor MCP, o
    /// provider não pode montar a ponte. Montá-la ali criaria um arquivo
    /// temporário que nunca seria removido — quem encerra o servidor é o
    /// cliente, sem rodar destrutores (achado real: sobrava um
    /// `/tmp/agentry-mcp-<pid>.json` por execução) — e um servidor MCP não tem
    /// motivo nenhum para lançar o `claude`.
    #[test]
    fn modo_servidor_mcp_nao_monta_a_ponte() {
        fn configs_mcp_em_tmp() -> usize {
            std::fs::read_dir(std::env::temp_dir())
                .expect("ler o diretório temporário")
                .filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with("agentry-mcp-"))
                .count()
        }

        let cfg = cfg_com_claude_cli(r#"{ "model": "haiku", "mcpTools": true }"#);
        let antes = configs_mcp_em_tmp();

        let registro = build_claude_cli_provider(
            &cfg,
            Arc::new(NoopAuditSink),
            false,
            &mut DiagnosticosDeInicializacao::default(),
        )
        .expect("o provider continua sendo montado, só sem a ponte");
        assert_eq!(registro.0.name(), CLAUDE_CLI_PROVIDER_NAME);

        assert_eq!(
            antes,
            configs_mcp_em_tmp(),
            "nenhum arquivo de configuração MCP deve ser criado no modo servidor"
        );
    }

    /// `anthropic` e `claude-cli` são nomes distintos no `Router` (diretriz de
    /// conformidade da ADR-0040) — nunca fundidos sob um nome só.
    #[tokio::test]
    async fn anthropic_e_claude_cli_coexistem_como_candidatos_distintos() {
        let camada = agentry_core::config::Settings::from_json_str(
            r#"{ "providers": {
                "anthropic": { "model": "claude-opus-5" },
                "claudeCli": { "model": "claude-sonnet-5" }
            } }"#,
        )
        .expect("JSON válido");
        let cfg = Config::resolve(vec![camada]);

        let anthropic = build_anthropic_provider(&cfg, Some("chave"), Arc::new(NoopAuditSink))
            .expect("configuração válida")
            .expect("deve montar Some");
        let claude_cli = build_claude_cli_provider(
            &cfg,
            Arc::new(NoopAuditSink),
            false,
            &mut DiagnosticosDeInicializacao::default(),
        )
        .expect("deve montar Some");

        let router = montar_router(
            agentry_core::config::privacy::EgressClass::CloudOk,
            Arc::new(MockProvider::new("ollama")),
            vec![anthropic, claude_cli],
            "modelo-ollama",
            &cfg,
        );

        for (nome, modelo) in [
            (ANTHROPIC_PROVIDER_NAME, "claude-opus-5"),
            (CLAUDE_CLI_PROVIDER_NAME, "claude-sonnet-5"),
        ] {
            let over = RuntimeOverride {
                provider: Some(nome.to_string()),
                ..RuntimeOverride::default()
            };
            let rota = router
                .resolve_with_override("chat", &over)
                .unwrap_or_else(|e| panic!("'{nome}' deveria resolver, mas: {e}"));
            assert_eq!(rota.provider.name(), nome);
            assert_eq!(
                rota.model, modelo,
                "cada provider mantém seu próprio modelo"
            );
        }
    }

    /// O Ollama local continua sendo o candidato preferencial mesmo com a
    /// Anthropic configurada — configurar nuvem nunca muda o default para
    /// sair da máquina sozinho.
    #[tokio::test]
    async fn ollama_continua_preferencial_com_anthropic_configurada() {
        let cfg = cfg_com_anthropic(r#"{ "model": "claude-opus-5" }"#);
        let registro = build_anthropic_provider(&cfg, Some("chave"), Arc::new(NoopAuditSink))
            .expect("configuração válida")
            .expect("deve montar Some");

        let router = montar_router(
            agentry_core::config::privacy::EgressClass::CloudOk,
            Arc::new(MockProvider::new("ollama")),
            vec![registro],
            "modelo-ollama",
            &cfg,
        );

        let rota = router
            .resolve_with_override("chat", &RuntimeOverride::default())
            .expect("chat deve resolver");

        assert_eq!(
            rota.provider.name(),
            "ollama",
            "sem override explícito, o local vence"
        );
    }

    #[tokio::test]
    async fn router_com_ollama_e_litellm_resolve_o_candidato_pedido_via_runtime_override() {
        let cfg = cfg_com_litellm(
            r#"{ "baseUrl": "http://litellm.interno:4000", "model": "modelo-30b", "egressClass": "local-only" }"#,
        );
        let mock_ollama = Arc::new(MockProvider::new("ollama"));
        let mock_litellm = Arc::new(MockProvider::new(LITELLM_PROVIDER_NAME));

        let mut router = agentry_core::router::Router::new(cfg.egress_class);
        router.register_provider(mock_ollama);
        router.register_provider(mock_litellm);

        let litellm = cfg.litellm.as_ref().expect("cfg.litellm deve ser Some");
        let candidato = agentry_core::router::RouteTarget::new(
            LITELLM_PROVIDER_NAME,
            litellm.model.clone(),
            litellm.egress_class,
        );
        repl::set_chat_route(
            &mut router,
            "modelo-ollama",
            &CallPreset::default(),
            std::slice::from_ref(&candidato),
        );

        // Sem override de provider: o candidato preferencial (Ollama, posição
        // 0) vence — comportamento default para quem não pediu LiteLLM.
        let rota_default = router
            .resolve_with_override(repl::TASK_CLASS, &RuntimeOverride::default())
            .expect("deve resolver");
        assert_eq!(rota_default.provider.name(), "ollama");

        // Pedindo explicitamente o provider "litellm" (mesmo mecanismo que a
        // futura flag --provider vai expor, MT-50): resolve o segundo
        // candidato, não o primeiro.
        let rota_litellm = router
            .resolve_with_override(
                repl::TASK_CLASS,
                &RuntimeOverride {
                    provider: Some(LITELLM_PROVIDER_NAME.to_string()),
                    ..RuntimeOverride::default()
                },
            )
            .expect("deve resolver o candidato litellm");
        assert_eq!(rota_litellm.provider.name(), LITELLM_PROVIDER_NAME);
        assert_eq!(rota_litellm.model, "modelo-30b");
    }

    // --- MT-56: CLI consome task-classes reais (ADR-0021) ---

    fn cfg_com_task_classes(task_classes_json: &str) -> Config {
        let json = format!(r#"{{ "taskClasses": {task_classes_json} }}"#);
        let camada = agentry_core::config::Settings::from_json_str(&json)
            .expect("JSON de teste deve ser válido");
        Config::resolve(vec![camada])
    }

    #[test]
    fn ausencia_de_arquivo_sintetiza_compact_e_guardrail_compliance_com_ollama_local_only() {
        let cfg = cfg_com_flags(true, true, true, true); // task_classes vazio
        let mut router = agentry_core::router::Router::new(cfg.egress_class);
        router.register_provider(Arc::new(MockProvider::new("ollama")));
        repl::set_chat_route(&mut router, "modelo-x", &CallPreset::default(), &[]);

        register_declared_task_classes(&mut router, &cfg, "modelo-x", &[]);

        for nome in ["chat", "compact", "guardrail-compliance"] {
            let rota = router
                .resolve_with_override(nome, &RuntimeOverride::default())
                .unwrap_or_else(|erro| panic!("'{nome}' deveria resolver, mas: {erro}"));
            assert_eq!(rota.provider.name(), "ollama");
            assert_eq!(rota.model, "modelo-x", "task-class '{nome}'");
        }
    }

    #[test]
    fn task_class_customizada_declarada_no_arquivo_resolve_via_seus_proprios_candidatos() {
        let cfg = cfg_com_task_classes(
            r#"{ "revisao": {
                "candidates": [
                    { "provider": "litellm", "model": "modelo-revisao-30b", "egressClass": "local-only" }
                ],
                "preset": { "temperature": 0.1 }
            } }"#,
        );
        let mut router = agentry_core::router::Router::new(cfg.egress_class);
        router.register_provider(Arc::new(MockProvider::new("ollama")));
        router.register_provider(Arc::new(MockProvider::new(LITELLM_PROVIDER_NAME)));
        repl::set_chat_route(&mut router, "modelo-x", &CallPreset::default(), &[]);

        register_declared_task_classes(&mut router, &cfg, "modelo-x", &[]);

        let rota = router
            .resolve_with_override("revisao", &RuntimeOverride::default())
            .expect("'revisao' foi declarada — deve resolver");
        assert_eq!(rota.provider.name(), LITELLM_PROVIDER_NAME);
        assert_eq!(rota.model, "modelo-revisao-30b");
        assert_eq!(rota.preset.temperature, Some(0.1));

        // Chat continua com o default sintetizado — declarar 'revisao' não
        // apaga a task-class 'chat'.
        let rota_chat = router
            .resolve_with_override(repl::TASK_CLASS, &RuntimeOverride::default())
            .expect("'chat' deve continuar resolvendo");
        assert_eq!(rota_chat.provider.name(), "ollama");
    }

    #[test]
    fn task_class_declarada_com_mesmo_nome_de_default_sobrescreve_o_sintetizado() {
        let cfg = cfg_com_task_classes(
            r#"{ "compact": {
                "candidates": [
                    { "provider": "litellm", "model": "modelo-compact-proprio", "egressClass": "local-only" }
                ]
            } }"#,
        );
        let mut router = agentry_core::router::Router::new(cfg.egress_class);
        router.register_provider(Arc::new(MockProvider::new("ollama")));
        router.register_provider(Arc::new(MockProvider::new(LITELLM_PROVIDER_NAME)));
        repl::set_chat_route(&mut router, "modelo-x", &CallPreset::default(), &[]);

        register_declared_task_classes(&mut router, &cfg, "modelo-x", &[]);

        let rota = router
            .resolve_with_override("compact", &RuntimeOverride::default())
            .expect("'compact' declarada deve resolver");
        assert_eq!(
            rota.provider.name(),
            LITELLM_PROVIDER_NAME,
            "task-class declarada pelo usuário deve vencer o default sintetizado"
        );
        assert_eq!(rota.model, "modelo-compact-proprio");
    }

    #[test]
    fn task_class_com_provider_nao_registrado_e_erro_tratado_sem_panic() {
        // Nome deliberadamente fictício: `anthropic` deixou de servir como
        // exemplo de "provider inexistente" quando a ADR-0040 passou a
        // registrá-lo de verdade.
        let cfg = cfg_com_task_classes(
            r#"{ "revisao": {
                "candidates": [
                    { "provider": "provider-inexistente", "model": "modelo-que-nao-existe-aqui", "egressClass": "local-only" }
                ]
            } }"#,
        );
        let mut router = agentry_core::router::Router::new(cfg.egress_class);
        router.register_provider(Arc::new(MockProvider::new("ollama")));
        repl::set_chat_route(&mut router, "modelo-x", &CallPreset::default(), &[]);

        register_declared_task_classes(&mut router, &cfg, "modelo-x", &[]);

        let erro = router
            .resolve_with_override("revisao", &RuntimeOverride::default())
            .expect_err("provider 'provider-inexistente' não está registrado no router");
        assert!(matches!(
            erro,
            agentry_core::router::RouterError::NoAvailableRoute { .. }
        ));
    }

    #[tokio::test]
    async fn session_compact_resolve_apos_register_declared_task_classes_sem_config_real() {
        // Critério de aceite do MT-56: "/compact num REPL com config real não
        // falha mais por falta de rota" — antes deste ticket, `compact` nunca
        // era registrada no router da CLI (só `chat`), então
        // `Session::compact` sempre devolvia `RouterError::UnknownTaskClass`
        // fora dos testes de `repl.rs` (que registravam a rota manualmente).
        let cfg = cfg_com_flags(true, true, true, true); // task_classes ausente do arquivo
        let mock = Arc::new(MockProvider::new("ollama"));
        mock.enqueue_chat(Ok(agentry_core::provider::ChatResponse {
            message: agentry_core::model::Message::assistant("resumo da conversa"),
            usage: agentry_core::model::Usage::default(),
        }));

        let mut router = agentry_core::router::Router::new(cfg.egress_class);
        router.register_provider(mock.clone());
        repl::set_chat_route(&mut router, "modelo-x", &CallPreset::default(), &[]);
        register_declared_task_classes(&mut router, &cfg, "modelo-x", &[]);

        let mut session = sessao_de_teste(&cfg, mock);
        session.push_user_message("mensagem original");

        session
            .compact(&router)
            .await
            .expect("MT-56: 'compact' deve ter rota registrada mesmo sem taskClasses no arquivo");
    }

    // --- MT-65: tool web_fetch só sob opt-in + CloudOk (ADR-0025) ---

    fn cfg_com_web_fetch(enabled: bool, profile: &str) -> Config {
        let json = format!(
            r#"{{ "profile": "{profile}", "tools": {{ "webFetch": {{ "enabled": {enabled} }} }} }}"#
        );
        let camada = agentry_core::config::Settings::from_json_str(&json)
            .expect("JSON de teste deve ser válido");
        Config::resolve(vec![camada])
    }

    #[test]
    fn web_fetch_habilitada_e_perfil_cloud_ok_registra_a_tool() {
        let cfg = cfg_com_web_fetch(true, "pessoal");
        assert_eq!(cfg.egress_class, EgressClass::CloudOk);

        assert!(build_web_fetch_tool(&cfg, Arc::new(NoopAuditSink)).is_some());
    }

    #[test]
    fn web_fetch_habilitada_mas_perfil_nao_cloud_ok_nao_registra_a_tool() {
        let cfg = cfg_com_web_fetch(true, "empresa");
        assert_eq!(cfg.egress_class, EgressClass::LocalOnly);

        assert!(
            build_web_fetch_tool(&cfg, Arc::new(NoopAuditSink)).is_none(),
            "tools.webFetch.enabled=true sozinho não deve bastar sob perfil local-only"
        );
    }

    #[test]
    fn web_fetch_desabilitada_mesmo_sob_cloud_ok_nao_registra_a_tool() {
        let cfg = cfg_com_web_fetch(false, "pessoal");
        assert_eq!(cfg.egress_class, EgressClass::CloudOk);

        assert!(
            build_web_fetch_tool(&cfg, Arc::new(NoopAuditSink)).is_none(),
            "egress_class=CloudOk sozinho não deve bastar sem o opt-in explícito"
        );
    }

    #[test]
    fn ausencia_de_tools_web_fetch_preserva_comportamento_atual_desabilitada() {
        let cfg = Config::resolve(vec![Settings::default()]);
        assert!(build_web_fetch_tool(&cfg, Arc::new(NoopAuditSink)).is_none());
    }

    // --- MT-66: tool web_search só quando searxngUrl declarada (ADR-0025) ---

    #[test]
    fn ausencia_de_searxng_url_preserva_comportamento_atual_nao_registrada() {
        let cfg = Config::resolve(vec![Settings::default()]);

        assert!(build_web_search_tool(&cfg, Arc::new(NoopAuditSink))
            .expect("ausência não deve ser erro")
            .is_none());
    }

    #[test]
    fn searxng_url_declarada_registra_a_tool() {
        let camada = agentry_core::config::Settings::from_json_str(
            r#"{ "tools": { "webSearch": { "searxngUrl": "https://searx.exemplo.com" } } }"#,
        )
        .expect("JSON de teste deve ser válido");
        let cfg = Config::resolve(vec![camada]);

        assert!(build_web_search_tool(&cfg, Arc::new(NoopAuditSink))
            .expect("URL válida não deve ser erro")
            .is_some());
    }

    #[test]
    fn searxng_url_invalida_e_erro_tratado() {
        let camada = agentry_core::config::Settings::from_json_str(
            r#"{ "tools": { "webSearch": { "searxngUrl": "não-é-uma-url" } } }"#,
        )
        .expect("JSON de teste deve ser válido");
        let cfg = Config::resolve(vec![camada]);

        assert!(build_web_search_tool(&cfg, Arc::new(NoopAuditSink)).is_err());
    }
}
