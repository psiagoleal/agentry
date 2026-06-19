---
name: adr-writer
description: >-
  Cria e atualiza Registros de Decisão de Arquitetura (ADRs) no formato
  Status/Contexto/Decisão/Consequências/Conformidade, e exige consulta aos ADRs
  ativos antes de propor mudanças funcionais. Aciona ao decidir bibliotecas,
  solvers, padrões arquiteturais, restrições de stack, ou quando o usuário pedir
  para registrar/documentar uma decisão técnica.
---

# adr-writer — Registros de Decisão de Arquitetura

Os ADRs registram o contexto histórico e a justificativa técnica das escolhas de
design. Modelos de IA exibem **maior conformidade** quando compreendem o *porquê* de
uma limitação do que quando recebem apenas uma diretriz seca — fornecer o racional
reduz o risco de o agente propor refatorações incompatíveis.

## Regras de uso (constituem restrição cognitiva)

1. **Antes de propor qualquer mudança funcional**, leia os ADRs ativos em `docs/adr/`.
   Se a mudança conflita com um ADR `Accepted`, **pare e relate o conflito** — não
   contorne a decisão silenciosamente.
2. Toda decisão arquitetural relevante (escolha de biblioteca, solver, padrão, fronteira
   de stack, formato de dados) deve gerar um novo ADR numerado sequencialmente.
3. ADRs são imutáveis após `Accepted`: para reverter, crie um novo ADR com status
   `Superseded by ADR-NNNN` no antigo.

## Formato (use o template)

Veja `templates/adr-template.md`. Estrutura mínima:

- **Título:** `ADR NNNN: <decisão em uma linha>`
- **Status:** `Proposed | Accepted | Deprecated | Superseded by ADR-MMMM`
- **Contexto:** forças em jogo, restrições (orçamento, hardware, regulação, prazos).
- **Decisão:** o que foi decidido, em voz ativa ("Fica acordada a utilização de…").
- **Consequências:** impactos positivos e negativos, *trade-offs* aceitos.
- **Diretriz de Conformidade de Código:** o que é **proibido** e **obrigatório** no código
  do projeto (ex.: "proibido introduzir dependências proprietárias/licenciadas sem ADR").
  Redija como restrição do projeto, sem se dirigir a um agente nem mencionar uso de IA.

## Numeração e localização

- Arquivos em `docs/adr/NNNN-titulo-em-kebab-case.md`, com `NNNN` zero-padded (0001, 0002…).
- Mantenha um índice em `docs/adr/README.md` (status atual de cada ADR).

## Definição de pronto da skill

- [ ] ADR usa o template e tem os cinco blocos.
- [ ] A seção "Diretriz de Conformidade" é explícita e acionável.
- [ ] O índice `docs/adr/README.md` foi atualizado.
