//! O **vendedor da venda** (`F4`): quem ganha a comissão — não é o operador do caixa (quem passa a
//! venda), é quem atendeu. Sem vendedor, a venda não gera comissão. Na finalização, a venda com
//! vendedor vira uma venda comissionável no `mod-vendas` (o percentual de cada produto congelado
//! ali); o estorno da venda a estorna junto.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Define (ou, com `None`, tira) o vendedor de um cupom em andamento.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirVendedorDoCupom {
    /// O cupom.
    pub cupom: Id,
    /// O vendedor (um usuário da empresa).
    pub vendedor: Option<Id>,
}

impl Comando for DefinirVendedorDoCupom {
    type Saida = ();
    const PERMISSAO: &'static str = "pdv.venda.editar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        let cupom = crate::comandos::carregar_cupom(uow, self.cupom)?;
        if cupom.estado != crate::cupom::EstadoCupom::EmAndamento {
            return Err(Erro::novo(
                cardeal_kernel::CodigoErro::ESTADO_INVALIDO,
                "O vendedor se escolhe com a venda em andamento",
            ));
        }
        gravar(uow.conexao(), self.cupom, self.vendedor)
    }
}

/// Os vendedores que o caixa pode escolher: os usuários ativos da empresa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VendedoresDoPdv;

impl Consulta for VendedoresDoPdv {
    type Saida = Vec<(Id, String)>;
    const PERMISSAO: &'static str = "pdv.venda.editar";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<(Id, String)>> {
        Ok(mod_empresa::sql::usuarios(c, ctx.empresa)?
            .into_iter()
            .filter(|u| u.ativo)
            .map(|u| (u.id, u.nome))
            .collect())
    }
}

/// Grava o vendedor do cupom.
///
/// # Errors
/// Erro do SQLite.
#[cfg(feature = "sqlite")]
pub(crate) fn gravar(c: &Connection, cupom: Id, vendedor: Option<Id>) -> Resultado<()> {
    c.execute(
        "UPDATE pdv_cupom SET vendedor = ?2 WHERE id = ?1",
        rusqlite::params![
            cupom.em_bytes().as_slice(),
            vendedor.map(|v| v.em_bytes().to_vec())
        ],
    )
    .map(|_| ())
    .map_err(|e| Erro::novo(cardeal_kernel::CodigoErro::FALHA_INTERNA, e.to_string()))
}

/// O vendedor do cupom.
///
/// # Errors
/// Erro do SQLite.
#[cfg(feature = "sqlite")]
pub(crate) fn do_cupom(c: &Connection, cupom: Id) -> Resultado<Option<Id>> {
    c.query_row(
        "SELECT vendedor FROM pdv_cupom WHERE id = ?1",
        [cupom.em_bytes().as_slice()],
        |r| r.get::<_, Option<Vec<u8>>>(0),
    )
    .map(|v| v.and_then(|b| <[u8; 16]>::try_from(b.as_slice()).ok().map(Id::de_bytes)))
    .map_err(|e| Erro::novo(cardeal_kernel::CodigoErro::FALHA_INTERNA, e.to_string()))
}
