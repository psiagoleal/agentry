<!-- Caminho relativo: docs/adr/0015-reviewer-auditoria-semantica-por-task-class.md -->

# ADR 0015: Reviewer — auditoria semântica de tarefas via `task-class` dedicada

- **Status:** Accepted
- **Data:** 2026-07-08
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** confiabilidade, router, session, segurança, guardrails

## Contexto

O ADR-0007 (Guardrail Gate) deliberadamente restringiu a v0.1 a correspondência
determinística (palavra-chave/regex) e adiou moderação **semântica** real — julgamento por
modelo, não por padrão fixo — para "uma v0.2, se necessária, sob novo ADR". Este é esse ADR.

A motivação concreta: modelos open-source pequenos (8B–30B, alvo de uso local do `agentry`)
têm autocrítica fraca dentro de uma mesma janela de contexto contínua — cometem erros que não
conseguem notar sozinhos no mesmo fôlego de raciocínio. Uma segunda passada, com contexto
fresco (e potencialmente um modelo diferente, maior ou mais especializado), captura uma classe
de erro que nem a allowlist de egresso (ADR-0002), nem o gate de permissão de ação (MT-11),
nem o Guardrail Gate determinístico (ADR-0007) cobrem: **o resultado está correto? é seguro?
cumpre o que foi pedido?**

A boa notícia arquitetural: uma auditoria é, estruturalmente, só mais uma `task-class`. O
Router (MT-09) já resolve `task-class → (provider, modelo, preset)`; a saída estruturada
(ADR-0012) já existe para obter um veredito em formato prevísivel. **Não é necessária
infraestrutura nova** — o Reviewer é um componente fino que monta a requisição certa e
reaproveita Router + `ChatRequest` inteiramente.

Diferença importante de custo em relação ao pacote ADR-0010..0013 (repo-map, RAG etc.): aquelas
capacidades são baratas (rodam localmente, sem chamada de modelo adicional na maioria dos
casos) e por isso vêm ativadas por padrão. Um Reviewer é uma **segunda chamada completa de
modelo** por tarefa revisada — custo e latência reais, especialmente relevante se o modelo de
revisão for de nuvem. Por isso a decisão aqui é deliberadamente diferente: **desligado por
padrão**, habilitado por tipo de auditoria.

## Decisão

Fica acordada a criação de um componente **Reviewer**, sem infraestrutura nova:

1. Cada **tipo de auditoria** (`correctness`, `security`, `guardrail-compliance`,
   `task-completion` — lista inicial, extensível) é, para efeitos de roteamento, uma
   `task-class` própria (ex.: `review-security`), resolvida pelo Router (MT-09) como
   qualquer outra — o usuário decide, por perfil/projeto, qual modelo audita cada tipo
   (pode ser o mesmo modelo da tarefa original, um maior, ou até um provider diferente).
2. O Reviewer monta a requisição (prompt específico do tipo de auditoria + o artefato a
   revisar + o contexto/instrução original da tarefa) e usa a saída estruturada do ADR-0012
   para obter um veredito em formato previsível (`pass`/`fail` + notas).
3. **Ponto de disparo (v0.1): pós-`Done`.** Após o agent loop (MT-10) terminar com
   `StopReason::Done`, se algum tipo de auditoria estiver habilitado para a `task-class`
   original, o Reviewer roda sobre o resultado final. Revisão pré-execução (antes de rodar
   shell/escrever arquivo) fica fora desta v0.1 — é uma extensão natural, não uma exclusão
   permanente.
4. **Dois modos por tipo de auditoria, configuráveis:**
   - **Advisory (padrão quando habilitado):** o veredito é anexado ao `SessionOutcome`
     (novo campo) e/ou logado — nunca impede a resposta de chegar ao usuário.
   - **Blocking:** um veredito `fail` gera um turno corretivo automático — as notas da
     auditoria voltam ao loop como observação, pedindo nova tentativa — limitado por um
     **teto de retentativas de revisão** (mesma disciplina de limite do `TokenBudget`, para
     nunca loopar indefinidamente); esgotado o teto, a falha persistente é exposta ao
     usuário, não escondida.
5. **Desligado por padrão.** Habilitação e modo (`advisory`/`blocking`) são configuráveis por
   tipo de auditoria e por `task-class`, via extensão do `settings-schema` (mesma camada
   perfil→projeto→env do MT-04).

## Consequências

- **Impacto positivo:** cobre a classe de risco "resultado incorreto/inseguro/incompleto" que
  nenhum mecanismo existente cobre; reaproveita Router + `ChatRequest` + saída estruturada
  quase inteiramente — sem subsistema novo; fecha a lacuna explicitamente deixada em aberto
  pelo ADR-0007.
- **Impacto negativo:** dobra o custo/latência da tarefa quando habilitado; modo `blocking`
  mal configurado (teto de retentativas alto, veredito com falso-positivo frequente) pode
  degradar a experiência ou gerar custo excessivo.
- **Trade-offs aceitos:** *default* desligado (ao contrário do pacote ADR-0010..0013) mitiga o
  risco de custo surpresa; revisão pré-execução adiada para não inflar o escopo desta decisão.

## Diretriz de Conformidade de Código

- **Proibido:** Reviewer rodar sem que o usuário tenha habilitado explicitamente aquele tipo
  de auditoria para a `task-class`; modo `blocking` sem teto de retentativas configurado;
  Reviewer inventar um mecanismo de chamada de modelo próprio — usa exclusivamente
  Router/`ChatRequest` como qualquer outra tarefa.
- **Obrigatório:** cada tipo de auditoria é uma `task-class` roteável independentemente;
  veredito sempre em formato estruturado (ADR-0012); falha persistente após o teto de
  retentativas é reportada ao usuário, nunca suprimida ou apenas logada em silêncio.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
