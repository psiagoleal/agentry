<!-- Caminho relativo: docs/adr/0003-consumo-artefatos-profiles.md -->

# ADR 0003: Consumo dos artefatos de política do `ai-coding-agent-profiles`

- **Status:** Accepted
- **Data:** 2026-06-19
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** integração, dados, formato, governança

## Contexto

A política do ecossistema vive no repositório irmão `ai-coding-agent-profiles`, sob o
contrato de interoperabilidade v1 (`docs/interop/SPEC.md`, canônico naquele repo). O `agentry`
deve **ler e aplicar** esses artefatos — `AGENTS.md`, `.claude/settings.json`, `SKILL.md`
(frontmatter), `.claudeignore` e a taxonomia de privacidade — **sem duplicar nem divergir** da
política. Os esquemas exatos de alguns artefatos ainda estão em definição, por isso este ADR
é **Proposed**: fixa a abordagem, não congela todos os detalhes.

**Emenda (2026-07-14, ADR-0020):** `.claudeignore` deixa de ser artefato de primeira classe
do contrato de interop v1 consumido daqui — o `agentry` passa a ter seu próprio artefato,
`.agentryignore` (mesmo padrão de posse do `.agentry/`, ADR-0017), com `.claudeignore`
mantido só como *fallback* de compatibilidade quando `.agentryignore` está ausente. O
`ai-coding-agent-profiles` continua distribuindo `.claudeignore` em seus três perfis sem
nenhuma mudança por enquanto — ver ADR-0020 para o racional completo e o item de migração
futura do lado `profiles`.

**Emenda (2026-07-15, ADR-0018/ADR-0023) — fechamento:** `.claude/settings.json` (formato
nativo do Claude Code, padrões `Bash(...)` em `permissions`) **não é consumido** — incompatível
por design com `agentry::config::Permissions` (nomes exatos de tool); o `agentry` tem seu
próprio artefato, `.agentry/agentry.settings.json` (ADR-0018), já registrado como mudança de
fronteira no `exchange-log` na época. Os demais artefatos previstos aqui estão **todos**
implementados: `AGENTS.md` (primário, `CLAUDE.md` como *fallback*) e `SKILL.md` por
*progressive disclosure* real — `name`+`description` sempre no contexto, corpo completo só
sob demanda via tool (MT-59..61, ADR-0023) —, `.agentryignore`/`.claudeignore` (ADR-0020),
taxonomia de privacidade (ADR-0002) e `settings-schema:1` com abortar *fail-closed* em
divergência de versão (`ConfigError::UnsupportedSchema`, desde o MT-04). Com isso, o objetivo
original desta ADR está cumprido e ela passa a `Accepted`.

## Decisão

Fica proposto que o `agentry` consuma os artefatos do perfil ativo como **fonte única de
política**, conforme o `docs/interop/SPEC.md` v1:

- Estabelece-se um **`settings-schema:1` mínimo**: parâmetros de modelo + permissões
  (`deny`/`ask`). Extensões entram por **novos ADRs**.
- `SKILL.md` é lido por **progressive disclosure** (apenas `name`+`description` no contexto até
  o gatilho acionar).
- A taxonomia de privacidade é a do ADR-0002 (`privacy-taxonomy:1`).
- **Divergência de versão de contrato ⇒ fail-closed** (abortar com mensagem explícita).

## Consequências

- **Impacto positivo:** fonte única de política; governança auditável; reuso da biblioteca de
  skills do `profiles`.
- **Impacto negativo:** acopla o `agentry` ao formato do `profiles` (mitigado por versionamento
  do contrato); o esquema mínimo pode precisar evoluir.
- **Trade-offs aceitos:** versionar o contrato em vez de congelá-lo cedo demais.

## Diretriz de Conformidade de Código

- **Proibido:** o `agentry` definir política própria que contorne os artefatos do perfil;
  consumir versão de contrato não suportada sem abortar.
- **Obrigatório:** respeitar o `docs/interop/SPEC.md`; registrar toda mudança de fronteira no
  `exchange-log` **e** em ADR.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
