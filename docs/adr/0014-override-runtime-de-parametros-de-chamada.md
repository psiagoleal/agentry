<!-- Caminho relativo: docs/adr/0014-override-runtime-de-parametros-de-chamada.md -->

# ADR 0014: Override runtime de parâmetros de chamada (sessão + invocação única)

- **Status:** Accepted
- **Data:** 2026-07-08
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** configuração, cli, router, session, segurança

## Contexto

O ADR-0008 já estabeleceu presets de parâmetros de chamada (`temperature`/`top_p`/
`system_prompt`/`max_tokens`) por `task-class`, resolvidos pelo Router (MT-09) como
`CallPreset`. Duas lacunas ficaram abertas:

1. **`CallPreset` está resolvido mas não é consumido.** `Session::build_request()` (MT-10)
   monta o `ChatRequest` diretamente (`model`/`messages`/`tools`/`max_tokens: None`) sem
   nunca passar pelo `Router::resolve()` nem aplicar `temperature`/`top_p`/`system_prompt` —
   uma lacuna pré-existente a esta discussão, não introduzida por ela.
2. **Nem "reasoning/thinking" nem override em runtime existem.** O usuário quer poder
   configurar um *default* (perfil/projeto/env, mesma camada do MT-04) e também **ajustar em
   tempo real** — por invocação única (flag de CLI) e por sessão interativa (comando no
   estilo `/model` do Claude Code) — não só para *reasoning*, mas para o conjunto mais amplo
   de parâmetros (modelo, `temperature` etc.), com paridade de ergonomia ao Claude CLI.

Isso levanta uma questão de segurança que precisa ser resolvida **nesta** decisão, não
depois: `agentry` já trata classe de egresso e permissões como *fail-closed*, resolvidas uma
única vez na inicialização (ADR-0002, MT-04). Um mecanismo de override em runtime **não pode**
se tornar um caminho para contornar isso — nem por comando explícito mal desenhado, nem (o
risco mais sério) por conteúdo de mensagem/tool-output sendo interpretado como comando de
override (superfície de *prompt injection*).

## Decisão

1. **`CallPreset` (ADR-0008/MT-09) ganha um campo `reasoning`** — representação abstrata
   (ex.: nível ou orçamento de tokens de raciocínio); cada adapter traduz para seu mecanismo
   nativo (Ollama: no mínimo um `think: bool` para modelos que suportam raciocínio, ex.
   Qwen3/DeepSeek-R1; providers futuros ganham granularidade própria sob ADR específico se o
   formato for muito distinto).
2. **Um novo tipo `RuntimeOverride`** carrega os parâmetros de chamada que o usuário pode
   alterar em tempo real: `model`, `provider`, `temperature`, `top_p`, `system_prompt`,
   `max_tokens`, `reasoning`. **Nunca** contém classe de egresso nem permissões/guardrails —
   essas permanecem fixas pela resolução de `Config` (MT-04) feita na inicialização da
   sessão, ponto final.
3. **Precedência** (mais específico vence — mesma filosofia de camadas já usada no MT-04 e no
   ADR-0008): override de **chamada única** (flag de CLI) > override de **sessão** (comando
   REPL, persiste até ser trocado de novo) > preset de `task-class` (ADR-0008/Router) >
   *default* do `settings-schema` (MT-04) > *default* do provider.
4. **Duas superfícies de interação**, ambas no CLI (MT-14): **flags na invocação one-shot**
   (ex.: `agentry --model llama3.1:8b --temperature 0.2 "tarefa"`) para override de chamada
   única; **comandos dentro do REPL** (ex.: `/model`, `/temperature`, `/reasoning`, no estilo
   do `/model` do Claude Code) para override de sessão.
5. **O override nunca contorna o fail-closed:** mesmo com override de `model`/`provider`, a
   resolução final continua passando pela verificação de classe de egresso do Router
   (ADR-0002/MT-09) — se o alvo do override violar a classe ativa da sessão, a chamada é
   bloqueada como qualquer outra, sem exceção.
6. **Override só vem de comando explícito do usuário** (flag de CLI ou comando REPL) —
   **nunca** inferido a partir do conteúdo de uma mensagem, tool-output ou arquivo lido; isso
   fecha a superfície de *prompt injection* mencionada no contexto.

## Consequências

- **Impacto positivo:** paridade de ergonomia com o Claude CLI (configuração em camadas +
  ajuste interativo), sem abrir mão do *fail-closed* de segurança já estabelecido; reaproveita
  a mesma filosofia de camadas usada em três lugares do projeto agora (MT-04, ADR-0008, e
  aqui); força o fechamento da lacuna de item 1 do Contexto (`CallPreset` finalmente
  consumido).
- **Impacto negativo:** mais uma camada de precedência para raciocinar e testar; a separação
  estrita entre "parâmetros de chamada" (*overridable*) e "parâmetros de política" (nunca
  *overridable* em runtime) exige disciplina de código para não vazar.
- **Trade-offs aceitos:** complexidade de precedência em troca da ergonomia interativa
  pedida — mitigado por reaproveitar o mesmo padrão de camadas já validado no MT-04.

## Diretriz de Conformidade de Código

- **Proibido:** `RuntimeOverride` conter classe de egresso ou permissões/guardrails; qualquer
  caminho que aplique override de `model`/`provider` sem passar pela mesma checagem de
  allowlist/classe do Router (ADR-0002/MT-09); inferir override a partir de conteúdo de
  mensagem, tool-output ou arquivo lido — override só vem de flag de CLI ou comando REPL
  explícito.
- **Obrigatório:** a precedência de camadas (item 3 da Decisão) é documentada e coberta por
  teste; um comando de override em REPL confirma a mudança ao usuário (nunca aplica em
  silêncio); campo de override ausente cai no valor da camada abaixo, nunca em erro.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
