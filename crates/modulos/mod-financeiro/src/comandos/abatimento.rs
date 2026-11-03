//! Abater o título de uma origem — a devolução, que diminui o que se deve pela nota ou pelo
//! pedido: a de compra abate a conta a pagar (`docs/modulos/compras.md` §7: D Fornecedores /
//! C Estoque); a de venda, a conta a receber (D Devoluções de venda / C Clientes a receber). Não
//! é pagamento: nenhum dinheiro anda (a baixa sai com `valor_recebido` zero — os relatórios de
//! pago e recebido não a contam), e não há juros nem multa, mesmo com a parcela vencida.
//!
//! O valor abate as parcelas em aberto da que vence primeiro em diante (o "abate no próximo
//! boleto" de sempre). O que sobra — a conta já paga, ou o que falta não cobre — vira um
//! **crédito com a contraparte**, um título da espécie oposta: a receber do fornecedor (D Clientes
//! a receber / C contrapartida), a pagar ao cliente (D contrapartida / C Fornecedores). Baixa-se
//! quando o dinheiro volta. Fica nas contas padrão da baixa, não em Adiantamentos (1.2.04/2.1.x,
//! o que a spec diz): a baixa do financeiro usa sempre o papel padrão da espécie, e um título em
//! outra conta deixaria as duas erradas na baixa.
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
/// [`ErroFinanceiro::ValorInvalido`] para valor não positivo; os erros do Razão e de
/// [`super::lancar_titulo_comum`].
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
    // A conta da dívida que se abate: Fornecedores (a pagar) ou Clientes a receber.
    let (da_divida, contrapartida) = {
        let repo = RepositorioRazao::novo(uow);
        let contas = Contas::nova(&repo, ctx.empresa);
        (
            contas
                .papel(match titulo.especie {
                    EspecieTitulo::Pagar => PapelConta::Fornecedores,
                    EspecieTitulo::Receber => PapelConta::ClientesAReceber,
                })
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
            (da_divida, contrapartida),
            &d.historico,
            ctx,
            uow,
        )?;
        resta -= x;
    }
    let abatido = d.valor - resta;
    let credito = if resta.e_positivo() {
        Some(lancar_credito(d, titulo.especie, resta, ctx, uow)?)
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

/// Abate `x` de uma parcela: o lançamento (a pagar: D Fornecedores / C contrapartida; a
/// receber: D contrapartida / C Clientes a receber), a baixa sem dinheiro e o evento.
#[cfg(feature = "sqlite")]
#[allow(clippy::too_many_arguments)]
fn abater_parcela(
    p: &mut crate::titulo::Parcela,
    x: Dinheiro,
    saldo: Dinheiro,
    titulo: &crate::titulo::Titulo,
    (da_divida, contrapartida): (Id, Id),
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
    .agora(ctx.agora);
    let lanc = match titulo.especie {
        EspecieTitulo::Pagar => lanc
            .debitar(da_divida, x)
            .contraparte_se(titulo.contraparte)
            .creditar(contrapartida, x),
        EspecieTitulo::Receber => lanc
            .debitar(contrapartida, x)
            .creditar(da_divida, x)
            .contraparte_se(titulo.contraparte),
    }
    .construir()
    .map_err(|_| Erro::de_dominio(&ErroFinanceiro::ValorInvalido))?;
    let lancamento = {
        let mut repo = RepositorioRazao::novo(uow);
        Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?
    };
    // Sem dinheiro: o principal sai da dívida, sem juros, multa nem desconto.
    let plano = PlanoBaixa {
        data: hoje,
        valor_recebido: Dinheiro::ZERO,
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
        valor_recebido: Dinheiro::ZERO,
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
        valor_recebido: Dinheiro::ZERO,
        quitada: plano.quita,
        lancamento,
    })
    .map_err(|e| Erro::de_dominio(&e))?;
    Ok(())
}

/// O crédito com a contraparte: o que sobrou, num título da espécie oposta.
#[cfg(feature = "sqlite")]
fn lancar_credito(
    d: &DadosAbatimento<'_>,
    especie: EspecieTitulo,
    resta: Dinheiro,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Id> {
    let hoje = ctx.hoje();
    let titulo = especie;
    super::lancar_titulo_comum(
        super::DadosLancamentoTitulo {
            especie: match titulo {
                EspecieTitulo::Pagar => EspecieTitulo::Receber,
                EspecieTitulo::Receber => EspecieTitulo::Pagar,
            },
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
            papel_contraparte: match titulo {
                EspecieTitulo::Pagar => PapelConta::ClientesAReceber,
                EspecieTitulo::Receber => PapelConta::Fornecedores,
            },
            papel_resultado: d.papel_contrapartida,
        },
        ctx,
        uow,
    )
    .map(|g| g.titulo)
}
