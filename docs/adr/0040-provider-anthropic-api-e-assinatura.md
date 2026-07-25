<!-- Caminho relativo: docs/adr/0040-provider-anthropic-api-e-assinatura.md -->

# ADR 0040: Provider Anthropic — API key e assinatura Pro/Max via CLI

- **Status:** Accepted
- **Data:** 2026-07-24
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** providers, egresso, auditoria, conformidade

## Contexto

O `AnthropicProvider` (MT-16, `crates/core/src/provider/anthropic.rs`) já existe completo e
testado desde 2026-07-08 — `chat`, `chat_stream` (SSE), *tool use*, extração de mensagens de
sistema para o campo `system` da Messages API — mas **nunca foi registrado no `Router` da
CLI**. Há inclusive um teste afirmando a ausência (`main.rs`, `provider 'anthropic' não está
registrado no router desta CLI`). Hoje a CLI só alcança Ollama (local) e LiteLLM (gateway).

O mantenedor pediu acesso à Anthropic por **dois caminhos distintos**:

1. **API key** — chave da Anthropic API, cobrada por token.
2. **Assinatura Claude Pro/Max** — a conta que já paga o Claude Code.

Os dois caminhos não são a mesma integração. A Messages API autentica por `x-api-key`. A
assinatura Pro/Max autentica por um token OAuth **emitido para o Claude Code**: reusá-lo a
partir de um CLI de terceiros viola os termos de uso, e o token não é um segredo que o
projeto deva ler, copiar ou persistir. O caminho suportado para construir sobre uma
assinatura é invocar o próprio binário `claude` em modo *headless*
(`claude -p --output-format stream-json`), que resolve a autenticação sozinho a partir do
login que o usuário já fez.

Isso cria uma tensão real com a ADR-0002: um subprocesso **não passa pelo `Transport`**, logo
fica fora da `Allowlist` e do audit log de egresso — exatamente os mecanismos que sustentam o
objetivo de homologação corporativa do projeto. Um provider que sai da máquina sem deixar
rastro na trilha de auditoria seria uma regressão de conformidade, não uma feature.

## Decisão

Ficam acordados **dois providers separados**, com nomes distintos no `Router`:

### 1. `anthropic` — Messages API por chave

Fiação do `AnthropicProvider` já existente, no mesmo molde de `build_litellm_provider`
(MT-49): `Transport` dedicado, `Allowlist` restrita ao host de `providers.anthropic.baseUrl`
sob a `egressClass` declarada, headers `x-api-key` e `anthropic-version` anexados via
`Transport::with_header`. A chave vem de `credentials::resolve_api_key` (MT-128/ADR-0038) —
variável de ambiente com curto-circuito, senão `~/.agentry/credentials.json`; nunca argumento
de linha de comando. Ausência do bloco `providers.anthropic` ⇒ provider não registrado,
mesmo padrão de `providers.litellm`.

**Correção acoplada:** o `AnthropicProvider` traduz `reasoning: Some(true)` para
`thinking: {"type":"enabled","budget_tokens":N}`. Esse formato é **rejeitado com HTTP 400**
nos modelos atuais (Opus 4.7 em diante, Opus 5, Sonnet 5, Fable 5), onde `budget_tokens` foi
removido. Passa a emitir `thinking: {"type":"adaptive"}`, e a profundidade é controlada por
`output_config.effort` — não por orçamento de tokens.

### 2. `claude-cli` — assinatura Pro/Max por subprocesso

Novo `ClaudeCliProvider` que faz *spawn* de `claude -p --output-format stream-json`,
traduzindo o `stream-json` para os `StreamEvent` do MT-02. Não lê, não copia e não persiste
nenhum token — a autenticação é inteiramente do binário `claude`.

**Auditoria equivalente é condição da decisão, não um extra.** Como o egresso não passa pelo
`Transport`, o `ClaudeCliProvider` emite ele próprio uma `AuditEntry` por invocação, no
**mesmo** `AuditSink` já fiado pela CLI (portanto no mesmo `.agentry/audit.log` da ADR-0037,
e no `stderr` quando aplicável). A entrada declara `destination` identificando o subprocesso
e o modelo, e a decisão de egresso resolvida — de forma que a trilha de conformidade não
tenha buraco: quem lê o audit log vê que houve egresso para a Anthropic, quando e sob que
classe, mesmo sem o `Transport` no caminho.

**A classe de egresso é verificada antes do *spawn*.** O `ClaudeCliProvider` recebe a
`EgressClass` resolvida da sessão e recusa executar quando ela não permite nuvem, emitindo
`AuditEntry` com `AuditOutcome::Blocked` e devolvendo `ProviderError` — sem nunca criar o
processo. Isso espelha o comportamento do `Transport` sob `Allowlist` (MT-05/07): bloquear
**antes** de tocar a rede, não depois.

Descartada a alternativa de ler o token OAuth do Claude Code diretamente: além de violar os
termos, colocaria o projeto na posição de manusear credencial de terceiro — o oposto da
postura da ADR-0038, onde a CLI nunca ecoa nem copia segredo.

## Consequências

- **Impacto positivo:** os dois modos de acesso que o mantenedor pediu passam a existir sem
  exceção na trilha de auditoria; o `AnthropicProvider` (já escrito e testado) deixa de ser
  código morto; a correção do `thinking` evita 400 em todo modelo atual.
- **Impacto negativo:** `claude-cli` depende de um binário externo instalado e autenticado —
  falha de *spawn* é um modo de erro novo, que precisa ser tratado e reportado claramente
  (nunca mascarado). O formato `stream-json` do Claude Code é superfície de terceiro e pode
  mudar entre versões.
- **Trade-offs aceitos:** aceitar a dependência de um binário externo e o acoplamento a um
  formato de terceiro, em troca de acesso legítimo à assinatura Pro/Max sem manusear token
  OAuth alheio.

## Diretriz de Conformidade de Código

- **Proibido:** ler, copiar, persistir ou logar o token OAuth do Claude Code; fazer *spawn*
  do `claude` sem antes verificar a classe de egresso da sessão; fazer *spawn* sem emitir a
  `AuditEntry` correspondente; passar chave de API como argumento de linha de comando
  (apareceria em `ps`); emitir `budget_tokens` no campo `thinking` da Messages API.
- **Obrigatório:** `claude-cli` e `anthropic` são nomes **distintos** no `Router` — nunca
  fundir os dois caminhos sob um nome só, porque têm modelos de custo, de autenticação e de
  auditoria diferentes, e o usuário precisa escolher explicitamente; toda invocação de
  subprocesso produz exatamente uma `AuditEntry` no mesmo sink da CLI, permitida ou
  bloqueada; falha de *spawn* ou `stream-json` malformado vira `ProviderError` tratado, nunca
  `panic` nem silêncio.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
