<!-- Caminho relativo: docs/adr/0033-teto-de-turnos-consecutivos-com-tool-call.md -->

# ADR 0033: Teto de turnos consecutivos com tool-call, independente do orçamento de tokens

- **Status:** Proposed
- **Data:** 2026-07-17
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** confiabilidade, agent-loop, sessão

## Contexto

Teste manual de usabilidade da TUI (rodada 4, `docs/CURRENT-STATE.md`) expôs um cenário sem
rede de segurança: depois de uma pergunta via `ask_user`, o modelo (Ollama local) continuou
mandando mensagens tipo "Processo concluído" repetidamente, aparentemente ignorando a resposta
do usuário — inclusive quando o usuário cancelava a pergunta (`Esc`).

Investigação (`crates/core/src/session/mod.rs`, `Session::run`/`run_streaming`) confirmou que o
loop do agente só tem **dois** critérios de parada:

1. `StopReason::Done` — o modelo parou de chamar tools (resposta final).
2. `StopReason::BudgetExceeded` — o orçamento de tokens (`TokenBudget`, já existente) acabou.

Não existe nenhum teto **independente de tokens** para turnos/tool-calls consecutivos. Um
modelo fraco ou mal calibrado (comum em provedores locais menores, ex. Ollama com um modelo
pequeno) pode ficar chamando tools indefinidamente — na prática, isso só para quando o
orçamento de tokens inteiro da sessão é consumido, o que pode demorar bastante e desperdiçar
uso real (tempo, quota de API, paciência do usuário) antes do usuário conseguir interromper.

Este comportamento afeta igualmente os três modos de exposição da CLI (REPL, one-shot, TUI) —
não é específico da TUI, então a correção pertence ao núcleo (`crates/core/src/session/mod.rs`),
não a um dos adaptadores.

## Decisão

`Session` ganha um novo campo `max_tool_turns: u32` — teto de turnos **consecutivos** em que o
modelo chamou pelo menos uma tool, contado a partir da última mensagem de usuário (reseta a
cada novo `push_user_message`, nunca acumula entre pedidos distintos do usuário). *Default*
generoso (25) quando não configurado explicitamente via builder — suficiente para tarefas
legítimas longas (múltiplas edições de arquivo, buscas, subagentes), mas finito.

Ao atingir o teto, o loop para com um novo `StopReason::MaxTurnsExceeded` — **nunca** um erro
fatal, pânico, ou truncamento silencioso do histórico. Mesmo padrão de `StopReason::BudgetExceeded`:
o histórico e o uso acumulado (`usage_total`) são preservados; o controle volta para o
chamador (REPL/one-shot/TUI), que reporta uma mensagem clara e permite ao usuário continuar a
conversa enviando outra mensagem.

Sem configuração via `agentry.settings.json` nesta versão (YAGNI) — só builder
(`Session::with_max_tool_turns`), com o *default* de 25 já cobrindo o caso comum.

## Consequências

- **Impacto positivo:** rede de segurança contra loop de modelo fraco/mal calibrado, sem
  depender só do orçamento de tokens (que pode ser grande o bastante para o loop parecer
  "travado" por muito tempo antes de parar). Nenhuma mudança de comportamento para sessões
  normais (a maioria das tarefas fica muito abaixo de 25 turnos consecutivos com tool-call).
- **Impacto negativo:** uma tarefa legítima e incomum que precise de mais de 25 turnos
  consecutivos de tool-call sem uma resposta de texto intermediária seria interrompida —
  aceitável dado que o usuário pode simplesmente continuar a conversa (não é um erro
  destrutivo, só uma pausa).
- **Trade-offs aceitos:** teto fixo/builder-only por ora, não configurável por perfil — extensão
  futura simples (mesmo padrão de outras flags de `Config`) se a necessidade aparecer.

## Diretriz de Conformidade de Código

- **Proibido:** qualquer código que trate `StopReason::MaxTurnsExceeded` como erro
  fatal/pânico, ou que descarte o histórico/uso acumulado da sessão ao atingir o teto.
- **Obrigatório:** os três pontos de exposição (REPL, one-shot, TUI) tratam
  `StopReason::MaxTurnsExceeded` com uma mensagem clara para o usuário, permitindo a sessão
  continuar; o contador de turnos consecutivos com tool-call reseta a cada nova mensagem de
  usuário, nunca acumulando de turnos anteriores já concluídos com `StopReason::Done`.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
