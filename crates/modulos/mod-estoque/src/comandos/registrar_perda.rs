//! Dá baixa de uma perda — vencimento, quebra, furto (`docs/modulos/estoque.md` §4 "Vencido →
//! Esgotado: baixado por `RegistrarPerda`", §7 "Perda por vencimento/quebra"): sai do saldo do
//! local (e do lote, quando a perda é de um lote), grava o movimento `Perda` e lança
//! D Perdas / C Estoque ao **custo médio** vigente — o mesmo de toda saída (§11 regra 1, e o CMV
//! da venda), para o saldo contábil do estoque continuar batendo com saldo × custo médio. O
//! custo do lote é informativo (o que aquela remessa custou), não entra no lançamento.
//!
//! Diferente da venda, a perda não deixa o saldo negativo: declarar perda do que o sistema não
//! tem é erro de digitação (a divergência real se acerta no inventário).

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Arredondamento, Dinheiro, Erro, Resultado};
use cardeal_kernel::{Id, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_ledger::{Razao, RepositorioRazao};
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
use crate::produto::EstadoLote;
#[cfg(feature = "sqlite")]
use crate::receituario::perda;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::saldo::{Movimento, TipoMovimento};

/// Por que se perdeu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MotivoPerda {
    /// Passou da validade.
    Vencimento,
    /// Quebrou, estragou, avariou.
    Quebra,
    /// Furto ou extravio.
    Furto,
    /// Outro (a observação explica).
    Outro,
}

impl MotivoPerda {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Vencimento => "vencimento",
            Self::Quebra => "quebra/avaria",
            Self::Furto => "furto/extravio",
            Self::Outro => "outro motivo",
        }
    }
}

/// Registra uma perda.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarPerda {
    /// O produto.
    pub produto: Id,
    /// O local de onde sai.
    pub local: Id,
    /// O lote perdido, quando a perda é de um lote (vencimento costuma ser).
    pub lote: Option<Id>,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O motivo.
    pub motivo: MotivoPerda,
    /// Uma observação livre (vai para o histórico do lançamento).
    pub observacao: String,
}

/// O que a baixa fez.
#[derive(Debug, Serialize, Deserialize)]
pub struct PerdaRegistrada {
    /// O movimento.
    pub movimento: Id,
    /// O valor da perda, ao custo.
    pub valor: cardeal_kernel::Dinheiro,
    /// O lançamento no razão (`None` sem custo conhecido).
    pub lancamento: Option<Id>,
    /// `true` quando o lote acabou com esta baixa.
    pub lote_esgotado: bool,
}

impl Comando for RegistrarPerda {
    type Saida = PerdaRegistrada;
    const PERMISSAO: &'static str = "estoque.movimento.perda";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        if !self.quantidade.e_positiva() {
            return Err(Erro::de_dominio(&ErroEstoque::QuantidadeInvalida));
        }
        crate::repositorio_inventario::exigir_local_livre(uow.conexao(), self.local)?;
        let mut repo = RepositorioEstoque::novo(uow);

        // O lote, quando informado: tem que ser deste produto e local, e ter o que se perde.
        let lote = match self.lote {
            Some(id) => Some(lote_para_perda(
                &repo,
                id,
                self.produto,
                self.local,
                self.quantidade,
            )?),
            None => None,
        };

        let mut saldo = repo
            .buscar_saldo(self.produto, self.local)?
            .ok_or_else(|| {
                Erro::de_dominio(&ErroEstoque::PerdaAcimaDoSaldo {
                    disponivel: Quantidade::ZERO.to_string(),
                    pedido: self.quantidade.to_string(),
                })
            })?;
        if saldo.quantidade_disponivel < self.quantidade {
            return Err(Erro::de_dominio(&ErroEstoque::PerdaAcimaDoSaldo {
                disponivel: saldo.quantidade_disponivel.to_string(),
                pedido: self.quantidade.to_string(),
            }));
        }
        let custo = saldo.custo_medio;
        saldo
            .saida(self.quantidade, ctx.agora)
            .map_err(|e| Erro::de_dominio(&e))?;
        repo.atualizar_saldo(&saldo)?;
        let movimento = Movimento {
            id: Id::novo(),
            empresa: ctx.empresa,
            produto: self.produto,
            variacao: None,
            local: self.local,
            tipo: TipoMovimento::Perda,
            quantidade: self.quantidade,
            custo_unitario: Some(custo),
            lote: self.lote,
            origem_modulo: "estoque".to_string(),
            origem_id: None,
            lancamento: None,
            criado_em: ctx.agora,
            criado_por: ctx.usuario,
        };
        repo.inserir_movimento(&movimento)?;
        let lote_esgotado = match &lote {
            Some((l, saldo_lote)) if *saldo_lote == self.quantidade => {
                repo.atualizar_estado_lote(l.id, EstadoLote::Esgotado)?;
                true
            }
            _ => false,
        };

        // O lançamento: D Perdas / C Estoque, com o motivo no histórico.
        let valor = Dinheiro::de_total(self.quantidade, custo, Arredondamento::MeioAcima);
        let lancamento = if valor.e_positivo() {
            let contas = crate::comandos::ajustar_saldo::contas_de_estoque(ctx, uow)?;
            let motivo = match self.observacao.trim() {
                "" => self.motivo.rotulo().to_owned(),
                obs => format!("{}: {obs}", self.motivo.rotulo()),
            };
            let balanceado = perda(
                ctx.empresa,
                movimento.id,
                self.quantidade,
                valor,
                contas,
                ctx.hoje(),
                autoria_de(ctx),
                &motivo,
            )
            .map_err(|e| Erro::de_dominio(&e))?;
            let mut repo_razao = RepositorioRazao::novo(uow);
            Some(Razao::registrar(&mut repo_razao, balanceado).map_err(|e| Erro::de_dominio(&e))?)
        } else {
            None
        };
        Ok(PerdaRegistrada {
            movimento: movimento.id,
            valor,
            lancamento,
            lote_esgotado,
        })
    }
}

/// O lote da perda: deste produto e local, e com o que se perde.
#[cfg(feature = "sqlite")]
fn lote_para_perda(
    repo: &RepositorioEstoque,
    id: Id,
    produto: Id,
    local: Id,
    quantidade: Quantidade,
) -> Resultado<(crate::produto::Lote, Quantidade)> {
    let lote = repo
        .buscar_lote(id)?
        .ok_or_else(|| Erro::de_dominio(&ErroEstoque::LoteInexistente))?;
    if lote.produto != produto || lote.local != local {
        return Err(Erro::de_dominio(&ErroEstoque::LoteDeOutroProduto));
    }
    let saldo_lote = repo.saldo_do_lote(id)?;
    if saldo_lote < quantidade {
        return Err(Erro::de_dominio(&ErroEstoque::LoteSaldoInsuficiente {
            disponivel: saldo_lote.to_string(),
            pedido: quantidade.to_string(),
        }));
    }
    Ok((lote, saldo_lote))
}
