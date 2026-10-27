# Módulo PDV

> A frente de caixa: teclado nas duas mãos, cupom impresso em menos de 400 ms, e a garantia de que
> nenhuma venda se perde mesmo que a energia caia no meio dela.

## 1. Escopo

### Faz

- Implementa a **venda de balcão** com fluxo 100% orientado a teclado (ver §1 do
  [doc 12](../12-ui-ux.md#1-princípios-de-interface), princípio 3).
- Mantém o **diário durável do carrinho** — recuperação de venda em andamento após queda de energia.
- Opera **sessão de caixa, sangria e suprimento** como camada de teclas rápidas sobre os comandos do
  `financeiro` (não duplica a entidade `SessaoCaixa`).
- Gerencia **faixas de numeração por terminal** para venda em modo autônomo, sem conflito entre PDVs.
- Integra **TEF**, **balança** e **impressora** como portas (traits), nunca como código acoplado a
  um fabricante.
- Suporta o layout de **pista** (posto de combustível, tela do frentista) e **delivery** (cupom com
  endereço e status de entrega).

### Não faz

- **Não é o dono da sessão de caixa.** `SessaoCaixa`, `MovimentoCaixa`, sangria e suprimento são
  entidades e comandos de `financeiro`; o PDV só oferece `F9`/`F12` como atalho que despacha os
  mesmos comandos (`financeiro.RegistrarSangria`, `financeiro.AbrirCaixa`...) — ver `financeiro.md`.
- **Não decide preço nem regra de desconto.** Lê `TabelaPreco`/`RegraPreco` de `vendas` pela mesma
  porta que o pedido de balcão usa; o PDV apenas aplica o resultado rapidamente.
- **Não controla estoque físico.** Consome saldo através da porta `PortaCatalogo` (implementada por
  `estoque`), com saldo consultado offline como estimativa do último sincronismo.
- **Não emite o documento fiscal.** `FinalizarVenda` fecha a operação comercial e imprime o cupom
  operacional; a NFC-e segue assíncrona por `mod-fiscal` (princípio central do
  [doc 10 §3](../10-modulo-fiscal.md#3-o-princípio-que-governa-tudo-fiscal-é-assíncrono) — a venda
  nunca espera a SEFAZ).
- **Não processa cartão.** Toda comunicação com a maquininha passa por `PortaTef`; o PDV só manda o
  valor e recebe autorização ou recusa.

## 2. Submódulos

| id | nome | essencial | depende de | o que some da UI quando desligado |
|---|---|---|---|---|
| `frente_caixa` | Frente de Caixa | sim | — | Não pode ser desligado: é a razão de existir do módulo. |
| `sangria` | Sangria e Suprimento | sim | — | `F9` some da tela de venda. |
| `turno` | Turno de Caixa | sim | — | `F12` some; caixa fica sempre "aberto" tecnicamente (não recomendado, existe só para telas de teste). |
| `tef` | TEF | não | — | Cartão sai da tela de pagamento; só dinheiro, Pix manual e carteira. |
| `balanca` | Balança | não | — | `F4` deixa de oferecer leitura automática de peso; quantidade é sempre digitada. |
| `pista` | Pista (posto) | não | `combustivel` | Tela de bico/frentista some; PDV vira só loja de conveniência. |
| `delivery` | Delivery | não | `frente_caixa` | Campo de endereço e status de entrega somem do cupom. |

## 3. Entidades

### Cupom

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | UUIDv7, gerado no terminal — funciona offline |
| `empresa` | Id | sim | |
| `sessao_caixa` | Id | sim | referencia `financeiro_sessao_caixa`, precisa estar `Aberta` |
| `terminal` | Id | sim | dispositivo que originou a venda |
| `numero_terminal` | Quantidade (inteiro) | sim | sequencial dentro da faixa reservada do terminal |
| `serie_fiscal` | Quantidade (inteiro) | sim | série NFC-e do terminal |
| `cliente` | Id | não | opcional — venda sem identificação é o padrão |
| `operador` | Id | sim | |
| `abertura` | Instante | sim | quando o primeiro item foi bipado |
| `finalizado_em` | Instante | não | |
| `subtotal` | Dinheiro | sim | soma dos itens antes de desconto |
| `desconto` | Dinheiro | sim | |
| `total` | Dinheiro | sim | |
| `estado` | enum(`EmAndamento`,`Finalizado`,`Cancelado`) | sim | ver §4 |
| `entrega_endereco` | String | não | só com submódulo `delivery` |
| `entrega_estado` | enum(`EmPreparo`,`EmRota`,`Entregue`) | não | idem |
| `documento_fiscal` | Id | não | preenchido quando `mod-fiscal` cria o documento correspondente |
| `versao` | Quantidade (inteiro) | sim | |

### ItemCupom

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | |
| `cupom` | Id | sim | |
| `produto`, `variacao` | Id | produto sim | |
| `quantidade` | Quantidade | sim | pode vir da balança (`PortaBalanca`) |
| `preco_unitario` | Preco | sim | congelado no momento da inclusão |
| `desconto_percentual` | Percentual | não | |
| `total_item` | Dinheiro | sim | |
| `cancelado` | enum(`Sim`,`Nao`) | sim | `F7` marca `Sim`, item some da tela mas não da tabela |
| `motivo_cancelamento` | String | não | |
| `autorizado_por` | Id | não | preenchido se o cancelamento exigiu supervisor (item já pago parcialmente) |

### PagamentoCupom

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | |
| `cupom` | Id | sim | |
| `forma_pagamento` | Id | sim | referencia `financeiro_forma_pagamento` |
| `valor` | Dinheiro | sim | |
| `autorizacao_tef` | String | não | código devolvido por `PortaTef` |
| `bandeira` | String | não | |
| `parcelas_cartao` | Quantidade (inteiro) | não | |

### FaixaNumeracao

| Campo | Tipo | Obrigatório | Regra |
|---|---|---|---|
| `id` | Id | sim | |
| `terminal` | Id | sim | |
| `serie_fiscal` | Quantidade (inteiro) | sim | uma série por terminal — nunca compartilhada |
| `numero_inicial` / `numero_final` | Quantidade (inteiro) | sim | intervalo reservado na abertura de caixa |
| `proximo` | Quantidade (inteiro) | sim | |
| `reservada_em` | Instante | sim | |

## 4. Máquinas de estado

```mermaid
stateDiagram-v2
    [*] --> EmAndamento: primeiro item bipado
    EmAndamento --> EmAndamento: item / quantidade / desconto / cliente
    EmAndamento --> Finalizado: F2 FinalizarVenda
    EmAndamento --> Cancelado: F8 CancelarCupom (exige supervisor)
    Finalizado --> Estornado: EstornarVenda (F11, exige supervisor)
    note right of Estornado
        Estorna o lançamento da venda e devolve a
        mercadoria ao custo em que saiu. A NFC-e
        é do mod-fiscal: AnularDocumentoDaVenda a
        cancela ou inutiliza pela fila.
    end note
```

O estado `Estornado` mora numa tabela à parte (`pdv_estorno`, migração v2): na coluna
`pdv_cupom.estado` a venda continua `Finalizado`, e o repositório devolve `Estornado` quando há a
linha do estorno. Recriar `pdv_cupom` para mudar o `CHECK` quebraria as chaves estrangeiras de
itens e pagamentos.

## 5. Comandos

| Comando | Permissão | Risco | O que faz | Erros possíveis |
|---|---|---|---|---|
| `AbrirCupom` ✅ (não estava no spec original) | `pdv.venda.editar` | Baixo | Abre o `Cupom EmAndamento` explicitamente, em vez de nascer implícito no primeiro `AdicionarItem` — mesmo padrão de `CriarPedido`/`AbrirOrdemServico` no resto do código (ver nota no arquivo do comando) | `CaixaFechado` |
| `AdicionarItem` ✅ | `pdv.venda.editar` | Baixo | Resolve produto e preço vigente na `TabelaPreco` do cupom (via `mod-vendas`), congela no item | `ProdutoNaoEncontrado`, `PrecoNaoEncontrado` |
| `AlterarQuantidade` | `pdv.venda.editar` | Baixo | Ainda não implementado — `PortaBalanca` não existe; quantidade só entra via `AdicionarItem` | `PesoInstavel` |
| `AplicarDesconto` ✅ (nomeado `AplicarDescontoItem`) | `pdv.desconto.aplicar` | Médio | `F5` — valida contra `vendas.desconto_maximo`; ainda **não** escala para autorização de supervisor acima do teto. Com desconto no total na venda, o teto vale para a venda inteira | `DescontoAcimaDoLimite`, `DescontoDaVendaAcimaDoLimite` |
| `AplicarDescontoNoTotal` ✅ (2026-10-16, `pdv.aplicar_desconto_no_total.v1`) | `pdv.desconto.aplicar` | Médio | `Shift+F5` — "10%" ou um valor ("fecha em R$ 100"); `None` tira. Vale sobre o que os itens somam (já com o desconto de cada um) e acompanha item bipado ou cancelado (`Cupom::recalcular`; migração v5 guarda a regra em `pdv_cupom`). O **teto do papel vale para a venda inteira**: desconto dos itens + do total ≤ `subtotal × vendas.desconto_maximo`. Ao finalizar, `FinalizarVenda` confere o teto de novo (item cancelado depois de um desconto em valor sobe o percentual) e **rateia** o desconto entre os itens, na proporção do valor de cada um (`Dinheiro::ratear_por_pesos`) — daí em diante ele mora nos itens: a NFC-e leva o `vDesc` por item, o comprovante e o estorno enxergam. O lançamento continua o mesmo (desconto concedido = `cupom.desconto`) | `DescontoInvalido`, `DescontoDaVendaAcimaDoLimite`, `CupomEmEstadoInvalido` |
| `IdentificarCliente` | `pdv.venda.editar` | Baixo | Ainda não implementado como comando separado — `AbrirCupom` já aceita `cliente: Option<Id>` desde a abertura | — |
| `CancelarItem` ✅ | `pdv.item.cancelar` | Baixo | `F7` — remove item do total, mantém linha auditável | `ItemInexistente` |
| `CancelarCupom` ✅ | `pdv.cupom.cancelar` | Alto | `F8` — registra `autorizado_por` e `motivo`; a re-autenticação do supervisor em si é da UI/sessão (mesmo padrão de `AprovarOrcamentoOs::identificacao_aprovador`), não algo que o comando reverifica | `MotivoObrigatorio`, `CupomJaFinalizado` |
| `ConsultarPreco` | `pdv.preco.consultar` | Baixo | Ainda não implementado | — |
| `IniciarPagamentoTef` | `pdv.tef.operar` | Médio | Ainda não implementado — `PortaTef` não existe | `TefIndisponivel`, `TefRecusado` |
| `EstornarVenda` ✅ (2026-10-15, `pdv.estornar_venda.v1`) | `pdv.venda.estornar` | Alto | Desfaz uma venda `Finalizado`, numa transação só: estorna no Razão o lançamento da venda (achado pela origem `pdv/cupom`) e devolve ao estoque cada saída do cupom **ao custo em que saiu** (`mod_estoque::devolver_saidas_da_origem`, que aceita custo zero). Exige motivo (≥ 10 caracteres, vai ao histórico do estorno) e `autorizado_por`; a sessão que executa é a do supervisor. Devolve o que restituir forma por forma | `MotivoDoEstornoCurto`, `VendaJaEstornada`, `CupomEmEstadoInvalido` |
| `DefinirImpressaoPdv` ✅ (2026-10-15, `pdv.definir_impressao.v1`) | `pdv.impressao.configurar` | Baixo | A impressão do PDV, por empresa (`pdv_preferencias`, migração v3): a bobina (80 ou 58 mm), se o comprovante sai sozinho ao finalizar a venda e o rodapé dele (até 200 caracteres). O DANFCE também sai na bobina configurada | — |
| `FinalizarVenda` ✅ | `pdv.venda.finalizar` | Alto | `F2` — valida soma dos pagamentos = total (`validar_pagamentos`), consome estoque de verdade, monta o lançamento combinado (um débito por forma + desconto + CMV), publica evento. **Não imprime nem emite fiscal** nesta fatia | `SomaDePagamentosDivergente`, `CupomSemItens` |
| `FinalizarVenda` — venda **a prazo** ✅ (2026-10-16) | `pdv.venda.finalizar` | Alto | A forma `Prazo` (fiado, crediário, boleto) com a condição (1º vencimento, 1 a 24 parcelas, intervalo). Exige **cliente identificado**; recusa crédito **bloqueado** e venda que passe do **limite** (`limite − em aberto`, o em aberto vindo de `mod_financeiro::em_aberto_a_receber_do_cliente`; sem limite cadastrado, sem teto). No lançamento da venda, a parte a prazo debita **Clientes a receber** com o cliente na partida, e o título a receber (origem `pdv/<cupom>`) nasce com as parcelas **apontando para esse lançamento** — como o faturamento da OS, sem lançar receita de novo. Uma forma a prazo por venda. O estorno da venda cancela o título se nada foi recebido; com recebimento, pede o estorno da baixa antes | `PrazoSemCliente`, `CreditoBloqueado`, `LimiteDeCreditoExcedido`, `CondicaoDePrazoInvalida`, `PrazoComRecebimento` |
| `AbrirCaixaPdv` | `financeiro.caixa.abrir` | Baixo | Ainda não implementado como comando próprio — o front-end chama `financeiro.abrir_caixa.v1` direto (a única vantagem real de um `AbrirCaixaPdv` seria reservar `FaixaNumeracao` por lote, e esta versão simplificou a faixa para contador contínuo, sem lote) | `CaixaJaAberto` |
| `FecharCaixaPdv` | `financeiro.caixa.fechar` | Médio | Ainda não implementado — o front-end chama `financeiro.fechar_caixa.v1` direto | `VendaEmAndamento` |
| `RegistrarSangriaPdv` | `financeiro.caixa.sangria` | Médio | Ainda não implementado — o front-end chama `financeiro.registrar_sangria.v1` direto | `CaixaFechado` |
| `RecuperarCupomEmAndamento` | (automático no boot do terminal) | Baixo | Ainda não implementado — não há diário durável nesta fatia (ver nota em `src/migracoes.rs`); a recuperação hoje é só reabrir a conexão e ler `pdv_cupom WHERE estado = 'EmAndamento'`, sem a granularidade de sub-transação que o diário promete | `DiarioCorrompido` |

> **Nota (2026-09-06):** as linhas ✅ estão implementadas com teste de integração de ponta a
> ponta (`crates/modulos/mod-pdv/tests/comandos.rs`, `Despachante` real contra SQLite):
> cliente + produto/estoque reais → tabela e regra de preço → caixa cadastrado e aberto
> (`mod-financeiro`) → cupom aberto, item adicionado com preço resolvido, desconto aplicado,
> finalizado com uma ou múltiplas formas de pagamento. Antes de desenhar, `../gestao-raiz` foi
> pesquisado para desconto/múltiplas formas de pagamento/NFC-e — conclusão: o spec deste
> arquivo já era mais rigoroso que a referência nos três eixos (desconto com teto e
> autorização, NFC-e assíncrona, sessão de caixa formal via `financeiro`), então o desenho
> seguiu este documento, não o gestao-raiz. `EstadoCupom` não tem a transição
> `Finalizado → Cancelado` do §4 (correção pós-fechamento no mesmo dia) — cancelar um cupom já
> finalizado é recusado (`CupomJaFinalizado`) nesta fatia.

## 6. Consultas

| Consulta | Permissão | Uso na UI | Índice que a sustenta |
|---|---|---|---|
| `CupomAtual` | `pdv.venda.editar` | Tela de venda | leitura em memória do diário local, não do banco |
| `PrecoDoProduto` | `pdv.preco.consultar` | `F3`/`F10`, item bipado | `PortaCatalogo::buscar_por_codigo` (implementada por `estoque`) |
| `SaldoEstimado` | `pdv.preco.consultar` | Rodapé da tela de venda | `PortaCatalogo::saldo_disponivel`, rotulado "estimado" offline |
| `ComprovanteDaVenda` ✅ (2026-10-15, `pdv.comprovante_da_venda.v1`) | `pdv.venda.ver` | O comprovante de venda (não fiscal): empresa (nome, razão, CNPJ, endereço da configuração fiscal), itens não cancelados com nome e unidade, cliente, pagamentos, estorno e a impressão configurada. Montado no próprio PDV para sair também para o operador de caixa, que não vê os dados da empresa nem o cadastro de clientes | `pdv_cupom(id)` |
| `ImpressaoDoPdv` ✅ (2026-10-15, `pdv.impressao.v1`) | `pdv.venda.editar` | A impressão configurada (padrão: 80 mm, sem comprovante automático) | `pdv_preferencias(empresa)` |
| `VendasDoPeriodo` ✅ (2026-10-15, `pdv.vendas_do_periodo.v1`) | `pdv.venda.ver` | `F11` Vendas de hoje: cupons `Finalizado` e `Estornado` abertos no período (fuso da empresa), com pagamentos e o estorno; até 1.000 | `pdv_cupom(empresa, abertura)` (migração v2) |
| `FaixaAtual` | `pdv.venda.ver` | Faixa de estoque do FaixaEstado (topo da tela) | `pdv_faixa_numeracao(terminal) WHERE proximo <= numero_final` |
| `CurvaDeClientes` ✅ (2026-10-26, `pdv.curva_de_clientes.v1`) | `pdv.clientes.ver` | Clientes › "Curva de clientes": quem comprou quanto nos últimos 12 meses (cupons finalizados não estornados com o cliente identificado + pedidos faturados de `vendas`, por `mod_vendas::vendas_por_cliente`), classe ABC relativa à carteira (80%/95%), o ritmo (intervalo médio entre as compras), a situação (Novo até 60 dias da 1ª compra; Sumido passado o dobro do ritmo e 30 dias; Ativo; Esporádico), o pagamento (`mod_financeiro::pagamento`, com `financeiro.receber.ver`) e o que fazer (cobrar o inadimplente; procurar o A/B sumido). Lição do gestao-raiz: lá as faixas de valor e de dias são fixas, iguais para qualquer empresa, e a pontualidade não entra | `pdv_cupom(empresa, estado, abertura)` + `vendas_pedido` + parcelas a receber |
| `ReceitaPorProduto` ✅ (2026-10-12, `pdv.receita_por_produto.v1`) | `pdv.margem.ver` | Rentabilidade por nível (aba do Estoque): receita por produto dos cupons `Finalizado` abertos no período, sem itens cancelados | `pdv_cupom(empresa, estado, abertura)` + `pdv_item_cupom(cupom)` |

`pdv.receita_por_produto.v1` recebe um `Periodo` e devolve `Vec<ReceitaDoProduto>` (tipo de
`cardeal-analytics`): `total_item` (já com desconto) dos itens não cancelados de cupons `Finalizado`
cuja `abertura` cai no período (dia do fuso da empresa — o cupom ainda não guarda o instante da
finalização). Cupom em andamento ou cancelado não conta.

## 7. Receituário contábil

`FinalizarVenda` é quem monta o `Lancamento` — na mesma transação que a baixa de estoque — porque é
o comando que sabe, no instante da venda, a combinação exata de formas de pagamento. Isto reproduz o
exemplo canônico do [doc 01 §3](../01-arquitetura-geral.md#3-o-caminho-de-um-comando).

| Evento de negócio | Débito | Crédito | Estado |
|---|---|---|---|
| Venda em dinheiro | Caixa (1.1.01) | Receita de vendas (4.1) | Realizado |
| Venda no cartão de débito | Cartões a receber (1.2.02) | Receita de vendas | Realizado |
| Venda no cartão de crédito | Cartões a receber | Receita de vendas | Realizado |
| Venda por Pix | Caixa/Bancos (conforme conciliação) | Receita de vendas | Realizado |
| Venda com múltiplas formas (dinheiro + cartão) | Caixa (parte) + Cartões a receber (parte) | Receita de vendas (total) | Realizado |
| Venda a prazo (fiado/crediário) | Clientes a receber (1.2.01), com o cliente na partida | Receita de vendas | Realizado (o título a receber aponta para este lançamento) |
| Baixa de estoque do cupom | CMV (5.1) | Estoque (1.3.01) | igual ao lançamento acima |
| Desconto aplicado no cupom | Descontos concedidos (4.4) | Receita de vendas | igual ao lançamento acima |
| Cancelamento de cupom antes de finalizar | *(nenhum lançamento — nada foi postado ainda)* | | — |
| Estorno da venda finalizada (`EstornarVenda`) | Receita de vendas + Estoque | Caixa/Valores em trânsito/Cartões a receber + CMV + Descontos (o lançamento da venda espelhado, `Razao::estornar`) | Realizado (estorno) |
| Sangria/suprimento a partir da tela do PDV | Ver `financeiro.md` §7 — mesmo lançamento, só a tecla muda | | |

## 8. Eventos

**Publicados**

- `pdv.venda_finalizada.v1` — `{ cupom, terminal, total, pagamentos }` — assinado por `financeiro`
  (registra `MovimentoCaixa`), por `mod-fiscal` (emite NFC-e) e por `crm`/`clientes` quando ativos.
- `pdv.venda_estornada.v1` — `{ cupom, terminal, total, motivo, autorizado_por }`
- `pdv.cupom_cancelado.v1` — `{ cupom, motivo, autorizado_por }`
- `pdv.item_cancelado.v1` — `{ cupom, item, motivo }`
- `pdv.cupom_recuperado.v1` — `{ cupom, itens, origem: "queda_de_energia" }`
- `pdv.faixa_numeracao_quase_esgotada.v1` — `{ terminal, restantes: 100 }`

**Assinados**

- `estoque.saldo_alterado.v1` → atualiza o cache local de saldo estimado.
- `clientes.credito_bloqueado.v1` → impede venda "na carteira" (a prazo) para o cliente identificado.

## 9. Permissões

| Chave | Descrição | Risco |
|---|---|---|
| `pdv.venda.editar` | Adicionar/alterar item, identificar cliente | Baixo |
| `pdv.venda.finalizar` | Finalizar venda (`F2`) | Médio |
| `pdv.venda.ver` | Consultar cupons do turno | Baixo |
| `pdv.item.cancelar` | Cancelar item antes de finalizar (`F7`) | Baixo |
| `pdv.cupom.cancelar` | Cancelar cupom inteiro (`F8`) | Alto |
| `pdv.venda.estornar` | Estornar venda finalizada (`F11`) | Alto |
| `pdv.impressao.configurar` | Configurar a impressão do PDV (Configurações › Impressão) | Baixo |
| `pdv.desconto.aplicar` | Aplicar desconto no item (`F5`) e no total (`Shift+F5`) | Médio |
| `pdv.preco.consultar` | Consulta de preço sem abrir venda (`F10`) | Baixo |
| `pdv.margem.ver` | Ver a receita por produto vendida no balcão (rentabilidade) | Médio |
| `pdv.clientes.ver` | Ver a curva de clientes — o Operador de Caixa não tem (`nega_prefixo` "pdv.clientes", 2026-10-26) | Médio |
| `pdv.tef.operar` | Iniciar pagamento por TEF | Médio |

`F12` (abrir/fechar caixa) e `F9` (sangria) usam as permissões de `financeiro` diretamente
(`financeiro.caixa.abrir`, `financeiro.caixa.fechar`, `financeiro.caixa.sangria`) — o PDV não
duplica permissão para uma ação que já pertence a outro módulo.

## 10. Telas

### Estado da implementação da tela (2026-09-19)

A tela de `cardeal-desktop` (`tela_pdv/`) segue o mapa de teclas abaixo — **teclado e leitor de
código de barras primeiro**. O que funciona hoje, e o que **não**:

| Tecla | Estado |
|---|---|
| `Enter` no campo de bipe | ✅ Código de barras (GTIN 8/12/13/14) → `estoque.produto_por_codigo_barras.v1`; texto → busca por nome (sem acento), `↑↓` escolhem, `Enter` adiciona |
| `3*789…` / `0,450*pao` | ✅ Prefixo `N*` é a quantidade (no lugar do campo `F4`) |
| `F2` | ✅ Abre o pagamento; dentro dele, finaliza |
| `F3` | ✅ Devolve o foco ao campo de bipe |
| `F5` | ✅ Desconto no item selecionado (`pdv.aplicar_desconto_item.v1`; o backend recusa acima do teto do papel) |
| `Shift+F5` | ✅ Desconto no total: `10%` ou um valor; mostra o que sai e quanto fica antes de confirmar; vazio (ou "Tirar desconto") tira (`pdv.aplicar_desconto_no_total.v1`) |
| `F7` | ✅ Cancela o item selecionado |
| `F8` | ✅ Cancela o cupom com **motivo** e **segunda identidade**: o supervisor informa login e senha, o sistema autentica e confere `pdv.cupom.cancelar`. O comando roda **com a sessão do supervisor** (o operador de caixa em geral não tem a permissão) e `autorizado_por` é ele. Num negócio de um usuário só, o próprio dono se autentica de novo |
| `F9` | ✅ Sangria (`financeiro.registrar_sangria.v1`): valor e motivo obrigatórios |
| `F12` | ✅ Abre o caixa (quando fechado) ou fecha (quando aberto e **sem venda em andamento**). O fechamento é de **contagem cega**: o esperado só aparece depois; o motivo é exigido pelo backend só se a quebra passar de R$ 5,00, então o campo fica sempre à vista |
| `F10` | ✅ Consulta de preço sem abrir venda (`pdv.preco_do_produto.v1`, permissão `pdv.preco.consultar`): bipa ou digita, mostra nome, código, preço vigente e saldo; continua aberto para a próxima consulta. O preço vem da **mesma função pura** que `AdicionarItem` usa, então nunca diverge do que a venda cobra |
| `F6` | ✅ Identifica o cliente (nome ou documento; `↑↓` e `Enter`; "Sem cliente" desfaz). Antes do primeiro item só guarda e vai em `AbrirCupom`; com o cupom aberto usa `pdv.identificar_cliente.v1`, que só age em cupom em andamento. Filtra a carteira inteira carregada na tela (a consulta sem busca traz até 50 mil): por palavras no nome, sem caixa nem acento; **só números** comparam os dígitos do documento e do telefone, com ou sem máscara. **"Cadastrar cliente"** (com `clientes.pessoa.criar`) abre o cadastro rápido já com o que foi digitado (nome, CPF/CNPJ válido ou telefone), avisa "já cadastrado com esse telefone" e "Cadastrar e usar" identifica o cliente na venda (2026-10-16) |
| Comprovante | ✅ (2026-10-15) **Imprimir comprovante** no painel da última venda (com o troco) e em Vendas de hoje (2ª via, sem o troco — a reimpressão não sabe quanto o cliente deu); sai sozinho ao finalizar quando Configurações › Impressão manda. Recibo **não fiscal** (`cardeal_pdf::gerar_comprovante`), em 80 ou 58 mm, com "NÃO É DOCUMENTO FISCAL", o rodapé configurado e a marca de venda estornada. **Impressora deste computador** (2026-10-27): em Configurações › Impressão, o PDF no visualizador (o padrão), uma impressora **instalada** (o spooler do Windows em modo cru; `lp -o raw` fora dele) ou uma **de rede** (IP, porta 9100), com "Imprimir teste" e acento opcional — o comprovante e o DANFCE vão em ESC/POS (`cardeal_pdf::comprovante_escpos`/`danfce_escpos`). A preferência é da **máquina** (`maquina.json` na pasta de dados), não da empresa: cada caixa tem a sua. O "sai sozinho" com NFC-e imprime o **cupom fiscal** quando a nota sai (autorizada ou em contingência), e não mais o comprovante — sem NFC-e (ou rejeitada na montagem), o comprovante. Lição do gestao-raiz: lá a impressão é sempre pelo navegador (`window.print()` com o tamanho da bobina no CSS), com a mesma preferência de imprimir sozinho |
| `F11` | ✅ **Vendas de hoje** (2026-10-15): grade com hora, número, cliente, formas, total, situação da venda e da NFC-e. **Estornar venda** pede motivo e login/senha de um supervisor com `pdv.venda.estornar` (segunda identidade, como o `F8`); depois do estorno o PDV pede ao fiscal a anulação da NFC-e e diz o que devolver (dinheiro pelo caixa; Pix/cartão pelo banco ou maquininha). Também **Imprimir cupom fiscal** e **Anular NFC-e** (quando a anulação não pôde ser pedida na hora). A NFC-e com cancelamento recusado aparece como "cancelamento recusado", com a orientação de emitir uma devolução. A venda selecionada mostra o **prazo de cancelamento** da NFC-e (30 min da autorização): dentro, quanto falta; fora, o aviso de que o estorno desfaz a venda mas a SEFAZ pode recusar a nota (não bloqueia; 2026-10-20) |
| `F4` | ➖ Não existe de propósito: a quantidade se digita no próprio campo de bipe com o prefixo `N*` |

Pagamento: `1..5` escolhem a forma (dinheiro, Pix, débito, crédito, **a prazo** — esta pede o 1º vencimento e as parcelas, a cada 30 dias, e o cliente identificado com `F6`), `Enter` confirma o valor (vazio =
"o que falta"), `F2` finaliza, `Esc` volta. Os dígitos escolhem a forma **só enquanto o campo de valor
não tem foco**; ao digitar um valor, valem como número. Só o dinheiro dá troco, e o troco aparece
grande depois da venda.

Diferença em relação ao mockup abaixo: o campo de bipe fica **no topo** da lista (não embaixo da
tabela), para nunca sair da tela. A coluna **Código** vem de `ProdutosComSaldo.codigo_barras`.
Testes: `cargo test -p cardeal-desktop` cobre a
interpretação do bipe, a conta do troco e o fluxo de teclas do pagamento com eventos reais do egui.

### Venda

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│ 🟢 Fiscal em dia          Caixa 1 · Carlos · aberto 08:02        Cupom #4087 (série 1) │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  Código        Produto                    Qtd      Unit.       Desc.      Total       │
│  7891234567... Refrigerante Cola 2L         2      6,90 UN               13,80        │
│  7891234568... Pão Francês (balança)      0,450    18,90 KG              8,51         │
│  7891234569... Achocolatado 400g            1      9,50 UN      5%       9,03         │
│                                                                                        │
│  ▸ [bipe o próximo item ou digite o código_______________]                            │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  Cliente: — não identificado (F6)                            Itens: 3    Qtd: 3,45    │
│                                                                     TOTAL: R$ 31,34    │
├───────────────────────────────────────────────────────────────────────────────────────┤
│ F2 Finalizar F3 Buscar F4 Qtd F5 Desconto F6 Cliente F7 Canc.item F8 Canc.cupom       │
│ F9 Sangria  F10 Consulta preço  F12 Abrir/Fechar caixa                                │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

### Pagamento (múltiplas formas)

```
┌───────────────────────────────────────────────────────────────────────────────────────┐
│  Pagamento · Cupom #4087                                          Total: R$ 31,34    │
├───────────────────────────────────────────────────────────────────────────────────────┤
│  Forma                    Valor            Status                                     │
│  [1] Dinheiro             R$ 20,00         ✓ recebido                                 │
│  [2] Cartão débito        R$ 11,34         ⏳ aguardando TEF...                       │
│                                                                                        │
│  Restante: R$ 0,00                                                                    │
│                                                                                        │
│  [1..9 escolhe a forma]   [Enter confirma o valor]   [F2 Finalizar]   [Esc Cancelar]  │
└───────────────────────────────────────────────────────────────────────────────────────┘
```

Se o valor em dinheiro exceder o total, o troco é calculado e mostrado automaticamente — nunca exige
conta de cabeça do operador nem do cliente.

### Recuperação de venda após queda de energia

```
┌─────────────────────────────────────────────────────────┐
│  Venda em andamento recuperada                            │
│                                                             │
│  7 itens, R$ 84,10 — última atualização há 8 segundos      │
│                                                             │
│              [Continuar a venda]  [Cancelar (F8, supervisor)]│
└─────────────────────────────────────────────────────────┘
```

### Cancelar cupom (supervisor)

```
┌─────────────────────────────────────────────────────────┐
│  Cancelar cupom #4087 — R$ 31,34                          │
│  Esta ação exige autorização de supervisor.                │
│                                                             │
│  Motivo:     [Cliente desistiu da compra_______________]  │
│  Supervisor: [________________]  Senha: [••••••••]        │
│                              [F2 Confirmar]  [Esc Voltar]  │
└─────────────────────────────────────────────────────────┘
```

### Sangria rápida

```
┌───────────────────────────────────────────┐
│  Sangria — Caixa 1                          │
│  Valor: [R$ ______]  Motivo: [___________]  │
│                    [F2 Confirmar] [Esc]     │
└───────────────────────────────────────────┘
```

## 11. Regras de negócio críticas

1. **O diário do carrinho grava a cada mutação, com `fsync`, antes de qualquer tela avançar.** Item
   bipado, quantidade alterada, desconto aplicado: cada evento é `append` num arquivo por terminal,
   com CRC32 por registro. Corte de energia perde no máximo o último evento não confirmado; o
   restante do carrinho volta inteiro (ver [doc 03 §2.1](../03-pilar-resiliencia.md#21-a-cadeia-de-durabilidade-de-uma-venda)).
2. **`F2` é o único caminho para fechar uma venda.** Não existe finalização automática por tempo
   ocioso nem por perda de foco — o operador decide o momento.
3. **Cancelar item é livre; cancelar cupom inteiro nunca é.** `F7` (item) usa a permissão do próprio
   operador; `F8` (cupom) sempre exige segunda identidade — senha ou crachá NFC de supervisor,
   registrada vinculando as duas pessoas.
4. **Cada terminal vende dentro da sua própria faixa de numeração.** Nunca dois terminais emitem o
   mesmo número na mesma série; a faixa é reservada na abertura de caixa e a UI avisa com 100 números
   de antecedência antes de esgotar (ver [doc 03 §3.2](../03-pilar-resiliencia.md#32-numeração-sem-conflito)).
5. **A soma dos pagamentos tem que fechar exatamente com o total antes de `F2` aceitar.** Não existe
   "arredondar e ajustar depois" — divergência de um centavo bloqueia a finalização com mensagem clara.
6. **O cupom operacional imprime antes do documento fiscal ser autorizado.** A venda nunca espera a
   SEFAZ — meta de produto de < 400 ms entre `F2` e cupom impresso
   ([doc 00 §6](../00-visao-produto.md#6-métricas-de-sucesso-do-produto)).
7. **Pagamento por TEF nunca finaliza a venda sozinho.** `IniciarPagamentoTef` só marca a forma como
   "aguardando"; `FinalizarVenda` só aceita quando todas as formas estão confirmadas ou o operador
   as removeu.
8. **Desconto acima do limite pré-autorizado em modo autônomo não é permitido**, mesmo com
   supervisor local — porque o teto de autorização em modo autônomo é fixo, sincronizado na abertura
   de caixa, e não pode ser ampliado sem o servidor (ver [doc 03 §3.1](../03-pilar-resiliencia.md#31-como-funciona)).
9. **Preço e saldo mostrados offline são sempre rotulados como estimados.** A UI nunca apresenta um
   número desatualizado como se fosse atual — transparência é parte do contrato de confiança do Pilar II.

## 12. Comportamento offline

**Funciona em modo autônomo:**
- Vender à vista e a prazo dentro do limite pré-autorizado.
- Consultar preço e saldo do último sincronismo (marcado como estimado).
- Abrir e fechar caixa, sangria, suprimento.
- Emitir NFC-e em contingência offline (`tpEmis=9`) e imprimir normalmente.
- Recuperar venda em andamento a partir do diário durável local.

**Não funciona em modo autônomo:**
- Alterar cadastro de produto, cliente ou tabela de preço.
- Conceder desconto acima do limite pré-autorizado.
- Consultar posição consolidada de outras lojas/filiais.
- Fechar o dia contábil (fechamento de período é ação do servidor).

**Política de conflito na reconciliação:** idêntica à política geral do produto — o físico ganha do
banco de dados. Se o preço mudou no servidor durante a janela offline, vale o praticado no cupom
impresso; se o estoque ficou negativo por uma venda offline, a venda é aceita e o sistema gera
`MovimentoDivergente` para conferência; se dois cupons de terminais diferentes tentam reservar o
mesmo número na reconexão, a idempotência por chave resolve — cada cupom já nasceu com número da
própria faixa, então esse conflito específico é estruturalmente impossível (ver
[doc 03 §3.2–3.3](../03-pilar-resiliencia.md#32-numeração-sem-conflito)).

## 13. Tabelas

> **Nota (2026-09-06):** este é o esquema-alvo completo. A fatia implementada
> (`crates/modulos/mod-pdv/src/migracoes.rs`) grava `tabela_preco`/`local_expedicao` em
> `pdv_cupom` (necessários para `AdicionarItem` resolver preço e para `FinalizarVenda`
> consumir estoque — o esquema-alvo original não os tinha, porque assumia que viriam de fora),
> mas **não** tem `entrega_endereco`/`entrega_estado` (submódulo `delivery`, não essencial,
> sem consumidor) nem `documento_fiscal` (emissão de NFC-e é `mod-fiscal`, que ainda não
> existe implementado). `pdv_faixa_numeracao` também simplifica a reserva por lote para um
> contador contínuo por `(empresa, terminal, serie_fiscal)` — `UNIQUE` nesses três campos, sem
> `reservada_em` na chave — ver a nota em `RepositorioPdv::proximo_numero_cupom`.

```sql
CREATE TABLE pdv_faixa_numeracao (
    id              BLOB PRIMARY KEY,
    empresa         BLOB    NOT NULL,
    terminal        BLOB    NOT NULL,
    serie_fiscal    INTEGER NOT NULL,
    numero_inicial  INTEGER NOT NULL,
    numero_final    INTEGER NOT NULL,
    proximo         INTEGER NOT NULL,
    reservada_em    INTEGER NOT NULL,
    UNIQUE (empresa, terminal, serie_fiscal, reservada_em)
) STRICT;
CREATE INDEX pdv_faixa_terminal_ativa ON pdv_faixa_numeracao(terminal) WHERE proximo <= numero_final;

CREATE TABLE pdv_cupom (
    id                 BLOB PRIMARY KEY,
    empresa            BLOB    NOT NULL,
    sessao_caixa       BLOB    NOT NULL,
    terminal           BLOB    NOT NULL,
    numero_terminal    INTEGER NOT NULL,
    serie_fiscal       INTEGER NOT NULL,
    cliente            BLOB,
    operador           BLOB    NOT NULL,
    abertura           INTEGER NOT NULL,
    finalizado_em      INTEGER,
    subtotal           INTEGER NOT NULL DEFAULT 0,
    desconto           INTEGER NOT NULL DEFAULT 0,
    total              INTEGER NOT NULL DEFAULT 0,
    estado             TEXT    NOT NULL CHECK (estado IN ('EmAndamento','Finalizado','Cancelado')),
    entrega_endereco   TEXT,
    entrega_estado     TEXT    CHECK (entrega_estado IN ('EmPreparo','EmRota','Entregue')),
    documento_fiscal   BLOB,
    versao             INTEGER NOT NULL DEFAULT 1,
    UNIQUE (empresa, terminal, serie_fiscal, numero_terminal)
) STRICT;
CREATE INDEX pdv_cupom_sessao ON pdv_cupom(sessao_caixa, abertura);
CREATE INDEX pdv_cupom_em_andamento ON pdv_cupom(terminal) WHERE estado = 'EmAndamento';

CREATE TABLE pdv_item_cupom (
    id                   BLOB PRIMARY KEY,
    cupom                BLOB    NOT NULL REFERENCES pdv_cupom(id),
    produto              BLOB    NOT NULL,
    variacao             BLOB    NOT NULL DEFAULT (X''),
    quantidade           INTEGER NOT NULL,   -- Quantidade, escala 1e-4
    preco_unitario       INTEGER NOT NULL,   -- Preco, escala 1e-6
    desconto_percentual  INTEGER NOT NULL DEFAULT 0,
    total_item           INTEGER NOT NULL,
    cancelado            INTEGER NOT NULL DEFAULT 0 CHECK (cancelado IN (0,1)),
    motivo_cancelamento  TEXT,
    autorizado_por       BLOB
) STRICT;
CREATE INDEX pdv_item_cupom_cupom ON pdv_item_cupom(cupom);

CREATE TABLE pdv_pagamento_cupom (
    id                BLOB PRIMARY KEY,
    cupom             BLOB    NOT NULL REFERENCES pdv_cupom(id),
    forma_pagamento   BLOB    NOT NULL,
    valor             INTEGER NOT NULL,
    autorizacao_tef   TEXT,
    bandeira          TEXT,
    parcelas_cartao   INTEGER
) STRICT;
CREATE INDEX pdv_pagamento_cupom_cupom ON pdv_pagamento_cupom(cupom);
```

> O **diário durável do carrinho** não é uma tabela SQL: é um arquivo append-only por terminal, com
> CRC32 por registro, gravado antes de qualquer efeito ser considerado confirmado. Formato e política
> de recuperação em [doc 03 §2.1](../03-pilar-resiliencia.md#21-a-cadeia-de-durabilidade-de-uma-venda).
