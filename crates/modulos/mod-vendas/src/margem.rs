//! O painel de margem do pedido: quanto cada item e o conjunto deixam de lucro líquido,
//! descontados custo da mercadoria, operação, impostos, comissão e taxas — o que o vendedor
//! precisa ver **antes** de fechar o negócio. O cálculo vem de `mod-estoque` (os mesmos
//! percentuais do preço sugerido); aqui só se monta o pedido.

use cardeal_kernel::{Id, Percentual};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use mod_estoque::precificacao::PainelDeMargem;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;

/// O painel de margem de um pedido.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MargemDoPedido {
    /// O pedido.
    pub pedido: Id,
}

/// O painel e o que a empresa considera margem aceitável.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MargemDoPedidoCalculada {
    /// A margem de cada item e do conjunto.
    pub painel: PainelDeMargem,
    /// A margem líquida mínima definida nos parâmetros de preço.
    pub margem_minima: Percentual,
    /// Posições (em `painel.itens`) dos itens com margem líquida abaixo da mínima.
    pub itens_abaixo_do_minimo: Vec<usize>,
    /// O conjunto está abaixo da margem mínima.
    pub pedido_abaixo_do_minimo: bool,
}

impl Consulta for MargemDoPedido {
    type Saida = MargemDoPedidoCalculada;
    const PERMISSAO: &'static str = "vendas.margem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use mod_estoque::precificacao::{painel_de_margem, ItemDeMargem};

        let itens = crate::consultas::itens_do_pedido(conexao, self.pedido)?;
        let ids: Vec<Id> = itens.iter().map(|i| i.produto).collect();
        let parametros = mod_estoque::parametros_da_empresa(conexao, ctx)?;
        let sugestoes = mod_estoque::sugerir_precos(conexao, ctx, None, Some(&ids))?;

        let entrada: Vec<ItemDeMargem> = itens
            .iter()
            .map(|i| {
                let s = sugestoes.iter().find(|s| s.produto == i.produto);
                ItemDeMargem {
                    rotulo: s.map_or_else(|| "Produto".to_owned(), |s| s.nome.clone()),
                    quantidade: i.quantidade,
                    receita: i.total_item,
                    custo_unitario: s.and_then(|s| s.custo),
                    encargos: s.map_or(Percentual::ZERO, |s| {
                        s.custo_operacional
                            + s.imposto
                            + parametros.comissao
                            + parametros.taxa_pagamento
                    }),
                }
            })
            .collect();
        let painel = painel_de_margem(&entrada);
        let minima = parametros.margem_minima;
        let itens_abaixo = painel
            .itens
            .iter()
            .enumerate()
            .filter(|(_, m)| m.margem_liquida_pct.is_some_and(|p| p < minima))
            .map(|(i, _)| i)
            .collect();
        let pedido_abaixo = painel.margem_liquida_pct.is_some_and(|p| p < minima);
        Ok(MargemDoPedidoCalculada {
            painel,
            margem_minima: minima,
            itens_abaixo_do_minimo: itens_abaixo,
            pedido_abaixo_do_minimo: pedido_abaixo,
        })
    }
}
