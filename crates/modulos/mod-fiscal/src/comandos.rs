//! Os comandos — todos de risco alto: mexem no que assina as notas da empresa.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::tipos::CertificadoResumo;
use cardeal_fiscal::AmbienteFiscal;

/// Instala (ou troca) o certificado A1 da empresa: confere a senha, a validade e o CNPJ,
/// regrava se usar RC2 e guarda cifrado no cofre da instalação.
#[derive(Clone, Serialize, Deserialize)]
pub struct InstalarCertificado {
    /// Os bytes do `.pfx`/`.p12`.
    pub arquivo: Vec<u8>,
    /// A senha.
    pub senha: String,
}

impl std::fmt::Debug for InstalarCertificado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstalarCertificado")
            .field("arquivo", &format_args!("{} bytes", self.arquivo.len()))
            .field("senha", &"[oculta]")
            .finish()
    }
}

impl Drop for InstalarCertificado {
    fn drop(&mut self) {
        zeroize::Zeroize::zeroize(&mut self.arquivo);
        zeroize::Zeroize::zeroize(&mut self.senha);
    }
}

impl Comando for InstalarCertificado {
    type Saida = CertificadoResumo;
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<CertificadoResumo> {
        use cardeal_kernel::{Cnpj, Erro};
        use cardeal_storage::Cofre;

        use crate::erros::ErroModuloFiscal;
        use crate::provedor::{contexto, SegredoDoCertificado};

        let preparado = cardeal_fiscal::preparar_certificado_a1(&self.arquivo, &self.senha)
            .map_err(|e| Erro::de_dominio(&e))?;
        let info = &preparado.info;
        if info.vencido(ctx.agora) {
            return Err(Erro::de_dominio(&ErroModuloFiscal::CertificadoVencido {
                em: info.valido_ate.data(ctx.fuso).formatar(),
            }));
        }
        let cnpj = info
            .cnpj
            .ok_or_else(|| Erro::de_dominio(&ErroModuloFiscal::CertificadoSemCnpj))?;
        let da_empresa = Cnpj::novo(&mod_empresa::sql::dados(uow.conexao(), ctx.empresa)?.cnpj)?;
        if cnpj != da_empresa {
            return Err(Erro::de_dominio(&ErroModuloFiscal::CnpjDiferente {
                do_certificado: cnpj.formatar(),
                da_empresa: da_empresa.formatar(),
            }));
        }

        let segredo = SegredoDoCertificado {
            pfx: preparado.pfx.clone(),
            senha: self.senha.clone(),
        };
        let claro =
            zeroize::Zeroizing::new(postcard::to_stdvec(&segredo).map_err(|e| {
                Erro::novo(cardeal_kernel::CodigoErro::FALHA_INTERNA, e.to_string())
            })?);
        let selado = ctx
            .recurso::<Cofre>()?
            .selar(&claro, &contexto(ctx.empresa));
        let resumo = CertificadoResumo {
            titular: info.titular.clone(),
            cnpj: cnpj.formatar(),
            emissor: info.emissor.clone(),
            numero_serie: info.numero_serie.clone(),
            valido_de: info.valido_de,
            valido_ate: info.valido_ate,
            convertido: preparado.convertido,
            instalado_em: ctx.agora,
        };
        crate::sql::gravar_certificado(uow.conexao(), ctx.empresa, &resumo, &selado, ctx.usuario)?;
        Ok(resumo)
    }
}

/// Remove o certificado da empresa (e o arquivo cifrado junto).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct RemoverCertificado;

impl Comando for RemoverCertificado {
    type Saida = bool;
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<bool> {
        crate::sql::remover_certificado(uow.conexao(), ctx.empresa)
    }
}

/// Troca o ambiente fiscal. Produção exige certificado instalado.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirAmbienteFiscal {
    /// O ambiente.
    pub ambiente: AmbienteFiscal,
}

impl Comando for DefinirAmbienteFiscal {
    type Saida = ();
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        if self.ambiente == AmbienteFiscal::Producao
            && crate::sql::certificado(uow.conexao(), ctx.empresa)?.is_none()
        {
            return Err(cardeal_kernel::Erro::de_dominio(
                &crate::erros::ErroModuloFiscal::SemCertificado,
            ));
        }
        crate::sql::gravar_ambiente(uow.conexao(), ctx.empresa, self.ambiente)
    }
}
