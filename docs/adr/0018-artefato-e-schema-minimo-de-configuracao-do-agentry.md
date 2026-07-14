<!-- Caminho relativo: docs/adr/0018-artefato-e-schema-minimo-de-configuracao-do-agentry.md -->

# ADR 0018: Artefato e schema mínimo de configuração do `agentry` (`agentry.settings.json`)

- **Status:** Accepted
- **Data:** 2026-07-12
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** integração, dados, formato, governança, settings-schema

## Contexto

A ADR-0003 propôs que o `agentry` consumisse um `settings-schema:1` mínimo (parâmetros de
modelo + permissões) a partir do `.claude/settings.json` de cada perfil do
`ai-coding-agent-profiles`, deixando o esquema exato "em definição". Nos micro-tickets
seguintes (MT-17 em diante), seis extensões desse schema foram propostas e registradas no
`exchange-log` (ADRs 0007/0008/0009/0010-13/0014/0015) — cada uma deliberadamente adiada com
a mesma nota: *"formato definitivo fica para a implementação, a confirmar com o `profiles`
antes de congelar"*. Nenhuma foi de fato confirmada. Hoje `Settings::from_process_env`
(`crates/core/src/config/mod.rs`) é o único carregador real — só variáveis de ambiente; ~9
chaves já mecanicamente prontas no código (flags de repo-map/RAG/LSP/saída-estruturada,
presets por `task-class`, `reasoning`, timeout/`keep_alive`, config do Reviewer) estão todas
travadas no próprio *default* hardcoded, sem nenhum jeito real do usuário mudar.

Investigação do lado `ai-coding-agent-profiles` (leitura direta do repositório, não suposição)
revelou que o artefato hoje existente — `profiles/<perfil>/.claude/settings.json` — é o
**formato nativo do Claude Code** (`env`: `MAX_THINKING_TOKENS`/`CLAUDE_CODE_SUBAGENT_MODEL`;
`permissions.deny`/`ask` como *padrões* estilo `"Bash(git push*)"`), não o que a ADR-0003
supõe. `agentry::config::Permissions.deny`/`ask` já espera **nomes exatos de tool do agentry**
(`"repo_map"`, `"shell_exec"`, `"code_search"`) — sintaxe incompatível com padrões glob de Bash, e
não há ali nada sobre roteamento por `task-class`, seleção de provider, ou as flags de
contexto (RAG/repo-map/LSP)/Reviewer. Consumir esse artefato diretamente exigiria reinterpretar
um formato desenhado para outro consumidor — não é uma opção sólida.

## Decisão

Fica acordado um **artefato separado**, de propriedade conceitual do `agentry` (o schema é
definido aqui, no repo executor — o `profiles` só distribui valores *default* por perfil,
igual já faz hoje com `.claude/settings.json`):

1. **Artefato:** `.agentry/agentry.settings.json`, um por repositório-alvo. JSON — mesma
   convenção de formato do `.claude/settings.json` vizinho, zero dependência nova
   (`serde_json` já é usado em `Settings`/`Permissions`, que já derivam `Serialize`/
   `Deserialize`).
2. **Local:** dentro do diretório de estado já resolvido pela ADR-0017/MT-38
   (`state_dir::resolve_root` — busca ascendente por `.git`, *fallback* pro cwd). **Exceção
   nomeada** à auto-exclusão do `.agentry/.gitignore` (emenda à ADR-0017): este arquivo
   **deve** ser versionado, ao contrário do resto de `.agentry/` (sessão, índices, audit
   log), porque é artefato de política distribuído pelo `profiles`, não estado privado da
   máquina.
3. **Precedência de camadas** (`Config::resolve`, `crates/core/src/config/mod.rs`): *default*
   do perfil (implícito, hoje só a taxonomia de privacidade do ADR-0002) < arquivo
   `agentry.settings.json` < variáveis de ambiente. O arquivo é a base versionada do projeto;
   env continua disponível para *overrides* pontuais (CI, execução isolada) sem editar o
   arquivo — mesma convenção já usada implicitamente por `Settings::resolve`.
4. **Ausência do arquivo não é erro** — mesmo espírito do "manifesto ausente" do MT-29:
   projeto sem o arquivo simplesmente usa os *defaults* de cada capacidade (documentados em
   cada ADR de origem, todos `true` para o pacote ADR-0010..0013).
5. **Primeira fatia de schema, congelada agora** (a mais simples e de maior alavancagem — 4
   *flags* booleanas já mecanicamente prontas no código, hoje hardcoded `true`, mais
   permissões):
   ```json
   {
     "$schema": "https://agentry.dev/schema/agentry-settings-schema-1.json",
     "schemaVersion": 1,
     "permissions": {
       "deny": [],
       "ask": []
     },
     "context": {
       "repoMap": { "enabled": true },
       "semanticRag": { "enabled": true },
       "lspGrounding": { "enabled": true }
     },
     "providers": {
       "ollama": { "structuredOutput": true }
     }
   }
   ```
   `permissions.deny`/`ask` usam **nomes exatos de tool do agentry** (`"repo_map"`,
   `"code_search"`, `"shell_exec"`, ...) — nunca os padrões Bash do Claude Code, domínio diferente.
   Todo campo é opcional; ausente ⇒ *default* de cada ADR de origem.
6. **Deliberadamente fora desta fatia** — cada um ganha sua própria extensão de schema
   (nova seção deste ADR ou um ADR sucessor) quando o ticket de consumo for implementado,
   mesmo padrão já usado em toda a Fase 6: presets por `task-class` (ADR-0008),
   timeout/`keep_alive` por provider (ADR-0009), `reasoning`/`RuntimeOverride` (ADR-0014),
   habilitação/modo do Reviewer por tipo de auditoria (ADR-0015), `guardrails` (ADR-0007 —
   mecanismo ainda nem implementado).
7. **Lado `profiles`:** cada perfil (`empresa`/`externo-confidencial`/`pessoal`) ganha um
   `.agentry/agentry.settings.json` *default* próprio (mesma lógica de diferenciação já usada
   em `.claude/settings.json` — `empresa` mais conservador), distribuído por
   `scripts/setup-profile.sh` com a mesma disciplina de `--update` não-destrutivo (*deep-merge*
   via `jq`, regra vence conflito, customização do usuário sobrevive) já usada para
   `.claude/settings.json`.

## Consequências

- **Impacto positivo:** desbloqueia a configuração real de tudo que hoje está hardcoded;
  schema de propriedade do `agentry` (o lado que efetivamente consome) em vez de forçar
  reinterpretação de um formato alheio; convivência sem conflito com `.claude/settings.json`
  (domínios/consumidores diferentes, mesma pasta de nível superior `.claude`/`.agentry`
  paralelas); reaproveita a descoberta de raiz e a auto-exclusão já construídas no MT-38, só
  com uma exceção nomeada.
- **Impacto negativo:** mais um artefato para o `profiles` manter por perfil; a exceção no
  `.gitignore` de `.agentry/` precisa de disciplina (só nomes exatos de arquivo, nunca
  padrões amplos) para não vazar estado privado por engano.
- **Trade-offs aceitos:** schema cresce incrementalmente (uma fatia por ADR de origem
  implementada) em vez de ser desenhado de uma vez — mais lento para cobrir tudo, mas cada
  fatia é validada contra um consumidor real antes de congelar, evitando desenhar campos que
  never chegam a ser usados como propostos.

## Diretriz de Conformidade de Código

- **Proibido:** `agentry` consumir `.claude/settings.json` (formato nativo do Claude Code,
  de outro domínio) como fonte de configuração própria; `permissions.deny`/`ask` do
  `agentry.settings.json` aceitar padrões glob estilo Bash — só nomes exatos de tool; qualquer
  camada de configuração contornar a checagem de classe de egresso/permissões já resolvida
  (mesma fronteira de segurança já estabelecida pela ADR-0014/`RuntimeOverride`); adicionar
  chave nova ao schema sem uma ADR de origem correspondente já implementada.
- **Obrigatório:** descoberta via `state_dir::resolve_root` (MT-38); ausência do arquivo
  tratada como *default*, nunca erro; precedência perfil < arquivo < variável de ambiente;
  toda extensão futura do schema referencia a ADR do ADR de origem da capacidade que a motiva.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
