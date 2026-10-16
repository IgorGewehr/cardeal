//! Corrigir um título: **cancelar** o que foi lançado errado e **alterar o vencimento** de uma
//! parcela em aberto (`docs/modulos/financeiro.md` §5).
//!
//! Cancelar nunca apaga: estorna no Razão o lançamento `Confirmado` de cada parcela e marca
//! parcelas e título como cancelados (`cancelado_em`). Parcela cancelada não aceita baixa — a
//! lição do `gestao-raiz`: baixar um título cancelado "ressuscitaria" a receita e mexeria no
//! saldo. Só o título lançado **no financeiro** (avulso ou de recorrência) se cancela aqui; o
//! que veio de compra, venda, OS ou renegociação se desfaz pela origem, senão estoque e
//! contabilidade ficariam desencontrados. Com baixa válida, o cancelamento é recusado: estornar
//! um recebimento ou pagamento de verdade é decisão à parte (`EstornarBaixa`), não efeito
//! colateral.

use cardeal_kernel::{Data, Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::erros::ErroFinanceiro;
#[cfg(feature = "sqlite")]
use crate::eventos::{TituloCancelado, VencimentoAlterado};
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioFinanceiro;
#[cfg(feature = "sqlite")]
use crate::titulo::EstadoParcela;

/// As origens de título que o próprio financeiro lança — as únicas que ele cancela.
#[cfg(feature = "sqlite")]
const ORIGENS_DO_FINANCEIRO: &[&str] = &[
    "avulso",
    "financeiro_recorrencia",
    crate::importacao::ORIGEM_IMPORTACAO,
];

/// De onde veio o título, para a mensagem de recusa.
#[cfg(feature = "sqlite")]
fn origem_por_extenso(origem: &str) -> &'static str {
    match origem {
        "compras" => "da entrada de uma nota de compra",
        "vendas" => "de uma venda",
        "pdv" => "do PDV",
        "os" | "os_peca" => "de uma ordem de serviço",
        "financeiro" => "de uma renegociação",
        _ => "de outro módulo",
    }
}

/// Cancela um título lançado errado, sem baixa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarTitulo {
    /// O título.
    pub titulo: Id,
    /// Por que (ao menos 10 caracteres; vai ao histórico do estorno).
    pub motivo: String,
}

/// O que o cancelamento devolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TituloFoiCancelado {
    /// O valor original do título.
    pub valor: Dinheiro,
    /// Quantas parcelas foram canceladas.
    pub parcelas: usize,
}

impl Comando for CancelarTitulo {
    type Saida = TituloFoiCancelado;
    const PERMISSAO: &'static str = "financeiro.titulo.cancelar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut titulo = RepositorioFinanceiro::novo(uow)
            .buscar_titulo(self.titulo)?
            .ok_or_else(|| Erro::nao_encontrado("título"))?;
        let parcelas = RepositorioFinanceiro::novo(uow).parcelas_do_titulo(titulo.id)?;

        // 2. Validar.
        let motivo = self.motivo.trim().to_owned();
        if motivo.chars().count() < 10 {
            return Err(Erro::de_dominio(&ErroFinanceiro::MotivoDoCancelamentoCurto));
        }
        if titulo.esta_cancelado() {
            return Err(Erro::de_dominio(&ErroFinanceiro::TituloJaCancelado));
        }
        if !ORIGENS_DO_FINANCEIRO.contains(&titulo.origem_modulo.as_str()) {
            return Err(Erro::de_dominio(&ErroFinanceiro::TituloDeOutraOrigem(
                origem_por_extenso(&titulo.origem_modulo),
            )));
        }
        for p in &parcelas {
            match p.estado {
                EstadoParcela::Aberta if p.valor_baixado.e_zero() => {}
                EstadoParcela::Cancelada => {}
                EstadoParcela::Renegociada => {
                    return Err(Erro::de_dominio(&ErroFinanceiro::TituloComMovimento(
                        "parcela renegociada (cancele o título novo da renegociação)",
                    )));
                }
                _ => {
                    return Err(Erro::de_dominio(&ErroFinanceiro::TituloComMovimento(
                        "recebimento ou pagamento registrado",
                    )));
                }
            }
        }

        // 3. Lançar: estorna o `Confirmado` de cada parcela (um lançamento pode cobrir mais
        //    de uma — estorna cada um uma vez).
        let historico = format!("Cancelamento de título — {motivo}");
        let mut estornados: Vec<Id> = Vec::new();
        for lancamento in parcelas
            .iter()
            .filter(|p| p.estado == EstadoParcela::Aberta)
            .filter_map(|p| p.lancamento)
        {
            if estornados.contains(&lancamento) {
                continue;
            }
            let mut repo = cardeal_ledger::RepositorioRazao::novo(uow);
            cardeal_ledger::Razao::estornar(&mut repo, lancamento, &historico, ctx.hoje())
                .map_err(|e| Erro::de_dominio(&e))?;
            estornados.push(lancamento);
        }

        // 4. Persistir — nunca apaga.
        let mut canceladas = 0;
        {
            let mut repo = RepositorioFinanceiro::novo(uow);
            for mut p in parcelas {
                if p.estado == EstadoParcela::Aberta {
                    p.cancelar();
                    repo.atualizar_parcela(&p)?;
                    canceladas += 1;
                }
            }
            titulo.cancelar(ctx.agora);
            repo.atualizar_titulo_cancelado(&titulo)?;
        }

        // 5. Publicar.
        uow.publicar(TituloCancelado {
            titulo: titulo.id,
            valor: titulo.valor_original,
            motivo,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(TituloFoiCancelado {
            valor: titulo.valor_original,
            parcelas: canceladas,
        })
    }
}

/// Muda o vencimento de uma parcela em aberto (ou com baixa parcial) — o fornecedor que
/// prorrogou, o cliente que combinou outra data. Vale para título de qualquer origem: o
/// vencimento é da cobrança, não do fato que gerou o título.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AlterarVencimento {
    /// A parcela.
    pub parcela: Id,
    /// O novo vencimento.
    pub vencimento: Data,
}

impl Comando for AlterarVencimento {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.titulo.editar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        // 1. Carregar.
        let mut parcela = RepositorioFinanceiro::novo(uow)
            .buscar_parcela(self.parcela)?
            .ok_or_else(|| Erro::nao_encontrado("parcela"))?;
        let titulo = RepositorioFinanceiro::novo(uow)
            .buscar_titulo(parcela.titulo)?
            .ok_or_else(|| Erro::nao_encontrado("título"))?;

        // 2. Validar.
        if titulo.esta_cancelado() || !parcela.estado.aceita_baixa() {
            return Err(Erro::de_dominio(&ErroFinanceiro::ParcelaNaoBaixavel(
                parcela.estado,
            )));
        }
        if self.vencimento < titulo.emissao {
            return Err(Erro::de_dominio(&ErroFinanceiro::VencimentoAntesDaEmissao(
                titulo.emissao,
            )));
        }
        if self.vencimento == parcela.vencimento {
            return Ok(());
        }
        let anterior = parcela.vencimento;
        parcela.vencimento = self.vencimento;
        parcela.versao = parcela.versao.proxima();

        // 4. Persistir.
        RepositorioFinanceiro::novo(uow).atualizar_vencimento(&parcela)?;

        // 5. Publicar.
        uow.publicar(VencimentoAlterado {
            parcela: parcela.id,
            de: anterior,
            para: parcela.vencimento,
        })
        .map_err(|e| Erro::de_dominio(&e))
    }
}
