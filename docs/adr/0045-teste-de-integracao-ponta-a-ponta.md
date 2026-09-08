<!-- Caminho relativo: docs/adr/0045-teste-de-integracao-ponta-a-ponta.md -->

# ADR 0045: Teste de integração ponta a ponta — provider falso local e binário real

- **Status:** Accepted
- **Data:** 2026-09-07
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** testes, ci, arquitetura, conformidade

## Contexto

Os 814 testes do projeto cobrem **unidades**. O que quebra em produção é a **fiação** entre
elas — parsing de argumento, carga e precedência de configuração, registro no `Router`, laço
do agente, `PermissionGate`, guardrails e audit log, nessa ordem, dentro do processo real.

A evidência é do próprio repositório, não uma preocupação teórica. Todos os defeitos de
produção encontrados até hoje vivem na fronteira `main()` → provider real, e passaram pela
suíte inteira verde:

- **`structuredOutput` *default*** fazia o modelo devolver a *tool-call* como texto — nenhuma
  tool executava na configuração padrão com Ollama. O bug mais grave já encontrado.
- **`shell_background` exposto** ao modelo sem estar previsto no conjunto de tools.
- **Config MCP temporária órfã** — arquivo não removido ao fim do processo.
- **Pontuação consecutiva** quebrando o detector de PII (ADR-0043).

Os três últimos apareceram numa única rodada, e só ao rodar o binário de verdade. Um teste
que exercite essa fronteira é, portanto, a correção de **causa estrutural**; qualquer feature
nova compete com ele em prioridade e perde.

Três caminhos foram considerados para chegar lá:

1. **Bater em provider real em CI.** Rejeitado: acopla o CI a rede, credencial e cota de
   terceiro, e a matriz de três SOs (ADR-0005) multiplicaria por três qualquer instabilidade.
   Um teste que falha por indisponibilidade alheia ensina a equipe a ignorar CI vermelho.
2. **Gravar e reproduzir tráfego** (*record/replay* de HTTP). Rejeitado para a v1: a gravação
   precisa ser feita contra o provider real (o mesmo acoplamento, adiado para o momento de
   regravar), e o artefato gravado envelhece em silêncio — passa a testar o passado sem
   avisar que o presente mudou.
3. **Provider falso local, roteirizado.** Escolhido.

Uma quarta escolha, ortogonal, é **como** invocar o `agentry`: chamar uma função `run()`
interna no mesmo processo, ou subir o binário como subprocesso. A fronteira que os bugs
atravessaram é a do processo — `main()`, argumentos, variáveis de ambiente, `cwd`, código de
saída —, e uma função interna não a exercita.

A decisão precisa ser tomada agora porque ela fixa o formato de todo teste da Fase K
(`docs/roadmap-v0.18.md`, MT-140 a MT-146), e reescrever a estratégia depois de escritos os
casos custa o dobro.

## Decisão

Fica acordado o **teste de integração ponta a ponta com provider falso local, exercitando o
binário real como subprocesso**, nos seguintes termos:

### 1. Provider falso, roteirizado, sem dependência nova

Binário de teste `fake_provider` (`crates/core/src/bin/`) que responde ao protocolo
OpenAI-compatible — o que cobre vLLM, LiteLLM e LM Studio pela ADR-0001 — com sequência de
respostas **determinística**, vinda de um roteiro em JSON. Registra as requisições recebidas,
para o teste poder afirmar **o que o `agentry` enviou**, e não apenas o que aceitou de volta.

Implementado sobre `std::net::TcpListener` com parsing mínimo: adotar `axum`/`hyper` como
dependência direta seria parada dura por ADR-0004, e um servidor de teste não a justifica.
Precedente direto no repositório: `fake_mcp_server` e `fake_lsp_server`, consumidos por
`env!("CARGO_BIN_EXE_<nome>")` a partir de testes de integração.

A porta é atribuída pelo SO (porta 0) e anunciada em `stdout`. **Porta fixa é proibida**: CI
roda casos em paralelo e porta fixa produz falha intermitente, que é pior que ausência de
teste.

### 2. O binário real, como subprocesso

Os casos sobem `env!("CARGO_BIN_EXE_agentry")` e afirmam sobre `stdout`, `stderr` e código de
saída. Nenhum caso da Fase K chama função interna do `agentry` para simular a invocação.

### 3. Hermetismo por `HOME` e `cwd` — sem código de produção novo

Verificado no código (2026-09-07) antes desta decisão, e é o que a torna barata:

- `crates/core/src/global_dir.rs` resolve o diretório *home* por `std::env::var_os("HOME")`,
  com `USERPROFILE` como *fallback*, e é o **único** ponto do workspace que lê essas
  variáveis. Definir `HOME` no processo filho redireciona `~/.agentry/` por inteiro —
  `agentry.settings.json` global (ADR-0038) e `credentials.json`.
- `crates/core/src/state_dir.rs::agentry_settings_path(start)` localiza
  `.agentry/agentry.settings.json` subindo a partir de um caminho dado. Definir o `cwd` do
  processo filho seleciona a configuração de projeto. Não existe — nem passa a existir por
  esta decisão — uma flag `--config`.

Portanto: **nenhuma variável de ambiente nova, nenhuma flag nova e nenhuma emenda à ADR-0038.**
O isolamento usa mecanismo que já existe e já é testado.

O diretório temporário do caso **não pode** estar aninhado sob um diretório que contenha
`.agentry/` — a busca sobe pela árvore e encontraria a configuração do repositório, tornando o
teste dependente de onde foi executado.

### 4. Escopo: a fiação, e as regressões que já custaram caro

Os casos cobrem a costura, não a lógica de cada unidade, que já tem cobertura. Cada defeito
listado no Contexto vira caso de regressão nomeado. Em particular, o conjunto de tools
oferecido ao modelo é **afirmado por inteiro** — não basta verificar que a chamada funciona,
já que o defeito real foi uma tool presente a mais.

Afirmação sobre o audit log verifica a **ausência** do valor sensível no arquivo, não apenas a
presença do nome do detector: o que a ADR-0043 promete é que o log registre `["cpf", 1]` e
nunca o CPF.

### 5. CI, com saída declarada

Os casos rodam na matriz de três SOs da ADR-0005. Se a parte frágil — subir processo e abrir
socket em Windows e macOS — se mostrar instável, **a saída acordada é restringir o teste ponta
a ponta a Linux e declarar o recorte aqui**, nunca conviver com teste intermitente na matriz.
Um teste que falha às vezes destrói a confiança em todos os outros.

Estabilidade é atestada por **duas execuções verdes seguidas**; uma só não distingue teste
estável de teste com sorte.

## Consequências

- **Impacto positivo:** a classe de defeito que hoje só aparece em uso real passa a ser
  detectável em CI, sem rede, credencial ou cota. O custo de manutenção é próprio do
  repositório — o roteiro é um arquivo JSON, não um serviço externo. O hermetismo sai de
  graça, por mecanismo já existente.
- **Impacto negativo:** o `fake_provider` é uma segunda implementação do protocolo, que pode
  divergir do comportamento real de um provider e dar falsa confiança — divergência que só
  aparece quando um provider real muda. Testes que sobem processo e abrem socket são mais
  lentos e mais frágeis que teste unitário, e o custo recai sobre toda execução de CI.
- **Trade-offs aceitos:** abre-se mão de fidelidade ao provider real em troca de determinismo
  e independência de rede. A fidelidade continua sendo obtida pelo *smoke-test* manual do
  binário a cada mudança observável, que a Fase K **não** substitui.

## Diretriz de Conformidade de Código

- **Proibido** que teste automatizado dependa de rede externa, credencial real ou cota de
  provider de terceiro.
- **Proibido** porta de rede fixa em teste; a porta é sempre atribuída pelo SO e anunciada
  pelo processo.
- **Proibido** que teste leia ou escreva o `~/.agentry/` do usuário: `HOME` e `cwd` do
  processo filho são sempre redirecionados para diretório temporário.
- **Proibido** resolver diretório *home* fora de `crates/core/src/global_dir.rs` — é essa
  centralização que torna o hermetismo possível, e furá-la quebra o isolamento em silêncio.
- **Obrigatório** que todo defeito encontrado na fronteira `main()` → provider real vire caso
  de regressão nomeado na suíte ponta a ponta, antes de ser fechado.
- **Obrigatório** afirmar o conjunto de tools oferecido ao modelo por inteiro, e a **ausência**
  de valor sensível no audit log, e não apenas a presença do que se espera.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
