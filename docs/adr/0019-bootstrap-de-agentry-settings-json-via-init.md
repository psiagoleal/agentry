<!-- Caminho relativo: docs/adr/0019-bootstrap-de-agentry-settings-json-via-init.md -->

# ADR 0019: Bootstrap de `.agentry/agentry.settings.json` via `--init`/`/init`

- **Status:** Proposed
- **Data:** 2026-07-13
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** bootstrap, cli, interop, segurança, egresso

## Contexto

MT-39/MT-40 fecharam o loop de **ler** `.agentry/agentry.settings.json` (ADR-0018): hoje, na
ausência do arquivo, o `agentry` usa os *defaults* de cada ADR de origem (todas as 4 flags
`true`, permissões vazias) — uma sessão efêmera, só em memória, sem nenhum estado
persistente. Isso é uma escolha deliberada (ADR-0017), mas não existe hoje **nenhum caminho
para criar o arquivo de fato** — quem quiser sair do modo efêmero precisa escrevê-lo à mão.

Ao discutir esse gap, surgiu a proposta de um comando de bootstrap (`--init` na CLI, `/init`
no REPL) que também integrasse com o repositório irmão `ai-coding-agent-profiles` (público),
para quem quiser os valores *default* já diferenciados por perfil, sem precisar cloná-lo
lado a lado. Duas abordagens foram consideradas e descartadas antes da decisão final:

1. **`curl <script> | sh`** — padrão comum de bootstrap de CLIs, mas um anti-padrão de
   *supply chain* conhecido: sem *pinning*/verificação de integridade, execução de shell
   arbitrário sem revisão prévia, e um vetor de comprometimento caso a URL não esteja fixada
   a uma referência imutável. Descartado.
2. **Buscar só o JSON diretamente, fora do `Transport`** — mais seguro que (1) (não executa
   nada), mas, ao revisar contra os ADRs `Accepted` (disciplina exigida pela skill
   `adr-writer` antes de qualquer mudança funcional), verificou-se que isso **viola
   literalmente** a Diretriz de Conformidade da ADR-0002: *"proibido qualquer chamada de rede
   fora do módulo de transporte central"* — sem nenhuma exceção prevista para bootstrap. Como
   a ADR-0002 é `Accepted` (imutável), a resposta correta não é abrir uma exceção nova nem
   contorná-la em silêncio, e sim verificar se a decisão já em vigor comporta o caso — e
   comporta: `Transport::new` já aceita uma `Allowlist`/`EgressClass` próprias por instância,
   então o fetch de bootstrap pode simplesmente **passar pelo mesmo módulo**, só que numa
   instância dedicada. Nenhuma emenda à ADR-0002 é necessária.

## Decisão

Fica acordado que o `agentry` ganha dois pontos de entrada equivalentes — a flag `--init`
(CLI, modo *one-shot*) e o comando `/init` (REPL) — para materializar
`.agentry/agentry.settings.json`, reaproveitando `state_dir::ensure_state_dir`/
`agentry_settings_path` (MT-38/39) para localizar a raiz e criar o `.gitignore` já existente:

1. **Sem `--profile`:** cria só o exemplo genérico já documentado na ADR-0018 §5 (schema
   mínimo, todas as flags `true`, permissões vazias) diretamente em disco — **nenhuma
   chamada de rede**. É o modo *default*, sempre funciona, e é a primeira forma real de sair
   do modo "sessão efêmera em memória" (ADR-0017) com um comando só.
2. **Com `--profile <empresa|externo-confidencial|pessoal>`:** busca o
   `agentry.settings.json` real daquele perfil, publicado no repositório público
   `ai-coding-agent-profiles`, via **um único GET HTTPS do arquivo JSON — nunca um script
   para executar**. Não há, em nenhuma circunstância, `curl | sh` nem qualquer forma de
   execução de código obtido pela rede (ver item 8).
3. **A busca passa pelo `Transport` central, não ao redor dele:** uma instância de
   `Transport` dedicada ao bootstrap, com `Allowlist` restrita a um único host fixo (o
   domínio de conteúdo bruto do GitHub) e `EgressClass::CloudOk` — nem a classe de egresso do
   perfil-alvo, nem herdada de nenhuma sessão (ainda não existe nenhuma resolvida nesse ponto
   do processo); é uma decisão isolada, própria do comando de bootstrap, auditada pelo mesmo
   audit log de qualquer outra chamada (ADR-0002). Isso cumpre a Diretriz de Conformidade da
   ADR-0002 ao pé da letra — o próprio módulo de transporte central, com allowlist explícita
   — em vez de abrir uma exceção a ela.
4. **Pinning de versão fixo, nunca "latest":** a referência (tag ou commit) do
   `ai-coding-agent-profiles` buscada fica gravada como constante no código-fonte do
   `agentry`, atualizada manualmente a cada *bump* deliberado — nunca resolvida
   dinamicamente contra o release mais recente do repositório remoto. Prioriza
   reprodutibilidade (o mesmo comando produz sempre o mesmo resultado, em qualquer máquina,
   em qualquer dia) sobre frescor automático.
5. **Comando manual sempre exibido, como complemento:** `--init`/`/init` sempre imprime (via
   stdout, além de agir) o comando equivalente para quem preferir buscar/inspecionar por
   conta própria — apontando para `scripts/setup-profile.sh` do `ai-coding-agent-profiles`,
   na mesma referência pinada. Nunca é a única via de adoção do perfil.
6. **Idempotência:** se `.agentry/agentry.settings.json` já existir, `--init`/`/init` **não
   sobrescreve por padrão** — mesmo princípio já usado por `ensure_state_dir` para o
   `.gitignore` (MT-38): não apagar customização do usuário sem um sinal explícito.
   Sobrescrita deliberada (flag `--force` ou confirmação interativa) fica para um ticket de
   implementação futuro, fora do escopo desta ADR.
7. **Falha de rede com `--profile` explícito é erro tratado, nunca fallback silencioso:** se
   o GET falhar (rede indisponível, host fora do ar, `schemaVersion` incompatível no artefato
   obtido), o comando termina com erro claro — **nunca** substitui silenciosamente pelo
   exemplo genérico do item 1, que não corresponde ao que foi pedido. Sem `--profile`, nunca
   há rede, logo esse cenário não se aplica.
8. **Nunca executa nada obtido pela rede:** mesmo que o conteúdo pinado viesse a ser
   comprometido, o pior caso é um `agentry.settings.json` malformado ou com `schemaVersion`
   divergente — pego pela mesma validação de parse/schema já existente
   (`Settings::from_json_str`, ADR-0003/0018) antes de qualquer gravação em disco. Em nenhum
   momento o conteúdo obtido é interpretado como código.

## Consequências

- **Impacto positivo:** primeira forma real de sair do modo "sessão efêmera em memória" com
  um comando só; não reintroduz o padrão `curl | sh`; mantém 100% de conformidade com o
  modelo de egresso *fail-closed* já estabelecido (ADR-0002), sem abrir exceção nova;
  reprodutível (pinning fixo em vez de "latest" dinâmico).
- **Impacto negativo:** mais uma constante para o mantenedor atualizar manualmente a cada
  release do `ai-coding-agent-profiles` que deva se refletir no `agentry`; um segundo
  caminho de rede (ainda que via `Transport`) a manter testado, distinto do caminho
  operacional normal (providers/tools) — precisa de sua própria `Allowlist`/instância.
- **Trade-offs aceitos:** reprodutibilidade em vez de frescor automático (item 4) — quem
  quiser sempre a versão mais recente do perfil precisa usar o comando manual (item 5) ou
  esperar o próximo *bump* da referência pinada; menos "mágico" do que delegar a integração
  inteira ao script do `profiles`, mas sem o risco de *supply chain* que isso traria.

## Diretriz de Conformidade de Código

- **Proibido:** qualquer chamada de rede do comando `--init`/`/init` fora do módulo
  `Transport` central; buscar e/ou executar qualquer script remoto (padrão `curl | sh` ou
  equivalente); resolver a referência do `ai-coding-agent-profiles` dinamicamente contra
  "latest" em vez da constante pinada no código-fonte; sobrescrever
  `.agentry/agentry.settings.json` já existente sem uma flag/confirmação explícita;
  *fallback* silencioso para o exemplo genérico quando `--profile` foi pedido explicitamente
  e a busca falhou; interpretar como código qualquer conteúdo obtido pela rede.
- **Obrigatório:** reaproveitar `state_dir::ensure_state_dir`/`agentry_settings_path`
  (MT-38/39) para localizar/criar o diretório de estado; construir uma instância de
  `Transport` dedicada ao bootstrap, com `Allowlist` restrita ao host fixo de conteúdo bruto
  do GitHub e `EgressClass::CloudOk`; validar o artefato obtido com o mesmo
  `Settings::from_json_str` (checagem de `schemaVersion`) já usado no carregamento normal,
  antes de qualquer gravação em disco; sempre imprimir o comando manual equivalente de
  `setup-profile.sh`.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
