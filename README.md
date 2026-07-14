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
  roadmap-v0.1.md..v0.4.md  # micro-tickets do roadmap, por fase
  CURRENT-STATE.md       # handoff — estado corrente entre turnos/sessões
  testing.md             # guia de testes (Linux/Windows) + scripts de automação
  adr/                   # Architecture Decision Records
  usuario/               # site MkDocs — guia do usuário
  governanca/            # site MkDocs — trilha de governança/compliance
mkdocs.yml               # config do site de documentação (ver seção "Documentação")
scripts/
  test.sh / test.ps1     # sequência de validação do CI (fmt/clippy/test/build), local
Makefile                 # atalhos de build/empacotamento (ver seção "Distribuir para Windows")
```

## Distribuir para Windows (cross-compile a partir do Linux)

Pré-requisitos (uma vez só — ver a seção "Cross-compile Linux → Windows" em
[`docs/testing.md`](docs/testing.md)):

```bash
sudo apt-get install -y mingw-w64
rustup target add x86_64-pc-windows-gnu
```

Depois, gerar o `.exe` e um `.zip` pronto pra copiar pra outra máquina:

```bash
make windows
```

Gera `dist/agentry-windows-x86_64-<versão>.zip` (`agentry.exe` + `README.md` + `LICENSE`).
`make windows-build` só compila, sem empacotar; `make windows-clean` remove `dist/`; `make`
sem argumento lista os alvos disponíveis.

## Testes

Ver [`docs/testing.md`](docs/testing.md) para o guia completo (configuração inicial,
comandos por SO, scripts de automação em `scripts/`).

## Documentação

Site de documentação completo (guia do usuário, trilha de governança/compliance para
avaliação de uso corporativo, e a documentação de desenvolvimento — arquitetura, ADRs,
roadmap) gerado via [MkDocs](https://www.mkdocs.org/) a partir de `docs/`:

```bash
# via uv (sem precisar de sudo/pip global)
uv venv .venv-docs
uv pip install --python .venv-docs/bin/python -r docs-requirements.txt
.venv-docs/bin/mkdocs serve   # http://127.0.0.1:8000, com live-reload
```

(ou `sudo apt install mkdocs-material` em Debian/Ubuntu, e `mkdocs serve` direto — ver
[`docs-requirements.txt`](docs-requirements.txt).) Nenhum deploy público está configurado
ainda; hoje o site é só para visualização local.

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
