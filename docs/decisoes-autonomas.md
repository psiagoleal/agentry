<!-- Caminho relativo: docs/decisoes-autonomas.md -->

# Log de decisões autônomas (loop de implementação)

Registro **append-only** de toda decisão tomada pelo agente durante a execução autônoma em
loop (`/loop /implementar-roadmap`) quando ele se depara com uma dúvida e escolhe a **opção
recomendada** sem parar para perguntar. Existe para que o mantenedor **revise depois** cada
escolha feita sozinho.

> Regra do loop: diante de uma dúvida com opção recomendada clara, o agente **segue a
> recomendada, registra aqui, e continua**. Diante de uma dúvida **sem** recomendação clara,
> ou de uma parada dura (dependência nova, repo irmão, afrouxar segurança), o agente **para e
> escala ao usuário** — ver `.claude/commands/implementar-roadmap.md`.

## Formato de cada entrada

```
### AAAA-MM-DD — <ticket/fase> — <título curto da decisão>
- **Contexto:** onde/por quê a decisão apareceu.
- **Opções consideradas:** (a) …; (b) …; (c) …
- **Escolha (recomendada):** <a opção adotada>.
- **Justificativa:** por que é a mais alinhada ao objetivo do agentry, à segurança/
  governança/confidencialidade (ADR-0002 fail-closed) e a um design mínimo (não
  over-engineered).
- **Commit:** `<hash>`.
```

## Entradas (mais recente no topo)

<!-- O loop acrescenta entradas acima desta linha. Nenhuma decisão registrada ainda. -->
