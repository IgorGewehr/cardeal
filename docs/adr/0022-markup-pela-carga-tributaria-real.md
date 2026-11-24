# ADR-0022 — Markup pela carga tributária real de cada produto

- **Status:** Aceita
- **Data:** 2026-11-24
- **Decisores:** Igor Gewehr
- **Relacionadas:** ADR-0018 (markup divisor), ADR-0021; spec
  `docs/superpowers/specs/2026-11-23-markup-por-carga-tributaria-design.md`

## Contexto

Dois produtos de mesmo custo e mesma margem desejada precisam sair com preços diferentes quando
um paga mais imposto (ou recupera menos na compra). O ADR-0018 tirava um percentual único de
imposto por regime, com um ajuste por produto só no Presumido/Real (soma de ICMS + PIS + COFINS),
e a conta do preço era separada da conta da nota emitida. O público (MEI, micro e pequenas,
até ~R$ 300 mil/mês) não sabe CST.

## Decisão

1. **Motor único, `cardeal-tributos`** (puro, sem I/O): a carga de um produto vira termos
   lineares sobre o preço — por dentro, por fora (IPI, ST do substituto) e sobre o lucro (IR do
   Real). Simples (Anexos I e II com a repartição; ICMS só sai do DAS para o **substituído**;
   sublimite), Presumido (ICMS/FCP, PIS/COFINS **sem o ICMS na base**, IRPJ/CSLL com o
   adicional) e Real (34% sobre o lucro). As tabelas do Simples mudaram para cá.
2. **O preço sai em forma fechada** (`resolver_preco`): `custo ÷ (1 − Σ por dentro − despesas −
   margem ÷ (1 − IR))`, decomposto ("para onde vai cada real"), fechando no centavo.
3. **O preço sugerido usa o motor** (`SugestaoDePreco.resolvido`); o regime Presumido/Real vem
   do cadastro da empresa. Produto sem alíquota no perfil continua no percentual manual.
4. **CST/CSOSN é a fonte única de ST e monofásico**; as marcas antigas do grupo só valem sem CST.
5. **A emissão não foi refeita**: um teste de amarração (99 casos) prova que o ICMS destacado é o
   que o motor previu. FCP ainda não é destacado — perfil com FCP é recusado na emissão.
6. **Em palavras do dono**: o seletor "Como este produto é tributado" grava CSOSN/CST pelo regime.
7. **A nota de compra ensina** (ST, monofásico, CEST, origem) o produto sem perfil, e a decisão
   vai para "Tributação a conferir" com desfazer; **o regime normal credita** ICMS (saída
   tributada) e PIS/COFINS (não cumulativo), com `D Impostos a recuperar / C Estoque`.
8. **Simulações no motor**: outro estado (interestadual, DIFAL) e a reforma (LC 214/2025, com
   referências **estimadas**).

## Consequências

- A peça da OS e a aba Preços mostram o preço e a conta; o lucro pedido é o lucro que sobra.
- As tabelas legais (Simples, monofásicos por NCM, alíquotas modais por UF, reforma) estão no
  código e **devem ser conferidas com o contador**.
- Pendências: sugestão pelo markup no pedido/orçamento/PDV; o imposto **realizado** por produto
  (das notas); telas de simulação de destino e da reforma; destaque do FCP na NFC-e; Imposto
  Seletivo; atividade do Presumido diferente de comércio.
