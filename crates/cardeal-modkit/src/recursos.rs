//! [`Recursos`] — o que o motor injeta nos comandos e consultas e que não mora na base: o
//! cofre de segredos, o provedor fiscal... (`docs/contratos-internos.md` §4, "portas").
//!
//! Um mapa por tipo: quem monta o motor (o desktop, o servidor, o teste) põe cada recurso com
//! [`Recursos::com`]; o manipulador pega com [`crate::Ctx::recurso`]. O núcleo não conhece
//! nenhum tipo concreto — a mesma regra dos módulos.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

/// Os recursos de uma instalação, por tipo.
#[derive(Clone, Default)]
pub struct Recursos {
    mapa: HashMap<TypeId, (&'static str, Arc<dyn Any + Send + Sync>)>,
}

impl Recursos {
    /// Nenhum recurso.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Acrescenta (ou troca) o recurso do tipo `T`.
    #[must_use]
    pub fn com<T: Send + Sync + 'static>(mut self, valor: Arc<T>) -> Self {
        self.mapa
            .insert(TypeId::of::<T>(), (std::any::type_name::<T>(), valor));
        self
    }

    /// O recurso do tipo `T`, se houver.
    #[must_use]
    pub fn obter<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        self.mapa
            .get(&TypeId::of::<T>())
            .and_then(|(_, v)| Arc::clone(v).downcast::<T>().ok())
    }
}

impl std::fmt::Debug for Recursos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut nomes: Vec<&str> = self.mapa.values().map(|(n, _)| *n).collect();
        nomes.sort_unstable();
        f.debug_tuple("Recursos").field(&nomes).finish()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    struct Cofre(u8);
    struct Provedor(&'static str);

    #[test]
    fn guarda_e_devolve_por_tipo() {
        let r = Recursos::novo()
            .com(Arc::new(Cofre(7)))
            .com(Arc::new(Provedor("sefaz-api")));
        assert_eq!(r.obter::<Cofre>().unwrap().0, 7);
        assert_eq!(r.obter::<Provedor>().unwrap().0, "sefaz-api");
        assert!(r.obter::<String>().is_none());
        let trocado = r.com(Arc::new(Cofre(9)));
        assert_eq!(trocado.obter::<Cofre>().unwrap().0, 9);
        assert!(format!("{trocado:?}").contains("Provedor"));
    }
}
