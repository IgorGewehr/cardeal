//! Devolução total e parcial, com rateio que não perde centavo.
//!
//! `docs/modulos/vendas.md` §4 e §11.5: a devolução parcial **rateia proporcionalmente** e
//! a soma dos itens devolvidos bate exatamente com `valor_total`. Domínio puro.

use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Quantidade, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroVendas;
use crate::pedido::{ItemVenda, Pedido};

/// Se a devolução cobre todos os itens (e quantidades) do pedido ou não.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoDevolucao {
    /// Devolução do pedido inteiro.
    Total,
    /// Devolução de parte dos itens/quantidades.
    Parcial,
}

/// O estado de uma [`Devolucao`] (`docs/modulos/vendas.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoDevolucao {
    /// Registrada, aguardando aprovação.
    Solicitada,
    /// Aprovada; pode ser concluída.
    Aprovada,
    /// Concluída — razão estornado, estoque reposto.
    Concluida,
    /// Cancelada.
    Cancelada,
}

impl EstadoDevolucao {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Solicitada => "Solicitada",
            Self::Aprovada => "Aprovada",
            Self::Concluida => "Concluida",
            Self::Cancelada => "Cancelada",
        }
    }
}

/// Um item devolvido, com o valor estornado já rateado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDevolvido {
    /// O item de pedido de origem.
    pub item_pedido: Id,
    /// O produto.
    pub produto: Id,
    /// A variação.
    pub variacao: Option<Id>,
    /// A quantidade devolvida (≤ comprada).
    pub quantidade: Quantidade,
    /// O valor a estornar deste item (proporcional ao `total_item` original).
    pub valor_estornado: Dinheiro,
}

/// Uma devolução de um pedido faturado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Devolucao {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O pedido de origem.
    pub pedido_origem: Id,
    /// O cliente.
    pub cliente: Id,
    /// Data.
    pub data: Data,
    /// O motivo (obrigatório).
    pub motivo: String,
    /// Total ou parcial.
    pub tipo: TipoDevolucao,
    /// Os itens devolvidos.
    pub itens: Vec<ItemDevolvido>,
    /// A soma de `valor_estornado` — bate exatamente com o total quando `Total`.
    pub valor_total: Dinheiro,
    /// O estado.
    pub estado: EstadoDevolucao,
    /// O lançamento de estorno, preenchido em `Concluida`.
    pub estorno_lancamento: Option<Id>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

fn valor_estornado_do_item(item: &ItemVenda, quantidade: Quantidade) -> Dinheiro {
    if quantidade >= item.quantidade {
        item.total_item
    } else {
        item.total_item.fracao(
            quantidade.unidades_internas(),
            item.quantidade.unidades_internas(),
            Arredondamento::MeioAcima,
        )
    }
}

impl Devolucao {
    /// Registra uma solicitação de devolução. `linhas` são `(item_pedido, quantidade)`;
    /// uma quantidade igual (ou maior) à comprada estorna o item inteiro.
    ///
    /// # Errors
    /// - [`ErroVendas::PedidoEmEstadoInvalido`] se o pedido não está faturado/concluído.
    /// - [`ErroVendas::DevolucaoVazia`] se `linhas` está vazia.
    /// - [`ErroVendas::QuantidadeInvalida`] / [`ErroVendas::QuantidadeDevolvidaExcede`].
    pub fn solicitar(
        pedido: &Pedido,
        linhas: &[(Id, Quantidade)],
        motivo: impl Into<String>,
        data: Data,
    ) -> Result<Self, ErroVendas> {
        use crate::pedido::EstadoPedido;
        if !matches!(
            pedido.estado,
            EstadoPedido::Faturado | EstadoPedido::EmEntrega | EstadoPedido::Concluido
        ) {
            return Err(ErroVendas::PedidoEmEstadoInvalido {
                atual: pedido.estado.rotulo(),
                esperado: "Faturado/EmEntrega/Concluido",
            });
        }
        if linhas.is_empty() {
            return Err(ErroVendas::DevolucaoVazia);
        }

        let mut itens = Vec::with_capacity(linhas.len());
        for (item_id, quantidade) in linhas {
            if !quantidade.e_positiva() {
                return Err(ErroVendas::QuantidadeInvalida);
            }
            let item = pedido
                .itens
                .iter()
                .find(|i| i.id == *item_id)
                .ok_or(ErroVendas::QuantidadeInvalida)?;
            if *quantidade > item.quantidade {
                return Err(ErroVendas::QuantidadeDevolvidaExcede);
            }
            itens.push(ItemDevolvido {
                item_pedido: item.id,
                produto: item.produto,
                variacao: item.variacao,
                quantidade: *quantidade,
                valor_estornado: valor_estornado_do_item(item, *quantidade),
            });
        }

        let valor_total = itens.iter().map(|i| i.valor_estornado).sum();
        let cobre_tudo = pedido.itens.iter().all(|orig| {
            itens
                .iter()
                .find(|d| d.item_pedido == orig.id)
                .is_some_and(|d| d.quantidade >= orig.quantidade)
        });
        let tipo = if cobre_tudo {
            TipoDevolucao::Total
        } else {
            TipoDevolucao::Parcial
        };

        Ok(Self {
            id: Id::novo(),
            empresa: pedido.empresa,
            pedido_origem: pedido.id,
            cliente: pedido.cliente,
            data,
            motivo: motivo.into(),
            tipo,
            itens,
            valor_total,
            estado: EstadoDevolucao::Solicitada,
            estorno_lancamento: None,
            versao: Versao::INICIAL,
        })
    }

    /// `Solicitada → Aprovada`.
    ///
    /// # Errors
    /// [`ErroVendas::DevolucaoEmEstadoInvalido`].
    pub fn aprovar(&mut self) -> Result<(), ErroVendas> {
        self.exigir(EstadoDevolucao::Solicitada)?;
        self.estado = EstadoDevolucao::Aprovada;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// `Aprovada → Concluida` — o comando faz o estorno no razão e repõe o estoque; aqui só
    /// a transição e o vínculo com o lançamento.
    ///
    /// # Errors
    /// [`ErroVendas::DevolucaoEmEstadoInvalido`].
    pub fn concluir(&mut self, estorno_lancamento: Id) -> Result<(), ErroVendas> {
        self.exigir(EstadoDevolucao::Aprovada)?;
        self.estado = EstadoDevolucao::Concluida;
        self.estorno_lancamento = Some(estorno_lancamento);
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Cancela uma devolução ainda não concluída.
    ///
    /// # Errors
    /// [`ErroVendas::DevolucaoEmEstadoInvalido`].
    pub fn cancelar(&mut self) -> Result<(), ErroVendas> {
        if matches!(
            self.estado,
            EstadoDevolucao::Solicitada | EstadoDevolucao::Aprovada
        ) {
            self.estado = EstadoDevolucao::Cancelada;
            self.versao = self.versao.proxima();
            Ok(())
        } else {
            Err(ErroVendas::DevolucaoEmEstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Solicitada/Aprovada",
            })
        }
    }

    fn exigir(&self, estado: EstadoDevolucao) -> Result<(), ErroVendas> {
        if self.estado == estado {
            Ok(())
        } else {
            Err(ErroVendas::DevolucaoEmEstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: estado.rotulo(),
            })
        }
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Fuso, Percentual, Preco};
    use proptest::prelude::*;

    use super::*;
    use crate::pedido::Pedido;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn pedido_faturado(itens: Vec<(i64, i64)>) -> Pedido {
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
        for (qtd, preco) in itens {
            p.adicionar_item(
                ItemVenda::novo(
                    Id::novo(),
                    None,
                    Quantidade::unidades(qtd),
                    Preco::reais(preco),
                    Percentual::ZERO,
                    Percentual::pontos(20),
                )
                .unwrap(),
            )
            .unwrap();
        }
        p.confirmar().unwrap();
        p.faturar().unwrap();
        p
    }

    #[test]
    fn devolucao_total_estorna_o_pedido_inteiro() {
        let p = pedido_faturado(vec![(120, 5), (60, 5)]);
        let linhas: Vec<_> = p.itens.iter().map(|i| (i.id, i.quantidade)).collect();
        let d = Devolucao::solicitar(&p, &linhas, "avaria", hoje()).unwrap();
        assert_eq!(d.tipo, TipoDevolucao::Total);
        assert_eq!(d.valor_total, p.total);
    }

    #[test]
    fn devolucao_parcial_rateia_proporcionalmente() {
        let p = pedido_faturado(vec![(120, 5)]); // total 600,00
        let item = p.itens[0].id;
        let d = Devolucao::solicitar(&p, &[(item, Quantidade::unidades(20))], "avaria", hoje())
            .unwrap();
        assert_eq!(d.tipo, TipoDevolucao::Parcial);
        // 20/120 de 600,00 = 100,00
        assert_eq!(d.valor_total, Dinheiro::reais(100));
    }

    #[test]
    fn devolver_mais_que_o_comprado_falha() {
        let p = pedido_faturado(vec![(10, 5)]);
        let item = p.itens[0].id;
        assert_eq!(
            Devolucao::solicitar(&p, &[(item, Quantidade::unidades(11))], "x", hoje()).unwrap_err(),
            ErroVendas::QuantidadeDevolvidaExcede
        );
    }

    #[test]
    fn pedido_nao_faturado_nao_aceita_devolucao() {
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
        p.adicionar_item(
            ItemVenda::novo(
                Id::novo(),
                None,
                Quantidade::unidades(1),
                Preco::reais(1),
                Percentual::ZERO,
                Percentual::pontos(20),
            )
            .unwrap(),
        )
        .unwrap();
        let item = p.itens[0].id;
        assert!(Devolucao::solicitar(&p, &[(item, Quantidade::UM)], "x", hoje()).is_err());
    }

    #[test]
    fn fluxo_de_estados() {
        let p = pedido_faturado(vec![(1, 10)]);
        let linhas: Vec<_> = p.itens.iter().map(|i| (i.id, i.quantidade)).collect();
        let mut d = Devolucao::solicitar(&p, &linhas, "x", hoje()).unwrap();
        assert!(d.concluir(Id::novo()).is_err()); // precisa aprovar antes
        d.aprovar().unwrap();
        d.concluir(Id::novo()).unwrap();
        assert_eq!(d.estado, EstadoDevolucao::Concluida);
        assert!(d.estorno_lancamento.is_some());
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(300))]

        /// A devolução total sempre estorna exatamente o total do pedido, para qualquer
        /// combinação de itens (`docs/modulos/vendas.md` §11.5).
        #[test]
        fn devolucao_total_bate_com_o_total_do_pedido(
            itens in prop::collection::vec((1i64..=1000, 1i64..=100_000), 1..8),
        ) {
            let p = pedido_faturado(itens);
            let linhas: Vec<_> = p.itens.iter().map(|i| (i.id, i.quantidade)).collect();
            let d = Devolucao::solicitar(&p, &linhas, "x", hoje()).unwrap();
            prop_assert_eq!(d.valor_total, p.total);
        }
    }
}
