<!-- Caminho relativo: docs/adr/0007-guardrails-configuraveis-de-conteudo.md -->

# ADR 0007: Guardrails configuráveis de conteúdo (gate distinto do Tool Registry)

- **Status:** Proposed
- **Data:** 2026-07-07
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** segurança, configuração, guardrails, interop

## Contexto

O `agentry` já tem dois mecanismos de controle bem definidos: o gate de **permissão de
ação** sobre tools (`allow`/`ask`/`deny`, previsto no MT-11) e a **allowlist de egresso**
sobre destinos de rede (ADR-0002, MT-05). Nenhum dos dois cobre uma terceira dimensão:
restrições sobre o **conteúdo** que entra ou sai de uma chamada de LLM — por exemplo,
bloquear um padrão de *prompt injection* conhecido, recusar um tópico proibido pelo perfil,
ou forçar redação de um trecho da resposta antes de exibi-la. Sem um lugar definido para
isso, cada integração futura tenderia a inventar sua própria checagem ad-hoc.

Pelo charter de interoperabilidade (`docs/interop/README.md`, SPEC v1 do
`ai-coding-agent-profiles`), **política** — o que é permitido, por perfil — é definida pelo
`profiles`; o `agentry` **executa e impõe**, não inventa regra. Guardrails de conteúdo são,
por natureza, política (o que pode/não pode passar), então a fonte das regras deve vir do
`settings-schema` (artefato do `profiles`, hoje rascunho sob ADR-0003), não de configuração
inventada localmente no `agentry`.

## Decisão

Fica acordada a criação de um **Guardrail Gate** — módulo de execução novo, paralelo ao gate
de tools e à allowlist de egresso — aplicado em dois pontos: **entrada** (antes de enviar o
prompt ao provider) e **saída** (depois de receber a resposta, antes de expor ao usuário ou
logar). As regras são correspondência determinística (palavra-chave/regex simples) — v0.1
**não** inclui moderação por modelo/serviço externo, o que seria egresso adicional sujeito à
allowlist do ADR-0002.

As regras vivem numa extensão do `settings-schema` (chave `guardrails`, a ratificar em ADR
específico de esquema quando implementada) e seguem o mesmo padrão de camadas do MT-04:
regras herdadas do perfil só podem ser **reforçadas**, nunca afrouxadas, por uma camada mais
específica (mesma semântica de união usada em `Permissions::union`).

## Consequências

- **Impacto positivo:** superfície configurável e auditável para uma classe de risco hoje
  descoberta; reaproveita os padrões de camada (MT-04) e de audit log (MT-06) já existentes;
  não inventa política — a fonte de regras continua sendo o perfil.
- **Impacto negativo:** correspondência por palavra-chave/regex tem falsos positivos e
  negativos; não substitui moderação semântica real.
- **Trade-offs aceitos:** determinismo e auditabilidade agora, em vez de sofisticação de
  detecção — moderação por modelo fica para uma v0.2, se necessária, sob novo ADR.

## Diretriz de Conformidade de Código

- **Proibido:** uma camada mais específica (projeto) afrouxar uma regra de guardrail
  herdada do perfil; qualquer guardrail que dependa de serviço externo de moderação sem
  passar pelo transporte único e pela allowlist (ADR-0002); inventar regra de guardrail fora
  do `settings-schema` do `profiles`.
- **Obrigatório:** toda decisão de bloqueio do Guardrail Gate gera entrada de auditoria
  (reaproveitando `AuditEntry`/`AuditSink` do MT-06 ou tipo análogo); guardrails de saída
  aplicam a redação do MT-06 antes de qualquer log; mudança de esquema correspondente
  registrada no `exchange-log` (`docs/interop/exchange-log.md`).

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
