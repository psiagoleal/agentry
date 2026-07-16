<!-- Caminho relativo: docs/adr/0029-uso-de-tokens-visivel-na-sessao.md -->

# ADR 0029: Uso de tokens visível durante a sessão

- **Status:** Accepted
- **Data:** 2026-07-16
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** observabilidade, UX, sessão

## Contexto

A Fase 16 (MCP) fechou inteira (MT-77..81, ADR-0028 `Accepted`). O roadmap de longo prazo
(`docs/roadmap-longo-prazo.md` §Fase 18+, à época ainda §Fase 17+ — renomeada ao nascer esta
ADR) enumerava cinco frentes de "segunda onda" sem ordem declarada entre elas; a decisão de
qual preparar primeiro está registrada em `docs/decisoes-autonomas.md` (2026-07-16) —
escolhida esta ("custo/uso visível", hoje Fase 17) por ser a única sem nenhuma pergunta de
segurança/confidencialidade/egresso em aberto.

`Usage` (`crates/core/src/model/mod.rs`) — `input_tokens`/`output_tokens` — já é calculado
por **turno** (uma chamada ao provider) dentro de `Session` (`crates/core/src/session/mod.rs`),
mas **descartado** ao final de cada turno: não é acumulado ao longo da sessão, e nenhum modo de
invocação (*one-shot*, REPL, TUI) expõe esse número ao usuário hoje. O único jeito de saber
quantos tokens uma sessão consumiu é inspecionar a saída bruta de auditoria de rede
(`AuditEntry`, ADR-0002) manualmente — não é uma resposta direta à pergunta "quanto essa
sessão custou/consumiu", que qualquer usuário de uma CLI agêntica local eventualmente faz.

Diferente de "custo" em dólares — que exigiria uma tabela de preço por modelo/provider,
informação que não é intrínseca ao provider (o mesmo modelo Ollama local não tem preço; um
gateway LiteLLM pode ou não expor preço; a Anthropic tem preço público, mas variável por
versão) — **tokens são um dado que o `agentry` já calcula com certeza, para todo provider,
sem nenhuma configuração adicional**. Uma tabela de preço configurável é um escopo
deliberadamente maior (novo bloco de configuração, manutenção de preços desatualizando com o
tempo) que não se justifica só para "ver o que já existe".

## Decisão

Fica acordado que esta fase entrega **uso de tokens acumulado por sessão**, não custo
monetário — a segunda fica deliberadamente fora de escopo (ver "Fora de escopo" abaixo).

### `Session` acumula `Usage` ao longo da sessão

`Session` ganha um campo interno de uso acumulado (`Usage`, mesma struct já existente —
nenhum tipo novo), somado a cada turno concluído (mesmo ponto onde `Usage` de um turno já é
calculado hoje, só que sem descartar o valor depois). Um método de leitura (`fn
usage_total(&self) -> Usage`, ou nome equivalente) expõe o total acumulado — chamado pela
CLI, nunca pelo `core` internamente (não influencia roteamento nem *budget* de contexto,
que já tem seu próprio mecanismo, `TokenBudget`, com responsabilidade distinta: truncar
histórico, não relatar consumo).

### Exposição por modo de invocação

- **Modo *one-shot*** (`agentry "tarefa"`) — uma linha de resumo em `stderr` ao final da
  tarefa (mesma classe de saída já usada para `[audit] ...`, nunca em `stdout`, que continua
  reservado à resposta do agente — um script que capture `stdout` não é afetado).
- **REPL** — novo comando `/usage` (mesmo padrão de `/compact`, `crates/cli/src/repl.rs`):
  imprime o total acumulado da sessão até aquele ponto, a qualquer momento, sem side-effect
  na conversa.
- **TUI** (`--tui`, ADR-0027) — o total acumulado aparece na barra de rodapé já existente
  (mesmo lugar da legenda de *keybindings*), atualizado a cada turno concluído — sem modal
  novo, sem tecla nova.

### Sem persistência entre sessões

O contador **zera a cada nova sessão/invocação** — não é gravado em disco, não é lido de
uma sessão anterior. Isso mantém o escopo mínimo (nenhuma decisão de onde/como persistir,
nenhuma pergunta de retenção) e é consistente com o fato de que "memória entre sessões" é
uma frente própria, ainda não preparada (`docs/roadmap-longo-prazo.md` §Fase 18+) — esta ADR
não antecipa nenhuma decisão daquela frente.

## Consequências

- **Impacto positivo:** resposta direta a "quanto essa sessão consumiu", sem precisar
  inspecionar auditoria de rede manualmente; reaproveita 100% de dado já calculado
  (`Usage`), nenhuma chamada nova ao provider, nenhum campo novo de configuração.
- **Impacto negativo:** não responde "quanto isso custou em dinheiro" — quem quer essa
  resposta ainda precisa fazer a conta manualmente a partir do total de tokens.
- **Trade-offs aceitos:** contador não persiste entre sessões (reinicia a cada invocação) —
  aceito porque persistência é uma decisão de escopo maior, pertencente à frente "memória
  entre sessões" (Fase 18+, ainda não preparada), não a esta.

## Fora de escopo

- **Custo em dólares/moeda** — exigiria uma tabela de preço por modelo/provider configurável
  pelo usuário (preço nem sempre público, muda com o tempo); fica para uma extensão futura
  desta mesma ADR ou uma nova, se/quando houver demanda concreta.
- **Uso agregado entre sessões** (histórico de consumo ao longo de dias/semanas) — depende de
  persistência em disco, mesma fronteira da frente "memória entre sessões" (Fase 18+).
- **Limite/orçamento configurável de tokens por sessão** (ex.: abortar ao ultrapassar N
  tokens) — é uma capacidade de controle, distinta de "visibilidade"; não pedida pelo
  roadmap atual, YAGNI até haver demanda concreta.

## Diretriz de Conformidade de Código

- **Proibido:** persistir uso de tokens em disco entre sessões sem uma ADR própria que
  resolva a pergunta de retenção (mesma disciplina de qualquer novo caminho de gravação de
  conteúdo de sessão); calcular ou estimar custo monetário sem uma tabela de preço explícita
  e configurada pelo usuário (nunca inferir preço).
- **Obrigatório:** o total de uso exposto ao usuário reflete exatamente a soma dos `Usage`
  por turno já calculados por `Session` — nenhuma fonte de dado paralela ou estimativa;
  saída de uso em modo *one-shot* vai para `stderr`, nunca para `stdout`.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
