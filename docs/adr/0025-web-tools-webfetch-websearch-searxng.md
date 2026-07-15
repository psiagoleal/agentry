<!-- Caminho relativo: docs/adr/0025-web-tools-webfetch-websearch-searxng.md -->

# ADR 0025: Web tools — `WebFetch` e `WebSearch` via SearXNG configurável

- **Status:** Proposed
- **Data:** 2026-07-15
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** tools, egresso, rede, anonimato, segurança

## Contexto

O agente não tem hoje nenhuma capacidade de acessar a web — toda tool existente é local
(filesystem, shell, repo-map/RAG, LSP, skills). O usuário pediu explicitamente **web search
anônimo via SearXNG configurável**, "seguindo conceitos de segurança e anonimato, evitando
rastreabilidade" — sem instância pública hardcoded (risco de disponibilidade e de cadeia de
suprimentos: uma instância pública de terceiros poderia ficar fora do ar, ser maliciosa, ou
registrar consultas).

Duas capacidades distintas cabem aqui: **`WebFetch`** (buscar o conteúdo de uma URL dada) e
**`WebSearch`** (pesquisar um termo, via SearXNG). As duas exigem rede — passam pelo `Transport`
único (ADR-0002), nunca por um `reqwest` avulso — mas têm um problema de design **diferente**
de todo provider já integrado (Ollama/LiteLLM/Anthropic/OpenAI-compatible): aqueles têm **um
host fixo, configurado uma vez**; `WebFetch` por natureza busca **qualquer URL que o modelo
peça**, um host que não dá para pré-cadastrar na `Allowlist` (MT-05) hoje — que só sabe casar
host exato ou `*.sufixo`, nunca "qualquer host". `WebSearch` já é diferente: o endpoint SearXNG
é **um host fixo**, configurado pelo usuário — cabe no modelo de allowlist existente, igual ao
LiteLLM (ADR-0006).

Uma decisão é necessária agora para (a) resolver esse problema de host arbitrário do
`WebFetch` sem abrir uma brecha no *fail-closed* do ADR-0002, e (b) fixar o mecanismo de
anonimato do `WebSearch`.

## Decisão

### `WebFetch` — host arbitrário, só sob `cloud-ok`

A `Allowlist`/`AllowlistEntry` (`crates/core/src/egress/allowlist.rs`, MT-05) ganha um
**padrão coringa explícito**, `"*"` (casa qualquer host — hoje só `*.sufixo`, nunca o
domínio nu; `"*"` sozinho é um terceiro caso, deliberado). Continua fail-closed por
construção: uma entrada coringa **precisa ser adicionada explicitamente** (nunca é o
comportamento padrão de uma `Allowlist` vazia) e carrega sua própria `required_class` como
qualquer entrada.

A CLI monta um `Transport` **dedicado** para `WebFetch` (mesmo padrão já usado para o
LiteLLM, `build_litellm_provider`) com uma única entrada coringa exigindo
`EgressClass::CloudOk` — a classe mais permissiva da taxonomia (ADR-0002). **A tool só é
registrada quando `cfg.egress_class == CloudOk` *e* `tools.webFetch.enabled` está ligado
(`FeatureToggle`, *default* `false`)** — as duas condições, não uma: acessar qualquer host da
internet é uma classe de risco distinta de "ligado por padrão porque é barato" (repoMap/
semanticRag/lspGrounding/agentsFile); segue a mesma disciplina de *opt-in* explícito do
Reviewer (ADR-0015) e dos guardrails (ADR-0007) — capacidades de risco real vêm **desligadas**
até o usuário optar. Sob `local-only`/`cloud-opt-out`, a entrada coringa nunca teria classe
suficiente de qualquer forma (`permits` sempre falha) — não registrar a tool nesses perfis
evita expor ao modelo uma tool que garantidamente falharia toda chamada.

### `WebSearch` — host fixo (SearXNG), mesmo modelo do LiteLLM

Sem instância pública *hardcoded*. Novo bloco `tools.webSearch` no schema
(`agentry.settings.json`, ADR-0018/0021):

```jsonc
"tools": {
  "webSearch": {
    "searxngUrl": null,
    "searxngEgressClass": null
  }
}
```

- **`searxngUrl`** — URL base de uma instância SearXNG (própria ou de confiança) informada
  pelo usuário. `null`/ausente ⇒ `WebSearch` **não é registrada** — mesmo padrão de
  `providers.litellm` (`baseUrl`+`model` ausentes ⇒ sem candidato).
- **`searxngEgressClass`** — classe de egresso mínima do endpoint, **sempre explícita quando
  o usuário declara `searxngUrl`** (ADR-0002: nunca inferida do host). Ausente ⇒ `cloud-ok`
  (o mais restritivo para liberar, mesmo *default* do `providers.litellm.egressClass`,
  ADR-0006) — mas diferente de `WebFetch`, aqui faz sentido real declarar `local-only`: uma
  instância SearXNG *self-hosted* na rede interna do usuário é tão local quanto o Ollama.

Endpoint único e conhecido ⇒ `AllowlistEntry::new(host_from_url(&searxng_url), egress_class)`,
exatamente o padrão já usado para LiteLLM — **sem** o coringa do `WebFetch`.

Consulta via a **API JSON do SearXNG** (`GET <searxngUrl>/search?q=<query>&format=json`) —
`serde_json` (já dependência) faz o *parse*; título/URL/resumo de cada resultado formatados
como texto para o modelo.

### Anonimato/segurança (`WebFetch` e `WebSearch`)

Requisito explícito do usuário, não opcional:

- **Sem cookies** — já garantido pela configuração atual do `reqwest` (`features = ["json",
  "rustls-tls"]`, sem a *feature* `cookies`): a *crate* nunca tem um *cookie jar*, não há
  nada para desligar.
- **User-Agent genérico, não identificável** — `Transport::with_header("User-Agent",
  "agentry-web-tool/1")` (reaproveita o builder já existente, criado para o `Authorization`
  do LiteLLM, MT-49) fixado nas duas tools — nunca o *default* do `reqwest` nem qualquer
  string que identifique o usuário/máquina/versão do SO.
- **Sem `Referer`** — nunca setado (não é automático fora de navegador; só aconteceria se o
  código explicitamente adicionasse o *header*, o que fica proibido abaixo).
- **Sem parâmetro de rastreio próprio** — a *query string* de `WebSearch` carrega só `q` e
  `format`; nenhum identificador de sessão/cliente é anexado pelo `agentry`.

### Conteúdo devolvido

`WebFetch` devolve o corpo da resposta **como texto puro**, truncado a um teto (evita
consumir todo o orçamento de contexto com uma página gigante) — **sem** conversão
HTML→Markdown nem extração de texto legível: isso exigiria um *parser* de HTML, **dependência
de runtime nova**, fora do escopo desta ADR (registrar a necessidade e escalar ao mantenedor
quando/se um ticket futuro decidir perseguir isso, seguindo a regra de parada dura do comando
de loop). `WebSearch` devolve a lista formatada de resultados (título, URL, resumo), sem
limite de dependência nova (a resposta já vem estruturada em JSON pelo SearXNG).

## Consequências

- **Impacto positivo:** o agente ganha acesso à web sem abrir brecha no modelo fail-closed —
  `WebFetch` só funciona no perfil mais permissivo **e** com *opt-in* explícito; `WebSearch`
  nunca usa uma instância de terceiros não configurada pelo usuário; anonimato é requisito de
  código, não best-effort. Zero dependência nova.
- **Impacto negativo:** `WebFetch` sem HTML→Markdown devolve texto bruto (pode incluir marcação
  HTML visível) — pior legibilidade em troca de não trazer dependência nova agora; o coringa
  `"*"` na `Allowlist` é uma extensão real do mecanismo de egresso (auditada aqui, não
  silenciosa).
- **Trade-offs aceitos:** `WebFetch` restrito ao perfil `cloud-ok` (não há meio-termo — um
  host arbitrário não pode ser classificado individualmente); texto bruto em vez de
  Markdown limpo.

## Diretriz de Conformidade de Código

- **Proibido:** qualquer chamada de rede de `WebFetch`/`WebSearch` fora do `Transport` único;
  registrar `WebFetch` fora da combinação `tools.webFetch.enabled=true` **e**
  `egress_class=CloudOk`; hardcode de qualquer instância SearXNG pública; anexar `Referer`,
  cookies, ou qualquer parâmetro de rastreio às requisições; adicionar um *parser*/conversor
  de HTML sem uma nova ADR que avalie a dependência (ADR-0004).
- **Obrigatório:** `User-Agent` genérico fixo nas duas tools; entrada coringa (`"*"`) da
  `Allowlist` só usada pelo `Transport` dedicado de `WebFetch`, nunca reaproveitada por
  nenhum outro provider/tool; `searxngEgressClass` ausente resolve `cloud-ok` (fail-closed);
  resposta de `WebFetch` sempre truncada a um teto de tamanho.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
