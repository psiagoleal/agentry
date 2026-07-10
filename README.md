<!-- Caminho relativo: README.md -->

# agentry

> CLI agêntico de codificação em Rust, multi-provedor (modelos locais e de nuvem), com
> roteamento por classe de privacidade e controle auditável de egresso. Projeto irmão do
> `ai-coding-agent-profiles` (camada de política).

![build](https://img.shields.io/badge/build-passing-brightgreen)
![coverage](https://img.shields.io/badge/coverage-0%25-lightgrey)
![version](https://img.shields.io/badge/version-0.1.0-blue)
![license](https://img.shields.io/badge/license-MIT-green)

Na v0.1, o `agentry` fala com um servidor [Ollama](https://ollama.com/) local — sem
nenhuma tarefa saindo da máquina por padrão (ADR-0002). Adapters de nuvem
(OpenAI-compatible, Anthropic) já existem no core, mas a fiação de configuração para
escolhê-los na CLI é trabalho futuro.

## Pré-requisitos

- **Rust** via [rustup](https://rustup.rs/) (Linux, macOS ou Windows — ver
  [`docs/testing.md`](docs/testing.md) para detalhes por SO).
- **[Ollama](https://ollama.com/)** instalado e rodando localmente, com ao menos um
  modelo já puxado:
  ```bash
  ollama pull llama3.1:8b
  ```
  (`llama3.1:8b` é o modelo *default*; qualquer outro serve, veja a flag `--model`
  abaixo.)
- **`protoc`** (compilador de Protocol Buffers) — exigido só em tempo de **build**
  (não pelo binário já compilado), pelo *build script* de uma dependência transitiva
  (`lance-encoding`, MT-27/ADR-0011). Instalação por SO e alternativas para ambientes
  sem `sudo`/`choco` interativo: ver [`docs/testing.md`](docs/testing.md).

## Instalação

```bash
git clone https://github.com/psiagoleal/agentry.git
cd agentry
cargo build --all --release
```

O binário fica em `target/release/agentry` (`target\release\agentry.exe` no Windows).

## Uso

```bash
# Modo one-shot: roda uma tarefa e sai.
./target/release/agentry "liste os arquivos .rs deste projeto"

# Modo REPL: sem tarefa na invocação, entra em modo interativo (/exit ou /quit pra sair).
./target/release/agentry
```

Flags de override por invocação (ou, no REPL, comandos de barra equivalentes —
`/model`, `/temperature` etc.):

```bash
./target/release/agentry --model llama3.1:70b --temperature 0.2 "revise este diff"
./target/release/agentry --ollama-host 127.0.0.1:11435 "..."  # outra porta/instância
```

## Estrutura de diretórios

```
crates/
  core/   # agentry_core — providers, router, sessão/agent loop, tools, contexto (RAG/repo-map/LSP)
  cli/    # agentry — binário: parsing de flags, REPL, streaming
docs/
  architecture.md       # visão geral de arquitetura
  roadmap-v0.1.md        # micro-tickets do roadmap
  CURRENT-STATE.md       # handoff — estado corrente entre turnos/sessões
  testing.md             # guia de testes (Linux/Windows) + scripts de automação
  adr/                   # Architecture Decision Records
scripts/
  test.sh / test.ps1     # sequência de validação do CI (fmt/clippy/test/build), local
```

## Testes

Ver [`docs/testing.md`](docs/testing.md) para o guia completo (configuração inicial,
comandos por SO, scripts de automação em `scripts/`).

## Como contribuir

1. Faça um *fork* e crie uma branch (`feature/minha-feature`).
2. Rode `./scripts/test.sh` (ou `.\scripts\test.ps1` no Windows) — mesma validação do CI.
3. Abra um PR descrevendo a mudança.

## Licença

Distribuído sob a licença MIT. Veja [`LICENSE`](./LICENSE).

---

## Apoie

**Feito com ❤️ por Iago Leal** | [☕ Apoie o criador]

Se este projeto ajudou você, considere apoiar:

- Buy Me a Coffee: https://buymeacoffee.com/psiagoleal

<a href="https://buymeacoffee.com/psiagoleal" target="_blank" rel="noopener">
  <img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me a Coffee" height="41" width="174" />
</a>
