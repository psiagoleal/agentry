<!-- Caminho relativo: docs/index.md -->

# agentry

CLI agêntico de codificação em Rust, multi-provedor (modelos locais e de nuvem), com
roteamento por classe de privacidade e controle auditável de egresso.

Na v0.1, o `agentry` fala com um servidor [Ollama](https://ollama.com/) **local** — nenhuma
tarefa sai da máquina por padrão. Adapters de nuvem (OpenAI-compatible, Anthropic) já
existem na biblioteca (`agentry_core`), mas a fiação de configuração para escolhê-los pela
CLI ainda é trabalho futuro — ver a trilha de Governança para o que isso significa na
prática hoje.

Este site tem três trilhas, para três públicos diferentes:

<div class="grid cards" markdown>

-   :material-account:{ .lg .middle } **Guia do Usuário**

    ---

    Instalar, configurar e usar o `agentry` no dia a dia — one-shot, REPL, o arquivo
    `agentry.settings.json`, guardrails de conteúdo.

    [:octicons-arrow-right-24: Começar](usuario/index.md)

-   :material-shield-check:{ .lg .middle } **Governança & Compliance**

    ---

    Para times de segurança/compliance avaliando o `agentry` para uso interno: o que sai da
    máquina, o que é auditado, como dependências e permissões são controladas.

    [:octicons-arrow-right-24: Ver visão geral](governanca/index.md)

-   :material-code-braces:{ .lg .middle } **Desenvolvimento**

    ---

    Arquitetura, decisões registradas (ADRs), roadmap e o guia de testes — para quem
    contribui com o código do projeto.

    [:octicons-arrow-right-24: Ver arquitetura](architecture.md)

</div>

## Projeto irmão

O `agentry` é a camada de **execução** de um ecossistema de dois repositórios; a camada de
**política** vive em [`ai-coding-agent-profiles`](https://github.com/psiagoleal/ai-coding-agent-profiles)
— perfis, permissões e taxonomia de privacidade que o `agentry` consome e impõe (ver
[ADR-0003](adr/0003-consumo-artefatos-profiles.md)).

## Licença

Distribuído sob a licença **MIT**. Código-fonte em
[github.com/psiagoleal/agentry](https://github.com/psiagoleal/agentry).
