<!-- Caminho relativo: docs/adr/0043-detectores-de-dado-pessoal.md -->

# ADR 0043: Detectores nomeados de dado pessoal, e o que sai registrado na auditoria

- **Status:** Accepted
- **Data:** 2026-07-28
- **Decisores:** Iago Leal (mantenedor)
- **Tags:** confidencialidade, guardrails, auditoria, PII

## Contexto

Três achados da rodada 8 (`docs/handoff-arquivo.md`), todos **verificados em execução** contra
modelos reais, e todos ainda abertos:

1. **Nada impõe a sanitização na fronteira local↔nuvem.** Com instrução explícita o modelo
   local sanitizou; sem ela, o CSV inteiro com nomes, CPFs e e-mails saiu da máquina. A
   proteção dependia inteiramente de um modelo de 7B obedecer.
2. **Guardrails são correspondência literal (ADR-0007, "sem regex") — o modelo contorna.**
   Com `{match: "cpf", action: "block"}`, o payload que continha o literal `cpf` foi
   bloqueado; o modelo **reformatou os dados sem aquele rótulo e mandou os nomes assim mesmo**.
   Bloquear literal dá falsa sensação de proteção: PII é um **formato**, não uma palavra.
3. **O audit log registra *que* houve egresso, não *o que* saiu.** Foi preciso instrumentar o
   binário externo para descobrir o vazamento — depois do fato, o log não permitia detectá-lo.

O ponto comum: o `agentry` sabe *que* dado cruzou a fronteira, e não sabe *o quê*. As três
lacunas se fecham com um mecanismo só — reconhecer **formatos** de dado pessoal.

## Decisão

### 1. Detectores **nomeados**, não expressões regulares do usuário

`GuardrailRule` ganha uma alternativa a `match`: `detect`, com um nome de detector embutido.

```json
{ "id": "nao-vaza-cpf", "detect": "cpf", "action": "redact" }
```

Detectores da primeira versão: `cpf`, `cnpj`, `email`, `telefone-br`, `cartao-credito`.

**Por que não expor regex ao usuário**, que seria a leitura literal do achado 2:

- **PII é difícil de escrever corretamente.** Quase toda regex de CPF publicada aceita
  `000.000.000-00` e recusa a forma sem pontuação. Errar aqui é silencioso e o usuário só
  descobre no vazamento.
- **Regex de terceiro é superfície de ataque.** Padrão vindo de configuração pode ter
  *backtracking* catastrófico; um artefato de perfil malicioso viraria negação de serviço.
  Um detector embutido tem custo previsível por construção.
- **Nome é auditável, padrão não.** `detect: "cpf"` diz o que a regra protege; uma regex de
  40 caracteres num audit log não diz nada a quem revisa.

Consequência aceita: cobertura fixa. Uma necessidade nova exige detector novo no código, não
configuração — troca deliberada de flexibilidade por correção. A ADR-0007 (§"sem regex")
**não** é revogada: continua sem motor de regex e sem dependência nova; o que muda é que
correspondência literal deixa de ser a única forma de casar.

### 2. Validação de dígito verificador, não só formato

`cpf`, `cnpj` e `cartao-credito` só contam como achado se os **dígitos verificadores**
conferirem (módulo 11 para os dois primeiros, Luhn para o terceiro). Sem isso, qualquer
identificador de 11 dígitos — chave estrangeira, número de pedido, *hash* numérico — viraria
"CPF" e o `redact` corromperia dado legítimo em silêncio. Falso positivo em guardrail não é
incômodo: é corrupção silenciosa de saída.

### 3. Detecção alimenta a auditoria (achado 3)

`GuardrailAuditEntry` passa a registrar **quantos** achados de cada detector houve —
`{"cpf": 4, "email": 4}` — e **nunca o valor encontrado**. Isso torna o vazamento detectável
depois do fato sem transformar o audit log num novo repositório de dado sensível, que é
exatamente o risco de "logar o que saiu".

A contagem é registrada **mesmo quando a ação é `redact`**: saber que quatro CPFs foram
mascarados numa resposta é informação de conformidade, não ruído.

## Consequências

- **Impacto positivo:** fecha os três achados com um mecanismo; PII passa a ser reconhecida
  por formato, então reformatar os dados (a evasão observada) deixa de funcionar; o log passa
  a permitir detectar vazamento depois do fato.
- **Impacto negativo:** cobertura fixa e centrada no Brasil (`cpf`/`cnpj`/`telefone-br`) —
  outros países exigem detectores novos. Custo de varredura por regra em cada checagem, agora
  proporcional ao tamanho do texto (antes era só busca de substring).
- **O que continua NÃO garantido:** um detector reconhece o que tem formato reconhecível.
  **Nome de pessoa não tem formato** — "Ana Ribeiro" continua passando. A arquitetura de
  contenção (`readAllow`, ADR-0041) segue sendo a proteção primária; detectores são a segunda
  linha, para o que escapa dela.
- **Trade-off aceito:** cobertura fixa e falso-negativo em dado sem formato, em troca de zero
  falso-positivo por *backtracking* e de um log auditável.

## Diretriz de Conformidade de Código

- **Proibido:** gravar em audit log, aviso ou mensagem de erro o **valor** detectado (só o
  nome do detector e a contagem); contar como achado um `cpf`/`cnpj`/`cartao-credito` cujo
  dígito verificador não confere; adotar motor de regex ou expor padrão livre ao usuário sem
  ADR que reavalie os três motivos acima.
- **Obrigatório:** toda regra declara `match` **ou** `detect`, nunca ambos nem nenhum —
  configuração ambígua é erro tratado, não um dos dois silenciosamente ignorado; a contagem
  por detector é registrada tanto em `block` quanto em `redact`; `redact` substitui o achado
  por marcador de tamanho fixo, sem preservar nenhum dígito do original.

> Qualquer desvio desta regra viola as diretrizes de conformidade arquitetural do projeto
> e deve ser reportado para revisão antes de prosseguir.
