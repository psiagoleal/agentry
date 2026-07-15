<!-- Caminho relativo: docs/roadmap-v0.7.md -->

# Roadmap v0.7 — Micro-tickets

O roadmap v0.6 (`docs/roadmap-v0.6.md`) cobre a Fase 12 (config completa e autoexplicativa,
ADR-0021/0022, **concluída**). Este documento detalha a **Fase 13** do roadmap de longo prazo
(`docs/roadmap-longo-prazo.md`): memória de projeto via leitura de `AGENTS.md`/`CLAUDE.md` e
*progressive disclosure* de `SKILL.md` (ADR-0023).

## Convenções

Mesmas dos roadmaps anteriores (`docs/roadmap-v0.1.md` §Convenções): **DoD** padrão
(`cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`), dependência nova exige
ADR (ADR-0004), skill `micro-ticket-planner` para granularidade. **Nenhuma dependência nova
nesta fase** — decisão explícita da ADR-0023 (parser de frontmatter próprio, ver
`docs/decisoes-autonomas.md`).

---

## Fase 13 — Memória de projeto: AGENTS.md + Skills (ADR-0023)

### MT-59: Leitor de `AGENTS.md`/`CLAUDE.md` — injeção como mensagem de sistema ✅ concluído
- **Objetivo:** novo módulo `crates/core/src/project_instructions.rs`:
  `load_project_instructions(root: &Path, ignore: &Gitignore) -> Option<String>` — lê
  `AGENTS.md` (primário) ou, na ausência dele, `CLAUDE.md` (*fallback*, nunca os dois — mesma
  precedência do ADR-0020); pula silenciosamente se o caminho estiver coberto pelo ignore do
  projeto. `crates/core/src/tools/fs.rs::load_ignore` promovida de privada para `pub(crate)`
  (reuso entre módulos do `agentry_core`, sem duplicar a construção do `Gitignore`).
  `Session` (`crates/core/src/session/mod.rs`) ganha `with_project_instructions(String)`
  (mesmo padrão builder de `with_guardrails`); `ensure_system_prompt` passa a concatenar
  instruções de projeto (se houver) + `preset.system_prompt` (se houver) numa única mensagem
  de sistema, nesta ordem, separados por linha em branco — preserva a inserção única já
  existente (nunca duplica ao longo da sessão). Novo campo `context.agentsFile.enabled`
  (`FeatureToggle`, ADR-0018, *default* `true`) em `Settings`/`Config` (mesmo padrão de
  `repoMap`/`semanticRag`/`lspGrounding`). `crates/cli/src/main.rs` chama o loader (com o
  `Gitignore` já resolvido para `respect_gitignore`/`.agentryignore`) e conecta via
  `with_project_instructions` quando há conteúdo e a flag está ligada.
- **Arquivos no escopo:** `crates/core/src/project_instructions.rs` (novo),
  `crates/core/src/session/mod.rs`, `crates/core/src/config/mod.rs`,
  `crates/core/src/tools/fs.rs` (visibilidade de `load_ignore`), `crates/cli/src/main.rs`.
- **Critério de aceite:** testes — `AGENTS.md` presente vira mensagem de sistema; `CLAUDE.md`
  só é lido quando `AGENTS.md` está ausente (nunca os dois, nunca merge); nenhum dos dois
  presente não insere mensagem nenhuma (comportamento atual preservado, zero-config idêntico);
  arquivo coberto por `.agentryignore`/`.claudeignore` nunca é lido; `preset.system_prompt` e
  instruções de projeto coexistem numa única mensagem quando ambos presentes (ordem: projeto
  primeiro); `context.agentsFile.enabled=false` desliga o carregamento mesmo com arquivo
  presente. Smoke-test manual do binário real com um `AGENTS.md` de teste.
- **Fora de escopo:** descoberta de `SKILL.md` (MT-60); qualquer UI de configuração
  interativa.
- **Depende de:** ADR-0023 · ADR-0020 (precedência `.agentryignore`/`.claudeignore`) ·
  ADR-0016 (mesmo padrão de manipulação de `Message::system` de `Session::compact`).
- **Nota de implementação:** `tools::fs::load_ignore` promovida diretamente para `pub`
  (não `pub(crate)` como o texto original sugeria) — a CLI (crate `agentry`, diferente de
  `agentry-core`) precisa montar o mesmo `Gitignore` para passar a
  `load_project_instructions`, e `pub(crate)` não cruza fronteira de crate.

### MT-60: Descoberta de `SKILL.md` + lista compacta no *system prompt* ✅ concluído
- **Objetivo:** novo módulo `crates/core/src/skills.rs`: `SkillDescriptor { name,
  description, path }`; `discover_skills(root: &Path, ignore: &Gitignore) -> Vec<SkillDescriptor>`
  varre `<root>/.claude/skills/*/SKILL.md` (um nível de subdiretórios, sem recursão) e extrai
  `name`/`description` do frontmatter YAML entre delimitadores `---` via **parser próprio**
  (decisão da ADR-0023, registrada em `docs/decisoes-autonomas.md`) — cobre só o subconjunto
  usado de fato pelos `SKILL.md` deste ecossistema: chave `escalar: valor` de uma linha e
  bloco dobrado `chave: >-` (concatena as linhas seguintes com espaço até a próxima chave ou
  o fim do frontmatter). `SKILL.md` sem `name`/`description` reconhecíveis é **pulado com
  aviso tratado** (nunca *panic*, nunca aborta a descoberta dos demais). Path coberto por
  `.agentryignore`/`.claudeignore` é pulado, mesma disciplina do MT-59.
  `render_skills_list(&[SkillDescriptor]) -> String` formata a lista compacta (`- nome:
  descrição` por linha). Ligado à mesma mensagem de sistema do MT-59 (concatenado por
  último, depois de instruções de projeto + preset).
- **Arquivos no escopo:** `crates/core/src/skills.rs` (novo), `crates/core/src/session/mod.rs`
  (ou reaproveita o hook de concatenação do MT-59), `crates/cli/src/main.rs`.
- **Critério de aceite:** testes — frontmatter com bloco dobrado (`>-`) do `adr-writer` real
  deste repositório (fixture copiada do arquivo real) é parseado corretamente; `SKILL.md` sem
  os dois campos é pulado sem interromper a descoberta dos demais; diretório
  `.claude/skills/` ausente não é erro (lista vazia, nenhuma mensagem extra); path coberto por
  `.agentryignore` é pulado; lista renderizada aparece na mensagem de sistema quando pelo
  menos uma skill é descoberta; nenhuma skill descoberta não insere lista vazia na mensagem
  (sem ruído).
- **Fora de escopo:** ativação/gatilho de skill via tool (MT-61); parser YAML genérico
  (decisão explícita da ADR-0023 — não revisitar aqui).
- **Depende de:** ADR-0023 · MT-59 (reaproveita o hook de injeção de mensagem de sistema).
- **Nota de implementação:** descoberto durante a escrita do teste da *fixture* real — o
  literal Rust escapado com continuação de linha (`\` no fim da linha) **remove os espaços de
  indentação** do início da linha seguinte, destruindo a indentação do bloco dobrado que o
  teste precisava preservar; corrigido usando *raw string* (`r#"..."#`) para a *fixture*, que
  preserva o texto exatamente como escrito. Achado de sintaxe do Rust, não do parser em si (o
  parser estava correto; o dado de teste que o alimentava estava malformado).

### MT-61: Tool `skill` — carrega o corpo completo sob demanda ✅ concluído
- **Objetivo:** `SkillTool` (novo, `crates/core/src/tools/skill.rs`), implementando `Tool`
  (mesma trait do MT-11) sobre o `Vec<SkillDescriptor>` descoberto pelo MT-60: recebe um nome
  de skill, lê o **corpo** do `SKILL.md` correspondente (todo o conteúdo após o frontmatter,
  sem o bloco `---`/`---` de metadados) e devolve como `ToolOutput` de sucesso; nome
  desconhecido é `ToolOutput::error` tratado (nunca *panic*). Registrada no `ToolRegistry`
  como qualquer outra tool, sob o mesmo `PermissionGate` (MT-11) — sem *default-deny* especial
  (diferente da tool de shell): é leitura local sem efeito colateral, mesma categoria de
  `fs_read`/`repo_map`.
- **Arquivos no escopo:** `crates/core/src/tools/skill.rs` (novo),
  `crates/core/src/tools/mod.rs` (declaração do módulo), `crates/cli/src/main.rs` (registro da
  tool, reaproveitando o `Vec<SkillDescriptor>` já descoberto pelo MT-60).
- **Critério de aceite:** testes — nome de skill válido devolve o corpo completo (sem o
  frontmatter); nome desconhecido é erro tratado; a tool respeita `deny`/`ask` do
  `PermissionGate` como qualquer outra (teste de integração via `ToolRegistry::execute`);
  `.claude/skills/` vazio ainda registra a tool, só sem nenhuma skill para carregar (erro
  tratado ao chamar, não ausência da tool).
- **Fora de escopo:** cache de conteúdo entre chamadas (releitura do arquivo a cada chamada é
  aceitável — arquivo pequeno, local, sem custo de rede); qualquer heurística automática de
  "quando" chamar a skill — a decisão é sempre do modelo, orientado pela lista de nome+
  descrição já presente na mensagem de sistema (MT-60).
- **Depende de:** MT-60.
- **Nota de implementação:** a descoberta de skills e a construção do `Gitignore`
  (`context_ignore`) precisaram mudar de posição em `main()` — o MT-60 as colocava **depois**
  da montagem do `ToolRegistry` (só usadas para a mensagem de sistema); a `SkillTool` precisa
  do `Vec<SkillDescriptor>` **antes** do registro da tool, então ambas subiram para antes da
  construção do `registry`, reaproveitadas depois tanto pelo registro da tool quanto por
  `render_skills_list`. **Smoke-test:** tentativa de invocação via linguagem natural não
  confirmou o *round-trip* completo — os modelos locais disponíveis (`llama3.1:8b`,
  `qwen2.5:7b`) não chamaram a tool de fato mesmo para `fs_read` (tool já madura, testada em
  tickets anteriores), simulando/alucinando uma resposta em vez de emitir uma *tool-call*
  real — limitação de confiabilidade de *tool-calling* de modelos locais pequenos neste
  ambiente, não uma regressão deste ticket. Correção coberta com confiança pelos testes de
  integração via `ToolRegistry::execute` real (inclusive o gate de permissão).

### MT-62: Documentação do site + ADR-0003 → `Accepted` ✅ concluído
- **Objetivo:** `docs/usuario/configuracao.md` ganha a seção `context.agentsFile` (o que é
  lido, precedência `AGENTS.md`/`CLAUDE.md`, relação com `.agentryignore`); novo
  `docs/usuario/skills.md` documenta a convenção `.claude/skills/<nome>/SKILL.md` (frontmatter
  obrigatório, corpo, como o agente descobre e usa via a tool `skill`) — adicionado à `nav` do
  `mkdocs.yml`. **ADR-0003** (`Proposed` desde o MT-04) promovida a `Accepted`: seu objetivo
  original — o `agentry` consumir instruções de projeto — está agora implementado (MT-59..61).
- **Arquivos no escopo:** `docs/usuario/configuracao.md`, `docs/usuario/skills.md` (novo),
  `docs/adr/0003-consumo-artefatos-profiles.md` (status), `docs/adr/README.md`, `mkdocs.yml`.
- **Critério de aceite:** `mkdocs build --strict` limpo (novo arquivo na `nav`, *anchors* de
  todo *cross-link* novo conferidos no HTML gerado); releitura confirmando que nada na trilha
  de usuário ficou desatualizado.
- **Fora de escopo:** trilha de governança (nenhuma afirmação de egresso muda — leitura de
  `AGENTS.md`/skills é 100% local, sem rede, ADR-0002 preservado).
- **Depende de:** MT-61.
- **Nota de implementação:** ao promover ADR-0003, achado real: `.claude/settings.json`
  (previsto no texto original da ADR) nunca foi de fato consumido — o `agentry` sempre usou
  seu próprio artefato (`.agentry/agentry.settings.json`, ADR-0018), decisão já tomada e
  registrada em sessão anterior, mas a ADR-0003 nunca tinha sido emendada para refletir isso;
  emenda adicionada antes de fechar como `Accepted`, para a ADR não afirmar algo que a
  implementação real não faz. Achado de *anchor* do mkdocs, pego pelo próprio
  `mkdocs build --strict`: o `id` gerado para "Memória de projeto (`AGENTS.md`/`CLAUDE.md`)"
  não tem hífen entre "agentsmd" e "claudemd" (a barra entre os dois nomes de arquivo é só
  removida do *slug*, não vira separador) — o *link* escrito com hífen extra apontava para um
  *anchor* que não existia; corrigido nos dois *cross-links* que o usavam.

---

## Sequência crítica

```
MT-59 → MT-60 → MT-61 → MT-62
```

Estritamente sequencial: cada ticket reaproveita o hook/mecanismo do anterior (mensagem de
sistema única do MT-59; lista de skills do MT-60 alimenta a tool do MT-61; documentação do
MT-62 cobre o conjunto inteiro já implementado).
