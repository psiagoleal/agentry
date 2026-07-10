# Caminho relativo: scripts/test.ps1
#
# Roda localmente a mesma sequência de validação do job `lint`+`build-test`
# de .github/workflows/ci.yml (fmt --check, clippy, test, build --release) —
# ver docs/testing.md. Uso: .\scripts\test.ps1

$ErrorActionPreference = "Stop"

function Invoke-Step {
    param([string]$Descricao, [string]$Executavel, [string[]]$Argumentos)
    Write-Host "==> $Descricao"
    & $Executavel @Argumentos
    if ($LASTEXITCODE -ne 0) {
        Write-Error "Falhou: $Descricao (exit code $LASTEXITCODE)"
        exit $LASTEXITCODE
    }
}

Write-Host "==> Verificando protoc (exigido por lance-encoding, MT-27/ADR-0011)..."
$protoc = Get-Command protoc -ErrorAction SilentlyContinue
if (-not $protoc) {
    Write-Error "'protoc' não encontrado no PATH. Instale via 'choco install protoc -y' (ver docs/testing.md)."
    exit 1
}
Write-Host "    $(protoc --version)"

Invoke-Step "cargo fmt --all -- --check" "cargo" @("fmt", "--all", "--", "--check")
Invoke-Step "cargo clippy --all-targets --all-features -- -D warnings" "cargo" @("clippy", "--all-targets", "--all-features", "--", "-D", "warnings")
Invoke-Step "cargo test --all --verbose" "cargo" @("test", "--all", "--verbose")
Invoke-Step "cargo build --all --release" "cargo" @("build", "--all", "--release")

Write-Host ""
Write-Host "Tudo verde. Binário em target\release\agentry.exe"
