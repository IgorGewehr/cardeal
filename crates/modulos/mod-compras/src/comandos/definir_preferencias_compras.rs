//! Define as preferências de importação de nota da empresa.
//!
//! Decisão desta sessão, não do spec original — ver `crate::preferencias`.

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::preferencias::{PreferenciasCompras, RateioPor};
use crate::repositorio::RepositorioCompras;

/// Define as preferências de importação de nota.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirPreferenciasCompras {
    /// Confirmar sozinho quando todos os itens da nota casarem por regra aprendida.
    pub confirma_automaticamente_quando_tudo_casa: bool,
    /// Gerar o título a pagar junto da confirmação.
    pub gera_titulo_a_pagar: bool,
    /// Como ratear frete/seguro/outras despesas entre os itens.
    pub rateio_por: RateioPor,
    /// O local de estoque para confirmação automática — obrigatório se
    /// `confirma_automaticamente_quando_tudo_casa` for `true`.
    pub local_padrao: Option<Id>,
}

impl Comando for DefinirPreferenciasCompras {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.entrada.preferencias";
    const RISCO: Risco = Risco::Medio;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        if self.confirma_automaticamente_quando_tudo_casa && self.local_padrao.is_none() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "confirmação automática exige um local de estoque padrão",
            ));
        }
        let preferencias = PreferenciasCompras {
            empresa: ctx.empresa,
            confirma_automaticamente_quando_tudo_casa: self
                .confirma_automaticamente_quando_tudo_casa,
            gera_titulo_a_pagar: self.gera_titulo_a_pagar,
            rateio_por: self.rateio_por,
            local_padrao: self.local_padrao,
        };
        RepositorioCompras::novo(uow).definir_preferencias(&preferencias)?;
        Ok(())
    }
}
