<!-- Caminho relativo: docs/adr/0004-postura-sinergia-open-source.md -->

# ADR 0004: Postura de sinergia com projetos open-source

- **Status:** Proposed
- **Data:** 2026-06-19
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** dependências, ecossistema, economia-de-tokens

## Contexto

Deseja-se sinergia desde o início com projetos OSS — `rtk` (compressão de tool-output em
Rust), `caveman` e `ponytail` (skills de economia de tokens / código minimalista), `OKF`
(Open Knowledge Format) e o padrão `LLM-Wiki` — **sempre pesando maturidade e confiabilidade**.
Duas restrições: (1) as métricas de maturidade desses projetos **ainda não foram verificadas**
(números obtidos por leitura de página parecem inflados); (2) `rtk` menciona **telemetria**,
o que conflita potencialmente com o ADR-0002. Por isso este ADR é **Proposed**.

**Verificação parcial (2026-07-14, `gh repo view rtk-ai/rtk`):** repositório real, não
arquivado, licença **Apache-2.0** (compatível, item (a) da Decisão), criado em 2026-01-22
(~6 meses), último push em 2026-07-09 (ativo), última release `v0.43.0`
(2026-06-28) — sinais de maturidade técnica razoáveis. Porém `stargazerCount: 70976` para um
repositório com ~6 meses de vida é uma anomalia estatística forte, reforçando (não
resolvendo) a suspeita original de números inflados registrada acima; a checagem de
telemetria 100% desligável (exigida pela Decisão antes de integrar o **binário**) não foi
feita — continua bloqueando qualquer adoção como dependência, só o *padrão* pode ser
adotado. `caveman`/`ponytail`/`OKF` seguem **sem identificador de repositório conhecido** em
nenhum dos dois repositórios do ecossistema — nenhuma verificação possível ainda. ADR
permanece **Proposed**.

## Decisão

Fica proposta a regra **"padrão antes de dependência"**:

- **(a) Compressão de tool-output:** adotar o *padrão* do `rtk` no Context Manager. Integrar o
  **binário** `rtk` apenas após ADR específico que comprove telemetria 100% desligável e
  compatível com o ADR-0002.
- **(b) `caveman` / `ponytail`:** consumíveis como **skills opcionais por perfil** (via
  biblioteca do `profiles`), **nunca** obrigatórios no core.
- **(c) Memória/contexto:** adotar o **padrão `LLM-Wiki`** (fontes imutáveis / wiki mantida /
  schema, com `index.md` + `log.md`).
- **(d) `OKF`:** **vigiar**; não depender enquanto imaturo (sem releases, spec não estável).
- Toda adoção de dependência externa exige **verificação de maturidade** (`gh repo view`:
  estrelas, último commit, releases, licença) e **licença compatível** (MIT/Apache-2.0/BSD).

## Consequências

- **Impacto positivo:** ganha padrões maduros sem acoplar a árvore de dependências; protege a
  confidencialidade (telemetria barrada na porta).
- **Impacto negativo:** reimplementar padrões custa esforço; é preciso acompanhar a evolução
  dos projetos de referência.
- **Trade-offs aceitos:** mais lentidão em troca de controle e confiabilidade.

## Diretriz de Conformidade de Código

- **Proibido:** adicionar dependência externa sem verificação de maturidade e licença
  registrada; integrar componente com telemetria/egresso que viole o ADR-0002; tornar skills
  de terceiros obrigatórias no núcleo.
- **Obrigatório:** preferir padrão a dependência; licenças compatíveis (MIT/Apache-2.0/BSD);
  registrar a avaliação no ADR e/ou no `exchange-log`.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
