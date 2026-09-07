//! Fatura a ordem de serviço concluída: monta o lançamento combinado (receita de serviço +
//! custo das peças aplicadas) e cria o título a receber no financeiro.
//!
//! `docs/modulos/os.md` §5 e §7. Não usa `mod_financeiro::lancar_titulo_comum` — aquele
//! helper monta um lançamento simples (só D contraparte/C resultado); a OS precisa de um
//! lançamento **combinado** (receita + CMV juntos, como o próprio doc exige — "mesmo
//! lançamento", não dois). Por isso a OS monta o lançamento com o próprio
//! [`crate::receituario::faturar_ordem_servico`] e reaproveita só as peças de mais baixo
//! nível do financeiro (`ConstrutorTitulo`, `RepositorioFinanceiro::inserir_titulo`) para
//! gravar o título vinculado a esse mesmo lançamento — nunca cria um segundo.

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_ledger::{
    Contas, Contraparte as ContraparteRazao, PapelConta, Razao, RepositorioRazao,
};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_financeiro::{ConstrutorTitulo, EspecieTitulo, RepositorioFinanceiro};
use serde::{Deserialize, Serialize};

use crate::comandos::{autoria_de, carregar_ordem};
use crate::eventos::OrdemFaturada;
use crate::execucao::ItemPeca;
use crate::receituario::{faturar_ordem_servico, ContasFaturamento};
use crate::repositorio::RepositorioOs;

/// Fatura a ordem de serviço.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FaturarOrdemServico {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct OrdemServicoFaturada {
    /// O lançamento gerado no razão, se houve algo a lançar (nenhum, se a OS foi cortesia:
    /// sem cobrança e sem peça aplicada).
    pub lancamento: Option<Id>,
    /// O título a receber gerado no financeiro, se houve cobrança (`valor_total > 0`).
    pub titulo: Option<Id>,
    /// O total faturado.
    pub valor_total: Dinheiro,
}

impl Comando for FaturarOrdemServico {
    type Saida = OrdemServicoFaturada;
    const PERMISSAO: &'static str = "os.faturar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut os = carregar_ordem(uow, self.ordem_servico)?;
        let itens_peca = RepositorioOs::novo(uow).itens_peca_da_ordem(os.id)?;

        // 2. Validar (domínio puro): transiciona Concluida -> Faturada.
        os.faturar().map_err(|e| Erro::de_dominio(&e))?;
        let total_custo_pecas: Dinheiro = itens_peca.iter().map(ItemPeca::total_custo).sum();

        // 3. Resolver contas e montar o lançamento combinado (receita + CMV).
        let contas = {
            let repo = RepositorioRazao::novo(uow);
            let resolvedor = Contas::nova(&repo, ctx.empresa);
            ContasFaturamento {
                clientes_a_receber: resolvedor
                    .papel(PapelConta::ClientesAReceber)
                    .map_err(|e| Erro::de_dominio(&e))?,
                receita_servicos: resolvedor
                    .papel(PapelConta::ReceitaServicos)
                    .map_err(|e| Erro::de_dominio(&e))?,
                custo_servico: if total_custo_pecas.e_positivo() {
                    resolvedor
                        .papel(PapelConta::CustoServico)
                        .map_err(|e| Erro::de_dominio(&e))?
                } else {
                    Id::NULO
                },
                estoque: if total_custo_pecas.e_positivo() {
                    resolvedor
                        .papel(PapelConta::EstoqueMercadorias)
                        .map_err(|e| Erro::de_dominio(&e))?
                } else {
                    Id::NULO
                },
            }
        };
        let lancamento_balanceado =
            faturar_ordem_servico(&os, total_custo_pecas, contas, autoria_de(ctx));

        let lancamento = match lancamento_balanceado {
            Some(lb) => {
                let mut repo = RepositorioRazao::novo(uow);
                Some(Razao::registrar(&mut repo, lb).map_err(|e| Erro::de_dominio(&e))?)
            }
            // Nada a lançar: sem cobrança e sem custo de peça — um serviço cortesia.
            None => None,
        };

        // 4. Persistir: título a receber vinculado a ESTE lançamento, só se houve cobrança
        // (uma OS de garantia/cortesia sem cobrança não gera título nenhum).
        let titulo = if os.valor_total.e_positivo() {
            let lancamento_id = lancamento.expect("valor_total positivo sempre gera um lançamento");
            let mut tcp = ConstrutorTitulo::novo(
                ctx.empresa,
                EspecieTitulo::Receber,
                ContraparteRazao::Cliente(os.cliente),
                os.valor_total,
                ctx.hoje(),
            )
            .origem("os", Some(os.id))
            .parcelas(1, ctx.hoje(), 0)
            .observacao(format!("Ordem de serviço #{}", os.numero))
            .construir()
            .map_err(|e| Erro::de_dominio(&e))?;
            tcp.parcelas[0].lancamento = Some(lancamento_id);
            RepositorioFinanceiro::novo(uow).inserir_titulo(&tcp)?;
            Some(tcp.titulo.id)
        } else {
            None
        };
        RepositorioOs::novo(uow).atualizar_ordem(&os)?;

        // 5. Publicar.
        uow.publicar(OrdemFaturada {
            ordem_servico: os.id,
            cliente: os.cliente,
            valor_total: os.valor_total,
            lancamento,
            titulo,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(OrdemServicoFaturada {
            lancamento,
            titulo,
            valor_total: os.valor_total,
        })
    }
}
