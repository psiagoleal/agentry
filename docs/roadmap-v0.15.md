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

### MT-96: Ancorar o histórico no fim mesmo quando a conversa cabe na tela inteira ✅ concluído (4a54ee5)
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

### MT-97: Caixa de entrada com wrap, altura dinâmica (com teto) e cursor real do terminal ✅ concluído (d697ba8)
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

### MT-98: Seleção por seta nas opções do `ask_user` + sentinela de cancelamento no Esc ✅ concluído (be02822)
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

### MT-99: Nota de documentação — comportamento de `ask_user`/tool-calling depende do modelo ✅ concluído (479c9d8)
- **Objetivo:** `docs/usuario/uso.md` ganha uma nota explicando que uso inconsistente/excessivo
  de `ask_user` (ou tool-calling em geral) é variação do modelo por trás do provider, não do
  `agentry` — `temperature`/`top_p` não são fixados por padrão; sugere fixar via
  `/temperature`/config para respostas mais determinísticas.
- **Arquivos no escopo:** `docs/usuario/uso.md`.
- **Critério de aceite:** `mkdocs build --strict` limpo.
- **Fora de escopo:** qualquer mudança de código/comportamento padrão.
- **Depende de:** nenhum.

## Teto de turnos consecutivos com tool-call (ADR-0033)

### MT-100: ADR-0033 — teto configurável de turnos consecutivos com tool-call ✅ concluído (c0110b7)
- **Objetivo:** `docs/adr/0033-teto-de-turnos-consecutivos-com-tool-call.md` (`Proposed`):
  `Session` ganha um teto de turnos consecutivos com tool-call, independente do orçamento de
  tokens — rede de segurança contra um modelo fraco em loop (achado desta rodada: "Processo
  concluído" repetido, ignorando a resposta do usuário).
- **Arquivos no escopo:** `docs/adr/0033-teto-de-turnos-consecutivos-com-tool-call.md`,
  `docs/adr/README.md`.
- **Critério de aceite:** ADR segue o template (`skill adr-writer`).
- **Depende de:** nenhum.

### MT-101: Implementação do teto em `Session` ✅ concluído (2842d12)
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

### MT-102: Exposição do novo `StopReason` nos 3 pontos de saída + documentação ✅ concluído (a9c3836) — fecha o teto de turnos, ADR-0033 → Accepted
- **Objetivo:** REPL, one-shot e TUI tratam `StopReason::MaxTurnsExceeded` com mensagem clara,
  nunca pânico/erro genérico. `docs/usuario/uso.md` ganha nota curta. ADR-0033 → `Accepted`.
- **Arquivos no escopo:** `crates/cli/src/repl.rs`, `crates/cli/src/main.rs`/
  `crates/cli/src/streaming.rs`, `crates/cli/src/tui/mod.rs`, `docs/usuario/uso.md`,
  `docs/adr/0033-*.md`.
- **Critério de aceite:** smoke-test real (mock HTTP roteirizado pra sempre devolver
  tool-call) confirmando que os 3 modos param no teto com mensagem clara.
- **Depende de:** MT-101.

## Fase B — Pesquisa de referência de UX/TUI (próxima rodada de trabalho)

### MT-103: Pesquisa profunda + mapeamento de ferramentas semelhantes ✅ concluído (0b25265) — fecha a Fase B
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

## Fase C — Primeiras melhorias de UX informadas pela pesquisa (ADR-0034)

Mantenedor revisou `docs/pesquisa-tui-referencias.md` e autorizou seguir, priorizando: (1)
usabilidade pelo usuário, (2) evitar confusão com modelos menos capazes, (3) produtividade com
segurança. Das 10 oportunidades da pesquisa, três atacadas nesta rodada — as demais 6 (arquivo
de *keybindings* do usuário, metadados no seletor de modelo, tema configurável, `Page Up`/
`Down`, editor externo, comando `/review`) ficam candidatas para uma rodada futura, não
descartadas. Decisão completa registrada no plano de implementação desta rodada.

### MT-104: ADR-0034 — tool `todo_write`, sem estado persistido no núcleo ✅ concluído (a479732)
- **Objetivo:** `docs/adr/0034-tool-todo-write-sem-estado-persistido.md` (`Proposed`): nova
  *tool* sem efeito colateral, semântica de substituição total por chamada, renderização só na
  TUI (reacumulação de fragmentos do lado da TUI em vez de mudar o contrato de `StreamEvent`).
- **Arquivos no escopo:** `docs/adr/0034-*.md`, `docs/adr/README.md`.
- **Critério de aceite:** ADR segue o template (`skill adr-writer`).
- **Depende de:** nenhum.

### MT-105: `TodoWriteTool` no núcleo ✅ concluído (e87baa6)
- **Objetivo:** `crates/core/src/tools/todo.rs` — schema (`items: [{content, status}]`),
  `execute()` só valida estruturalmente e devolve confirmação; nenhum efeito colateral, nenhum
  estado persistido.
- **Arquivos no escopo:** `crates/core/src/tools/todo.rs` (novo), `crates/core/src/tools/mod.rs`
  ou `lib.rs` conforme o padrão de `pub mod` já usado pelas demais *tools*.
- **Critério de aceite:** testes puros — item válido/lista vazia/status desconhecido (erro
  tratado); confirmação menciona o número de itens.
- **Depende de:** MT-104.

### MT-106: Registro da tool + documentação ✅ concluído (71029f6)
- **Objetivo:** `crates/cli/src/main.rs` registra `TodoWriteTool` (mesmo padrão de
  `SkillTool`/`AskUserTool`), antes de `register_subagent_tool` (entra na fiação dual-registry
  do MT-91 automaticamente). `docs/usuario/uso.md` ganha uma entrada na lista de *tools*.
- **Arquivos no escopo:** `crates/cli/src/main.rs`, `docs/usuario/uso.md`.
- **Critério de aceite:** teste de fiação (tool aparece registrada); `mkdocs build --strict`
  limpo.
- **Depende de:** MT-105.

### MT-107: Renderização do *checklist* na TUI ✅ concluído (c90ccee)
- **Objetivo:** `ChatState` ganha um mapa transiente (`id -> nome + argumentos acumulados`),
  populado por `ToolCallStart`/`ToolCallDelta`; ao `MessageEnd`, `todo_write` com JSON válido
  vira um bloco de *checklist* formatado (`[x]`/`[~]`/`[ ]`) anexado ao turno; JSON inválido é
  ignorado silenciosamente (sem pânico/erro exibido).
- **Arquivos no escopo:** `crates/cli/src/tui/chat.rs`.
- **Critério de aceite:** testes — JSON válido gera o *checklist* certo; JSON inválido não
  altera o comportamento existente (só o marcador genérico permanece); mapa reseta a cada
  `MessageEnd`. *Smoke-test* real via `tmux` + mock HTTP chamando `todo_write` de verdade.
- **Depende de:** MT-105.

### MT-108: Blocos de código cercado no histórico ✅ concluído (ebe7394)
- **Objetivo:** máquina de estados linha-a-linha (dentro/fora de um bloco ` ``` `) em
  `montar_linhas_do_historico`; linhas dentro do bloco continuam quebrando via
  `quebrar_em_linhas` normalmente, só ganham um estilo distinto.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** testes — mensagem com bloco cercado tem as linhas internas
  estilizadas diferente do texto normal; bloco não fechado (ainda em *streaming*) não quebra
  nada. *Smoke-test* real via `tmux`.
- **Depende de:** nenhum.

### MT-109: Negrito e código inline no histórico ✅ concluído (649ae8a)
- **Objetivo:** `tokenizar_enfase` (linha a linha, marcadores só valem se **fechados** na mesma
  linha — resolve o caso de *streaming* com `**`/`` ` `` ainda sem par) + nova
  `quebrar_em_linhas_com_estilo` (*wrap* ciente de segmento) + `montar_linhas_do_historico`
  monta `Line`s com múltiplos `Span`s. Detecção do marcador `⚙` continua no texto bruto antes
  de tokenizar.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** testes — negrito/código isolados, mistos numa linha só, marcador não
  fechado permanece literal, negrito que atravessa um limite de quebra de linha preserva o
  estilo nos dois pedaços. *Smoke-test* real via `tmux`.
- **Fora de escopo:** cabeçalhos, listas, links, itálico, tabelas.
- **Depende de:** nenhum (independente do MT-108).

### MT-110: Painel de ajuda (`?`) + comando `/help` ✅ concluído (cfd8a8d)
- **Objetivo:** `?` com a caixa de entrada vazia abre um painel de tela cheia (tabela de
  *keybindings* + lista de comandos `/` com descrição de uma linha, incluindo o aviso sobre
  `/model`/`/init`); `?` com texto já digitado continua digitando normalmente. `/help` em
  `processar_comando_de_texto` devolve o mesmo conteúdo como mensagem de sistema — fonte única.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`, possivelmente `crates/cli/src/tui/keybind.rs`.
- **Critério de aceite:** testes — `?` com campo vazio abre o painel, com campo preenchido
  digita `?` normalmente; `/help` e o painel mostram o mesmo texto. *Smoke-test* real via
  `tmux`.
- **Depende de:** nenhum.

---

## Fase D — logo de abertura em halfblock/truecolor

Pedido ad-hoc do mantenedor (fora do escopo original da Fase C, mesma disciplina de
ticket/DoD/*smoke-test*): substituir o robô ASCII simples da tela de abertura por uma
releitura em cor de verdade do logo oficial do projeto (`assets/logo/agentry-logo-fonte.png`).

### MT-111: Logo de abertura em halfblock/truecolor com fallback ASCII ✅ concluído (af001bd)
- **Objetivo:** ícone do logo oficial (chapéu + robô + terminal, sem o texto "AGENTRY"/subtítulo
  do arquivo original — ilegível nessa resolução, renderizado à parte como texto de terminal
  nítido) pré-processado *offline* (`assets/logo/gerar-logo-icone.py`, não roda como parte do
  `cargo build`) e embutido como asset binário (`crates/cli/assets/logo-icone.rgb`, via
  `include_bytes!`) — **nenhuma dependência nova**, sem decodificador de imagem em runtime.
  Renderizado como halfblock/truecolor (`▀` com `fg`=pixel de cima, `bg`=pixel de baixo, técnica
  de `chafa`/`viu`), com *fallback* para um robô em ASCII simples quando o terminal não anuncia
  suporte a 24 bits de cor (`COLORTERM=truecolor`/`24bit`) ou `NO_COLOR` está setado.
- **Arquivos no escopo:** novo `crates/cli/src/tui/logo.rs`, `crates/cli/assets/logo-icone.rgb`
  (novo), `assets/logo/` (novo, fonte + script gerador, fora da árvore do crate), `crates/cli/src/tui/mod.rs`
  (`LOGO_ABERTURA` removido, `Estado` ganha campo `logo` montado uma vez em `Estado::new`).
- **Critério de aceite:** testes — tamanho do asset bate com `LARGURA*ALTURA_PX*3`; núcleo puro
  da heurística de *truecolor* (`decide_truecolor`) testado sem mutar variável de ambiente
  (evita `unsafe` do Rust 1.82+); *fallback* ASCII nunca fica vazio. *Smoke-test* real via `tmux`
  com e sem `COLORTERM=truecolor` — confirmado visualmente (captura ANSI convertida em imagem)
  que o ícone colorido bate com o logo oficial e que o *fallback* ASCII aparece sem cor quando
  o sinal de *truecolor* está ausente.
- **Fora de escopo:** redimensionamento adaptativo ao tamanho do terminal (resolução fixa,
  44x30px/15 linhas); protocolos de imagem de verdade (Sixel/Kitty graphics protocol) — avaliados
  e descartados para este caso por serem menos portáveis que halfblock/truecolor e exigirem
  dependência nova (`ratatui-image` + `image`), sem caso de uso concreto além desta logo estática.
- **Depende de:** nenhum.

---

## Fase E — modal de confirmação sempre mostra o comando completo

Pedido ad-hoc do mantenedor: um teste real (pedido de criar pasta + CSV) mostrou que o modal
de confirmação de tool (`ask`) cortava o comando de shell além da largura do modal, sem
nenhum jeito de ver o resto.

### MT-112: Modal de confirmação sempre mostra o comando completo ✅ concluído (2715a3b)
- **Objetivo:** `argumentos: {json}` do modal de confirmação (`SolicitacaoAtiva::Confirmacao`)
  vira uma única `Line` sem *wrap* — `Paragraph` sem `.wrap()` clipa em vez de quebrar linha
  sozinho (mesmo achado do MT-97). `linhas_de_confirmacao` (nova, função pura) sempre quebra
  os argumentos via `quebrar_em_linhas`; o modal ganha rolagem própria
  (`Estado::scroll_confirmacao`, zerada a cada nova confirmação) para o caso de um comando tão
  longo que nem cabe na altura do modal já quebrado.
- **Arquivos no escopo:** `crates/cli/src/tui/mod.rs`.
- **Critério de aceite:** testes — comando mais longo que o modal aparece por inteiro,
  quebrado em várias linhas; modal com *diff* disponível não mostra o JSON bruto. *Smoke-test*
  real via `tmux` + mock HTTP com um comando de ~140 caracteres.
- **Depende de:** nenhum.

**Fora de escopo desta rodada (adiado, não descartado):** mostrar um preview do comando +
saída completa no **corpo da conversa** (não só no modal de confirmação), com
expandir/recolher — acompanhando o pedido, mas com duas partes de custo bem diferente: (a)
preview do comando reaproveita a mesma técnica de reacumulação de argumentos já usada pelo
`todo_write` (MT-107), sem mudança de arquitetura; (b) mostrar a **saída** da tool exigiria
expor `ToolResult` ao `on_event` da TUI, que hoje só recebe eventos do que o **modelo**
transmite (texto/tool-calls) — a execução da tool acontece dentro de
`Session::after_response`, sem nenhum gancho de evento. Isso é uma decisão de arquitetura de
verdade (nova variante de `StreamEvent` quebra *matches* exaustivos em `session/mod.rs` e
`tui/chat.rs`, mesmo trade-off já documentado no ADR-0034) — não decidida ainda, aguardando
definição do mecanismo de expandir/recolher (tecla vs. clique de mouse, que sacrifica seleção
nativa de texto no terminal).

---

## Sequência crítica

```
MT-96 → MT-97 → MT-98 → MT-99   (Fase A, independentes entre si — ordem por conveniência)
MT-100 → MT-101 → MT-102         (teto de turnos)
MT-103                           (Fase B)
MT-104 → MT-105 → MT-106/MT-107  (Fase C, tool de todo)
MT-108, MT-109                   (Fase C, Markdown — independentes entre si)
MT-110                           (Fase C, ajuda — independente)
MT-111                           (Fase D, logo — independente)
MT-112                           (Fase E, modal de confirmação — independente)
```
