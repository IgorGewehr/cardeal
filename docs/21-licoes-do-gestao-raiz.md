# 21 — Lições do `gestao-raiz`

> Comparação feita em 2026-10-15 contra o `gestao-raiz` atualizado (`main` em `4cb2c1e`, em
> `Documents/Automa/gestao-raiz`). Ele é um ERP **industrial** web (Next.js + Firestore, ~185 mil
> linhas, 33 arquivos de teste) **em produção, com usuários reais**. Ele não é modelo de arquitetura,
> porque o Cardeal tem Razão de partidas dobradas, é offline-first e roda no desktop. O que vale ouro
> são as **regras de negócio que só aparecem com uso real**: os comentários `BUG FIX`, `Fix N` e
> `NUNCA` do código são a lista de armadilhas que já custaram caro a alguém.

## 1. O que cada um cobre

| Área | gestao-raiz | Cardeal |
|---|---|---|
| Fiscal | NF-e, NFC-e (síncrona, sem contingência), MDF-e, CC-e, cancelamento, DANFE, notas recebidas (DFe + manifestação) | NFC-e assíncrona com fila (offline-first), DANFCE, cancelamento, inutilização. Sem NF-e, CC-e e DFe real |
| Compras | XML/SEFAZ, classificação da nota, custo com rateio, parcelas, estornar importação | XML de arquivo, casamento em cascata, rateio, ST/IPI no custo. Parcela única, sem estorno, sem DFe real |
| Estoque | Matéria-prima com FEFO, embalagens, produto acabado, rastreabilidade e recall | Árvore mercadológica, markup com imposto, inventário cego, transferência, perdas e validade |
| PDV | Venda, NFC-e, pagamento misto, **cancelar venda** (estorna tudo) | Teclado, diário do carrinho, sessão de caixa (sangria, suprimento, fechamento cego), NFC-e em fila. **Sem estorno de venda finalizada** |
| Vendas B2B | Pedido em etapas (aprovar, pagar, preparar, faturar, enviar, entregar), app do revendedor, comissão | Pedido, tabela e regra de preço, promoção, comissão apurada e paga |
| Clientes / CRM | Cadastro, CEP → IBGE, importação, score/tier automático, interações | Pessoa única com papéis, limite de crédito, contatos e endereços. Sem score e sem importação |
| Financeiro | Contas a pagar/receber, portadores, **transferências**, DRE, orçamento por categoria, extrato PDF (Cresol), gateways | Razão de partidas dobradas, caixa, OFX, conciliação, DFC, balancete, cheques, régua de cobrança, plano de contas. **Sem transferência entre contas** |
| Contábil | **Fechamento do mês**: ZIP com XMLs emitidos e recebidos, DANFEs, SPED EFD e relatórios, enviado por e-mail/WhatsApp | Pacote do contador (PDF + `.xlsx`), ainda sem XMLs e sem SPED |
| Indústria | Fórmulas, OP, MRP, PCP, rendimento, qualidade/SGQ, ANVISA, rotulagem, estabilidade, pragas | `mod-industria` ainda é spec. Fora do foco atual |
| IA | Assistente, onboarding por chat, agente | Fora (Fase 7) |

## 2. Onde o Cardeal já está à frente (não copiar)

- **Saldo derivado por construção.** O gestao-raiz precisou de um cron noturno que recalcula o saldo
  bancário a partir dos títulos. Dois bugs vieram daí: o lançamento do PDV era apagado pelo cron e
  duplicado na conciliação do extrato. No Cardeal o saldo sai do Razão, então esse problema não existe.
- **NFC-e offline-first.** Lá a emissão é síncrona e, se falhar, o operador precisa tentar de novo.
  Aqui o documento nasce com a venda e a fila o transmite. O "pre-flight" que eles criaram (gravar a
  nota antes de chamar o gateway) é exatamente o nosso desenho.
- **Caixa do PDV.** Lá não há sessão de caixa, sangria, suprimento nem fechamento cego.
- **Análise.** DFC, balancete, rentabilidade por nível da árvore e ABC; os relatórios deles são básicos.

## 3. O que trazer, em ordem de prioridade para o MVP

1. **Estornar a venda finalizada no PDV.** Lá, "cancelar venda" devolve o estoque, estorna o
   financeiro e o saldo e pede o cancelamento da NFC-e com uma justificativa automática. Se a SEFAZ
   recusar, por exemplo por prazo, a venda fica marcada "NFC-e a cancelar" e o operador é orientado a
   emitir uma NFC-e de devolução. No Cardeal o `CancelarNfce` já existe; falta o estorno da venda que o
   chama.
2. **Nunca transformar em compra uma nota que não é compra.** A função `classifyNote` deles:
   - se o emitente é a própria empresa, é saída própria;
   - se o destinatário é outro CNPJ, a nota não é nossa;
   - se `finNFe=4`, é devolução de cliente (entra no estoque, mas não gera conta a pagar);
   - só se o destinatário é a empresa a nota é compra.

   O `cardeal_fiscal::interpretar` não lê o destinatário nem o `finNFe`, então hoje o XML de uma
   venda nossa entraria como compra, com estoque e título a pagar.
3. **Transferência entre contas** (caixa para banco, banco para banco). Ela entrou lá em julho de 2026,
   quando o Caixa virou um portador de verdade. O Cardeal não tem; a sangria não leva o dinheiro a
   nenhuma conta.
4. **Notas recebidas pela SEFAZ (DFe) e manifestação.** Fluxo deles (`lib/fiscal/sefaz-distribuicao.ts`):
   - a distribuição por NSU;
   - se só vier o resumo (`resNFe`), registram a **Ciência da Operação** sozinhos e baixam o XML completo;
   - se a manifestação vier com 596 (passaram os 10 dias), salvam o resumo e pedem o XML ao fornecedor
     (upload manual);
   - na conexão mTLS usam o **trust store ICP-Brasil**, nunca `rejectUnauthorized:false`.

   **O `sefaz-api` não tem essas rotas** (só emissão, consulta, cancelamento, CC-e, inutilização e
   status). O código é TypeScript e pode ser levado quase direto para lá (`/nfe/distribuicao` e
   `/nfe/manifestar`). Depois disso, `verificar_notas_na_sefaz` vira uma tarefa do despacho.
5. **Parcelas da nota de compra** (`cobr/dup`) e **nota que já entra paga**. Lá cada parcela tem um id
   determinístico, então refazer a importação cria só o que falta. Aqui a parcela é única e vence na
   emissão.
6. **Estornar a entrada de uma nota** (`revertNoteImport`): desfaz lotes, estoque e conta a pagar.
7. **Fechamento do mês para o contador com os XMLs**: emitidas, canceladas, inutilizadas e recebidas,
   mais os DANFEs, o SPED EFD e os relatórios, num ZIP. Se faltar algum XML na base, ele é baixado da
   SEFAZ antes. O pacote do contador do Cardeal ganha os documentos fiscais. Isso pede guardar o **XML
   do evento de cancelamento**, que hoje só fica com o protocolo; a guarda é obrigatória por 5 anos.
8. **Importar clientes por planilha.** Trazer a carteira de outro sistema é o primeiro dia do cliente.
9. **Prazo de cancelamento visível**: 30 minutos para NFC-e e 24 horas para NF-e. Lá o prazo bloqueia o
   pedido e a mensagem indica a alternativa (NFC-e de devolução ou CC-e). Aqui a SEFAZ responde 501.
   Proposta: mostrar o tempo decorrido e avisar, sem bloquear, porque algumas UFs aceitam fora do prazo
   (155).
10. **Orçamento mensal por categoria** (realizado × orçado, alerta de estouro) e **score/tier de
    cliente** (bronze, prata, ouro pelo histórico): úteis, mas depois do básico.

## 4. Checklist para a NF-e (modelo 55), quando chegar a vez

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

## 5. Já aplicado

- **Pix leva o grupo `card` na NFC-e** (NT 2025.001; lá os códigos `03`, `04` e `17`). O Cardeal só o
  mandava em crédito e débito.
