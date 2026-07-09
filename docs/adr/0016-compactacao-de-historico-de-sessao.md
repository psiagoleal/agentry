<!-- Caminho relativo: docs/adr/0016-compactacao-de-historico-de-sessao.md -->

# ADR 0016: Compactação de histórico de sessão (`Session::compact`)

- **Status:** Proposed
- **Data:** 2026-07-09
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** confiabilidade, router, session, contexto

## Contexto

`Session.messages` (`crates/core/src/session/mod.rs`) cresce sem limite a cada turno do
REPL (MT-14): cada `push_user_message` acrescenta ao vetor, e `build_request` sempre clona
o histórico inteiro para a próxima chamada. **Auditoria do estado atual:** `TokenBudget` —
único mecanismo de orçamento que existe hoje — é uma variável local (`consumed`) reiniciada
a cada chamada a `run()`/`run_streaming()`; ela limita só o sub-loop de tool-calls **dentro**
de um turno, e não tem nenhuma relação com o tamanho acumulado de `self.messages` entre
turnos. Ou seja: **não existe hoje nenhuma estratégia de recuperação** quando uma conversa
longa se aproxima ou estoura a janela de contexto real do modelo — a única coisa que acontece
é a chamada ao provider falhar (erro de API) ou, em providers menos rigorosos, o contexto ser
truncado silenciosamente sem o `agentry` saber.

Esta lacuna foi identificada ao analisar o repositório de referência `anomalyco/opencode`
(usado a pedido do usuário como fonte de ideias de usabilidade/estratégias, não só de TUI) —
o conceito deles de separar "System Context" (fatos estáveis do ambiente) de "Session History"
(conversa compactável), e de manter o *system prompt* como base estável para aproveitar cache
de prompt do provider, informa a decisão abaixo, mas **não** é adotado na sua forma completa
(o "Context Epoch"/`Mid-Conversation System Message` deles é infraestrutura bem mais pesada do
que o necessário para o estágio atual do `agentry` — ver "Fora de escopo").

A boa notícia arquitetural é a mesma do ADR-0015 (Reviewer): compactação não precisa de
infraestrutura nova. É só mais uma `task-class`, resolvida pelo Router (MT-09) como qualquer
outra, com uma chamada de chat comum (sem tools, sem streaming) pedindo um resumo.

## Decisão

Fica acordada a criação de `Session::compact(&mut self, router: &Router) -> Result<(), SessionError>`
(assinatura exata a refinar no micro-ticket):

1. **Disparo explícito, nunca automático na v0.1.** Compactação só roda quando pedida —
   mesma disciplina de *default* conservador do ADR-0015 (Reviewer desligado por padrão) e
   do próprio `session_compact` do OpenCode (keybind do usuário, não um gatilho por limiar).
   O ponto de entrada concreto (comando `/compact` no REPL, MT-14) é responsabilidade de um
   micro-ticket separado — este ADR decide só o mecanismo em `agentry_core`.
2. **`task-class` dedicada (`"compact"`), resolvida pelo Router como qualquer outra.** O
   usuário decide, por perfil/projeto, qual modelo compacta (pode ser o mesmo da conversa,
   um menor/mais barato, ou um diferente) — mesmo padrão do Reviewer (ADR-0015).
3. **Chamada simples, sem tools, sem saída estruturada.** Diferente do Reviewer (que precisa
   de veredito previsível via ADR-0012), compactar é só "resumir texto" — uma chamada comum
   a `LlmProvider::chat` com uma mensagem de instrução + o histórico atual renderizado como
   transcript, pedindo um resumo que preserve decisões, fatos e estado necessários para
   continuar a conversa.
4. **Ponto de corte seguro: só entre turnos completos.** Compactação só é chamada quando não
   há tool-call pendente — condição que já vale trivialmente sempre que o REPL devolve o
   controle ao prompt (um turno só termina, com `Done` ou `BudgetExceeded`, depois que o
   sub-loop de tools já se resolveu por completo) — sem precisar reproduzir o conceito mais
   elaborado de "Safe Provider-Turn Boundary" do OpenCode.
5. **Substituição total, não incremental.** O resultado vira a **única** mensagem de sistema
   do histórico (`vec![Message::system(resumo)]`), absorvendo tanto o *system prompt* original
   do preset (se houver, incluído no prompt de compactação) quanto o resumo da conversa —
   substitui `self.messages` inteiro. Turnos seguintes continuam normalmente a partir daí;
   `ensure_system_prompt` não precisa de nenhuma mudança (a checagem "já existe mensagem de
   sistema?" continua válida, agora satisfeita pelo resumo).
6. **Falha do provider durante a compactação não apaga o histórico.** Se a chamada falhar,
   `self.messages` permanece intocado — a operação é tudo-ou-nada, nunca deixa a sessão num
   estado intermediário corrompido.

## Consequências

- **Impacto positivo:** fecha uma lacuna real e auditada (nenhuma estratégia de recuperação
  hoje); reaproveita Router/`ChatRequest` inteiramente, sem subsistema novo; abre caminho para
  conversas de duração arbitrária no REPL/TUI.
- **Impacto negativo:** perda de detalhe do histórico original (resumo é *lossy* por
  definição); mais uma chamada de modelo (custo/latência), ainda que pontual e sob pedido
  explícito do usuário.
- **Trade-offs aceitos:** substituição total (em vez de preservar as últimas *K* mensagens
  verbatim) simplifica a v0.1 às custas de granularidade — refinamento natural para uma
  revisão futura, não uma limitação permanente; disparo automático por limiar de tokens fica
  de fora agora (ver "Fora de escopo") para não acoplar a decisão de *quando* compactar a uma
  heurística ainda não validada em uso real.

## Fora de escopo

- Disparo automático por limiar de tokens/turnos (fica para um ADR futuro, se a experiência de
  uso mostrar necessidade real — mesmo princípio de "não construir para requisito hipotético").
- Formalismo de "Context Epoch"/cache de prompt do provider e "Mid-Conversation System
  Message" (aviso de mudança de ambiente no meio da conversa) do OpenCode — infraestrutura
  desproporcional ao estágio atual do `agentry`; se cache de prompt do provider vier a importar
  de verdade (custo mensurável em uso real), é assunto de um ADR próprio.
- Compactação parcial (preservar as últimas mensagens verbatim) — ver "Trade-offs aceitos".
- Superfície de interação (comando `/compact` no REPL/TUI) — fica para o micro-ticket de CLI,
  mesma separação já usada pelo ADR-0014/MT-33 (mecanismo) vs. MT-14 (superfície).

## Diretriz de Conformidade de Código

- **Proibido:** compactação rodar sem pedido explícito do usuário (nada de gatilho automático
  na v0.1); `Session::compact` inventar um mecanismo de chamada de modelo próprio — usa
  exclusivamente Router/`ChatRequest` como qualquer outra tarefa; deixar `self.messages` num
  estado parcial/corrompido se a chamada de compactação falhar.
- **Obrigatório:** compactação é uma `task-class` roteável independentemente (`"compact"` ou
  nome equivalente); resultado da compactação sempre substitui o histórico inteiro por uma
  única mensagem de sistema, nunca mistura resumo com mensagens antigas remanescentes; falha
  do provider durante a compactação preserva o histórico original intacto.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
