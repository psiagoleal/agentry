<!-- Caminho relativo: docs/adr/0041-permissoes-por-caminho-e-do-subagente.md -->

# ADR 0041: Permissões escopadas por caminho e conjunto próprio para o subagente

- **Status:** Accepted
- **Data:** 2026-07-28
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** confidencialidade, permissões, subagentes, conformidade

## Contexto

O caso de uso que motiva esta ADR (testado na rodada 8b, `docs/handoff-arquivo.md`): o
mantenedor quer o **Claude como sessão principal** — planejamento e conversa complexa — mas
**sem que ele leia dados confidenciais**. Toda leitura sensível deve ser delegada a um
subagente com modelo **local**, que devolve apenas o resultado já sanitizado.

A fiação dessa arquitetura já funciona (verificado: `cloud-ok` → `destino local-only` →
`cloud-ok`, sem PII chegando ao modelo de nuvem). Faltam duas coisas, ambas confirmadas em
execução:

1. **O agente principal recebe todas as tools**, inclusive `fs_read`. Hoje ele é *instruído*
   a não ler o que é confidencial, não *impedido* — a mesma fragilidade do achado nº 1 da
   rodada 8, só que invertida.
2. **A saída óbvia não funciona.** `register_subagent_tool` (`crates/cli/src/main.rs`) passa
   as **mesmas** `Permissions` ao registry do subagente. Negar `fs_read` para conter o agente
   principal **cega também o subagente** — verificado: com `permissions.deny=["fs_read"]` o
   modelo local passou a inventar o conteúdo do arquivo em vez de lê-lo.

Some-se a isso que o agente principal **precisa** ler parte do repositório para ser útil:
`README.md`, `AGENTS.md`, `docs/`, `skills/`. "Negar `fs_read`" inteiro é grosseiro demais —
a granularidade real do problema é **por caminho**, não por tool.

## Decisão

Ficam acordados **dois mecanismos independentes e combináveis**.

### 1. `subagentPermissions` — conjunto próprio para o subagente

Bloco novo no `settings-schema`, com o mesmo formato de `permissions`. Quando **ausente**, o
subagente herda `permissions` (comportamento atual, nenhuma configuração existente muda).
Quando **presente**, substitui — não soma: um conjunto que "só cresce" não conseguiria
*devolver* ao subagente uma tool negada ao principal, que é justamente o objetivo.

O subagente ter **mais** capacidade que a sessão-mãe é deliberado, e é o mecanismo pelo qual
esta ADR funciona. Não conflita com a ADR-0031 (egresso do subagente nunca mais permissivo
que o da mãe): são eixos diferentes — **ação** sobre a máquina local vs. **alcance de rede**.
A ADR-0031 continua valendo integralmente.

### 2. `readAllow` — escopo de leitura por caminho

Lista de padrões (sintaxe `.gitignore`, via o crate `ignore` já presente — nenhuma dependência
nova, e é a sintaxe que o usuário já conhece de `.agentryignore`) dentro de `permissions` e de
`subagentPermissions`:

```json
{
  "permissions": {
    "readAllow": ["README.md", "AGENTS.md", "docs/**", "skills/**", "*.toml"],
    "deny": ["shell_exec", "glob", "fs_search"]
  },
  "subagentPermissions": { "deny": [] }
}
```

**Allowlist, não denylist** — coerente com o *fail-closed* da ADR-0002: um arquivo
confidencial novo nasce protegido, em vez de exposto até alguém lembrar de bloqueá-lo. Uma
denylist inverteria o ônus e falharia em silêncio no caso que mais importa.

**Ausente ⇒ sem escopo de caminho** (tudo que a tool já permitia continua permitido). Presente
⇒ caminho fora da lista é `Deny`. O *opt-in* é da existência da chave; uma vez declarada, ela
é fechada.

A decisão vive no [`PermissionGate`](../../crates/core/src/tools/permission.rs), único ponto
de estrangulamento por onde `ToolRegistry::execute` já passa — não nas tools de filesystem
individualmente, senão cada tool nova precisaria reimplementar a regra e esquecer seria fácil.
O gate passa a receber a `ToolCall` inteira (não só o nome) para poder inspecionar o argumento
`path`, e normaliza esse argumento pela **mesma** função que a tool usará
(`fs::resolve_within_root`), de modo que `../` e caminho absoluto sejam recusados **antes** da
comparação com os padrões — nunca comparando uma string crua que a tool interpretaria de
outro jeito.

## Consequências

- **Impacto positivo:** o agente de nuvem passa a ser *estruturalmente* incapaz de ler o que
  não foi liberado, em vez de apenas instruído; e continua lendo o que precisa
  (`README.md`/`AGENTS.md`/`docs/`) para ser útil. A proteção deixa de depender da obediência
  de um modelo.
- **Impacto negativo — limitação que esta ADR NÃO resolve, e que precisa ficar explícita:**
  o agente principal ainda pode pedir ao subagente que leia um arquivo confidencial **e
  devolva tudo**. O que esta ADR garante é que **nenhuma leitura confidencial acontece sem
  passar por um modelo local**, que é onde a sanitização pode ser instruída — não que a
  sanitização vá ocorrer. Os achados 1-3 da rodada 8 (guardrails só literais, sem padrão para
  PII) continuam abertos e são o complemento necessário.
- **Cobertura parcial por natureza:** `readAllow` só alcança tools com argumento `path`
  (`fs_read`/`fs_write`/`fs_edit`). `shell_exec` (`cat arquivo`), `glob` e `fs_search` leem ou
  revelam conteúdo por outros caminhos e **não** são cobertos — precisam entrar em `deny` para
  o agente principal. A documentação de usuário deve dizer isso na mesma seção, sem letra
  miúda: um `readAllow` com `shell_exec` liberado é uma falsa sensação de proteção.
- **Trade-offs aceitos:** aceitar que o gate conheça o nome do argumento `path` (leve
  acoplamento à convenção das tools de filesystem) em troca de ter a decisão num ponto único
  e auditável.

## Diretriz de Conformidade de Código

- **Proibido:** implementar a checagem de caminho dentro das tools em vez do
  `PermissionGate`; comparar o argumento `path` cru contra os padrões sem passar por
  `resolve_within_root` (abriria evasão por `../` e por caminho absoluto); fazer
  `subagentPermissions` somar-se a `permissions` em vez de substituir; tratar `readAllow`
  ausente como lista vazia (quebraria toda configuração existente ao negar tudo).
- **Obrigatório:** `readAllow` presente é *fail-closed* — caminho que não casa nenhum padrão
  é `Deny`, e caminho que não resolve dentro da raiz também; a precedência de `deny` sobre
  tudo o mais (ADR-0002) continua valendo, então uma tool em `deny` nunca é liberada por
  `readAllow`; a documentação de usuário precisa declarar quais tools **não** são cobertas.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
