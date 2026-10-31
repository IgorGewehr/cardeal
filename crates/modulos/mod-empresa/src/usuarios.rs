//! Os **usuários do banco local** (desktop monoposto): cadastrar com a senha inicial, trocar o
//! papel, redefinir a senha de quem esqueceu e reativar. No servidor, quem entra são as contas
//! (`cardeal-server`, `POST /v1/e/{empresa}/membros`); estes comandos cuidam de quem entra pela
//! base.
//!
//! **A senha nunca vai no comando:** quem chama confere a política e manda só o hash Argon2id
//! (`cardeal_auth::hash_senha`) — a auditoria e a idempotência gravam o comando, e senha em
//! texto não pode morar em lugar nenhum.
//!
//! **Escalada de privilégio:** dar ou tirar o papel de Administrador e redefinir a senha de um
//! administrador é coisa de administrador (o Gerente também gerencia usuários, mas não vira dono
//! por tabela); e a empresa nunca fica sem administrador ativo.

use cardeal_auth::PapelDeFabrica;
use cardeal_kernel::Id;
use cardeal_modkit::{Comando, Risco};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;

/// Cadastra um usuário do banco local com a senha inicial (o hash) e um papel. A senha fica
/// marcada para troca no primeiro acesso.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CadastrarUsuario {
    /// Login (minúsculo, sem espaço).
    pub login: String,
    /// Nome exibido.
    pub nome: String,
    /// O papel nesta empresa.
    pub papel: PapelDeFabrica,
    /// O hash Argon2id da senha inicial (formato PHC) — nunca a senha.
    pub hash_senha: String,
}

impl Comando for CadastrarUsuario {
    type Saida = Id;
    const PERMISSAO: &'static str = "empresa.usuarios.gerenciar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Id> {
        sqlite::cadastrar(&self, ctx, uow)
    }
}

/// Troca o papel do usuário nesta empresa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DefinirPapelDoUsuario {
    /// O usuário.
    pub usuario: Id,
    /// O papel novo.
    pub papel: PapelDeFabrica,
}

impl Comando for DefinirPapelDoUsuario {
    type Saida = ();
    const PERMISSAO: &'static str = "empresa.usuarios.gerenciar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::definir_papel(self, ctx, uow)
    }
}

/// O administrador redefine a senha de quem esqueceu: a nova fica marcada para troca no
/// próximo acesso e o bloqueio por tentativas é limpo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedefinirSenhaDoUsuario {
    /// O usuário.
    pub usuario: Id,
    /// O hash Argon2id da senha provisória (formato PHC).
    pub hash_senha: String,
}

impl Comando for RedefinirSenhaDoUsuario {
    type Saida = ();
    const PERMISSAO: &'static str = "empresa.usuarios.gerenciar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::redefinir_senha(&self, ctx, uow)
    }
}

/// Reativa o usuário desativado.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReativarUsuario {
    /// O usuário.
    pub usuario: Id,
}

impl Comando for ReativarUsuario {
    type Saida = ();
    const PERMISSAO: &'static str = "empresa.usuarios.gerenciar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::reativar(self.usuario, ctx, uow)
    }
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use cardeal_auth::{Bloqueio, HashDeSenha, PapelDeFabrica, RepositorioAuth, Usuario};
    use cardeal_kernel::{CodigoErro, Erro, Id, Resultado, Versao};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;

    use super::{CadastrarUsuario, DefinirPapelDoUsuario, RedefinirSenhaDoUsuario};

    const ADMINISTRADOR: &str = "Administrador";

    fn hash_valido(phc: &str) -> Resultado<HashDeSenha> {
        if phc.starts_with("$argon2id$") {
            Ok(HashDeSenha::de_phc(phc.to_owned()))
        } else {
            Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "a senha precisa chegar como hash Argon2id",
            ))
        }
    }

    /// O usuário tem o papel de Administrador (o do primeiro acesso) nesta empresa?
    fn e_administrador(uow: &mut UnidadeDeTrabalho, usuario: Id, empresa: Id) -> Resultado<bool> {
        Ok(RepositorioAuth::novo(uow)
            .papeis_do_usuario(usuario, empresa)
            .map_err(|e| Erro::de_dominio(&e))?
            .iter()
            .any(|p| p.nome == ADMINISTRADOR && !p.sistema))
    }

    fn exigir_administrador(ctx: &Ctx, uow: &mut UnidadeDeTrabalho, o_que: &str) -> Resultado<()> {
        if e_administrador(uow, ctx.usuario, ctx.empresa)? {
            return Ok(());
        }
        Err(Erro::novo(
            CodigoErro::SEM_PERMISSAO,
            format!("{o_que} é coisa do Administrador da empresa"),
        ))
    }

    /// Quantos administradores **ativos** a empresa tem, tirando `exceto`.
    fn outros_administradores(
        uow: &mut UnidadeDeTrabalho,
        empresa: Id,
        exceto: Id,
    ) -> Resultado<i64> {
        uow.conexao()
            .query_row(
                "SELECT COUNT(DISTINCT u.id) FROM nucleo_usuario u
                 JOIN nucleo_usuario_papel up ON up.usuario = u.id
                 JOIN nucleo_papel p ON p.id = up.papel
                 WHERE u.ativo = 1 AND u.id <> ?2 AND p.nome = ?3 AND p.sistema = 0
                   AND (up.empresa = ?1 OR up.empresa IS NULL)",
                rusqlite::params![
                    empresa.em_bytes().as_slice(),
                    exceto.em_bytes().as_slice(),
                    ADMINISTRADOR
                ],
                |r| r.get(0),
            )
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))
    }

    fn usuario(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<Usuario> {
        RepositorioAuth::novo(uow)
            .usuario_por_id(id)
            .map_err(|e| Erro::de_dominio(&e))?
            .ok_or_else(|| Erro::novo(CodigoErro::NAO_ENCONTRADO, "usuário não encontrado"))
    }

    pub(super) fn cadastrar(
        cmd: &CadastrarUsuario,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<Id> {
        let login = cmd.login.trim().to_lowercase();
        if login.is_empty() || login.chars().any(char::is_whitespace) {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "o login não pode ser vazio nem ter espaço",
            )
            .no_campo("login"));
        }
        if cmd.nome.trim().is_empty() {
            return Err(
                Erro::novo(CodigoErro::CAMPO_OBRIGATORIO, "informe o nome").no_campo("nome")
            );
        }
        let hash = hash_valido(&cmd.hash_senha)?;
        if cmd.papel == PapelDeFabrica::Administrador {
            exigir_administrador(ctx, uow, "Dar o papel de Administrador")?;
        }
        if RepositorioAuth::novo(uow)
            .usuario_por_login(&login)
            .map_err(|e| Erro::de_dominio(&e))?
            .is_some()
        {
            return Err(Erro::novo(
                CodigoErro::DUPLICADO,
                "já existe um usuário com este login — reative-o ou troque o papel dele",
            )
            .no_campo("login"));
        }
        let papel = crate::membros::papel_da_empresa(ctx, uow, cmd.papel)?;
        let u = Usuario {
            id: Id::novo(),
            login,
            nome: cmd.nome.trim().to_owned(),
            email: None,
            hash_senha: hash,
            senha_trocada_em: ctx.agora,
            exige_troca_senha: true,
            mfa_habilitado: false,
            ativo: true,
            bloqueio: Bloqueio::NOVO,
            versao: Versao::INICIAL,
        };
        let mut repo = RepositorioAuth::novo(uow);
        repo.inserir_usuario(&u).map_err(|e| Erro::de_dominio(&e))?;
        repo.atribuir_papel(u.id, papel, ctx.empresa)
            .map_err(|e| Erro::de_dominio(&e))?;
        Ok(u.id)
    }

    pub(super) fn definir_papel(
        cmd: DefinirPapelDoUsuario,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<()> {
        if cmd.usuario == ctx.usuario {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "você não pode trocar o próprio papel",
            ));
        }
        let _ = usuario(uow, cmd.usuario)?;
        let era_admin = e_administrador(uow, cmd.usuario, ctx.empresa)?;
        let vira_admin = cmd.papel == PapelDeFabrica::Administrador;
        if era_admin != vira_admin {
            exigir_administrador(ctx, uow, "Dar ou tirar o papel de Administrador")?;
        }
        if era_admin && !vira_admin && outros_administradores(uow, ctx.empresa, cmd.usuario)? == 0 {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "a empresa ficaria sem administrador",
            ));
        }
        let papel = crate::membros::papel_da_empresa(ctx, uow, cmd.papel)?;
        uow.conexao()
            .execute(
                "DELETE FROM nucleo_usuario_papel WHERE usuario = ?1 AND empresa = ?2",
                rusqlite::params![
                    cmd.usuario.em_bytes().as_slice(),
                    ctx.empresa.em_bytes().as_slice()
                ],
            )
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
        RepositorioAuth::novo(uow)
            .atribuir_papel(cmd.usuario, papel, ctx.empresa)
            .map_err(|e| Erro::de_dominio(&e))
    }

    pub(super) fn redefinir_senha(
        cmd: &RedefinirSenhaDoUsuario,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<()> {
        if cmd.usuario == ctx.usuario {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "para a sua senha, use \"Trocar a minha senha\" (pede a atual)",
            ));
        }
        let hash = hash_valido(&cmd.hash_senha)?;
        if e_administrador(uow, cmd.usuario, ctx.empresa)? {
            exigir_administrador(ctx, uow, "Redefinir a senha de um administrador")?;
        }
        let mut u = usuario(uow, cmd.usuario)?;
        u.hash_senha = hash;
        u.senha_trocada_em = ctx.agora;
        u.exige_troca_senha = true;
        u.bloqueio = Bloqueio::NOVO;
        u.versao = u.versao.proxima();
        RepositorioAuth::novo(uow)
            .atualizar_credenciais(&u)
            .map_err(|e| Erro::de_dominio(&e))
    }

    pub(super) fn reativar(
        usuario_id: Id,
        _ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<()> {
        let mut u = usuario(uow, usuario_id)?;
        if u.ativo {
            return Ok(());
        }
        u.reativar();
        RepositorioAuth::novo(uow)
            .atualizar_credenciais(&u)
            .map_err(|e| Erro::de_dominio(&e))
    }
}
