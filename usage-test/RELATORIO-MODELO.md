<!-- Caminho relativo: usage-test/RELATORIO-MODELO.md -->

# Relatório de teste de uso — `agentry` TUI

> Preencha uma cópia deste arquivo (ex.: `RELATORIO-2026-09-08.md`). Siga
> [`PLANO-DE-TESTE.md`](./PLANO-DE-TESTE.md).

## Identificação

| Campo | Valor |
|---|---|
| Data | |
| Commit testado (`git rev-parse --short HEAD`) | |
| Versão (`agentry --version`) | |
| SO / terminal / `tmux -V` | |
| Provider e modelo usados | |
| Executado por | |
| `--tui` era o padrão? | sim / não |

## Resumo

| | Quantidade |
|---|---|
| Cenários executados | |
| `PASSOU` | |
| `FALHOU` | |
| `BLOQUEADO` | |

**Veredito geral, em uma frase:**

**A TUI está pronta para ser o modo padrão?** (sim / não / com ressalvas — e por quê)

## Resultados por cenário

Repita o bloco abaixo para cada cenário. Cole a captura de `tmux capture-pane` em vez de
descrever a tela.

### `<ID>` — `<título>`

- **Veredito:** PASSOU / FALHOU / BLOQUEADO
- **O que foi feito:** (comandos e teclas exatos)
- **O que aconteceu:**

```
(captura de tela)
```

- **Divergência do esperado:** (vazio se nenhuma)
- **Impacto para quem usa:** (só se houve divergência)

## Achados

Ordene por impacto, do maior para o menor. Um achado por linha na tabela, detalhado embaixo.

| # | Tipo | Cenário | Resumo | Impacto |
|---|---|---|---|---|
| 1 | defeito / usabilidade / documentação | | | alto / médio / baixo |

### Achado 1 — `<título>`

- **Tipo:** defeito \| usabilidade \| documentação
- **Como reproduzir:** (passos mínimos)
- **Observado:**
- **Esperado:**
- **Por que importa para quem usa:**
- **Já conhecido?** (MT-147 / MT-148 / MT-149 / não)

## Achados não previstos pelo plano

Coisas que apareceram fora do roteiro. Estas costumam ser as mais valiosas — o roteiro só
encontra o que quem o escreveu já imaginava.

## Perguntas de usabilidade

Responda com frases, não com sim/não seco:

1. Depois de abrir a TUI pela primeira vez, **quanto tempo** até entender o que fazer?
2. Os atalhos (`Ctrl+P`, `Ctrl+A`, `Ctrl+Z`) são **descobríveis** sem ler o código?
3. Quando algo deu errado, a mensagem dizia **o que fazer a seguir**, ou só que falhou?
4. Alguma tela deixou dúvida sobre **para onde os dados estavam indo** (local vs. nuvem)?
5. Se você fosse usar isto no dia a dia, **o que te faria desistir**?

## Higiene pós-teste

| Verificação | OK? |
|---|---|
| `~/.agentry/` real não foi tocado | |
| `HOME` temporário só tem o que o teste criou | |
| Nenhum processo `agentry` órfão | |
| Terminal restaurado após `Ctrl+C` | |
| Nenhum segredo aparece neste relatório | |
