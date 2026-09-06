//! Orçamento, pedido e item — o fluxo comercial `orçamento → pedido → faturamento`.
//!
//! `docs/modulos/vendas.md` §3, §4 e §11. Domínio puro. Regras modeladas aqui:
//! preço **congelado no item** (§11.1), reserva só na confirmação (§11.2 — a reserva em si
//! é comando de `estoque`; aqui só a transição), **faturar é irreversível** (§11.3),
//! **desconto acima do limite do papel exige autorização na hora** (§11.4).

// `Orcamento::novo`/`Pedido::novo` levam cliente + vendedor + datas + condição + tabela +
// local — agrupar isso em struct só moveria a verbosidade para o call site.
#![allow(clippy::too_many_arguments)]

use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Percentual, Quantidade, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroVendas;

/// O estado de um [`Orcamento`] (`docs/modulos/vendas.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoOrcamento {
    /// Em edição / aguardando o cliente.
    Aberto,
    /// Cliente aceitou.
    Aprovado,
    /// Validade vencida.
    Expirado,
    /// Virou pedido.
    Convertido,
    /// Cancelado.
    Cancelado,
}

/// O estado de um [`Pedido`] (`docs/modulos/vendas.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoPedido {
    /// Em montagem; não compromete estoque.
    Rascunho,
    /// Confirmado; estoque reservado.
    Confirmado,
    /// Faturado; razão, estoque e comissão lançados. Irreversível.
    Faturado,
    /// Em rota de entrega.
    EmEntrega,
    /// Concluído.
    Concluido,
    /// Cancelado (só antes de faturar).
    Cancelado,
}

impl EstadoPedido {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Rascunho => "Rascunho",
            Self::Confirmado => "Confirmado",
            Self::Faturado => "Faturado",
            Self::EmEntrega => "EmEntrega",
            Self::Concluido => "Concluido",
            Self::Cancelado => "Cancelado",
        }
    }
}

/// Uma linha de orçamento ou pedido, com preço e desconto já congelados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemVenda {
    /// Identidade.
    pub id: Id,
    /// O produto.
    pub produto: Id,
    /// A variação de grade.
    pub variacao: Option<Id>,
    /// A quantidade (> 0).
    pub quantidade: Quantidade,
    /// O preço unitário resolvido no momento da adição — **congelado** (§11.1).
    pub preco_unitario: cardeal_kernel::Preco,
    /// O desconto em pontos percentuais.
    pub desconto_percentual: Percentual,
    /// O valor do desconto, arredondado por item.
    pub desconto_valor: Dinheiro,
    /// `quantidade × preco_unitario − desconto_valor`.
    pub total_item: Dinheiro,
    /// A reserva de estoque — no desenho do domínio, preenchida ao confirmar o pedido
    /// (§11.2). **Hoje sempre `None`**: `ConfirmarPedido` só faz a transição de estado; a
    /// reserva de verdade depende de `ReservarEstoque`/`LimiteDisponivel`, que ainda não
    /// existem em `mod-estoque`/`mod-clientes` (ver `docs/modulos/vendas.md` §5).
    pub reserva: Option<Id>,
}

impl ItemVenda {
    /// Monta um item calculando desconto e total, recusando desconto acima do teto do papel.
    ///
    /// # Errors
    /// [`ErroVendas::QuantidadeInvalida`], [`ErroVendas::DescontoAcimaDoLimite`].
    pub fn novo(
        produto: Id,
        variacao: Option<Id>,
        quantidade: Quantidade,
        preco_unitario: cardeal_kernel::Preco,
        desconto_percentual: Percentual,
        limite_desconto: Percentual,
    ) -> Result<Self, ErroVendas> {
        if !quantidade.e_positiva() {
            return Err(ErroVendas::QuantidadeInvalida);
        }
        if desconto_percentual > limite_desconto {
            return Err(ErroVendas::DescontoAcimaDoLimite {
                pedido: desconto_percentual,
                limite: limite_desconto,
            });
        }
        let total_bruto = Dinheiro::de_total(quantidade, preco_unitario, Arredondamento::MeioAcima);
        let desconto_valor = total_bruto.aplicar(desconto_percentual, Arredondamento::MeioAcima);
        Ok(Self {
            id: Id::novo(),
            produto,
            variacao,
            quantidade,
            preco_unitario,
            desconto_percentual,
            desconto_valor,
            total_item: total_bruto - desconto_valor,
            reserva: None,
        })
    }

    /// O total bruto antes do desconto.
    #[must_use]
    pub fn total_bruto(&self) -> Dinheiro {
        self.total_item + self.desconto_valor
    }
}

fn soma_itens(itens: &[ItemVenda]) -> (Dinheiro, Dinheiro) {
    let desconto = itens.iter().map(|i| i.desconto_valor).sum();
    let total = itens.iter().map(|i| i.total_item).sum();
    (desconto, total)
}

/// Um orçamento.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Orcamento {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O cliente.
    pub cliente: Id,
    /// O vendedor.
    pub vendedor: Id,
    /// Data de emissão.
    pub data: Data,
    /// Validade — depois dela o orçamento expira sozinho.
    pub validade: Data,
    /// A condição de pagamento (`financeiro_condicao_pagamento`).
    pub condicao_pagamento: Id,
    /// A tabela de preço usada.
    pub tabela_preco: Id,
    /// Os itens.
    pub itens: Vec<ItemVenda>,
    /// Soma dos descontos de item + cabeçalho.
    pub desconto_total: Dinheiro,
    /// O estado.
    pub estado: EstadoOrcamento,
    /// Observação livre.
    pub observacao: Option<String>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Orcamento {
    /// Cria um orçamento aberto com os itens dados.
    #[must_use]
    pub fn novo(
        empresa: Id,
        cliente: Id,
        vendedor: Id,
        data: Data,
        validade: Data,
        condicao_pagamento: Id,
        tabela_preco: Id,
        itens: Vec<ItemVenda>,
    ) -> Self {
        let (desconto_total, _) = soma_itens(&itens);
        Self {
            id: Id::novo(),
            empresa,
            cliente,
            vendedor,
            data,
            validade,
            condicao_pagamento,
            tabela_preco,
            itens,
            desconto_total,
            estado: EstadoOrcamento::Aberto,
            observacao: None,
            versao: Versao::INICIAL,
        }
    }

    /// O total líquido do orçamento.
    #[must_use]
    pub fn total(&self) -> Dinheiro {
        soma_itens(&self.itens).1
    }

    /// Verdadeiro se a validade já passou na data dada.
    #[must_use]
    pub fn expirado_em(&self, hoje: Data) -> bool {
        self.validade < hoje
    }

    /// `Aberto → Aprovado`.
    ///
    /// # Errors
    /// [`ErroVendas::OrcamentoEmEstadoInvalido`], [`ErroVendas::OrcamentoExpirado`].
    pub fn aprovar(&mut self, hoje: Data) -> Result<(), ErroVendas> {
        self.exigir(EstadoOrcamento::Aberto, "Aberto")?;
        if self.expirado_em(hoje) {
            return Err(ErroVendas::OrcamentoExpirado(self.validade.formatar()));
        }
        self.estado = EstadoOrcamento::Aprovado;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Marca como expirado se a validade venceu (verificação diária). No-op caso contrário.
    pub fn expirar_se_vencido(&mut self, hoje: Data) {
        if self.estado == EstadoOrcamento::Aberto && self.expirado_em(hoje) {
            self.estado = EstadoOrcamento::Expirado;
            self.versao = self.versao.proxima();
        }
    }

    /// Cancela o orçamento (a partir de `Aberto` ou `Aprovado`).
    ///
    /// # Errors
    /// [`ErroVendas::OrcamentoEmEstadoInvalido`].
    pub fn cancelar(&mut self) -> Result<(), ErroVendas> {
        if !matches!(
            self.estado,
            EstadoOrcamento::Aberto | EstadoOrcamento::Aprovado
        ) {
            return Err(ErroVendas::OrcamentoEmEstadoInvalido {
                atual: self.rotulo_estado(),
                esperado: "Aberto/Aprovado",
            });
        }
        self.estado = EstadoOrcamento::Cancelado;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Converte o orçamento num pedido `Rascunho`, copiando os itens (com o preço já
    /// congelado). Marca o orçamento como `Convertido`.
    ///
    /// # Errors
    /// [`ErroVendas::OrcamentoEmEstadoInvalido`] se não está em `Aberto`/`Aprovado`;
    /// [`ErroVendas::OrcamentoExpirado`], [`ErroVendas::PedidoSemItens`].
    pub fn converter_em_pedido(
        &mut self,
        hoje: Data,
        local_expedicao: Id,
        criado_por: Id,
    ) -> Result<Pedido, ErroVendas> {
        if !matches!(
            self.estado,
            EstadoOrcamento::Aberto | EstadoOrcamento::Aprovado
        ) {
            return Err(ErroVendas::OrcamentoEmEstadoInvalido {
                atual: self.rotulo_estado(),
                esperado: "Aberto/Aprovado",
            });
        }
        if self.expirado_em(hoje) {
            return Err(ErroVendas::OrcamentoExpirado(self.validade.formatar()));
        }
        if self.itens.is_empty() {
            return Err(ErroVendas::PedidoSemItens);
        }
        let itens: Vec<ItemVenda> = self
            .itens
            .iter()
            .map(|i| ItemVenda {
                id: Id::novo(),
                reserva: None,
                ..i.clone()
            })
            .collect();
        let mut pedido = Pedido::novo(
            self.empresa,
            self.cliente,
            self.vendedor,
            hoje,
            self.condicao_pagamento,
            self.tabela_preco,
            local_expedicao,
            criado_por,
        );
        pedido.orcamento_origem = Some(self.id);
        for item in itens {
            pedido.itens.push(item);
        }
        pedido.recalcular();
        self.estado = EstadoOrcamento::Convertido;
        self.versao = self.versao.proxima();
        Ok(pedido)
    }

    fn rotulo_estado(&self) -> &'static str {
        match self.estado {
            EstadoOrcamento::Aberto => "Aberto",
            EstadoOrcamento::Aprovado => "Aprovado",
            EstadoOrcamento::Expirado => "Expirado",
            EstadoOrcamento::Convertido => "Convertido",
            EstadoOrcamento::Cancelado => "Cancelado",
        }
    }

    fn exigir(&self, estado: EstadoOrcamento, rotulo: &'static str) -> Result<(), ErroVendas> {
        if self.estado == estado {
            Ok(())
        } else {
            Err(ErroVendas::OrcamentoEmEstadoInvalido {
                atual: self.rotulo_estado(),
                esperado: rotulo,
            })
        }
    }
}

/// Um pedido de venda.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pedido {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O orçamento de origem, se convertido.
    pub orcamento_origem: Option<Id>,
    /// O cliente.
    pub cliente: Id,
    /// O vendedor.
    pub vendedor: Id,
    /// Data.
    pub data: Data,
    /// A condição de pagamento.
    pub condicao_pagamento: Id,
    /// A tabela de preço.
    pub tabela_preco: Id,
    /// Centro de custo (requer o submódulo `financeiro.centro_custo`).
    pub centro_custo: Option<Id>,
    /// O local de expedição (`estoque_local`).
    pub local_expedicao: Id,
    /// Os itens.
    pub itens: Vec<ItemVenda>,
    /// Soma dos descontos.
    pub desconto_total: Dinheiro,
    /// Total líquido.
    pub total: Dinheiro,
    /// O estado.
    pub estado: EstadoPedido,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
    /// Quando foi criado.
    pub criado_por: Id,
}

impl Pedido {
    /// Cria um pedido `Rascunho` sem itens.
    #[must_use]
    pub fn novo(
        empresa: Id,
        cliente: Id,
        vendedor: Id,
        data: Data,
        condicao_pagamento: Id,
        tabela_preco: Id,
        local_expedicao: Id,
        criado_por: Id,
    ) -> Self {
        Self {
            id: Id::novo(),
            empresa,
            orcamento_origem: None,
            cliente,
            vendedor,
            data,
            condicao_pagamento,
            tabela_preco,
            centro_custo: None,
            local_expedicao,
            itens: Vec::new(),
            desconto_total: Dinheiro::ZERO,
            total: Dinheiro::ZERO,
            estado: EstadoPedido::Rascunho,
            versao: Versao::INICIAL,
            criado_por,
        }
    }

    /// Acrescenta um item. Só em `Rascunho`.
    ///
    /// # Errors
    /// [`ErroVendas::PedidoEmEstadoInvalido`].
    pub fn adicionar_item(&mut self, item: ItemVenda) -> Result<(), ErroVendas> {
        self.exigir(EstadoPedido::Rascunho)?;
        self.itens.push(item);
        self.recalcular();
        Ok(())
    }

    /// Remove um item pelo id. Só em `Rascunho`.
    ///
    /// # Errors
    /// [`ErroVendas::PedidoEmEstadoInvalido`].
    pub fn remover_item(&mut self, item: Id) -> Result<(), ErroVendas> {
        self.exigir(EstadoPedido::Rascunho)?;
        self.itens.retain(|i| i.id != item);
        self.recalcular();
        Ok(())
    }

    /// `Rascunho → Confirmado`. A reserva de estoque item a item é do comando; aqui só a
    /// transição, que exige ao menos um item.
    ///
    /// # Errors
    /// [`ErroVendas::PedidoEmEstadoInvalido`], [`ErroVendas::PedidoSemItens`].
    pub fn confirmar(&mut self) -> Result<(), ErroVendas> {
        self.exigir(EstadoPedido::Rascunho)?;
        if self.itens.is_empty() {
            return Err(ErroVendas::PedidoSemItens);
        }
        self.transicionar(EstadoPedido::Confirmado);
        Ok(())
    }

    /// `Confirmado → Faturado` — **irreversível** (§11.3).
    ///
    /// # Errors
    /// [`ErroVendas::PedidoEmEstadoInvalido`].
    pub fn faturar(&mut self) -> Result<(), ErroVendas> {
        self.exigir(EstadoPedido::Confirmado)?;
        self.transicionar(EstadoPedido::Faturado);
        Ok(())
    }

    /// `Faturado → EmEntrega` (submódulo `entrega`).
    ///
    /// # Errors
    /// [`ErroVendas::PedidoEmEstadoInvalido`].
    pub fn registrar_entrega(&mut self) -> Result<(), ErroVendas> {
        self.exigir(EstadoPedido::Faturado)?;
        self.transicionar(EstadoPedido::EmEntrega);
        Ok(())
    }

    /// `Faturado`/`EmEntrega → Concluido`.
    ///
    /// # Errors
    /// [`ErroVendas::PedidoEmEstadoInvalido`].
    pub fn concluir(&mut self) -> Result<(), ErroVendas> {
        if !matches!(
            self.estado,
            EstadoPedido::Faturado | EstadoPedido::EmEntrega
        ) {
            return Err(ErroVendas::PedidoEmEstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Faturado/EmEntrega",
            });
        }
        self.transicionar(EstadoPedido::Concluido);
        Ok(())
    }

    /// Cancela um pedido ainda não faturado (libera a reserva — feito pelo comando).
    ///
    /// # Errors
    /// [`ErroVendas::PedidoJaFaturado`] a partir de `Faturado`;
    /// [`ErroVendas::PedidoEmEstadoInvalido`] de `Concluido`/`Cancelado`.
    pub fn cancelar(&mut self) -> Result<(), ErroVendas> {
        match self.estado {
            EstadoPedido::Rascunho | EstadoPedido::Confirmado => {
                self.transicionar(EstadoPedido::Cancelado);
                Ok(())
            }
            EstadoPedido::Faturado | EstadoPedido::EmEntrega => Err(ErroVendas::PedidoJaFaturado),
            _ => Err(ErroVendas::PedidoEmEstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Rascunho/Confirmado",
            }),
        }
    }

    /// Recalcula `desconto_total` e `total` a partir dos itens.
    pub fn recalcular(&mut self) {
        let (desconto, total) = soma_itens(&self.itens);
        self.desconto_total = desconto;
        self.total = total;
    }

    fn transicionar(&mut self, novo: EstadoPedido) {
        self.estado = novo;
        self.versao = self.versao.proxima();
    }

    fn exigir(&self, estado: EstadoPedido) -> Result<(), ErroVendas> {
        if self.estado == estado {
            Ok(())
        } else {
            Err(ErroVendas::PedidoEmEstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: estado.rotulo(),
            })
        }
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Fuso, Preco};

    use super::*;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn item(qtd: i64, preco: i64, desc_pct: i64) -> ItemVenda {
        ItemVenda::novo(
            Id::novo(),
            None,
            Quantidade::unidades(qtd),
            Preco::reais(preco),
            Percentual::pontos(desc_pct),
            Percentual::pontos(20),
        )
        .unwrap()
    }

    #[test]
    fn item_calcula_desconto_e_total() {
        let i = item(120, 5, 5); // 120 * 5,00 = 600,00 ; 5% = 30,00
        assert_eq!(i.total_bruto(), Dinheiro::reais(600));
        assert_eq!(i.desconto_valor, Dinheiro::reais(30));
        assert_eq!(i.total_item, Dinheiro::reais(570));
    }

    #[test]
    fn desconto_acima_do_limite_do_papel_e_recusado_na_hora() {
        let erro = ItemVenda::novo(
            Id::novo(),
            None,
            Quantidade::unidades(1),
            Preco::reais(10),
            Percentual::pontos(12),
            Percentual::pontos(5),
        )
        .unwrap_err();
        assert!(matches!(erro, ErroVendas::DescontoAcimaDoLimite { .. }));
    }

    #[test]
    fn pedido_soma_os_itens_e_congela_o_preco() {
        let mut p = Pedido::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            hoje(),
            Id::novo(),
            Id::novo(),
            Id::novo(),
            Id::novo(),
        );
        assert!(p.confirmar().is_err()); // sem itens
        p.adicionar_item(item(120, 5, 5)).unwrap();
        p.adicionar_item(item(60, 5, 5)).unwrap();
        assert_eq!(p.total, Dinheiro::reais(855)); // 570 + 285
        assert_eq!(p.desconto_total, Dinheiro::reais(45));
        p.confirmar().unwrap();
        assert!(p.adicionar_item(item(1, 1, 0)).is_err()); // não Rascunho
    }

    #[test]
    fn faturar_e_irreversivel() {
        let mut p = Pedido::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            hoje(),
            Id::novo(),
            Id::novo(),
            Id::novo(),
            Id::novo(),
        );
        p.adicionar_item(item(1, 10, 0)).unwrap();
        p.confirmar().unwrap();
        p.faturar().unwrap();
        assert_eq!(p.cancelar().unwrap_err(), ErroVendas::PedidoJaFaturado);
        p.concluir().unwrap();
        assert_eq!(p.estado, EstadoPedido::Concluido);
    }

    #[test]
    fn orcamento_aprova_converte_e_expira() {
        let mut orc = Orcamento::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            hoje(),
            hoje().mais_dias(7),
            Id::novo(),
            Id::novo(),
            vec![item(10, 5, 0)],
        );
        orc.aprovar(hoje()).unwrap();
        let pedido = orc
            .converter_em_pedido(hoje(), Id::novo(), Id::novo())
            .unwrap();
        assert_eq!(orc.estado, EstadoOrcamento::Convertido);
        assert_eq!(pedido.estado, EstadoPedido::Rascunho);
        assert_eq!(pedido.orcamento_origem, Some(orc.id));
        assert_eq!(pedido.total, Dinheiro::reais(50));

        // Um orçamento já convertido não converte de novo.
        assert!(orc
            .converter_em_pedido(hoje(), Id::novo(), Id::novo())
            .is_err());

        // Orçamento vencido expira e não aprova.
        let mut vencido = Orcamento::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            hoje().mais_dias(-30),
            hoje().mais_dias(-1),
            Id::novo(),
            Id::novo(),
            vec![item(1, 1, 0)],
        );
        assert!(matches!(
            vencido.aprovar(hoje()).unwrap_err(),
            ErroVendas::OrcamentoExpirado(_)
        ));
        vencido.expirar_se_vencido(hoje());
        assert_eq!(vencido.estado, EstadoOrcamento::Expirado);
    }
}
