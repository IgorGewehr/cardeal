//! Os usuários da empresa: quem o administrador põe para trabalhar e com que papel.
//!
//! O papel é um dos de fábrica (`docs/08` §3.2 — Gerente, Vendedor, Operador de Caixa…),
//! materializado contra o catálogo **desta** empresa na primeira vez que é usado; o
//! Administrador é o papel criado no primeiro acesso (que a sincronização do motor mantém
//! com o catálogo inteiro).

use cardeal_auth::PapelDeFabrica;
use cardeal_kernel::Id;
use cardeal_modkit::{Comando, Risco};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;

/// Adiciona (ou reativa) um usuário com um papel de fábrica. Idempotente pelo `login`: o
/// mesmo login devolve o mesmo usuário — reativado, se estava desativado, e com o papel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdicionarUsuario {
    /// Login (no servidor, o e-mail da conta).
    pub login: String,
    /// Nome exibido.
    pub nome: String,
    /// O papel.
    pub papel: PapelDeFabrica,
}

impl Comando for AdicionarUsuario {
    type Saida = Id;
    const PERMISSAO: &'static str = "empresa.usuarios.gerenciar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Id> {
        use cardeal_auth::{PoliticaSenha, RepositorioAuth, Usuario};

        if self.login.trim().is_empty() {
            return Err(
                Erro::novo(CodigoErro::CAMPO_OBRIGATORIO, "informe o login").no_campo("login")
            );
        }
        let papel = papel_da_empresa(ctx, uow, self.papel)?;
        let mut repo = RepositorioAuth::novo(uow);
        let existente = repo
            .usuario_por_login(&self.login)
            .map_err(|e| Erro::de_dominio(&e))?;
        let usuario = if let Some(mut u) = existente {
            if !u.ativo {
                u.reativar();
                repo.atualizar_credenciais(&u)
                    .map_err(|e| Erro::de_dominio(&e))?;
            }
            u
        } else {
            // A senha que vale online é a da conta (diretório); esta é inutilizável.
            let senha = format!("{}{}", Id::novo(), Id::novo());
            let u = Usuario::novo(
                self.login.trim(),
                self.nome.trim(),
                &senha,
                PoliticaSenha::PADRAO,
                ctx.agora,
            )
            .map_err(|e| Erro::de_dominio(&e))?;
            repo.inserir_usuario(&u).map_err(|e| Erro::de_dominio(&e))?;
            u
        };
        let ja_tem = repo
            .papeis_do_usuario(usuario.id, ctx.empresa)
            .map_err(|e| Erro::de_dominio(&e))?
            .iter()
            .any(|p| p.id == papel);
        if !ja_tem {
            repo.atribuir_papel(usuario.id, papel, ctx.empresa)
                .map_err(|e| Erro::de_dominio(&e))?;
        }
        if usuario.login.is_empty() {
            return Err(
                Erro::novo(CodigoErro::CAMPO_OBRIGATORIO, "informe o login").no_campo("login")
            );
        }
        Ok(usuario.id)
    }
}

/// Desativa um usuário: ele não entra mais nesta empresa (o histórico fica). Ninguém
/// desativa a si mesmo — senão a empresa pode ficar sem administrador.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesativarUsuario {
    /// O usuário.
    pub usuario: Id,
}

impl Comando for DesativarUsuario {
    type Saida = ();
    const PERMISSAO: &'static str = "empresa.usuarios.gerenciar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        use cardeal_auth::RepositorioAuth;

        if self.usuario == ctx.usuario {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "você não pode desativar o próprio usuário",
            ));
        }
        // O usuário é da organização (ADR-0017): se ele também está em outro CNPJ, sair
        // desta empresa é perder os papéis **nela**; desativar o usuário o tiraria de todos.
        let usuario_b = self.usuario.em_bytes();
        let empresa_b = ctx.empresa.em_bytes();
        let em_outra: bool = uow
            .conexao()
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM nucleo_usuario_papel
                                WHERE usuario = ?1 AND empresa IS NOT NULL AND empresa <> ?2)",
                rusqlite::params![usuario_b.as_slice(), empresa_b.as_slice()],
                |r| r.get(0),
            )
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
        if em_outra {
            let n = uow
                .conexao()
                .execute(
                    "DELETE FROM nucleo_usuario_papel WHERE usuario = ?1 AND empresa = ?2",
                    rusqlite::params![usuario_b.as_slice(), empresa_b.as_slice()],
                )
                .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
            if n == 0 {
                return Err(Erro::novo(
                    CodigoErro::NAO_ENCONTRADO,
                    "usuário não encontrado nesta empresa",
                ));
            }
            return Ok(());
        }
        let mut repo = RepositorioAuth::novo(uow);
        let mut u = repo
            .usuario_por_id(self.usuario)
            .map_err(|e| Erro::de_dominio(&e))?
            .ok_or_else(|| Erro::novo(CodigoErro::NAO_ENCONTRADO, "usuário não encontrado"))?;
        u.desativar();
        repo.atualizar_credenciais(&u)
            .map_err(|e| Erro::de_dominio(&e))
    }
}

/// O id do papel desta empresa para um papel de fábrica — criando-o, contra o catálogo atual,
/// na primeira vez.
#[cfg(feature = "sqlite")]
fn papel_da_empresa(
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
    fabrica: PapelDeFabrica,
) -> Resultado<Id> {
    use cardeal_auth::RepositorioAuth;
    use rusqlite::OptionalExtension;

    // O Administrador da empresa é o do primeiro acesso (não-sistema, mantido completo).
    let sistema = i64::from(fabrica != PapelDeFabrica::Administrador);
    let existente: Option<Vec<u8>> = uow
        .conexao()
        .query_row(
            "SELECT id FROM nucleo_papel
             WHERE nome = ?1 AND sistema = ?2 AND (empresa = ?3 OR empresa IS NULL)
             ORDER BY empresa IS NULL LIMIT 1",
            rusqlite::params![fabrica.nome(), sistema, ctx.empresa.em_bytes().as_slice()],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
    if let Some(id) = existente.and_then(|b| <[u8; 16]>::try_from(b).ok()) {
        return Ok(Id::de_bytes(id));
    }
    if fabrica == PapelDeFabrica::Administrador {
        return Err(Erro::novo(
            CodigoErro::ESTADO_INVALIDO,
            "a empresa não tem administrador",
        ));
    }
    let mut papel = fabrica.materializar(ctx.conjunto().permissoes.iter().copied());
    papel.empresa = Some(ctx.empresa);
    RepositorioAuth::novo(uow)
        .inserir_papel(&papel)
        .map_err(|e| Erro::de_dominio(&e))?;
    Ok(papel.id)
}
