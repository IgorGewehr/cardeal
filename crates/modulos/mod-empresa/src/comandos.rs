//! Os comandos — todos de risco alto e auditados: mudam o que aparece em todo documento.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

/// Atualiza razão social, nome fantasia e regime tributário.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtualizarDadosEmpresa {
    /// Razão social.
    pub razao_social: String,
    /// Nome fantasia.
    pub nome_fantasia: String,
    /// Regime tributário.
    pub regime: String,
}

impl Comando for AtualizarDadosEmpresa {
    type Saida = ();
    const PERMISSAO: &'static str = "empresa.dados.editar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        crate::sql::atualizar_dados(
            uow.conexao(),
            self.razao_social.trim(),
            self.nome_fantasia.trim(),
            self.regime.trim(),
        )
    }
}

/// Grava telefone, e-mail, site e endereço de exibição.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirContatoEmpresa {
    /// Telefone.
    pub telefone: String,
    /// E-mail.
    pub email: String,
    /// Site.
    pub site: String,
    /// Endereço de exibição (uma linha).
    pub endereco: String,
}

impl Comando for DefinirContatoEmpresa {
    type Saida = ();
    const PERMISSAO: &'static str = "empresa.dados.editar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        crate::sql::gravar_config(
            uow.conexao(),
            ctx.empresa,
            &[
                ("empresa.telefone", self.telefone.trim()),
                ("empresa.email", self.email.trim()),
                ("empresa.site", self.site.trim()),
                ("empresa.endereco_exibicao", self.endereco.trim()),
            ],
        )
    }
}

/// Grava (`Some`) ou remove (`None`) a logo — PNG de até 512 KB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirLogoEmpresa {
    /// Os bytes do PNG, ou `None` para remover.
    pub png: Option<Vec<u8>>,
}

impl Comando for DefinirLogoEmpresa {
    type Saida = ();
    const PERMISSAO: &'static str = "empresa.dados.editar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        crate::sql::gravar_logo(uow.conexao(), ctx.empresa, self.png.as_deref())
    }
}
