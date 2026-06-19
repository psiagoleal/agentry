---
name: handoff-updater
description: >-
  Mantém o documento de transição docs/CURRENT-STATE.md (handoff) como fonte
  central de sincronização entre turnos e desenvolvedores, atualizado a cada
  commit com hash, metas cumpridas e impedimentos. Aciona após concluir um
  micro-ticket, antes de encerrar a sessão, ao fazer commit, ou quando o usuário
  perguntar "onde paramos".
---

# handoff-updater — Estado corrente e handoff

O progresso de tarefas não deve depender de relatórios verbais nem de re-ingestão de
logs. O arquivo `docs/CURRENT-STATE.md` (também chamado `HANDOFF.md`) é a **fonte central
de sincronização**: atualizado de forma mandatória a cada commit na branch de trabalho,
ele permite que o próximo turno (humano ou agente) retome o trabalho sem reconstruir
contexto a partir do histórico.

## Quando atualizar

- A **cada commit** na branch de trabalho.
- Ao **concluir um micro-ticket**.
- Antes de **encerrar a sessão** ou pausar o trabalho.

## O que registrar

Use `templates/CURRENT-STATE.template.md`. Cada entrada deve conter:

- **Hash do commit** correspondente à alteração (curto, 7+ chars).
- **Metas cumpridas** no turno (referenciando os micro-tickets MT-n).
- **Quadro de impedimentos** técnicos abertos.
- **Próximo passo** sugerido para quem assumir.

## Princípios

- Escreva para quem **não estava na sessão**: sem jargão de contexto perdido.
- Não cole segredos nem trechos de log sensíveis (ver skill `secrets-guard`).
- Mantenha conciso: o handoff substitui a re-leitura do histórico, não o duplica.

## Definição de pronto da skill

- [ ] `docs/CURRENT-STATE.md` reflete o último commit (hash confere).
- [ ] Metas, impedimentos e próximo passo estão preenchidos.
- [ ] Nenhum segredo ou log sensível foi incluído.
