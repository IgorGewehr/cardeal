//! Estorna uma venda já finalizada — o "cancelar venda" do balcão depois do `F2`
//! (`docs/modulos/pdv.md` §5). Desfaz, na mesma transação, as duas pontas que a finalização
//! fez: estorna no Razão o lançamento da venda (pagamentos, receita, desconto e CMV voltam) e
//! devolve ao estoque cada saída do cupom, ao custo em que saiu
//! ([`mod_estoque::devolver_saidas_da_origem`]). O dinheiro sai das mesmas contas por onde
//! entrou: o do caixa reduz o esperado no fechamento, e Pix e cartão saem de "em trânsito" e
//! "cartões a receber", à espera da devolução feita no banco ou na maquininha.
//!
//! A NFC-e não é assunto do PDV: quem chama pede ao `mod-fiscal` que a anule
//! (`AnularDocumentoDaVenda`), e a fila cancela ou inutiliza conforme o estado da nota.
//!
//! Como o `F8`, exige motivo e quem autorizou; a sessão que executa é a do supervisor.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::carregar_cupom;
use crate::cupom::PagamentoCupom;
#[cfg(feature = "sqlite")]
use crate::erros::ErroPdv;
#[cfg(feature = "sqlite")]
use crate::eventos::VendaEstornada;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioPdv;

/// O tamanho mínimo do motivo — o mesmo que o Razão exige no histórico de um estorno.
pub const MOTIVO_MINIMO: usize = 10;

/// Estorna uma venda `Finalizado`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstornarVenda {
    /// O cupom da venda.
    pub cupom: Id,
    /// Por que a venda está sendo desfeita (ao menos [`MOTIVO_MINIMO`] caracteres).
    pub motivo: String,
    /// Quem autorizou.
    pub autorizado_por: Id,
}

/// O que o comando devolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VendaFoiEstornada {
    /// Os lançamentos de estorno gerados no Razão.
    pub lancamentos: Vec<Id>,
    /// Quantas linhas de estoque voltaram.
    pub itens_devolvidos: usize,
    /// O que devolver ao cliente, forma por forma (o dinheiro sai do caixa; Pix e cartão se
    /// devolvem no banco ou na maquininha).
    pub devolver: Vec<PagamentoCupom>,
}

/// Cancela o título a receber da parte a prazo da venda, se houver e não tiver recebimento.
#[cfg(feature = "sqlite")]
fn cancelar_conta_a_prazo(cupom: Id, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
    let Some(mut titulo) =
        mod_financeiro::titulo_da_origem(uow.conexao(), ctx.empresa, "pdv", cupom)?
            .filter(|t| !t.esta_cancelado())
    else {
        return Ok(());
    };
    let mut repo = mod_financeiro::RepositorioFinanceiro::novo(uow);
    let parcelas = repo.parcelas_do_titulo(titulo.id)?;
    if parcelas
        .iter()
        .any(|p| p.valor_baixado.e_positivo() || p.estado != mod_financeiro::EstadoParcela::Aberta)
    {
        return Err(Erro::de_dominio(&ErroPdv::PrazoComRecebimento));
    }
    for mut p in parcelas {
        p.cancelar();
        repo.atualizar_parcela(&p)?;
    }
    titulo.cancelar(ctx.agora);
    repo.atualizar_titulo_cancelado(&titulo)
}

impl Comando for EstornarVenda {
    type Saida = VendaFoiEstornada;
    const PERMISSAO: &'static str = "pdv.venda.estornar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;

        // 2. Validar (domínio puro).
        let motivo = self.motivo.trim().to_owned();
        if motivo.chars().count() < MOTIVO_MINIMO {
            return Err(Erro::de_dominio(&ErroPdv::MotivoDoEstornoCurto));
        }
        cupom.estornar().map_err(|e| Erro::de_dominio(&e))?;

        // 3. Estornar o lançamento da venda — achado pela origem, como o receituário o gravou.
        let historico = format!("Estorno da venda nº {} — {motivo}", cupom.numero_terminal);
        let lancamentos = {
            let mut repo = cardeal_ledger::RepositorioRazao::novo(uow);
            let vivos = repo
                .lancamentos_vivos_da_origem(ctx.empresa, "pdv", "cupom", cupom.id)
                .map_err(|e| Erro::de_dominio(&e))?;
            if vivos.is_empty() {
                return Err(Erro::novo(
                    CodigoErro::ESTADO_INVALIDO,
                    "O lançamento desta venda não foi encontrado no Razão",
                ));
            }
            vivos
                .into_iter()
                .map(|original| {
                    cardeal_ledger::Razao::estornar(&mut repo, original, &historico, ctx.hoje())
                        .map_err(|e| Erro::de_dominio(&e))
                })
                .collect::<Resultado<Vec<_>>>()?
        };

        // 4. A conta a prazo da venda some junto — o lançamento estornado acima já desfez o
        //    Clientes a receber; se o cliente já pagou parte, o recebimento é estornado antes.
        cancelar_conta_a_prazo(cupom.id, ctx, uow)?;

        // 5. Devolver a mercadoria, ao custo em que saiu.
        let devolvidos = mod_estoque::devolver_saidas_da_origem("pdv", cupom.id, ctx, uow)?;

        // 6. Persistir.
        {
            let mut repo = RepositorioPdv::novo(uow);
            repo.atualizar_cupom(&cupom)?;
            repo.inserir_estorno(&cupom, &motivo, self.autorizado_por, ctx.usuario, ctx.agora)?;
        }

        // 7. Publicar.
        uow.publicar(VendaEstornada {
            cupom: cupom.id,
            terminal: cupom.terminal,
            total: cupom.total,
            motivo,
            autorizado_por: self.autorizado_por,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(VendaFoiEstornada {
            lancamentos,
            itens_devolvidos: devolvidos.len(),
            devolver: cupom.pagamentos,
        })
    }
}
