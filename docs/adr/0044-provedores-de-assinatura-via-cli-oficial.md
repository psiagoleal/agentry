<!-- Caminho relativo: docs/adr/0044-provedores-de-assinatura-via-cli-oficial.md -->

# ADR 0044: Assinatura de serviços de chat por CLI oficial — navegador embutido rejeitado

- **Status:** Accepted
- **Data:** 2026-09-06
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** providers, egresso, auditoria, conformidade, dependências

## Contexto

O mantenedor pediu acesso às contas de assinatura de serviços de chat — **ChatGPT**,
**Gemini** e outros — sem exigir chave de API, mantendo em paralelo os caminhos por API
(OpenAI, Google) para quem os tiver. O objetivo é legítimo e já foi atendido uma vez: a
ADR-0040 resolveu exatamente esse problema para a Anthropic.

Duas rotas técnicas atendem ao pedido:

1. **Navegador embutido** — empacotar um motor de navegador no binário, abrir a interface web
   do serviço, reaproveitar a sessão autenticada do usuário e dirigir o DOM.
2. **CLI oficial do fornecedor** — invocar o binário que o próprio fornecedor distribui, em
   modo *headless*, deixando que ele resolva a autenticação da assinatura.

A ADR-0040 já rejeitou a família da rota 1 no caso da Anthropic, e o racional transporta
diretamente: a assinatura autentica por credencial **emitida para um cliente específico**, e
reutilizá-la a partir de um programa de terceiros viola os termos de uso; a sanção recai sobre
a **conta do usuário**, não sobre o projeto. Dirigir a interface web de ChatGPT ou Gemini por
automação é a mesma classe de reuso, agravada por os termos desses serviços vedarem
explicitamente acesso automatizado à interface web.

Há um segundo impedimento, estrutural, e ele é decisivo para o objetivo de **homologação
corporativa** que sustenta o projeto. As ADRs 0001 e 0002 fundam a conformidade em uma
**fronteira de rede única e auditável** — `Transport`, `Allowlist` por classe de egresso,
audit log. A ADR-0040 tratou como condição da decisão o fato de um *subprocesso* escapar do
`Transport`, e só o aceitou mediante `AuditEntry` própria por invocação e verificação da
`EgressClass` **antes** do *spawn*. Um motor de navegador embutido não é uma exceção pontual
do mesmo tipo: é uma superfície de egresso irrestrita que busca e executa JavaScript remoto
arbitrário na máquina do desenvolvedor, sem enumeração possível de destinos. Não há
`AuditEntry` que descreva com honestidade o que saiu, porque o conjunto de destinos não é
conhecido nem estável. Isso reprova numa revisão de segurança corporativa, e o comportamento
*fail-closed* da ADR-0002 não teria como ser imposto.

Somam-se custos práticos que, sozinhos, não decidiriam, mas confirmam a direção: um motor
Chromium acrescenta ordem de 100–200 MB a um binário que já tem 281 MB; a automação colide
com proteção anti-bot; e o acoplamento ao DOM de terceiros quebra a cada mudança de layout,
transferindo para o projeto um custo de manutenção permanente e imprevisível.

A decisão precisa ser tomada agora porque toda a matriz de providers da próxima fase — e a
expectativa de usuários que só têm assinatura, sem chave de API — depende de qual das duas
rotas o projeto adota.

## Decisão

Fica **rejeitado o navegador embutido como mecanismo de autenticação ou de transporte de
provider**. Nenhum motor de navegador entra na árvore de dependências do `agentry` com essa
finalidade, e nenhum provider autentica reaproveitando sessão de interface web.

Fica acordado o atendimento ao pedido por **duas famílias de providers**, espelhando a
estrutura já validada na ADR-0040:

### 1. Providers por assinatura, via CLI oficial do fornecedor

Providers que fazem *spawn* do binário oficial em modo *headless* — **Codex CLI** para conta
ChatGPT e **Gemini CLI** para conta Google —, no mesmo molde do `claude-cli`: o provider não
lê, não copia e não persiste token algum; a autenticação é inteiramente do binário do
fornecedor, a partir do login que o usuário já fez.

As condições da ADR-0040 são vinculantes para esses providers, sem exceção:

- **`AuditEntry` própria por invocação**, no mesmo `AuditSink`/`.agentry/audit.log` da
  ADR-0037, declarando o subprocesso e o modelo como `destination` e a decisão de egresso
  resolvida — a trilha de conformidade não pode ter buraco por o egresso não passar pelo
  `Transport`.
- **`EgressClass` verificada antes do *spawn***, recusando executar quando a classe não
  permite nuvem, com `AuditOutcome::Blocked` e `ProviderError`, sem criar o processo.
- **Ferramentas embutidas do CLI desligadas**, pelo achado da ADR-0040: esses binários são
  agentes completos, não *endpoints*. Usá-los como gerador de texto puro mantém a execução de
  tool sob o `PermissionGate`/`CheckpointStore` do `agentry`. Paridade agêntica, quando
  desejada, passa pela ponte de servidor MCP da ADR-0042 — não por delegação cega.

Cada provider desta família exige verificação prévia de que o CLI do fornecedor **autoriza**
consumo programático sob a assinatura, e de qual modo *headless* é o suportado. A ausência
dessa verificação bloqueia a adoção do provider, e o resultado dela se registra no ADR de
implementação correspondente.

### 2. Providers por API

Providers OpenAI e Google por chave, no molde de `anthropic` (ADR-0040): `Transport`
dedicado, `Allowlist` restrita ao host da `baseUrl` sob a `egressClass` declarada, chave
resolvida por `credentials::resolve_api_key` (ADR-0038) — nunca por argumento de linha de
comando. O caminho OpenAI-compatible já coberto pela ADR-0001 permanece válido.

### 3. Navegador como *tool*, não como provider

Permanece **admissível** — e fora do escopo desta decisão — um navegador *headless* usado
como **ferramenta de leitura de página** pelo agente, estendendo as web tools da ADR-0025 para
páginas que exigem execução de JavaScript. Nessa posição ele fica atrás do `Transport`, da
`Allowlist` e da auditoria, que é o lugar arquitetonicamente correto. Sua adoção depende de
ADR próprio, que precisará tratar do egresso indireto originado pela própria página.

## Consequências

- **Impacto positivo:** o objetivo do usuário — usar a assinatura sem chave de API — é
  atendido por caminho compatível com os termos dos fornecedores, sem expor a conta do usuário
  a suspensão. A fronteira de egresso auditável das ADRs 0001/0002 permanece íntegra, e com
  ela a viabilidade de homologação corporativa. O binário não incorpora motor de navegador.
- **Impacto negativo:** os providers por assinatura herdam as limitações da ADR-0040 — sem
  tool-calling nativo enquanto a ponte MCP da ADR-0042 não os cobrir — e exigem que o usuário
  tenha o CLI do fornecedor instalado e autenticado, o que acrescenta um pré-requisito externo
  e uma dependência de estabilidade da interface *headless* de terceiros. Serviços de chat que
  **não** publiquem CLI oficial ficam inalcançáveis por assinatura, e só por API.
- **Trade-offs aceitos:** abre-se mão da cobertura universal de qualquer serviço com interface
  web, em troca de conformidade contratual, auditabilidade e custo de manutenção previsível.
  Aceita-se depender de binários de terceiros, mitigado por a autenticação nunca ser manuseada
  pelo projeto.

## Diretriz de Conformidade de Código

- **Proibido** introduzir motor de navegador (Chromium/CEF/WebView/*webdriver* e equivalentes)
  na árvore de dependências para fins de autenticação ou de transporte de provider.
- **Proibido** ler, copiar, persistir ou reenviar cookie de sessão, token OAuth ou qualquer
  credencial emitida para outro cliente, inclusive a partir do perfil de navegador do usuário.
- **Proibido** provider cujo egresso não seja enumerável: todo destino de rede é conhecido em
  tempo de configuração, seja pela `Allowlist` do `Transport`, seja pelo binário nomeado no
  *spawn*.
- **Obrigatório** que todo provider por subprocesso emita `AuditEntry` própria por invocação e
  verifique a `EgressClass` antes de criar o processo, conforme a ADR-0040.
- **Obrigatório** que todo provider por API passe pelo `Transport` com `Allowlist` restrita ao
  host da `baseUrl`, com a chave resolvida por `credentials::resolve_api_key` (ADR-0038).
- **Obrigatório** verificar, e registrar no ADR de implementação, que o CLI do fornecedor
  autoriza consumo programático sob a assinatura antes de adotar o provider correspondente.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
