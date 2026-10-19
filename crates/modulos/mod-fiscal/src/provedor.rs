//! [`ProvedorFiscal`] — o recurso que diz com quem a instalação fala (o `sefaz-api`, o
//! simulado, ou ninguém) — e [`porta_da_empresa`], que abre o certificado do cofre e monta a
//! porta fiscal da empresa da sessão.

use std::sync::Arc;

use cardeal_fiscal::{
    AmbienteFiscal, ApiFiscalHttp, CertificadoA1, ConfigApiFiscal, FiscalSimulado, PortaFiscal,
};
use cardeal_kernel::{Erro, Resultado};

use crate::erros::ErroModuloFiscal;

type Fabrica =
    dyn Fn(CertificadoA1, AmbienteFiscal) -> Resultado<Box<dyn PortaFiscal>> + Send + Sync;

/// Como esta instalação fala com a API fiscal. Um por instalação, injetado pelo motor
/// ([`cardeal_modkit::Recursos`]).
pub struct ProvedorFiscal {
    nome: String,
    fabrica: Box<Fabrica>,
}

impl std::fmt::Debug for ProvedorFiscal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("ProvedorFiscal").field(&self.nome).finish()
    }
}

impl ProvedorFiscal {
    /// O `sefaz-api` de verdade.
    #[must_use]
    pub fn sefaz_api(config: ConfigApiFiscal) -> Self {
        let config = Arc::new(config);
        Self {
            nome: format!("sefaz-api ({})", config.url),
            fabrica: Box::new(move |certificado, ambiente| {
                Ok(Box::new(ApiFiscalHttp::novo(
                    Arc::clone(&config),
                    certificado,
                    ambiente,
                )))
            }),
        }
    }

    /// O simulado em memória (testes, demonstração) — o mesmo para todas as empresas.
    #[must_use]
    pub fn simulado(porta: Arc<FiscalSimulado>) -> Self {
        Self {
            nome: "simulado".into(),
            fabrica: Box::new(move |_, _| Ok(Box::new(Arc::clone(&porta)))),
        }
    }

    /// Sem chave da API: toda conversa com a SEFAZ responde
    /// [`ErroModuloFiscal::ProvedorNaoConfigurado`].
    #[must_use]
    pub fn nao_configurado() -> Self {
        Self {
            nome: "não configurado".into(),
            fabrica: Box::new(|_, _| {
                Err(Erro::de_dominio(&ErroModuloFiscal::ProvedorNaoConfigurado))
            }),
        }
    }

    /// O do ambiente do processo: o `sefaz-api` se `CARDEAL_FISCAL_TOKEN` estiver definida,
    /// senão "não configurado".
    #[must_use]
    pub fn do_ambiente() -> Self {
        ConfigApiFiscal::do_ambiente().map_or_else(Self::nao_configurado, Self::sefaz_api)
    }

    /// Para a tela e o log.
    #[must_use]
    pub fn nome(&self) -> &str {
        &self.nome
    }

    /// A porta para um certificado e um ambiente.
    ///
    /// # Errors
    /// [`ErroModuloFiscal::ProvedorNaoConfigurado`] se a instalação não tem a chave da API.
    pub fn porta(
        &self,
        certificado: CertificadoA1,
        ambiente: AmbienteFiscal,
    ) -> Resultado<Box<dyn PortaFiscal>> {
        (self.fabrica)(certificado, ambiente)
    }
}

/// O conteúdo selado no cofre: o arquivo (já preparado) e a senha.
#[cfg(feature = "sqlite")]
#[derive(serde::Serialize, serde::Deserialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub(crate) struct SegredoDoCertificado {
    pub pfx: Vec<u8>,
    pub senha: String,
}

/// O contexto do selado — o certificado de uma empresa não abre como o de outra.
#[cfg(feature = "sqlite")]
pub(crate) fn contexto(empresa: cardeal_kernel::Id) -> Vec<u8> {
    let mut c = b"fiscal.certificado/".to_vec();
    c.extend_from_slice(&empresa.em_bytes());
    c
}

/// A porta fiscal da empresa da sessão: abre o certificado do cofre e pede ao
/// [`ProvedorFiscal`] uma porta no ambiente configurado.
///
/// # Errors
/// [`ErroModuloFiscal::SemCertificado`]; [`cardeal_storage::ErroCofre`] se o certificado foi
/// guardado em outra instalação; [`ErroModuloFiscal::ProvedorNaoConfigurado`]; recurso
/// ausente (`Cofre`/`ProvedorFiscal`).
#[cfg(feature = "sqlite")]
pub fn porta_da_empresa(
    ctx: &cardeal_modkit::Ctx,
    c: &rusqlite::Connection,
) -> Resultado<Box<dyn PortaFiscal>> {
    let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
    porta_da_empresa_no_ambiente(ctx, c, ambiente)
}

/// Como [`porta_da_empresa`], num ambiente dado — o de um documento já criado (uma nota de
/// homologação na fila continua indo para a homologação, mesmo que a empresa já tenha passado
/// para a produção).
///
/// # Errors
/// Como [`porta_da_empresa`].
#[cfg(feature = "sqlite")]
pub fn porta_da_empresa_no_ambiente(
    ctx: &cardeal_modkit::Ctx,
    c: &rusqlite::Connection,
    ambiente: AmbienteFiscal,
) -> Resultado<Box<dyn PortaFiscal>> {
    use cardeal_storage::{Cofre, Segredo};

    let selado = crate::sql::selado(c, ctx.empresa)?
        .ok_or_else(|| Erro::de_dominio(&ErroModuloFiscal::SemCertificado))?;
    let cofre = ctx.recurso::<Cofre>()?;
    let aberto = cofre
        .abrir(&selado, &contexto(ctx.empresa))
        .map_err(|e| Erro::de_dominio(&e))?;
    let segredo: SegredoDoCertificado = postcard::from_bytes(aberto.expor())
        .map_err(|e| Erro::novo(cardeal_kernel::CodigoErro::FALHA_INTERNA, e.to_string()))?;
    let certificado = CertificadoA1 {
        pfx: Segredo::novo(segredo.pfx.clone()),
        senha: Segredo::novo(segredo.senha.clone()),
    };
    ctx.recurso::<ProvedorFiscal>()?
        .porta(certificado, ambiente)
}
