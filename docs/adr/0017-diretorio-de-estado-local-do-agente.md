<!-- Caminho relativo: docs/adr/0017-diretorio-de-estado-local-do-agente.md -->

# ADR 0017: Diretório de estado local por projeto (`.agentry/`) para memória, histórico e índices

- **Status:** Accepted
- **Data:** 2026-07-10 (emendado em 2026-07-12 — ver nota abaixo)
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** dados, persistência, portabilidade

> **Nota de revisão (2026-07-12):** a auto-exclusão do git (item 2) ganhou uma exceção — o
> artefato de configuração do `agentry` (ADR-0018) também vive em `.agentry/`, mas
> **precisa** ser versionado (é política distribuída pelo `ai-coding-agent-profiles`, não
> estado privado da máquina). Sem essa exceção, um `.gitignore` cego (`*`) o esconderia do
> git incondicionalmente.

## Contexto

Hoje o `agentry` não persiste nada em disco. O audit log (`crates/cli/src/main.rs`,
`StderrAuditSink`) só emite em `eprintln!`, com comentário explícito no código adiando
persistência estruturada "para quando houver demanda concreta". `Session::messages`
(`crates/core/src/session/mod.rs`) é um `Vec<Message>` puramente em memória —
`Session::compact` (MT-36) reescreve esse vetor, mas nunca grava em disco. Os dois índices
RAG são construídos inteiramente em memória: `LexicalIndex` (MT-26) via
`tantivy::Index::create_in_ram`, `SemanticIndex` (MT-27) via `lancedb::connect("memory://")` —
ambos recomeçam do zero a cada novo processo. `Settings` (ADR-0003) resolve só de variáveis de
ambiente, sem descoberta de arquivo. Não existe nenhuma dependência de diretório (`dirs`/
`directories`/`xdg`) no workspace.

Isso já é uma lacuna prática, não só filosófica: o **MT-29** (indexação incremental) promete
"reembedar/reindexar só arquivos alterados... nunca o repositório inteiro" — mas sem os
índices sobreviverem entre invocações do processo, "incremental" só faria sentido dentro de
uma única sessão de processo longa (o REPL), não no uso mais comum da CLI (`agentry
"<tarefa>"` one-shot, MT-14), que reinicia o processo a cada chamada. A decisão de **onde**
persistir precisa vir antes de qualquer ticket que implemente persistência de fato.

Padrão comum de agentes de codificação — inclusive esta própria sessão do Claude Code, cuja
memória/histórico vive em `~/.claude/projects/<hash-do-caminho>/` — é armazenar esse estado
numa pasta ligada ao caminho absoluto do projeto. Isso quebra silenciosamente se o diretório
for renomeado, movido ou copiado: a "memória" fica órfã, presa ao caminho antigo, e uma cópia
do projeto não carrega o histórico junto. O requisito é armazenamento **local ao projeto**,
para que renomear/mover/copiar o projeto preserve memória/histórico automaticamente, sem
lógica de re-vínculo.

O alvo de homologação corporativa (ADR-0002) já trata confidencialidade como requisito, não
feature opcional — qualquer diretório de estado novo precisa nascer com uma garantia clara de
que nunca vaza para o histórico do git nem sai da máquina, mesmo sendo uma decisão sobre disco
local, não sobre rede.

## Decisão

Fica acordado que o `agentry` persiste seu próprio estado (memória de sessão, índices RAG,
audit log estruturado — quando cada um for implementado) num diretório **local ao projeto que
está sendo operado**, nunca num diretório global do usuário (`~/.config`, `~/.cache`,
`~/.agentry` etc.) como localização primária.

1. **Raiz do estado:** `<raiz>/.agentry/`, onde `<raiz>` é o primeiro ancestral do diretório
   de trabalho que contém `.git` (arquivo ou diretório — cobre *worktrees*), subindo a partir
   do cwd; sem `.git` em nenhum ancestral, `<raiz>` é o próprio cwd. Mesma técnica de
   descoberta de raiz que o próprio git usa — funciona corretamente em monorepo/subdiretório
   sem caso especial.
2. **Auto-exclusão do git, com uma exceção nomeada:** na primeira escrita, o `agentry` cria
   `.agentry/.gitignore` com o conteúdo `*` seguido de uma exceção explícita por nome de
   arquivo para cada artefato de **política** que **deve** ser versionado — hoje só
   `!agentry.settings.json` (ADR-0018). Uma segunda exceção puramente técnica,
   `!.gitignore`, também é necessária: sem ela, a regra `*` se aplicaria ao próprio arquivo
   que a contém, e `git add` descartaria o `.gitignore` em silêncio (achado real ao
   distribuir o mesmo conteúdo pelo `ai-coding-agent-profiles`, ADR-0006 daquele repo) — não
   é um segundo artefato de política, só a mecânica para a primeira exceção funcionar de
   fato. O diretório continua se autoexcluindo do controle de versão por padrão, sem nunca
   tocar no `.gitignore` do projeto (arquivo que o `agentry` não é dono); toda exceção é
   sempre por nome de arquivo específico, nunca um padrão amplo que arrisque expor estado
   privado (sessão, índices, audit log) por engano. Como as tools de leitura já existentes
   (`fs.rs` do MT-12, `repo_map.rs` do MT-21) usam a crate `ignore`, que respeita
   `.gitignore` por padrão, o resto de `.agentry/` continua saindo de graça de
   qualquer varredura de repo-map/RAG — nenhuma tool precisa de caso especial.
3. **Layout reservado**, criado por quem precisar, não todo de uma vez — esta ADR não
   implementa a maioria, só reserva o espaço para que tickets futuros não reabram a decisão
   nem inventem uma raiz paralela:
   - `.agentry/agentry.settings.json` — **artefato de política, versionado** (ADR-0018) —
     única exceção à auto-exclusão do item 2. Distribuído pelo `ai-coding-agent-profiles`,
     consumido por `Settings::from_file` (MT-39).
   - `.agentry/index/` — índices RAG persistidos (tantivy do MT-26, lancedb do MT-27) —
     substitui `create_in_ram`/`memory://` quando a persistência for implementada.
   - `.agentry/session/` — histórico de sessão persistido, se/quando um ticket futuro
     decidir implementar retomada de sessão.
   - `.agentry/audit.log` — log estruturado de egresso persistido, se/quando decidido
     substituir/complementar o `StderrAuditSink` atual.
4. **Portabilidade:** como tudo vive dentro de `<raiz>`, renomear/mover/copiar o diretório do
   projeto preserva memória/histórico automaticamente — não há re-vínculo a fazer, ao
   contrário de um esquema chaveado por caminho absoluto.
5. Configuração (`Settings`, ADR-0003) tem sua localização de artefato definida por esta ADR
   (item 3) — o formato/schema exato é definido separadamente pela ADR-0018, que também
   decide a precedência de camadas (perfil < arquivo < variável de ambiente).

## Consequências

- **Impacto positivo:** memória/histórico portáteis junto com o projeto; zero-configuração (o
  usuário nunca precisa tocar em `.gitignore` nem criar a pasta manualmente); reaproveita o
  mecanismo de `.gitignore` já respeitado pelas tools existentes (MT-12/MT-21), sem duplicar
  lógica de exclusão; desbloqueia o MT-29 fazer sentido de fato entre invocações de processo.
- **Impacto negativo:** mais um diretório "escondido" na raiz de cada projeto operado pelo
  `agentry` (ainda que autoexcluído do git); resolução de raiz por busca ascendente adiciona
  uma pequena chamada de I/O (checar cada ancestral) no caminho de inicialização.
- **Trade-offs aceitos:** nenhuma opção de estado global, nem como *fallback* — se o usuário
  roda o `agentry` fora de qualquer diretório de projeto reconhecível, o estado nasce e morre
  no cwd atual, sem acumular histórico entre pastas temporárias diferentes; considerado
  aceitável porque o caso de uso central é "operar sobre um projeto", não "conversa avulsa sem
  projeto".

## Diretriz de Conformidade de Código

- **Proibido:** qualquer subsistema (sessão, índices RAG, audit log, cache futuro) persistir
  estado fora da raiz resolvida por este mecanismo; usar diretório global do usuário como
  localização primária de estado por-projeto; modificar diretamente o `.gitignore` do projeto
  para excluir `.agentry/` — a exclusão é sempre via `.agentry/.gitignore` próprio; expor ou
  sincronizar o conteúdo de `.agentry/` para fora da máquina local por qualquer canal (mantém
  o espírito do ADR-0002, mesmo sendo uma decisão de disco, não de rede); adicionar uma
  exceção de auto-exclusão que não seja um nome de arquivo exato (padrões amplos arriscam
  expor estado privado por engano).
- **Obrigatório:** resolver a raiz via busca ascendente por `.git`, com *fallback* para o cwd;
  garantir `.agentry/.gitignore` (conteúdo `*` + exceções nomeadas, hoje só
  `agentry.settings.json`) antes de qualquer outra escrita no diretório; qualquer novo
  subsistema de persistência usa um subdiretório próprio dentro da raiz resolvida, nunca uma
  raiz paralela.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
