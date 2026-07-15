<!-- Caminho relativo: docs/decisoes-autonomas.md -->

# Log de decisões autônomas (loop de implementação)

Registro **append-only** de toda decisão tomada pelo agente durante a execução autônoma em
loop (`/loop /implementar-roadmap`) quando ele se depara com uma dúvida e escolhe a **opção
recomendada** sem parar para perguntar. Existe para que o mantenedor **revise depois** cada
escolha feita sozinho.

> Regra do loop: diante de uma dúvida com opção recomendada clara, o agente **segue a
> recomendada, registra aqui, e continua**. Diante de uma dúvida **sem** recomendação clara,
> ou de uma parada dura (dependência nova, repo irmão, afrouxar segurança), o agente **para e
> escala ao usuário** — ver `.claude/commands/implementar-roadmap.md`.

## Formato de cada entrada

```
### AAAA-MM-DD — <ticket/fase> — <título curto da decisão>
- **Contexto:** onde/por quê a decisão apareceu.
- **Opções consideradas:** (a) …; (b) …; (c) …
- **Escolha (recomendada):** <a opção adotada>.
- **Justificativa:** por que é a mais alinhada ao objetivo do agentry, à segurança/
  governança/confidencialidade (ADR-0002 fail-closed) e a um design mínimo (não
  over-engineered).
- **Commit:** `<hash>`.
```

## Entradas (mais recente no topo)

### 2026-07-15 — MT-72 (Fase 15, TUI) — auditoria descartada sob `--tui`, não redirecionada para um widget
- **Contexto:** o smoke-test manual do MT-72 (`agentry --tui`, mensagem real via Ollama)
  revelou que `StderrAuditSink`/o `impl GuardrailAuditSink` (ambos em `crates/cli/src/main.rs`,
  MT-05/46) escrevem via `eprintln!` diretamente no terminal a cada chamada de rede — sob o
  modo bruto/tela-alternativa do `crossterm` (ADR-0027), essa escrita cai por cima do buffer
  que o `ratatui` está desenhando (ele não sabe da escrita, então não a repõe no próximo
  `draw`), corrompendo a tela a cada turno. Não é um problema no REPL/one-shot (stderr
  simplesmente intercala com a saída normal na mesma tty, sem "buffer" a violar).
- **Opções consideradas:**
  (a) redirecionar a auditoria para um *widget* de log dentro da própria TUI (painel dedicado,
  rolável) quando `--tui` está ativo;
  (b) descartar silenciosamente a auditoria (`NoopAuditSink`, novo tipo unitário implementando
  `AuditSink`/`GuardrailAuditSink` como no-op) enquanto o modo TUI estiver ativo, preservando o
  comportamento atual (stderr) para REPL/one-shot.
- **Escolha (recomendada):** (b).
- **Justificativa:** um *widget* de log é uma peça de UI nova, não pedida por nenhum ticket da
  Fase 15 (MT-70..76) nem pela ADR-0027 — construí-la agora seria escopo além do objetivo do
  MT-72 ("view de chat com streaming real"), violando a disciplina de não introduzir
  funcionalidade além do *Objetivo* do ticket. Descartar é o comportamento correto enquanto não
  existe onde mostrar a auditoria sem corromper a tela; a auditoria em si (rastreabilidade de
  chamadas de rede, ADR-0002) continua ativa e correta no REPL/one-shot, os modos usados hoje
  para qualquer fluxo que dependa de auditoria de verdade. Um *widget* de log fica anotado como
  candidato de ticket futuro, condicionado a demanda real (YAGNI) — não uma lacuna esquecida.
  Não afrouxa nenhuma garantia de segurança/egresso: a decisão de **permitir** ou **negar** uma
  chamada de rede continua inteiramente no `Transport`/`Allowlist` (MT-05/07, ADR-0002),
  inalterados; só o **registro** posterior da chamada já permitida deixa de ser impresso.
- **Commit:** `04db36e`.

### 2026-07-15 — MT-72 (Fase 15, TUI) — revisão dos *keybindings* de letra do MT-71 (`q`/`k`/`j`) para liberar a digitação
- **Contexto:** o MT-71 (`docs/roadmap-v0.9.md`) havia fixado `q` (sair), `k`/`j` (rolar,
  estilo vim) como alternativas às setas na tabela única de `crates/cli/src/tui/keybind.rs` —
  nesse momento a TUI ainda não tinha nenhuma caixa de entrada de texto real, então não havia
  ambiguidade. O MT-72 introduz a digitação de mensagens de verdade, e uma letra solta não pode
  significar simultaneamente "ação fixa" (sair/rolar) e "caractere digitado" sem um modo
  explícito (insert/normal, à la vim) — fora do escopo mínimo desta ticket.
- **Opções consideradas:**
  (a) introduzir um modo explícito (ex.: `Tab` alterna entre "navegação" e "digitação"), preservando os atalhos de letra do MT-71 dentro do modo de navegação;
  (b) remover os atalhos de letra (`q`, `k`, `j`) da tabela `DEFINITIONS`, mantendo só teclas
  que nunca colidem com texto digitado (`Ctrl+C` para sair — convenção universal de terminal,
  inambígua mesmo com o campo de texto focado; setas para rolar).
- **Escolha (recomendada):** (b).
- **Justificativa:** um sistema de modos (a) é a escolha certa para um editor modal completo,
  mas over-engineering para o escopo do MT-72 — nenhum ticket da Fase 15 pede navegação modal
  estilo vim, e introduzir um conceito de modo agora obrigaria também a expor visualmente qual
  modo está ativo (mais uma peça de UI não pedida). (b) resolve a ambiguidade com a mudança
  mínima: `Ctrl+C` já é a convenção universal e inambígua de "sair" em qualquer aplicação de
  terminal (funciona igual estando o campo de texto focado ou não), e setas nunca colidem com
  texto digitado. Nenhuma regressão de segurança — a tabela de *keybindings* continua sem
  conflito de tecla (mesmo teste do MT-71, `tabela_nao_tem_duas_acoes_para_a_mesma_tecla_default`,
  ainda passa) e a garantia "tecla sem ação mapeada não é erro" (MT-71) se estende naturalmente
  para "vira caractere digitado", não um estado de erro.
- **Commit:** `04db36e`.

### 2026-07-15 — ADR-0023 (preparação da Fase 13) — parser de frontmatter de `SKILL.md` próprio, sem dependência YAML
- **Contexto:** ADR-0023 (memória de projeto: `AGENTS.md`/`CLAUDE.md` + *progressive
  disclosure* de `SKILL.md`) precisa extrair `name`/`description` do frontmatter YAML de cada
  `SKILL.md` descoberto (delimitado por `---`), incluindo o estilo de bloco dobrado (`>-`)
  usado nos `SKILL.md` reais deste projeto (ex.: `.claude/skills/adr-writer/SKILL.md`).
- **Opções consideradas:**
  (a) adotar uma dependência de parser YAML (ex.: `serde_yaml` ou `saphyr`) para interpretar o
  frontmatter de forma genérica e robusta a qualquer sintaxe YAML válida;
  (b) escrever um parser próprio, mínimo, cobrindo só o subconjunto realmente usado por
  `SKILL.md` neste ecossistema — duas chaves de string fixas (`name`/`description`), incluindo
  o bloco dobrado `>-` — sem tentar cobrir YAML arbitrário (listas, mapas aninhados, âncoras,
  tipos numéricos/booleanos).
- **Escolha (recomendada):** (b).
- **Justificativa:** o schema do frontmatter de `SKILL.md` é fixo, pequeno e conhecido —
  trazer um parser YAML genérico traria uma superfície de API/manutenção desproporcional ao
  problema, além de ser uma **dependência de runtime nova**, que o próprio comando de loop
  trata como gatilho de parada dura quando decidida durante *implementação* (não durante
  preparação de fase). Decidir agora, na ADR, por um parser mínimo evita completamente esse
  gatilho — mesmo espírito de MT-06 (redação de segredos sem regex) e do casamento de
  guardrail por substring (ADR-0007), que já evitam dependência nova para problemas estreitos
  e bem definidos deste projeto. O trade-off aceito (frontmatter fora do subconjunto suportado
  falha de forma tratada, não silenciosa) é proporcional ao ganho de continuar com árvore de
  dependências auditável (ADR-0001).
- **Commit:** `384899b`.

### 2026-07-15 — MT-55 (Fase 12, `taskClasses`) — `Config` não sintetiza defaults de task-class
- **Contexto:** o ticket MT-55 (`docs/roadmap-v0.6.md`) pedia que, quando `taskClasses` não
  declarar `chat`/`compact`/`guardrail-compliance`, o `Config` (`crates/core/src/config/mod.rs`)
  sintetize internamente esses defaults hoje hardcoded na CLI, para "zero-config idêntico" e
  para `/compact`/Reviewer terem rota mesmo sem configuração explícita.
- **Opções consideradas:**
  (a) `Config::resolve` sintetiza os três defaults concretos (provider `"ollama"`, modelos e
  presets fixos) quando ausentes do mapa declarado — como o texto do ticket propunha;
  (b) `Config.task_classes` expõe só o que foi declarado pelo usuário (mapa vazio quando
  nada é configurado), e a síntese de defaults concretos de provider/modelo passa a ser
  responsabilidade da CLI (MT-56), que já é o ponto que hoje hardcoda `set_chat_route`
  (Ollama, `local-only`).
- **Escolha (recomendada):** (b).
- **Justificativa:** `crates/core` é a camada de domínio (rotas, presets, egresso) e não deve
  conhecer qual provider é o produto usa como fallback — isso é uma decisão de *produto* da
  CLI de referência, não do modelo de dados. Colocar `"ollama"` hardcoded dentro do `core`
  quebraria a separação já estabelecida (o `core` não hardcoda nenhum provider concreto hoje)
  e tornaria a lib reutilizável menos genérica sem necessidade — não há teste ou consumidor de
  `agentry-core` fora da CLI que precise desse comportamento embutido no tipo de config. A
  CLI (MT-56) é o lugar correto para registrar `chat`/`compact`/`guardrail-compliance` como
  rotas concretas quando `task_classes` resolvido vier vazio, preservando o resultado
  observável do ticket (zero-config idêntico) sem violar a fronteira de camadas. Não afrouxa
  segurança/egresso — quando a CLI sintetizar, o candidato de fallback continua `local-only`
  (Ollama), igual ao comportamento anterior a esta mudança.
- **Commit:** `8f0ba55`.
