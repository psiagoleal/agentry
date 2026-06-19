<!-- Caminho relativo: docs/interop/exchange-log.md -->

# Exchange Log — `agentry` ⇄ `ai-coding-agent-profiles`

Registro **append-only** das trocas entre os dois projetos. Regras: anexar ao final; nunca
reescrever entradas; decisões vinculantes viram ADR (referenciar o ADR aqui).

---

## 2026-06-19 — Bootstrap do ecossistema

- **Origem:** `agentry`.
- **Contexto:** definição inicial dos dois projetos como ecossistema (política + execução).
- **Decisões:**
  - Estabelecido o contrato de interoperabilidade **v1** (canônico em `profiles/docs/interop/SPEC.md`).
  - Divisão de responsabilidades (charter) ratificada: `profiles` = política; `agentry` = execução/imposição.
  - Provedores da v0.1 do `agentry`: **Ollama**, **vLLM** e **Anthropic**. GitHub Copilot/GitHub Enterprise **adiado** (caminho oficial — GitHub Models ou API Enterprise — ainda indefinido pela empresa).
  - Privacidade/egresso é **requisito** de arquitetura (não feature): router com classes de egresso desde a v0.1.
- **Pendências (rascunho a ratificar por ADR no `agentry`):**
  - `settings-schema:1` — quais chaves de `settings.json` o `agentry` lê.
  - `privacy-taxonomy:1` — mapa perfil → classe de egresso (`empresa`→`local-only`, `externo-confidencial`→`cloud-opt-out`, `pessoal`→`cloud-ok`).
- **Sinergia OSS avaliada (maturidade a verificar via `gh repo view`):** `rtk` (Rust, compressão de tool-output — candidato a dependência, auditar telemetria), `caveman`/`ponytail` (skills consumíveis), `LLM-Wiki` (padrão da camada de memória), `OKF` (vigiar — imaturo).
- **Status:** ✅ contrato v1 criado; ADRs do `agentry` pendentes.

---

## 2026-06-19 — Pacote de ADRs do `agentry`

- **Origem:** `agentry`.
- **Contexto:** ratificação das decisões estruturais da v0.1 (sem código ainda).
- **ADRs criados:**
  - **ADR-0001** (Accepted) — fundação da camada LLM: abstração própria sobre `reqwest`, sem framework (`rig`/`genai` fora do runtime).
  - **ADR-0002** (Accepted) — privacidade/egresso: transporte único auditável + allowlist + *fail-closed*; **ratifica `privacy-taxonomy:1`** (empresa→local-only, externo-confidencial→cloud-opt-out, pessoal→cloud-ok).
  - **ADR-0003** (Proposed) — consumo dos artefatos do `profiles`; `settings-schema:1` mínimo, extensível por novos ADRs.
  - **ADR-0004** (Proposed) — sinergia OSS: padrão antes de dependência (rtk/caveman/ponytail/LLM-Wiki/OKF); telemetria barrada por ADR-0002.
- **Efeito no contrato:** `privacy-taxonomy:1` passa de *(rascunho)* a **ratificado** no SPEC; `settings-schema:1` segue *(rascunho)* vinculado ao ADR-0003.
- **Documentação:** criado `agentry/docs/architecture.md` (módulos + fluxo de egresso).
- **Status:** ✅ ADRs e arquitetura criados. Próximo: roadmap de micro-tickets da v0.1.

---

## 2026-06-19 — Nome da CLI definido: `agentry`

- **Origem:** `agentry`.
- **Decisão:** o repositório/crate/binário da CLI passa a se chamar **`agentry`** (confirmado livre na crates.io), substituindo o placeholder `ai-cli` em toda a documentação dos dois repos.
- **Pareamento:** `ai-coding-agent-profiles` (política) + `agentry` (execução).
- **Ressalvas:** colisão leve de marca com "SAP Agentry" (plataforma legada de mobilidade empresarial, domínio distinto) — sem conflito de crate. A **pasta local** continua `~/dev/ai-cli` até renomeação manual (afeta CWD da sessão e o caminho da auto-memória).
- **Status:** ✅ documentação renomeada nos dois repos.

