<!-- Caminho relativo: docs/adr/0038-configuracao-global-do-usuario.md -->

# ADR 0038: Configuração global do usuário (`~/.agentry/`)

- **Status:** Accepted
- **Data:** 2026-07-24
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** configuração, credenciais, persistência

## Contexto

Pedido do mantenedor: hoje só existe uma camada de configuração real em runtime —
`.agentry/agentry.settings.json` (por projeto, ADR-0017/0018, git-versionado, distribuído via
`ai-coding-agent-profiles`) — mais variáveis de ambiente. Duas categorias de informação não
se encaixam bem nesse arranjo:

1. **Preferências pessoais reutilizáveis entre projetos** (ex.: um `baseUrl` padrão do
   LiteLLM próprio do usuário) — hoje precisariam ser repetidas em todo `agentry.settings.json`
   de todo projeto, ou forçariam o usuário a versionar uma preferência pessoal num arquivo
   compartilhado com o time.
2. **Credenciais de provider** (chave de API) — quando providers diretos (Anthropic, OpenAI)
   forem implementados, cada um exigirá uma chave pessoal. O padrão já estabelecido para
   `AGENTRY_LITELLM_API_KEY` (variável de ambiente, nunca arquivo git-versionado — comentário
   em `crates/core/src/config/mod.rs:198-200`) continua exigindo que o usuário exporte a
   variável em **toda** sessão de terminal nova, a menos que ele mesmo configure isso no
   `.bashrc`/`.zshrc` — sem nenhum lugar persistente e centralizado pra guardar isso uma vez.

**Verificado antes de decidir, para não contornar uma ADR `Accepted` silenciosamente:** a
ADR-0017 proíbe "usar diretório global do usuário como localização primária de **estado
por-projeto**". Credenciais de provider e preferências pessoais não são estado por-projeto —
são inerentemente por-**usuário** (a chave da Anthropic do mantenedor vale pra qualquer
projeto seu, não é específica de um repositório). Confirmado com o mantenedor: essa leitura
está correta, a ADR-0017 não bloqueia esta decisão — é uma categoria diferente da que ela
mirava (portabilidade de sessão/RAG/audit ao mover uma pasta de projeto).

**Também verificado:** a regra de "segredo nunca no repositório" (`AGENTS.md` §7,
`docs/adr/...` diversas) é especificamente sobre nunca **commitar** um segredo — não "nunca em
nenhum arquivo, nunca". Um arquivo em `~/.agentry/`, fora de qualquer repositório git, não
viola essa regra.

## Decisão

### 1. `~/.agentry/` — dois arquivos, dois propósitos, nunca misturados

- **`~/.agentry/agentry.settings.json`** — **mesmo schema** do arquivo por-projeto (ADR-0018),
  resolvido pela mesma `Settings::from_file`, só que apontando para o diretório *home* em vez
  da raiz do projeto. Guarda preferências pessoais reutilizáveis (ex.: `baseUrl` padrão,
  modelo padrão) — **nunca** chave de API (o tipo `LiteLlmSettings`/futuros
  `AnthropicSettings`/`OpenAiSettings` continuam sem campo de credencial, mesma disciplina de
  hoje).
- **`~/.agentry/credentials.json`** — schema **novo e separado**, só credenciais:
  ```json
  {
    "$schema": "https://agentry.dev/schema/agentry-credentials-schema-1.json",
    "schemaVersion": 1,
    "providers": {
      "litellm": { "apiKey": "..." },
      "anthropic": { "apiKey": "..." },
      "openai": { "apiKey": "..." }
    }
  }
  ```
  Arquivo distinto de propósito **por design** — nunca soma ao mesmo `Settings`/mesma
  struct que é git-versionada por projeto, tornando estruturalmente impossível uma chave de
  API vazar para dentro do arquivo compartilhado por engano (não é só disciplina de
  desenvolvedor, é o *type system* não ter onde colocar o campo).

### 2. Precedência: variável de ambiente sempre vence, arquivo global só é *fallback*

Zero mudança de comportamento pra quem já usa `AGENTRY_LITELLM_API_KEY`: a variável de
ambiente continua tendo a palavra final. `~/.agentry/credentials.json` só é consultado
**quando a variável de ambiente correspondente não está definida** — aditivo, nunca uma
segunda fonte de verdade concorrente.

Para `agentry.settings.json` (preferências, não credenciais), a cadeia de camadas do
`Config::resolve` (hoje só `arquivo < ambiente`, `crates/cli/src/main.rs::build_config`) ganha
uma camada nova **antes** do arquivo de projeto:

```
~/.agentry/agentry.settings.json  <  .agentry/agentry.settings.json (projeto)  <  variável de ambiente
```

Projeto sempre pode sobrescrever uma preferência pessoal global; variável de ambiente
continua sendo o override mais específico (efêmero, por invocação).

### 3. Permissão de arquivo: `0600` em `credentials.json`

Criado com permissão restrita (só o dono lê/escreve) — primeira vez que o `agentry` guarda
segredo em texto plano em disco, então a permissão do arquivo é a única barreira real contra
outro usuário da mesma máquina lendo. Se o arquivo já existir com permissão mais aberta (ex.:
o usuário criou/editou manualmente), o `agentry` avisa (`stderr`) mas não recusa a operação —
mesma filosofia de "avisar, não travar" já usada em outros lugares do projeto.

### 4. Resolução do diretório *home*: sem dependência nova

`$HOME` (Unix) / `%USERPROFILE%` (Windows) via `std::env::var_os` — sem a crate `dirs`/
`directories` (nenhuma dependência nova, ADR-0004). Sem essas variáveis definidas (raro, mas
possível em ambiente restrito/contêiner), a configuração global é simplesmente ausente — cai
nos defaults de sempre, nunca erro fatal (mesmo padrão de "arquivo ausente não é erro" já
usado por `Settings::from_file`/`MemoryStore`/`CheckpointStore`).

## Consequências

- **Positivas:** credencial configurada uma vez, funciona em todo projeto, sem exportar
  variável de ambiente em toda sessão de terminal; preferência pessoal não força o usuário a
  escolher entre repetir em todo projeto ou versionar num arquivo de time.
- **Negativas/riscos aceitos:** mais um lugar onde configuração pode "morar" — precedência
  precisa ficar clara na documentação de usuário (`docs/usuario/`), senão vira fonte de
  confusão ("por que meu `baseUrl` não é o que configurei no projeto?" quando na real o
  projeto está sobrescrevendo de propósito). Arquivo de credencial em texto plano, ainda que
  com permissão restrita, é sempre menos seguro que um *keychain*/*credential manager* do SO —
  aceito como trade-off de simplicidade (nenhuma dependência nova de integração com
  *keychain*), documentado explicitamente no aviso ao gravar a credencial.
- **Fora de escopo:** integração com *keychain*/*credential manager* nativo do SO;
  criptografia do `credentials.json` em repouso; rotação/expiração de credencial.

## Diretriz de Conformidade de Código

- **Proibido:** qualquer struct de configuração git-versionada (schema de
  `agentry.settings.json`, por-projeto **ou** global) ganhar um campo de credencial —
  credencial só existe no schema separado `credentials.json`. Ler `credentials.json` quando a
  variável de ambiente correspondente já está definida (a variável sempre vence, nunca os
  dois somados/mesclados). Sincronizar `~/.agentry/` para fora da máquina local por qualquer
  canal (mesma regra da ADR-0017/ADR-0002, agora também para o diretório global).
- **Obrigatório:** `credentials.json` criado com permissão `0600`; aviso (não erro fatal) se
  encontrado com permissão mais aberta; ausência de `$HOME`/`%USERPROFILE%` ou de qualquer
  arquivo em `~/.agentry/` cai nos defaults, nunca erro fatal.
