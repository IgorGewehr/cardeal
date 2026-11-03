//! Apuração e pagamento da comissão do vendedor (`docs/modulos/vendas.md` §5 e §7).
//!
//! **Apurar** é idempotente por pedido: lança D Despesas comerciais / C Comissões a pagar para
//! cada pedido faturado no mês que ainda não tem comissão, item a item com o percentual
//! congelado no faturamento (a comissão por estrutura do estoque). Rodar de novo só pega os
//! pedidos faturados depois. **Pagar** quita, num lançamento só, o que um vendedor tem apurado
//! no mês: D Comissões a pagar / C Caixa ou Bancos.
//!
//! As contas: o débito vai para "Comissões de vendedores" (5.42) quando a empresa aplicou o
//! modelo de plano do Comércio, senão para Despesas comerciais (5.5); o crédito, para Salários
//! e encargos (2.1.03) — comissão é remuneração, e o plano padrão não tem a subconta.

use cardeal_kernel::{Competencia, Data, Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_ledger::{Contas, ErroRazao, PapelConta, Razao, RepositorioRazao};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::autoria_de;
#[cfg(feature = "sqlite")]
use crate::comissao::EstadoComissao;
#[cfg(feature = "sqlite")]
use crate::erros::ErroVendas;
#[cfg(feature = "sqlite")]
use crate::eventos::ComissaoApurada;
#[cfg(feature = "sqlite")]
use crate::receituario::{apurar_comissao, pagar_comissoes, ContasComissao};
#[cfg(feature = "sqlite")]
use crate::repositorio_comissao;

/// Apura as comissões dos pedidos faturados num mês.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApurarComissoes {
    /// O mês dos pedidos.
    pub competencia: Competencia,
}

/// O que a apuração lançou agora (zero se já estava tudo apurado).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComissoesApuradas {
    /// Pedidos apurados nesta rodada.
    pub pedidos: u32,
    /// A comissão somada.
    pub valor: Dinheiro,
}

/// As contas da comissão, resolvidas na empresa.
#[cfg(feature = "sqlite")]
pub(crate) fn contas_de_comissao(
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ContasComissao> {
    let repo = RepositorioRazao::novo(uow);
    let contas = Contas::nova(&repo, ctx.empresa);
    let despesas_comerciais = match contas.por_codigo("5.42") {
        Ok(conta) => conta,
        Err(ErroRazao::CodigoNaoEncontrado(_)) => contas
            .papel(PapelConta::DespesaComercial)
            .map_err(|e| Erro::de_dominio(&e))?,
        Err(e) => return Err(Erro::de_dominio(&e)),
    };
    let comissoes_a_pagar = contas
        .papel(PapelConta::SalariosAPagar)
        .map_err(|e| Erro::de_dominio(&e))?;
    Ok(ContasComissao {
        despesas_comerciais,
        comissoes_a_pagar,
    })
}

impl Comando for ApurarComissoes {
    type Saida = ComissoesApuradas;
    const PERMISSAO: &'static str = "vendas.comissao.apurar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let comissoes =
            repositorio_comissao::a_apurar(uow.conexao(), ctx, self.competencia, ctx.hoje())?;
        let mut saida = ComissoesApuradas {
            pedidos: 0,
            valor: Dinheiro::ZERO,
        };
        if comissoes.is_empty() {
            return Ok(saida);
        }
        let contas = contas_de_comissao(ctx, uow)?;
        for (comissao, _) in &comissoes {
            let lancamento = apurar_comissao(comissao, contas, autoria_de(ctx))
                .map_err(|e| Erro::de_dominio(&e))?;
            let lancamento = {
                let mut repo = RepositorioRazao::novo(uow);
                Razao::registrar(&mut repo, lancamento).map_err(|e| Erro::de_dominio(&e))?
            };
            repositorio_comissao::inserir(uow.conexao(), comissao, lancamento, ctx.agora)?;
            uow.publicar(ComissaoApurada {
                comissao: comissao.id,
                vendedor: comissao.vendedor,
                valor: comissao.valor,
            })
            .map_err(|e| Erro::de_dominio(&e))?;
            saida.pedidos += 1;
            saida.valor += comissao.valor;
        }
        Ok(saida)
    }
}

/// Paga tudo o que um vendedor tem apurado (e ainda não pago) num mês.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PagarComissoes {
    /// O vendedor (usuário).
    pub vendedor: Id,
    /// O mês da apuração.
    pub competencia: Competencia,
    /// A conta de onde sai o dinheiro — caixa ou banco (grupo 1.1 Disponível).
    pub conta: Id,
    /// A data do pagamento: entre a apuração e hoje.
    pub data: Data,
}

/// O pagamento feito.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComissoesPagas {
    /// O lançamento do pagamento.
    pub lancamento: Id,
    /// Pedidos quitados.
    pub pedidos: u32,
    /// O valor pago.
    pub valor: Dinheiro,
}

/// A conta tem de ser analítica, ativa e do Disponível (caixa ou banco) da empresa.
#[cfg(feature = "sqlite")]
fn exigir_conta_disponivel(uow: &mut UnidadeDeTrabalho, empresa: Id, conta: Id) -> Resultado<()> {
    use rusqlite::OptionalExtension;
    let codigo: Option<String> = uow
        .conexao()
        .query_row(
            "SELECT codigo FROM razao_conta
             WHERE id = ?1 AND empresa = ?2 AND tipo = 'Analitica' AND ativa = 1",
            rusqlite::params![conta.em_bytes().as_slice(), empresa.em_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(crate::repositorio::persist)?;
    match codigo {
        Some(c) if c.starts_with("1.1.") => Ok(()),
        _ => Err(Erro::de_dominio(&ErroVendas::ContaDePagamentoInvalida)),
    }
}

impl Comando for PagarComissoes {
    type Saida = ComissoesPagas;
    const PERMISSAO: &'static str = "vendas.comissao.pagar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut apuradas: Vec<_> = repositorio_comissao::do_mes(
            uow.conexao(),
            ctx.empresa,
            self.competencia,
            Some(self.vendedor),
        )?
        .into_iter()
        .map(|(c, _)| c)
        .filter(|c| c.estado == EstadoComissao::Apurada)
        .collect();
        let Some(apurada_em) = apuradas.iter().map(|c| c.competencia).max() else {
            return Err(Erro::de_dominio(&ErroVendas::SemComissaoAPagar));
        };
        if self.data < apurada_em || self.data > ctx.hoje() {
            return Err(Erro::de_dominio(&ErroVendas::DataDoPagamentoInvalida {
                apurada_em: apurada_em.formatar(),
            }));
        }
        exigir_conta_disponivel(uow, ctx.empresa, self.conta)?;

        let bruto = apuradas
            .iter()
            .fold(Dinheiro::ZERO, |total, c| total + c.valor);
        // Os estornos por devolução ainda não descontados (deste mês ou de antes) saem daqui.
        let ajustes = repositorio_comissao::ajustes(
            uow.conexao(),
            ctx.empresa,
            self.competencia,
            Some(self.vendedor),
            true,
        )?;
        let estornado: Dinheiro = ajustes.iter().map(|a| a.5).sum();
        let valor = bruto - estornado;
        if !valor.e_positivo() {
            return Err(Erro::de_dominio(&ErroVendas::EstornoMaiorQueAComissao {
                comissao: bruto.formatar_com_simbolo(),
                estorno: estornado.formatar_com_simbolo(),
            }));
        }
        let contas = contas_de_comissao(ctx, uow)?;
        let lancamento = pagar_comissoes(
            ctx.empresa,
            self.vendedor,
            valor,
            Id::novo(),
            self.conta,
            contas.comissoes_a_pagar,
            self.data,
            autoria_de(ctx),
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        let lancamento = {
            let mut repo = RepositorioRazao::novo(uow);
            Razao::registrar(&mut repo, lancamento).map_err(|e| Erro::de_dominio(&e))?
        };
        for comissao in &mut apuradas {
            comissao
                .pagar(lancamento)
                .map_err(|e| Erro::de_dominio(&e))?;
            repositorio_comissao::atualizar_estado(uow.conexao(), comissao)?;
        }
        let descontados: Vec<Id> = ajustes.iter().map(|a| a.0).collect();
        repositorio_comissao::descontar_ajustes(uow.conexao(), &descontados, lancamento)?;
        Ok(ComissoesPagas {
            lancamento,
            pedidos: u32::try_from(apuradas.len()).unwrap_or(u32::MAX),
            valor,
        })
    }
}
