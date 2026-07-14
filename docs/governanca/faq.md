<!-- Caminho relativo: docs/governanca/faq.md -->

# Perguntas frequentes de segurança

**O `agentry` envia nosso código-fonte para algum serviço de terceiros?**

Hoje (v0.1), não há caminho de configuração para isso: a CLI distribuída só se conecta a um
servidor Ollama local. Se/quando adapters de nuvem forem conectados à CLI, o envio continua
sujeito à classe de egresso ativa (ver [Modelo de privacidade e
egresso](privacidade-e-egresso.md)) — `local-only` proíbe egresso para nuvem por completo, e
esse é o *default* quando a configuração é ausente ou ambígua.

**O software tem telemetria?**

Não. Nenhuma dependência atualmente compilada no binário reporta uso, erros ou metadados a
um serviço de terceiros. Ver [Postura de dependências](dependencias.md).

**O projeto é certificado (SOC 2, ISO 27001, etc.)?**

Não. É um projeto de código aberto mantido por um único desenvolvedor, sem certificação
formal nem auditoria de segurança externa independente até o momento. Ver a seção
"Maturidade e status" na [visão geral desta trilha](index.md) para uma leitura honesta do
que isso implica antes de qualquer decisão de aceite.

**Como sabemos que dados sensíveis não vazam para um log em algum lugar?**

Os dois sistemas de auditoria do projeto (rede e conteúdo) são desenhados para nunca gravar
o conteúdo/segredo capturado — só metadados sobre a tentativa (destino, regra que casou,
ação tomada). Essa é uma propriedade verificável lendo o código-fonte diretamente (é
pequeno e público). Ver [Auditoria e rastreabilidade](auditoria.md).

**Podemos rodar isso totalmente isolado de rede (air-gapped)?**

Sim, na configuração padrão de hoje: o único provedor conectável é um Ollama local, que pode
rodar na mesma rede isolada. `--init --profile <nome>` é a única funcionalidade que
precisa de rede externa (busca de configuração de perfil) — e é opcional.

**Quem responde em caso de vulnerabilidade encontrada?**

Não há um time de segurança dedicado nem SLA formal — é um projeto pessoal de código aberto.
Vulnerabilidades devem ser reportadas via *issue* em
[github.com/psiagoleal/agentry](https://github.com/psiagoleal/agentry). Para uso interno
crítico, recomenda-se que a própria empresa avalie o código (MIT, aberto) e mantenha um
processo interno de resposta a incidentes independente do mantenedor do projeto.

**O que impede o agente de executar comandos destrutivos no sistema?**

O controle de permissões de ferramentas (`permissions.deny`/`ask`) e, por padrão, a
ferramenta de shell já vem sem nenhum comando pré-liberado na CLI de referência. Isso é uma
barreira de configuração, não uma sandbox de sistema operacional — para isolamento mais
forte (contêiner, VM, usuário com privilégios restritos), a responsabilidade é de quem
opera o processo, não do software em si. Ver [Controle de permissões de
ferramentas](permissoes.md).

**Como avaliamos isso com mais profundidade do que este site permite?**

Leia o código-fonte diretamente — é um projeto pequeno, os módulos citados nesta trilha
(transporte, allowlist, auditoria, guardrails, permissões) são objetivamente localizáveis, e
cada decisão estrutural relevante tem um ADR associado com o racional completo. Comece pela
trilha de [Desenvolvimento](../architecture.md).
