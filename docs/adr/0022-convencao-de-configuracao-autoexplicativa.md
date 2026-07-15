<!-- Caminho relativo: docs/adr/0022-convencao-de-configuracao-autoexplicativa.md -->

# ADR 0022: Convenção de configuração autoexplicativa (`_comentario` obrigatório)

- **Status:** Accepted
- **Data:** 2026-07-14
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** configuração, usabilidade, documentação, convenção

## Contexto

O `agentry.settings.json` (ADR-0018) cresceu em superfície: permissões, contexto
(repo-map/RAG/LSP), providers (Ollama, LiteLLM), guardrails, e — com a ADR-0021 —
task-classes. Um achado real do usuário durante a Fase 10 mostrou o problema: o bloco
`providers.litellm` não tinha exemplo nenhum no arquivo gerado por `--init`, e a única forma
de descobrir a chave certa (`baseUrl`/`model`/`egressClass`) era ler o código-fonte ou a ADR.
A correção pontual (commit `ed0988c`/`95406c1`) adicionou `_comentario` por bloco e mostrou
todo campo configurável — mas foi uma resposta ad-hoc, não uma norma. O usuário pediu que
**toda configuração** (task-class, guardrails e demais) "sempre tenha algo configurado por
padrão, com comentários explicativos e exemplos diversos".

Avaliou-se trocar o formato inteiro para **TOML** (que tem comentários nativos). Descartado
(registrado no handoff da Fase 10): o `ai-coding-agent-profiles` distribui este artefato em
**JSON real**, com uma ferramenta de merge não-destrutivo própria para JSON
(`update_json_settings()`/`hybrid_json` em `scripts/setup-profile.sh`); trocar de formato
quebraria essa ferramenta e criaria dois formatos coexistindo (`--init` genérico vs.
`--init --profile`). Além disso, os arquivos reais dos três perfis daquele repositório **já
usam** uma chave `_comentario` (prefixo `_`, ignorada pelo parser — `Settings` não usa
`deny_unknown_fields`) exatamente para esse fim. Ou seja: a convenção já existe no
ecossistema; falta elevá-la a norma no `agentry`.

## Decisão

Fica acordado que **todo bloco configurável do `agentry.settings.json`, no exemplo gerado por
`--init`/`/init`**, deve vir com três coisas:

1. **Um default funcional** — nunca um bloco vazio sem indicação; o que fica inerte até o
   usuário preencher aparece como `null` (JSON não tem comentário; `null` é "a chave existe,
   ainda desligada").
2. **Uma chave `_comentario`** explicando, em uma ou duas frases, o que aquele bloco faz e
   como preenchê-lo — ignorada pelo `agentry` em tempo de execução, só para leitura humana.
3. **Exemplos de alternativas** — quando o bloco admite mais de uma forma útil (ex.:
   task-class de nuvem vs. de dados sensíveis; regra de guardrail `block` vs. `redact`),
   incluir entradas de exemplo (comentadas via `_comentario` na própria entrada, ou como
   itens de exemplo claramente marcados) mostrando as configurações possíveis.

Esta convenção vale para **todo config presente e futuro** (permissions, context, providers,
guardrails, task-classes, tools, e o que vier). Um campo novo de schema só é considerado
"pronto" quando aparece no exemplo gerado seguindo esta convenção — vira item de Definition
of Done implícito de qualquer ticket que estenda o schema.

Formaliza-se assim o `_comentario` (convenção herdada dos perfis do `ai-coding-agent-profiles`)
como **norma do `agentry`**, não improviso pontual. Nenhuma mudança de comportamento em tempo
de execução (chaves `_` já são ignoradas) — a mudança é de disciplina de UX/documentação.

## Consequências

- **Impacto positivo:** o arquivo gerado é autoexplicativo (descobrível sem ler código/ADR);
  novos campos nascem documentados; alinhamento com a convenção já usada no repositório
  irmão; zero custo de runtime.
- **Impacto negativo:** o exemplo gerado fica mais longo (mitigado: comentários são
  concisos e o valor de descoberta compensa); exige disciplina em cada ticket que mexe no
  schema (mitigado: vira item de DoD).
- **Trade-offs aceitos:** manter JSON + `_comentario` em vez de migrar para TOML nativo em
  comentários — preserva o contrato de interop e a ferramenta de merge do `profiles`.

## Diretriz de Conformidade de Código

- **Proibido:** adicionar um campo/bloco novo ao `agentry.settings.json` sem representá-lo no
  exemplo gerado por `--init` com default + `_comentario` + exemplos; gerar um bloco vazio
  sem indicação de como preenchê-lo; usar `deny_unknown_fields` em `Settings` (quebraria a
  convenção `_comentario`).
- **Obrigatório:** todo bloco configurável no exemplo gerado tem default funcional (ou `null`
  explícito), `_comentario` explicativo e exemplos das configurações possíveis; a validação
  do exemplo como JSON válido do schema real (com todo campo de exemplo inerte) é coberta por
  teste.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
