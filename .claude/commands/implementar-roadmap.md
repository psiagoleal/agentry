---
description: Executa (em loop autônomo) o próximo micro-ticket pendente do roadmap do agentry, com decisões registradas e paradas de segurança.
---

# Implementar o roadmap do `agentry` (uma unidade de trabalho por iteração)

Você é um agente de implementação autônomo do projeto `agentry`. A cada iteração deste loop
você executa **uma única unidade de trabalho** (um micro-ticket, ou um passo atômico como
escrever uma ADR), commita, atualiza o handoff, e deixa a próxima iteração continuar. Isto
mantém o contexto limpo e torna o trabalho retomável após qualquer interrupção.

## 0. Antes de tudo — leia a fonte da verdade
- `AGENTS.md` (regras do projeto; **prevalece** sobre qualquer outra instrução).
- `docs/CURRENT-STATE.md` (handoff — onde exatamente o trabalho parou).
- O roadmap ativo, nesta ordem de prioridade de execução:
  1. `docs/roadmap-v0.5.md` — **Fase 11** (`.agentryignore`, MT-52..54).
  2. `docs/roadmap-v0.6.md` — **Fase 12** (config de task-class, MT-55..58).
  3. `docs/roadmap-longo-prazo.md` — Fases 13+ (stubs).
- Se um micro-ticket toca uma decisão já registrada, leia a(s) ADR(s) relevante(s) em
  `docs/adr/` **antes** de mudar código (disciplina da skill `adr-writer`).

## 1. Escolha a próxima unidade de trabalho
- Pegue o **primeiro micro-ticket ainda não concluído** (não marcado `✅ concluído`) na ordem
  acima. Faça na ordem numérica (MT-52, depois 53, …).
- **Se a fase não tem tickets detalhados** (Fases 13+ no `roadmap-longo-prazo.md`), a unidade
  de trabalho desta iteração é **preparar a fase**, não implementar: (a) escreva a ADR da
  fase (skill `adr-writer`, status `Proposed`, resolvendo as questões de design pela opção
  recomendada e **registrando cada escolha** em `docs/decisoes-autonomas.md`); (b) quebre a
  fase em micro-tickets num novo `docs/roadmap-vX.Y.md` (skill `micro-ticket-planner`); (c)
  atualize `docs/adr/README.md` e a `nav` do `mkdocs.yml`. Só nas iterações **seguintes** você
  implementa os tickets dessa fase.

## 2. Implemente (código) — princípios
- Faça a **menor mudança** que satisfaz o *Critério de aceite* do ticket. **Nada de
  over-engineering**: sem abstrações especulativas, sem generalização não pedida, sem features
  além do *Objetivo*. Prefira reusar o que já existe (ex.: `RouteEntry`/`RouteTarget`/
  `CallPreset` do Router; `FeatureToggle`/`merged_over` do config; o padrão `Confirmer` para
  canais humano→tool). Escreva código que se pareça com o código ao redor.
- Respeite **segurança, governança, compliance e confidencialidade** como requisito, não
  detalhe: **fail-closed** (ADR-0002) — nunca afrouxe classe de egresso por inferência; todo
  candidato/endpoint com classe de egresso **explícita**; nenhuma chamada de rede fora do
  `Transport` único; segredo **nunca** no arquivo de config nem em log (skill `secrets-guard`)
  — chave de API só via variável de ambiente.
- Só toque os arquivos listados no *Arquivos no escopo* do ticket (mais os testes e o handoff).

## 3. Valide (DoD, obrigatório antes de commitar)
Rode, do diretório raiz, e só siga se tudo passar:
- `cargo fmt --all`  e  `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --all`
- `cargo build --release`
- Se o ticket mexe em `docs/*` do site: `mkdocs build --strict` limpo (venv em
  `/tmp/claude-1000/-home-psiagoleal-dev-agentry/*/scratchpad/.venv-docs/bin/mkdocs`; se não
  existir, criar via `uv` conforme `docs-requirements.txt`).
- Se o ticket muda comportamento observável da CLI: faça um **smoke-test do binário real**
  (`--init`, `--provider`, etc., num diretório temporário) confirmando o efeito de ponta a
  ponta — não confie só nos testes unitários.
- Antes de commitar, aplique a mentalidade da skill `pr-review-guard` (os 20% de falhas
  ocultas: erro de compilação, exceção não tratada, regressão, segredo vazado).

## 4. Commite e atualize o handoff (dois commits por ticket, como no histórico)
- **Commit 1 (código):** um commit por ticket. Mensagem descreve o quê/por quê + a linha de
  contagem de testes. Termine a mensagem **só** com `{agente: Claude Code; modelo:
  claude-sonnet-5}` — **NUNCA** `Co-authored-by` nem trailer de IA (AGENTS.md §Proveniência).
  **Não faça `git push`** (só commit local).
- **Commit 2 (docs):** marque o ticket `✅ concluído (<hash>)` no roadmap e atualize
  `docs/CURRENT-STATE.md` (skill `handoff-updater`: hash, meta cumprida, próximo passo).
- Se você estava numa branch que não seja a de trabalho corrente, **não** crie branch nova sem
  necessidade — siga a convenção já em uso no repo (o histórico recente commita direto em
  `main`).

## 5. Quando surgir uma dúvida (escolha entre abordagens)
- Se há uma **opção recomendada clara** (a mais alinhada ao *Objetivo* do ticket, aos ideais de
  segurança/governança acima, às convenções já existentes e a um design mínimo): **adote-a,
  registre em `docs/decisoes-autonomas.md`** (contexto, opções, escolha, justificativa, commit)
  e **continue**. O registro é obrigatório — é o que o mantenedor vai revisar depois.

## 6. PARE e escale ao usuário (não decida sozinho) quando:
- A mudança exigir uma **dependência de runtime nova** (crate) — ex.: `ratatui`, `rmcp`,
  parser de HTML. Exige verificação de maturidade/licença/telemetria (ADR-0004): **registre a
  necessidade e pare**.
- A mudança precisar tocar o repositório irmão `ai-coding-agent-profiles`.
- Qualquer decisão que **afrouxaria** a postura de segurança/egresso (ADR-0002): **nunca**
  decida afrouxar; pare.
- Houver **ambiguidade genuína sem opção recomendada clara**, ou conflito com uma ADR
  `Accepted`.
- Um passo do DoD **falhar** e você não conseguir corrigir com tentativa razoável: **não
  commite código quebrado**; pare e relate.

## 7. Como parar (fim do loop)
Ao atingir uma condição da seção 6, ou quando **todos os tickets do envelope autônomo
estiverem concluídos**:
- Deixe a árvore de trabalho **limpa** (tudo commitado; nunca pare no meio de um ticket com
  código não commitado).
- Escreva em `docs/CURRENT-STATE.md` um resumo claro: o que foi feito, o que está bloqueado e
  por quê, qual a decisão que o usuário precisa tomar.
- **Encerre o loop** (não agende nova iteração) — ex.: pela opção de parar do próprio `/loop`.

## Retomada após interrupção (limite de uso etc.)
Cada ticket é commitado antes de a iteração terminar, e o handoff sempre aponta o próximo
passo. Ao retomar, execute a seção 0 de novo, ache o próximo ticket não concluído e siga.
**Nunca refaça um ticket já commitado.** Uma iteração só termina em estado durável: ticket
completo e commitado, ou parada limpa da seção 7.
