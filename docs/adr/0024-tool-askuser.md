<!-- Caminho relativo: docs/adr/0024-tool-askuser.md -->

# ADR 0024: Tool `AskUser` — pergunta/confirmação entre agente e usuário

- **Status:** Proposed
- **Data:** 2026-07-15
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** tools, interação, UX

## Contexto

Hoje o único canal humano→agente é o `Confirmer` (`crates/cli/src/tool_executor.rs`, MT-14):
aprova ou recusa uma `ToolCall` sob `ask` (sim/não, sem espaço para nuance). O agente não tem
como **perguntar** algo ao usuário no meio de uma tarefa — esclarecer uma ambiguidade,
confirmar uma escolha de design, ou pedir uma informação que só o humano tem. O usuário pediu
explicitamente esse mecanismo, comparando-o à tool `AskUserQuestion` do Claude Code CLI (ver
`docs/roadmap-longo-prazo.md` §Fase 14, planejamento original).

Uma decisão é necessária agora para fixar **onde** essa capacidade vive (core ou CLI) e **como**
ela se conecta ao `ToolRegistry` já existente, sem duplicar o padrão do `Confirmer` nem
introduzir um segundo jeito de fazer a mesma coisa.

## Decisão

Fica acordada uma nova tool **`ask_user`**, implementando a `trait Tool` (ADR/MT-11) como
qualquer outra — **sem mudar a trait `Tool`** e **sem mecanismo de acionamento especial**: o
modelo decide chamar `ask_user` como decide chamar qualquer tool, orientado só pela sua
`description`.

**Onde o canal de interação vive:** ao contrário do `Confirmer` (que é um tipo só da CLI,
usado pela ponte `RegistryToolExecutor`/`ToolExecutor`), a tool `ask_user` — sendo uma `Tool`
de verdade — precisa viver em `agentry_core::tools` (mesmo lugar de toda `Tool`,
`RepoMapTool`/`SkillTool`/etc.). Por isso o canal de interação segue o padrão de
**`AuditSink`/`GuardrailAuditSink`** (interface definida no `core`, implementação concreta
fornecida pela CLI), não o padrão do `Confirmer` (tipo só da CLI) — apesar do texto original
do roadmap dizer "mesmo padrão do `Confirmer`", essa frase se refere à **forma** (trait
dyn-compatible via `BoxFuture`, injetada via `Arc<dyn ...>`, sem `async-trait`), não ao
**lugar** onde o tipo é definido.

```rust
// agentry_core::tools::ask_user
pub trait Prompter: Send + Sync {
    /// Pergunta `question` ao usuário; `options`, se não vazio, são
    /// sugestões (o usuário ainda pode responder livremente). Devolve a
    /// resposta como texto — sem parsing/validação aqui, quem decide o que
    /// fazer com a resposta é o próprio modelo no próximo turno.
    fn ask(&self, question: &str, options: &[String]) -> BoxFuture<'_, String>;
}
```

`AskUserTool::new(prompter: Arc<dyn Prompter>)` guarda o canal e o consulta em `execute()`. A
CLI fornece `InteractivePrompter` (lê `stdin`, mesmo padrão síncrono já usado por
`InteractiveConfirmer`) — funciona tanto no modo *one-shot* quanto no REPL, sem distinção (a
mesma raiz de código já roda nos dois modos hoje).

**Escopo deliberadamente mínimo (sem *over-engineering*):** `question: String` +
`options: Vec<String>` (sugestões opcionais, texto livre sempre aceito) → resposta em texto.
**Não** há seleção múltipla, *preview* rico, nem validação de formato — isso é o que o Claude
Code CLI oferece na sua própria ferramenta (`AskUserQuestion`), mas replicar tudo aqui seria
superfície não pedida; texto livre + sugestões cobre o caso de uso descrito pelo usuário.

**Permissão:** a tool roda sob o `PermissionGate` genérico como qualquer outra (MT-11) — sem
*default-deny* especial (diferente da tool de shell). Colocá-la em `ask` geraria uma
confirmação **antes** da própria pergunta (duplo atrito) — não é proibido, mas não é o padrão
recomendado; nenhum tratamento especial no código para isso.

**TUI (Fase 15):** quando a TUI existir, ela implementa seu próprio `Prompter` (widget de
pergunta) — sem mudança nenhuma em `AskUserTool`/`Prompter`, é só uma segunda implementação da
mesma interface, exatamente como `AuditSink` já tem duas implementações (`StderrAuditSink`
hoje; um `Prompter` de TUI viria depois).

## Consequências

- **Impacto positivo:** fecha uma lacuna explícita de UX pedida pelo usuário; reaproveita
  100% da infraestrutura de tool já existente (`Tool`/`ToolRegistry`/`PermissionGate`,
  MT-11); zero dependência nova; interface pronta para a TUI (Fase 15) sem retrabalho.
- **Impacto negativo:** mais uma tool sempre registrada — o modelo precisa saber quando
  **não** usá-la (perguntar demais é ruído); mitigado só pela qualidade da `description` da
  tool, não por nenhum mecanismo de código.
- **Trade-offs aceitos:** escopo mínimo (texto livre + sugestões) em vez de replicar toda a
  superfície de `AskUserQuestion` do Claude Code CLI.

## Diretriz de Conformidade de Código

- **Proibido:** `AskUserTool`/`Prompter` fazerem qualquer chamada de rede (é canal 100%
  local, humano↔processo); mudar a `trait Tool` para acomodar esta tool — ela implementa a
  trait existente como qualquer outra; bloquear o agent loop de forma que outra sessão/tarefa
  fique presa esperando — o bloqueio é só da própria chamada `await` desta tool-call, mesmo
  modelo do `Confirmer`.
- **Obrigatório:** `Prompter` definido em `agentry_core` (padrão `AuditSink`), implementação
  concreta na CLI; resposta do usuário sempre texto livre, nunca validação/parsing forçado
  dentro da tool.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
