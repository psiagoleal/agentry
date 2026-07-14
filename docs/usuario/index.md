<!-- Caminho relativo: docs/usuario/index.md -->

# Guia do Usuário — Visão geral

O `agentry` é uma CLI agêntica: você dá uma tarefa em linguagem natural, ela conversa com um
modelo de linguagem, e o modelo pode pedir para executar ferramentas (ler/escrever
arquivos, rodar comandos de shell, buscar no código) até chegar a uma resposta final.

Dois modos de uso:

- **One-shot** — `agentry "<tarefa>"` roda uma tarefa e sai.
- **REPL** — `agentry` sem argumento entra em modo interativo, com histórico de
  conversa persistente entre mensagens e comandos de barra (`/model`, `/compact` etc.).

## Por onde começar

1. [Instalação](instalacao.md) — pré-requisitos e como compilar o binário.
2. [Configuração](configuracao.md) — o arquivo `agentry.settings.json`: modelo padrão,
   permissões, guardrails, flags de contexto.
3. [Uso da CLI e do REPL](uso.md) — flags, comandos de barra, exemplos.
4. [Guardrails de conteúdo](guardrails.md) — como bloquear ou mascarar padrões de texto
   antes que cheguem ao modelo ou antes que a resposta volte para você.
5. [Perguntas frequentes](faq.md).

Se você está avaliando o `agentry` para uso dentro de uma empresa (segurança, compliance,
privacidade de dados), a trilha certa é [Governança & Compliance](../governanca/index.md),
não esta.
