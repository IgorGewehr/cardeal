//! A comissão do mês: o que já foi apurado (e pago) e a prévia do que ainda falta apurar,
//! pedido a pedido — e o resumo por vendedor que a tela de fechamento mostra.

use std::collections::BTreeMap;

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Competencia, Data, Dinheiro, Id, Percentual};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::comissao::EstadoComissao;
#[cfg(feature = "sqlite")]
use crate::repositorio_comissao;

/// A comissão de um pedido no mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaComissao {
    /// O pedido.
    pub pedido: Id,
    /// A data do pedido.
    pub data_pedido: Data,
    /// O vendedor (usuário).
    pub vendedor: Id,
    /// A base: o total do pedido, líquido de desconto.
    pub base: Dinheiro,
    /// O percentual efetivo (valor ÷ base; itens podem ter percentuais diferentes).
    pub percentual: Percentual,
    /// A comissão.
    pub valor: Dinheiro,
    /// O estado; `None` = ainda a apurar (prévia, pelo percentual congelado no faturamento).
    pub estado: Option<EstadoComissao>,
}

/// As comissões de um mês, apuradas e a apurar.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComissoesDoMes {
    /// O mês dos pedidos.
    pub competencia: Competencia,
}

impl Consulta for ComissoesDoMes {
    type Saida = Vec<LinhaComissao>;
    const PERMISSAO: &'static str = "vendas.comissao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let linha =
            |c: crate::Comissao, data_pedido: Data, estado: Option<EstadoComissao>| LinhaComissao {
                pedido: c.pedido,
                data_pedido,
                vendedor: c.vendedor,
                base: c.base_calculo,
                percentual: c.percentual,
                valor: c.valor,
                estado,
            };
        let mut linhas: Vec<LinhaComissao> =
            repositorio_comissao::do_mes(conexao, ctx.empresa, self.competencia, None)?
                .into_iter()
                .map(|(c, data)| {
                    let estado = Some(c.estado);
                    linha(c, data, estado)
                })
                .collect();
        linhas.extend(
            repositorio_comissao::a_apurar(conexao, ctx, self.competencia, ctx.hoje())?
                .into_iter()
                .map(|(c, data)| linha(c, data, None)),
        );
        linhas.sort_by_key(|l| l.data_pedido);
        Ok(linhas)
    }
}

/// O fechamento de um vendedor no mês.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumoDoVendedor {
    /// O vendedor (usuário).
    pub vendedor: Id,
    /// Pedidos com comissão (de qualquer estado, menos cancelada).
    pub pedidos: u32,
    /// A base somada.
    pub base: Dinheiro,
    /// Prévia do que falta apurar.
    pub a_apurar: Dinheiro,
    /// Apurado e ainda não pago.
    pub a_pagar: Dinheiro,
    /// Já pago.
    pub pago: Dinheiro,
}

/// Agrupa as linhas do mês por vendedor (cancelada não conta).
#[must_use]
pub fn resumo_por_vendedor(linhas: &[LinhaComissao]) -> Vec<ResumoDoVendedor> {
    let mut por: BTreeMap<Id, ResumoDoVendedor> = BTreeMap::new();
    for l in linhas {
        if l.estado == Some(EstadoComissao::Cancelada) {
            continue;
        }
        let r = por.entry(l.vendedor).or_insert(ResumoDoVendedor {
            vendedor: l.vendedor,
            pedidos: 0,
            base: Dinheiro::ZERO,
            a_apurar: Dinheiro::ZERO,
            a_pagar: Dinheiro::ZERO,
            pago: Dinheiro::ZERO,
        });
        r.pedidos += 1;
        r.base += l.base;
        match l.estado {
            None => r.a_apurar += l.valor,
            Some(EstadoComissao::Apurada) => r.a_pagar += l.valor,
            Some(EstadoComissao::Paga) => r.pago += l.valor,
            Some(EstadoComissao::Cancelada) => {}
        }
    }
    por.into_values().collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn linha(vendedor: Id, valor: i64, estado: Option<EstadoComissao>) -> LinhaComissao {
        LinhaComissao {
            pedido: Id::novo(),
            data_pedido: Data::de_ymd(2026, 9, 10).unwrap(),
            vendedor,
            base: Dinheiro::reais(valor * 20),
            percentual: Percentual::pontos(5),
            valor: Dinheiro::reais(valor),
            estado,
        }
    }

    #[test]
    fn resumo_separa_a_apurar_a_pagar_e_pago_e_ignora_cancelada() {
        let (ana, beto) = (Id::novo(), Id::novo());
        let linhas = [
            linha(ana, 10, None),
            linha(ana, 4, Some(EstadoComissao::Apurada)),
            linha(ana, 6, Some(EstadoComissao::Paga)),
            linha(ana, 99, Some(EstadoComissao::Cancelada)),
            linha(beto, 3, Some(EstadoComissao::Apurada)),
        ];
        let resumo = resumo_por_vendedor(&linhas);
        let a = resumo.iter().find(|r| r.vendedor == ana).unwrap();
        assert_eq!(a.pedidos, 3);
        assert_eq!(a.base, Dinheiro::reais(400));
        assert_eq!(
            (a.a_apurar, a.a_pagar, a.pago),
            (Dinheiro::reais(10), Dinheiro::reais(4), Dinheiro::reais(6))
        );
        let b = resumo.iter().find(|r| r.vendedor == beto).unwrap();
        assert_eq!((b.pedidos, b.a_pagar), (1, Dinheiro::reais(3)));
    }
}
