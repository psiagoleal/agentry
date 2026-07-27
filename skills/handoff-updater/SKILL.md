---
name: handoff-updater
description: >-
  Mantém o documento de transição docs/CURRENT-STATE.md (handoff) como fonte
  central de sincronização entre turnos e desenvolvedores, atualizado a cada
  commit com hash, metas cumpridas e impedimentos. Mantém o handoff enxuto
  arquivando rodadas antigas em docs/handoff-arquivo.md. Aciona após concluir um
  micro-ticket, antes de encerrar a sessão, ao fazer commit, ou quando o usuário
  perguntar "onde paramos".
---

# handoff-updater — Estado corrente e handoff

O progresso de tarefas não deve depender de relatórios verbais nem de re-ingestão de
logs. O arquivo `docs/CURRENT-STATE.md` (também chamado `HANDOFF.md`) é a **fonte central
de sincronização**: atualizado de forma mandatória a cada commit na branch de trabalho,
ele permite que o próximo turno (humano ou agente) retome o trabalho sem reconstruir
contexto a partir do histórico.

## Dois arquivos, propósitos diferentes

Esta separação é **obrigatória** — não é uma otimização opcional:

| Arquivo | Contém | Quando é lido |
|---|---|---|
| `docs/CURRENT-STATE.md` | **Só o estado corrente**: último turno, metas dele, o que está em andamento, impedimentos abertos, próximo passo. | **Sempre**, no início de qualquer sessão. |
| `docs/handoff-arquivo.md` | Todo o histórico: rodadas encerradas, fases fechadas, tabela de commits. *Append-only.* | **Sob demanda**, por busca, para entender *por que* algo foi decidido. |

O handoff vivo aponta para o arquivo logo no cabeçalho; o arquivo aponta de volta.

### Por que a separação existe

Um handoff que cresce indefinidamente **deixa de ser um handoff**. Quando o arquivo não cabe
mais na janela de contexto de quem assume, ele passa a custar exatamente o que deveria
economizar: a re-leitura do histórico. O ponto da skill é substituir essa releitura, não
duplicá-la em outro formato.

> Caso real (`agentry`, 2026-07): o `CURRENT-STATE.md` chegou a ~2550 linhas / ~114k tokens
> antes de ser dividido — um agente novo não conseguia lê-lo por inteiro. O conteúdo estava
> correto; o formato é que tinha falhado.

### Teto de tamanho

**Se `CURRENT-STATE.md` passar de ~150 linhas, arquive antes de escrever a entrada nova.**
Não deixe acumular "só mais uma rodada": a próxima é sempre a que estoura.

## Quando atualizar

- A **cada commit** na branch de trabalho.
- Ao **concluir um micro-ticket**.
- Antes de **encerrar a sessão** ou pausar o trabalho.

## Como arquivar uma rodada

Ao começar uma rodada nova, a anterior deixa de ser "o estado corrente":

1. **Recorte** a seção da rodada anterior de `CURRENT-STATE.md` (não resuma nem reescreva —
   mova o texto como está; ele já foi revisado quando foi escrito).
2. **Cole** em `docs/handoff-arquivo.md`, preservando a ordem cronológica e o índice do topo.
3. **Atualize** o "Último turno" de `CURRENT-STATE.md` com o commit e as metas da rodada nova.
4. Mantenha em `CURRENT-STATE.md` apenas o que **continua verdadeiro**: impedimentos ainda
   abertos, trabalho ainda em andamento, próximo passo ainda válido.

Um impedimento resolvido **não** vai para o arquivo como impedimento: some do handoff vivo e
sua resolução fica registrada na rodada que o resolveu.

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
- **Registre a causa raiz, não só o sintoma.** "Corrigido bug X" não ajuda quem assume;
  "X acontecia porque Y, confirmado por Z" evita que o próximo turno refaça a investigação.
- **Dívida apontada é dívida rastreada:** se você identificou um problema mas não o resolveu,
  ele vira item de impedimento ou de próximo passo — nunca só um comentário no meio do texto.

## Definição de pronto da skill

- [ ] `docs/CURRENT-STATE.md` reflete o último commit (hash confere).
- [ ] `docs/CURRENT-STATE.md` tem menos de ~150 linhas.
- [ ] Rodadas encerradas foram movidas para `docs/handoff-arquivo.md`, com o link entre os
      dois arquivos intacto nos dois sentidos.
- [ ] Metas, impedimentos e próximo passo estão preenchidos.
- [ ] Nenhum segredo ou log sensível foi incluído.
