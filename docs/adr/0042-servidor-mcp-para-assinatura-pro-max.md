<!-- Caminho relativo: docs/adr/0042-servidor-mcp-para-assinatura-pro-max.md -->

# ADR 0042: Servidor MCP — tools do `agentry` para a assinatura Pro/Max

- **Status:** Accepted
- **Data:** 2026-07-28
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** providers, mcp, confidencialidade, permissões

## Contexto

A ADR-0040 registrou o provider `claude-cli` (assinatura Claude Pro/Max via *spawn* de
`claude -p`) com uma limitação estrutural declarada: **não faz tool-calling**. `claude -p` é
um agente completo, com laço e ferramentas próprios; deixá-lo usar as ferramentas dele faria
as edições escaparem do `PermissionGate`/*checkpoints*/auditoria do `agentry`, então a
implementação desligou as ferramentas embutidas (`--tools ""`) e usou o subprocesso como
gerador de texto puro. A própria ADR-0040 registrou a ponte MCP como trabalho futuro.

A ADR-0041 entregou o mecanismo que faltava do lado da política: `readAllow` (escopo de
leitura por caminho) e `subagentPermissions` (conjunto próprio do subagente). Com eles, o
agente principal pode ler `README.md`/`AGENTS.md`/`docs/` e ser **estruturalmente incapaz** de
abrir os arquivos confidenciais, delegando essas leituras a um modelo local. Falta apenas o
transporte: **como fazer o `claude -p` chamar as tools do `agentry`.**

Três incógnitas foram resolvidas experimentalmente antes desta decisão, com um servidor MCP
descartável e o `claude` real:

1. **`--tools ""` desliga as ferramentas embutidas mas preserva as de MCP.** Com
   `--mcp-config` + `--strict-mcp-config`, a única ferramenta visível na sessão foi
   `mcp__agentry__<nome>`. É a propriedade central: o Claude passa a ter **exatamente** o que
   o `agentry` expõe, e nada mais.
2. **Em modo *headless* a aprovação não é interativa** — sem `--allowedTools`, o modelo
   responde pedindo permissão e a tarefa não avança. Listar os nomes prefixados em
   `--allowedTools` resolve.
3. **O ciclo completo funciona:** a tool foi chamada com os argumentos certos e o resultado
   voltou para a resposta final do modelo.

## Decisão

Fica acordado um **modo servidor MCP** no próprio binário: `agentry --mcp-server`, falando
JSON-RPC sobre *stdio*, expondo o `ToolRegistry` já montado pela configuração do projeto —
**as mesmas tools, sob o mesmo `PermissionGate`**, incluindo `readAllow`/`subagentPermissions`
da ADR-0041.

**Expor o registry inteiro, não apenas o `subagent`.** A intuição inicial era expor uma única
ferramenta de delegação, tornando o Claude incapaz de tocar em arquivo. A ADR-0041 tornou isso
desnecessário e pior: com `readAllow`, o agente principal já é contido por caminho, e limitá-lo
a delegar *tudo* — inclusive ler o `README.md` — o deixaria inútil para planejar, que é
justamente o papel dele nesta arquitetura. A contenção passa a ser responsabilidade da
política (ADR-0041), não da ausência de transporte.

**Reuso do `rmcp` (ADR-0028), feature `server`.** O projeto já adotou `rmcp` para o lado
cliente; escrever um servidor JSON-RPC à mão enquanto se depende de uma biblioteca MCP seria
incoerente. Verificado antes de decidir: a feature `server` puxa `schemars`, que **já está na
árvore** — não é dependência nova, mesmo raciocínio da feature `io-util` do `tokio` na
ADR-0040.

**Fiação no `ClaudeCliProvider`.** Quando `providers.claudeCli.mcpTools` está ativo, o provider
passa a `claude -p`: `--mcp-config` (arquivo temporário apontando para `agentry --mcp-server`),
`--strict-mcp-config` (ignora qualquer MCP do usuário — a sessão vê só o que o `agentry`
declarou) e `--allowedTools` com os nomes prefixados. Mantém `--tools ""`: as ferramentas
embutidas continuam desligadas.

### Arquitetura resultante e o que ela **não** cobre

```
agentry            (sessão do usuário; ClaudeCliProvider)
 └─ claude -p      (laço de agente, assinatura Pro/Max)
     └─ agentry --mcp-server   (tools sob PermissionGate/readAllow/auditoria)
```

Consequência que precisa ficar explícita: **o laço interno de tool-calling pertence ao Claude
Code**, mas a `Session` do `agentry` continua existindo em volta dele — o `claude -p` é
invocado *de dentro* de `Session::run_streaming`, como qualquer outro provider.

O que **continua valendo** (verificado em execução, não deduzido):

| Mecanismo | Vale? | Por quê |
|---|---|---|
| Histórico de sessão, `/save`, `--resume` | ✅ | A `Session` registra as mensagens normalmente; o arquivo em `.agentry/session/` sai com `provider`/`model`/`task_class` e uso de tokens. |
| Guardrails de conteúdo (ADR-0007) | ✅ | `Session::run_streaming` aplica entrada **e** saída também neste caminho. |
| Guardrails no **subagente** | ✅ | `SubagentTool` herda o par gate/*sink* da sessão-mãe (`with_guardrails`). |
| Permissões, `readAllow`, *checkpoints*, audit log | ✅ | São por tool, e toda tool passa pelo servidor MCP → `PermissionGate`. |
| `/compact` (ADR-0016) | ✅ | Opera sobre `Session.messages`, que existe. |

O que **não** vale:

- **Nenhuma inspeção do que trafega *dentro* do `claude -p`.** Os tool-calls que ele executa
  via MCP acontecem em outro processo; a `Session` só vê o texto final. Consequência prática:
  o histórico salvo **não** registra esses tool-calls (com o provider `anthropic`, registra),
  e os guardrails não inspecionam esse tráfego intermediário — só a mensagem do usuário e a
  resposta final.
- **Teto de turnos (ADR-0033) não entra em ação** — não por estar desligado, mas porque a
  `Session` observa zero turnos com tool-call: eles são internos ao subprocesso. Quem limita
  ali é o próprio Claude Code.

Isto foi corrigido depois de a primeira versão desta ADR afirmar, sem verificar, que
guardrails/compactação "não se aplicam". Aplicam-se — a fronteira real é entre **o que a
`Session` observa** e **o que roda dentro do subprocesso**, não entre "por tool" e "por
turno".

## Consequências

- **Impacto positivo:** a assinatura Pro/Max passa a servir como agente principal com
  tool-calling real, contida pela política da ADR-0041; fecha a pendência aberta na ADR-0040.
- **Impacto negativo:** três processos encadeados (mais superfície de falha e diagnóstico mais
  difícil); o processo servidor precisa resolver a mesma configuração do processo-pai, e uma
  divergência entre os dois seria silenciosa; o formato `stream-json`/MCP do Claude Code segue
  sendo superfície de terceiro.
- **Trade-offs aceitos:** abrir mão dos mecanismos por turno da `Session` (guardrails, teto de
  turnos, compactação) em troca de usar a assinatura em vez de chave de API — desde que essa
  perda esteja documentada onde o usuário escolhe o provider, não em letra miúda.

## Diretriz de Conformidade de Código

- **Proibido:** expor pelo servidor MCP qualquer tool que não passe pelo `PermissionGate`
  (seria uma porta lateral em volta da ADR-0041); omitir `--strict-mcp-config` (a sessão
  herdaria servidores MCP do usuário, fora de qualquer política do `agentry`); reabilitar as
  ferramentas embutidas do Claude Code junto do MCP; gerar o arquivo de `--mcp-config` em
  local previsível/compartilhado ou deixá-lo para trás após a execução.
- **Obrigatório:** o modo `--mcp-server` resolve a configuração pelas **mesmas** camadas do
  modo normal (`~/.agentry/` < projeto < ambiente), nunca um caminho próprio; falha de
  *spawn*, handshake ou JSON malformado vira erro tratado e reportado, nunca `panic` nem
  silêncio; a documentação de usuário declara, junto da configuração do provider, quais
  garantias **não** valem nesta arquitetura.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
