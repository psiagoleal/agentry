<!-- Caminho relativo: CHANGELOG.md -->

# Changelog

Todos os registros notáveis deste projeto são documentados aqui.
O formato segue [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/)
e este projeto adere ao [Versionamento Semântico](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Adicionado
- Planejamento de arquitetura registrado em ADRs: **0001** (fundação da camada LLM por
  abstração própria sobre `reqwest`), **0002** (modelo de privacidade/egresso e taxonomia de
  classes), **0003** (consumo dos artefatos do `ai-coding-agent-profiles`), **0004** (postura
  de sinergia com projetos open-source).
- **Contrato de interoperabilidade v1** com o projeto irmão `ai-coding-agent-profiles`
  (`docs/interop/`): charter de responsabilidades, esquema de artefatos versionado e taxonomia
  de privacidade (perfil → classe de egresso).
- `docs/architecture.md` (módulos e fluxo de egresso) e `docs/roadmap-v0.1.md` (16 micro-tickets).
- Provedores-alvo da v0.1 definidos: Ollama, vLLM e Anthropic.

> Ainda sem implementação (código Rust): esta seção registra apenas o pacote de planejamento.
> A primeira release `[0.1.0]` será datada quando a v0.1 do roadmap for entregue.
