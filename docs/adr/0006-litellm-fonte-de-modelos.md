<!-- Caminho relativo: docs/adr/0006-litellm-fonte-de-modelos.md -->

# ADR 0006: LiteLLM como fonte de modelos via adapter OpenAI-compatible

- **Status:** Accepted
- **Data:** 2026-07-06
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** llm, providers, egresso, integração

## Contexto

O `agentry` deve alcançar o maior número possível de modelos sem inflar a árvore de
dependências (ADR-0001) nem furar a fronteira de egresso (ADR-0002). O **LiteLLM**
(open-source, licença MIT) é um *gateway*/proxy amplamente adotado que expõe uma API
**OpenAI-compatible** na frente de 100+ provedores (OpenAI, Anthropic, Bedrock, Vertex,
Azure, Ollama, etc.), com gestão de chaves, cotas e observabilidade centralizadas — perfil
comum em ambientes corporativos, onde um único gateway interno intermedeia todo acesso a LLMs.

Duas forças exigem decisão agora:

1. **Como integrar** — adapter dedicado ou reutilizar o adapter OpenAI-compatible já
   previsto (MT-15)?
2. **Como classificar o egresso** — um proxy LiteLLM pode estar hospedado **localmente** e
   ainda assim **encaminhar dados para a nuvem**. O destino efetivo dos dados não é o host
   do proxy, e tratá-lo ingenuamente como "endpoint local" degradaria a confidencialidade
   silenciosamente — exatamente o que o ADR-0002 proíbe.

## Decisão

Fica acordado que o **LiteLLM é fonte de modelos suportada da v0.1**, consumido
**exclusivamente através do adapter OpenAI-compatible** (MT-15), sem adapter dedicado e sem
dependência nova de runtime — o LiteLLM é um **endpoint configurável**, não uma biblioteca.

Para o egresso, ratifica-se a regra de **classificação pelo destino efetivo**:

- Todo endpoint LiteLLM declarado na configuração recebe **obrigatoriamente** uma classe de
  egresso explícita (`local-only` / `cloud-opt-out` / `cloud-ok`), atribuída por quem opera
  o gateway conforme os *backends* que ele roteia.
- **Fail-closed invertido para proxies:** na ausência de declaração explícita, um endpoint
  LiteLLM é tratado como **`cloud-ok` do ponto de vista de risco** — isto é, **proibido**
  para perfis `local-only`/`cloud-opt-out` — ainda que o host seja local. Um proxy só é
  utilizável em perfil restritivo se declarado (e auditável) como roteando apenas para
  *backends* aprovados.

O critério de aceite do MT-15 passa a incluir um caso de teste com endpoint LiteLLM
(allowlist + classe declarada + caso fail-closed sem declaração).

## Consequências

- **Impacto positivo:** alcance imediato de 100+ provedores e aderência ao padrão
  corporativo de gateway central, com **zero dependência nova** e sem código de transporte
  adicional; a decisão reforça (em vez de contornar) ADR-0001 e ADR-0002.
- **Impacto negativo:** recursos específicos de provedores (ex.: *prompt caching* da
  Anthropic) ficam limitados ao que o LiteLLM traduz pela superfície OpenAI; a
  classificação de egresso do gateway depende de declaração honesta do operador — o
  `agentry` audita a fronteira, não o interior do proxy.
- **Trade-offs aceitos:** menor fidelidade a recursos nativos quando acessados via gateway
  (o adapter Anthropic direto do MT-16 permanece para esses casos); superfície de
  configuração um pouco maior (classe obrigatória por endpoint proxy).

## Diretriz de Conformidade de Código

- **Proibido:** adapter dedicado para LiteLLM ou dependência de runtime ligada a ele;
  tratar endpoint de proxy/gateway como `local-only` por inferência de host (ex.:
  `localhost`) sem classe declarada na configuração.
- **Obrigatório:** todo tráfego para LiteLLM passa pelo adapter OpenAI-compatible e pelo
  transporte único (ADR-0001/0002); endpoints de proxy exigem classe de egresso explícita
  na configuração, com ausência ⇒ tratado como nuvem (bloqueado em perfis restritivos);
  audit log registra o endpoint do gateway como destino do egresso.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
