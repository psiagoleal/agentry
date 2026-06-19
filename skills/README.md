<!-- Caminho relativo: skills/README.md -->

# Biblioteca de Skills de Governança para Agentes de IA

Skills (Habilidades de Agente) são pacotes modulares, autocontidos e versionáveis,
descritos por um arquivo `SKILL.md` com *frontmatter* YAML (`name`, `description`). Sua
propriedade central é a **divulgação progressiva** (*progressive disclosure*): o agente
carrega apenas o cabeçalho descritivo na inicialização e só expande o corpo completo
quando a tarefa se enquadra nos gatilhos da `description`. Isso reduz o consumo de tokens
em sessões onde a capacidade não é necessária e permite manter uma biblioteca extensa sem
inflar o contexto de cada interação.

Esta biblioteca contém apenas skills **de governança/fluxo, independentes de setor** —
reutilizáveis nos três perfis (empresa, externo-confidencial, pessoal).

## Catálogo

| Skill | Para quê | Aciona quando |
|-------|----------|---------------|
| [`secrets-guard`](secrets-guard/SKILL.md) | Não-exposição de segredos antes de comandos/saída | Há credenciais, `.env`, cofres, `aws/gcloud/kubectl config` |
| [`adr-writer`](adr-writer/SKILL.md) | Criar/consultar ADRs como restrição cognitiva | Decisão de stack/biblioteca/solver; "registrar decisão" |
| [`micro-ticket-planner`](micro-ticket-planner/SKILL.md) | Quebrar trabalho em tickets de um ciclo de contexto | Planejar sprint; tarefa ampla/ambígua |
| [`handoff-updater`](handoff-updater/SKILL.md) | Manter `docs/CURRENT-STATE.md` | Após commit/ticket; "onde paramos" |
| [`pr-review-guard`](pr-review-guard/SKILL.md) | Checklist do "problema dos 80%" + OWASP | Antes de abrir/aprovar PR ou merge |

## Modelo de instalação (independente de agente)

As skills são descritas pelo padrão `SKILL.md`, **portável entre plataformas**. Para não
amarrar a biblioteca a um único agente, adotamos dois níveis:

1. **Fonte da verdade neutra** — uma pasta `skills/` no repositório alvo, versionável e
   revisável por pares. É **independente de agente** e o único lugar onde o conteúdo vive.
2. **Adaptadores por agente** — como cada ferramenta descobre skills em local próprio
   (o Claude Code, por exemplo, lê de `.claude/skills/`), geramos ali apenas **ponteiros
   (symlinks)** para a pasta neutra. Trocar de agente, ou usar vários, é só gerar outro
   adaptador — sem duplicar conteúdo.

### Forma recomendada: o script

O [`scripts/setup-profile.sh`](../scripts/setup-profile.sh) faz os dois níveis de uma vez:

```bash
# copia o perfil + biblioteca neutra (skills/) + adaptador .claude/skills/ (symlinks)
scripts/setup-profile.sh empresa ~/dev/meu-projeto

# só a pasta neutra, sem adaptar a nenhum agente:
scripts/setup-profile.sh empresa ~/dev/meu-projeto --skills-mode none

# cópias em vez de symlinks (recomendado p/ checkouts no Windows):
scripts/setup-profile.sh empresa ~/dev/meu-projeto --skills-mode copy

# selecionar skills específicas:
scripts/setup-profile.sh pessoal ~/dev/oss --skills secrets-guard,pr-review-guard
```

### Forma manual

```bash
# 1) fonte neutra (independente de agente)
cp -r skills/ /caminho/do/alvo/skills/

# 2) adaptador do agente como ponteiro à fonte neutra (Claude Code)
mkdir -p /caminho/do/alvo/.claude/skills
ln -s ../../skills/secrets-guard /caminho/do/alvo/.claude/skills/secrets-guard
# repita para as demais

# (alternativa: instalação pessoal, todas as sessões) ~/.claude/skills/
```

> Os perfis em `profiles/*/` referenciam estas skills na seção "Skills disponíveis" do seu
> `AGENTS.md` apontando para a pasta neutra; o adaptador `.claude/skills/` é só a ponte de
> descoberta para o Claude Code.

## Governança da biblioteca

Trate este repositório como uma biblioteca interna de software:

- **Revisão por pares** de toda alteração em `SKILL.md` via PR.
- **Fixtures sintéticos** e exemplos de invocação para cada skill.
- **Testes de invocação automatizados** quando a skill tiver *scripts* anexos.
- *Definition of Done* específico por skill (ver seção final de cada `SKILL.md`).

## Skills vs. Servidores MCP

São camadas **complementares e ortogonais**:

- **Skill** = camada *declarativa* — orienta **como** o agente trabalha com uma
  capacidade (convenções, arquivos a consultar, armadilhas a evitar).
- **Servidor MCP** = camada de *execução* — expõe *tools*, *resources* e *prompts* a
  múltiplos agentes via protocolo padronizado, abrindo conexões a sistemas reais (bancos,
  APIs internas, simuladores).

Uma skill pode invocar servidores MCP para executar leituras, disparar simulações ou
consultar APIs, mantendo separados o **conhecimento operacional** (Skill) e a
**infraestrutura de execução** (MCP). Servidores MCP corporativos devem ser tratados como
serviços de produção (gestão de identidade, auditoria, *rate limiting*, isolamento de rede).
