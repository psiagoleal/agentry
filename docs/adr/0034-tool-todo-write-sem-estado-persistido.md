<!-- Caminho relativo: docs/adr/0034-tool-todo-write-sem-estado-persistido.md -->

# ADR 0034: Tool `todo_write` — lista de tarefas explícita do agente, sem estado persistido no núcleo

- **Status:** Proposed
- **Data:** 2026-07-17
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** ferramentas, usabilidade, TUI

## Contexto

`docs/pesquisa-tui-referencias.md` (MT-103) comparou o `agentry` com 5 ferramentas
concorrentes e identificou o *widget*/*tool* de lista de tarefas como o gap mais consistente
entre elas — Claude Code (`TodoWrite`/`Task*`), OpenCode (`todo-item`) e Gemini CLI (`Ctrl+T`,
`app.showFullTodos`) têm algum mecanismo equivalente; Aider e Codex CLI não. O mantenedor pediu
que a próxima rodada de melhorias de UX priorizasse usabilidade, redução de confusão em
modelos menos capazes, e produtividade com segurança — uma lista de tarefas explícita atende
aos três: dá ao usuário visibilidade do progresso numa tarefa de vários passos, e o próprio ato
de manter uma lista estruturada tende a ajudar modelos mais fracos a não perder o fio (achado
direto da rodada 4 de teste manual: um modelo local via Ollama ficou confuso/repetindo
mensagens numa tarefa de vários passos).

## Decisão

Nova *tool* `todo_write` (`crates/core/src/tools/todo.rs`), sem nenhum efeito colateral
(não toca sistema de arquivos, rede, nem estado do processo) — o modelo a chama para
declarar/atualizar a lista de passos da tarefa atual.

**Semântica de substituição total:** cada chamada carrega a lista **inteira e atual** (não um
*diff* incremental) — item `{content: string, status: "pending" | "in_progress" |
"completed"}`. Mesmo padrão do `TodoWrite`/`Task*` deste próprio ambiente: os argumentos da
própria chamada **são** o estado; não existe nenhum armazenamento paralelo no núcleo do
`agentry` para sincronizar entre chamadas ou entre turnos. `execute()` só valida
estruturalmente os argumentos e devolve uma confirmação textual curta — nunca falha por razão
de negócio, só por JSON malformado (mesmo padrão de erro tratado das demais *tools*).

**Renderização só na TUI, sem mudar o contrato de `StreamEvent`.** O `Session::after_response`
(núcleo) já reconstrói os argumentos completos de uma chamada de tool depois que o *stream*
termina, mas isso nunca foi exposto ao `on_event` do chamador — só os fragmentos brutos de
`StreamEvent::ToolCallDelta` chegam em tempo real, e hoje são ignorados pela TUI. Duas opções
foram avaliadas: (a) adicionar uma variante nova a `StreamEvent` sinalizando "esta chamada de
tool está completa, aqui estão os argumentos interpretados"; (b) reacumular os fragmentos do
lado da TUI, a mesma técnica que o `StreamAggregator` privado do núcleo já faz internamente,
só que na camada de renderização. **Opção (b) escolhida** — `StreamEvent` não é
`#[non_exhaustive]`; uma variante nova quebraria os `match`es exaustivos existentes em
`crates/core/src/session/mod.rs` e `crates/cli/src/tui/chat.rs`, custo desproporcional para uma
única *tool* que só precisa de uma exibição melhor, não de uma mudança de contrato entre
núcleo e chamador. A TUI ganha um mapa transiente (`id da chamada -> nome + argumentos
acumulados`), populado por `ToolCallStart`/`ToolCallDelta`; ao `MessageEnd`, qualquer entrada
com `nome == "todo_write"` tenta interpretar o JSON acumulado — sucesso vira um bloco de
*checklist* formatado anexado ao turno; falha (JSON malformado, ex.: um modelo confuso) é
ignorada silenciosamente, sem pânico nem erro exibido ao usuário — o marcador genérico
`⚙ usando todo_write...` (já existente desde a rodada 2) continua sendo o único traço visível
nesse caso.

**Sem paridade em REPL/*one-shot* nesta rodada** — nenhum dos dois mostra qualquer indicador de
atividade de tool hoje (decisão já tomada na rodada 2); `todo_write` segue a mesma disciplina.

## Consequências

- **Impacto positivo:** visibilidade de progresso em tarefas de vários passos, sem exigir
  nenhuma mudança de contrato entre `crates/core` e os consumidores (REPL/*one-shot*/TUI); modelos
  mais fracos ganham um mecanismo estruturado de "planejar antes de agir" que tende a reduzir
  confusão/repetição.
- **Impacto negativo:** a reacumulação de fragmentos JSON é duplicada entre o `StreamAggregator`
  privado do núcleo e a nova lógica da TUI — pequena divergência de responsabilidade aceita
  deliberadamente para não alargar o contrato de `StreamEvent` por uma única *tool*.
- **Trade-offs aceitos:** sem persistência entre turnos/sessões (cada chamada é o estado
  inteiro, nada é lembrado se o modelo simplesmente parar de chamar a *tool*); sem paridade
  visual em REPL/*one-shot*; sem `/forget`/edição manual da lista (o modelo é o único autor).

## Diretriz de Conformidade de Código

- **Proibido:** persistir estado de `todo_write` em qualquer arquivo/estrutura do núcleo
  (`.agentry/` ou equivalente) — a lista vive só nos argumentos da própria chamada e na
  renderização efêmera da TUI; adicionar uma variante a `StreamEvent` só para esta *tool*.
- **Obrigatório:** `execute()` de `todo_write` nunca falha por conteúdo da lista em si (só por
  JSON malformado); falha de interpretação do lado da TUI é sempre silenciosa (sem pânico, sem
  mensagem de erro disruptiva) — o marcador genérico de tool em uso já cobre o caso de
  degradação.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
