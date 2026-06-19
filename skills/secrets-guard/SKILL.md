---
name: secrets-guard
description: >-
  Aplica os quatro princípios de não-exposição de segredos antes de executar
  qualquer comando de shell, ler arquivos ou produzir saída. Aciona quando a
  tarefa envolver credenciais, variáveis de ambiente, arquivos .env, cofres,
  chaves de API, tokens, certificados, ou inspeção de configuração de
  ferramentas (aws/gcloud/az/kubectl/docker/git config).
---

# secrets-guard — Guardião de segredos

Esta skill codifica a seção *Segurança da Informação no Desenvolvimento Acelerado*
da nota técnica. Vale para os três perfis (empresa, externo-confidencial, pessoal),
com rigor decrescente — mas os princípios são sempre os mesmos.

## Por que isto existe (o "porquê" reduz desvios)

Um vetor frequente e subestimado de vazamento é a execução, **pelo próprio agente**,
de comandos que imprimem segredos no histórico da conversa. Uma vez no histórico, o
segredo pode ser reenviado a provedores externos de modelo, persistido em caches de
prompt, registrado em *traces* de observabilidade, ou copiado inadvertidamente em
*issues*, descrições de PR e mensagens de *commit*.

## Os quatro princípios (obrigatórios)

1. **Nunca executar** comandos cujo objetivo seja, direta ou indiretamente, exibir o
   conteúdo de arquivos `.env`, cofres desbloqueados, *keyrings* do SO, históricos de
   variáveis de ambiente sensíveis, tokens JWT ainda válidos ou trechos de log que
   possam conter chaves, certificados ou *fingerprints* criptográficos.
2. **Avaliar previamente**, antes de invocar qualquer comando, se a saída poderá
   conter material sensível. Em caso de dúvida, **abster-se** e relatar a hesitação ao
   operador humano.
3. **Preferir verificações indiretas** quando for preciso validar uma credencial:
   - testar a autenticação contra um *endpoint* de saúde (`/healthz`, `/me`);
   - conferir apenas o *hash* SHA-256 truncado;
   - expor somente os últimos quatro caracteres mascarados (`****abcd`);
   - executar a chamada-fim e relatar apenas sucesso/insucesso funcional.
4. Quando a inspeção direta for inevitável (ex.: depurar variável malformada),
   **solicitar ao operador humano** que execute o comando localmente, fora da janela do
   agente, e relate apenas o resultado funcional já saneado.

## Comandos proibidos (lista não exaustiva)

Nunca invoque, e bloqueie se solicitado:

```
cat .env            *.env       .env.*          # leitura de arquivos de segredo
env | grep -i (token|key|secret|pass)
printenv            export -p
git config --get-all   git config --list
kubectl get secret ... -o yaml|-o json
aws configure list     aws sts get-...           # quando ecoa credenciais
gcloud config list     gcloud auth print-access-token
az account show        az account get-access-token
docker compose config  docker inspect ...        # quando expõe env de containers
Get-Content secrets.json   cat ~/.aws/credentials   cat ~/.ssh/id_*
```

> Se o operador realmente precisar de um desses valores, **peça que ele rode o comando
> fora do agente** e cole somente o resultado saneado.

## Verificações indiretas recomendadas (faça assim)

```bash
# Em vez de imprimir a chave, confirme só que está presente e o tamanho:
test -n "${API_KEY:-}" && printf 'API_KEY presente (%d chars)\n' "${#API_KEY}"

# Validar credencial pela chamada-fim, sem exibi-la:
curl -fsS -o /dev/null -w '%{http_code}\n' -H "Authorization: Bearer $API_KEY" https://api.exemplo/healthz

# Conferir digest sem revelar o segredo:
printf '%s' "$API_KEY" | sha256sum | cut -c1-12
```

## Armazenamento de segredos (por perfil)

- **empresa:** HashiCorp Vault / OpenBao on-premise, ou AWS Secrets Manager / Azure Key
  Vault / Google Secret Manager, com auditoria de leitura e rotação automática.
- **externo-confidencial:** Doppler, Infisical ou 1Password Secrets Automation; ativar
  opt-out de retenção de dados no provedor de modelo.
- **pessoal:** cofre leve aceitável; `.env` local **apenas** se constar no `.gitignore`
  global da máquina e nos arquivos de exclusão do agente (`.claudeignore` etc.).

Injete credenciais **somente em tempo de execução**, via variáveis de ambiente
carregadas do cofre — nunca materializadas em `.env` versionável.

## Rede de segurança

Recomende ao operador habilitar varredura de segredos pré-commit (`gitleaks`,
`trufflehog`, `detect-secrets`) e *hooks* de pré-execução de comando que bloqueiem os
padrões acima antes mesmo de o modelo invocá-los.

## Definição de pronto da skill

- [ ] Nenhum comando da lista proibida foi executado na sessão.
- [ ] Toda validação de credencial usou verificação indireta.
- [ ] Inspeções inevitáveis foram delegadas ao humano.
