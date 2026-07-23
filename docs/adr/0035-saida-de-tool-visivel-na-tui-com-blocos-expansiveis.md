<!-- Caminho relativo: docs/adr/0035-saida-de-tool-visivel-na-tui-com-blocos-expansiveis.md -->

# ADR 0035: Saída de tool visível na TUI, com blocos expansíveis por clique

**Status:** Accepted

## Contexto

Pedido do mantenedor, num teste real: hoje a TUI mostra só um marcador genérico
(`⚙ usando <tool>...`) quando o modelo chama uma tool — nem o comando/argumentos completos,
nem a saída da execução aparecem no corpo da conversa (só no modal de confirmação, que o
MT-112 já corrigiu para nunca cortar o texto). O pedido: mostrar um *preview* do comando no
corpo da conversa, com a possibilidade de expandir (por clique do mouse) para ver o comando
completo **e** a saída completa da tool — mesmo padrão do Claude Code CLI.

Duas partes do pedido têm custo bem diferente:

1. **Preview do comando** — já viável sem mudar o núcleo: a TUI já reacumula os argumentos de
   qualquer chamada de tool via `StreamEvent::ToolCallStart`/`ToolCallDelta` (mesma técnica do
   MT-107 para o `todo_write`), só nunca generalizou essa exibição além do *checklist* de
   `todo_write`.
2. **Saída da tool** — não existe hoje nenhum canal para isso chegar à TUI. A execução de
   tools acontece dentro de `Session::after_response` (núcleo), que empurra o
   `ContentBlock::ToolResult` direto em `self.messages`, sem nenhum gancho de evento — o
   `on_event` de `run_streaming` só recebe o que o **modelo** transmite (texto/tool-calls), não
   o resultado de executar uma tool.

## Decisão

### 1. Nova variante em `StreamEvent`: `ToolCallResult`

```rust
ToolCallResult { id: String, content: String, is_error: bool }
```

Campos espelham `ToolResult` (`call_id` renomeado para `id`, consistente com
`ToolCallStart`/`ToolCallDelta`) — o nome da tool **não** viaja de novo aqui, o consumidor
correlaciona pelo `id` com o `ToolCallStart` já recebido antes (mesmo mapa
`chamadas_em_andamento` que o `ChatState` já mantém desde o MT-107).

**Verificado antes de decidir:** o *blast radius* de adicionar essa variante é exatamente duas
correspondências exaustivas em produção — `StreamAggregator::apply` (`session/mod.rs`, privado,
vira um no-op: o resultado de uma tool não é parte da mensagem do modelo sendo acumulada) e
`ChatState::aplicar_evento` (`tui/chat.rs`, onde a exibição de verdade acontece). Bem menor que
o caso do MT-107/ADR-0034 (que evitou uma variante nova por causa de um custo desproporcional
para uma única *tool*) — aqui a mudança serve **qualquer** tool, e não tem alternativa: a saída
de uma execução não existe nos argumentos, reacumular do lado da TUI (como o `todo_write` faz)
não é aplicável.

### 2. `Session::after_response` ganha um canal de saída para os resultados executados

`after_response` já usa o padrão de parâmetro de saída por mutação (`consumed: &mut Usage`);
ganha mais um: `resultados: &mut Vec<ToolResult>`, populado no mesmo laço que já executa cada
`ToolCall` via `self.executor.execute(call).await`. `Session::run` (não-*streaming*, REPL/
*one-shot*) passa um `Vec` descartável — sem paridade nova para REPL/*one-shot*, mesma decisão
de escopo já tomada no MT-107. `Session::run_streaming` (TUI) drena o `Vec` logo depois de
cada chamada a `after_response` e emite um `StreamEvent::ToolCallResult` por item via
`on_event` — mesmo ponto do laço onde os eventos brutos do turno já são repassados em lote.

### 3. TUI: blocos de tool viram entidades estruturadas, não texto solto

Hoje `Mensagem` é só `texto: String`, com o marcador `⚙ usando <tool>...` embutido direto no
meio do texto corrido — não dá para "expandir" um trecho que não existe como entidade própria.
`Mensagem` ganha uma representação de **blocos** (texto corrido intercalado com blocos de
chamada de tool, cada um com id/nome/argumentos/resultado/estado de expansão) em vez de uma
`String` única. Detalhado no plano de micro-tickets, não nesta ADR (é implementação, não
decisão de arquitetura).

### 4. Expandir/recolher por clique do mouse, com `Shift`+arraste preservando seleção nativa

Captura de mouse via `crossterm::event::{EnableMouseCapture, DisableMouseCapture}` — já
disponível pela dependência `ratatui`/`crossterm` já existente (`features = ["crossterm"]`),
**nenhuma dependência nova**. `Shift`+arraste continua selecionando texto nativamente **sem
nenhum código nosso** — é convenção do próprio emulador de terminal (herdada do xterm por
GNOME Terminal, Windows Terminal, iTerm2, Alacritty, etc.): ao ativar modo de captura de mouse,
seguro `Shift` faz o terminal interceptar o evento antes de repassar à aplicação. Só precisamos
ativar/desativar a captura de forma limpa (`ratatui::try_init`/`restore` não mexem nisso
sozinhos hoje — precisa de verificação explícita de que a captura é desligada mesmo em saída
por pânico, para não deixar o terminal do usuário preso em modo de captura de mouse).

## Consequências

- **Positivas:** paridade real com o padrão de UX do Claude Code CLI citado pelo mantenedor;
  visibilidade de tool completa (comando + saída) sem exigir rolagem/modal separado; nenhuma
  dependência nova.
- **Negativas/custos aceitos:** mudança de arquitetura real no núcleo (nova variante de
  `StreamEvent`, ainda que de *blast radius* pequeno e verificado); refatoração do modelo de
  dados de `Mensagem` na TUI (texto solto → blocos estruturados), que tem efeito colateral em
  toda a renderização de histórico (Markdown mínimo, recuo pendurado, etc. — MT-108/109
  precisam continuar funcionando sobre blocos de texto, não só sobre uma `String`); captura de
  mouse é uma mudança de comportamento de terminal que precisa de teste manual cuidadoso
  (inclusive saída por pânico) para não regredir a experiência de quem usa o `agentry` num
  terminal onde mouse não é esperado/desejado.
- **Escopo desta ADR:** só a TUI. REPL/*one-shot* continuam sem nenhuma visibilidade de
  atividade de tool, mesma decisão já tomada no MT-107.

## Diretriz de Conformidade de Código

- Toda nova variante de `StreamEvent` **deve** ter suas correspondências exaustivas
  atualizadas em `StreamAggregator::apply` e `ChatState::aplicar_evento` no mesmo commit —
  nunca introduzir um `_ =>` genérico nesses dois pontos para "economizar" a atualização (o
  objetivo de manter o `match` exaustivo é forçar essa revisão a cada variante nova).
- Captura de mouse **precisa** ser desativada em todo caminho de saída do modo TUI, incluindo
  pânico — verificado manualmente antes de fechar o ticket correspondente, não só por teste
  automatizado (efeito é observável só no terminal real do usuário).
- REPL/*one-shot* não ganham nenhuma paridade de exibição de tool nesta frente — proibido
  estender `StreamEvent::ToolCallResult`/blocos de mensagem para esses modos sem uma nova
  decisão registrada.
