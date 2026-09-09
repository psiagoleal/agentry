<!-- Caminho relativo: usage-test/exemplos/README.md -->

# Exemplos de configuração

Configurações de referência, versionadas. Cada uma **parseia de verdade** — o teste
`crates/core/tests/exemplos_de_configuracao.rs` carrega todas e falha se alguma quebrar, ou se
alguma apontar para um host que não seja espaço reservado ou *loopback*.

| Arquivo | Cenário |
|---|---|
| [`01-ollama-local.json`](./01-ollama-local.json) | Só modelo local. Nada sai da máquina. |
| [`02-gateway-litellm.json`](./02-gateway-litellm.json) | Gateway LiteLLM (corporativo ou próprio). |
| [`03-nuvem-planeja-local-le.json`](./03-nuvem-planeja-local-le.json) | O agente de nuvem planeja, o modelo local lê o que é confidencial (ADR-0041). |
| [`04-permissoes-e-guardrails.json`](./04-permissoes-e-guardrails.json) | Confirmação, bloqueio de tool e regras de conteúdo. |

## Como usar

Copie para `../local/` (que não é versionado), edite lá, e rode **fora** do repositório do
`agentry` — a busca de configuração sobe a árvore e acharia o `.agentry/` do próprio projeto.

```bash
cp exemplos/02-gateway-litellm.json local/meu-gateway.json
$EDITOR local/meu-gateway.json          # baseUrl e model reais vão só aqui

mkdir -p /tmp/prova/.agentry && cd /tmp/prova
cp ~/dev/agentry/usage-test/local/meu-gateway.json .agentry/agentry.settings.json
agentry "responda apenas: funcionou"
```

## Onde entram as variáveis de ambiente

**Chave de API nunca vai no JSON**, em nenhum destes exemplos — não há campo para isso. Ela vem
de uma das duas vias, nesta ordem de precedência:

1. `AGENTRY_LITELLM_API_KEY` / `ANTHROPIC_API_KEY` no ambiente;
2. `~/.agentry/credentials.json`, gravado por `agentry --set-credential <provider>`.

A variável de ambiente **vence** quando as duas existem. Preferir `--set-credential` tem uma
razão prática: ele lê de `stdin` (o valor não passa por `argv` nem pelo histórico do shell) e
grava com permissão `0600`, enquanto uma variável exportada no `.zshrc` fica em texto claro num
arquivo de configuração pessoal e é herdada por **todo** processo que você iniciar.

O que **não** existe hoje: interpolação de variável dentro do JSON (`"baseUrl": "${MEU_GATEWAY}"`
é lido literalmente, não expandido) e variável de ambiente para `baseUrl`. É por isso que
`02-gateway-litellm.json` traz um endereço de espaço reservado em vez de ser utilizável direto
— ver **MT-158** no `docs/roadmap-v0.18.md`.
