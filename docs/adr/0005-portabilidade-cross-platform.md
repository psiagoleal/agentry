<!-- Caminho relativo: docs/adr/0005-portabilidade-cross-platform.md -->

# ADR 0005: Portabilidade cross-platform (Linux, Windows, macOS)

- **Status:** Accepted
- **Data:** 2026-06-19
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** portabilidade, plataforma, ci

## Contexto

O `agentry` tem como alvo a homologação em empresa, onde o parque de máquinas é
heterogêneo: o HPC que roda as LLMs é Linux, mas as estações de desenvolvimento incluem
**Windows** e **macOS**. Para ser adotável, o binário precisa compilar e funcionar nos três
sistemas. Algumas áreas do projeto são sensíveis a plataforma: execução de shell (MT-13),
manipulação de caminhos, *symlinks* do adaptador de skills (frágeis no Windows) e fins de
linha (CRLF vs LF, que quebram `cargo fmt --check`). A decisão precisa ser tomada agora para
guiar a implementação desde a v0.1.

## Decisão

Fica acordado que **Linux, Windows e macOS são plataformas de primeira classe (tier-1)** do
`agentry`. O CI passa a rodar em **matriz nos três sistemas** (`ubuntu-latest`,
`windows-latest`, `macos-latest`), com `cargo test` e `cargo build` obrigatórios em todos;
`fmt`/`clippy` rodam em um único SO (são independentes de plataforma). Adota-se
`.gitattributes` para normalizar fins de linha (LF no repositório).

## Consequências

- **Impacto positivo:** alcance amplo e aderente à realidade multi-SO da empresa; problemas de
  portabilidade aparecem no CI, não no usuário.
- **Impacto negativo:** custo de CI ~3×; exige disciplina com APIs específicas de SO e testes
  de caminho; *symlinks* de skills precisam de modo cópia no Windows.
- **Trade-offs aceitos:** mais tempo de CI e cuidado de implementação em troca de adoção ampla.

## Diretriz de Conformidade de Código

- **Proibido:** APIs/syscalls específicas de Unix sem *fallback* documentado; caminhos com
  separador fixo (`/` ou `\`) em literais; assumir a presença de `sh`/`bash`; assumir
  *symlinks* disponíveis.
- **Obrigatório:** usar `std::path::Path`/`PathBuf` para caminhos; manter o CI verde nos três
  SOs; a tool de shell (MT-13) abstrai o interpretador por plataforma; o adaptador de skills
  usa **cópia** quando *symlink* não é suportado (Windows).

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
