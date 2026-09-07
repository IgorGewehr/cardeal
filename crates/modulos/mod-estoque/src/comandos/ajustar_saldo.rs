//! Corrige manualmente o saldo disponível de um produto num local, fora do fluxo formal de
//! inventário (`docs/modulos/estoque.md` §5, `AjustarSaldo`) — o caminho que faltava para
//! consertar um erro de digitação (quantidade contada/lançada errada) sem precisar abrir e
//! encerrar um [`crate::Inventario`] inteiro por causa de um único item.
//!
//! Auditoria de produção de 2026-09-06: o domínio de correção (`SaldoLocal::ajustar`,
//! `receituario::ajuste_manual`) já existia, mas nenhum comando o expunha — não havia jeito
//! de corrigir um saldo errado sem editar o SQLite direto.

use cardeal_kernel::{Arredondamento, Dinheiro, Erro, Id, Quantidade, Resultado};
use cardeal_ledger::{Contas, PapelConta, Razao, RepositorioRazao};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::autoria_de;
use crate::erros::ErroEstoque;
use crate::inventario::AjusteInventario;
use crate::receituario::{ajuste_manual, ContasEstoque};
use crate::repositorio::RepositorioEstoque;
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

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar o motivo antes de tocar em qualquer estado (§11.8).
        if self.motivo.trim().chars().count() < 10 {
            return Err(Erro::de_dominio(&ErroEstoque::MotivoObrigatorio));
        }

        // 2. Carregar (ou criar) o saldo.
        let existente = RepositorioEstoque::novo(uow).buscar_saldo(self.produto, self.local)?;
        let saldo_e_novo = existente.is_none();
        let mut saldo = existente.unwrap_or_else(|| {
            SaldoLocal::zerado(ctx.empresa, self.produto, None, self.local, ctx.agora)
        });

        // 3. Validar (domínio puro): aplica o delta ao disponível.
        let delta = self.nova_quantidade - saldo.quantidade_disponivel;
        if delta.e_zero() {
            return Err(Erro::de_dominio(&ErroEstoque::QuantidadeInvalida));
        }
        let custo_medio = saldo.custo_medio;
        saldo.ajustar(delta, ctx.agora);

        // 4. Persistir: saldo, movimento e, se houver valor, o lançamento no razão.
        let mut repo = RepositorioEstoque::novo(uow);
        if saldo_e_novo {
            repo.inserir_saldo(&saldo)?;
        } else {
            repo.atualizar_saldo(&saldo)?;
        }
        let movimento = Movimento {
            id: Id::novo(),
            empresa: ctx.empresa,
            produto: self.produto,
            variacao: None,
            local: self.local,
            tipo: if delta.e_positiva() {
                TipoMovimento::AjustePositivo
            } else {
                TipoMovimento::AjusteNegativo
            },
            quantidade: delta.abs(),
            custo_unitario: None,
            lote: None,
            origem_modulo: "estoque".to_string(),
            origem_id: None,
            lancamento: None,
            criado_em: ctx.agora,
            criado_por: ctx.usuario,
        };
        repo.inserir_movimento(&movimento)?;

        let valor = Dinheiro::de_total(delta.abs(), custo_medio, Arredondamento::MeioAcima);
        let ajuste = AjusteInventario {
            produto: self.produto,
            variacao: None,
            lote: None,
            delta,
        };
        let contas = {
            let repo_razao = RepositorioRazao::novo(uow);
            let resolvedor = Contas::nova(&repo_razao, ctx.empresa);
            ContasEstoque {
                estoque: resolvedor
                    .papel(PapelConta::EstoqueMercadorias)
                    .map_err(|e| Erro::de_dominio(&e))?,
                perdas: resolvedor
                    .papel(PapelConta::Perdas)
                    .map_err(|e| Erro::de_dominio(&e))?,
                outras_receitas: resolvedor
                    .papel(PapelConta::OutrasReceitas)
                    .map_err(|e| Erro::de_dominio(&e))?,
            }
        };
        let lancamento_balanceado = ajuste_manual(
            ctx.empresa,
            movimento.id,
            &ajuste,
            valor,
            contas,
            ctx.hoje(),
            autoria_de(ctx),
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        let lancamento = match lancamento_balanceado {
            Some(lb) => {
                let mut repo_razao = RepositorioRazao::novo(uow);
                Some(Razao::registrar(&mut repo_razao, lb).map_err(|e| Erro::de_dominio(&e))?)
            }
            None => None,
        };

        Ok(SaldoAjustado {
            movimento: movimento.id,
            delta,
            lancamento,
        })
    }
}
