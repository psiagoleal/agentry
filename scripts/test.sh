#!/usr/bin/env bash
# Caminho relativo: scripts/test.sh
#
# Roda localmente a mesma sequência de validação do job `lint`+`build-test`
# de .github/workflows/ci.yml (fmt --check, clippy, test, build --release) —
# ver docs/testing.md. Uso: ./scripts/test.sh

set -euo pipefail

echo "==> Verificando protoc (exigido por lance-encoding, MT-27/ADR-0011)..."
if ! command -v protoc >/dev/null 2>&1; then
    echo "erro: 'protoc' não encontrado no PATH." >&2
    echo "Instale via 'sudo apt-get install -y protobuf-compiler' (Debian/Ubuntu)," >&2
    echo "'brew install protobuf' (macOS), ou o gerenciador nativo da sua distro." >&2
    echo "Em ambiente sandboxed sem sudo interativo, ver a seção correspondente" >&2
    echo "em docs/testing.md (variáveis PROTOC/PROTOC_INCLUDE)." >&2
    exit 1
fi
echo "    $(protoc --version)"

echo "==> cargo fmt --all -- --check"
cargo fmt --all -- --check

echo "==> cargo clippy --all-targets --all-features -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings

echo "==> cargo test --all --verbose"
cargo test --all --verbose

echo "==> cargo build --all --release"
cargo build --all --release

echo
echo "Tudo verde. Binário em target/release/agentry"
