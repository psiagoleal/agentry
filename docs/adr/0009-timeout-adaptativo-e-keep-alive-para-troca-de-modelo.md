<!-- Caminho relativo: docs/adr/0009-timeout-adaptativo-e-keep-alive-para-troca-de-modelo.md -->

# ADR 0009: Timeout adaptativo e `keep_alive` configurável para troca de modelo em provider local

- **Status:** Accepted
- **Data:** 2026-07-07
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** confiabilidade, providers, router, transporte, configuração

## Contexto

Um uso previsto do `agentry` é **100% local com Ollama** (perfil `empresa`/`local-only`,
sem nenhum egresso de nuvem). O Ollama carrega e descarrega modelos em memória conforme o
uso recente; quando o Router (MT-09) resolve, para uma nova `task-class`, um modelo
**diferente** do último usado naquele mesmo provider, o Ollama precisa fazer um *cold load*
— que pode levar bem mais tempo que uma inferência já aquecida. Hoje (auditado antes deste
ADR): o `Transport` (MT-07) constrói `reqwest::Client::new()` **sem nenhum timeout
configurado** — não por decisão, mas por omissão — e o `OllamaProvider` (MT-08) não envia o
parâmetro `keep_alive` da API do Ollama, então vale o default do próprio Ollama (5 minutos),
o que pode causar descarregamentos desnecessários em alternância frequente de `task-class`.

Um timeout único e fixo para todas as chamadas força uma escolha ruim: generoso demais
mascara conexões genuinamente travadas (contrária à prioridade de confiabilidade do
projeto); agressivo demais gera falhas espúrias exatamente nas trocas de modelo legítimas.
O `agentry` está em posição única para saber **antecipadamente** quando uma troca vai
acontecer, porque é ele quem decide, via Router, qual `(provider, modelo)` chamar a seguir —
essa informação não está disponível para o transporte nem para o provider isoladamente.

## Decisão

Fica acordado que:

1. O **Router** (MT-09) passa a rastrear, por provider, o último modelo resolvido
   (`provider → modelo`) e sinaliza em [`ResolvedRoute`] se a resolução atual implica
   **troca de modelo** naquele provider (`is_model_switch: bool`). O rastreio é otimista:
   assume que toda resolução será de fato usada para uma chamada.
2. O **Transporte** (`Transport::post_json`/`post_json_lines`) passa a aceitar um
   **timeout por chamada**, via o mecanismo nativo do `reqwest` (`.timeout()` no builder da
   requisição) — o timeout do `Client` construído internamente continua sendo o *fallback*
   quando nenhum override é passado.
3. O **adapter Ollama** (MT-08) usa `is_model_switch` para escolher entre um timeout
   "frio" (troca de modelo) e um timeout "quente" (mesmo modelo), e envia o parâmetro
   `keep_alive` em toda chamada — evitando descarregamento desnecessário do modelo entre
   chamadas frequentes.
4. Timeout frio, timeout quente e `keep_alive` são **configuráveis pelo usuário** via
   extensão do `settings-schema` (mesma camada perfil→projeto→env do MT-04); ausência cai
   em *defaults* conservadores definidos no `agentry` (valores exatos a definir no ticket
   de implementação).
5. Escopo desta v0.1: o sinal `is_model_switch` é consumido apenas pelo adapter Ollama
   (único provider local real hoje) — mas o campo no Router é genérico o bastante para
   outros providers locais reaproveitarem depois. Providers de API gerenciada (Anthropic,
   OpenAI-compatible) não têm esse problema e não recebem tratamento especial aqui.

## Consequências

- **Impacto positivo:** ataca a causa raiz (menos load/unload via `keep_alive`) e reduz
  falsos timeouts em troca de modelo, sem sacrificar o *fail-fast* em chamadas quentes;
  parâmetros configuráveis por perfil/projeto, coerente com ADR-0007/0008.
- **Impacto negativo:** o Router ganha estado mutável (antes era decisão pura, sem efeito
  colateral) — exige disciplina de concorrência (`Mutex`), já que o Router deve ser
  compartilhável entre chamadas concorrentes; o Transporte ganha mais uma dimensão de
  configuração por chamada.
- **Trade-offs aceitos:** mais uma seção de `settings-schema` para manter; a heurística de
  "troca" é otimista — no pior caso produz um timeout um pouco mais generoso ou mais curto
  do que o ideal, nunca incorreto a ponto de comprometer egresso/segurança (isso continua
  garantido pela allowlist, ADR-0002, independentemente do timeout escolhido).

## Diretriz de Conformidade de Código

- **Proibido:** hardcode de timeout ou `keep_alive` sem caminho de configuração; qualquer
  lógica de detecção de troca de modelo duplicada fora do Router (ex.: um provider
  tentando adivinhar sozinho se está trocando de modelo); qualquer módulo além de
  `transport/mod.rs` construir seu próprio `reqwest::Client` ou aplicar timeout por fora do
  Transporte (mantém o invariante do MT-07).
- **Obrigatório:** timeout por chamada implementado via API nativa do `reqwest`, dentro do
  Transporte; `is_model_switch` calculado pelo Router **antes** de qualquer chamada de
  rede; `keep_alive`/timeouts configuráveis via `settings-schema`, com *defaults*
  conservadores documentados no código; mudança de esquema correspondente registrada no
  `exchange-log` (`docs/interop/exchange-log.md`).

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
