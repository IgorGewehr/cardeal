# Módulo Vendas

> O caminho comercial completo — orçamento, pedido, preço, comissão, entrega e devolução — que
> termina sempre no mesmo lugar: um lançamento balanceado no Razão.

## 1. Escopo

### Faz

- Mantém **orçamento** e **pedido**, com o fluxo orçamento → pedido → faturamento → entrega.
- Calcula **preço** por tabela, por faixa de quantidade, por promoção com vigência e respeita o
  **desconto máximo por papel** do vendedor.
- **Reserva estoque** no pedido confirmado (não no orçamento) e consome de verdade só no faturamento.
- Apura e paga **comissão** de vendedor.
- Processa **devolução** total e parcial, com estorno de venda, estoque e comissão coerentes.
- Suporta **contrato recorrente** (assinatura de produto/serviço), gerando pedidos periódicos que
  alimentam uma `Recorrencia` do financeiro.

### Não faz

- **Não é a frente de caixa.** `pdv` é quem finaliza venda de balcão em segundos; `vendas` é o fluxo
  de balcão consultivo, atacado e faturamento a prazo — os dois compartilham `TabelaPreco` e
  `RegraPreco`, mas `pdv` tem seu próprio comando de finalização (ver `pdv.md`).
- **Não controla saldo físico de estoque.** Reserva e consumo são comandos de `estoque`; `vendas`
  apenas os chama e usa o `custo_medio` devolvido para montar o CMV.
- **Não gera o título nem baixa o pagamento.** `FaturarPedido` cria o `Lancamento` e delega a
  criação do `Titulo`/`Parcela` ao financeiro pelo mesmo evento que qualquer outro módulo usa —
  `vendas` não lê nem escreve tabela do financeiro diretamente.
- **Não decide limite de crédito.** Consulta `LimiteDisponivel` em `clientes` e reage ao resultado;
  quem calcula o limite e o score é `clientes`.
- **Não entrega fisicamente.** `RegistrarEntrega` só marca o pedido como despachado/entregue; rota,
  motorista e rastreamento são responsabilidade de integração externa (fora do núcleo do produto).

## 2. Submódulos

| id | nome | essencial | depende de | o que some da UI quando desligado |
|---|---|---|---|---|
| `pedido` | Pedido | sim | — | Não pode ser desligado: é o agregado central do módulo. |
| `orcamento` | Orçamento | não | `pedido` | Etapa de orçamento some; venda vai direto para pedido. |
| `tabela_preco` | Tabela de Preço | sim | — | Sem ela não há preço; não pode ser desligado. |
| `comissao` | Comissão | não | `pedido` | Aba "Comissão" some da ficha do vendedor e do pedido. |
| `entrega` | Entrega | não | `pedido` | Estado `EmEntrega` e tela de expedição somem; pedido faturado vai direto a `Concluido`. |
| `devolucao` | Devolução | não | `pedido` | Botão "Devolver" some do pedido faturado. |
| `contrato_recorrente` | Contrato Recorrente | não | `pedido`, financeiro `recorrencia` | Tela de assinaturas some. |

## 3. Entidades

### Orcamento

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | |
| `empresa` | Id | sim | |
| `cliente` | Id | sim | pessoa com papel `Cliente` |
| `vendedor` | Id | sim | pessoa com papel `Vendedor` |
| `data` | Data | sim | |
| `validade` | Data | sim | após essa data o orçamento expira sozinho |
| `condicao_pagamento` | Id | sim | referencia `financeiro_condicao_pagamento` |
| `tabela_preco` | Id | sim | |
| `desconto_total` | Dinheiro | sim | soma dos descontos de item + desconto de cabeçalho |
| `estado` | enum(`Aberto`,`Aprovado`,`Expirado`,`Convertido`,`Cancelado`) | sim | ver §4 |
| `observacao` | String | não | |
| `versao` | Quantidade (inteiro) | sim | |

### Pedido

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | |
| `empresa` | Id | sim | |
| `orcamento_origem` | Id | não | preenchido se convertido de orçamento |
| `cliente` | Id | sim | |
| `vendedor` | Id | sim | |
| `data` | Data | sim | |
| `condicao_pagamento` | Id | sim | |
| `tabela_preco` | Id | sim | |
| `centro_custo` | Id | não | requer submódulo `financeiro.centro_custo` |
| `local_expedicao` | Id | sim | referencia `estoque_local` |
| `desconto_total` | Dinheiro | sim | |
| `total` | Dinheiro | sim | soma dos itens líquida de desconto |
| `estado` | enum(`Rascunho`,`Confirmado`,`Faturado`,`EmEntrega`,`Concluido`,`Cancelado`) | sim | ver §4 |
| `versao` | Quantidade (inteiro) | sim | |

### ItemPedido

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | |
| `pedido` | Id | sim | (também usado por `Orcamento`, com `pedido` apontando ao orçamento) |
| `produto` | Id | sim | |
| `variacao` | Id | não | |
| `quantidade` | Quantidade | sim | > 0 |
| `preco_unitario` | Preco | sim | resolvido por `RegraPreco` no momento da adição, congelado depois |
| `desconto_percentual` | Percentual | não | limitado por `vendas.desconto_maximo` do papel |
| `desconto_valor` | Dinheiro | sim | calculado a partir do percentual, arredondado por item |
| `total_item` | Dinheiro | sim | `Dinheiro::de_preco(quantidade, preco_unitario, MeioAcima) - desconto_valor` |
| `reserva` | Id | não | id do `Movimento Reserva` em `estoque`, preenchido ao confirmar o pedido |

### TabelaPreco / RegraPreco

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| **TabelaPreco** `id`, `nome`, `tipo`, `vigente_de`, `vigente_ate`, `ativa` | Id/String/enum(`Venda`,`Atacado`,`Promocional`)/Data/Data/enum | sim exceto `vigente_ate` | |
| **RegraPreco** `id`, `tabela_preco`, `produto`, `grupo_produto`, `quantidade_minima`, `preco`, `periodo_de`, `periodo_ate` | Id/Id/Id/Id/Quantidade/Preco/Data/Data | sim: `tabela_preco`, `preco` | `produto` **xor** `grupo_produto`; `periodo_*` só em `TabelaPreco` do tipo `Promocional`; regras com `quantidade_minima` maior vencem a de menor quando a quantidade pedida atinge a faixa |

### Comissao

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | |
| `vendedor` | Id | sim | |
| `pedido` | Id | sim | |
| `base_calculo` | Dinheiro | sim | total do pedido, líquido de devolução |
| `percentual` | Percentual | sim | **Desde 2026-10-14:** a comissão é item a item — cada item leva o percentual da comissão por estrutura de `estoque` (a do produto, a do grupo ou a padrão), **congelado no faturamento** (`vendas_item_pedido.comissao_percentual`); o `percentual` da comissão é o efetivo (valor ÷ base). Pedido faturado antes da migração usa o percentual vigente na apuração. Não há "tabela de comissão do vendedor" |
| `valor` | Dinheiro | sim | |
| `competencia` | Data | sim | mês de apuração |
| `estado` | enum(`Apurada`,`Paga`,`Cancelada`) | sim | ver §4 |
| `lancamento` | Id | não | preenchido quando `Paga` |

### Devolucao

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | |
| `pedido_origem` | Id | sim | |
| `cliente` | Id | sim | |
| `data` | Data | sim | |
| `motivo` | String | sim | |
| `tipo` | enum(`Total`,`Parcial`) | sim | |
| `valor_total` | Dinheiro | sim | soma dos itens devolvidos |
| `estado` | enum(`Solicitada`,`Aprovada`,`Concluida`,`Cancelada`) | sim | ver §4 |
| `estorno_lancamento` | Id | não | preenchido em `Concluida` |

## 4. Máquinas de estado

```mermaid
stateDiagram-v2
    [*] --> Aberto: CriarOrcamento
    Aberto --> Aprovado: cliente aceita
    Aprovado --> Convertido: ConverterOrcamentoEmPedido
    Aberto --> Expirado: validade vencida (verificação diária)
    Aberto --> Cancelado
    Aprovado --> Cancelado
```

```mermaid
stateDiagram-v2
    [*] --> Rascunho: CriarPedido
    Rascunho --> Confirmado: ConfirmarPedido (reserva estoque)
    Confirmado --> Faturado: FaturarPedido (razão + estoque + comissão)
    Faturado --> EmEntrega: RegistrarEntrega (submódulo entrega)
    Faturado --> Concluido: sem submódulo entrega
    EmEntrega --> Concluido: entrega confirmada
    Rascunho --> Cancelado: LiberarReserva não se aplica (ainda não reservou)
    Confirmado --> Cancelado: libera a reserva de estoque
    Faturado --> Cancelado: não permitido — use Devolução
```

```mermaid
stateDiagram-v2
    [*] --> Solicitada: SolicitarDevolucao
    Solicitada --> Aprovada: aprovação (permissão dedicada)
    Solicitada --> Cancelada
    Aprovada --> Concluida: ConcluirDevolucao (estorna razão + repõe estoque)
```

## 5. Comandos

| Comando | Permissão | Risco | O que faz | Erros possíveis |
|---|---|---|---|---|
| `CriarOrcamento` | `vendas.orcamento.criar` | Baixo | Ainda não implementado — orçamento ficou fora desta fatia (domínio pronto, ver §1.2 do doc 19) | `ProdutoInativo` |
| `AprovarOrcamento` | `vendas.orcamento.aprovar` | Baixo | Ainda não implementado | `OrcamentoExpirado` |
| `ConverterOrcamentoEmPedido` | `vendas.pedido.criar` | Baixo | Ainda não implementado | `OrcamentoJaConvertido` |
| `CriarPedido` ✅ | `vendas.pedido.criar` | Baixo | Abre um `Pedido Rascunho` sem itens | — |
| `AdicionarItemPedido` ✅ | `vendas.pedido.editar` | Baixo | Resolve preço vigente na `TabelaPreco` do pedido (produto/grupo via `mod-estoque`), congela no item; recusa desconto acima de `vendas.desconto_maximo` na hora (não escala para autorização de supervisor ainda) | `PrecoNaoEncontrado`, `DescontoAcimaDoLimite` |
| `AplicarDesconto` | `vendas.pedido.descontar` | Médio | Ainda não implementado como comando separado — o desconto é parâmetro de `AdicionarItemPedido` | `DescontoAcimaDoLimite` |
| `ConfirmarPedido` ✅ | `vendas.pedido.confirmar` | Médio | Só a transição `Rascunho → Confirmado` (§11.2) — **não** reserva estoque nem consulta `LimiteDisponivel` ainda (`ReservarEstoque`/`LimiteDisponivel` não existem em `mod-estoque`/`mod-clientes`) | `PedidoEmEstadoInvalido`, `PedidoSemItens` |
| `fiscal.emitir_nfe_do_pedido.v1` ✅ (2026-10-27, no `mod-fiscal`) | `fiscal.emitir` | Médio | A NF-e de venda do pedido faturado: o cliente é o destinatário (endereço com o código IBGE pelo nome da cidade), a prazo as parcelas do título viram duplicatas; vai pela fila fiscal (`docs/modulos/fiscal.md` §7) | — |
| `FaturarPedido` ✅ | `vendas.pedido.faturar` | Alto | Consome estoque item a item (`mod_estoque::registrar_saida_comum`, somando o CMV), monta o lançamento combinado (D CMV/C Estoque + D Caixa-Clientes/C Receita) e, só a prazo, cria um título a receber de parcela única vinculado a esse lançamento; publica `vendas.pedido_faturado.v1` | `PedidoEmEstadoInvalido`, `PedidoSemItens` |
| `CancelarPedido` ✅ | `vendas.pedido.cancelar` | Médio | Só antes de `Faturado` | `PedidoJaFaturado` |
| `RegistrarEntrega` | `vendas.entrega.registrar` | Baixo | Ainda não implementado | `PedidoNaoFaturado` |
| `SolicitarDevolucao` ✅ (2026-10-27, `vendas.solicitar_devolucao.v1`) | `vendas.devolucao.solicitar` | Médio | Pede a devolução de parte ou de todo um pedido faturado, item a item, com o motivo (≥ 10 caracteres). Desconta o que já voltou em devoluções anteriores (não canceladas): a quantidade não passa do que resta, e a última devolução de cada item leva o resto do valor dele | `PedidoEmEstadoInvalido`, `QuantidadeDevolvidaExcede`, `DevolucaoVazia` |
| `AprovarDevolucao` ✅ / `CancelarDevolucao` ✅ (2026-10-27) | `vendas.devolucao.aprovar` | Alto | `Solicitada → Aprovada`; cancelar vale até concluir | `DevolucaoEmEstadoInvalido` |
| `ConcluirDevolucao` ✅ (2026-10-27, `vendas.concluir_devolucao.v1`) | `vendas.devolucao.concluir` | Alto | Numa transação: a mercadoria volta ao estoque **ao custo de quando saiu** (`mod_estoque::devolver_parte_da_saida`); o dinheiro — à vista, volta ao cliente pelo caixa (D Devoluções de venda / C Caixa); a prazo, o título a receber é **abatido** (`mod_financeiro::abater_titulo_da_origem_comum`, D Devoluções de venda / C Clientes a receber, sem dinheiro) e o que já tinha sido recebido vira **crédito do cliente** (um título a pagar a ele); o CMV volta (D Estoque / C CMV); e a **comissão**: se o pedido já foi apurado, a parte devolvida (o percentual congelado de cada item sobre o valor dele) é estornada (D Comissões a pagar / C Despesas comerciais, em `vendas_comissao_ajuste`) e descontada no próximo pagamento do vendedor — se ainda não, a apuração já sai sem ela. Publica `vendas.devolucao_concluida.v1` | `DevolucaoEmEstadoInvalido`, `DevolucaoMaiorQueOResto` (estoque) |
| `fiscal.emitir_nfe_da_devolucao_de_venda.v1` ✅ (2026-10-27, no `mod-fiscal`) | `fiscal.emitir` | Médio | A NF-e de entrada da devolução concluída, quando a venda saiu em NF-e e o cliente não é contribuinte (`docs/modulos/fiscal.md` §7) | — |
| `CriarTabelaPreco` ✅ | `vendas.tabela_preco.criar` | Baixo | | `NomeDuplicado` |
| `CriarRegraPreco` ✅ | `vendas.tabela_preco.editar` | Baixo | Recusa regra que mira produto **e** grupo, ou nenhum dos dois | `RegraPrecoAmbigua` |
| `ApurarComissoes` ✅ (2026-10-14, `vendas.apurar_comissoes.v1`) | `vendas.comissao.apurar` (nova) | Médio | Idempotente por pedido: um lançamento por pedido faturado no mês ainda sem comissão, item a item com o percentual congelado no faturamento; rodar de novo só pega os pedidos novos | — |
| Venda comissionável do PDV ✅ (2026-10-28, `registrar_venda_comissionavel_comum` / `estornar_venda_comissionavel_comum`) | (a do PDV) | — | A venda do PDV com vendedor (`F4`) entra na comissão como um pedido: o PDV a registra ao finalizar, com o percentual de cada produto **congelado ali** (`vendas_venda_comissionavel` + `_item`, migração v4), e a `ApurarComissoes`, as `ComissoesDoMes` e o `PagarComissoes` leem os dois (`vendas_comissao.origem` = `pedido` ou `pdv`; a v4 recria `vendas_comissao` e `vendas_comissao_ajuste` sem a chave estrangeira para `vendas_pedido`). O estorno do cupom tira a venda da apuração ou, se ela já foi apurada, estorna a comissão inteira (D Comissões a pagar / C Despesas comerciais, em `vendas_comissao_ajuste`) e ela é descontada no próximo pagamento — o mesmo caminho da devolução | — |
| `PagarComissoes` ✅ (2026-10-14, `vendas.pagar_comissoes.v1`) | `vendas.comissao.pagar` | Médio | Quita tudo o que o vendedor tem apurado no mês, num lançamento só, de uma conta do Disponível (1.1), com data entre a apuração e hoje | `SemComissaoAPagar`, `DataDoPagamentoInvalida`, `ContaDePagamentoInvalida` |
| `CriarContratoRecorrente` | `vendas.contrato_recorrente.criar` | Baixo | Ainda não implementado | `ClienteSemCadastro` |

> **Nota (2026-09-05):** as linhas ✅ estão implementadas com teste de integração de ponta a
> ponta (`tests/comandos.rs`, `Despachante` real contra SQLite) — cliente + produto/estoque
> reais → tabela e regra de preço → pedido criado, item adicionado com preço resolvido,
> confirmado, faturado à vista (sem título) e a prazo (título vinculado ao lançamento). O
> restante desta tabela (orçamento, devolução, comissão, contrato recorrente, e a reserva de
> estoque real em `ConfirmarPedido`) fica para quando tiver consumidor — mesmo critério que
> `mod-compras` usou para cotação/pedido formal.

## 6. Consultas

| Consulta | Permissão | Uso na UI | Índice que a sustenta |
|---|---|---|---|
| `PrecoVigente` | (interna, síncrona) | Cálculo de item no orçamento/pedido | `vendas_regra_preco(tabela_preco, produto, periodo_de, periodo_ate)` |
| `OrcamentosAbertos` | `vendas.orcamento.ver` | Fila de orçamentos | `vendas_orcamento(empresa, estado, validade)` |
| `PedidosPorEstado` | `vendas.pedido.ver` | Painel de pedidos | `vendas_pedido(empresa, estado, data)` |
| `ItensDoPedido` | `vendas.pedido.ver` | Detalhe do pedido | `vendas_item_pedido(pedido)` |
| `ComissoesDoMes` ✅ (2026-10-14, `vendas.comissoes_do_mes.v1`; no lugar de `ComissaoDoVendedor`) | `vendas.comissao.ver` | Diálogo "Comissões" da tela de Vendas: pedido a pedido, o apurado/pago e a **prévia** do que falta apurar (o mesmo cálculo da apuração); `resumo_por_vendedor` agrupa | `vendas_comissao(empresa, competencia, vendedor)` + `vendas_pedido(empresa, estado, data)` |
| `DevolucoesPendentes` | `vendas.devolucao.ver` | Fila de aprovação | `vendas_devolucao(empresa, estado)` |
| `HistoricoDeComprasDoCliente` | `vendas.pedido.ver` | Aba "Histórico" da ficha do cliente | `vendas_pedido(cliente, data DESC)` |
| `ReceitaPorProduto` ✅ (2026-10-12, `vendas.receita_por_produto.v1`) | `vendas.margem.ver` | Rentabilidade por nível (aba do Estoque): receita por produto dos pedidos **faturados** com `data` no período | `vendas_pedido(empresa, estado, data)` + `vendas_item_pedido(pedido)` |

## 7. Receituário contábil

| Evento de negócio | Débito | Crédito | Estado |
|---|---|---|---|
| Pedido faturado à vista | Caixa | Receita de vendas (4.1) | Realizado |
| Pedido faturado a prazo | Clientes a receber (1.2.01) | Receita de vendas | Confirmado |
| Baixa de estoque do pedido faturado | CMV (5.1) | Estoque (1.3.01) | igual ao lançamento acima |
| Desconto concedido no pedido | Descontos concedidos (4.4) | Receita de vendas | igual ao lançamento acima |
| Devolução total | Devoluções de venda (4.5) + Estoque | Caixa/Clientes a receber + CMV (estorno) | Realizado |
| Devolução parcial | Devoluções de venda (proporcional) + Estoque (itens devolvidos) | Caixa/Clientes a receber (proporcional) + CMV (proporcional) | Realizado |
| Devolução de venda a prazo já recebida | Devoluções de venda | Fornecedores (o crédito do cliente, título a pagar a ele) | Confirmado |
| Estorno de comissão por devolução (pedido já apurado) | Salários e encargos (2.1.03) | Comissões de vendedores (5.42) ou Despesas comerciais (5.5) | Confirmado |

> **Nota (2026-10-27) — devolução de venda:** o pagamento das comissões do vendedor desconta os estornos por
> devolução ainda não descontados (do mês ou de antes); se o estorno passa da comissão do mês, o pagamento é
> recusado (`EstornoMaiorQueAComissao`) e o estorno espera. O crédito do cliente fica em Fornecedores (com o
> cliente como contraparte), não numa conta de adiantamento: a baixa do financeiro usa o papel padrão da
> espécie. O papel de fábrica **Vendedor** pede e acompanha a devolução (`vendas.devolucao.ver` e
> `.solicitar`); aprovar e concluir fica com o gerente.
| Comissão apurada | Comissões de vendedores (5.42, do modelo Comércio) ou Despesas comerciais (5.5) | Salários e encargos (2.1.03) | Confirmado |
| Pagamento de comissão | Salários e encargos (2.1.03) | Caixa ou Bancos (conta escolhida) | Realizado |
| Contrato recorrente faturado no ciclo | Clientes a receber | Receita de vendas/serviços | Confirmado |

> **Nota (2026-10-14) — comissão do vendedor:** a "subconta 2.1.03" do desenho original não existe no plano
> padrão; a comissão a pagar fica direto em **2.1.03 Salários e encargos** (é remuneração, e é ali que o
> contador espera vê-la). O débito vai para **5.42 Comissões de vendedores** quando a empresa aplicou o modelo de
> plano do Comércio, senão para 5.5. A apuração é lançada no **último dia do mês** (ou hoje, se o mês não
> acabou), para a despesa cair na competência da venda; o mês é o da data do pedido, a mesma do lançamento da
> receita. O pagamento é um lançamento por vendedor e mês, com o vendedor como contraparte.

## 8. Eventos

**Publicados**

- `vendas.orcamento_aprovado.v1` — `{ orcamento, cliente }`
- `vendas.pedido_confirmado.v1` — `{ pedido, itens_reservados }`
- `vendas.pedido_faturado.v1` — `{ pedido, cliente, total, competencia }` — assinado por `financeiro`
  (gera `Titulo`) e por `crm` (registra interação de compra), quando ativos.
- `vendas.pedido_cancelado.v1` — `{ pedido, motivo }`
- `vendas.devolucao_concluida.v1` — `{ devolucao, pedido_origem, valor_total }`
- `vendas.comissao_apurada.v1` — `{ comissao, vendedor, valor }`

**Assinados**

- `estoque.saldo_alterado.v1` → recalcula viabilidade de itens em orçamentos abertos (aviso de
  ruptura, não bloqueio).
- `clientes.credito_bloqueado.v1` → impede `ConfirmarPedido` a prazo para o cliente até liberação.

## 9. Permissões

| Chave | Descrição | Risco |
|---|---|---|
| `vendas.orcamento.ver` | Consultar orçamentos | Baixo |
| `vendas.orcamento.criar` | Criar orçamento | Baixo |
| `vendas.orcamento.aprovar` | Aprovar orçamento | Baixo |
| `vendas.pedido.ver` | Consultar pedidos | Baixo |
| `vendas.pedido.criar` | Criar pedido | Baixo |
| `vendas.pedido.editar` | Editar itens do pedido | Baixo |
| `vendas.pedido.descontar` | Aplicar desconto | Médio |
| `vendas.pedido.confirmar` | Confirmar pedido (reserva estoque) | Médio |
| `vendas.pedido.faturar` | Faturar pedido | Alto |
| `vendas.pedido.cancelar` | Cancelar pedido não faturado | Médio |
| `vendas.entrega.registrar` | Registrar entrega | Baixo |
| `vendas.devolucao.ver` | Ver devoluções | Baixo |
| `vendas.devolucao.solicitar` | Solicitar devolução | Baixo |
| `vendas.devolucao.aprovar` | Aprovar devolução | Alto |
| `vendas.devolucao.concluir` | Concluir devolução (estorno) | Alto |
| `vendas.tabela_preco.ver` | Consultar tabelas de preço | Baixo |
| `vendas.tabela_preco.criar` | Criar tabela de preço | Médio |
| `vendas.tabela_preco.editar` | Editar regra de preço | Médio |
| `vendas.comissao.ver` | Ver apuração de comissão | Baixo |
| `vendas.comissao.pagar` | Pagar comissão | Médio |
| `vendas.contrato_recorrente.criar` | Criar contrato recorrente | Médio |

As permissões de leitura `vendas.devolucao.ver` e `vendas.tabela_preco.ver` foram
acrescentadas junto do manifesto (`mod-vendas`): a §6 já pressupunha as consultas e toda
entrada de menu precisa de uma permissão declarada.

## 10. Telas

### Pedido (montagem)

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│  Pedido #10482 · Mercado Bom Preço LTDA          Tabela: Atacado    Rascunho          │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  🔍 [buscar produto_____________________]                            Ctrl+N Item novo │
├──────────┬──────────────────────────┬─────────┬────────────┬──────────┬──────────────┤
│ Código   │ Produto                  │ Qtd     │ Preço unit.│ Desc.    │ Total         │
├──────────┼──────────────────────────┼─────────┼────────────┼──────────┼──────────────┤
│ 78912...  │ Refrigerante Cola 2L     │  120 UN │    4,90    │   5,0%   │    558,60    │
│ 78913...  │ Refrigerante Guaraná 2L  │   60 UN │    4,90    │   5,0%   │    279,30    │
├──────────┴──────────────────────────┴─────────┴────────────┴──────────┴──────────────┤
│  Condição: [30/60/90 ▾]     Centro de custo: [Loja ▾]         Total: R$ 837,90        │
│                                              [F2 Confirmar pedido]  [F8 Cancelar]      │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

### Autorização de desconto acima do limite

```
┌─────────────────────────────────────────────────────────┐
│  Desconto acima do seu limite (5%)                        │
│  Solicitado: 12% — requer autorização de supervisor       │
│                                                             │
│  Supervisor: [________________]  Senha: [••••••••]        │
│                              [F2 Autorizar]  [Esc Cancelar]│
└─────────────────────────────────────────────────────────┘
```

### Devolução

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│  Devolução do Pedido #10482                                     [Total] [Parcial ●]   │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  ☑ Refrigerante Cola 2L        120 UN → devolver [ 20] UN     valor: R$ 93,10         │
│  ☐ Refrigerante Guaraná 2L      60 UN → devolver [  0] UN                             │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  Motivo: [Avaria no transporte________________]                                       │
│                                            [F2 Solicitar devolução]  [Esc Cancelar]   │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

## 11. Regras de negócio críticas

1. **Preço é congelado no item no momento em que é adicionado.** Alterar a `RegraPreco` depois não
   muda pedidos já criados — só afeta pedidos novos ou itens adicionados depois da mudança.
2. **Reserva de estoque acontece na confirmação, nunca na criação do pedido.** Um `Rascunho` não
   compromete saldo — o vendedor pode montar um pedido grande sem travar estoque de outros clientes
   até de fato confirmar a intenção.
3. **Faturar é irreversível; a correção é devolução.** `FaturarPedido` não tem `Cancelar` depois —
   simetricamente à regra do Razão (nada é `UPDATE`, é estorno), a correção de um pedido faturado é
   sempre uma `Devolucao`, nunca uma edição.
4. **Desconto acima do limite do papel sempre exige autorização de supervisor na hora**, nunca fica
   pendente para aprovação posterior — ou o vendedor sabe o preço final ao fechar com o cliente, ou
   não fecha.
5. **Devolução parcial rateia proporcionalmente, sem perder centavo.** O valor estornado de cada
   item usa `Dinheiro::ratear`, e a soma dos itens devolvidos bate exatamente com `valor_total`.
6. **Comissão nunca é apurada sobre pedido cancelado ou totalmente devolvido**, e é recalculada
   (não editada) quando há devolução parcial após a apuração — gera um lançamento de ajuste, nunca
   sobrescreve o valor apurado original.
7. **`vendas` nunca decide se um cliente pode comprar a prazo.** Ele só lê `LimiteDisponivel` de
   `clientes` e, se negativo, oferece as opções que a permissão do usuário autoriza (autorizar
   mesmo assim, mudar para à vista, ou recusar).
8. **Contrato recorrente nunca duplica a `Recorrencia`.** `CriarContratoRecorrente` cria exatamente
   uma `Recorrencia` no financeiro por contrato; o financeiro é quem decide quando ela vira `Titulo`.

## 12. Comportamento offline

**Funciona em modo autônomo:**
- Criar orçamento e pedido, adicionar itens com preço do último sincronismo da tabela.
- Confirmar e faturar pedido à vista (reserva e baixa de estoque locais, como qualquer venda).
- Consultar histórico de pedidos do cliente já sincronizado.

**Não funciona em modo autônomo:**
- Faturar pedido a prazo acima do limite de crédito sincronizado sem confirmação humana explícita
  (a venda é aceita conforme a política geral, mas fica marcada `AcimaDoLimite` — ver
  [doc 03 §3.3](../03-pilar-resiliencia.md#33-reconciliação)).
- Aprovar devolução (permissão de risco alto costuma exigir papel de gerência com acesso ao servidor).
- Apurar e pagar comissão (é processo em lote, mensal, coordenado no servidor).
- Editar tabela de preço e regra de preço.

**Política de conflito na reconciliação:** um pedido faturado offline nunca é desfeito ao reconectar.
Se o preço mudou no servidor durante a janela offline, vale o preço praticado no terminal — é o que
foi cobrado do cliente — e o sistema gera relatório de divergência de preço para conferência, sem
jamais alterar retroativamente o valor já cobrado.

## 13. Tabelas

> **Nota (2026-09-05):** este é o esquema-alvo completo. A fatia implementada
> (`crates/modulos/mod-vendas/src/migracoes.rs`) tem só `vendas_tabela_preco`/
> `vendas_regra_preco`/`vendas_pedido`/`vendas_item_pedido` — os únicos submódulos
> essenciais são `pedido` e `tabela_preco` (§2). `vendas_orcamento` e as tabelas de
> devolução/comissão/contrato recorrente estão adiadas, então não existem ainda; `criado_em`
> também não está na versão implementada de `vendas_pedido` (só `criado_por`).

```sql
CREATE TABLE vendas_tabela_preco (
    id           BLOB PRIMARY KEY,
    empresa      BLOB    NOT NULL,
    nome         TEXT    NOT NULL,
    tipo         TEXT    NOT NULL CHECK (tipo IN ('Venda','Atacado','Promocional')),
    vigente_de   INTEGER NOT NULL,
    vigente_ate  INTEGER,
    ativa        INTEGER NOT NULL DEFAULT 1 CHECK (ativa IN (0,1)),
    versao       INTEGER NOT NULL DEFAULT 1
) STRICT;

CREATE TABLE vendas_regra_preco (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    tabela_preco        BLOB    NOT NULL REFERENCES vendas_tabela_preco(id),
    produto             BLOB,
    grupo_produto       BLOB,
    quantidade_minima   INTEGER,     -- Quantidade, escala 1e-4
    preco               INTEGER NOT NULL,   -- Preco, escala 1e-6
    periodo_de          INTEGER,
    periodo_ate         INTEGER,
    CHECK ((produto IS NULL) <> (grupo_produto IS NULL))
) STRICT;
CREATE INDEX vendas_regra_preco_busca ON vendas_regra_preco(tabela_preco, produto);
CREATE INDEX vendas_regra_preco_grupo ON vendas_regra_preco(tabela_preco, grupo_produto);

CREATE TABLE vendas_orcamento (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    cliente             BLOB    NOT NULL,
    vendedor            BLOB    NOT NULL,
    data                INTEGER NOT NULL,
    validade            INTEGER NOT NULL,
    condicao_pagamento  BLOB    NOT NULL,
    tabela_preco        BLOB    NOT NULL REFERENCES vendas_tabela_preco(id),
    desconto_total      INTEGER NOT NULL DEFAULT 0,
    estado              TEXT    NOT NULL CHECK (estado IN ('Aberto','Aprovado','Expirado','Convertido','Cancelado')),
    observacao          TEXT,
    versao              INTEGER NOT NULL DEFAULT 1
) STRICT;
CREATE INDEX vendas_orcamento_estado ON vendas_orcamento(empresa, estado, validade);

CREATE TABLE vendas_pedido (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    orcamento_origem    BLOB REFERENCES vendas_orcamento(id),
    cliente             BLOB    NOT NULL,
    vendedor            BLOB    NOT NULL,
    data                INTEGER NOT NULL,
    condicao_pagamento  BLOB    NOT NULL,
    tabela_preco        BLOB    NOT NULL REFERENCES vendas_tabela_preco(id),
    centro_custo        BLOB,
    local_expedicao     BLOB    NOT NULL,
    desconto_total      INTEGER NOT NULL DEFAULT 0,
    total               INTEGER NOT NULL DEFAULT 0,
    estado              TEXT    NOT NULL CHECK (estado IN ('Rascunho','Confirmado','Faturado','EmEntrega','Concluido','Cancelado')),
    versao              INTEGER NOT NULL DEFAULT 1,
    criado_em           INTEGER NOT NULL,
    criado_por          BLOB    NOT NULL
) STRICT;
CREATE INDEX vendas_pedido_estado ON vendas_pedido(empresa, estado, data);
CREATE INDEX vendas_pedido_cliente ON vendas_pedido(cliente, data DESC);

CREATE TABLE vendas_item_pedido (
    id                    BLOB PRIMARY KEY,
    pedido                BLOB    NOT NULL REFERENCES vendas_pedido(id),
    produto               BLOB    NOT NULL,
    variacao              BLOB    NOT NULL DEFAULT (X''),
    quantidade            INTEGER NOT NULL,   -- Quantidade
    preco_unitario        INTEGER NOT NULL,   -- Preco
    desconto_percentual   INTEGER NOT NULL DEFAULT 0,
    desconto_valor        INTEGER NOT NULL DEFAULT 0,
    total_item            INTEGER NOT NULL,
    reserva               BLOB
) STRICT;
CREATE INDEX vendas_item_pedido_pedido ON vendas_item_pedido(pedido);

-- Migração vendas v2 (2026-10-14). `pedido` é único (a apuração é idempotente por pedido);
-- `competencia` é a data do lançamento da apuração (sempre dentro do mês); `lancamento` é o do
-- pagamento. O item ganhou `comissao_percentual`, congelado no faturamento.
ALTER TABLE vendas_item_pedido ADD COLUMN comissao_percentual INTEGER;
CREATE TABLE vendas_comissao (
    id                   BLOB PRIMARY KEY,
    empresa              BLOB    NOT NULL,
    vendedor             BLOB    NOT NULL,
    pedido               BLOB    NOT NULL UNIQUE REFERENCES vendas_pedido(id),
    base_calculo         INTEGER NOT NULL,
    percentual           INTEGER NOT NULL,   -- Percentual efetivo (valor ÷ base)
    valor                INTEGER NOT NULL,
    competencia          INTEGER NOT NULL,
    estado               TEXT    NOT NULL CHECK (estado IN ('Apurada','Paga','Cancelada')),
    lancamento_apuracao  BLOB    NOT NULL,
    lancamento           BLOB,
    criado_em            INTEGER NOT NULL
) STRICT;
CREATE INDEX vendas_comissao_competencia ON vendas_comissao(empresa, competencia, vendedor);

CREATE TABLE vendas_devolucao (
    id                  BLOB PRIMARY KEY,
    empresa             BLOB    NOT NULL,
    pedido_origem       BLOB    NOT NULL REFERENCES vendas_pedido(id),
    cliente             BLOB    NOT NULL,
    data                INTEGER NOT NULL,
    motivo              TEXT    NOT NULL,
    tipo                TEXT    NOT NULL CHECK (tipo IN ('Total','Parcial')),
    valor_total         INTEGER NOT NULL,
    estado              TEXT    NOT NULL CHECK (estado IN ('Solicitada','Aprovada','Concluida','Cancelada')),
    estorno_lancamento  BLOB
) STRICT;
CREATE INDEX vendas_devolucao_estado ON vendas_devolucao(empresa, estado);
```

## Painel de margem do pedido (2026-10-09)

`vendas.margem_do_pedido.v1` (permissão `vendas.margem.ver`, risco Médio) devolve, para um pedido, a
margem de cada item e do conjunto: receita (já com desconto), custo (custo médio atual do estoque,
o maior entre os locais), encargos sobre a receita (custo operacional + imposto do produto +
comissão do produto — própria, do grupo ou a padrão, a comissão por estrutura de `estoque` — + taxa de
pagamento — os mesmos percentuais do preço sugerido, ver
[ADR-0018](../adr/0018-formacao-de-preco-por-markup-divisor.md)), margem bruta e líquida em R$ e em %.
Itens sem custo conhecido ficam **fora** das margens e são contados à parte: um produto sem custo
nunca aparece como lucro de 100%. A margem mínima vem dos parâmetros de preço; abaixo dela o pedido
e os itens aparecem sinalizados. `vendas.aplicar_precos_sugeridos.v1` leva o preço sugerido do
estoque para uma tabela de venda/atacado (ver `docs/modulos/estoque.md` §5).

## Preços defasados (2026-10-12)

`vendas.precos_defasados.v1` (permissão `vendas.tabela_preco.ver`, só leitura): para cada regra
**permanente** de produto de uma tabela (sem faixa de quantidade e sem período; havendo mais de uma,
vale a mais recente) compara o preço praticado com o que o estoque calcula hoje
(`mod_estoque::sugerir_precos`, com o custo de referência configurado). Devolve produto, preço atual,
custo, margem atual (`precificacao::margem_do_preco`), margem desejada, preço sugerido, preço mínimo e a
`situacao`, em ordem de urgência:

| Situação | Regra (função pura `classificar`) |
|---|---|
| `Prejuizo` | preço abaixo do preço de empate, ou margem atual negativa |
| `AbaixoDoAlvo` | margem atual mais de 1 ponto abaixo da desejada |
| `NoAlvo` | de desejada − 1 ponto até desejada + 5 pontos (limites inclusivos) |
| `AcimaDoAlvo` | margem atual mais de 5 pontos acima da desejada |
| `SemCalculo(motivo)` | sem custo ou sem margem; o motivo é o `problema` da sugestão |

Na aba **Preços** do Estoque, a seção "Preços defasados" lista por tabela (Defasados / Acima do alvo /
Todos) e o botão "Aplicar o preço sugerido aos defasados" chama `vendas.aplicar_precos_sugeridos.v1`
só para `Prejuizo` e `AbaixoDoAlvo`. Ao **confirmar uma entrada de compra**, a tela de Compras consulta
a primeira tabela de venda ativa e avisa quantos preços ficaram defasados (erros e falta de permissão
são ignorados; lançar a nota não muda custo, então não avisa).

## Receita por produto (2026-10-12)

`vendas.receita_por_produto.v1` (permissão `vendas.margem.ver`, só leitura) recebe um `Periodo` e devolve
`Vec<ReceitaDoProduto { produto, quantidade, receita }>` (tipo de `cardeal-analytics`, igual ao de `pdv` e
`os`): soma `total_item` (já com o desconto do item) dos itens de pedidos em `Faturado` cuja `data` está no
período. Rascunho, confirmado sem faturar e cancelado ficam de fora. Alimenta a aba Rentabilidade do
Estoque; ver `docs/modulos/estoque.md`.
