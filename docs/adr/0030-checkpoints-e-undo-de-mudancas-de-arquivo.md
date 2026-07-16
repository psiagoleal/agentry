<!-- Caminho relativo: docs/adr/0030-checkpoints-e-undo-de-mudancas-de-arquivo.md -->

# ADR 0030: Checkpoints e *undo* de mudanças de arquivo feitas pelo agente

- **Status:** Proposed
- **Data:** 2026-07-16
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** ferramentas, persistência, UX

## Contexto

A Fase 17 (uso de tokens visível) fechou inteira (MT-82..85, ADR-0029 `Accepted`). O roadmap
de longo prazo (`docs/roadmap-longo-prazo.md` §Fase 18+, à época) enumera quatro frentes
restantes de "segunda onda" sem ordem declarada entre elas; a decisão de qual preparar agora
está registrada em `docs/decisoes-autonomas.md` (2026-07-16) — **checkpoints/*undo*** de
mudanças de arquivo, escolhida por ser a única sem nenhuma pergunta de segurança/
confidencialidade/egresso em aberto (diferente de multimodal, memória entre sessões e
subagentes — ver a entrada de decisão para o detalhe de cada uma).

O `agentry` já tem duas tools que escrevem arquivo (`crates/core/src/tools/fs.rs`): `fs_write`
(sobrescreve por inteiro) e `fs_edit` (substitui uma ocorrência única de `old_string` por
`new_string`) — ambas com uma chave `path` no schema de argumentos. Nenhuma das duas guarda o
conteúdo anterior do arquivo; uma edição malfeita do agente hoje só é revertida manualmente
(`git checkout`/`git diff` se o arquivo estiver versionado, ou perdida de vez se não estiver).
Equivalente ao "rewind" do Claude Code CLI/OpenCode — mas o `agentry` não tem esse mecanismo.

O diretório de estado local (`.agentry/`, ADR-0017) já resolve **onde** persistir dado local
ao projeto, auto-excluído do git por padrão (`gitignore` gerado dentro do próprio diretório) —
o mesmo mecanismo que os índices RAG (MT-27/28) e o schema de configuração (ADR-0018) já
usam. Checkpoints de arquivo são outro caso do mesmo padrão: dado local, nunca versionado por
padrão, específico da máquina/checkout.

## Decisão

### O que é registrado, e quando

Fica acordado que **só `fs_write`/`fs_edit`** geram checkpoint — nunca mudanças feitas por
`shell_exec`/`shell_background` (um comando de shell pode alterar arquivos de formas
arbitrárias e indetermináveis de antemão; capturar esse caso fica fora de escopo, mesmo nível
de confiança já aceito para qualquer efeito colateral de comando, ADR-0026). Antes de cada
chamada bem-sucedida de `fs_write`/`fs_edit`, o conteúdo **anterior** do arquivo-alvo (ou a
ausência dele, se o arquivo ainda não existia) é registrado como um novo checkpoint — numa
pilha *LIFO* (`.agentry/checkpoints.json`, um array JSON: `path`, `conteudo_antes` (`None` se
o arquivo não existia), `timestamp`). Uma chamada que falha (erro tratado da tool) não gera
checkpoint — nada mudou de fato, não há o que desfazer.

Implementado como um decorador (`CheckpointingTool`, novo em `crates/core/src/tools/`)
envolvendo a tool real — lê `arguments["path"]` (chave comum às duas tools), lê o conteúdo
atual do arquivo (se existir) **antes** de delegar a chamada de verdade, grava o checkpoint só
se o resultado delegado não for erro. Só `fs_write`/`fs_edit` são envolvidas na fiação da CLI
(`crates/cli/src/main.rs`) — nenhuma mudança na *trait* `Tool` (MT-11) nem no `ToolRegistry`
genérico, que continua sem saber que checkpoints existem.

### *Undo*: um nível por chamada, pilha

`CheckpointStore::undo()` (novo em `crates/core/src/checkpoint/mod.rs`) desempilha a entrada
mais recente, restaura o arquivo ao conteúdo anterior (ou remove o arquivo, se ele não
existia antes do checkpoint) e devolve o que foi desfeito (caminho + se foi restaurado ou
removido) para o chamador exibir. Chamar de novo desfaz o passo anterior a esse — nenhum
mecanismo de "escolher qual checkpoint" nesta versão, sempre o mais recente (mesma
simplicidade de um `Ctrl+Z` de editor de texto).

Exposto em três pontos, mesmo padrão da Fase 17 (ADR-0029) — reaproveitar uma decisão já
validada em vez de inventar uma nova:

- **Flag `--undo`** (modo *one-shot*, `crates/cli/src/main.rs`) — desfaz o checkpoint mais
  recente (de **qualquer** invocação anterior, já que os checkpoints persistem em disco) e
  sai, sem rodar nenhuma tarefa. Mutuamente exclusiva com `--init`/`--tui`/tarefa (mesmo
  padrão de `--init` hoje).
- **Comando `/undo`** no REPL (mesmo padrão de `/compact`).
- ***Keybinding* na TUI** (`Ctrl+Z`, único ainda livre na tabela de `crates/cli/src/tui/keybind.rs`).

### Teto de checkpoints — constante fixa, sem config nova

Fica acordado um teto fixo (constante, não configurável nesta versão) de checkpoints
retidos — ao ultrapassar o teto, o mais antigo é descartado silenciosamente antes de
persistir o novo. Evita crescimento ilimitado de `.agentry/checkpoints.json` sem abrir uma
nova superfície de configuração (ADR-0022) para uma necessidade ainda hipotética — se houver
demanda real de ajustar o teto, isso vira uma extensão futura desta ADR, não uma decisão
antecipada agora (YAGNI).

## Consequências

- **Impacto positivo:** uma mudança indesejada de `fs_write`/`fs_edit` fica reversível com um
  comando, sem depender de o projeto estar versionado no git; reaproveita 100% da
  infraestrutura já existente (diretório de estado, ADR-0017; *trait* `Tool`, MT-11).
- **Impacto negativo:** mudanças feitas por `shell_exec`/`shell_background` continuam
  irreversíveis pelo `agentry` (fora de escopo, ver acima) — um usuário pode confundir
  "*undo* existe" com "toda mudança é reversível", risco de expectativa equivocada que a
  documentação (MT-89) precisa deixar explícito.
- **Trade-offs aceitos:** só um nível de desfazer por vez (sem seleção de checkpoint
  específico) e teto fixo sem configuração — ambos aceitos pela simplicidade do design
  mínimo; podem virar extensões futuras se houver demanda concreta.

## Diretriz de Conformidade de Código

- **Proibido:** gerar checkpoint de qualquer mudança que não seja `fs_write`/`fs_edit`
  bem-sucedida; qualquer mecanismo de *undo* que não seja a pilha *LIFO* de um nível
  (introduzir seleção de checkpoint específico exige revisar esta ADR primeiro); persistir
  checkpoint fora de `.agentry/` (ADR-0017) — nunca um diretório global do usuário.
- **Obrigatório:** `CheckpointingTool` só envolve `fs_write`/`fs_edit` na fiação de produção;
  `--undo`/`/undo`/*keybinding* de *undo* chamam a **mesma** `CheckpointStore::undo()`, nunca
  três implementações divergentes do mesmo desfazer.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
