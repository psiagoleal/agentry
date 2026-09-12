<!-- Caminho relativo: docs/adr/0046-hooks-de-ciclo-sensor-e-gate-de-conclusao.md -->

# ADR 0046: Hooks de ciclo — sensor após a ação e gate de conclusão

- **Status:** Proposed
- **Data:** 2026-09-12
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** arquitetura, segurança, verificação, extensibilidade

## Contexto

O `agentry` tem hoje **duas** camadas determinísticas, e as duas agem **antes** da ação:
permissões (ADR-0041) decidem se a tool roda; guardrails (ADR-0007) e detectores de PII
(ADR-0043) decidem se o conteúdo passa. Não existe nada **depois**: nenhum sensor que rode sobre
o que a tool acabou de escrever, e nenhum portão que decida se o turno pode de fato encerrar.

A consequência é registrada em [`analise-harness-engineering.md`](../analise-harness-engineering.md):
a única condição de sucesso do laço hoje é `StopReason::Done`, que significa apenas *"o modelo
parou de pedir tools"* — não *"o resultado passa em algum critério"*. O agente é, na prática,
juiz do próprio trabalho.

A demanda chegou por dois caminhos independentes, e é isso que a torna concreta:

1. **O mapeamento do projeto contra o vocabulário de *harness engineering*** aponta
   "verificação" como lacuna: a literatura separa *guias* (agem antes, reduzem o espaço de erro)
   de *sensores* (agem depois, capturam o que escapou), e o projeto só tem guias.

2. **Frameworks de engenharia agentic já existentes exigem o mecanismo.** O material de
   referência do mantenedor (`cepia`) é um núcleo portável entre seis *harnesses* — Claude Code,
   Codex, ZCode, Copilot, Gemini CLI e OpenCode — cujo mecanismo central é um arquivo de gates
   com prova executável:

   ```
   - [x] G2: skills, agents e manifests do núcleo continuam válidos
     CHECK: node scripts/check.mjs
     EXPECT: CHECK OK
     EVIDENCE: exit=0; EXPECT=matched; output-sha256=70a77c...; output-bytes=66
   ```

   Um gate desses **só vale se o runtime impedir o encerramento**. Escrito como instrução no
   prompt, ele volta a ser probabilístico — que é exatamente o que o gate existe para evitar. O
   `agentry` **não está** na lista de *harnesses* suportados por esse núcleo, e não está
   justamente por não ter esses pontos de extensão.

O que torna a decisão difícil não é o valor, é o custo: **um hook é execução arbitrária
declarada em arquivo de configuração**. Num projeto cujo diferencial é egresso auditado e
recusa *fail-closed*, isso é uma capacidade nova e perigosa — um `agentry.settings.json` herdado
de um repositório clonado passaria a poder rodar comando na máquina de quem abrisse o projeto.
O precedente existe e é instrutivo: `mcpServers` (ADR-0028) já roda subprocesso declarado em
configuração, mas **só** com `egressClass` obrigatória e restrita a `local-only`. E o MT-151
custou caro justamente por subestimar um valor de exemplo que era executado.

## Decisão

Fica acordada a adoção de **hooks de ciclo** no `agentry`, restritos a **dois** pontos e sob
sete condições de conformidade.

### 1. Dois pontos do ciclo, não seis

- **`PostToolUse`** — roda depois de uma tool executar, sobre o resultado dela. É o *sensor*: o
  lugar de `cargo fmt`, `ruff`, `tsc`, um teste focado. Saída não-zero volta ao modelo **como
  observação do mesmo turno**, para ele corrigir, não como erro fatal.
- **`Stop`** — roda quando o modelo declara o turno pronto. É o *gate de conclusão*: veto
  devolve o motivo e força mais um turno.

**`PreToolUse` fica deliberadamente de fora.** Ele seria redundante com as permissões, que já
são fronteira estrutural, já têm semântica declarativa (`deny`/`ask`/`readAllow`) e já são
auditáveis sem executar nada. Trocar uma política declarada por um comando arbitrário no
caminho pré-ação seria **piorar** a capacidade de prestar contas, não melhorar.

### 2. Hook só vem do arquivo do projeto

Hook declarado na camada **global** (`~/.agentry/agentry.settings.json`) ou na camada de
**ambiente** é erro tratado ao carregar a configuração. Motivo: a camada global é preferência
pessoal e não passa por revisão de ninguém; a de ambiente não é versionada. Um hook precisa ser
**revisável no mesmo diff que o código** — é o que separa "o time decidiu" de "alguém na
máquina decidiu".

### 3. `egressClass` obrigatória, só `local-only`

Mesmo contrato de `mcpServers` (ADR-0028), pela mesma razão: um hook é um subprocesso local, no
mesmo modelo de confiança de um *language server* (ADR-0013). Declarar qualquer outra classe é
erro tratado. Hook que fale com a rede fica fora desta versão e exige ADR própria.

### 4. Toda execução de hook é auditada

Comando, código de saída e duração entram no `audit.log`. Um hook é um **efeito**, e efeito não
registrado é precisamente o que a ADR-0002 proíbe. Isto também é o que permite responder "o gate
reprovou, ou nem rodou?" — pergunta que não tem resposta se a execução for silenciosa.

### 5. `Stop` exige teto de tentativas declarado

Gate sem critério de desistência vira laço infinito — o risco é conhecido e tem nome. O teto é
**declarado na configuração**, e esgotá-lo encerra o turno com motivo explícito, nunca em
silêncio e nunca fingindo sucesso.

### 6. Hook nunca afrouxa política

Um hook pode **injetar observação** ou **vetar**. Não pode conceder permissão, alterar
`egressClass`, liberar caminho fora do `readAllow` nem desfazer um `deny`. A direção é de mão
única: hook só aperta.

### 7. Opt-in explícito

Bloco ausente ⇒ nenhum hook, nenhuma mudança de comportamento. O `--init` **não** gera exemplo
de hook — regra herdada do MT-151: bloco cujo valor de exemplo é executado não pode ter exemplo
inerte.

## Consequências

- **Impacto positivo:** fecha a lacuna de verificação apontada no mapeamento — o critério de
  conclusão deixa de ser autoavaliação do modelo e passa a ser comando com código de saída. O
  `agentry` vira alvo viável para núcleos de engenharia agentic portáveis, que hoje o ignoram
  por falta exatamente desses pontos. O sensor pós-ação corrige no mesmo turno em vez de deixar
  o defeito virar dívida.
- **Impacto negativo:** uma superfície de execução nova, que não existia. Mesmo com as sete
  restrições, o projeto passa a ter um caminho em que abrir um repositório de terceiro executa
  comando local — mitigado por §2 (só arquivo do projeto, revisável) e §4 (auditado), não
  eliminado. Latência: `PostToolUse` soma a **cada** edição do turno.
- **Trade-off aceito:** paga-se superfície de execução por capacidade de prova. A alternativa —
  manter o critério de aceite como instrução no prompt — já se sabe que não funciona: é a
  diferença entre convencer o modelo e configurar o ambiente, e o projeto inteiro existe do lado
  de configurar.
- **Risco residual declarado:** um gate mal escrito que reprova sem motivo real ensina quem usa
  a reexecutar até passar. Nesse ponto o gate deixou de filtrar e virou ruído tolerado. Não há
  mitigação técnica — é disciplina de quem escreve o gate, e precisa estar na documentação.

## Diretriz de Conformidade de Código

- **Proibido** declarar hook fora do arquivo de configuração **do projeto** — camada global e
  camada de ambiente são erro tratado ao carregar.
- **Proibido** hook sem `egressClass`, ou com classe diferente de `local-only`.
- **Proibido** hook que conceda permissão, altere classe de egresso, amplie `readAllow` ou
  reverta um `deny`. O único efeito admissível de um hook sobre a política é apertá-la.
- **Proibido** ponto de ciclo fora de `PostToolUse` e `Stop` sem nova ADR. Em especial, é
  proibido introduzir `PreToolUse`: o caminho pré-ação pertence às permissões.
- **Obrigatório** registrar cada execução de hook no audit log, com comando, código de saída e
  duração.
- **Obrigatório** teto de tentativas declarado para veto de `Stop`, e encerramento com motivo
  explícito ao esgotá-lo.
- **Obrigatório** que a ausência do bloco de hooks preserve o comportamento atual byte a byte, e
  que o modelo gerado por `--init` não declare hook nenhum.
