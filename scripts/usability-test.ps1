# Caminho relativo: scripts/usability-test.ps1
#
# Teste de usabilidade: simula a primeira configuração e o primeiro uso
# simples do agentry, na perspectiva de quem acabou de clonar o repositório
# e ainda não tem nada configurado. NÃO é substituto de `cargo test` — este
# script testa a EXPERIÊNCIA (mensagens de erro, passos do README), não
# lógica interna.
#
# Uso: .\scripts\usability-test.ps1 [-Modelo <nome>] [-OllamaHost <host:porta>]

param(
    [string]$Modelo = "llama3.1:8b",
    [string]$OllamaHost = "127.0.0.1:11434"
)

$Bin = "target\release\agentry.exe"
$Falhas = 0

function Titulo($texto) { Write-Host ""; Write-Host "== $texto ==" }
function Ok($texto) { Write-Host "  [OK] $texto" }
function Falha($texto) { Write-Host "  [FALHA] $texto"; $script:Falhas++ }

function Test-OllamaAcessivel {
    try {
        Invoke-WebRequest -Uri "http://$OllamaHost/api/tags" -TimeoutSec 2 -UseBasicParsing | Out-Null
        return $true
    } catch {
        return $false
    }
}

function Test-ModeloPresente {
    try {
        $resposta = Invoke-WebRequest -Uri "http://$OllamaHost/api/tags" -TimeoutSec 2 -UseBasicParsing
        return $resposta.Content.Contains("`"$Modelo`"")
    } catch {
        return $false
    }
}

Titulo "1. Build do binário (simula 'acabei de clonar e vou compilar')"
if (Test-Path $Bin) {
    Ok "binário já existe em $Bin"
} else {
    Write-Host "  compilando (cargo build --release -p agentry)..."
    cargo build --release -p agentry
    if ($LASTEXITCODE -eq 0) {
        Ok "build concluído"
    } else {
        Falha "build falhou -- ver docs/testing.md (causa provável: protoc ausente)"
        Write-Host ""
        Write-Host "Resumo: $Falhas cenário(s) com problema."
        exit 1
    }
}

Titulo "2. '--help' não deve exigir nada configurado"
& $Bin --help *> "$env:TEMP\agentry-usability-help.out"
if ($LASTEXITCODE -eq 0) {
    Ok "--help roda sem exigir Ollama/config"
} else {
    Falha "--help falhou -- saída em $env:TEMP\agentry-usability-help.out"
}

Titulo "3. Ollama ausente/inacessível deve dar erro claro, não travar nem panicar"
if (Test-OllamaAcessivel) {
    Write-Host "  (Ollama já está acessível em $OllamaHost -- pulando este cenário;"
    Write-Host "   o cenário 5 já cobre o caminho com Ollama de verdade.)"
} else {
    $saida = & $Bin --ollama-host $OllamaHost "responda apenas com OK" 2>&1 | Out-String
    $codigo = $LASTEXITCODE
    $linhaErro = ($saida -split "`n" | Where-Object { $_ -match "^erro:" } | Select-Object -First 1)
    if ($codigo -ne 0 -and $saida -notmatch "panicked") {
        if (-not $linhaErro) { $linhaErro = ($saida -split "`n")[-1] }
        Ok "erro tratado (exit $codigo), sem panic -- mensagem: $linhaErro"
    } else {
        Falha "esperava erro tratado (Ollama fora do ar); saída: $saida"
    }
}

Titulo "4. Verificação do modelo padrão ('$Modelo') no Ollama"
$modeloPresente = Test-ModeloPresente
if ($modeloPresente) {
    Ok "modelo '$Modelo' já está puxado"
} else {
    Write-Host "  modelo '$Modelo' não encontrado (ou Ollama não está acessível)."
    Write-Host "  para rodar o cenário 5 de verdade: ollama pull $Modelo"
}

Titulo "5. Primeiro uso simples (one-shot, com modelo disponível)"
if ($modeloPresente) {
    $saida = & $Bin --model $Modelo --ollama-host $OllamaHost "responda apenas com a palavra OK" 2>&1 | Out-String
    $codigo = $LASTEXITCODE
    if ($codigo -eq 0) {
        $resumo = $saida.Replace("`n", " ")
        if ($resumo.Length -gt 200) { $resumo = $resumo.Substring(0, 200) }
        Ok "tarefa simples completou (exit 0). Saída: $resumo"
    } else {
        Falha "tarefa simples falhou (exit $codigo). Saída: $saida"
    }
} else {
    Write-Host "  pulado -- modelo/Ollama não disponível neste ambiente."
}

Write-Host ""
if ($Falhas -eq 0) {
    Write-Host "Resumo: nenhum problema de usabilidade encontrado."
} else {
    Write-Host "Resumo: $Falhas cenário(s) com problema -- ver acima."
    exit 1
}
