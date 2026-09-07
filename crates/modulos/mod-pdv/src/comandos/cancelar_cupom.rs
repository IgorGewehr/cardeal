//! Cancela o cupom inteiro (`F8`) — sempre exige motivo e identificação de quem autorizou.
//! `docs/modulos/pdv.md` §5 e §11 regra 3.
//!
//! A checagem de senha/crachá do supervisor em si é responsabilidade da UI/sessão (como
//! `AprovarOrcamentoOs::identificacao_aprovador` em `mod-os`): o comando registra **quem**
//! autorizou, não reautentica — o despacho já exige a permissão `pdv.cupom.cancelar` (Alto)
//! de quem está executando, e a autenticação do supervisor acontece antes de chegar aqui.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_cupom;
use crate::erros::ErroPdv;
use crate::eventos::CupomCancelado;
use crate::repositorio::RepositorioPdv;

/// Cancela um cupom `EmAndamento`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarCupom {
    /// O cupom.
    pub cupom: Id,
    /// O motivo — obrigatório.
    pub motivo: String,
    /// Quem autorizou o cancelamento.
    pub autorizado_por: Id,
}

impl Comando for CancelarCupom {
    type Saida = ();
    const PERMISSAO: &'static str = "pdv.cupom.cancelar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut cupom = carregar_cupom(uow, self.cupom)?;

        // 2. Validar (domínio puro).
        if self.motivo.trim().is_empty() {
            return Err(Erro::de_dominio(&ErroPdv::MotivoObrigatorio));
        }
        cupom.cancelar().map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioPdv::novo(uow).atualizar_cupom(&cupom)?;

        // 5. Publicar.
        uow.publicar(CupomCancelado {
            cupom: cupom.id,
            motivo: self.motivo,
            autorizado_por: self.autorizado_por,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(())
    }
}
