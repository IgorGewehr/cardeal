//! Fatura o pedido confirmado: consome o estoque item a item, monta o lançamento combinado
//! (receita + desconto + CMV) e, se a prazo, cria o título a receber vinculado a ele.
//!
//! `docs/modulos/vendas.md` §5 e §7. Como em `mod-os::FaturarOrdemServico`, não usa
//! `mod_financeiro::lancar_titulo_comum` — o lançamento aqui é **combinado**
//! (receita+desconto+CMV juntos); o título, quando existe, nasce vinculado a esse mesmo
//! lançamento, nunca a um segundo.

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{
    Contas, Contraparte as ContraparteRazao, PapelConta, Razao, RepositorioRazao,
};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::{registrar_saida_comum, DadosSaida};
use mod_financeiro::{ConstrutorTitulo, EspecieTitulo, RepositorioFinanceiro};
use serde::{Deserialize, Serialize};

use crate::comandos::{autoria_de, carregar_pedido};
use crate::eventos::PedidoFaturado;
use crate::receituario::{faturar_pedido, ContasFaturamento};
use crate::repositorio::RepositorioVendas;

/// Fatura o pedido confirmado.
///
/// A prazo gera **um** título de parcela única vencendo na data do pedido — o mesmo
/// desenho de `mod-os::FaturarOrdemServico`. Parcelamento de verdade depende de uma
/// `CondicaoPagamento` resolúvel, que ainda não existe (`docs/17-roadmap.md`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FaturarPedido {
    /// O pedido.
    pub pedido: Id,
    /// Se o pagamento é à vista (Caixa) ou a prazo (Clientes a receber, com título).
    pub a_vista: bool,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PedidoFoiFaturado {
    /// O lançamento gerado no razão.
    pub lancamento: Id,
    /// O título a receber gerado no financeiro, se a prazo.
    pub titulo: Option<Id>,
    /// O total faturado.
    pub valor_total: Dinheiro,
}

impl Comando for FaturarPedido {
    type Saida = PedidoFoiFaturado;
    const PERMISSAO: &'static str = "vendas.pedido.faturar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut pedido = carregar_pedido(uow, self.pedido)?;

        // 2. Validar (domínio puro): transiciona Confirmado -> Faturado.
        pedido.faturar().map_err(|e| Erro::de_dominio(&e))?;

        // 3. Consumir o estoque item a item, somando o CMV apurado (quem consome é quem
        //    lança — `docs/modulos/estoque.md` §7).
        let mut cmv_total = Dinheiro::ZERO;
        for item in &pedido.itens {
            let saida = registrar_saida_comum(
                DadosSaida {
                    produto: item.produto,
                    local: pedido.local_expedicao,
                    quantidade: item.quantidade,
                    origem_modulo: "vendas",
                    origem_id: Some(pedido.id),
                },
                ctx,
                uow,
            )?;
            cmv_total += saida.aplicada.custo_total;
        }

        // Resolver contas e montar o lançamento combinado (receita + desconto + CMV).
        let contas = {
            let repo = RepositorioRazao::novo(uow);
            let resolvedor = Contas::nova(&repo, ctx.empresa);
            ContasFaturamento {
                destino: resolvedor
                    .papel(if self.a_vista {
                        PapelConta::Caixa
                    } else {
                        PapelConta::ClientesAReceber
                    })
                    .map_err(|e| Erro::de_dominio(&e))?,
                receita: resolvedor
                    .papel(PapelConta::ReceitaVendas)
                    .map_err(|e| Erro::de_dominio(&e))?,
                descontos: if pedido.desconto_total.e_positivo() {
                    resolvedor
                        .papel(PapelConta::DescontosConcedidos)
                        .map_err(|e| Erro::de_dominio(&e))?
                } else {
                    Id::NULO
                },
                cmv: if cmv_total.e_positivo() {
                    resolvedor
                        .papel(PapelConta::Cmv)
                        .map_err(|e| Erro::de_dominio(&e))?
                } else {
                    Id::NULO
                },
                estoque: if cmv_total.e_positivo() {
                    resolvedor
                        .papel(PapelConta::EstoqueMercadorias)
                        .map_err(|e| Erro::de_dominio(&e))?
                } else {
                    Id::NULO
                },
            }
        };
        let lancamento_balanceado =
            faturar_pedido(&pedido, self.a_vista, cmv_total, contas, autoria_de(ctx))
                .map_err(|e| Erro::de_dominio(&e))?;

        let lancamento = {
            let mut repo = RepositorioRazao::novo(uow);
            Razao::registrar(&mut repo, lancamento_balanceado).map_err(|e| Erro::de_dominio(&e))?
        };

        // 4. Persistir: título a receber vinculado a ESTE lançamento, só a prazo.
        let titulo = if self.a_vista {
            None
        } else {
            let mut tcp = ConstrutorTitulo::novo(
                ctx.empresa,
                EspecieTitulo::Receber,
                ContraparteRazao::Cliente(pedido.cliente),
                pedido.total,
                pedido.data,
            )
            .origem("vendas", Some(pedido.id))
            .parcelas(1, pedido.data, 0)
            .construir()
            .map_err(|e| Erro::de_dominio(&e))?;
            tcp.parcelas[0].lancamento = Some(lancamento);
            RepositorioFinanceiro::novo(uow).inserir_titulo(&tcp)?;
            Some(tcp.titulo.id)
        };
        RepositorioVendas::novo(uow).atualizar_pedido(&pedido)?;

        // 5. Publicar.
        uow.publicar(PedidoFaturado {
            pedido: pedido.id,
            cliente: pedido.cliente,
            total: pedido.total,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(PedidoFoiFaturado {
            lancamento,
            titulo,
            valor_total: pedido.total,
        })
    }
}
