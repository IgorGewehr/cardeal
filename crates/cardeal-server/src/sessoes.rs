//! O cache das sessões de conta: o caminho quente de toda requisição não toca o diretório.
//!
//! Uma entrada guarda a conta, a validade e **as empresas em que ela entra** (com o usuário de
//! cada uma). É recarregada do diretório a cada [`RECARGA`]: um vínculo revogado ou uma
//! sessão encerrada em outro processo valem em até esse tempo. O logout neste processo vale na
//! hora.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
use parking_lot::RwLock;

use crate::diretorio::Diretorio;
use crate::token::{self, HashToken};

const RECARGA: Duration = Duration::from_secs(5 * 60);

/// Uma sessão de conta resolvida.
#[derive(Debug)]
pub struct SessaoConta {
    /// Identidade da sessão.
    pub id: Id,
    /// A conta.
    pub conta: Id,
    expira_em: Instante,
    /// empresa → usuário da conta naquela empresa.
    empresas: HashMap<Id, Id>,
    carregada_em: Instant,
}

impl SessaoConta {
    /// O usuário com que esta conta entra na `empresa`.
    ///
    /// # Errors
    /// `SEM_PERMISSAO` se a conta não tem vínculo com a empresa — inclusive quando a empresa
    /// nem existe: a resposta não revela quais empresas existem.
    pub fn usuario_em(&self, empresa: Id) -> Resultado<Id> {
        self.empresas.get(&empresa).copied().ok_or_else(|| {
            Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "esta conta não tem acesso a esta empresa",
            )
        })
    }
}

/// O cache.
#[derive(Default)]
pub struct Sessoes {
    cache: RwLock<HashMap<HashToken, Arc<SessaoConta>>>,
}

fn sessao_invalida() -> Erro {
    Erro::novo(
        CodigoErro::SESSAO_INVALIDA,
        "sessão inexistente ou expirada — entre de novo",
    )
}

impl Sessoes {
    /// Resolve um token. **Bloqueante** numa falta do cache (lê o diretório).
    ///
    /// # Errors
    /// `SESSAO_INVALIDA` se o token não existe ou expirou; falha do diretório.
    pub fn resolver(&self, diretorio: &Diretorio, token: &str) -> Resultado<Arc<SessaoConta>> {
        let hash = token::hash(token);
        let agora = Instante::agora();
        if let Some(s) = self.cache.read().get(&hash) {
            if s.expira_em > agora && s.carregada_em.elapsed() < RECARGA {
                return Ok(Arc::clone(s));
            }
        }
        let Some(persistida) = diretorio.sessao(&hash, agora)? else {
            self.cache.write().remove(&hash);
            return Err(sessao_invalida());
        };
        let empresas = diretorio
            .vinculos(persistida.conta)?
            .into_iter()
            .map(|v| (v.empresa, v.usuario))
            .collect();
        let s = Arc::new(SessaoConta {
            id: persistida.id,
            conta: persistida.conta,
            expira_em: persistida.expira_em,
            empresas,
            carregada_em: Instant::now(),
        });
        self.cache.write().insert(hash, Arc::clone(&s));
        Ok(s)
    }

    /// Esquece um token (logout).
    pub fn esquecer(&self, token: &str) {
        self.cache.write().remove(&token::hash(token));
    }

    /// Tira do cache o que expirou ou passou da recarga — memória não cresce com sessões
    /// abandonadas.
    pub fn podar(&self) {
        let agora = Instante::agora();
        self.cache
            .write()
            .retain(|_, s| s.expira_em > agora && s.carregada_em.elapsed() < RECARGA);
    }

    /// Quantas sessões estão no cache.
    #[must_use]
    pub fn em_cache(&self) -> usize {
        self.cache.read().len()
    }
}
