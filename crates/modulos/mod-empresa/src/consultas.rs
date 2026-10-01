//! As consultas.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::tipos::{EmpresaResumo, IdentidadeVisual, PapelResumo, UsuarioResumo};

/// Os dados cadastrais da empresa.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DadosDaEmpresa;

impl Consulta for DadosDaEmpresa {
    type Saida = EmpresaResumo;
    const PERMISSAO: &'static str = "empresa.dados.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        crate::sql::dados(c)
    }
}

/// A identidade visual (cabeçalho de todo PDF: nome, contato, logo).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct IdentidadeDaEmpresa;

impl Consulta for IdentidadeDaEmpresa {
    type Saida = IdentidadeVisual;
    const PERMISSAO: &'static str = "empresa.dados.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        crate::sql::identidade(c, ctx.empresa)
    }
}

/// Os usuários da empresa.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct UsuariosDaEmpresa;

impl Consulta for UsuariosDaEmpresa {
    type Saida = Vec<UsuarioResumo>;
    const PERMISSAO: &'static str = "empresa.usuarios.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        crate::sql::usuarios(c)
    }
}

/// Os papéis da empresa.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PapeisDaEmpresa;

impl Consulta for PapeisDaEmpresa {
    type Saida = Vec<PapelResumo>;
    const PERMISSAO: &'static str = "empresa.usuarios.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        crate::sql::papeis(c)
    }
}
