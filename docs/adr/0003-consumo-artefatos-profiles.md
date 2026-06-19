<!-- Caminho relativo: docs/adr/0003-consumo-artefatos-profiles.md -->

# ADR 0003: Consumo dos artefatos de política do `ai-coding-agent-profiles`

- **Status:** Proposed
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
