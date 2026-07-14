<!-- Caminho relativo: docs/governanca/dependencias.md -->

# Postura de dependências e cadeia de suprimentos

Base técnica: [ADR-0001](../adr/0001-fundacao-camada-llm.md) e
[ADR-0004](../adr/0004-postura-sinergia-open-source.md).

## Sem framework de agente por baixo

O `agentry` não é construído sobre um framework de agente de terceiros — a camada de
comunicação com modelos é uma abstração própria, fina, sobre uma única biblioteca HTTP de
base. Isso reduz a árvore de dependências transitivas a auditar e mantém controle total
sobre o único ponto de rede do processo (ver [Modelo de privacidade e
egresso](privacidade-e-egresso.md)) — não seria possível garantir "um único ponto de
transporte" com a mesma confiança se o transporte de rede estivesse espalhado dentro de um
framework de terceiros.

A biblioteca HTTP de base é configurada para usar uma implementação TLS que não depende da
biblioteca de sistema operacional (evita a superfície e o gerenciamento de patch de uma
dependência de TLS nativa por plataforma).

## Critério de adoção de dependência externa

Toda dependência nova exige, antes de ser adotada:

- **Verificação de maturidade** — atividade recente, histórico de releases, ausência de
  arquivamento.
- **Licença compatível** — MIT, Apache-2.0 ou BSD; nenhuma dependência sob licença
  incompatível ou não verificada é adotada.
- **Ausência de telemetria não desligável** — qualquer componente candidato que reporte uso,
  erros ou metadados a um serviço de terceiros por padrão é barrado, a menos que a
  telemetria seja comprovadamente 100% desligável e a integração passe por uma decisão de
  arquitetura registrada explicitamente para esse caso.

Regra geral declarada: **"padrão antes de dependência"** — quando um projeto de referência
externo populariza uma boa prática (ex.: um formato de compressão, um padrão de memória),
a preferência é reimplementar o *padrão* internamente em vez de importar o binário/biblioteca
de terceiros, especialmente quando a maturidade ou a postura de telemetria do projeto de
referência ainda não foi totalmente verificada.

## Nenhuma dependência adotada hoje tem telemetria conhecida

Todas as dependências atualmente compiladas no binário (processamento de texto/AST,
protocolo de *language server*, busca textual, cliente HTTP, serialização, runtime
assíncrono) são bibliotecas de propósito geral, sem canal de telemetria embutido conhecido.
Um candidato específico (compressão de saída de ferramentas, avaliado mas **ainda não
adotado como dependência binária**) menciona telemetria na própria documentação — por isso
permanece bloqueado até uma verificação explícita de que essa telemetria é 100% desligável;
o padrão que ele populariza pode, mesmo assim, ser reimplementado internamente sem trazer
o binário de terceiros junto.

## Transparência do processo de decisão

Toda decisão sobre dependência — adotada, avaliada e descartada, ou avaliada e ainda
pendente — fica registrada num Registro de Decisão de Arquitetura (ADR), com data,
contexto e critério aplicado. Nada é adotado silenciosamente fora desse processo; é
possível auditar o histórico completo de decisões de dependência lendo os ADRs do projeto.
