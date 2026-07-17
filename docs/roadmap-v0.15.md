<!-- Caminho relativo: docs/roadmap-v0.15.md -->

# Roadmap v0.15 — Micro-tickets

Rodada 4 de teste manual de usabilidade da TUI (`--tui`, Windows+LiteLLM e Linux+Ollama) —
achados registrados em `docs/CURRENT-STATE.md` §"Nota fora do loop"/"Rodada 4". Não corresponde
a uma "Fase N" do `docs/roadmap-longo-prazo.md` (esse roadmap segue esgotado/bloqueado desde a
Fase 20, ver ADR-0004(c)) — é trabalho ad-hoc de correção pós-teste manual, mesmo padrão das
rodadas 1-3 já registradas, só que grande o bastante para merecer micro-tickets formais em vez
de só um commit solto.

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`),
skill `micro-ticket-planner` para granularidade, smoke-test real via `tmux` para toda mudança
observável de TUI (mesma técnica já usada nas rodadas 1-3). **Nenhuma dependência nova** —
tudo com `ratatui`/`std` já presentes.

---

## Fase A — Correções de UX da TUI

### MT-96: Ancorar o histórico no fim mesmo quando a conversa cabe na tela inteira
- **Objetivo:** quando a conversa (já quebrada em linhas) cabe inteira na área visível do
  histórico, preencher com linhas em branco **no início** (não no fim) antes de renderizar —
  a última linha real sempre cai na última linha visível da caixa, espaço vazio (se houver)
  fica em cima. Mesmo comportamento de qualquer chat (Slack/Discord/iMessage). Achado ao vivo
  via `tmux` (pane 100×40, 1 mensagem): sem este fix, a mensagem aparece no topo com ~25 linhas
  em branco embaixo.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs` (função `draw`).
- **Critério de aceite:** testes — conversa mais curta que a altura visível recebe padding no
  início, última linha real cai na posição correta; conversa mais longa que a altura visível
  não recebe nenhum padding (comportamento existente inalterado); rolar para cima numa
  conversa curta (com padding) não quebra a matemática de scroll. Smoke-test real via `tmux`.
- **Fora de escopo:** qualquer mudança de largura/wrap (já corrigido na rodada 2).
- **Depende de:** nenhum.

### MT-97: Caixa de entrada com wrap, altura dinâmica (com teto) e cursor real do terminal
- **Objetivo:** a caixa de entrada reaproveita `quebrar_em_linhas` (já existe, mesma função do
  histórico); a altura da caixa passa a ser calculada a cada frame a partir do número de linhas
  quebradas + 2 (borda), com um teto — além do teto, altura para de crescer e a caixa mostra
  sempre a cauda do texto (cursor sempre no fim, já que a edição hoje só existe por
  `push`/`pop` no fim da `String`, sem navegação de cursor no meio). O cursor real do terminal
  (`Frame::set_cursor_position`, já disponível no `ratatui` 0.30.2 em uso) é posicionado ali,
  só quando nenhum modal (seletor/solicitação) está aberto.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** testes da função pura de altura (vazio → mínimo; curto → mínimo;
  longo → cresce; enorme → satura no teto); smoke-test real via `tmux`.
- **Fora de escopo:** navegação de cursor no meio do texto (setas esquerda/direita, Home/End).
- **Depende de:** `quebrar_em_linhas` (já existe).

### MT-98: Seleção por seta nas opções do `ask_user` + sentinela de cancelamento no Esc
- **Objetivo:** quando `ask_user` tem `options` não-vazio, `Up`/`Down` movem um destaque entre
  as opções; `Enter` com a caixa de resposta vazia envia o texto exato da opção destacada
  (elimina a ambiguidade "1"/"2" pro modelo); `Enter` com texto digitado envia o texto livre,
  ignorando a seleção. `Esc` passa a mandar uma string-sentinela descritiva em vez de
  `String::new()` (hoje indistinguível de "respondeu vazio"). Nenhuma mudança de tipo/trait —
  `Prompter::ask` já devolve `String` simples.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** testes — mover seleção satura nos limites; `Enter` com campo vazio +
  opção destacada envia o texto exato; `Enter` com texto digitado envia o texto digitado; `Esc`
  envia a sentinela, nunca string vazia; pergunta sem `options` continua puro texto livre.
  Smoke-test real via `tmux`.
- **Fora de escopo:** paginação/scroll dentro do modal se a lista de opções exceder a área.
- **Depende de:** nenhum.

### MT-99: Nota de documentação — comportamento de `ask_user`/tool-calling depende do modelo
- **Objetivo:** `docs/usuario/uso.md` ganha uma nota explicando que uso inconsistente/excessivo
  de `ask_user` (ou tool-calling em geral) é variação do modelo por trás do provider, não do
  `agentry` — `temperature`/`top_p` não são fixados por padrão; sugere fixar via
  `/temperature`/config para respostas mais determinísticas.
- **Arquivos no escopo:** `docs/usuario/uso.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo.
- **Fora de escopo:** qualquer mudança de código/comportamento padrão.
- **Depende de:** nenhum.

## Teto de turnos consecutivos com tool-call (ADR-0033)

### MT-100: ADR-0033 — teto configurável de turnos consecutivos com tool-call
- **Objetivo:** `docs/adr/0033-teto-de-turnos-consecutivos-com-tool-call.md` (`Proposed`):
  `Session` ganha um teto de turnos consecutivos com tool-call, independente do orçamento de
  tokens — rede de segurança contra um modelo fraco em loop (achado desta rodada: "Processo
  concluído" repetido, ignorando a resposta do usuário).
- **Arquivos no escopo:** `docs/adr/0033-teto-de-turnos-consecutivos-com-tool-call.md`,
  `docs/adr/README.md`.
- **Critério de aceite:** ADR segue o template (`skill adr-writer`).
- **Depende de:** nenhum.

### MT-101: Implementação do teto em `Session`
- **Objetivo:** `crates/core/src/session/mod.rs` — novo campo `max_tool_turns: u32` (builder,
  *default* generoso caso não configurado); `run`/`run_streaming` contam turnos consecutivos
  com tool-call e param com `StopReason::MaxTurnsExceeded` ao atingir o teto, preservando
  histórico e uso acumulado.
- **Arquivos no escopo:** `crates/core/src/session/mod.rs`.
- **Critério de aceite:** testes — sessão com mais tool-calls enfileirados que o teto para
  exatamente nele, com o `StopReason` certo; sessão abaixo do teto termina normalmente;
  contador reseta corretamente entre mensagens distintas.
- **Fora de escopo:** tornar o teto configurável via `agentry.settings.json` (YAGNI por ora).
- **Depende de:** MT-100.

### MT-102: Exposição do novo `StopReason` nos 3 pontos de saída + documentação
- **Objetivo:** REPL, one-shot e TUI tratam `StopReason::MaxTurnsExceeded` com mensagem clara,
  nunca pânico/erro genérico. `docs/usuario/uso.md` ganha nota curta. ADR-0033 → `Accepted`.
- **Arquivos no escopo:** `crates/cli/src/repl.rs`, `crates/cli/src/main.rs`/
  `crates/cli/src/streaming.rs`, `crates/cli/src/tui/mod.rs`, `docs/usuario/uso.md`,
  `docs/adr/0033-*.md`.
- **Critério de aceite:** smoke-test real (mock HTTP roteirizado pra sempre devolver
  tool-call) confirmando que os 3 modos param no teto com mensagem clara.
- **Depende de:** MT-101.

## Fase B — Pesquisa de referência de UX/TUI (próxima rodada de trabalho)

### MT-103: Pesquisa profunda + mapeamento de ferramentas semelhantes
- **Objetivo:** produzir `docs/pesquisa-tui-referencias.md` comparando Claude Code CLI,
  OpenCode, Aider, Gemini CLI e Codex CLI nos eixos: *keybindings*, seleção de modelo/provider,
  UX de confirmação/permissão, revisão de diff, *streaming*/formatação de Markdown,
  paginação/scroll, *onboarding*, temas, *widget* de lista de tarefas, descoberta de atalhos.
  Termina com lista priorizada de oportunidades pro `agentry`.
- **Arquivos no escopo:** `docs/pesquisa-tui-referencias.md` (novo).
- **Critério de aceite:** documento cobre as 5 ferramentas nos eixos listados, fontes citadas;
  `mkdocs build --strict` limpo.
- **Fora de escopo:** qualquer implementação de código.
- **Depende de:** nenhum (mas fica para a próxima rodada, não espremido nesta).

## Fase C+ — Redesenho da TUI informado pela pesquisa

Stub — tickets detalhados só depois que o MT-103 estiver concluído e revisado, mesma
disciplina do resto do roadmap de longo prazo para fases futuras.

---

## Sequência crítica

```
MT-96 → MT-97 → MT-98 → MT-99  (Fase A, independentes entre si — ordem por conveniência)
MT-100 → MT-101 → MT-102        (teto de turnos)
MT-103                          (Fase B — rodada seguinte)
```
