<!-- Caminho relativo: docs/CURRENT-STATE.md -->

# Estado Corrente (Handoff)

> **Fonte central de sincronização — contém apenas o estado corrente.**
> Atualizado a cada commit. Não inclua segredos. Mantido conforme a skill `handoff-updater`.
>
> **Histórico completo:** [`docs/handoff-arquivo.md`](./handoff-arquivo.md) — rodadas
> anteriores, fases fechadas e a tabela de commits. Consulte sob demanda; não é necessário
> ler para retomar o trabalho.

## Último turno

- **Data:** 2026-09-12
- **Branch:** `chore/build-linux-e-higiene-de-disco` — **no remoto** (41 commits), ainda
  **não mesclada em `main`**
- **Último commit:** `05de1fa` (MT-168). A rodada anterior (MT-158 a MT-165) está arquivada em
  [`handoff-arquivo.md`](./handoff-arquivo.md) como **Rodada 12**.
- **Estado da árvore:** `AGENTS.md`, `skills/README.md` e os diretórios de skills não
  rastreados seguem modificados de **outra frente**, intocados. Há um binário `agentry` de
  ~280 MB solto na raiz (não rastreado, fora do `.gitignore`) — cópia de build; remover é
  decisão do dono.
- **DoD:** `cargo fmt` aplicado, `cargo clippy --workspace --all-targets -- -D warnings` com
  saída **0**, suíte completa **865 testes verdes**.

## O que o primeiro CI da matriz encontrou (MT-146, MT-165 a MT-168)

Cinco execuções, cinco causas distintas, **nenhuma** detectável por leitura de código ou pela
suíte local — todas estavam verdes havia meses porque nunca tinham rodado fora de uma máquina de
desenvolvimento Linux:

| # | Commit | Causa | Onde |
|---|---|---|---|
| 1 | `fb08f6b` | `claude` ausente do `PATH` do runner | teste |
| 2 | `9b314bc` | relógio do Windows avança em passos de ~15 ms; diretórios temporários colidiam | teste |
| 3 | `6ff2760` | `glob`/`code_search`/`repo_map` devolviam caminho com separador nativo | **produção** |
| 4 | `cb344b3` | isenção das próprias guardas comparava caminho presumindo `/` | guarda |
| 5 | `05de1fa` | `file://` URI inválido no Windows — `lsp_hover` inutilizável lá | **produção** |

**A regra que sai disso:** código que pergunta ao sistema *"como é o mundo aqui?"* — ou que
**presume uma propriedade** do que observou — amarra o resultado à máquina de quem roda. Sondar
na fronteira e injetar; construir o que não se pode observar; e nunca comparar caminho como texto
presumindo o separador. As quatro guardas estáticas de `crates/core/tests/hermetismo.rs`
endereçam parte da família — e o MT-167 mostrou que **elas próprias não estão imunes a ela**.

## Em andamento

**Nada bloqueado.** O MT-146 fechou em 2026-09-14 com o critério de aceite cumprido — duas
execuções verdes seguidas nos três SOs, `34841143019` (`05de1fa`) e `34848388663` (`e99078b`).
**A Fase K está encerrada**, e o CI passa a ser a rede que antes não existia.

O método que funcionou nas cinco rodadas de diagnóstico, para quem precisar: `gh run view <id>
--json jobs` para saber **quais** SOs caíram, depois `gh run view <id> --log-failed | grep -E
"test result: FAILED|panicked at|Process completed with exit code"`, e então `grep -A 3` no
`panicked at` para ler a mensagem. **Não** faça `grep` amplo por `error` no log — ele casa com
`--error-format=json` e despeja o log inteiro no contexto.

**Gateway LiteLLM alcançável desta máquina** — endereço e chave ficam **fora do repositório**.
`qwen3-coder:30b` faz *tool-calling*. **Nunca ecoar a chave:** `agentry --set-credential litellm`
lê de `stdin` (grava `0600`, mantém o valor fora de `argv`). Desde o MT-158 o endereço vem de
`AGENTRY_LITELLM_BASE_URL`, então `usage-test/exemplos/02-gateway-litellm.json` roda sem edição.
O gateway interno está declarado **`cloud-opt-out`** (decisão do mantenedor, `d4e7151`), o que o
torna alcançável sob o perfil `externo-confidencial` que o `.zshrc` exporta. Existe a skill
`delegacao-litellm` para mandar trabalho volumoso ao gateway em vez de gastar cota da assinatura.

## Próximo passo sugerido

1. **Mesclar `chore/build-linux-e-higiene-de-disco` em `main`.** São 47 commits com o CI verde
   atrás deles; a branch nunca foi mesclada, e quanto mais ela anda, mais cara fica. **Decisão do
   mantenedor.**
2. **MT-164** — mostrar a camada de origem de cada valor da configuração. Custou uma rodada de
   teste de uso; é diagnóstico que qualquer pessoa vai precisar.
3. **MT-153** — promover a TUI a modo padrão, com `--repl` como escape hatch e *fallback* para
   texto quando não houver TTY (`std::io::IsTerminal`). **Desbloqueado** por `882abeb`; exige
   ADR nova, citando o relatório de uso como a evidência que a ADR-0027 pedia.
4. **MT-160** — trilha de decisão, observabilidade além do egresso. Absorve MT-149 e MT-155 e
   exige ADR. É a maior lacuna para homologação: o projeto **sabe impedir e não sabe contar**.
5. **Frente nova proposta pelo mantenedor (2026-09-12): frontend local com acesso remoto** —
   SvelteKit 5 servido pelo processo no PC, acessível por LAN/VPN do celular ou notebook.
   **Precede o código uma ADR de ingresso:** hoje toda a conformidade do projeto (`Transport`,
   allowlist, `egressClass`, audit log) descreve **saída**; um frontend remoto abre **entrada**,
   e o `egressClass` não diz nada sobre ela. Requisitos mínimos a decidir na ADR: *bind* em
   `127.0.0.1` por padrão, exposição na interface da VPN **opt-in explícito**, token por cliente
   e origem da requisição em cada `AuditEntry`. **Tauri 2.0 não resolve este caso** — ele
   empacota cliente desktop, e o que a LAN precisa é do servidor + SPA no navegador; Tauri
   entra depois, se houver cliente nativo. Fechar o MT-147 antes ajuda: ele é sobre corpo
   não-SSE, e o transporte do frontend será SSE ou WebSocket.
6. **Implementar a ADR-0044** — providers por assinatura via CLI oficial (Codex/Gemini) e por
   API. **Pré-requisito duro, ainda não verificado:** confirmar que cada CLI autoriza consumo
   programático sob a assinatura e qual o modo *headless* suportado. Quebrar com
   `micro-ticket-planner`: o molde da ADR-0040 exige `AuditEntry` por invocação e
   `EgressClass` verificada antes do *spawn* em cada provider.
7. **Detector de nome de pessoa** — lacuna que a ADR-0043 declara e não resolve; custo de falso
   positivo alto, decisão do mantenedor.

## Decisões pendentes do mantenedor

Nenhuma virou asserção, para não congelar comportamento antes da decisão:

- **ADR-0046 (hooks de ciclo)** está **`Proposed`**. Acrescenta execução arbitrária declarada em
  arquivo de configuração; quem ratifica é o mantenedor.
- **MT-147** — corpo **não-SSE** numa requisição de *stream* produz `stdout` vazio, `0 tokens` e
  **código 0** — indistinguível de "o modelo não teve o que responder", e é o que acontece com
  endpoint mal configurado ou proxy que intercepta.
- **MT-148** — a regressão do `providers.ollama.structuredOutput` (nenhuma tool executando no
  *default*) **segue sem rede de proteção**: o *flag* é do `OllamaProvider` e o `fake_provider`
  só fala OpenAI-compatible. O MT-144 cobriu a *classe* da falha, não aquele defeito.
- **MT-149 + MT-155** — **decidir juntos**: nem a recusa na camada de rota (MT-149) nem a
  negação de tool por `permissions.deny` (MT-155) deixam trilha persistente, enquanto o bloqueio
  no `Transport` deixa. É o mesmo formato: todo caminho *fail-closed* do projeto acerta o efeito
  e não registra nada. Ou a auditoria cobre só egresso — e isso vira texto explícito na ADR-0002
  —, ou passa a cobrir decisão de política, e os dois entram pela mesma porta.

## O que saber antes de mexer nos testes ponta a ponta

Sete armadilhas já pagas (hermetismo, `CARGO_BIN_EXE`, *defaults* de contexto, SSE, as duas
camadas do *fail-closed*, o invariante do `Ctrl+A` e o método de `tmux`) estão registradas em
[`docs/roadmap-v0.18.md`](./roadmap-v0.18.md), junto do MT-146. **Leia antes de escrever
qualquer caso novo** — cada uma custou um teste que passava sem verificar nada.

## Impedimentos de ambiente (não são bugs do código)

- **`protoc` é pré-requisito de build** (build script de `lance-encoding`, transitiva do
  `lancedb`) — sem ele nem o `clippy` compila. **Já resolvido** nesta máquina e no CI; ambiente
  novo precisa instalar antes de `cargo build`/`test` (`docs/testing.md`). Exige `sudo`, que não
  é interativo aqui — pedir ao usuário, nunca tentar sozinho.
- **Lacuna de ~140 GB entre `df` e arquivos visíveis (não é bug do projeto).** Assinatura de
  arquivo deletado preso em processo de **root**: nenhuma varredura acha o consumidor e
  `lsof +L1` sem privilégio não o vê. Pendente de `sudo lsof -nP +L1` pelo usuário, ou reboot.
  Consumidores legítimos mapeados, que não são a causa: Steam 162 GB, containers 49 GB,
  gnome-boxes 23 GB. `make disk` mostra o estado de `target/`; `make clean-debug` é **higiene
  recorrente**, não correção pontual.

## Impedimentos abertos

- **Não há workflow de *release*:** só `ci.yml`. Ninguém instala o `agentry` hoje sem compilar.
- **Roadmap de longo prazo original esgotado — só resta multimodal, bloqueada.** Fases 11 a 20
  concluídas. A Fase 21+ está bloqueada pela resposta do mantenedor (2026-07-16,
  `docs/decisoes-autonomas.md`): adiada até existir um *guardrail* de imagem, já que os
  guardrails hoje só inspecionam texto e multimodal sem isso abriria um canal não auditado.
  Provavelmente exige **dependência nova** (OCR). **Decisão pendente:** qual caminho seguir, ou
  apontar uma frente nova.
- **ADR-0004 pendente de dado:** maturidade real de `rtk`/`caveman`/`ponytail` não verificada
  via `gh repo view`. Verificar antes de qualquer adoção como dependência.
- **Copilot/GitHub Enterprise:** caminho oficial (GitHub Models vs. API Enterprise) indefinido
  pela empresa; adapter adiado.
- **Verificação de "processo não órfão" do MT-23 é Unix-only de fato:** `processo_existe`
  (`crates/core/tests/lsp_client.rs`) usa `kill -0` e devolve `false` em `#[cfg(not(unix))]`,
  então em Windows os dois testes de ciclo de vida do `LspClient` passam **vacuamente**. Falta
  uma verificação real (ex.: `tasklist`) agora que a matriz de CI roda.
