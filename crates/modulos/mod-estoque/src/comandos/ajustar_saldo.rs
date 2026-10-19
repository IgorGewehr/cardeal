//! Corrige manualmente o saldo disponível de um produto num local, fora do fluxo formal de
//! inventário (`docs/modulos/estoque.md` §5, `AjustarSaldo`) — o caminho que faltava para
//! consertar um erro de digitação (quantidade contada/lançada errada) sem precisar abrir e
//! encerrar um [`crate::Inventario`] inteiro por causa de um único item.
//!
//! Auditoria de produção de 2026-09-06: o domínio de correção (`SaldoLocal::ajustar`,
//! `receituario::ajuste_manual`) já existia, mas nenhum comando o expunha — não havia jeito
//! de corrigir um saldo errado sem editar o SQLite direto.
//!
//! [`aplicar_delta_de_saldo`] é o corpo comum de qualquer ajuste de quantidade (este comando e
//! o encerramento do inventário): saldo, movimento e lançamento no razão.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Arredondamento, Dinheiro, Erro, Resultado};
use cardeal_kernel::{Id, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_ledger::RepositorioRazao;
#[cfg(feature = "sqlite")]
use cardeal_ledger::{Contas, PapelConta, Razao};
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
use crate::erros::ErroEstoque;
#[cfg(feature = "sqlite")]
use crate::inventario::AjusteInventario;
#[cfg(feature = "sqlite")]
use crate::receituario::{ajuste_de_inventario, ajuste_manual, ContasEstoque};
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::saldo::{Movimento, SaldoLocal, TipoMovimento};

/// Corrige o saldo disponível de um produto num local para `nova_quantidade`, exigindo um
/// motivo (§11.8).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AjustarSaldo {
    /// O produto.
    pub produto: Id,
    /// O local ajustado.
    pub local: Id,
    /// A quantidade correta, apurada por contagem.
    pub nova_quantidade: Quantidade,
    /// Por que o saldo estava errado — no mínimo 10 caracteres (§11.8).
    pub motivo: String,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct SaldoAjustado {
    /// O movimento criado.
    pub movimento: Id,
    /// `nova_quantidade − quantidade anterior`.
    pub delta: Quantidade,
    /// O lançamento no razão, se o delta e o custo médio geraram valor a lançar.
    pub lancamento: Option<Id>,
}

impl Comando for AjustarSaldo {
    type Saida = SaldoAjustado;
    const PERMISSAO: &'static str = "estoque.movimento.ajustar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar o motivo antes de tocar em qualquer estado (§11.8).
        if self.motivo.trim().chars().count() < 10 {
            return Err(Erro::de_dominio(&ErroEstoque::MotivoObrigatorio));
        }
        // Inventário em andamento trava o local (§11.4) — inclusive para o ajuste avulso.
        crate::repositorio_inventario::exigir_local_livre(uow.conexao(), self.local)?;

        let atual = RepositorioEstoque::novo(uow)
            .buscar_saldo(self.produto, self.local)?
            .map_or(Quantidade::ZERO, |s| s.quantidade_disponivel);
        let delta = self.nova_quantidade - atual;
        if delta.e_zero() {
            return Err(Erro::de_dominio(&ErroEstoque::QuantidadeInvalida));
        }
        let aplicado = aplicar_delta_de_saldo(
            self.produto,
            self.local,
            delta,
            OrigemDoAjuste::Manual,
            ctx,
            uow,
        )?;
        Ok(SaldoAjustado {
            movimento: aplicado.movimento,
            delta,
            lancamento: aplicado.lancamento,
        })
    }
}

/// De onde vem um ajuste de quantidade — decide a origem do movimento e o histórico do
/// lançamento.
#[cfg(feature = "sqlite")]
#[derive(Debug, Clone, Copy)]
pub(crate) enum OrigemDoAjuste {
    /// `AjustarSaldo`: correção avulsa.
    Manual,
    /// O encerramento deste inventário.
    Inventario(Id),
}

/// O que um ajuste gravou.
#[cfg(feature = "sqlite")]
pub(crate) struct DeltaAplicado {
    /// O movimento.
    pub movimento: Id,
    /// O lançamento no razão, quando houve valor.
    pub lancamento: Option<Id>,
    /// O valor do ajuste (|delta| × custo médio).
    pub valor: Dinheiro,
}

/// Soma `delta` ao disponível do produto no local (criando o saldo se não existir), grava o
/// movimento `AjustePositivo`/`AjusteNegativo` e lança a sobra ou a falta no razão ao custo
/// médio vigente (D Estoque/C Outras receitas na sobra, D Perdas/C Estoque na falta).
///
/// # Errors
/// Erro de persistência, ou o razão sem as contas de estoque/perdas/outras receitas.
#[cfg(feature = "sqlite")]
pub(crate) fn aplicar_delta_de_saldo(
    produto: Id,
    local: Id,
    delta: Quantidade,
    origem: OrigemDoAjuste,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<DeltaAplicado> {
    // 1. Carregar (ou criar) o saldo e aplicar o delta (domínio puro).
    let existente = RepositorioEstoque::novo(uow).buscar_saldo(produto, local)?;
    let saldo_e_novo = existente.is_none();
    let mut saldo = existente
        .unwrap_or_else(|| SaldoLocal::zerado(ctx.empresa, produto, None, local, ctx.agora));
    let custo_medio = saldo.custo_medio;
    saldo.ajustar(delta, ctx.agora);

    // 2. Persistir: saldo e movimento.
    let mut repo = RepositorioEstoque::novo(uow);
    if saldo_e_novo {
        repo.inserir_saldo(&saldo)?;
    } else {
        repo.atualizar_saldo(&saldo)?;
    }
    let (origem_modulo, origem_id) = match origem {
        OrigemDoAjuste::Manual => ("estoque", None),
        OrigemDoAjuste::Inventario(i) => ("inventario", Some(i)),
    };
    let movimento = Movimento {
        id: Id::novo(),
        empresa: ctx.empresa,
        produto,
        variacao: None,
        local,
        tipo: if delta.e_positiva() {
            TipoMovimento::AjustePositivo
        } else {
            TipoMovimento::AjusteNegativo
        },
        quantidade: delta.abs(),
        custo_unitario: None,
        lote: None,
        origem_modulo: origem_modulo.to_string(),
        origem_id,
        lancamento: None,
        criado_em: ctx.agora,
        criado_por: ctx.usuario,
    };
    repo.inserir_movimento(&movimento)?;

    // 3. O lançamento da sobra ou da falta, ao custo médio.
    let valor = Dinheiro::de_total(delta.abs(), custo_medio, Arredondamento::MeioAcima);
    let ajuste = AjusteInventario {
        produto,
        variacao: None,
        lote: None,
        delta,
    };
    let contas = contas_de_estoque(ctx, uow)?;
    let balanceado = match origem {
        OrigemDoAjuste::Manual => ajuste_manual(
            ctx.empresa,
            movimento.id,
            &ajuste,
            valor,
            contas,
            ctx.hoje(),
            autoria_de(ctx),
        ),
        OrigemDoAjuste::Inventario(i) => ajuste_de_inventario(
            ctx.empresa,
            i,
            &ajuste,
            valor,
            contas,
            ctx.hoje(),
            autoria_de(ctx),
        ),
    }
    .map_err(|e| Erro::de_dominio(&e))?;
    let lancamento = match balanceado {
        Some(lb) => {
            let mut repo_razao = RepositorioRazao::novo(uow);
            Some(Razao::registrar(&mut repo_razao, lb).map_err(|e| Erro::de_dominio(&e))?)
        }
        None => None,
    };
    Ok(DeltaAplicado {
        movimento: movimento.id,
        lancamento,
        valor,
    })
}

/// As contas que os lançamentos de estoque usam: estoque, perdas e outras receitas.
///
/// # Errors
/// O razão sem alguma das três contas.
#[cfg(feature = "sqlite")]
pub(crate) fn contas_de_estoque(
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ContasEstoque> {
    let repo_razao = RepositorioRazao::novo(uow);
    let resolvedor = Contas::nova(&repo_razao, ctx.empresa);
    Ok(ContasEstoque {
        estoque: resolvedor
            .papel(PapelConta::EstoqueMercadorias)
            .map_err(|e| Erro::de_dominio(&e))?,
        perdas: resolvedor
            .papel(PapelConta::Perdas)
            .map_err(|e| Erro::de_dominio(&e))?,
        outras_receitas: resolvedor
            .papel(PapelConta::OutrasReceitas)
            .map_err(|e| Erro::de_dominio(&e))?,
    })
}
