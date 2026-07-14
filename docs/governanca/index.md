<!-- Caminho relativo: docs/governanca/index.md -->

# Governança & Compliance — Visão geral

Esta trilha existe para times de segurança, compliance, privacidade ou jurídico avaliando
o `agentry` para uso interno numa empresa. Descreve **o que o software faz de fato** —
controles técnicos reais, verificáveis no código — sem detalhamento de implementação.
Para o código-fonte por trás de cada afirmação aqui, os links apontam para os Registros de
Decisão de Arquitetura (ADRs) correspondentes, na trilha de Desenvolvimento.

## Em uma frase

O `agentry` é desenhado com **privacidade e controle de egresso como requisito de
arquitetura, não como funcionalidade opcional**: nenhum dado sai da máquina sem passar por
um único ponto de transporte auditado, sob uma classe de egresso explícita, com *default*
restritivo quando a configuração é ausente ou ambígua.

## Maturidade e status do projeto — leia antes de avançar

Para uma avaliação de risco honesta, é importante entender o que o `agentry` **é** hoje:

- **Projeto pessoal de código aberto**, mantido por um único desenvolvedor, licença **MIT**,
  código-fonte público e auditável em
  [github.com/psiagoleal/agentry](https://github.com/psiagoleal/agentry).
- **Versão 0.1** — na prática, hoje, a CLI fala **somente** com um servidor
  [Ollama](https://ollama.com/) **local**. Adapters para provedores de nuvem
  (OpenAI-compatible, Anthropic) já existem na biblioteca, mas ainda não há caminho de
  configuração para ativá-los pela CLI — ver [Modelo de privacidade e
  egresso](privacidade-e-egresso.md).
- **Sem certificação formal.** O projeto não é certificado SOC 2, ISO 27001, nem equivalente,
  e não passou por auditoria de segurança externa independente. As afirmações desta trilha
  descrevem controles técnicos existentes no código, verificáveis por leitura direta do
  código-fonte (aberto) — não uma alegação de conformidade com um framework específico.
- **Sem telemetria embutida.** Nenhuma dependência do projeto reporta uso, erros ou
  metadados a um serviço de terceiros — ver [Postura de
  dependências](dependencias.md).
- **Governança arquitetural via ADRs.** Toda decisão estrutural relevante — inclusive as que
  afetam segurança e privacidade — é registrada num Registro de Decisão de Arquitetura
  (ADR), com contexto, decisão e uma "Diretriz de Conformidade" explícita sobre o que é
  proibido/obrigatório no código. Ver o índice completo na trilha de Desenvolvimento.

**Recomendação:** trate este site como o ponto de partida para a avaliação técnica, não
como o resultado dela. Para uma decisão de aceite formal, revise o código-fonte
diretamente (é pequeno e o repositório é público) ou solicite uma revisão independente.

## O que cobrir nesta trilha

- [Modelo de privacidade e egresso](privacidade-e-egresso.md) — o que pode sair da máquina,
  sob que condição, e o que acontece quando a configuração é ambígua.
- [Auditoria e rastreabilidade](auditoria.md) — o que é logado, o que nunca é logado.
- [Controle de permissões de ferramentas](permissoes.md) — o que o agente pode executar
  sobre o sistema de arquivos e o shell, e como isso é restringível.
- [Guardrails de conteúdo](guardrails.md) — filtro determinístico e auditável sobre o texto
  das mensagens, independente do controle de egresso.
- [Postura de dependências e cadeia de suprimentos](dependencias.md) — critério de adoção de
  dependências externas, licenciamento, telemetria.
- [Perguntas frequentes de segurança](faq.md).
