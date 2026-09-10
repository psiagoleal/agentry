<!-- Caminho relativo: usage-test/README.md -->

# `usage-test/` — teste de uso, exemplos e configuração

Esta pasta existe porque a suíte automatizada não responde a uma pergunta: **como é usar o
`agentry` depois do build?** Testes unitários e ponta a ponta afirmam que o código faz o que
foi escrito; nada neles percebe uma ajuda que anuncia atalhos inexistentes, um `--init` que
gera erro fixo, ou um erro que nunca chega à tela. Os três achados do relatório de 2026-09-08
são exatamente disso.

## O que tem aqui

| Arquivo | Para quê |
|---|---|
| [`PLANO-DE-TESTE.md`](./PLANO-DE-TESTE.md) | Roteiro que um agente (ou pessoa) segue para exercitar a TUI de ponta a ponta, com a receita de `tmux` que torna isso possível sem TTY interativo. |
| [`RELATORIO-MODELO.md`](./RELATORIO-MODELO.md) | Molde de relatório. Copie, não edite. |
| `RELATORIO-<data>[-<recorte>].md` | Execuções já feitas. São **registro histórico**: não corrija um relatório antigo quando o defeito for consertado — o conserto é registrado no ticket e no handoff. |
| [`exemplos/`](./exemplos/) | Configurações de referência, versionadas, que **parseiam de verdade** (há teste garantindo). |
| `local/` | Suas configurações reais. **Nunca versionado.** |

## A regra que separa `exemplos/` de `local/`

> **Versione o formato, nunca o alvo.**

Um exemplo mostra *como* declarar um gateway, um perfil ou uma permissão. Ele nunca contém o
endereço de um serviço real, credencial, nome de host interno, usuário ou caminho que revele a
topologia de uma rede — mesmo que o valor não seja secreto por si só. Este repositório é
aberto: um endereço interno commitado é topologia publicada, e não se despublica.

Concretamente:

- **Pode ser versionado:** `exemplos/*.json` com endereços de espaço reservado
  (`gateway-interno.exemplo`, `127.0.0.1`), relatórios de teste, o plano.
- **Não pode:** `baseUrl` de gateway real, chave de API (em qualquer lugar, sob qualquer
  nome), captura de tela que mostre credencial, `credentials.json`.

`local/` está no `.gitignore` e é o lugar de tudo isso. Nenhuma chave vai em arquivo de
configuração de qualquer forma — nem em `local/`: use `agentry --set-credential <provider>`,
que lê de `stdin` (o valor não passa por `argv`, nem pelo histórico do shell) e grava em
`~/.agentry/credentials.json` com permissão `0600`.

Duas redes de segurança independentes cobrem esta regra: o `gitleaks` do CI, que varre todo
*push*, e o teste `crates/core/tests/exemplos_de_configuracao.rs`, que falha se um exemplo
apontar para um host que não seja espaço reservado ou *loopback* — e também se ele deixar de
parsear, porque exemplo que não carrega é pior que exemplo nenhum.

## Como usar um exemplo com um gateway real

```bash
mkdir -p usage-test/local
cp usage-test/exemplos/02-gateway-litellm.json usage-test/local/meu-gateway.json
# edite baseUrl e model em local/ — nunca no exemplo versionado
agentry --set-credential litellm    # a chave é digitada/piped, nunca escrita no JSON

mkdir -p /tmp/prova/.agentry && cd /tmp/prova
cp ~/dev/agentry/usage-test/local/meu-gateway.json .agentry/agentry.settings.json
agentry "responda apenas: funcionou"
```

Rode **fora** do repositório do `agentry`: a busca de configuração sobe a árvore de diretórios
e acharia o `.agentry/` do próprio projeto.

## Variáveis de ambiente reconhecidas

Estas são **todas** as que o `agentry` lê hoje. Não há interpolação de variável dentro do JSON:
`"baseUrl": "${MEU_GATEWAY}"` é lido literalmente, não expandido.

| Variável | Efeito | Precedência |
|---|---|---|
| `AGENTRY_PROFILE` | Perfil ativo: `empresa`, `externo-confidencial` ou `pessoal`. | Camada de ambiente, **vence o arquivo do projeto**. |
| `AGENTRY_MODEL` | Modelo padrão da sessão. | idem |
| `AGENTRY_MAX_TOKENS` | Teto de tokens de saída (inteiro; valor não-vazio e não-numérico é erro explícito). | idem |
| `AGENTRY_LITELLM_BASE_URL` | Endereço do gateway LiteLLM (MT-158). | idem |
| `AGENTRY_LITELLM_MODEL` | Modelo nesse gateway (MT-158). | idem |
| `AGENTRY_LITELLM_API_KEY` | Chave do gateway LiteLLM. | **Vence** `~/.agentry/credentials.json`. |
| `ANTHROPIC_API_KEY` | Chave da Messages API da Anthropic. | idem |
| `NO_COLOR` / `COLORTERM` | Só renderização; não são configuração. | — |

**Variável exportada e vazia conta como ausente** — `export AGENTRY_LITELLM_BASE_URL=` não
sobrepõe o que o arquivo do projeto declarou. Sem isso, um `export` de espaço reservado no
`.zshrc` apagaria silenciosamente a configuração versionada.

**`egressClass` não tem variável, e é deliberado:** endereço é conveniência, classe de egresso é
política. Ajustável por ambiente, daria para afrouxar a classe de um endpoint sem tocar em nada
versionado e sem deixar rastro na revisão.

> **Cuidado com o `.zshrc`.** Como a camada de ambiente vence o arquivo do projeto, uma variável
> exportada lá muda o comportamento de **todos** os projetos, sem que nada na tela explique por
> quê. O caso que já mordeu: com `AGENTRY_PROFILE=externo-confidencial` exportado, um gateway
> declarado `cloud-ok` no `agentry.settings.json` é recusado pelo `Router` e a rota cai para o
> candidato local — o sintoma aparece como erro de rede do Ollama, longe da causa. Para rodar um
> exemplo com a configuração dele mesmo, limpe o ambiente:
> `env -u AGENTRY_PROFILE -u AGENTRY_MODEL -u AGENTRY_LITELLM_BASE_URL agentry "..."`.

Sobre exportar a chave no `.zshrc`: funciona, e **sobrepõe silenciosamente** o que estiver em
`~/.agentry/credentials.json` — se um dia a chave parecer "errada", é o primeiro lugar a olhar.
Prefira `agentry --set-credential <provider>`, que lê de `stdin` (o valor não passa por `argv`
nem pelo histórico do shell) e grava `0600`; uma variável exportada fica em texto claro no
dotfile e é herdada por todo processo que você iniciar.
