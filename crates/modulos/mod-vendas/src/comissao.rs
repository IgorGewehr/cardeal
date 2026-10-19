//! Apuração de comissão de vendedor.
//!
//! `docs/modulos/vendas.md` §3, §4 e §11.6: comissão **nunca é apurada sobre pedido
//! cancelado ou totalmente devolvido**, e a base é o total **líquido de devolução**.
//! Domínio puro.

use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Percentual, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroVendas;
use crate::pedido::{EstadoPedido, Pedido};

/// O estado de uma [`Comissao`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoComissao {
    /// Calculada, aguardando pagamento.
    Apurada,
    /// Paga ao vendedor.
    Paga,
    /// Cancelada (pedido cancelado / devolução total após a apuração).
    Cancelada,
}

/// A comissão de um vendedor por um pedido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comissao {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O vendedor.
    pub vendedor: Id,
    /// O pedido.
    pub pedido: Id,
    /// A base de cálculo: total do pedido líquido de devolução.
    pub base_calculo: Dinheiro,
    /// O percentual da tabela do vendedor.
    pub percentual: Percentual,
    /// O valor da comissão.
    pub valor: Dinheiro,
    /// O mês de apuração.
    pub competencia: Data,
    /// O estado.
    pub estado: EstadoComissao,
    /// O lançamento de pagamento, preenchido quando `Paga`.
    pub lancamento: Option<Id>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Comissao {
    /// Apura a comissão de um pedido faturado. `valor_devolvido` é o total já devolvido do
    /// pedido (0 se nada). Devolve `None` quando não há comissão a apurar (pedido não
    /// faturado, ou base ≤ 0 por devolução total).
    #[must_use]
    pub fn apurar(
        pedido: &Pedido,
        percentual: Percentual,
        competencia: Data,
        valor_devolvido: Dinheiro,
    ) -> Option<Self> {
        if !matches!(
            pedido.estado,
            EstadoPedido::Faturado | EstadoPedido::EmEntrega | EstadoPedido::Concluido
        ) {
            return None;
        }
        let base = (pedido.total - valor_devolvido).nao_negativo();
        if base.e_zero() {
            return None;
        }
        Some(Self {
            id: Id::novo(),
            empresa: pedido.empresa,
            vendedor: pedido.vendedor,
            pedido: pedido.id,
            base_calculo: base,
            percentual,
            valor: base.aplicar(percentual, Arredondamento::MeioAcima),
            competencia,
            estado: EstadoComissao::Apurada,
            lancamento: None,
            versao: Versao::INICIAL,
        })
    }

    /// Apura a comissão de um pedido faturado **item a item**, cada um com o seu percentual — a
    /// comissão por estrutura mercadológica (a do produto, a do grupo ou a padrão), congelada
    /// no faturamento. `itens` são (base do item, percentual). A base é o total líquido de
    /// desconto; o `percentual` guardado é o efetivo (valor ÷ base). `None` sem nada a pagar.
    #[must_use]
    pub fn de_itens(
        empresa: Id,
        vendedor: Id,
        pedido: Id,
        itens: &[(Dinheiro, Percentual)],
        competencia: Data,
    ) -> Option<Self> {
        let base = itens
            .iter()
            .fold(Dinheiro::ZERO, |t, (b, _)| t + b.nao_negativo());
        let valor = itens.iter().fold(Dinheiro::ZERO, |t, (b, p)| {
            t + b.nao_negativo().aplicar(*p, Arredondamento::MeioAcima)
        });
        if !valor.e_positivo() {
            return None;
        }
        Some(Self {
            id: Id::novo(),
            empresa,
            vendedor,
            pedido,
            base_calculo: base,
            percentual: Percentual::da_razao(valor.em_centavos(), base.em_centavos()),
            valor,
            competencia,
            estado: EstadoComissao::Apurada,
            lancamento: None,
            versao: Versao::INICIAL,
        })
    }

    /// `Apurada → Paga`, vinculando o lançamento.
    ///
    /// # Errors
    /// [`ErroVendas::ComissaoEmEstadoInvalido`].
    pub fn pagar(&mut self, lancamento: Id) -> Result<(), ErroVendas> {
        if self.estado != EstadoComissao::Apurada {
            return Err(ErroVendas::ComissaoEmEstadoInvalido {
                atual: self.rotulo_estado(),
            });
        }
        self.estado = EstadoComissao::Paga;
        self.lancamento = Some(lancamento);
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Cancela a comissão (pedido cancelado / devolvido por inteiro). Não pode cancelar uma
    /// já paga.
    ///
    /// # Errors
    /// [`ErroVendas::ComissaoEmEstadoInvalido`].
    pub fn cancelar(&mut self) -> Result<(), ErroVendas> {
        if self.estado == EstadoComissao::Paga {
            return Err(ErroVendas::ComissaoEmEstadoInvalido { atual: "Paga" });
        }
        self.estado = EstadoComissao::Cancelada;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// A diferença de comissão devida após uma devolução parcial que reduziu a base
    /// (`docs/modulos/vendas.md` §11.6: recalcula, gera ajuste — **não** sobrescreve).
    /// Negativa quando o vendedor deve devolver parte.
    #[must_use]
    pub fn ajuste_para_nova_base(&self, nova_base: Dinheiro) -> Dinheiro {
        let novo_valor = nova_base
            .nao_negativo()
            .aplicar(self.percentual, Arredondamento::MeioAcima);
        novo_valor - self.valor
    }

    fn rotulo_estado(&self) -> &'static str {
        match self.estado {
            EstadoComissao::Apurada => "Apurada",
            EstadoComissao::Paga => "Paga",
            EstadoComissao::Cancelada => "Cancelada",
        }
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Fuso, Preco, Quantidade};

    use super::*;
    use crate::pedido::ItemVenda;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn pedido_faturado(total_reais: i64) -> Pedido {
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
                Preco::reais(total_reais),
                Percentual::ZERO,
                Percentual::pontos(20),
            )
            .unwrap(),
        )
        .unwrap();
        p.confirmar().unwrap();
        p.faturar().unwrap();
        p
    }

    #[test]
    fn apura_sobre_a_base_liquida() {
        let p = pedido_faturado(1000);
        let c = Comissao::apurar(&p, Percentual::pontos(3), hoje(), Dinheiro::ZERO).unwrap();
        assert_eq!(c.base_calculo, Dinheiro::reais(1000));
        assert_eq!(c.valor, Dinheiro::reais(30));

        // Com devolução de R$ 200,00 a base cai.
        let c = Comissao::apurar(&p, Percentual::pontos(3), hoje(), Dinheiro::reais(200)).unwrap();
        assert_eq!(c.base_calculo, Dinheiro::reais(800));
        assert_eq!(c.valor, Dinheiro::reais(24));
    }

    #[test]
    fn nao_apura_com_devolucao_total_nem_pedido_nao_faturado() {
        let p = pedido_faturado(1000);
        assert!(
            Comissao::apurar(&p, Percentual::pontos(3), hoje(), Dinheiro::reais(1000)).is_none()
        );

        let rascunho = Pedido::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            hoje(),
            Id::novo(),
            Id::novo(),
            Id::novo(),
            Id::novo(),
        );
        assert!(
            Comissao::apurar(&rascunho, Percentual::pontos(3), hoje(), Dinheiro::ZERO).is_none()
        );
    }

    #[test]
    fn ajuste_apos_devolucao_gera_delta_negativo() {
        let p = pedido_faturado(1000);
        let c = Comissao::apurar(&p, Percentual::pontos(3), hoje(), Dinheiro::ZERO).unwrap();
        let delta = c.ajuste_para_nova_base(Dinheiro::reais(800));
        assert_eq!(delta, Dinheiro::reais(-6)); // 24 - 30
    }

    #[test]
    fn de_itens_soma_cada_um_com_o_seu_percentual() {
        let (e, v, p) = (Id::novo(), Id::novo(), Id::novo());
        // R$ 100 a 3% + R$ 50 a 10% = 3 + 5 = R$ 8 sobre R$ 150 (5,33%).
        let c = Comissao::de_itens(
            e,
            v,
            p,
            &[
                (Dinheiro::reais(100), Percentual::pontos(3)),
                (Dinheiro::reais(50), Percentual::pontos(10)),
            ],
            hoje(),
        )
        .unwrap();
        assert_eq!(c.valor, Dinheiro::reais(8));
        assert_eq!(c.base_calculo, Dinheiro::reais(150));
        assert_eq!(c.percentual.formatar(2), "5,33");
        // Tudo a 0%: nada a apurar.
        assert!(
            Comissao::de_itens(e, v, p, &[(Dinheiro::reais(100), Percentual::ZERO)], hoje())
                .is_none()
        );
    }

    #[test]
    fn pagar_e_cancelar_respeitam_o_estado() {
        let p = pedido_faturado(1000);
        let mut c = Comissao::apurar(&p, Percentual::pontos(3), hoje(), Dinheiro::ZERO).unwrap();
        c.pagar(Id::novo()).unwrap();
        assert_eq!(c.estado, EstadoComissao::Paga);
        assert!(c.cancelar().is_err());
        assert!(c.pagar(Id::novo()).is_err());
    }
}
