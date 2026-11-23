# Markup pela carga tributária real de cada produto — desenho

- **Data:** 2026-11-23
- **Pedido:** Igor Gewehr (sessão de 23/11)
- **Relacionadas:** ADR-0018 (markup divisor), ADR-0021 (contingência), `docs/modulos/estoque.md`
  (estrutura mercadológica e formação de preço), `docs/modulos/compras.md` (entrada por XML),
  `docs/modulos/os.md` (peças)

## 1. Intenção

Dois produtos que custam o mesmo (R$ 15) e devem dar o mesmo lucro (15%) precisam sair com
preços diferentes quando um paga mais imposto na venda (ou recupera menos na compra). O usuário
quer que o Lunno **saiba quanto cada produto paga e vai pagar de imposto** e que o preço
sugerido acerte o lucro de verdade, com os impostos compostos (um na base do outro), o crédito da
compra e a reforma tributária.

Quem usa: MEI, micro e pequena empresa — pequenos mercados, assistências técnicas, comércio que
acabou de abrir — faturando até ~R$ 300 mil/mês. O dono **não sabe o que é CST** e não deve
precisar saber. "Inteligente sem IA": regras determinísticas que deduzem o que dá para deduzir
(da nota de compra, do NCM, do regime), dizem o que falta em português e nunca inventam número.

Caso de uso que o usuário deu, e que serve de teste de aceitação:

> Configuro a categoria **Peças** (peças de celular) com a tributação X e a margem que quero.
> Compro a peça sob demanda para trocar num celular e não cadastro preço de venda no estoque.
> Quando puxo a peça na OS, o **preço de venda sugerido já vem preenchido** pelo markup da
> categoria.

## 2. Estado de hoje (o que já existe e não se refaz)

- Estrutura mercadológica de 4 níveis com margem, comissão e perfil fiscal herdados
  (`mod-estoque::hierarquia`, ADR-0018).
- Markup divisor exato (`mod-estoque::precificacao`): `preço = custo ÷ (1 − Σ percentuais)`,
  com preço de empate e decomposição de cada real. `PrecoSugeridoDoProduto` e
  `AplicarPrecosSugeridos` (leva à tabela de venda).
- Carga: MEI 0; Simples pela alíquota efetiva (RBT12 da lei, `simples::rbt12`), com ST e
  monofásico tirando a parcela no **Anexo I**; Presumido/Real = `ICMS × (1 − redução) + PIS +
  COFINS` do perfil do produto, ou percentual manual.
- Custo de referência: médio, última compra ou o maior dos dois.
- Entrada por XML: custo = bruto − desconto + rateio + ICMS-ST + FCP-ST + IPI.
- Emissão: `mod-fiscal::tributacao::tributar` monta os impostos do item da NFC-e a partir do
  regime e do perfil efetivo (CSOSN 102/103/300/400/500; CST 00/20/40/41/50/60).
- OS: ao escolher a peça, o preço vem da 1ª tabela de preço, senão do último cobrado, senão
  fica vazio (`tela_os/orcamento.rs::sugerir_preco`). **O markup nunca entra.**

### Lacunas que este desenho fecha

1. Presumido/Real somam ICMS + PIS + COFINS: falta o ICMS fora da base do PIS/COFINS (STF, Tema
   69), FCP, IPI da venda (indústria), IRPJ/CSLL.
2. Crédito da compra não existe: a nota de entrada não guarda o ICMS próprio nem o crédito do
   Simples (`pCredSN`/`vCredICMSSN`); para o regime normal o custo fica inflado.
3. Simples: dedução de ST/monofásico só no Anexo I (indústria — Anexo II — e serviço não).
4. Sublimite: acima de R$ 3,6 mi o ICMS sai do DAS — hoje só um aviso; o preço subestima.
5. Um preço só, sem considerar destino (interestadual, consumidor de outra UF).
6. Sem CBS/IBS e sem projeção da reforma.
7. A conta do preço e a da nota são duas: podem divergir sem ninguém ver.
8. A OS não usa o preço sugerido.
9. O perfil fiscal depende de alguém preencher. O leitor da NF-e já extrai CST/CSOSN, base,
   alíquota e valor do ICMS, ST e FCP-ST (`cardeal_fiscal::nfe::interpretar`), `escrituracao.rs`
   já lê `pCredSN`/`vCredICMSSN` e `mod-compras::st` já detecta ST e estima por MVA — mas nada
   disso **vira perfil do produto** nem crédito; e o CST/valores de PIS/COFINS e o CEST da nota
   não são lidos.
10. ST/monofásico moram em dois lugares: `CamposFiscais.cst_icms` (o que a NFC-e usa) e as
    marcas `icms_st`/`monofasico` do grupo (o que o preço usa). Podem discordar.

## 3. Objetivos, não-objetivos, anti-objetivos

**Objetivos**

- Mesmo custo + mesma margem desejada ⇒ mesmo lucro líquido em reais por real vendido, para
  qualquer combinação de regime e tributação suportada.
- "Lucro" = o que sobra de cada real vendido depois de custo, todos os impostos (inclusive
  IRPJ/CSLL quando forem sobre o lucro ou a receita), custo operacional, comissão e taxa do
  meio de pagamento. 15% de lucro = R$ 0,15 de cada R$ 1,00 vendido. (**Escolha minha**,
  apresentada ao usuário; ver §9.)
- Preço sugerido preenchido onde o usuário digita preço: cadastro do produto, peça na OS,
  item do pedido/orçamento, produto sem preço no PDV (só como sugestão — o PDV continua
  respeitando "preço sugerido obrigatório").
- Configuração pela **categoria**, em palavras do dono ("Revenda com imposto já pago pelo
  fornecedor"), não em códigos.
- O sistema aprende a tributação com a nota de compra.
- Uma tela que responde "quanto este produto paga de imposto, por quê, e quanto vai pagar com a
  reforma", e o realizado do período (notas emitidas e de entrada).

**Não-objetivos (agora)**

- Apuração e guia (DAS, EFD, DARF): o Lunno não substitui o contador.
- ST como **substituto** (calcular MVA ajustada e cobrar ST na própria venda): o público é
  substituído. O motor deixa o termo previsto (por fora), sem a tabela de MVA por UF.
- Serviço (ISS, Anexo III/V) na mão de obra da OS: entra depois, no mesmo motor.
- Taxas exatas da CBS/IBS de 2027+ antes de o Senado publicar as alíquotas de referência: vão
  como estimativa marcada.

**Anti-objetivos (falha mesmo que "funcione")**

- Número inventado: sem dado, a sugestão diz o que falta (regra do ADR-0018 mantida).
- Preço que prevê um imposto e uma nota que cobra outro.
- Exigir CST/CSOSN do usuário comum para ter preço.
- Sobrescrever em silêncio um perfil fiscal ou preço que o usuário definiu.
- Tela com mais de uma decisão por vez para quem só quer o preço.

## 4. Abordagens consideradas

| | Abordagem | A favor | Contra |
|---|---|---|---|
| A | Estender a conta atual: mais termos no percentual de imposto | rápido | duas contas (preço × nota) continuam separadas; destino e reforma viram remendos |
| **B** | **Um motor de carga único, usado pelo preço e pela emissão; o preço resolvido em forma fechada** | o que o preço prevê é o que a nota cobra; destino, regime e ano são parâmetros; base para o relatório de quanto se paga | mais trabalho: unificar `tributacao` e `precificacao` |
| C | Carga média por NCM (tipo IBPT) | sem cadastro | aproximado, ignora regime e crédito; não acerta o lucro |

**Escolhida: B** (decisão minha — o usuário entregou o "como": "faça o melhor pensando em
simplicidade e intuitividade"). C fica só como referência de transparência, não para preço.

## 5. Desenho

### 5.1 Motor de carga (`cardeal-tributos`, crate puro, sem I/O)

Entrada — o **contexto da venda**:

- regime da empresa, lido de `nucleo_empresa.regime` (`MEI`/`SimplesNacional`/`LucroPresumido`/
  `LucroReal`): Simples com anexo e RBT12 (acima/abaixo do sublimite); PIS/COFINS cumulativo no
  Presumido e não cumulativo no Real por padrão, com um parâmetro por CNPJ para o caso contrário.
  O `Manual(%)` continua como está para quem o escolheu;
- perfil fiscal efetivo do produto (o `CamposFiscais` herdado, mais as marcas ST/monofásico);
- destino: **interna ao consumidor** (padrão) · interestadual a contribuinte (UF) ·
  interestadual a não contribuinte (UF, DIFAL) — os dois últimos só na simulação (§5.6);
- ano-calendário (reforma).

Saída — a **carga linear sobre o preço**: uma lista de termos `Termo { imposto, natureza,
coeficiente_sobre_preco, valor_fixo, explicacao }`, onde `natureza ∈ {PorDentro, PorFora,
SobreOLucro}`. Todos os tributos deste público são lineares no preço; a composição vira
coeficiente:

- ICMS por dentro: `c_icms = alíquota × (1 − redução da base)` (+ FCP);
- PIS/COFINS sobre a receita sem o ICMS: `c_pc = alíquota_pc × (1 − c_icms)`;
- IRPJ/CSLL no Presumido: `presunção × alíquota` sobre a receita (comércio: 8% × 15% + 12% × 9%
  = 2,28%; o adicional de 10% só acima de R$ 20 mil/mês de lucro presumido, como coeficiente
  marginal quando a empresa passa dele);
- IRPJ/CSLL no Real: sobre o lucro — vira `SobreOLucro` com 34% (15% + adicional 10% + CSLL 9%,
  a alíquota marginal; a margem alvo antes do IR é `m ÷ (1 − 34%)`). O adicional do Presumido
  liga pelo faturamento real do Razão, não por configuração;
- Simples: alíquota efetiva menos as parcelas da repartição que o perfil retira — ICMS só quando
  a empresa é **substituída** (CSOSN 500; LC 123 art. 18 §4º-A), PIS/COFINS se monofásico —
  para os Anexos I e II (repartição do II carregada). A indústria **substituta** (CSOSN 201/202)
  continua pagando o ICMS próprio no DAS e a ST da venda vai por fora. Acima do sublimite, a
  parcela de ICMS sai do DAS e o ICMS do perfil entra pelas regras do normal;
- MEI: nenhum termo variável (o DAS fixo é custo operacional, como hoje);
- FCP: alíquota no perfil (campo novo `aliquota_fcp`, herdado), somada ao ICMS; a emissão passa
  a destacar o FCP — os dois lados leem o mesmo campo;
- IPI da venda (indústria) e ST como substituto: `PorFora` — somam ao que o cliente paga, não
  entram no divisor da margem. A ST de substituto reaproveita o `mva_st` do perfil e o cálculo
  de MVA que `mod-compras::st` já tem (não se cria um segundo);
- CBS/IBS (por fora, não cumulativos): termo por ano conforme §5.7. Na transição (2027–2032) a
  base exclui ICMS, ISS e PIS/COFINS (LC 214), então o coeficiente é `alíquota × (1 − c_icms)`,
  não sobre o preço cheio;
- Imposto Seletivo (2027+, bebidas e tabaco), por NCM: termo próprio.

O preço resolve em forma fechada:

```text
custo_liquido = custo de referência − créditos recuperáveis da compra (§5.3)
m'            = margem alvo, ou m ÷ (1 − alíquota IR) se houver termo SobreOLucro
divisor       = 1 − (Σ coeficientes PorDentro + custo operacional + comissão + taxa + m')
preço         = custo_liquido ÷ divisor                 (centavo, meio-acima)
cobrado       = preço × (1 + Σ coeficientes PorFora)    (o que vai na nota)
```

Função pura, aritmética exata (`Dinheiro`/`Percentual`, i128), cada termo com a frase que
explica de onde veio ("ICMS 17% sobre o preço, sem redução — do perfil da categoria Peças").

**A emissão usa o mesmo motor:** `mod-fiscal::tributacao::tributar` passa a pedir ao motor os
termos do item (situação, base, alíquota) e só os traduz para os campos da NF-e/NFC-e. Os testes
atuais de `tributacao` ficam como trava de comportamento. Um teste de propriedade amarra os dois
lados: para qualquer perfil suportado, o imposto da nota emitida a um preço P é igual ao que o
motor previu para P (centavo).

`precificacao::carga_sobre_venda`/`carga_do_produto` viram adaptadores finos do motor (ou somem),
e `SugestaoDePreco` ganha a lista de termos no lugar do percentual único (mantendo `imposto`
como soma, para não quebrar quem lê).

### 5.2 Tributação pela categoria, em palavras do dono

No detalhe da categoria (aba Estrutura) e do produto, um seletor **"Como este produto é
tributado"** com opções prontas; cada opção preenche os campos fiscais certos **para o regime
da empresa** (CSOSN no Simples, CST no normal):

| Opção (texto da tela) | Simples | Regime normal |
|---|---|---|
| Revenda comum | CSOSN 102 | CST 00, ICMS pela alíquota interna da UF |
| Revenda com ICMS já pago pelo fornecedor (ST) | CSOSN 500, ST | CST 60 |
| PIS/COFINS monofásico (bebidas, higiene, autopeças…) | marca monofásico | CST PIS/COFINS 04 |
| ST e monofásico | as duas | CST 60 + 04 |
| Isento / não tributado | CSOSN 300/400 | CST 40/41 |
| Avançado… | campos fiscais completos (como hoje) | |

Herança como hoje (categoria → produto; `NULL` herda). A margem desejada fica ao lado, na mesma
seção ("Margem de lucro que quero: 15%").

**Uma fonte só para ST e monofásico:** o CST/CSOSN do perfil (e o CST de PIS/COFINS) é a
verdade; as marcas `icms_st`/`monofasico` passam a ser derivadas dele. Migração: onde só a marca
existe, o CSOSN/CST correspondente é gravado; onde os dois discordam, vale o CST (é o que a nota
emitida já usa) e o caso entra em "Tributação a conferir".

**Alíquota de ICMS no regime normal:** "Revenda comum" não grava alíquota sozinha (ela muda por
produto: cesta básica, bebidas, FCP). O campo aparece já sugerido com a alíquota interna da UF
(tabela no código) e o usuário confirma; sem confirmação, o perfil fica incompleto e o preço diz
o que falta. No Simples a alíquota não é pedida.

**Quando o produto deixa o `Manual`:** exatamente o gatilho de hoje (`carga_do_produto`: alguma
alíquota de ICMS, PIS ou COFINS no perfil efetivo). Produto que hoje usa o percentual manual
continua com o mesmo preço.

### 5.3 A nota de compra ensina e dá o crédito

**Guardar o que a nota diz** (migração em `compras_item_nota_entrada`): `situacao_icms`
(CST/CSOSN), `origem`, `valor_icms` próprio, `valor_credito_sn` (`vCredICMSSN`), `cst_pis`,
`cst_cofins`, `valor_pis`, `valor_cofins`, `cest`. O leitor já extrai o ICMS, a ST, o FCP-ST e o
crédito do Simples; passa a extrair também CST/valores de PIS/COFINS e o CEST.

**Crédito na compra** — só regime normal (Simples e MEI não creditam):

- ICMS próprio destacado (ou `vCredICMSSN` quando o fornecedor é do Simples) → crédito, **só se
  a saída do produto é tributada** (CST 00/20). Produto revendido com ST ou isento não credita;
- PIS/COFINS → crédito só no não cumulativo (alíquota × base do item), não no monofásico;
- o custo que vai ao estoque passa a ser **sem o crédito**, e o lançamento da entrada ganha
  `D ICMS a recuperar` / `D PIS/COFINS a recuperar` (contas novas no plano, receituário de
  `compras` atualizado). Vale para as entradas novas; o custo médio do saldo que já existe não é
  recalculado.
- Simples (inclusive acima do sublimite) e MEI não creditam aqui. Acima do sublimite a apuração
  do ICMS fora do DAS tem crédito, mas isso é do contador; o preço avisa a limitação.

**Aprender a tributação** — quando o produto casado **não tem** perfil (próprio nem herdado):

- CSOSN 500 / CST 60 / ST cobrado na nota ⇒ sugere "ICMS já pago (ST)";
- CST PIS/COFINS 04 (ou NCM na tabela de monofásicos) ⇒ sugere "monofásico";
- CEST e origem da mercadoria da nota completam o produto (como o NCM já é completado hoje); a
  origem é o que decide os 4% interestaduais do importado na simulação de destino.

A sugestão é aplicada ao produto e entra numa lista **"Tributação a conferir"** (Estoque), com
"de onde veio" (a nota X do fornecedor Y) e "desfazer". Produto que já tem perfil nunca é
sobrescrito; se a nota contradiz o perfil, vira aviso na mesma lista.

A tabela de NCM monofásicos (Leis 10.147/2000, 10.485/2002, 13.097/2015) e de NCM sujeitos a ST
(CEST, Convênio ICMS 142/2018) fica no código, revisável, com a fonte citada.

### 5.4 Preço sugerido onde se digita preço

Uma consulta única `estoque.preco_sugerido.v2` (produto, custo opcional, destino opcional)
devolve preço, empate, termos e o texto curto. Usada em:

- **OS — peça (caso do usuário):** ordem de sugestão ao escolher a peça: (1) tabela de preço,
  se a empresa mantém uma e o produto tem regra nela; (2) **markup** sobre o custo de
  referência; (3) último cobrado. No diálogo "Valores" da peça, ao digitar o **custo previsto**
  com o preço vazio (ou ainda sugerido), o preço se recalcula na hora pelo markup da
  categoria; depois que o usuário digita o preço, ele manda. Dica de uma linha:
  `Sugerido pela categoria Peças · custo R$ 15,00 · lucro 15%`.
- Cadastro do produto (aba Preços), item do pedido de venda/orçamento, produto sem preço no PDV.

### 5.5 "Para onde vai cada real" e "quanto paga de imposto"

No produto (e no clique da dica), um painel curto:

```text
Preço sugerido  R$ 21,74            Lucro  R$ 3,26 (15%)
Custo           R$ 15,00
Impostos        R$ 1,95   DAS 8,97% (sem ICMS: produto com ST)   ▸ ver conta
Operação        R$ 0,87   Cartão R$ 0,43   Comissão R$ 0,22
```

"Ver conta" abre os termos com a frase de cada um. Em Estoque › Análise, **Impostos por
produto**: por produto/categoria, o imposto por unidade previsto, o crédito na compra, e o
**realizado** do período (somas das notas de venda emitidas e de entrada), com exportação
(PDF/xlsx como os demais relatórios).

### 5.6 Destino

O preço sugerido é sempre o da venda interna ao consumidor. No painel, "Simular para outra
UF / para empresa" mostra o preço que dá o mesmo lucro naquele destino (alíquota
interestadual 4/7/12%, DIFAL ao não contribuinte). Tabela de alíquotas internas por UF e a
regra 4% de importados no código.

### 5.7 Reforma tributária

Tabela de transição por ano no motor (LC 214/2025): 2026 teste (CBS 0,9% + IBS 0,1%,
compensáveis — custo zero para o regime normal; Simples fora); 2027–2028 CBS cheia, IBS 0,1% de
teste, fim do PIS/COFINS, IPI zerado (salvo ZFM), Imposto Seletivo começa; 2029–2032 ICMS caindo
e IBS subindo; 2033 só IBS/CBS.
Alíquotas de referência **estimadas** e marcadas como tal até a publicação; um parâmetro por
CNPJ diz se a empresa do Simples recolhe IBS/CBS por fora (opção da LC 214).
Painel "Daqui a alguns anos": o preço que mantém o mesmo lucro em 2027, 2029 e 2033 — só
informativo, nunca aplicado sozinho.

## 6. Restrições

- Aritmética exata; nenhum `f64` em dinheiro.
- UI pelo ADR-0015 (componentes do design system), `verificar-ui` e `verificar-wasm` limpos;
  cabe em 1280×720 lógicos.
- Offline: tudo calculado localmente; nenhuma consulta externa.
- Tabelas legais (Simples, monofásicos, ST, alíquotas por UF, reforma) no código, com fonte e
  data, e um aviso de "conferir com o contador" — como o ADR-0018.
- Permissões: mexer em tributação da categoria/produto exige `estoque.tributario.editar`
  (risco Alto, como hoje); ver o painel, `estoque.produto.ver`.
- Compatibilidade: empresas que já usam `Manual` com percentual continuam com o mesmo preço
  até definirem perfil (o motor devolve o termo manual).
- ADR novo (0022) registrando a decisão; `estoque.md`, `compras.md`, `os.md`, `fiscal.md` e
  `19-estado-e-processo.md` atualizados.

## 7. Critérios de aceitação

1. Dois produtos de custo R$ 15, margem 15%, um "Revenda comum" e outro "ICMS já pago (ST)",
   numa empresa do Simples Anexo I: preços diferentes, e a decomposição de ambos dá lucro de
   15% (±1 centavo).
2. O mesmo par no Presumido com ICMS 17%: o preço de A paga ICMS e PIS/COFINS sem o ICMS na
   base; o lucro dos dois é 15%.
3. Empresa do regime normal compra com ICMS destacado: o custo do estoque sai sem o crédito, o
   razão tem o imposto a recuperar e o preço sugerido cai na proporção.
4. Nota de compra com CSOSN 500 de produto sem perfil: o produto passa a "ICMS já pago (ST)",
   aparece em "Tributação a conferir" e o desfazer volta ao estado anterior.
5. Categoria Peças com margem e tributação; peça criada às pressas na OS (`CriarPecaRapida`)
   com custo previsto digitado: o preço vem preenchido pelo markup, com a dica.
6. Para todo perfil suportado: o imposto da NFC-e emitida a um preço P = o previsto pelo motor.
7. Acima do sublimite: o preço inclui o ICMS do perfil e o DAS sem a parcela de ICMS.

## 8. O que fica para o construtor

Nome e fronteira exatos do crate (`cardeal-tributos` é a proposta), números das migrações,
divisão em ondas, a forma da tabela de NCM monofásicos/ST e o layout fino das telas dentro do
design system.

## 9. Escolhas minhas para o usuário conferir

(As da revisão do construtor, já incorporadas acima: CST como fonte única de ST/monofásico;
regime lido do cadastro; IR do Real a 34%; crédito só com saída tributada e só nas entradas
novas; alíquota de ICMS sugerida e confirmada, nunca gravada sozinha.)


- "Lucro" = sobra líquida sobre o preço, depois do IR quando ele incide.
- Na OS: tabela de preço vence o markup; o markup vence o último cobrado.
- Aprender com a nota aplica a sugestão no produto sem perfil e lista para conferência (em vez
  de esperar aprovação).
- Preço sugerido sempre para venda interna ao consumidor; outros destinos só simulados.
- Reforma só informativa.
