<!-- Caminho relativo: docs/adr/0013-tool-de-grounding-via-lsp.md -->

# ADR 0013: Tool de *grounding* via LSP (Language Server Protocol)

- **Status:** Accepted
- **Data:** 2026-07-08
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** contexto, especialização-sem-fine-tuning, dependências, lsp

## Contexto

Modelos open-source pequenos alucinam assinatura de função, nome de campo e tipo com
frequência maior que modelos de fronteira, especialmente em bases de código grandes que
excedem o que cabe no contexto. O repo-map (ADR-0010) e o RAG semântico (ADR-0011) ajudam a
encontrar código relevante, mas não garantem que o modelo "veja" a assinatura exata ou o tipo
resolvido de um símbolo — informação que um *Language Server* já calcula com precisão (via
compilador/analisador real, não aproximação textual). Expor essa informação como tool reduz
alucinação de API diretamente na fonte mais confiável disponível, sem qualquer fine-tuning.

**Maturidade verificada** (`gh repo view` + crates.io, 2026-07-08):

| Crate | Repositório | Estrelas | Downloads | Licença | Último push |
|---|---|---|---|---|---|
| `lsp-types` | `gluon-lang/lsp-types` | 408 | 28.117.262 | MIT | 2024-07-09 |
| `lsp-server` | `rust-lang/rust-analyzer` | 16.646 | 12.543.577 | Apache-2.0 | dia da verificação |

`lsp-types` não recebe *push* há mais de um ano — à primeira vista um sinal de alerta pelo
critério do ADR-0004. Mitigante registrado aqui: é dependência **direta** do `rust-analyzer`
(ativamente mantido, *push* no dia da verificação), o que sugere uma API de tipos de
protocolo já estável, não abandono — mas fica **registrado para reverificação** se o
`rust-analyzer` migrar de dependência ou o projeto ficar sem *release* por período maior.

## Decisão

Fica acordada a construção de um **cliente LSP mínimo** (via `lsp-types` + `lsp-server`) que
fala com o *language server* já instalado no ambiente do usuário para a linguagem do projeto
(`rust-analyzer`, `pyright`, `gopls` etc.) — **o `agentry` não empacota nem instala nenhum
language server**, só fala o protocolo com o que já está disponível.

O cliente expõe operações de **leitura** (hover, *go-to-definition*, referências) como tool
(`lsp_hover`/`lsp_definition`) ao agent loop, sob o gate de permissão do MT-11. **Nenhuma
operação de escrita/refatoração via LSP nesta v0.1** — só *grounding* de leitura.

**Ativada por padrão**, mas **desabilitável pelo usuário** via `settings-schema` (ex.:
`context.lsp_grounding.enabled`, *default* `true`) — necessário porque a ausência do language
server da linguagem no ambiente do usuário deve poder ser contornada desligando a tool, em
vez de falhar.

## Consequências

- **Impacto positivo:** reduz alucinação de assinatura/tipo diretamente na fonte mais
  confiável (compilador/analisador real); não empacota nem gerencia o ciclo de vida de nenhum
  *language server* (menor superfície de manutenção); reaproveita o gate de permissão
  existente (MT-11).
- **Impacto negativo:** depende de o usuário já ter o *language server* da linguagem
  instalado e configurado; adiciona um subprocesso à sessão, com gestão de ciclo de vida
  (start/shutdown) e tratamento de timeout.
- **Trade-offs aceitos:** funcionalidade condicional à presença externa do *language server*
  (ausência é erro tratado, não trava o agent loop) em troca de não reinventar análise
  estática.

## Diretriz de Conformidade de Código

- **Proibido:** o `agentry` empacotar ou instalar automaticamente um *language server*; a
  tool de LSP ignorar o gate de permissão do MT-11; qualquer comunicação com o *language
  server* fora de um cliente LSP dedicado (não abrir subprocessos ad-hoc em outros módulos).
- **Obrigatório:** a tool respeita a flag de configuração (*default*: ativada); ausência do
  *language server* no ambiente é erro tratado (a tool reporta indisponibilidade, não trava o
  agent loop); o processo do *language server* é encerrado ao final da sessão (sem processos
  órfãos).

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
