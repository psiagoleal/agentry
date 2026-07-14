<!-- Caminho relativo: docs/governanca/auditoria.md -->

# Auditoria e rastreabilidade

Base técnica: [ADR-0002](../adr/0002-modelo-privacidade-egresso.md) e
[ADR-0007](../adr/0007-guardrails-configuraveis-de-conteudo.md).

O `agentry` tem dois sistemas de auditoria independentes, sobre duas dimensões diferentes:
**egresso de rede** e **conteúdo filtrado por guardrail**. Nenhum dos dois é opcional de
ativar em código — cada tentativa relevante sempre gera uma entrada; o que é configurável é
só para onde essa entrada é entregue.

## Auditoria de egresso

Toda tentativa de rede — permitida **ou bloqueada pela allowlist** — gera uma entrada de
auditoria, sempre, sem caminho silencioso. A entrada identifica o destino e o resultado
(permitido/bloqueado) e passa por uma etapa de redação antes de ser entregue — nunca carrega
segredos capturados incidentalmente no conteúdo da chamada.

## Auditoria de guardrail

Toda regra de conteúdo ([Guardrails](guardrails.md)) que efetivamente age — bloqueia ou
mascara algo — gera uma entrada separada, com:

- direção (entrada ou saída da chamada);
- identificador da regra que casou;
- ação tomada (bloqueio ou mascaramento);
- rótulo da tarefa.

**O texto que casou nunca é gravado** — nem no log de auditoria, nem em nenhum outro lugar.
Uma regra que nunca casa (`Allowed`) não gera entrada nenhuma — só ações reais são
auditadas, não tentativas de checagem.

## Onde essas entradas vão parar

O destino de cada entrada é um componente plugável (uma interface que qualquer integração
pode implementar). A CLI de referência distribuída com o projeto implementa a versão mais
simples possível: imprime uma linha por entrada em stderr. **Isso é uma limitação real a
considerar na avaliação:** não há, hoje, uma implementação embutida que grave em arquivo
estruturado, envie a um SIEM, ou persista de forma durável — quem precisar disso hoje
precisa capturar/redirecionar a saída padrão de erro do processo, ou implementar a
interface de destino de auditoria para o sistema de logging da própria empresa (a interface
é pequena e documentada nos ADRs correspondentes).

## O que isso permite responder, e o que não

Com esses dois sistemas, é possível responder: *"esta sessão tentou se conectar a algum
host fora da allowlist?"*, *"alguma regra de guardrail bloqueou ou mascarou algo nesta
sessão, e qual regra foi?"*. Não é possível responder, só com o log de auditoria, *"o que
exatamente o modelo respondeu"* — o conteúdo em si não é auditado, propositalmente (auditar
conteúdo integral criaria uma cópia paralela de dados potencialmente sensíveis, o oposto do
objetivo). Quem precisar de rastreabilidade de conteúdo completo precisa de uma camada
adicional, fora do escopo atual do projeto.
