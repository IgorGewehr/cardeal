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
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct EntradaConfirmada {
    /// O título a pagar gerado, se algum.
    pub titulo: Option<Id>,
}

impl Comando for ConfirmarEntrada {
    type Saida = EntradaConfirmada;
    const PERMISSAO: &'static str = "compras.entrada.confirmar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let gerar_titulo = match self.gerar_titulo_a_pagar {
            Some(g) => g,
            None => {
                RepositorioCompras::novo(uow)
                    .preferencias(ctx.empresa)?
                    .gera_titulo_a_pagar
            }
        };
        let titulo =
            confirmar_entrada_comum(self.nota_entrada, self.local, gerar_titulo, ctx, uow)?;
        Ok(EntradaConfirmada { titulo })
    }
}
