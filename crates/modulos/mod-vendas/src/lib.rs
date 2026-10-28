//! # mod-vendas
//!
//! O caminho comercial completo — orçamento, pedido, preço, comissão, entrega e devolução —
//! que termina sempre num lançamento balanceado no Razão. Ver `docs/modulos/vendas.md`.
//!
//! ## O que este crate contém agora
//!
//! O **domínio puro** (`docs/19-estado-e-processo.md` §3.2):
//!
//! - [`preco_vigente`] sobre [`TabelaPreco`]/[`RegraPreco`] — resolução por faixa de
//!   quantidade e promoção com vigência; a regra mais específica que cobre vence.
//! - [`Orcamento`] / [`Pedido`] / [`ItemVenda`] — as duas máquinas de estado, o total
//!   calculado a partir dos itens, o **preço congelado no item** (§11.1), a reserva só na
//!   confirmação (§11.2), **faturar irreversível** (§11.3) e o **desconto acima do teto do
//!   papel recusado na hora** (§11.4).
//! - [`Devolucao`] — total e parcial, com rateio proporcional que **não perde centavo**
//!   (§11.5, propriedade).
//! - [`Comissao`] — apuração sobre a base líquida de devolução; nunca sobre pedido
//!   cancelado ou devolvido por inteiro (§11.6).
//! - [`receituario`] — faturamento (receita + desconto + CMV), devolução e comissão viram
//!   [`LancamentoBalanceado`](cardeal_ledger::LancamentoBalanceado) (§7).
//! - [`manifesto`] — a identidade declarativa do módulo, validada.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 2)
//!
//! Os **comandos** e **consultas** paginadas, que dependem de `cardeal-storage`; as portas
//! síncronas para `estoque` (reservar/consumir) e `clientes` (`LimiteDisponivel`); a
//! apuração mensal de comissão em lote; o contrato recorrente que cria a `Recorrencia` no
//! financeiro.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]
// `persist(e)` (conversão de erro do SQLite) é o padrão dos módulos irmãos.
#![allow(clippy::needless_pass_by_value)]

mod comandos;
mod comissao;
mod consultas;
mod consultas_comissao;
mod defasados;
mod devolucao;
mod erros;
pub mod eventos;
mod manifesto;
mod margem;
#[cfg(feature = "sqlite")]
pub mod migracoes;
#[cfg(feature = "sqlite")]
mod modulo;
mod pedido;
mod preco;
mod receita;
pub mod receituario;
#[cfg(feature = "sqlite")]
mod repositorio;
#[cfg(feature = "sqlite")]
mod repositorio_comissao;

#[cfg(feature = "sqlite")]
pub use comandos::devolucao_por_id;
#[cfg(feature = "sqlite")]
pub use comandos::{estornar_venda_comissionavel_comum, registrar_venda_comissionavel_comum};
pub use comandos::{
    AdicionarItemPedido, AplicarPrecosSugeridos, ApurarComissoes, CancelarPedido,
    ComissoesApuradas, ComissoesPagas, ConfirmarPedido, CriarPedido, CriarRegraPreco,
    CriarTabelaPreco, FaturarPedido, ItemFoiAdicionado, PagarComissoes, PedidoCriado,
    PedidoFoiFaturado, PrecosAplicados, RegraPrecoCriada, TabelaPrecoCriada,
};
pub use comandos::{
    AprovarDevolucao, CancelarDevolucao, ConcluirDevolucao, DevolucaoFoiConcluida,
    DevolucoesDoPedido, DevolucoesPendentes, SolicitarDevolucao, MOTIVO_MINIMO_DEVOLUCAO_VENDA,
};
pub use comissao::{Comissao, EstadoComissao};
pub use consultas::{ItemPedido, ItensDoPedido, PedidosRecentes, RegrasDaTabela, TabelasDePreco};
pub use consultas_comissao::{
    resumo_por_vendedor, ComissoesDoMes, LinhaComissao, ResumoDoVendedor,
};
pub use defasados::{classificar, PrecoDefasado, PrecosDefasados, SituacaoDoPreco};
pub use devolucao::{Devolucao, EstadoDevolucao, ItemDevolvido, JaDevolvido, TipoDevolucao};
pub use erros::ErroVendas;
pub use manifesto::{manifesto, MANIFESTO};
pub use margem::{MargemDoPedido, MargemDoPedidoCalculada};
#[cfg(feature = "sqlite")]
pub use modulo::ModuloVendas;
pub use pedido::{EstadoOrcamento, EstadoPedido, ItemVenda, Orcamento, Pedido};
pub use preco::{preco_vigente, AlvoRegra, RegraPreco, TabelaPreco, TipoTabela};
pub use receita::ReceitaPorProduto;
#[cfg(feature = "sqlite")]
pub use repositorio::RepositorioVendas;

/// As vendas faturadas de cada cliente desde `desde` (cliente, dia, total) — a porta pública para
/// a curva de clientes do PDV, que junta as vendas do balcão às dos pedidos.
///
/// # Errors
/// Erro do SQLite.
#[cfg(feature = "sqlite")]
pub fn vendas_por_cliente(
    c: &rusqlite::Connection,
    empresa: cardeal_kernel::Id,
    desde: cardeal_kernel::Data,
) -> cardeal_kernel::Resultado<
    Vec<(
        cardeal_kernel::Id,
        cardeal_kernel::Data,
        cardeal_kernel::Dinheiro,
    )>,
> {
    use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id};
    let falha = |e: rusqlite::Error| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string());
    let mut stmt = c
        .prepare(
            "SELECT cliente, data, total FROM vendas_pedido
             WHERE empresa = ?1 AND estado IN ('Faturado','EmEntrega','Concluido') AND data >= ?2",
        )
        .map_err(falha)?;
    let v = stmt
        .query_map(
            rusqlite::params![empresa.em_bytes().as_slice(), i64::from(desde.em_dias())],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            },
        )
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha)?
        .into_iter()
        .map(|(cliente, data, total)| {
            (
                <[u8; 16]>::try_from(cliente).map_or(Id::NULO, Id::de_bytes),
                Data::de_dias(i32::try_from(data).unwrap_or(0)),
                Dinheiro::centavos(total),
            )
        })
        .collect();
    Ok(v)
}
