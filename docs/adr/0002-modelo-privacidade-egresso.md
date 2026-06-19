<!-- Caminho relativo: docs/adr/0002-modelo-privacidade-egresso.md -->

# ADR 0002: Modelo de privacidade/egresso e taxonomia de classes

- **Status:** Accepted
- **Data:** 2026-06-19
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** segurança, privacidade, rede, dados

## Contexto

O alvo do projeto é homologação corporativa, onde **confidencialidade dos dados é requisito,
não feature opcional**. É preciso garantir, de forma **auditável**, que dados marcados como
sensíveis não saiam para a nuvem. Os perfis definidos no `ai-coding-agent-profiles`
(`empresa` / `externo-confidencial` / `pessoal`) precisam mapear em comportamento de rede
**determinístico**. A decisão é fundacional: o agent loop, os providers e as tools só podem
ser construídos sobre uma fronteira de egresso já definida.

## Decisão

Fica acordado que o controle de egresso é **imposto na fronteira HTTP** por um **único módulo
de transporte auditável**, com **allowlist de endpoints por perfil**. Ratifica-se a taxonomia
`privacy-taxonomy:1`:

| Perfil | Classe | Regra de rede |
|---|---|---|
| `empresa` | `local-only` | Egresso para nuvem **proibido**; só endpoints on-premise/aprovados na allowlist |
| `externo-confidencial` | `cloud-opt-out` | Nuvem só com opt-out de retenção comprovado + allowlist |
| `pessoal` | `cloud-ok` | APIs de nuvem livres (bom senso de custo) |

Comportamento **fail-closed**: ausência ou ambiguidade de classe ⇒ tratar como `local-only`.
**Zero telemetria**. **Redação de segredos** na borda (alinhado à skill `secrets-guard`).
**Audit log** estruturado de cada egresso (destino, perfil, classe, tarefa).

## Consequências

- **Impacto positivo:** garantia auditável de não-vazamento; argumento direto de homologação;
  *fail-closed* evita degradação silenciosa de confidencialidade.
- **Impacto negativo:** exige disciplina — todo provider/tool que faça rede deve passar pelo
  transporte central; a classe `local-only` inviabiliza providers de nuvem para aquele perfil
  (comportamento esperado).
- **Trade-offs aceitos:** rigidez de arquitetura em favor de confiança e auditabilidade.

## Diretriz de Conformidade de Código

- **Proibido:** qualquer chamada de rede fora do módulo de transporte central; endpoint fora
  da allowlist do perfil ativo; telemetria ou coleta externa de qualquer natureza; registrar
  segredos em logs.
- **Obrigatório:** resolver a classe de privacidade **antes** de qualquer chamada; *default*
  `fail-closed = local-only`; emitir audit log estruturado de todo egresso.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
