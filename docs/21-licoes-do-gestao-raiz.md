# 21 — Lições do `gestao-raiz`

> Comparação feita em 2026-10-15 contra o `gestao-raiz` atualizado (`main` em `938b532`, em
> `Documents/Automa/gestao-raiz`). Ele é um ERP **industrial** web (Next.js + Firestore, ~185 mil
> linhas, 33 arquivos de teste) **em produção, com usuários reais**. Ele não é modelo de arquitetura,
> porque o Cardeal tem Razão de partidas dobradas, é offline-first e roda no desktop. O que vale ouro
> são as **regras de negócio que só aparecem com uso real**: os comentários `BUG FIX`, `Fix N` e
> `NUNCA` do código e os testes deles são a lista de armadilhas que já custaram caro a alguém.
>
> Feita em duas passadas. A primeira cobriu todas as áreas, a fundo só em fiscal e compras. A segunda
> leu a fundo **PDV e Financeiro**, o centro do MVP (§3 e §4). Os outros módulos são comparados a
> fundo logo antes de cada fatia que mexer neles.

## 1. O que cada um cobre

| Área | gestao-raiz | Cardeal |
|---|---|---|
| Fiscal | NF-e, NFC-e (síncrona, sem contingência), MDF-e, CC-e, cancelamento, DANFE, notas recebidas (DFe + manifestação) | NFC-e assíncrona com fila (offline-first), DANFCE, cancelamento, inutilização. Sem NF-e, CC-e e DFe real |
| Compras | XML/SEFAZ, classificação da nota, custo com rateio, parcelas, estornar importação | XML de arquivo, casamento em cascata, rateio, ST/IPI no custo, parcelas da nota (duplicatas), estorno da entrada. Sem DFe real |
| Estoque | Matéria-prima com FEFO, embalagens, produto acabado, rastreabilidade e recall | Árvore mercadológica, markup com imposto, inventário cego, transferência, perdas e validade |
| PDV | Grade de produtos, desconto no item e no total, pagamento misto, boleto/cheque a receber, vendas do dia, **cancelar venda**, comprovante em 80/58 mm/A4 | Teclado, diário do carrinho, sessão de caixa, consulta de preço, NFC-e em fila. Ver §3 |
| Vendas B2B | Pedido em etapas (aprovar, pagar, preparar, faturar, enviar, entregar), app do revendedor, comissão | Pedido, tabela e regra de preço, promoção, comissão apurada e paga |
| Clientes / CRM | Cadastro, CEP → IBGE, importação, score/tier automático, interações | Pessoa única com papéis, limite de crédito, contatos e endereços. Sem score e sem importação |
| Financeiro | Portadores com saldo inicial, **transferências**, cancelar/editar/excluir título, parcelar, importar títulos, DRE mensal e anual, orçamento anual, resumo do Simples | Razão de partidas dobradas, caixa, OFX, conciliação, DFC, balancete, aging, cheques, régua, recorrências, renegociação. Ver §4 |
| Contábil | **Fechamento do mês**: ZIP com XMLs emitidos e recebidos, DANFEs, SPED EFD e relatórios, enviado por e-mail/WhatsApp | Pacote do contador: PDF, `.xlsx` e o ZIP dos XMLs do mês (emitidas, canceladas com o evento, inutilizadas, de entrada), com as pendências. Sem SPED |
| Indústria | Fórmulas, OP, MRP, PCP, rendimento, qualidade/SGQ, ANVISA, rotulagem, estabilidade, pragas | `mod-industria` ainda é spec. Fora do foco atual |
| IA | Assistente, onboarding por chat, agente | Fora (Fase 7) |

## 2. Onde o Cardeal já está à frente (não copiar)

- **Saldo derivado por construção.** O gestao-raiz precisou de um cron noturno que recalcula o saldo
  bancário a partir dos títulos. Dois bugs vieram daí: o lançamento do PDV era apagado pelo cron e
  duplicado na conciliação do extrato. No Cardeal o saldo sai do Razão, então esse problema não existe.
- **Pix e cartão em trânsito.** Lá a venda no Pix ou no cartão credita o banco na hora. Aqui vai para
  *Valores em trânsito* e *Cartões a receber* até a conciliação confirmar, que é o que de fato acontece.
- **NFC-e offline-first.** Lá a emissão é síncrona e, se falhar, o operador precisa tentar de novo.
  Aqui o documento nasce com a venda e a fila o transmite. O "pre-flight" que eles criaram (gravar a
  nota antes de chamar o gateway) é exatamente o nosso desenho.
- **Caixa do PDV.** Lá não há sessão de caixa, sangria, suprimento nem fechamento cego.
- **Teclado no PDV.** Lá há só `/`, `F12` e `Esc`; aqui o fluxo inteiro é de teclado, com supervisor no
  cancelamento do cupom.
- **Análise.** DFC, balancete, aging, saúde financeira, rentabilidade por nível da árvore e ABC; os
  relatórios deles são básicos.
- **Não copiar:** os gateways Asaas e Pagar.me (`lib/gateway`) **não são importados por nenhum
  arquivo**: são código morto, e a "conciliação de gateways" do manual não existe na prática.

## 3. PDV a fundo

O que eles têm e o Cardeal não, com o que aprenderam:

| O que | Como fazem lá | No Cardeal |
|---|---|---|
| **Cancelar venda finalizada** | Pelo histórico do dia. Devolve o estoque ao lote (se o lote está bloqueado, em quarentena ou vencido, só devolve a quantidade, sem liberar o lote), cancela as contas a receber da venda, estorna o saldo e pede o cancelamento da NFC-e com uma justificativa automática ("Cancelamento de venda PDV #XXXX a pedido do operador"). Se a SEFAZ recusar, a venda fica com o selo "NFC-e a cancelar" e o aviso indica uma NFC-e de devolução | Falta. O `CancelarNfce` já existe; falta o estorno da venda que o chama. Aqui com permissão de supervisor, como o `F8` |
| **Vendas do dia** | Lista com os totais do dia, reimpressão, cancelamento e o selo de NFC-e pendente | Falta. Só a última venda aparece |
| **Comprovante não fiscal** | Recibo após a venda, em 80 mm, 58 mm ou A4, com impressão automática opcional | Falta. Só há o DANFCE, e só com NFC-e autorizada. Quem não emite NFC-e (MEI vendendo a pessoa física) fica sem recibo |
| **Venda a receber** | Boleto ou cheque na finalização vira conta a receber com vencimento em 30 dias, sem tocar no saldo. Pagamento misto pago + a receber deixa a venda "parcial" | Falta. O `FormaPagamentoPdv` só tem dinheiro, Pix, débito e crédito; a venda a prazo (fiado) estava adiada no código |
| **Desconto no total** | Além do desconto por item | Falta: só por item (`F5`) — **feito (item 8)** |
| **Busca de cliente** | Busca por nome/documento, ou nome livre | O `F6` busca por nome ou documento na carteira inteira (a nota "200 primeiros" estava errada: sem busca vêm até 50 mil). Faltava achar pelo documento com máscara e pelo telefone, e cadastrar ali — **feito (item 8)** |
| **Venda em aberto** | Salva sem cobrar (não baixa estoque nem lança financeiro) e finaliza depois na tela de vendas | Falta. Baixa prioridade |
| Idempotência da finalização | Uma chave por venda: repetir a finalização (duplo clique, nova tentativa) não duplica o financeiro | O cupom só finaliza a partir de "em andamento"; nada a fazer |
| CPF incompleto | Recusa CPF parcial antes de finalizar | Conferir no `IdentificarCliente` |

## 4. Financeiro a fundo

| O que | Como fazem lá | No Cardeal |
|---|---|---|
| **Cancelar título lançado errado** | Conta a pagar ou a receber cancelada ou excluída; **título cancelado não pode ser baixado**, porque "ressuscitaria a venda cancelada e creditaria o saldo" (tem teste) | **Falta.** Só há `EstornarBaixa` e `RenegociarTitulo`. O spec prevê o estado `Cancelada`, mas não há comando. Lançou errado, não tem como desfazer |
| **Corrigir título** | Editar valor e vencimento da conta em aberto | Falta. Alterar o vencimento é o caso mais comum |
| **Saldo inicial da conta** | Por conta; alterar o saldo inicial ajusta o saldo atual pela diferença. Eles tinham duas fontes de saldo inicial, com telas divergentes, e unificaram | **Falta.** A conferência banco × sistema lista "saldo inicial não lançado" como diferença sem explicação. É o primeiro dia de qualquer cliente |
| **Transferência entre contas** | Caixa para banco e banco para banco, os dois lados de uma vez; mesma conta e saldo insuficiente são recusados; cancelar estorna os dois lados | **Feito (2026-10-15).** A sangria cai no `1.1.02 Bancos` genérico; com várias contas bancárias, a transferência leva o dinheiro à conta certa |
| Desativar conta | Não deixa desativar o único Caixa nem uma conta com saldo ("transfira antes") | Não há desativação de conta. Trazer junto com a transferência |
| **Importar títulos** | Contas a pagar e a receber em lote, de planilha | Falta. É a implantação: trazer o que está em aberto no sistema antigo |
| Parcelar título | Dividir uma conta em N parcelas mensais | O `RenegociarTitulo` já cobre, com juros e multa |
| Conta padrão por forma de pagamento | Com troca na hora | Já existe (`conta_destino` na baixa; Pix e cartão em trânsito no PDV) |
| **Orçamento** | Por conta do plano e por ano, com valor mensal; realizado × orçado | Falta. Média prioridade |
| **Resumo do Simples** | Receita acumulada no ano e imposto estimado pela faixa do anexo | Falta. As tabelas já existem na precificação; precisa conferir com o contador |
| DRE anual | 12 meses acumulados | A `DreComparativa` cobre |
| Extrato em PDF | Leitor do extrato do Cresol (banco de um cliente real) | OFX e CSV cobrem; PDF só se um cliente pedir |

## 5. O que trazer, em ordem de prioridade para o MVP

1. ~~**Estornar a venda finalizada no PDV**, com a lista das **vendas do dia** de onde ela é feita
   (§3).~~ **Feito (2026-10-15).** `EstornarVenda` estorna o lançamento e devolve a mercadoria ao custo
   em que saiu; a NFC-e é anulada pela fila conforme o estado (cancela, inutiliza, ou emite e cancela
   quando a tentativa pode ter chegado à SEFAZ). Melhor que lá: com supervisor, em uma transação, e
   sem marcar a venda como "NFC-e a cancelar" à mão — a nota tem a marca permanente e a fila a leva
   até o fim.
2. ~~**Nunca transformar em compra uma nota que não é compra.**~~ **Feito (2026-10-15).** Mais rígido que
   lá em dois pontos:
   - a nota de entrada de terceiro (`tpNF = 0`) não é compra (eles aceitam);
   - o CFOP de cada item é lido: remessa para conserto, bonificação, consignação e entrega futura deixam
     um aviso e nunca entram sozinhas. Um ponto que não tratam e importa para a assistência técnica.

   A função `classifyNote` deles:
   - se o emitente é a própria empresa, é saída própria;
   - se o destinatário é outro CNPJ, a nota não é nossa;
   - se `finNFe=4`, é devolução de cliente (entra no estoque, mas não gera conta a pagar);
   - só se o destinatário é a empresa a nota é compra.

   O `cardeal_fiscal::interpretar` não lê o destinatário nem o `finNFe`, então hoje o XML de uma
   venda nossa entraria como compra, com estoque e título a pagar.
3. ~~**Cancelar título** lançado errado e **alterar o vencimento** de uma parcela em aberto (§4).~~
   **Feito (2026-10-15).** Diferente de lá:
   - lá a conta é excluída de vez (até a já paga, estornando o saldo) e o valor é editável mesmo com
     recebimento parcial;
   - aqui cancelar é estornar (nada some), só vale para a conta lançada no financeiro e exige estornar
     as baixas antes;
   - o valor não se edita: corrigir valor é cancelar e lançar de novo.

   Mantida a lição deles: conta cancelada não se baixa.
4. ~~**Saldo inicial** de cada conta e **transferência entre contas**, com a desativação de conta (§4).~~
   **Feito (2026-10-15).** As regras de lá foram mantidas:
   - o saldo inicial é um só por conta (redefinir substitui);
   - a transferência é atômica e cancelar estorna os dois lados;
   - não se desativa conta com saldo nem a do caixa em uso.

   Diferente de lá: só o **caixa** exige saldo para transferir. Banco pode ficar negativo (cheque
   especial), e o saldo inicial também aceita negativo.
5. ~~**Comprovante não fiscal** em 80 e 58 mm, com reimpressão; o DANFCE também em 58 mm (§3).~~
   **Feito (2026-10-15).** Lá o recibo da venda é só na tela; o que imprime é o DANFE, com as
   escolhas de papel (80, 58 mm, A4) e de impressão automática, que trouxemos para cá. Aqui há também
   o comprovante não fiscal impresso, com rodapé configurável, 2ª via e a marca de venda estornada.
6. ~~**Venda a receber no PDV**: crediário, boleto e cheque, gerando o título com vencimento e
   respeitando o limite de crédito que o cadastro já tem (§3).~~ **Feito (2026-10-16).** Lá:
   - boleto e cheque viram conta a receber com vencimento fixo em 30 dias;
   - o cliente é opcional ("Consumidor Final");
   - não há limite.

   Aqui:
   - uma forma "A prazo" com 1º vencimento e parcelas;
   - cliente obrigatório; respeita bloqueio e limite;
   - o título aponta para o lançamento da venda (sem receita em dobro);
   - o estorno da venda cancela o título se nada foi recebido;
   - a NFC-e leva `tPag 05`;
   - o comprovante ganha a linha de assinatura do cliente.

   O cheque à vista continua pelo registro de cheque do financeiro, sobre a parcela.
7. ~~**Implantação por planilha**: clientes e títulos em aberto.~~ **Feito (2026-10-16).**

   Lá:
   - os clientes "importados" vêm dos pedidos do marketplace, não de planilha;
   - as contas vêm de uma planilha lida no navegador e gravadas sem conferência de pessoa,
     sem contabilidade e sem proteção contra importar duas vezes.

   Aqui:
   - **Clientes:** `Importar clientes…` com prévia (novos, já existiam, erros por linha) e o
     modelo para baixar. CPF/CNPJ e contatos são conferidos; quem já existe não duplica.
   - **Contas em aberto:** `Importar contas em aberto…` acha ou cadastra a pessoa de cada
     conta. Entra como saldo de abertura (Lucros acumulados), sem inflar a receita do mês.
     Importar de novo não duplica nem ressuscita o que foi cancelado.
   - **Nas duas:** tudo ou nada, e sem atalho de permissão. O leitor de CSV agora é um só
     (`cardeal_kernel::planilha`), também usado pelo estoque.
8. ~~**Desconto no total** e **busca de cliente** no PDV.~~ **Feito (2026-10-16).**

   Lá:
   - desconto em % ou valor, sem teto;
   - o rateio por item só na hora da nota;
   - busca de cliente com 5 resultados, ou nome livre.

   Aqui:
   - `Shift+F5` com `10%` ou valor, acompanhando os itens;
   - o teto do papel vale para a venda inteira;
   - rateado entre os itens ao finalizar (a NFC-e, o comprovante e o estorno veem o desconto
     por item);
   - o `F6` acha pelo documento com máscara e pelo telefone, e cadastra o cliente ali mesmo
     (com aviso de telefone repetido).

   Achados no caminho, corrigidos:
   - **nenhum papel tinha teto de desconto**, então nem o `F5` funcionava fora dos testes. Os
     papéis de fábrica agora nascem com ele: Admin e Gerente sem teto, Caixa e Vendedor 10%;
   - **o Operador de Caixa não via clientes** (o `F6` vinha vazio, e sem cliente não há venda a
     prazo). Agora vê e cadastra;
   - **papéis de fábrica gravados ficavam congelados**: a correção do caixa no item 1 (fiscal) não
     chegava a bases existentes. Agora o motor os completa ao abrir.
9. ~~**Notas recebidas pela SEFAZ (DFe) e manifestação.**~~ **Feito (2026-10-26)**: rotas no
   `sefaz-api` (842576f) e a tarefa `compras.buscar_notas_na_sefaz` (ver
   [compras.md](modulos/compras.md), nota de 2026-10-26). Fluxo deles (`lib/fiscal/sefaz-distribuicao.ts`):
   - a distribuição por NSU;
   - se só vier o resumo (`resNFe`), registram a **Ciência da Operação** sozinhos e baixam o XML completo;
   - se a manifestação vier com 596 (passaram os 10 dias), salvam o resumo e pedem o XML ao fornecedor
     (upload manual);
   - na conexão mTLS usam o **trust store ICP-Brasil**, nunca `rejectUnauthorized:false`.

   Aqui (com o OK do usuário para mexer no `sefaz-api`):
   - `/nfe/dfe/distribuicao`, `/nfe/dfe/consulta-chave` e `/nfe/manifestacao` no `sefaz-api`, sem
     estado; resposta sem `cStat` ou sem `ultNSU` volta como erro e **sem NSU**, para o cliente nunca
     gravar um NSU ruim;
   - a busca virou tarefa (fora de transação), com a vez tomada na base, o NSU **por ambiente** e
     nunca para trás, a hora da próxima consulta respeitada (1 hora em dia ou com 656), a ciência
     automática, o 596 pedindo o XML ao fornecedor, a nota que não importa guardada sem travar a
     fila, e o cancelamento pelo fornecedor tirando a nota da espera (ou avisando, se já entrou);
   - nota de homologação não entra no estoque.

   Falta: conferir contra o Ambiente Nacional de homologação (pede o certificado com a senha) e as
   manifestações conclusivas.
10. ~~**Compras:** parcelas da nota (`cobr/dup`; lá cada parcela tem um id determinístico, então refazer
    a importação cria só o que falta) e **estornar a entrada** de uma nota (`revertNoteImport`).~~
    **Feito (2026-10-20).**

    Lá:
    - as parcelas vêm de quem importa (quantas, intervalo, 1º vencimento), divididas em partes
      iguais; o XML de compra não é lido para isso;
    - reverter apaga lotes e contas a pagar (devolvendo o saldo do banco) e recusa se algum lote
      já foi liberado ou consumido.

    Aqui:
    - as **duplicatas do XML** viram as parcelas, com vencimento e valor de cada; a tela as
      mostra para ajustar, dividir em N ou acrescentar, e confere a soma; duplicatas que não
      fecham com o total viram aviso (a nota não entra sozinha);
    - o estorno tira a mercadoria **ao custo da nota** (o custo médio volta ao de antes), estorna
      o lançamento e cancela o título — nada se apaga; pagamento só se estorna marcando; e a
      mercadoria já vendida ou movida bloqueia.

    Achado no caminho, corrigido: a tela de Compras só mostrava "Confirmar entrada" para nota
    "Conferida", estado que nada alcança — a nota que não se confirmava sozinha não tinha como
    entrar pela tela. E o `titulo_da_origem` podia devolver o título cancelado quando a origem
    era desfeita e refeita no mesmo dia (afetaria o desfaturamento de OS).
11. ~~**Fechamento do mês para o contador com os XMLs**: emitidas, canceladas, inutilizadas e recebidas,
    mais os DANFEs, o SPED EFD e os relatórios, num ZIP. Se faltar algum XML na base, ele é baixado da
    SEFAZ antes. Isso pede guardar o **XML do evento de cancelamento**, que hoje só fica com o
    protocolo; a guarda é obrigatória por 5 anos.~~ **Feito (2026-10-20); o XML do evento, em
    2026-10-26, com o `sefaz-api` (bc58e77).**

    Lá (`accounting-export.service.ts`), na prática:
    - o mês da nota é o da criação, não o da emissão;
    - só autorizadas e canceladas entram; a inutilizada fica de fora;
    - o XML do cancelamento é guardado se vier, mas não vai no pacote. E não vem: o `sefaz-api`
      responde ao cancelamento só com o protocolo;
    - "baixar da SEFAZ se faltar" lê o `xml` da consulta, que o `sefaz-api` nunca preenche (a consulta
      devolve o protocolo). O XML que falta continua faltando, em silêncio;
    - gera DANFE para cada nota e o SPED EFD.

    Aqui:
    - o pacote do contador ganha `XMLs AAAA-MM.zip` (comprimido): emitidas em
      `Saidas/<modelo>/<situação>/` (as canceladas com a nota e o evento, as inutilizadas com o evento)
      e as de entrada em `Entradas/`;
    - o mês é o da **emissão** (`dhEmi` do XML), e o da inutilizada é o da inutilização;
    - só **produção** vai ao contador; as notas de homologação são contadas e ficam de fora;
    - o PDF resume as notas e lista as **pendências**, cada uma com o que fazer: a nota sem desfecho (um
      buraco na série), o XML que falta e por quê, a nota de entrada sem XML;
    - a planilha lista nota a nota, com chave e protocolos;
    - a nota de compra passa a **guardar o XML original**, e a importada antes ganha o XML ao importar o
      mesmo arquivo de novo;
    - a base guarda o XML do evento (`xml_evento`) quando o provedor o devolve.

    De fora, por decisão:
    - o **DANFE**, porque o documento é o XML e o contador importa XML;
    - o **SPED**, porque a empresa do Simples em geral é dispensada da EFD ICMS/IPI e o contador a gera
      dos XMLs;
    - **baixar a nota sem XML da SEFAZ**: a consulta não devolve a nota que não se guardou assinada, e o
      emitente não baixa a própria nota pela distribuição — o pacote diz para baixar no portal.

    **O `sefaz-api` (2026-10-26, bc58e77, com o OK do usuário)** passou a devolver o `procEventoNFe` no
    cancelamento e na CC-e, o `procInutNFe` na inutilização, a nota assinada no desfecho incerto e, na
    consulta, o `nfeProc` (com a nota assinada) e o XML do cancelamento. E corrigiu a consulta da nota
    cancelada, que voltava "autorizado" com o cStat 100 do protocolo da autorização — o `saas-erp` e o
    `gestao-raiz` já esperavam o 101. Aqui, o que não veio vai para a **busca**: a fila consulta a nota e
    guarda o que a SEFAZ devolver (o cancelamento já registrado — 573 —, o aceito sem o XML, e os
    antigos, que a migração pôs na busca); o pacote do contador pede a busca antes de montar o mês. O
    `gestao-raiz` passa a receber os mesmos XMLs sem mudar nada lá — o "baixar da SEFAZ se faltar" dele
    só funciona se mandar o XML assinado na consulta.

    Achado no caminho, corrigido: o PDF trocava o sinal de menos da DRE ("(−) Deduções") por "?", porque
    o caractere não existe na codificação da fonte padrão.
12. ~~**Prazo de cancelamento visível**: 30 minutos para NFC-e e 24 horas para NF-e. Lá o prazo bloqueia o
    pedido; aqui a SEFAZ responde 501. Proposta: mostrar o tempo decorrido e avisar, sem bloquear,
    porque algumas UFs aceitam fora do prazo (155).~~ **Feito (2026-10-20).**

    Lá: o pedido fora do prazo é recusado antes de ir à SEFAZ; o diálogo de cancelar venda avisa
    "prazo: 30 min".

    Aqui: o detalhe da nota e o `F11` mostram quanto passou e quanto falta, o pedido de
    cancelamento traz o aviso (vermelho fora do prazo), e faltando 5 minutos o aviso lembra que, sem
    internet, o pedido espera na fila enquanto o prazo corre. Não bloqueia: recusado (501), a nota
    fica como "cancelamento recusado", com a orientação de emitir uma devolução.
13. ~~**Orçamento** por categoria, **resumo do Simples** e **score/tier de cliente**: úteis, mas depois do
    básico.~~ **Feito (2026-10-26).**

    Score/tier — lá (`client-crm.service.ts`): notas de lealdade, valor e risco de churn por faixas
    **fixas** (R$ 100 a R$ 10 mil gastos; 30, 60, 90, 180 dias sem comprar), as mesmas para
    qualquer empresa, gravadas no cadastro e recalculadas em lote, sem olhar a pontualidade. Aqui:
    a pontualidade sai das parcelas a prazo (`mod_financeiro::pagamento`: nota 0–1000 e selo, na
    ficha do cliente); a curva de clientes do PDV (`CurvaDeClientes`) junta balcão e pedidos, dá a
    classe ABC **relativa à carteira**, mede o sumiço contra o **ritmo do próprio cliente** e diz o
    que fazer (cobrar, procurar). Tudo calculado na hora.

    Orçamento — lá (`financial_budgets`): anual por conta do plano, 12 meses, DRE com orçado e
    variação, "copiar do ano anterior". Mas a variação é `realizado ÷ orçado − 1` em toda linha: a
    despesa que estoura o orçado aparece positiva. Aqui: por conta do resultado (a "categoria" do
    Cardeal vira conta), o desvio no sentido do resultado (positivo é melhor), a cópia com reajuste
    só nas contas ainda sem orçamento, e a conta orçada sem movimento aparece na DRE.

    Resumo do Simples:

    Lá (`api/industrial/financial/tax-summary`):
    - a alíquota é a **nominal** da faixa (não a efetiva, com a parcela a deduzir);
    - a receita é o acumulado **do ano** (não os 12 meses anteriores) e pelo **recebido** (não pela
      competência);
    - o MEI paga 5% sobre a receita (paga um DAS fixo);
    - o padrão, sem configuração, é o Anexo II (indústria).

    Aqui: o RBT12 da lei (12 meses anteriores, competência, anualização do primeiro ano, receita de
    antes do Cardeal informável), a alíquota efetiva que o preço já usava, o DAS estimado mês a mês,
    os avisos de sublimite e limite, o MEI pelo limite do ano — e o preço sugerido passa a usar o
    mesmo RBT12 (antes: a janela até hoje, com o mês corrente).

## 6. Checklist para a NF-e (modelo 55), quando chegar a vez

Cada item é uma rejeição que eles já tomaram em produção:

- O ambiente tem que ser explícito; sem ele, a nota não sai. Valide tudo **antes** de consumir o número.
- A soma das formas de pagamento precisa bater com o total, conferida antes de reservar o número.
- `indIEDest`: com IE, `1`. **CNPJ sem IE, `2` (isento), não `9`**; o `9` dá rejeição 791 na revenda
  B2B. CPF, `9`.
- A tag IE do destinatário só vai quando `indIEDest=1`; nos outros casos a SEFAZ rejeita.
- Destinatário identificado exige `enderDest` completo, **com o código IBGE do município**, que também
  entra no registro 0150 do SPED. O cadastro de cliente precisa de CEP → IBGE.
- CFOP: se faltar, 5101 dentro do estado e 6101 fora; e se vier do lado errado (5xxx numa
  interestadual), corrija.
- `modBC=3` (valor da operação) no ICMS 00; `0` é MVA, usado na ST.
- Devolução (`finNFe=4`) exige `NFref/refNFe`.
- DIFAL (interestadual para não contribuinte) e ST por MVA: **detectar e mandar para revisão
  humana**, não calcular de improviso.
- O cStat 150 (autorizada fora do prazo) merece alerta de auditoria.
- **Denegada não se reemite**: regularizar o cadastro antes.
- Uma rejeitada se reenvia **no mesmo número** (o Cardeal já faz).
- O cancelamento só muda o estado quando a SEFAZ **aceita**. Lá houve um bug em que a nota era marcada
  cancelada mesmo com a recusa (o Cardeal já está certo).
- Se a gravação local falhar **depois** do aceite da SEFAZ, registre uma pendência de reconciliação
  com a chave e o protocolo.

## 7. Já aplicado

- **Operador de Caixa emite NFC-e.** Achado ao fazer o estorno: o papel de fábrica não tinha
  `fiscal.ver`/`fiscal.emitir`, então a venda de um operador saía sem nota, sem aviso.
- **Pix leva o grupo `card` na NFC-e** (NT 2025.001; lá os códigos `03`, `04` e `17`). O Cardeal só o
  mandava em crédito e débito.
