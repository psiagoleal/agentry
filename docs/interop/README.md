<!-- Caminho relativo: docs/interop/README.md -->

# Interoperabilidade — `agentry` (lado executor)

Este repositório (`agentry`) é o **motor de execução** de um ecossistema de dois projetos.
A política que ele aplica é definida no projeto irmão `ai-coding-agent-profiles`.

## Contrato

- **Fonte canônica do contrato:** `ai-coding-agent-profiles` → `docs/interop/SPEC.md`.
- **Versão do contrato suportada por este repo:** `1`.
- **Regra fail-closed:** se a versão do SPEC consumido divergir da suportada, `agentry`
  **aborta** em vez de degradar confidencialidade silenciosamente.

## Charter (resumo)

- `profiles` = **política**: define regras, perfis e o esquema dos artefatos. Não executa.
- `agentry` = **execução**: lê os artefatos e **impõe** (egresso, permissões, skills). Não inventa política.

Detalhes e a tabela de artefatos/versões de esquema: ver o `SPEC.md` canônico.

## `exchange-log.md`

Registro **append-only** das trocas entre os dois projetos (pedidos, decisões, mudanças de
esquema). Anexe entradas datadas; **nunca** reescreva entradas antigas. Decisões vinculantes
viram ADR no repo dono e são referenciadas na entrada do log.
