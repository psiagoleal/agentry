<!-- Caminho relativo: docs/usuario/instalacao.md -->

# Instalação

Não há binário pré-compilado distribuído ainda (v0.1) — a instalação é compilar a partir do
código-fonte.

## Pré-requisitos

- **Rust**, via [rustup](https://rustup.rs/) — Linux, macOS ou Windows.
- **[Ollama](https://ollama.com/)** instalado e rodando localmente, com ao menos um modelo
  já puxado:

  ```bash
  ollama pull llama3.1:8b
  ```

  (`llama3.1:8b` é o modelo *default* da CLI; qualquer outro modelo servido pelo Ollama
  funciona — veja a flag `--model` em [Uso da CLI](uso.md).)

- **`protoc`** (compilador de Protocol Buffers) — exigido só em **tempo de build**, pelo
  *build script* de uma dependência transitiva do RAG semântico. Não é necessário depois do
  binário compilado. Instalação por sistema operacional e alternativas para ambientes sem
  `sudo`/`choco` interativo: ver o [guia de testes](../testing.md) no repositório
  (seções "Linux"/"Windows" e "Ambientes *sandboxed* sem `sudo`/`choco` interativo").

## Compilar

```bash
git clone https://github.com/psiagoleal/agentry.git
cd agentry
cargo build --all --release
```

O binário fica em `target/release/agentry` (`target\release\agentry.exe` no Windows).

## Verificar a instalação

```bash
./target/release/agentry --help
```

Se o Ollama estiver rodando e o modelo *default* já tiver sido puxado, um teste rápido:

```bash
./target/release/agentry "diga oi"
```

Próximo passo: [Configuração](configuracao.md), para personalizar modelo, permissões e
guardrails via `agentry.settings.json` — ou pule direto para [Uso da CLI e do
REPL](uso.md) se os *defaults* já servem.
