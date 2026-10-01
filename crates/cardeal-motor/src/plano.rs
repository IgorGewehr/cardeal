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
    /// Hash das migrações e do catálogo de permissões: igual ao gravado na base = nada a
    /// verificar na abertura.
    pub(crate) impressao: [u8; 32],
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

        let impressao = impressao(&migracoes, &conjunto);
        Ok(Arc::new(Self {
            despachante,
            conjunto: Arc::new(conjunto),
            migracoes,
            impressao,
        }))
    }
}

/// A impressão digital do plano: tudo o que, mudando, exige verificar a base de novo — as
/// migrações de cada módulo (versão, nome e o próprio SQL) e o catálogo de permissões que o
/// papel de administrador acompanha.
fn impressao(migracoes: &[ConjuntoMigracoes], conjunto: &ConjuntoEfetivo) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    for c in migracoes {
        h.update(c.modulo.as_bytes());
        for d in c.depende_de {
            h.update(d.as_bytes());
        }
        for m in c.migracoes {
            h.update(&m.versao.to_le_bytes());
            h.update(m.nome.as_bytes());
            h.update(m.sql.as_bytes());
        }
        h.update(&[0]);
    }
    for p in &conjunto.permissoes {
        h.update(p.as_bytes());
        h.update(&[0]);
    }
    *h.finalize().as_bytes()
}
