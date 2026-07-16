<!-- Caminho relativo: docs/adr/0028-mcp-client-via-rmcp.md -->

# ADR 0028: Cliente MCP via `rmcp` — só servidores locais (stdio) na v1

- **Status:** Accepted
- **Data:** 2026-07-15
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** dependências, MCP, tools, egresso, segurança

## Contexto

O `agentry` já tem um `ToolRegistry` (MT-11) com um gate de permissão uniforme
(`deny`/`ask`/`allow`) e um conjunto de tools próprias (`fs_*`, `shell*`, `repo_map`,
`code_search`, `lsp_*`, `glob`, `web_fetch`/`web_search`, `ask_user`). O usuário pediu
interoperar com o ecossistema **MCP** (Model Context Protocol) — deixar qualquer servidor MCP
já existente funcionar no `agentry`, o maior efeito de rede possível para o projeto (Fase 16
do roadmap de longo prazo, `docs/roadmap-longo-prazo.md`), autorizada pelo mantenedor junto da
Fase 15 (mensagem de 2026-07-15: "Vamos seguir para a fase 15 e depois a 16. Pode confirmar
ratatui e rmcp.").

`rmcp` — **verificação de maturidade feita agora, com dados de hoje** (ADR-0004):

- **MIT/Apache-2.0** → **Apache-2.0** (licença única do crate), compatível com a política do
  projeto (ADR-0001).
- **15,9M downloads totais / 8,1M nos últimos 90 dias**, versão estável atual `2.2.0`,
  atualizado pela última vez em 2026-07-08 (ativo, cadência de lançamento frequente).
- Repositório: `github.com/modelcontextprotocol/rust-sdk` — **SDK oficial**, mantido pela
  própria organização do protocolo MCP (`modelcontextprotocol`), não um projeto pessoal ou de
  terceiros. Verificado via `crates.io/api/v1/crates/rmcp`.

Uma decisão de escopo é necessária além da simples adoção da dependência: o MCP define **dois**
tipos de transporte para um cliente falar com um servidor — **local** (subprocesso, JSON-RPC
sobre `stdin`/`stdout`, feature `transport-child-process` do `rmcp`) e **remoto** (HTTP/SSE,
feature `transport-streamable-http-client`, que traz **seu próprio cliente `reqwest` interno**
ao `rmcp`). O segundo tipo colide de frente com a arquitetura já estabelecida do `agentry`:
**um único ponto de rede** (`Transport`, `crates/core/src/transport/mod.rs`, ADR-0001), com uma
`Allowlist`/`EgressClass` por candidato e auditoria centralizada (`AuditSink`) — garantida por
um teste que varre o código-fonte do projeto procurando qualquer uso de `reqwest::` fora desse
módulo. Um servidor MCP remoto conectado via o transporte HTTP nativo do `rmcp` faria chamadas
de rede **completamente fora** desse mecanismo — nenhuma `Allowlist`, nenhuma checagem de
`EgressClass`, nenhuma entrada de auditoria. Isso é uma questão de **fail-closed** (ADR-0002),
não um detalhe de implementação, e por isso não pode ser decidida em silêncio nesta ADR.

## Decisão

### Adoção da dependência — escopo mínimo de features

Fica acordada a adoção de **`rmcp`** em `crates/core` (não em `crates/cli` — descoberta e
execução de tools MCP são lógica de domínio, mesmo lugar de `LspClient`/`lsp-types`/
`lsp-server`, ADR-0013 — a CLI só registra o que o `core` descobre, como já faz para
`repo_map`/`code_search`/`lsp_*`). Features habilitadas: **`client` + `transport-child-process`
apenas** — explicitamente **sem** `server` (o `agentry` é consumidor de MCP, não expõe a si
mesmo como servidor MCP nesta fase) e **sem** `reqwest`/`transport-streamable-http-client`
(nenhum transporte HTTP nesta fase — ver próxima seção).

### v1 é só servidores MCP locais (subprocesso, `stdio`) — servidores remotos ficam fora de escopo

Fica acordado que a Fase 16 implementa **só** o transporte local: cada servidor MCP
configurado é um comando spawnado como subprocesso (`rmcp::transport::child_process::
TokioChildProcess`), falando JSON-RPC sobre `stdin`/`stdout` — **mesmo modelo de confiança já
aceito para `LspClient`** (ADR-0013): um subprocesso local, nunca mediado pela `Transport`/
`Allowlist` do projeto, porque a comunicação em si não é uma chamada de rede (é um `pipe`
local). O que o subprocesso *em si* faz depois (se ele mesmo abre uma conexão de rede) está no
mesmo nível de confiança de qualquer comando rodado via `shell_exec`/`shell_background`
(MT-13/68) — não é uma regressão de postura, é a mesma fronteira que já existe hoje.

**Servidores MCP remotos (HTTP/SSE) ficam explicitamente fora de escopo desta fase** — não
porque sejam impossíveis, mas porque exigem primeiro resolver como o tráfego atravessa (ou é
compatibilizado com) o `Transport` único do projeto sem duplicar a superfície de rede
auditável. Essa é uma decisão de arquitetura nova, não uma consequência natural desta ADR —
fica registrada aqui como **trabalho futuro explicitamente adiado**, a ser retomada só quando
houver desenho próprio (nova ADR ou revisão desta), nunca implementada via o cliente HTTP
embutido do `rmcp` sem essa etapa.

### Configuração — `mcpServers`, sempre `local-only`, nunca inferido

Novo bloco `mcpServers` em `agentry.settings.json` (mesmo padrão autoexplicativo do ADR-0022):
mapa nome → `{ command, args, egressClass }`. Vazio/desligado por padrão (zero-config = zero
servidores MCP conectados, mesmo padrão de `providers.litellm`/`tools.webSearch`). O campo
`egressClass` é **obrigatório e explícito** mesmo sendo sempre `local-only` nesta fase — nunca
inferido do fato de o transporte ser local (mesma disciplina de nunca inferir classe de egresso
por tipo de conexão, ADR-0002) — declarar qualquer outra classe é `ConfigError` tratado, com
mensagem explicando que servidores remotos ainda não são suportados (nunca um erro silencioso
ou um servidor conectado apesar da configuração pedir algo que o código não sabe fazer).

### Tools MCP no `ToolRegistry` — mesmo gate, nome prefixado por servidor

Cada tool descoberta via `list_tools()` do servidor (após o *handshake* MCP,
`ServiceExt::serve`) vira uma entrada no `ToolRegistry` já existente, implementando a *trait*
`Tool` (MT-11) como qualquer outra tool — sob o **mesmo** `PermissionGate`
(`deny`/`ask`/`allow`), nenhum mecanismo paralelo. O nome registrado é prefixado pelo nome do
servidor (`"<servidor>__<tool>"`) para nunca colidir entre dois servidores que exponham uma
tool de mesmo nome — o usuário sempre sabe, pelo nome, de qual servidor uma tool veio.

### Ciclo de vida — mesmo padrão de `LspClient`

Cada subprocesso de servidor MCP é gerenciado com o mesmo padrão já estabelecido para
`LspClient` (ADR-0013) e `ShellBackgroundTool` (MT-68): `Drop` mata o processo como rede de
segurança quando nenhum encerramento explícito aconteceu — aceitando a mesma limitação já
documentada (não cobre `std::process::exit()`), não uma regressão nova.

## Consequências

- **Impacto positivo:** qualquer servidor MCP local existente (a maioria do ecossistema hoje —
  editores, IDEs, e a maior parte dos servidores da comunidade rodam como subprocesso) passa a
  funcionar no `agentry` sem código novo por servidor; reaproveita 100% da infraestrutura já
  existente (`ToolRegistry`/`PermissionGate`) — prova mais uma vez que a fronteira de `Tool`
  (MT-11) generaliza bem, mesma lição da ADR-0027 sobre `Confirmer`/`Prompter`.
- **Impacto negativo:** servidores MCP remotos (HTTP/SSE) — parte real do ecossistema — ficam
  de fora até uma fase dedicada; primeira dependência do projeto cujo protocolo de rede
  (mesmo que só local nesta fase) não é mediado por `Transport`, uma exceção explícita à regra
  "todo I/O de rede passa por um único ponto", justificada porque não é I/O de rede real (é
  IPC local via `pipe`) mas que precisa ficar clara para quem audita a superfície do projeto.
- **Trade-offs aceitos:** escopo deliberadamente menor (só local) em troca de nunca abrir uma
  segunda via de rede não auditada; nome de tool prefixado por servidor é mais verboso do que
  nomes curtos, aceito pela clareza de origem que dá em troca.

## Diretriz de Conformidade de Código

- **Proibido:** habilitar qualquer *feature* de transporte HTTP/rede do `rmcp`
  (`reqwest`/`transport-streamable-http-client*`) sem uma ADR nova que resolva explicitamente
  como esse tráfego se integra (ou é compatibilizado) com o `Transport` único do projeto
  (ADR-0001); registrar ou conectar um servidor MCP com `egressClass` diferente de
  `local-only` nesta fase; qualquer tool MCP contornar o `PermissionGate` (MT-11) — todas
  entram no mesmo `ToolRegistry`, sob o mesmo gate, sem exceção; inferir a classe de egresso de
  um servidor MCP pelo tipo de transporte em vez de exigi-la explícita na configuração.
- **Obrigatório:** `rmcp` só em `crates/core` (nunca em `crates/cli`); nome de tool MCP
  registrado sempre prefixado pelo nome do servidor de origem; subprocesso de servidor MCP
  gerenciado com o mesmo padrão de `Drop`-mata-como-rede-de-segurança já usado por `LspClient`.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
