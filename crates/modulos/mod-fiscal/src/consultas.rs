//! As consultas.

use cardeal_fiscal::{ModeloFiscal, StatusSefaz};
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::tipos::SituacaoFiscalDaEmpresa;

/// A situação fiscal: certificado, ambiente e pendências.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SituacaoFiscal;

impl Consulta for SituacaoFiscal {
    type Saida = SituacaoFiscalDaEmpresa;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<SituacaoFiscalDaEmpresa> {
        let certificado = crate::sql::certificado(c, ctx.empresa)?;
        let dados = mod_empresa::sql::dados_fiscais(c, ctx.empresa)?;
        Ok(SituacaoFiscalDaEmpresa {
            pendencias: crate::tipos::pendencias(
                certificado.as_ref(),
                dados.completos(),
                ctx.hoje(),
                ctx.fuso,
            ),
            certificado,
            ambiente: crate::sql::ambiente(c, ctx.empresa)?,
        })
    }
}

/// O status do serviço de autorização da SEFAZ da UF da empresa, pelo provedor, com o
/// certificado instalado — a conferência ponta a ponta de certificado, chave da API e rede.
/// Fala com a rede (uma conexão de leitura fica ocupada até a resposta ou o tempo limite).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct StatusDaSefaz {
    /// NF-e ou NFC-e (os serviços são separados em algumas UFs).
    pub modelo: ModeloFiscal,
}

impl Consulta for StatusDaSefaz {
    type Saida = StatusSefaz;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<StatusSefaz> {
        let uf = mod_empresa::sql::dados_fiscais(c, ctx.empresa)?
            .normalizado()
            .ok()
            .and_then(|d| d.uf())
            .ok_or_else(|| {
                cardeal_kernel::Erro::de_dominio(
                    &crate::erros::ErroModuloFiscal::DadosFiscaisIncompletos,
                )
            })?;
        crate::provedor::porta_da_empresa(ctx, c)?.status_servico(uf, self.modelo)
    }
}
