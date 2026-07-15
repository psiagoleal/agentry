<!-- Caminho relativo: docs/adr/0026-tool-glob-e-shell-em-background.md -->

# ADR 0026: Tool `Glob` (busca por padrão de arquivo) e shell em background/streaming

- **Status:** Proposed
- **Data:** 2026-07-15
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** tools, filesystem, shell

## Contexto

Duas lacunas independentes de paridade com Claude Code CLI/OpenCode, agrupadas na mesma ADR
por serem pequenas e não terem nenhuma decisão de design compartilhada — só convivem na mesma
fase do roadmap (Fase 14):

1. **Busca por nome de arquivo.** `FsSearchTool` (MT-12) busca **conteúdo** (substring
   literal dentro dos arquivos); `RepoMapTool`/`CodeSearchTool` (MT-21/30) rankeiam
   relevância. Nenhuma tool responde "quais arquivos casam com `**/*.test.rs`?" — um padrão de
   nome de arquivo, sem olhar conteúdo. Claude Code CLI e OpenCode têm essa tool (`Glob`);
   `agentry` não.
2. **Shell de longa duração.** `ShellTool` (MT-13) só roda um comando até o fim e devolve o
   resultado — não dá para rodar um `dev server`/`watch` que precisa continuar rodando
   **enquanto o agente segue trabalhando**, e depois consultar a saída.

## Decisão

### `Glob`

Fica acordada a tool **`glob`**, reaproveitando `ignore::overrides::OverrideBuilder` +
`ignore::WalkBuilder` (mesma *crate* já usada por `tools::fs`/`repo_map`/`code_search`,
MT-12/21/30 — **nenhuma dependência nova**; `OverrideBuilder` é o mesmo mecanismo que o
`ripgrep` usa para a própria *flag* `--glob`, já maduro dentro da *crate* já auditada,
ADR-0004). Recebe um padrão (`"**/*.rs"`, sintaxe glob padrão) e devolve os caminhos que
casam, relativos à raiz do *workspace* — respeitando `.agentryignore`/`.claudeignore` (e
`context.gitignore.enabled`, se ligado) como qualquer tool de filesystem, e capados a um teto
de resultados (mesmo espírito de `MAX_RESULTADOS` do `repo_map`, MT-21) para nunca devolver
uma lista gigante. Sob o mesmo `PermissionGate` genérico, sem *default-deny* especial (leitura
de metadados de caminho, sem conteúdo, mesma categoria de `fs_read`).

### Shell em background/streaming

Fica acordada uma extensão de `crates/core/src/tools/shell.rs` (MT-13) — **não** uma tool
nova isolada, e sim mais capacidades sobre a mesma `ShellPolicy`/`CommandRunner` já
existentes, para que rodar em segundo plano **nunca** contorne a política *default-deny* do
comando (mesma checagem de `ShellPolicy::decide` que `ShellTool::execute` já faz hoje).

Uma tool `shell_background` com um campo `action` (`"start"` | `"output"` | `"stop"`):

- **`start`**: spawna o comando via `tokio::process` (já dependência, *feature* `process` já
  ligada desde o MT-13) **sem esperar terminar**; devolve um identificador de processo
  (`id`). Uma tarefa em segundo plano (`tokio::spawn`) lê `stdout`/`stderr` continuamente para
  um buffer compartilhado (`Arc<Mutex<...>>`), **truncado** a um teto de tamanho — um `watch`
  que nunca para não pode crescer sem limite na memória do processo `agentry`.
- **`output`**: dado um `id`, devolve o que foi capturado **desde a última consulta** (leitura
  não bloqueante do buffer) — nunca espera o processo terminar.
- **`stop`**: dado um `id`, mata o processo (mesmo `kill`/`wait` já usado pelo `Drop` do
  `LspClient`, MT-23, como referência de "matar processo filho de forma limpa").

Processos em segundo plano que sobrevivem ao fim da sessão (`agentry` encerrado sem `stop`
explícito) são **rede de segurança**, não o caminho feliz — mesmo espírito do `Drop` do
`LspClient` (mata + espera se `shutdown` nunca foi chamado): o registro de processos em
segundo plano faz o mesmo na finalização do processo `agentry`.

## Consequências

- **Impacto positivo:** paridade com uma capacidade real do Claude Code CLI/OpenCode (rodar
  `dev server`/`watch` sem bloquear o agente); reaproveita 100% de infraestrutura já madura
  (`ignore`, `tokio::process`, `ShellPolicy`); zero dependência nova.
- **Impacto negativo:** processos em segundo plano são um novo tipo de estado que precisa ser
  limpo corretamente (memória do buffer, processos órfãos) — mitigado pelo teto de buffer e
  pela limpeza na finalização.
- **Trade-offs aceitos:** um único identificador de processo por chamada de `start` (sem
  gerência de múltiplos processos com nomes/grupos) — suficiente para o caso de uso descrito
  (um `dev server` por vez), generalização além disso fica para quando houver demanda real.

## Diretriz de Conformidade de Código

- **Proibido:** `shell_background` contornar `ShellPolicy` (mesmo *default-deny* de comando
  do `ShellTool`, MT-13) — é a mesma política, não uma política paralela mais permissiva;
  buffer de saída sem teto de tamanho; processo em segundo plano sem mecanismo de finalização
  ao processo `agentry` terminar.
- **Obrigatório:** `Glob` respeita `.agentryignore`/`.claudeignore` como qualquer tool de
  filesystem; resultado de `Glob` capado a um teto de itens; `shell_background output` nunca
  bloqueia esperando o processo terminar.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
