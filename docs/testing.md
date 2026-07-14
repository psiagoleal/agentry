<!-- Caminho relativo: docs/testing.md -->

# Guia de Testes (Linux e Windows)

> Espelha exatamente os jobs `lint`/`build-test` de `.github/workflows/ci.yml`
> (ADR-0005 — Linux/Windows/macOS são plataformas tier-1). Rodar este guia
> localmente antes de abrir um PR pega a maioria dos problemas antes do CI.

## Visão geral

A sequência de validação é sempre a mesma, em qualquer SO:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --verbose
cargo build --all --release
```

Um único pré-requisito não-óbvio existe além do toolchain Rust: **`protoc`**
(compilador de Protocol Buffers), exigido pelo *build script* do
`lance-encoding` (dependência transitiva do `lancedb`, MT-27/ADR-0011) — sem
ele, nem `cargo clippy` compila o crate `agentry-core`.

## Pré-requisitos comuns

- **Rust** via [rustup](https://rustup.rs/) — versão mínima em
  `Cargo.toml` (`rust-version`). `rustup show` confirma a *toolchain* ativa.
- **`protoc`** — ver instalação por SO abaixo.
- Repositório clonado com `git clone` (os testes não dependem de rede além
  de baixar dependências do `crates.io` na primeira compilação).

## Linux

### Configuração inicial

```bash
# Toolchain Rust (pula se já tiver rustup instalado)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# protoc (Debian/Ubuntu)
sudo apt-get update && sudo apt-get install -y protobuf-compiler

# Verificação
protoc --version
cargo --version
```

Em distros sem `apt` (Fedora/Arch/etc.), use o gerenciador nativo
(`dnf install protobuf-compiler`, `pacman -S protobuf`, ...) — o pacote
sempre se chama algo próximo de `protobuf-compiler`/`protobuf`.

### Comandos de teste

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --verbose
cargo build --all --release
```

### Smoke-test manual do binário

```bash
./target/release/agentry --help
```

## Windows

### Configuração inicial

- **Rust**: instale via [rustup-init.exe](https://rustup.rs/). O *target*
  padrão é `x86_64-pc-windows-msvc`, que exige as **Visual Studio Build
  Tools** (workload "Desktop development with C++") — o instalador do
  `rustup` avisa e oferece o link se faltar.
- **`protoc`** via [Chocolatey](https://chocolatey.org/) (mesma abordagem do
  CI, `.github/workflows/ci.yml`):
  ```powershell
  choco install protoc -y
  ```

### Comandos de teste (PowerShell)

```powershell
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all --verbose
cargo build --all --release
```

### Smoke-test manual do binário

```powershell
.\target\release\agentry.exe --help
```

> **WSL2**: se o seu ambiente de desenvolvimento é WSL2 (Ubuntu), rodar os
> testes *dentro* do WSL cobre o caminho **Linux**, não o Windows nativo — o
> binário resultante (`target/release/agentry`, sem `.exe`) é um ELF, não um
> PE32+. Para testar o caminho Windows de verdade, rode os comandos acima a
> partir de um PowerShell/`cmd` fora do WSL (ou numa VM/máquina Windows).

## Ambientes sandboxed sem `sudo`/`choco` interativo

Se o `sudo`/gerenciador de pacotes não estiver disponível de forma
interativa (comum em contêineres/CI restritos), baixe um binário `protoc`
*standalone* das
[releases oficiais](https://github.com/protocolbuffers/protobuf/releases)
(`protoc-<versão>-linux-x86_64.zip`/`-win64.zip`) e aponte as variáveis de
ambiente abaixo — o binário solto **não** embute os *well-known types*
(`google/protobuf/*.proto`), por isso `PROTOC_INCLUDE` é obrigatório junto:

```bash
export PROTOC=/caminho/para/protoc
export PROTOC_INCLUDE=/caminho/para/include   # pasta "include/" do zip da release
```

Instalado via gerenciador de pacote nativo (`apt`/`choco`/`brew`), essas
variáveis **não são necessárias** — o pacote já registra os *well-known
types* no local padrão que `protoc` procura sozinho.

## Cross-compile Linux → Windows (opcional)

Só relevante se você precisa gerar um `.exe` a partir de uma máquina Linux
sem acesso a uma máquina/VM Windows — o caminho recomendado para validar
Windows de verdade continua sendo a seção **Windows** acima (ou o CI, que
já roda em `windows-latest`).

```bash
sudo apt-get install -y mingw-w64
rustup target add x86_64-pc-windows-gnu
```

**Pegadinha conhecida:** o `mingw-w64` do Debian/Ubuntu registra duas
variantes de *threading model* via `update-alternatives`
(`x86_64-w64-mingw32-gcc-posix`/`-win32`); o *default* costuma ser `win32`,
mas o `std` do Rust exige `posix`. Se `sudo update-alternatives --set` não
estiver disponível interativamente, aponte o linker direto para a variante
`posix` num `.cargo/config.toml` **local, não versionado**:

```toml
# .cargo/config.toml (não commitar — específico desta máquina)
[target.x86_64-pc-windows-gnu]
linker = "/usr/bin/x86_64-w64-mingw32-gcc-posix"
ar = "/usr/bin/x86_64-w64-mingw32-gcc-ar"
```

```bash
cargo build --release --target x86_64-pc-windows-gnu
# binário em target/x86_64-pc-windows-gnu/release/agentry.exe
```

Com o `.cargo/config.toml` acima já resolvido, `make windows` (raiz do repo) faz esse build
e ainda empacota `agentry.exe` + `README.md` + `LICENSE` num `.zip` pronto para copiar a
outra máquina, em `dist/agentry-windows-x86_64-<versão>.zip` — ver a seção "Distribuir para
Windows" do [`README.md`](../README.md).

## Scripts de automação

- `scripts/test.sh` (Linux/macOS) — `./scripts/test.sh`
- `scripts/test.ps1` (Windows) — `.\scripts\test.ps1`

Ambos rodam a sequência completa (`fmt --check` → `clippy` → `test` →
`build --release`), verificam `protoc` antes de começar (com uma mensagem
de erro clara se faltar) e param no primeiro passo que falhar — mesmo
comportamento do CI, para reproduzir localmente um problema visto lá.

## Teste de usabilidade (primeira configuração + primeiro uso)

`cargo test` valida lógica interna; **não** valida a experiência de quem
acabou de clonar o repositório e nunca rodou o `agentry` antes — mensagens
de erro, passos do README, o que acontece se o Ollama não estiver rodando
ou o modelo ainda não tiver sido puxado. Para isso:

- `scripts/usability-test.sh` (Linux/macOS) — `./scripts/usability-test.sh`
- `scripts/usability-test.ps1` (Windows) —
  `.\scripts\usability-test.ps1`

Ambos simulam, em sequência: (1) build do binário do zero; (2) `--help`
sem nada configurado; (3) Ollama ausente/inacessível — deve dar erro
tratado, nunca *panic*; (4) verificação se o modelo *default*
(`llama3.1:8b`) já foi puxado; (5) uma tarefa *one-shot* simples de
verdade, se o Ollama e o modelo estiverem disponíveis. Aceitam
`--model`/`--ollama-host` (`-Modelo`/`-OllamaHost` no PowerShell) para
testar contra outro modelo/instância.

**Exige um Ollama real** — os cenários 4/5 são pulados (não falham) se
nenhum Ollama estiver acessível no host informado; rode numa máquina que
já tenha o Ollama (ou um container equivalente) para exercitar o teste
completo.

> Achado real desta sessão: a mensagem de erro útil (`erro: erro do
> provider: ...`) vinha **depois** de uma linha `[audit] AuditEntry { ... }`
> (o *dump* de Debug do audit log, `StderrAuditSink` em
> `crates/cli/src/main.rs`) — poluindo a saída de stderr para quem só quer
> entender por que a tarefa falhou. **Corrigido**: `AuditEntry` ganhou um
> `impl Display` (uma linha compacta, ex.: `chat_stream ->
> http://127.0.0.1:11434/api/chat (local-only, allowed)`), usado no lugar
> de `{:?}`.
