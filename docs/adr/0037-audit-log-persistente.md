<!-- Caminho relativo: docs/adr/0037-audit-log-persistente.md -->

# ADR 0037: Audit log persistente (`.agentry/audit.log`)

- **Status:** Accepted
- **Data:** 2026-07-24
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** persistência, auditoria, compliance

## Contexto

Hoje o audit log (`AuditSink`/`GuardrailAuditSink`, `crates/core/src/transport/mod.rs`) só
existe como `StderrAuditSink` (`crates/cli/src/main.rs`) — cada entrada (chamada de rede
auditada pelo `Transport` único, ADR-0002; decisão do Guardrail Gate, ADR-0007) vira uma
linha de `eprintln!` e desaparece assim que o terminal rola ou o processo termina. Pra um
projeto cujo objetivo declarado é homologação corporativa ([[project-purpose-and-origin]]),
um *log* de auditoria que não sobrevive ao fechar do terminal não cumpre função de auditoria
de verdade. `.agentry/audit.log` já está **reservado** desde a ADR-0017 ("log estruturado de
egresso persistido, se/quando decidido substituir/complementar o `StderrAuditSink` atual") —
esta é essa decisão.

Diferente da ADR-0036 (sessão), isto **não** é uma exceção a nenhuma decisão anterior: um log
de auditoria — por definição um registro do que o *agentry* fez, não do conteúdo da conversa
em si — é exatamente o tipo de persistência que o objetivo de compliance do projeto **pede**,
não o contrário.

## Decisão

### Complementa, não substitui, o `stderr`

`AuditEntry`/`GuardrailAuditEntry` continuam saindo em `stderr` (visibilidade em tempo real
durante uso interativo) **e** são acrescentadas a `.agentry/audit.log`, uma por linha, formato
**JSON Lines** (uma entrada JSON completa por linha — `serde_json`, ambos os tipos já
`Serialize`) — não o `Display` compacto do `stderr`, que é pra leitura humana ao vivo, não
para consumo por ferramenta depois. Sempre ativo, sem *flag* pra desligar (é auditoria, não
uma preferência de UX) — mesmo espírito de "sempre auditado" já estabelecido pelo Guardrail
Gate (ADR-0007).

### Escrita append-only, uma linha por vez

Cada `record()` abre o arquivo em modo *append* (`OpenOptions::append(true).create(true)`),
escreve uma linha JSON, fecha — sem manter um *file handle* aberto entre chamadas (mais
simples, e correto mesmo se duas invocações do `agentry` rodarem em paralelo no mesmo
projeto, já que `append` é atômico a nível de SO para escritas pequenas). Falha ao escrever no
arquivo é logada em `stderr` (`eprintln!`) mas **nunca** interrompe a operação em andamento —
auditoria degradada (só `stderr`) é preferível a travar o `agentry` por um problema de disco.

### `FileAuditSink` — novo sink, combinado com o existente

Novo `FileAuditSink` (implementa `AuditSink`/`GuardrailAuditSink`) escrevendo em
`.agentry/audit.log`; `main.rs` passa a instanciar um sink combinado
(`SinkDuplo`/`SinksCombinados`, novo, simples — chama `record()` nos dois sinks em sequência)
envolvendo `StderrAuditSink` + `FileAuditSink`. **Não** reaproveita `ColetorDuplo`
(`crates/core/src/session/mod.rs`) — aquele é privado ao módulo e serve a um propósito
diferente (coletar `GuardrailAuditEntry` para anexar a `SessionOutcome`, não combinar sinks
em geral); o combinador novo é uma struct própria, pequena o bastante para não valer a pena
compartilhar código com ele.

## Consequências

- **Positivas:** trilha de auditoria sobrevive ao fechar do terminal; formato JSONL é trivial
  de consumir por qualquer ferramenta de análise depois (`jq`, importação em planilha, etc.).
- **Negativas/riscos aceitos:** mais um arquivo que cresce indefinidamente em `.agentry/` —
  sem rotação/limite nesta ADR (extensão futura se o tamanho incomodar na prática); mesma
  superfície de "conteúdo sensível em disco" que a ADR-0036 já levanta para sessão, mas menor
  em escopo (`AuditEntry` registra metadados de chamada — URL/classe de egresso/status —, não
  o corpo da conversa).
- **Fora de escopo:** rotação/compactação do arquivo; UI pra consultar o log (fica pro
  usuário abrir com um editor/`jq` por enquanto).

## Diretriz de Conformidade de Código

- **Obrigatório:** toda entrada que hoje vai para `StderrAuditSink` também vai para
  `FileAuditSink` — nenhum caminho de auditoria (`Transport`, Guardrail Gate) pode escrever só
  num dos dois.
- **Proibido:** `.agentry/audit.log` sair da raiz resolvida pela ADR-0017, ou ser sincronizado
  para fora da máquina local por qualquer canal (mesma regra da ADR-0017, sem exceção nova).
  Falha de escrita no arquivo nunca pode derrubar uma chamada de rede que já passou pela
  checagem de egresso — auditoria é *best-effort* em disco, não um novo ponto de falha.
