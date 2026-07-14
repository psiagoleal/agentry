<!-- Caminho relativo: docs/governanca/guardrails.md -->

# Guardrails de conteúdo (perspectiva de compliance)

Base técnica: [ADR-0007](../adr/0007-guardrails-configuraveis-de-conteudo.md).

Guardrails são um filtro de **conteúdo**, complementar (não sobreposto) ao [controle de
permissões de ferramentas](permissoes.md) e ao [modelo de privacidade e
egresso](privacidade-e-egresso.md). Enquanto egresso decide **para onde** dados podem
trafegar, guardrails decidem **o que**, dentro do texto de uma mensagem, é permitido
trafegar em primeiro lugar — nos dois sentidos: antes de uma mensagem ir ao modelo, e antes
da resposta do modelo voltar ao operador.

## Propriedade central: determinístico, não um segundo modelo de IA

A checagem é correspondência de texto literal (substring, sem diferenciar
maiúsculas/minúsculas) — **não** é um modelo de linguagem analisando o conteúdo. Isso é uma
escolha deliberada de postura de risco: uma regra de guardrail sempre produz o mesmo
resultado para o mesmo texto, é auditável por inspeção direta da configuração (sem
depender de interpretar o comportamento de um segundo modelo), e não introduz uma segunda
superfície de chamada a um provedor externo. O trade-off é cobertura: um guardrail
determinístico não generaliza para variações de fraseado — é adequado a padrões fixos e
conhecidos (ex.: prefixos de identificador de credencial, domínios internos), não a
detecção semântica de conteúdo sensível em geral.

## Duas ações, dois níveis de severidade

- **Bloquear** — a mensagem inteira (de entrada ou de saída) é substituída por um aviso
  fixo. Numa regra de **entrada**, o efeito é forte: o modelo **nunca chega a ser chamado**
  para aquela mensagem — nenhum dado sai da máquina para aquele turno, mesmo que o provedor
  ativo fosse de nuvem.
- **Mascarar** — só o trecho que casou é substituído por um marcador de redação; a conversa
  segue. Quando várias regras de mascaramento casam no mesmo texto, todas são aplicadas.

Bloqueio sempre vence sobre mascaramento, se as duas casarem no mesmo texto — a ação mais
severa nunca é afrouxada por engano de configuração. Entre camadas de configuração, a mesma
regra (mesmo identificador) definida em duas camadas resolve sempre para a ação mais
severa das duas, nunca a mais permissiva.

## O que fica registrado, o que nunca fica

Toda vez que uma regra efetivamente age (bloqueia ou mascara), isso gera uma entrada de
auditoria com a direção, o identificador da regra e a ação tomada — ver [Auditoria e
rastreabilidade](auditoria.md). **O texto que casou nunca é gravado** em nenhum log —
nem o trecho sensível, nem a mensagem completa. Uma regra que não casa não deixa rastro.

## Configuração é responsabilidade de quem opera

O `agentry` fornece o mecanismo; as regras concretas (quais padrões bloquear/mascarar) são
definidas por quem configura o projeto — normalmente via política central do repositório
irmão de perfis. Não há, hoje, um conjunto de regras pré-definido embutido no software para
categorias comuns de dado sensível (ex.: PII, segredos de nuvem por formato) — cada
organização precisa declarar suas próprias regras.
