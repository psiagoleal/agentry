<!-- Caminho relativo: docs/adr/0027-tui-via-ratatui.md -->

# ADR 0027: TUI via `ratatui` — modo interativo opt-in, sem substituir o REPL

- **Status:** Accepted
- **Data:** 2026-07-15
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** dependências, TUI, UX, arquitetura

## Contexto

O REPL de texto puro (`crates/cli/src/repl.rs`, MT-14) é funcional, mas limitado: histórico
rolante linear, sem retorno visual para confirmação de tool (`ask`, `Confirmer`), sem seletor
de modelo/provider — só o comando `/model <nome>`/`/provider <nome>` por texto exato. O
usuário pediu explicitamente uma TUI, como um dos dois temas centrais do roadmap de longo
prazo (junto de tools, Fase 14, já concluída).

`ratatui` já estava **vetado por maturidade/licença como pendente** desde `docs/architecture.md`
§Roadmap original (nunca implementado) — este ADR fecha essa pendência com verificação de
maturidade de verdade (ADR-0004), feita agora, com dados de hoje:

- **`ratatui`** — MIT, **37,9M downloads totais / 14,2M nos últimos 90 dias**, 87 versões
  publicadas desde fevereiro de 2023, versão estável atual `0.30.2`, atualizado pela última
  vez em 2026-06-19 (ativo). Repositório: `github.com/ratatui/ratatui` (organização própria,
  não um projeto pessoal). Verificado via `crates.io/api/v1/crates/ratatui`.
- **Backend de terminal: `crossterm`** — dependência natural do `ratatui` (*feature*
  `crossterm` embutida, via o sub-crate `ratatui-crossterm`) e a escolha certa para este
  projeto especificamente por ser **cross-platform de verdade** (Windows/macOS/Linux
  uniformemente) — ao contrário de `termion`, que é só Unix — mesma exigência de portabilidade
  já estabelecida pelo ADR-0005 (matriz de CI em 3 SOs).

Uma decisão de arquitetura é necessária além da simples adoção da dependência: **como** a TUI
se conecta à infraestrutura já existente (`Session`/`Router`/`ToolRegistry`/`Confirmer`/
`Prompter`) sem duplicá-la, e **como** ela convive com o REPL de texto (substituir? coexistir?).

## Decisão

### Adoção da dependência

Fica acordada a adoção de **`ratatui`** (com a *feature* `crossterm`) como dependência de
`crates/cli` (não de `crates/core` — é puramente uma camada de apresentação da CLI, o `core`
não sabe nem precisa saber que uma TUI existe). Maturidade verificada acima; licença MIT
compatível com a política do projeto (ADR-0001).

### A TUI é um **modo opt-in novo**, nunca substitui o REPL de texto

Nova flag `--tui` ativa o modo TUI; sem ela, o comportamento **one-shot**/**REPL** de texto
continua **inalterado**, byte a byte — zero risco de regressão no caminho já testado e usado
(MT-14 em diante). Promover a TUI a *default* fica para um ticket futuro, só depois dela
provar estabilidade — decisão deliberadamente adiada, não hoje.

### Reaproveitamento total da infraestrutura existente — a TUI não duplica nada do `core`

A TUI roda sobre exatamente a mesma `Session`/`Router`/`ToolRegistry`/`PermissionGate` que o
REPL já usa — muda só a **apresentação**:

- **Streaming:** `Session::run_streaming` já aceita um *callback* genérico
  (`FnMut(&StreamEvent)`, ver `crates/core/src/session/mod.rs`) — o mesmo mecanismo que
  `streaming::stream_to_writer` (CLI) já usa para escrever texto incremental no REPL. A TUI
  roda `run_streaming` numa *task* separada (`tokio::spawn`), cujo *callback* envia cada
  `StreamEvent` (já `Clone`) por um canal (`tokio::sync::mpsc`) de volta ao laço de eventos
  principal — que faz `tokio::select!` entre eventos de terminal (teclado/*resize*, via
  `crossterm::event`) e eventos de *stream* do canal. **Nenhuma mudança no `core`** é
  necessária para isso — a API já foi desenhada de forma genérica o suficiente (mesmo espírito
  do `Confirmer`/`Prompter`, ADR-0024).
- **Confirmação de tool (`ask`):** nova `TuiConfirmer` implementando `Confirmer`
  (`crates/cli/src/tool_executor.rs`, MT-14) — mesma *trait*, implementação por widget em vez
  de `print!`/`read_line`.
- **`AskUser` (ADR-0024):** nova `TuiPrompter` implementando `Prompter`
  (`crates/core/src/tools/ask_user.rs`, MT-63) — mesma *trait*, widget em vez de terminal
  síncrono. Prova que a decisão do ADR-0024 de definir `Prompter` no `core` (não como tipo só
  da CLI) estava correta: a TUI ganha uma segunda implementação de graça, sem tocar
  `AskUserTool`.

### Modelo de teclas: mapa único, desacoplado dos widgets

Uma única tabela `Definitions` (nome de ação → tecla *default* + descrição), inspirada em
`packages/tui/src/config/keybind.ts` do OpenCode (referência de UX explícita do usuário, não
de código — *stack* deles é TypeScript/SolidJS, irreproduzível em Rust, só a **ideia**
importa): os widgets consultam a ação pelo nome, nunca a tecla bruta. Uma tecla customizada
pelo usuário que não bate com nenhum nome conhecido é erro tratado — mesmo espírito
*fail-closed* de `ConfigError::UnsupportedSchema` (ADR-0003), aplicado a keybinds.

### Seletor de modelo/*provider* (MT-73)

Evolução natural do `/model <nome>`/`/provider <nome>` (texto exato, MT-14/50): um widget de
seleção com busca difusa (*fuzzy*) sobre os candidatos já declarados na `task-class` ativa —
**nunca** introduz um candidato novo, mesma disciplina de override já vetada pelo ADR-0014
(`--model`/`--provider` só escolhem entre o que já está registrado).

### Permissão: *toggle* de dois estados, nunca contorna `deny`

Inspirado em `packages/tui/src/context/permission.tsx` do OpenCode: um atalho único alterna
entre dois modos de UX (`auto`/`normal`) — **não** expõe `allow`/`ask`/`deny` cru na tela. Mas
isso é só simplificação de **apresentação**: o modo `auto` só pode remover o atrito de
confirmar um `ask` mais rápido (aprovação automática de tools sob `ask`, nunca sob `deny`) —
**nunca** contorna um `deny` do `PermissionGate` (MT-11). Essa é uma invariante de segurança,
não uma escolha de UX; qualquer implementação que a violar é um bug, não uma feature.

### Visualizador de diff — modal de primeira classe

Para confirmações de `fs_write`/`fs_edit` (MT-12): em vez de imprimir os argumentos brutos da
tool-call antes do prompt de confirmação (comportamento atual do `InteractiveConfirmer`), um
modal mostra o diff de verdade (linhas removidas/adicionadas). Reaproveita só apresentação —
nenhuma mudança em `FsWriteTool`/`FsEditTool` (MT-12), que já devolvem o suficiente para
montar o diff no lado da CLI.

### Fora de escopo desta fase (YAGNI — sem funcionalidade correspondente hoje)

- **Widget de lista de tarefas** (`todo-item.tsx` do OpenCode): agentry **não tem** nenhum
  conceito de lista de tarefas visível hoje (`Session` não rastreia nada parecido) — construir
  a UI sem a funcionalidade por trás seria UI para um dado que não existe. Fica condicionado a
  um ticket futuro que primeiro adicione o conceito ao `core`, se/quando houver demanda real.
- Paleta de comandos (`ctrl+p`), "stash" de mensagem não enviada, undo/redo de mensagem —
  ideias menores anotadas na referência do OpenCode, nenhuma pedida explicitamente pelo
  usuário; ficam para tickets futuros independentes, não parte do escopo mínimo desta fase.
- Promover a TUI a modo *default* (hoje é sempre opt-in via `--tui`).

## Consequências

- **Impacto positivo:** aproxima o `agentry` do Claude Code CLI/OpenCode na dimensão que o
  usuário mais pediu (junto de tools, já concluída); reaproveita 100% da infraestrutura de
  domínio existente — `TuiConfirmer`/`TuiPrompter` provam que as fronteiras de trait já
  desenhadas (ADR-0024) generalizam de verdade; zero risco para o caminho REPL existente
  (aditivo, não substitutivo).
- **Impacto negativo:** primeira dependência de UI pesada do projeto (`ratatui`+`crossterm`,
  mais o que cada um traz transitivamente) — aumenta o tempo de compilação e o tamanho do
  binário da CLI; mais uma superfície de código para manter (widgets, laço de eventos).
- **Trade-offs aceitos:** escopo mínimo deliberado (sem lista de tarefas, sem paleta de
  comandos) em troca de entregar o essencial (*streaming*, confirmação, seletor de modelo,
  diff) sem inflar o ticket-mãe; opt-in via `--tui` em vez de virar *default* imediatamente,
  aceitando que a maioria dos usuários só veja a TUI se pedir explicitamente até ela amadurecer.

## Diretriz de Conformidade de Código

- **Proibido:** `ratatui`/`crossterm` como dependência de `crates/core` — são só de
  `crates/cli`; qualquer widget de permissão/confirmação contornar um `deny` do
  `PermissionGate` (MT-11), mesmo sob o modo `auto`; a TUI reimplementar lógica de domínio já
  existente no `core` (resolução de rota, execução de tool, política de shell) em vez de
  reaproveitar `Session`/`Router`/`ToolRegistry` como estão; mudar o comportamento do modo
  one-shot/REPL de texto existente para acomodar a TUI.
- **Obrigatório:** `--tui` como *opt-in* explícito (sem `--tui`, comportamento atual
  preservado byte a byte); `TuiConfirmer`/`TuiPrompter` implementam as *traits* já existentes
  (`Confirmer`/`Prompter`), nunca um mecanismo paralelo; seleção de modelo/*provider* pela TUI
  restrita aos candidatos já declarados (mesma disciplina do ADR-0014); tabela de keybinds
  única, nome de ação desconhecido é erro tratado.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
