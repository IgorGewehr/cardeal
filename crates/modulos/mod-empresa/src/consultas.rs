//! As consultas.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::tipos::{EmpresaResumo, IdentidadeVisual, PapelResumo, TetoDeDesconto, UsuarioResumo};

/// Os dados cadastrais da empresa.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DadosDaEmpresa;

impl Consulta for DadosDaEmpresa {
    type Saida = EmpresaResumo;
    const PERMISSAO: &'static str = "empresa.dados.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        crate::sql::dados(c, ctx.empresa)
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
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        crate::sql::usuarios(c, ctx.empresa)
    }
}

/// Os papéis da empresa.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PapeisDaEmpresa;

impl Consulta for PapeisDaEmpresa {
    type Saida = Vec<PapelResumo>;
    const PERMISSAO: &'static str = "empresa.usuarios.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        crate::sql::papeis(c, ctx.empresa)
    }
}

/// O teto de desconto de cada papel que vende (Administrador, Gerente, Operador de Caixa,
/// Vendedor), nesta ordem.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct TetosDeDesconto;

impl Consulta for TetosDeDesconto {
    type Saida = Vec<TetoDeDesconto>;
    const PERMISSAO: &'static str = "empresa.usuarios.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        use cardeal_auth::{PapelDeFabrica, ValorLimite, CHAVE_DESCONTO_MAXIMO};
        use cardeal_kernel::{CodigoErro, Erro, Percentual};

        let falha =
            |e: &dyn std::fmt::Display| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string());
        let mut tetos = Vec::new();
        for fabrica in PapelDeFabrica::TODOS {
            let Some(padrao) = fabrica.limites_padrao().remove(CHAVE_DESCONTO_MAXIMO) else {
                continue;
            };
            let (valor, usuarios) = match crate::membros::papel_existente(c, ctx.empresa, fabrica)?
            {
                None => (Some(padrao), 0),
                Some(id) => {
                    let papel =
                        cardeal_auth::consultas::papel_por_id(c, id).map_err(|e| falha(&e))?;
                    let usuarios: i64 = c
                        .query_row(
                            "SELECT COUNT(*) FROM nucleo_usuario_papel up
                               JOIN nucleo_usuario u ON u.id = up.usuario
                              WHERE up.papel = ?1 AND u.ativo = 1
                                AND (up.empresa = ?2 OR up.empresa IS NULL)",
                            rusqlite::params![
                                id.em_bytes().as_slice(),
                                ctx.empresa.em_bytes().as_slice()
                            ],
                            |r| r.get(0),
                        )
                        .map_err(|e| falha(&e))?;
                    (
                        papel.and_then(|p| p.limites.get(CHAVE_DESCONTO_MAXIMO).copied()),
                        usize::try_from(usuarios).unwrap_or(0),
                    )
                }
            };
            // Sem o limite gravado, o teto é zero (`limite_desconto` no PDV e nas vendas).
            let teto = match valor {
                Some(ValorLimite::Ilimitado) => None,
                Some(ValorLimite::Percentual(p)) => Some(p),
                _ => Some(Percentual::ZERO),
            };
            tetos.push(TetoDeDesconto {
                papel: fabrica,
                teto,
                usuarios,
            });
        }
        Ok(tetos)
    }
}
