<!-- Caminho relativo: README.md -->

# agentry

> CLI agêntico de codificação em Rust, multi-provedor (modelos locais e de nuvem), com
> roteamento por classe de privacidade e controle auditável de egresso. Projeto irmão do
> `ai-coding-agent-profiles` (camada de política).

![build](https://img.shields.io/badge/build-passing-brightgreen)
![coverage](https://img.shields.io/badge/coverage-0%25-lightgrey)
![version](https://img.shields.io/badge/version-0.1.0-blue)
![license](https://img.shields.io/badge/license-MIT-green)

## Pré-requisitos

- WSL2 / Linux / macOS / Windows
- Python 3.12+ com [`uv`](https://github.com/astral-sh/uv) (ou o runtime da linguagem do projeto)
- Docker (opcional, para serviços e testes)

## Instalação

```bash
git clone https://github.com/usuario/projeto.git
cd projeto
uv sync
```

## Uso

```bash
uv run python -m projeto --help
```

## Estrutura de diretórios

```
src/      # código-fonte
tests/    # testes
docs/     # documentação (architecture.md, api.md, development.md)
```

## Como contribuir

1. Faça um *fork* e crie uma branch (`feature/minha-feature`).
2. Garanta que linter e testes passam.
3. Abra um PR descrevendo a mudança (ver checklist da skill `pr-review-guard`).

## Licença

Distribuído sob a licença MIT. Veja [`LICENSE`](./LICENSE).

---

## Apoie

**Feito com ❤️ por Iago Leal** | [☕ Apoie o criador]

Se este projeto ajudou você, considere apoiar:

- Buy Me a Coffee: https://buymeacoffee.com/psiagoleal

<a href="https://buymeacoffee.com/psiagoleal" target="_blank" rel="noopener">
  <img src="https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png" alt="Buy Me a Coffee" height="41" width="174" />
</a>
