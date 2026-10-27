//! Abater o título de uma origem — a devolução de compra, que diminui a conta a pagar da nota
//! (`docs/modulos/compras.md` §7: D Fornecedores / C Estoque). Não é pagamento: nenhum dinheiro
//! anda, e não há juros nem multa, mesmo com a parcela vencida.
//!
//! O valor abate as parcelas em aberto da que vence primeiro em diante (o "abate no próximo
//! boleto" de sempre). O que sobra — a conta já paga, ou o que falta pagar não cobre — vira um
//! **crédito com a contraparte**: um título a receber dela (D Clientes a receber / C Estoque),
//! que se baixa quando ela devolve o dinheiro. Fica em Clientes a receber, não em Adiantamentos
//! a fornecedores (1.2.04, o que a spec diz): a baixa do financeiro credita sempre Clientes a
//! receber, e um título em 1.2.04 deixaria as duas contas erradas na baixa.
//!
//! Sem título vivo na origem (a entrada confirmada sem gerar conta a pagar), nada acontece no
//! financeiro — como na entrada.

use cardeal_kernel::{Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_ledger::{
    ConstrutorLancamento, Contas, Contraparte, Origem, PapelConta, Razao, RepositorioRazao,
};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::baixa::PlanoBaixa;
#[cfg(feature = "sqlite")]
use crate::erros::ErroFinanceiro;
#[cfg(feature = "sqlite")]
use crate::eventos::ParcelaBaixada;
#[cfg(feature = "sqlite")]
use crate::repositorio::{BaixaGravada, RepositorioFinanceiro};
#[cfg(feature = "sqlite")]
use crate::titulo::{EspecieTitulo, EstadoParcela};

/// O que o abatimento fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AbatimentoFeito {
    /// O título abatido (o da origem), se havia.
    pub titulo: Option<Id>,
    /// Quanto se abateu das parcelas em aberto.
    pub abatido: Dinheiro,
    /// O título a receber com o que sobrou — o crédito com a contraparte —, se sobrou.
    pub credito: Option<Id>,
    /// O valor do crédito.
    pub valor_credito: Dinheiro,
}

/// O que abater, de onde, e para onde vai a contrapartida.
#[cfg(feature = "sqlite")]
pub struct DadosAbatimento<'a> {
    /// A origem do título a abater (`"compras"`) …
    pub origem_modulo: &'a str,
    /// … e o agregado (a nota de entrada).
    pub origem_id: Id,
    /// Quanto abater.
    pub valor: Dinheiro,
    /// A conta que recebe a contrapartida (o estoque que voltou ao fornecedor).
    pub papel_contrapartida: PapelConta,
    /// A origem do crédito, se sobrar (`"compras_devolucao"`) …
    pub origem_do_credito: &'static str,
    /// … e o agregado (a devolução).
    pub id_do_credito: Id,
    /// Com quem fica o crédito (o fornecedor).
    pub contraparte: Contraparte,
    /// O histórico dos lançamentos ("Devolução de compra — nota 77/1").
    pub historico: String,
}

/// Abate o título vivo de uma origem; o que não couber vira crédito com a contraparte.
///
/// # Errors
/// [`ErroFinanceiro::ValorInvalido`] para valor não positivo; título da origem que não é a
/// pagar; os erros do Razão e de [`super::lancar_titulo_comum`].
#[cfg(feature = "sqlite")]
pub fn abater_titulo_da_origem_comum(
    d: &DadosAbatimento<'_>,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<AbatimentoFeito> {
    if !d.valor.e_positivo() {
        return Err(Erro::de_dominio(&ErroFinanceiro::ValorInvalido));
    }
    let Some(titulo) = crate::consultas::titulo_da_origem(
        uow.conexao(),
        ctx.empresa,
        d.origem_modulo,
        d.origem_id,
    )?
    .filter(|t| !t.esta_cancelado()) else {
        return Ok(AbatimentoFeito {
            titulo: None,
            abatido: Dinheiro::ZERO,
            credito: None,
            valor_credito: Dinheiro::ZERO,
        });
    };
    if titulo.especie != EspecieTitulo::Pagar {
        return Err(Erro::novo(
            cardeal_kernel::CodigoErro::REGRA_VIOLADA,
            "Só se abate um título a pagar",
        ));
    }
    let (fornecedores, contrapartida) = {
        let repo = RepositorioRazao::novo(uow);
        let contas = Contas::nova(&repo, ctx.empresa);
        (
            contas
                .papel(PapelConta::Fornecedores)
                .map_err(|e| Erro::de_dominio(&e))?,
            contas
                .papel(d.papel_contrapartida)
                .map_err(|e| Erro::de_dominio(&e))?,
        )
    };
    let mut parcelas: Vec<_> = RepositorioFinanceiro::novo(uow)
        .parcelas_do_titulo(titulo.id)?
        .into_iter()
        .filter(|p| matches!(p.estado, EstadoParcela::Aberta | EstadoParcela::Parcial))
        .collect();
    parcelas.sort_by_key(|p| (p.vencimento, p.numero));
    let hoje = ctx.hoje();
    let mut resta = d.valor;
    for mut p in parcelas {
        let saldo = p.valor - p.valor_baixado;
        if !resta.e_positivo() {
            break;
        }
        if !saldo.e_positivo() {
            continue;
        }
        let x = if saldo < resta { saldo } else { resta };
        abater_parcela(
            &mut p,
            x,
            saldo,
            &titulo,
            (fornecedores, contrapartida),
            &d.historico,
            ctx,
            uow,
        )?;
        resta -= x;
    }
    let abatido = d.valor - resta;
    let credito = if resta.e_positivo() {
        Some(
            super::lancar_titulo_comum(
                super::DadosLancamentoTitulo {
                    especie: EspecieTitulo::Receber,
                    contraparte: Some(d.contraparte),
                    valor_total: resta,
                    emissao: hoje,
                    parcelas: 1,
                    primeiro_vencimento: hoje,
                    intervalo_dias: 0,
                    cronograma: None,
                    observacao: Some(format!("Crédito — {}", d.historico)),
                    categoria: None,
                    origem_modulo: d.origem_do_credito,
                    origem_id: Some(d.id_do_credito),
                    papel_contraparte: PapelConta::ClientesAReceber,
                    papel_resultado: d.papel_contrapartida,
                },
                ctx,
                uow,
            )?
            .titulo,
        )
    } else {
        None
    };
    Ok(AbatimentoFeito {
        titulo: Some(titulo.id),
        abatido,
        credito,
        valor_credito: resta,
    })
}

/// Abate `x` de uma parcela: o lançamento D Fornecedores / C contrapartida, a baixa sem
/// dinheiro e o evento.
#[cfg(feature = "sqlite")]
#[allow(clippy::too_many_arguments)]
fn abater_parcela(
    p: &mut crate::titulo::Parcela,
    x: Dinheiro,
    saldo: Dinheiro,
    titulo: &crate::titulo::Titulo,
    (fornecedores, contrapartida): (Id, Id),
    historico: &str,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let hoje = ctx.hoje();
    let lanc = ConstrutorLancamento::novo(
        ctx.empresa,
        hoje,
        format!("{} — abate a parcela {}", historico, p.numero),
    )
    .origem(
        Origem::modulo("financeiro")
            .tipo("titulo")
            .agregado(titulo.id),
    )
    .liquidacao(hoje)
    .criado_por(ctx.usuario, ctx.dispositivo)
    .agora(ctx.agora)
    .debitar(fornecedores, x)
    .contraparte_se(titulo.contraparte)
    .creditar(contrapartida, x)
    .construir()
    .map_err(|_| Erro::de_dominio(&ErroFinanceiro::ValorInvalido))?;
    let lancamento = {
        let mut repo = RepositorioRazao::novo(uow);
        Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
    };
    // Sem dinheiro: o principal sai da dívida, sem juros, multa nem desconto.
    let plano = PlanoBaixa {
        data: hoje,
        valor_recebido: x,
        principal: x,
        juros: Dinheiro::ZERO,
        multa: Dinheiro::ZERO,
        desconto: Dinheiro::ZERO,
        quita: x == saldo,
    };
    p.aplicar_baixa(&plano);
    let baixa = BaixaGravada {
        id: Id::novo(),
        empresa: ctx.empresa,
        parcela: p.id,
        data: hoje,
        valor_recebido: x,
        principal: x,
        juros: Dinheiro::ZERO,
        multa: Dinheiro::ZERO,
        desconto: Dinheiro::ZERO,
        lancamento,
        estornada_em: None,
    };
    {
        let mut repo = RepositorioFinanceiro::novo(uow);
        repo.atualizar_parcela(p)?;
        repo.inserir_baixa(&baixa)?;
    }
    uow.publicar(ParcelaBaixada {
        parcela: p.id,
        titulo: titulo.id,
        valor_recebido: x,
        quitada: plano.quita,
        lancamento,
    })
    .map_err(|e| Erro::de_dominio(&e))?;
    Ok(())
}
