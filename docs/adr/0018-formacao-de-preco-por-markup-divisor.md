# ADR-0018 — Formação de preço por markup divisor, com carga tributária e custo operacional lidos do sistema

- **Status:** Aceita
- **Data:** 2026-10-09
- **Decisores:** Igor Gewehr
- **Relacionadas:** ADR-0002, ADR-0006, ADR-0017

## Contexto

O comércio pequeno precisa responder "por quanto eu vendo isto?" sem ser contador. O método de
mercado é o **markup divisor** (`preço = custo ÷ (1 − Σ percentuais sobre o preço)`), mas na
prática os percentuais são digitados à mão e envelhecem: o imposto do Simples muda de faixa
conforme o faturamento, produto com substituição tributária ou monofásico paga menos, e o
"custo operacional" vira um número parado que ninguém atualiza. O cadastro de produtos também
não tinha onde guardar o que a empresa planeja lucrar por tipo de mercadoria.

O sistema já tem o que falta para fazer melhor: o Razão sabe o faturamento e as despesas reais,
e a estrutura de grupos já formava uma árvore.

## Decisão

1. **Estrutura mercadológica de até 4 níveis** (Grupo › Subgrupo › Categoria › Subcategoria) sobre
   `estoque_grupo_produto.pai`, validada por um domínio puro (`hierarquia::Arvore`). Uso reduzido
   é não criar os níveis de baixo.
2. **Margem desejada e perfil fiscal herdados** pela árvore (`NULL` = herda; produto sobrescreve
   a margem). A margem padrão da empresa é o último recurso; sem nenhuma, o sistema diz o que falta.
3. **O preço é calculado no servidor, em aritmética exata**, e devolvido decomposto (para onde vai
   cada real) junto com o **preço de empate** (sem margem).
4. **Imposto é determinístico, não um palpite:**
   - MEI: nenhum percentual (o DAS fixo é despesa);
   - Simples: alíquota efetiva pelas tabelas dos Anexos I–V e o faturamento dos últimos 12 meses
     lido do Razão; ICMS-ST e monofásico retiram a parcela da repartição (Anexo I);
   - Presumido/Real: percentual informado.
5. **Custo operacional pode ser derivado do Razão** (despesas operacionais ÷ faturamento dos
   últimos 12 meses), com o valor informado como reserva sem histórico. Custo da mercadoria,
   imposto sobre a venda, taxas de cartão e perdas ficam fora dele: cada um tem o seu termo.
6. **Parâmetros por CNPJ**, alteráveis só com `estoque.tributario.editar` (risco Alto), como o
   spec do Lunno pede para "parâmetros de markup e tributação".

### Imposto por produto no Presumido/Real (2026-10-12)

No regime `Manual` (`precificacao::carga_do_produto`), se o produto (com a herança dos grupos) tem
alíquota de ICMS, PIS ou COFINS, o imposto é `ICMS × (100% − redução de base) + PIS + COFINS`
(campo ausente conta 0); sem nenhum deles vale o percentual do regime. MEI e Simples não mudam: o
DAS é por faixa e os campos fiscais não entram. A sugestão informa `imposto_origem`. **Limitação:**
ST/MVA ainda não entram no custo; o painel de margem do pedido e da OS usam o imposto do produto.

## Consequências

- Quem usa o sistema vê o preço se corrigir quando o financeiro muda — o diferencial sobre digitar
  percentuais.
- As **tabelas do Simples ficam no código** e precisam de revisão a cada mudança legal e com a
  reforma tributária (IBS/CBS). Conferir com o contador antes de uso real.
- A 6ª faixa do Simples (acima de R$ 3,6 mi) tira ICMS/ISS do DAS: a efetiva subestima a carga e a
  recomendação é o modo `Manual`. A dedução de ST/monofásico só existe para o Anexo I.
- Empresa com menos de 12 meses usa o faturamento que existe; a anualização proporcional da LC 123
  não foi implementada.
- Acoplamento: `mod-estoque` lê o Razão por `cardeal_ledger::resultado_por_conta` (o núcleo, não
  as tabelas de outro módulo), mantendo a regra de `docs/contratos-internos.md` §7.

## Quando revisitar

- Se for preciso preço por canal ou por cliente (tabelas de preço múltiplas já existem em
  `mod-vendas`): o preço sugerido vira só a **base** das regras de preço.
- Se a reforma tributária mudar a base de cálculo: o motor de carga é uma função pura, troca-se
  `carga_sobre_venda`.
