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
- **Commit:** a registrar junto com o commit da ADR-0023.

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
