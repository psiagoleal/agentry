<!-- Caminho relativo: docs/roadmap-v0.16.md -->

# Roadmap v0.16 — Micro-tickets

Pedido do mantenedor: paridade com o `--resume`/`--continue` do Claude Code CLI (persistência
de sessão), audit log persistente, e RAG estendido às sessões salvas. Três frentes, cada uma
com uma ADR própria (0036, 0037, e uma futura pra RAG-sobre-sessões, que depende da Fase G
existir de verdade). Não corresponde a uma "Fase N" do `docs/roadmap-longo-prazo.md` (esgotado
desde a Fase 20) — mesmo padrão ad-hoc do `roadmap-v0.15.md`, só que numerado à parte por ser
um lote de trabalho novo e coeso.

## Convenções

Mesmas de sempre (`docs/roadmap-v0.1.md` §Convenções): DoD padrão (`cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test`, `cargo build --release`), *smoke-test* real do
binário pra toda mudança observável, skill `micro-ticket-planner` para granularidade.
**Nenhuma dependência nova** — tudo com `serde`/`serde_json`/`std` já presentes.

---

## Fase G — Persistência de sessão opt-in em Markdown (ADR-0036)

Conflito real levantado e resolvido antes de qualquer código: a ADR-0032 (memória de projeto
explícita) já tinha decidido, em 2026-07-16, nunca persistir o conteúdo integral de uma
conversa — motivo declarado, retenção/confidencialidade incompatível com o objetivo de
homologação corporativa do projeto. Escalado ao mantenedor (não contornado); resposta: seguir,
mas só como exceção **opt-in explícita** (`/save`), com aviso claro toda vez — a ADR-0032
continua valendo como padrão (nada automático). Ver ADR-0036 para o registro completo.

### MT-118: ADR-0036 (Accepted) + nota de atualização na ADR-0032 ✅ concluído
- **Objetivo:** registrar a decisão acima — formato Markdown com *front matter* YAML +
  cabeçalhos `## <Papel>` por mensagem + blocos cercados `tool-call`/`tool-result` (JSON de
  uma linha, reaproveitando `Serialize`/`Deserialize` já existente de `ToolCall`/`ToolResult`);
  local `.agentry/session/<id>.md` (já reservado pela ADR-0017); comandos `/save [nome]`,
  `--resume [id-ou-nome]`, `/sessions`.
- **Arquivos no escopo:** `docs/adr/0036-*.md` (novo), `docs/adr/0032-*.md` (nota, sem alterar
  a decisão original), `docs/adr/README.md`, `mkdocs.yml`, `docs/roadmap-v0.16.md` (este
  arquivo, novo).
- **Depende de:** nenhum.

### MT-119: Serialização `Vec<Message>` → Markdown ✅ concluído (165a207)
- **Objetivo:** função pura (núcleo, `crates/core/src/session/`) que converte o histórico de
  uma `Session` (mensagens + metadados: id, criado_em, provider/model, task-class, uso
  acumulado) no formato Markdown definido na ADR-0036 — *front matter* YAML + uma seção
  `## <Papel>` por `Message`, cada `ContentBlock::Text` como prosa e cada
  `ContentBlock::ToolCall`/`ToolResult` como bloco cercado `tool-call`/`tool-result` (JSON de
  uma linha).
- **Arquivos no escopo:** novo `crates/core/src/session/persist.rs` (ou equivalente),
  `crates/core/src/session/mod.rs` (`pub mod`).
- **Critério de aceite:** testes — mensagem de texto simples vira uma seção; tool-call/result
  viram blocos cercados válidos (JSON reparsa de volta ao tipo original); várias seções do
  mesmo papel em sequência (duas chamadas de tool seguidas) preservadas na ordem certa;
  *front matter* contém todos os metadados esperados.
- **Fora de escopo:** o *parser* inverso (MT-120); escrita em disco (MT-121).
- **Depende de:** MT-118.

### MT-120: Desserialização Markdown → `Vec<Message>` ✅ concluído (165a207)
- **Objetivo:** *parser* inverso do MT-119 — nunca falha silenciosamente (ADR-0036, diretriz
  de conformidade): JSON malformado num bloco `tool-call`/`tool-result`, ou cabeçalho de papel
  desconhecido, é erro tratado e claro, nunca uma sessão retomada com histórico
  truncado/incorreto sem aviso.
- **Arquivos no escopo:** mesmo módulo do MT-119.
- **Critério de aceite:** testes — *round-trip* completo (serializar → desserializar devolve
  o `Vec<Message>` original, incluindo tool-calls/resultados); JSON malformado é erro tratado;
  cabeçalho de papel desconhecido é erro tratado; arquivo vazio/sem *front matter* é erro
  tratado, não pânico.
- **Depende de:** MT-119.

### MT-121: Comando `/save [nome]` — grava a sessão corrente ✅ concluído (de1e69d)
- **Objetivo:** novo comando (`repl::aplicar_comando`, reaproveitado pela TUI, mesmo padrão de
  `/compact`/`/usage`) que serializa (MT-119) a sessão corrente e grava em
  `.agentry/session/<id>.md` (`id` = *timestamp* `AAAAMMDD-HHMMSS`, `-<nome>` sufixado e
  sanitizado quando `nome` é dado). Imprime o aviso de retenção obrigatório (ADR-0036) toda
  vez, sem *flag* pra silenciar.
- **Arquivos no escopo:** `crates/cli/src/repl.rs`, `crates/cli/src/main.rs` (se precisar de
  fiação nova), `docs/usuario/uso.md`.
- **Critério de aceite:** testes — arquivo gravado no caminho certo, com o conteúdo esperado;
  aviso sempre impresso; nome sanitizado (só `[a-z0-9-]`). *Smoke-test* real: `/save`, abrir o
  arquivo gerado, conferir que é Markdown legível.
- **Depende de:** MT-119.

### MT-122: Flag `--resume [id-ou-nome]` — retoma uma sessão salva ✅ concluído (5a4a4f0)
- **Objetivo:** antes do primeiro turno (REPL/*one-shot*/TUI), se `--resume` foi passado,
  localiza o arquivo em `.agentry/session/` (sem argumento: mais recente por *timestamp*; com
  argumento: correspondência exata ou prefixo único — ambíguo é erro claro, não uma escolha
  arbitrária), desserializa (MT-120) e pré-popula `Session::messages` antes de rodar.
- **Arquivos no escopo:** `crates/cli/src/main.rs`, `crates/core/src/session/mod.rs`
  (`Session::with_messages`), `crates/cli/src/sessao.rs` (`carregar_sessao`/
  `localizar_arquivo`/`listar_arquivos_de_sessao`), `crates/cli/src/tui/chat.rs`
  (`ChatState::semear_historico`), `crates/cli/src/tui/mod.rs` (fiação da semeadura),
  `crates/cli/src/repl.rs` (`imprimir_historico_retomado`).
- **Critério de aceite:** testes — sessão retomada continua exatamente de onde parou (próxima
  chamada ao provider já leva o histórico completo); sem sessão nenhuma salva, `--resume` é
  erro claro (não silenciosamente vazio); *id*/nome ambíguo é erro claro. *Smoke-test* real:
  `/save`, fechar, `--resume`, confirmar que o modelo "lembra" do que foi dito antes.
- **Depende de:** MT-120.
- **Achados do *smoke-test* real (corrigidos antes de fechar o ticket, não eram escopo
  originalmente previsto no texto acima, mas necessários pra "paridade com Claude Code"):**
  a TUI/REPL mostravam a sessão retomada visualmente vazia mesmo com o histórico completo
  chegando certo ao provider (`ChatState::semear_historico`/`imprimir_historico_retomado`
  resolvem); e `--resume "tarefa"` sem `=` fazia o `clap` consumir avidamente a tarefa
  *one-shot* seguinte como valor de `--resume` (`require_equals = true` resolve — a forma
  sem valor continua `--resume` sozinho, a forma com valor passa a ser `--resume=<id>`).

### MT-123: Comando `/sessions` — lista sessões salvas
- **Objetivo:** lista `id`, data e um título (início da primeira mensagem do usuário) de cada
  sessão em `.agentry/session/`, mais recente primeiro — sem isso, `--resume <id>` exige que o
  usuário já saiba o *id* de cor.
- **Arquivos no escopo:** `crates/cli/src/repl.rs`, `docs/usuario/uso.md`.
- **Critério de aceite:** testes — lista vazia sem nenhuma sessão salva (não erro); ordem mais
  recente primeiro; título extraído corretamente da primeira mensagem de usuário.
- **Depende de:** MT-119 (só precisa ler o *front matter* + a primeira seção `## Usuário`, não
  o histórico inteiro).

---

## Fase H — Audit log persistente (ADR-0037)

### MT-124: ADR-0037 (Accepted) ✅ concluído
- **Objetivo:** registrar a decisão — `FileAuditSink` novo, JSON Lines em
  `.agentry/audit.log`, complementa (não substitui) o `StderrAuditSink` existente.
- **Arquivos no escopo:** `docs/adr/0037-*.md` (novo), `docs/adr/README.md`, `mkdocs.yml`.
- **Depende de:** nenhum.

### MT-125: `FileAuditSink` + sink combinado
- **Objetivo:** novo `FileAuditSink` (implementa `AuditSink`/`GuardrailAuditSink`), escreve
  uma linha JSON por entrada em `.agentry/audit.log` (modo *append*, sem *handle* mantido
  aberto entre chamadas); falha de escrita cai em `eprintln!`, nunca interrompe a chamada de
  rede em andamento. Novo combinador (`SinksCombinados` ou nome equivalente) chama `record()`
  nos dois sinks em sequência — `main.rs` passa a instanciar `StderrAuditSink` +
  `FileAuditSink` combinados em vez de só o primeiro.
- **Arquivos no escopo:** novo `crates/core/src/transport/audit_file.rs` (ou equivalente),
  `crates/cli/src/main.rs`.
- **Critério de aceite:** testes — entrada gravada vira uma linha JSON válida no arquivo;
  falha de escrita (ex.: diretório sem permissão) não propaga erro pra chamada de rede;
  `stderr` continua recebendo as mesmas entradas de sempre (regressão zero). *Smoke-test*
  real: rodar uma tarefa, conferir `.agentry/audit.log` populado.
- **Depende de:** MT-124.

---

## Fase J — Configuração global do usuário (ADR-0038)

Sequenciada depois da Fase G/H, antes da próxima atualização de release (pedido explícito do
mantenedor). `~/.agentry/` com dois arquivos de propósito distinto — `agentry.settings.json`
(preferências pessoais reutilizáveis, mesmo schema do arquivo por-projeto) e
`credentials.json` (só credenciais, schema separado, nunca soma ao schema git-versionado).
Variável de ambiente continua vencendo sempre — o arquivo global é *fallback* aditivo, zero
mudança de comportamento pra quem já usa `AGENTRY_LITELLM_API_KEY`.

### MT-126: ADR-0038 (Accepted) ✅ concluído
- **Objetivo:** registrar a decisão — dois arquivos (`agentry.settings.json`/
  `credentials.json`), precedência (`~/.agentry/ < .agentry/ do projeto < variável de
  ambiente`), permissão `0600` em `credentials.json`, resolução de `$HOME`/`%USERPROFILE%`
  sem dependência nova. Confirmado com o mantenedor antes de escrever: não conflita com a
  ADR-0017 (categoria diferente — credencial/preferência é por-usuário, não estado
  por-projeto).
- **Arquivos no escopo:** `docs/adr/0038-*.md` (novo), `docs/adr/README.md`, `mkdocs.yml`,
  `docs/roadmap-v0.16.md` (este arquivo).
- **Depende de:** nenhum.

### MT-127: Resolução de `~/.agentry/` + leitura de `agentry.settings.json` global
- **Objetivo:** novo helper de resolução de diretório *home* (`$HOME`/`%USERPROFILE%`, sem
  `dirs`/`directories`); `build_config` (`crates/cli/src/main.rs`) ganha a camada nova
  **antes** do arquivo de projeto: `~/.agentry/agentry.settings.json < .agentry/ do projeto
  < ambiente`. Arquivo global ausente não é erro (cai nos defaults, mesmo padrão de sempre).
- **Arquivos no escopo:** novo módulo (`crates/core/src/global_dir.rs` ou equivalente),
  `crates/cli/src/main.rs`.
- **Critério de aceite:** testes — preferência só no arquivo global aparece na `Config`
  resolvida; preferência no projeto sobrescreve a global; variável de ambiente sobrescreve as
  duas; sem `$HOME`/arquivo global, comportamento idêntico ao de hoje (regressão zero).
- **Depende de:** MT-126.

### MT-128: `credentials.json` — leitura com permissão verificada
- **Objetivo:** novo schema `credentials.json` (`providers.<nome>.apiKey`); leitura só como
  *fallback* quando a variável de ambiente correspondente (ex.: `AGENTRY_LITELLM_API_KEY`)
  não está definida — nunca os dois somados. Permissão mais aberta que `0600` gera aviso
  (`stderr`), nunca erro fatal.
- **Arquivos no escopo:** novo módulo (`crates/core/src/credentials.rs` ou equivalente),
  `crates/cli/src/main.rs` (`chave_litellm`, linha ~920).
- **Critério de aceite:** testes — variável de ambiente definida nunca consulta o arquivo;
  variável ausente lê a chave do arquivo; permissão aberta gera aviso sem falhar; arquivo
  ausente não é erro.
- **Depende de:** MT-126.

### MT-129: Comando/flag para gravar credencial (`--set-credential` ou equivalente)
- **Objetivo:** forma de o usuário gravar uma credencial em `credentials.json` sem editar o
  arquivo à mão — cria o diretório/arquivo com permissão `0600` desde a primeira escrita.
- **Arquivos no escopo:** `crates/cli/src/main.rs`, `docs/usuario/uso.md`,
  `docs/usuario/instalacao.md` (precedência documentada, pra não confundir usuário sobre por
  que uma preferência de projeto "ganhou" da global).
- **Critério de aceite:** testes — grava com permissão `0600`; sobrescreve valor existente;
  nunca imprime a credencial de volta no terminal (mesmo princípio da skill `secrets-guard`).
- **Depende de:** MT-128.

---

## Fase I — RAG estendido às sessões salvas (adiada, depende da Fase G)

Reaproveita o pipeline híbrido já completo do ADR-0011 (`tantivy` + `lancedb` + *reciprocal
rank fusion* + *reranking* via chat, `crates/core/src/context/rag/`), trocando o chunking
AST-aware (código) por um chunking por turno/mensagem (conversas são prosa, não código) sobre
o corpus de `.agentry/session/*.md`. **Não detalhada em micro-tickets ainda** — só faz sentido
depois que a Fase G estiver rodando de verdade e houver sessões reais salvas pra indexar; ADR
própria a escrever quando chegar a vez, incluindo a pergunta de que provider de embeddings é
aceitável rodar sobre conteúdo de sessão (mesma disciplina de classe de egresso do ADR-0002 —
embeddings locais via Ollama não levantam a questão, um provider de embeddings na nuvem
levantaria).

---

## Sequência crítica

```
MT-118 → MT-119 → MT-120 → MT-121 → MT-122      (Fase G, sessão -- sequencial)
                        └→ MT-123                (lista sessões, só depende do MT-119)
MT-124 → MT-125                                  (Fase H, audit log -- independente da Fase G)
MT-126 → MT-127                                  (Fase J, config global)
      └→ MT-128 → MT-129                         (credenciais, independente do MT-127)
Fase I (RAG sobre sessões)                        (depende da Fase G, sem tickets ainda)
```

Ordem de execução pedida pelo mantenedor: Fase G → Fase H → Fase J → atualizar release
(Fase E/F/G/H/J juntas) → Fase I fica para depois, sem data.
