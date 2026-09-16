//! Confirma a entrada de uma nota de compra — o comando de despacho fino sobre
//! [`super::confirmar_entrada_comum`] (o corpo é compartilhado com a confirmação automática
//! disparada por [`super::importar_nota_da_sefaz`]).

use cardeal_kernel::{Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::confirmar_entrada_comum;
use crate::repositorio::RepositorioCompras;

/// Confirma a entrada de uma nota com todos os itens já casados.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConfirmarEntrada {
    /// A nota de entrada.
    pub nota_entrada: Id,
    /// O local de estoque que recebe.
    pub local: Id,
    /// Gerar o título a pagar agora — `None` usa a preferência da empresa.
    pub gerar_titulo_a_pagar: Option<bool>,
    /// A compra já foi paga à vista, na hora (dinheiro, cartão, PIX na entrega) — se
    /// verdadeiro e um título foi gerado, a mesma transação já dá baixa completa nele em vez
    /// de deixá-lo pendente no Contas a Pagar. `None` usa a preferência da empresa
    /// (`PreferenciasCompras::pago_no_ato_padrao`). Sem efeito se nenhum título for gerado.
    pub pago_no_ato: Option<bool>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct EntradaConfirmada {
    /// O título a pagar gerado, se algum.
    pub titulo: Option<Id>,
    /// Verdadeiro se `titulo` já nasceu com baixa completa (`pago_no_ato`). Sempre `false`
    /// quando `titulo` é `None`.
    pub pago: bool,
}

impl Comando for ConfirmarEntrada {
    type Saida = EntradaConfirmada;
    const PERMISSAO: &'static str = "compras.entrada.confirmar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let preferencias = RepositorioCompras::novo(uow).preferencias(ctx.empresa)?;
        let gerar_titulo = self
            .gerar_titulo_a_pagar
            .unwrap_or(preferencias.gera_titulo_a_pagar);
        let pago_no_ato = self.pago_no_ato.unwrap_or(preferencias.pago_no_ato_padrao);
        let confirmacao = confirmar_entrada_comum(
            self.nota_entrada,
            self.local,
            gerar_titulo,
            pago_no_ato,
            ctx,
            uow,
        )?;
        Ok(EntradaConfirmada {
            titulo: confirmacao.titulo,
            pago: confirmacao.pago,
        })
    }
}
