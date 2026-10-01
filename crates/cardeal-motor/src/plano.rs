//! O [`Plano`]: tudo o que depende só da distribuição (quais módulos, quais submódulos) e não da
//! empresa — o despacho, o conjunto efetivo e as migrações. Preparado **uma vez por processo**.
//!
//! No desktop há uma empresa e a diferença é nula. No servidor multi-tenant (ADR-0016) é a
//! diferença entre cada empresa aberta carregar a própria cópia do registro de comandos e do
//! catálogo de permissões, e todas apontarem para a mesma — menos RAM por empresa e uma
//! abertura a frio mais curta.

use std::sync::Arc;

use cardeal_kernel::Resultado;
use cardeal_modkit::{ConjuntoEfetivo, Despachante, Modulo, PedidoAtivacao, RegistroModulos};
use cardeal_storage::ConjuntoMigracoes;

use crate::erro_registro;

/// A distribuição preparada.
pub struct Plano {
    pub(crate) despachante: Despachante,
    pub(crate) conjunto: Arc<ConjuntoEfetivo>,
    pub(crate) migracoes: Vec<ConjuntoMigracoes>,
}

impl Plano {
    /// Monta o despacho, resolve o conjunto efetivo do pedido e junta as migrações (razão +
    /// módulos).
    ///
    /// # Errors
    /// Comando registrado duas vezes, manifesto inválido ou dependência de módulo ausente.
    pub fn preparar(modulos: &[&dyn Modulo], pedido: &PedidoAtivacao) -> Resultado<Arc<Self>> {
        let despachante = Despachante::construir(modulos)?;

        let mut registro = RegistroModulos::novo();
        for m in modulos {
            registro
                .registrar(m.manifesto())
                .map_err(|e| erro_registro(&e))?;
        }
        let conjunto = registro.resolver(pedido).map_err(|e| erro_registro(&e))?;

        let mut migracoes = vec![cardeal_ledger::migracoes::conjunto()];
        migracoes.extend(modulos.iter().map(|m| m.migracoes()));

        Ok(Arc::new(Self {
            despachante,
            conjunto: Arc::new(conjunto),
            migracoes,
        }))
    }
}
