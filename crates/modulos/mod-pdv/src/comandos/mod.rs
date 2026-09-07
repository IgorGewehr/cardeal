//! Os comandos de PDV (`docs/modulos/pdv.md` §5). Um arquivo, um comando, nas cinco etapas de
//! `docs/15-convencoes-codigo.md` §3. TEF, balança, faixa por lote e diário durável ficam
//! para depois (ver `src/lib.rs`).

mod abrir_cupom;
mod adicionar_item;
mod aplicar_desconto_item;
mod cancelar_cupom;
mod cancelar_item;
mod finalizar_venda;

pub use abrir_cupom::{AbrirCupom, CupomAberto};
pub use adicionar_item::{AdicionarItem, ItemFoiAdicionado};
pub use aplicar_desconto_item::AplicarDescontoItem;
pub use cancelar_cupom::CancelarCupom;
pub use cancelar_item::CancelarItem;
pub use finalizar_venda::{FinalizarVenda, PagamentoInformado, VendaFoiFinalizada};

use cardeal_auth::ValorLimite;
use cardeal_kernel::{Erro, Id, Percentual, Resultado};
use cardeal_ledger::PapelConta;
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::cupom::{Cupom, FormaPagamentoPdv};
use crate::repositorio::RepositorioPdv;

/// Carrega um cupom ou devolve `NAO_ENCONTRADO`. Usado por todo comando que opera sobre um
/// cupom já existente.
pub(crate) fn carregar_cupom(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<Cupom> {
    RepositorioPdv::novo(uow)
        .buscar_cupom(id)?
        .ok_or_else(|| Erro::nao_encontrado("cupom"))
}

/// A [`crate::receituario::Autoria`] extraída de um [`Ctx`] de comando.
pub(crate) fn autoria_de(ctx: &Ctx) -> crate::receituario::Autoria {
    crate::receituario::Autoria {
        usuario: ctx.usuario,
        dispositivo: ctx.dispositivo,
        agora: ctx.agora,
    }
}

/// O teto de desconto do papel do operador (`vendas.desconto_maximo` — a mesma chave de
/// `mod-vendas`, per `docs/modulos/pdv.md` §5: "valida contra `vendas.desconto_maximo`"; o PDV
/// não duplica um limite que já pertence ao papel comercial do operador). Sem limite
/// configurado, o teto é zero — desconto exige um limite explícito.
pub(crate) fn limite_desconto(ctx: &Ctx) -> Percentual {
    match ctx.limite("vendas.desconto_maximo") {
        Some(ValorLimite::Percentual(p)) => p,
        Some(ValorLimite::Ilimitado) => Percentual::CEM,
        _ => Percentual::ZERO,
    }
}

/// A conta do Razão que uma forma de pagamento do PDV resolve, por papel semântico
/// (`docs/modulos/pdv.md` §7). `Pix` vai para `ValoresEmTransito` — o papel cuja própria
/// descrição em `cardeal-ledger` é "cartão a compensar, Pix a liquidar" — até existir
/// conciliação bancária que o mova para `Bancos`.
pub(crate) const fn papel_da_forma(forma: FormaPagamentoPdv) -> PapelConta {
    match forma {
        FormaPagamentoPdv::Dinheiro => PapelConta::Caixa,
        FormaPagamentoPdv::Pix => PapelConta::ValoresEmTransito,
        FormaPagamentoPdv::Debito | FormaPagamentoPdv::Credito => PapelConta::CartoesAReceber,
    }
}
