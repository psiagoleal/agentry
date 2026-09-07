<!-- Caminho relativo: docs/roadmap-v0.18.md -->

# Roadmap v0.18 — Micro-tickets

Fase K: **teste de integração ponta a ponta**. É o item 1 do handoff desde a rodada 8, e a
justificativa é estrutural, não uma preferência de cobertura: **todo bug de produção
encontrado até hoje neste projeto vive na fronteira `main()` → provider real**, que os 814
testes unitários não alcançam. Só na rodada 8, três defeitos apareceram exclusivamente ao
rodar o binário de verdade (`shell_background` exposto, config MCP temporária órfã, pontuação
consecutiva no detector de PII), e o mais grave de todos — nenhuma tool executava no Ollama
por causa do `structuredOutput` *default* — passou por toda a suíte verde.

A causa é conhecida: o que os testes cobrem são **unidades**; o que quebra é a **fiação**
entre elas — parsing de argumento, carga de configuração, registro no `Router`, laço do
agente, `PermissionGate`, guardrails e audit log, nessa ordem, no processo real.

## Convenções

Mesmas de sempre (`docs/roadmap-v0.1.md` §Convenções): DoD padrão (`cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`), *smoke-test* real do
binário para toda mudança observável, skill `micro-ticket-planner` para granularidade.

**Nenhuma dependência nova.** O provider falso é servidor HTTP sobre `std::net::TcpListener`
com parsing mínimo — introduzir `axum`/`hyper` como dependência direta seria parada dura por
ADR-0004, e um servidor de teste não justifica isso. Precedente direto no repositório:
`fake_mcp_server` e `fake_lsp_server` (`crates/core/src/bin/`), consumidos por
`env!("CARGO_BIN_EXE_<nome>")` a partir de testes de integração.

---

## Fase K — Teste de integração ponta a ponta (MT-140 a MT-146)

### MT-140: ADR-0045 — estratégia de teste ponta a ponta
- **Objetivo:** registrar a decisão antes de qualquer código, porque ela fixa três escolhas
  que depois custam caro para reverter: (a) **provider falso local** em vez de gravação/
  *replay* de tráfego ou de bater em provider real em CI — CI não pode depender de rede,
  credencial ou cota de terceiro, e a matriz de 3 SOs (ADR-0005) multiplicaria qualquer
  flakiness por três; (b) o teste **sobe o binário real** (`CARGO_BIN_EXE_agentry`) como
  subprocesso, e não uma função `run()` interna, porque é exatamente a fronteira do processo
  que os bugs atravessaram; (c) o escopo é a **fiação**, não a lógica de cada unidade, que já
  tem cobertura.
- **Arquivos no escopo:** `docs/adr/0045-*.md` (novo), `docs/adr/README.md`, `mkdocs.yml`,
  `docs/roadmap-v0.18.md` (este arquivo).
- **Critério de aceite:** ADR com os cinco blocos do template; índice atualizado.
- **Fora de escopo:** qualquer código de teste.
- **Depende de:** nenhum.

### MT-141: tornar a raiz de configuração redirecionável em teste
- **Objetivo:** teste ponta a ponta precisa ser **hermético** — não pode ler nem escrever o
  `~/.agentry/` real da máquina (ADR-0038), sob pena de o teste depender do estado do
  desenvolvedor e, pior, de mexer nele. Expor uma variável de ambiente que redirecione a raiz
  de configuração global para um diretório temporário.
- **Arquivos no escopo:** o módulo que resolve `~/.agentry/` (`crates/core/src/config/`),
  mais testes unitários do próprio resolvedor.
- **Critério de aceite:** com a variável apontando para um diretório temporário, nenhuma
  leitura ou escrita ocorre em `~/.agentry/`, verificado no teste.
- **Fora de escopo:** mudar precedência de configuração, formato ou schema.
- **Depende de:** MT-140.
- **Atenção:** é mudança em código de produção para viabilizar teste. Se a variável ficar
  legível fora de teste, vira superfície de configuração nova e **exige emenda à ADR-0038** —
  decidir no MT-140 se ela é pública e documentada, ou restrita a build de teste.

### MT-142: `fake_provider` — provider HTTP falso, roteirizado
- **Objetivo:** binário de teste que responde ao protocolo OpenAI-compatible (o que cobre
  vLLM/LiteLLM/LM Studio pela ADR-0001) com **roteiro determinístico** — a sequência de
  respostas vem de um arquivo JSON passado por argumento, incluindo respostas com
  `tool_calls` e resposta em *streaming* (SSE). Registra as requisições recebidas para o
  teste poder afirmar **o que o `agentry` enviou**, não só o que aceitou de volta.
- **Arquivos no escopo:** `crates/core/src/bin/fake_provider.rs` (novo).
- **Critério de aceite:** teste que sobe o binário, faz uma requisição com `reqwest` (já em
  `dev-dependencies`) e recebe a resposta roteirizada; porta atribuída pelo SO (porta 0) e
  informada em `stdout`, nunca fixa — porta fixa é flaky em CI paralelo.
- **Fora de escopo:** protocolo Anthropic e protocolo Ollama nativo (MT-146).
- **Depende de:** MT-140.

### MT-143: harness ponta a ponta + primeiro caso (conversa sem tool)
- **Objetivo:** primeiro teste que sobe o **binário real** com `HOME`/raiz de config/cwd
  temporários e um `agentry.settings.json` apontando para o `fake_provider`, e afirma sobre
  `stdout` e código de saída. Fecha a fronteira `main()` → provider pela primeira vez.
- **Arquivos no escopo:** `crates/cli/tests/e2e.rs` (novo — o pacote `agentry` **não tem**
  diretório `tests/` hoje), `crates/cli/tests/fixtures/` (novo).
- **Critério de aceite:** `cargo test -p agentry` executa o caso sem rede e sem tocar
  `~/.agentry/`; falha se o binário responder erro de configuração.
- **Fora de escopo:** tool-calling, guardrails, auditoria (MT-144 a MT-146).
- **Depende de:** MT-141, MT-142.

### MT-144: caso de tool-calling — regressão do bug do Ollama
- **Objetivo:** o roteiro faz o modelo pedir uma tool; o teste afirma que o `agentry`
  **executou** a tool sob `PermissionGate` e devolveu o resultado ao modelo. É a regressão
  direta do bug mais grave já encontrado (`structuredOutput` *default* fazendo o modelo
  devolver a tool-call como texto, com nenhuma tool executando). Inclui o caso da tool
  indevidamente exposta (`shell_background`): afirmar **qual é** o conjunto de tools
  oferecido ao modelo, não só que a chamada funciona.
- **Arquivos no escopo:** `crates/cli/tests/e2e.rs`, fixtures.
- **Critério de aceite:** com `structuredOutput` no *default* atual, a tool executa; o
  conjunto de tools enviado ao provider confere com o esperado.
- **Depende de:** MT-143.

### MT-145: casos de egresso, auditoria e guardrail
- **Objetivo:** três afirmações que hoje só existem em teste unitário e nunca ponta a ponta —
  (a) `.agentry/audit.log` registra destino e classe de egresso a cada chamada (ADR-0037);
  (b) classe `local-only` **bloqueia** egresso para nuvem, *fail-closed* (ADR-0002); (c) um
  CPF no prompt é bloqueado/redigido antes de sair, e o log registra nome do detector e
  contagem, **nunca o valor** (ADR-0043) — incluindo o caso de pontuação consecutiva, que foi
  defeito real.
- **Arquivos no escopo:** `crates/cli/tests/e2e.rs`, fixtures.
- **Critério de aceite:** os três casos passam; o teste **grep**a o `audit.log` para garantir
  ausência do valor sensível, não só presença do nome do detector.
- **Depende de:** MT-143.

### MT-146: fiar no CI (e o que fica de fora)
- **Objetivo:** rodar os testes ponta a ponta na matriz de 3 SOs, ou decidir e **registrar**
  um recorte menor. Ponto de decisão real: o `fake_provider` sobe processo e abre socket, o
  que é a parte mais frágil em Windows e macOS; se a matriz completa se mostrar instável, a
  saída honesta é rodar ponta a ponta só em Linux e declarar isso no ADR-0045, não deixar
  teste intermitente na matriz. Inclui a regressão da config MCP temporária órfã (arquivo
  removido ao fim do processo).
- **Arquivos no escopo:** `.github/workflows/ci.yml`, `docs/testing.md`, `docs/adr/0045-*.md`
  (se o recorte mudar).
- **Critério de aceite:** CI verde nos SOs escolhidos, em duas execuções seguidas — uma só
  não distingue teste estável de teste com sorte.
- **Depende de:** MT-143, MT-144, MT-145.

---

## Pontos a verificar antes de começar

Levantados no planejamento e **não confirmados no código** — confirmar no MT-140, porque
mudam o escopo dos tickets seguintes:

1. **Nomes reais das chaves de configuração** (`providers.*.baseUrl`, `structuredOutput`,
   classe de egresso) — o plano usa os nomes como aparecem no handoff e nos ADRs.
2. **Se já existe alguma forma de redirecionar `~/.agentry/`** — se existir, MT-141 encolhe
   para "documentar e usar"; se não existir, é mudança em código de produção (ver a Atenção
   do ticket).
3. **Se o binário aceita um caminho de configuração por argumento** — mudaria o MT-143, que
   hoje presume `cwd` temporário com `agentry.settings.json` local.
