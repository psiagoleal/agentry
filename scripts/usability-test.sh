#!/usr/bin/env bash
# Caminho relativo: scripts/usability-test.sh
#
# Teste de usabilidade: simula a primeira configuração e o primeiro uso
# simples do agentry, na perspectiva de quem acabou de clonar o repositório
# e ainda não tem nada configurado. NÃO é substituto de `cargo test` — este
# script testa a EXPERIÊNCIA (mensagens de erro, passos do README), não
# lógica interna.
#
# Uso: ./scripts/usability-test.sh [--model <nome>] [--ollama-host <host:porta>]

set -uo pipefail  # sem -e: cada cenário decide seu próprio sucesso/falha

MODELO="llama3.1:8b"
OLLAMA_HOST="127.0.0.1:11434"
while [[ $# -gt 0 ]]; do
    case "$1" in
        --model) MODELO="$2"; shift 2 ;;
        --ollama-host) OLLAMA_HOST="$2"; shift 2 ;;
        *) shift ;;
    esac
done

BIN="target/release/agentry"
FALHAS=0

titulo() { echo; echo "== $1 =="; }
ok()     { echo "  [OK] $1"; }
falha()  { echo "  [FALHA] $1"; FALHAS=$((FALHAS + 1)); }

titulo "1. Build do binário (simula 'acabei de clonar e vou compilar')"
if [[ -x "$BIN" ]]; then
    ok "binário já existe em $BIN"
else
    echo "  compilando (cargo build --release -p agentry)..."
    if cargo build --release -p agentry; then
        ok "build concluído"
    else
        falha "build falhou — ver docs/testing.md (causa provável: protoc ausente)"
        echo
        echo "Resumo: $FALHAS cenário(s) com problema."
        exit 1
    fi
fi

titulo "2. '--help' não deve exigir nada configurado"
if "$BIN" --help >/tmp/agentry-usability-help.out 2>&1; then
    ok "--help roda sem exigir Ollama/config"
else
    falha "--help falhou — saída em /tmp/agentry-usability-help.out"
fi

titulo "3. Ollama ausente/inacessível deve dar erro claro, não travar nem panicar"
if curl -s -m 2 "http://${OLLAMA_HOST}/api/tags" >/dev/null 2>&1; then
    echo "  (Ollama já está acessível em ${OLLAMA_HOST} — pulando este cenário;"
    echo "   o cenário 5 já cobre o caminho com Ollama de verdade.)"
else
    saida=$(timeout 15 "$BIN" --ollama-host "$OLLAMA_HOST" "responda apenas com OK" 2>&1)
    codigo=$?
    mensagem_erro=$(echo "$saida" | grep "^erro:" | head -1)
    if [[ $codigo -ne 0 ]] && ! echo "$saida" | grep -qi "panicked"; then
        ok "erro tratado (exit $codigo), sem panic — mensagem: ${mensagem_erro:-$(echo "$saida" | tail -1)}"
    else
        falha "esperava erro tratado (Ollama fora do ar); saída: $saida"
    fi
fi

titulo "4. Verificação do modelo padrão ('$MODELO') no Ollama"
MODELO_PRESENTE=0
if curl -s -m 2 "http://${OLLAMA_HOST}/api/tags" 2>/dev/null | grep -q "\"$MODELO\""; then
    ok "modelo '$MODELO' já está puxado"
    MODELO_PRESENTE=1
else
    echo "  modelo '$MODELO' não encontrado (ou Ollama não está acessível)."
    echo "  para rodar o cenário 5 de verdade: ollama pull $MODELO"
fi

titulo "5. Primeiro uso simples (one-shot, com modelo disponível)"
if [[ "$MODELO_PRESENTE" -eq 1 ]]; then
    saida=$(timeout 60 "$BIN" --model "$MODELO" --ollama-host "$OLLAMA_HOST" "responda apenas com a palavra OK" 2>&1)
    codigo=$?
    if [[ $codigo -eq 0 ]]; then
        resumo=$(echo "$saida" | tr '\n' ' ' | cut -c1-200)
        ok "tarefa simples completou (exit 0). Saída: $resumo"
    else
        falha "tarefa simples falhou (exit $codigo). Saída: $saida"
    fi
else
    echo "  pulado — modelo/Ollama não disponível neste ambiente."
fi

echo
if [[ $FALHAS -eq 0 ]]; then
    echo "Resumo: nenhum problema de usabilidade encontrado."
else
    echo "Resumo: $FALHAS cenário(s) com problema — ver acima."
    exit 1
fi
