---
name: micro-ticket-planner
description: >-
  Quebra histórias de usuário e demandas amplas em micro-tickets autocontidos
  que cabem em um único ciclo limpo de contexto do agente, evitando "ansiedade
  de contexto" e estouro de tokens. Aciona ao planejar sprint, refinar backlog,
  receber uma tarefa grande/ambígua, ou quando o usuário pedir para "quebrar",
  "planejar" ou "dividir" um trabalho.
---

# micro-ticket-planner — Planejamento por micro-tickets

Agentes confrontados com escopo amplo demais ou dependências ambíguas tendem a
interromper a execução precocemente ou ignorar restrições de segurança à medida que se
aproximam dos limites da janela de contexto ("ansiedade de contexto"). A mitigação é a
**granularidade fina**: cada tarefa cabe em um ciclo limpo de interação.

## Como quebrar (heurística)

1. **Uma saída verificável por ticket.** Se o ticket produz mais de um artefato testável
   independentemente, divida-o.
2. **Escopo de arquivos fechado.** Liste, no ticket, os arquivos que serão tocados. Se a
   lista é incerta ou cresce sem limite, o escopo ainda está amplo demais.
3. **Sem dependências ambíguas.** Se concluir o ticket exige uma decisão arquitetural
   ainda não tomada, primeiro registre um ADR (ver skill `adr-writer`).
4. **Cabe em um turno.** Estime se o agente consegue ler o contexto necessário, alterar e
   validar dentro de uma sessão sem podar histórico. Se não, divida.

## Formato de um micro-ticket

```markdown
### MT-<n>: <título imperativo curto>
- **Objetivo:** <resultado único e verificável>
- **Arquivos no escopo:** path/a.py, path/b.py
- **Critério de aceite:** <comando de teste/linter que deve passar>
- **Fora de escopo:** <o que explicitamente NÃO fazer aqui>
- **Depende de:** MT-<m> | ADR-<NNNN> | nenhum
```

## Conexão com os rituais ágeis (DoD)

Um micro-ticket só é "Concluído" quando:
- os scripts de teste/linter definidos no `AGENTS.md` passam;
- o `docs/CURRENT-STATE.md` foi atualizado (ver skill `handoff-updater`);
- a revisão humana de PR foi feita (ver skill `pr-review-guard`).

## Definição de pronto da skill

- [ ] Cada micro-ticket tem objetivo único, escopo de arquivos fechado e critério de aceite.
- [ ] Dependências e itens fora de escopo estão explícitos.
- [ ] Nenhum ticket exige decisão arquitetural não registrada em ADR.
