<!-- Caminho relativo: docs/adr/0007-guardrails-configuraveis-de-conteudo.md -->

# ADR 0007: Guardrails configuráveis de conteúdo (gate distinto do Tool Registry)

- **Status:** Accepted
- **Data:** 2026-07-07 (emendado em 2026-07-13 — ver nota abaixo)
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** segurança, configuração, guardrails, interop

> **Nota de revisão (2026-07-13):** esta ADR ficou sem implementação nem micro-ticket desde
> que foi escrita. Antes de abrir o roadmap de implementação, as decisões deixadas em aberto
> no texto original ("palavra-chave/regex simples") foram fechadas na seção **"Schema mínimo
> e mecânica do Guardrail Gate"**, adicionada abaixo. A moderação semântica por modelo, já
> adiada aqui para "v0.2, se necessária, sob novo ADR", foi de fato coberta por um ADR
> separado — a **ADR-0015 (Reviewer)**, que audita `guardrail-compliance` como um dos seus
> tipos de auditoria. As duas ADRs são complementares, não sobrepostas: a ADR-0015 audita
> *semanticamente*, pós-resposta, via chamada de modelo; esta ADR aplica correspondência
> *determinística* (substring/palavra-chave), nos dois lados de uma chamada (entrada e
> saída), sem nenhuma chamada de modelo adicional.

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

## Schema mínimo e mecânica do Guardrail Gate (emenda 2026-07-13)

1. **Mecanismo de correspondência: substring/palavra-chave literal, sem `regex`.** Mesma
   filosofia já usada em `tools/fs.rs` (`fs_search`, "substring literal, sem regex") e em
   `tools/shell.rs` (`ShellPolicy`) — determinismo simples, sem dependência nova (evita abrir
   a ADR-0004 para vetar a crate `regex` sem necessidade real hoje). Comparação
   *case-insensitive*. Regex de verdade fica para uma extensão futura, só se houver demanda
   concreta que substring não cubra.
2. **Schema mínimo** (primeira fatia, mesmo padrão de fatia da ADR-0018):
   ```json
   {
     "guardrails": {
       "input": [
         { "id": "bloqueia-senha-explicita", "match": "senha:", "action": "block" }
       ],
       "output": [
         { "id": "mascara-hostname-interno", "match": "internal.corp", "action": "redact" }
       ]
     }
   }
   ```
   `input` é checado sobre a mensagem de usuário mais recente, **antes** de qualquer chamada
   ao provider; `output` é checado sobre a resposta final do turno, **antes** do Reviewer
   (ADR-0015) rodar e antes de a resposta ser devolvida ao chamador. Escopo deliberadamente
   restrito a texto simples de mensagens de usuário/assistente — argumentos e resultados de
   tool-call ficam fora (já cobertos pelo gate de permissão de tools, MT-11). Ausência da
   chave `guardrails` ⇒ nenhuma regra ⇒ nada é checado (mesmo *default* harmless-absence do
   pacote ADR-0010..0013/ADR-0018).
3. **Merge por camada, reforço monotônico:** regras são unidas por `id` entre camadas
   (perfil < arquivo < ambiente, mesma ordem do `Config::resolve`); uma regra nova (`id`
   inédito) é sempre adicionada; se duas camadas declaram o **mesmo** `id` com ações
   diferentes, vence a mais severa (`block` > `redact`) — nunca a mais permissiva, mesmo
   princípio de `Permissions::union` (só cresce), aqui generalizado para uma ordem de
   severidade de duas ações em vez de duas listas.
4. **Efeito de `block` (entrada ou saída): substitui por aviso fixo, sessão continua
   normalmente.** A mensagem flagrada (do usuário, na entrada; do assistente, na saída) é
   substituída por um aviso de sistema identificando a regra (`id`); a sessão devolve
   `StopReason::Done` normalmente, **sem** erro exposto ao chamador e **sem** retentativa.
   Bloqueio na **entrada** significa que o provider **nunca é chamado** para aquele turno —
   zero egresso, ainda mais estrito que o bloqueio na saída (que já envolveu uma chamada).
   Bloqueio na saída acontece **antes** do Reviewer — não faz sentido auditar semanticamente
   um conteúdo que acabou de ser substituído.
5. **Efeito de `redact`:** os trechos casados são mascarados no texto (mesmo *placeholder*
   `[REDACTED]` de `egress::redact`, por consistência visual) e o turno segue seu curso
   normal — na entrada, o texto já mascarado é o que vai para o provider; na saída, o texto
   já mascarado é o que segue para o Reviewer e para o chamador.
6. **Auditoria via tipo análogo, não `AuditEntry` literal.** `AuditEntry`/`AuditSink` (MT-06)
   carregam `profile`/`egress_class` — campos de egresso de rede que não fazem sentido para
   uma checagem de conteúdo, e que a `Session` nem possui hoje (vivem em `Config`/`Transport`,
   camada acima). Em vez de forçar esses campos ou plumb-ar profile/classe até a `Session` só
   para isso, um par novo e enxuto — `GuardrailAuditEntry`/`GuardrailAuditSink` — espelha a
   mesma forma (uma *trait* com `record`, uma *struct* de entrada), carregando só
   `direction`/`rule_id`/`action`/`task`. **Nunca o texto casado nem o conteúdo da mensagem**
   entra na entrada de auditoria — não por aplicar a redação do MT-06 em cima do trecho
   logado, e sim por **nunca logar o trecho em primeiro lugar** (mais simples e
   estruturalmente mais seguro que "redigir antes de logar": nada sensível chega perto do
   log). Só uma regra que efetivamente agiu (`block`/`redact`) gera entrada — uma correspondência
   ausente não é evento de auditoria, mesmo espírito do módulo de egresso (que só audita
   tentativas de fato, não a ausência delas).

## Consequências

- **Impacto positivo:** superfície configurável e auditável para uma classe de risco hoje
  descoberta; reaproveita os padrões de camada (MT-04) e de audit log (MT-06) já existentes;
  não inventa política — a fonte de regras continua sendo o perfil.
- **Impacto negativo:** correspondência por palavra-chave/regex tem falsos positivos e
  negativos; não substitui moderação semântica real.
- **Trade-offs aceitos:** determinismo e auditabilidade agora, em vez de sofisticação de
  detecção — moderação por modelo fica para uma v0.2, se necessária, sob novo ADR.

## Diretriz de Conformidade de Código

- **Proibido:** uma camada mais específica (projeto/ambiente) afrouxar uma regra de
  guardrail herdada de uma camada menos específica, mesmo por reuso do mesmo `id` com ação
  mais fraca (vence sempre a mais severa); qualquer guardrail que dependa de serviço externo
  de moderação sem passar pelo transporte único e pela allowlist (ADR-0002); inventar regra
  de guardrail fora do `settings-schema` do `profiles`; adicionar a crate `regex` (ou
  qualquer motor de correspondência além de substring/palavra-chave) sem uma ADR própria
  vetando a dependência (ADR-0004); logar o texto casado ou o conteúdo integral da mensagem
  numa entrada de auditoria do Guardrail Gate (a entrada carrega só
  `direction`/`rule_id`/`action`/`task`).
- **Obrigatório:** toda regra que efetivamente agir (`block` ou `redact`) gera uma
  `GuardrailAuditEntry` via `GuardrailAuditSink`; checagem de entrada roda **antes** de
  qualquer chamada ao provider (bloqueio na entrada nunca toca a rede); checagem de saída
  roda **antes** do Reviewer (ADR-0015); mudança de esquema correspondente registrada no
  `exchange-log` (`docs/interop/exchange-log.md`).

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
