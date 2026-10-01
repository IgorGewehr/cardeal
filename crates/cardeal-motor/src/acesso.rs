//! Usuários e papéis da empresa: listagens de Configurações, a autenticação por login/senha e
//! a sessão de um usuário já autenticado por outro meio (a conta do servidor, ADR-0016).

use std::collections::BTreeSet;

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, RepositorioAuth, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
use cardeal_modkit::ConjuntoEfetivo;
use cardeal_storage::{Armazenamento, ContextoEscrita, ErroArmazenamento};
use rusqlite::OptionalExtension;

use crate::motor::MotorLocal;
use crate::{como_erro_armazenamento, erro_armazenamento};

impl MotorLocal {
    /// Todos os usuários cadastrados (id, login, nome, ativo). `id` existe desde sempre em
    /// `nucleo_usuario` — só não era selecionado porque a tela de Configurações (única
    /// consumidora até aqui) não precisava dele; o comprovante de OS precisa para resolver
    /// `tecnico_responsavel`/`aprovado_por` num nome exibível.
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn usuarios(&self) -> Resultado<Vec<UsuarioResumo>> {
        self.arm
            .leitor()
            .consultar(|c| {
                let mut stmt = c
                    .prepare("SELECT id, login, nome, ativo FROM nucleo_usuario ORDER BY nome")
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                let linhas = stmt
                    .query_map([], |r| {
                        let id_bytes = r.get::<_, Vec<u8>>(0)?;
                        Ok(UsuarioResumo {
                            id: <[u8; 16]>::try_from(id_bytes).map_or(Id::NULO, Id::de_bytes),
                            login: r.get(1)?,
                            nome: r.get(2)?,
                            ativo: r.get::<_, i64>(3)? != 0,
                        })
                    })
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                linhas
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)
    }

    /// Os papéis da empresa (nome, descrição, nº de permissões).
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn papeis(&self) -> Resultado<Vec<PapelResumo>> {
        self.arm
            .leitor()
            .consultar(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT p.nome, p.descricao, p.sistema,
                                (SELECT COUNT(*) FROM nucleo_papel_permissao pp WHERE pp.papel = p.id)
                         FROM nucleo_papel p
                         ORDER BY p.sistema DESC, p.nome",
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                let linhas = stmt
                    .query_map([], |r| {
                        Ok(PapelResumo {
                            nome: r.get(0)?,
                            descricao: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                            sistema: r.get::<_, i64>(2)? != 0,
                            permissoes: usize::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
                        })
                    })
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                linhas
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)
    }
}

/// Um usuário, resumido para a lista de Configurações.
#[derive(Debug, Clone)]
pub struct UsuarioResumo {
    /// Identidade.
    pub id: Id,
    /// Login.
    pub login: String,
    /// Nome.
    pub nome: String,
    /// Se está ativo.
    pub ativo: bool,
}

/// Um papel, resumido para a lista de Configurações.
#[derive(Debug, Clone)]
pub struct PapelResumo {
    /// Nome.
    pub nome: String,
    /// Descrição.
    pub descricao: String,
    /// Papel de fábrica (imutável).
    pub sistema: bool,
    /// Quantas permissões concede.
    pub permissoes: usize,
}

/// Completa o papel "Administrador" da empresa com toda permissão do catálogo atual que
/// ainda não esteja nele. Idempotente: se nada falta, não escreve. Só adiciona — uma
/// permissão retirada de propósito não volta a menos que o catálogo a redeclare.
pub(crate) fn sincronizar_papel_admin(
    arm: &Armazenamento,
    empresa: Id,
    conjunto: &ConjuntoEfetivo,
) -> Resultado<()> {
    let catalogo: BTreeSet<String> = conjunto
        .permissoes
        .iter()
        .map(|p| (*p).to_string())
        .collect();
    if catalogo.is_empty() {
        return Ok(());
    }

    let alvo = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT id FROM nucleo_papel
                 WHERE empresa = ?1 AND nome = 'Administrador' AND sistema = 0
                 LIMIT 1",
                [empresa.em_bytes().as_slice()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .map_err(erro_armazenamento)?
        .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
        .map(Id::de_bytes);
    let Some(papel_id) = alvo else {
        return Ok(());
    };

    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            let mut repo = RepositorioAuth::novo(uow);
            let Some(mut papel) = repo
                .papel_por_id(papel_id)
                .map_err(|e| como_erro_armazenamento(&e))?
            else {
                return Ok(());
            };
            let antes = papel.permissoes.len();
            papel.permissoes.extend(catalogo);
            if papel.permissoes.len() == antes {
                return Ok(());
            }
            papel.versao = papel.versao.proxima();
            repo.atualizar_papel(&papel)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?;
    Ok(())
}

/// Busca o usuário, autentica, persiste o efeito colateral (bloqueio/tentativas) e — só em
/// caso de sucesso — monta a `Sessao` a partir dos papéis do usuário cruzados com o conjunto
/// efetivo. Função livre porque não depende de nada de `MotorLocal` além do armazenamento.
pub(crate) fn autenticar_usuario(
    arm: &Armazenamento,
    empresa: Id,
    login: &str,
    senha: &str,
) -> Resultado<Sessao> {
    let login = login.to_string();
    let mut usuario = arm
        .leitor()
        .consultar(|c| {
            cardeal_auth::consultas::usuario_por_login(c, &login)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?
        .ok_or_else(|| Erro::novo(CodigoErro::NAO_ENCONTRADO, "login ou senha inválidos"))?;

    let agora = Instante::agora();
    let resultado = usuario.autenticar(senha, agora);

    let usuario_final = usuario.clone();
    let ctx = ContextoEscrita::novo(empresa, usuario.id, Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            RepositorioAuth::novo(uow)
                .atualizar_credenciais(&usuario_final)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?;

    resultado.map_err(|e| Erro::de_dominio(&e))?;

    let papeis = arm
        .leitor()
        .consultar(|c| {
            cardeal_auth::consultas::papeis_do_usuario(c, usuario.id, empresa)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?;
    let autorizacoes = AutorizacoesEfetivas::consolidar(&papeis);

    Ok(Sessao::abrir(EmissaoSessao::padrao(
        usuario.id,
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        autorizacoes,
        agora,
    )))
}

/// A sessão de um usuário ativo, sem conferir senha — quem chama já provou a identidade (a
/// conta do servidor). Mesmo cruzamento papéis × empresa de [`autenticar_usuario`].
pub(crate) fn sessao_de_usuario_ativo(
    arm: &Armazenamento,
    empresa: Id,
    usuario: Id,
    dispositivo: Id,
) -> Resultado<Sessao> {
    let ativo = arm
        .leitor()
        .consultar(move |c| {
            cardeal_auth::consultas::usuario_por_id(c, usuario)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?
        .is_some_and(|u| u.ativo);
    if !ativo {
        return Err(Erro::novo(
            CodigoErro::SESSAO_INVALIDA,
            "usuário inexistente ou inativo nesta empresa",
        ));
    }
    let papeis = arm
        .leitor()
        .consultar(move |c| {
            cardeal_auth::consultas::papeis_do_usuario(c, usuario, empresa)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?;
    Ok(Sessao::abrir(EmissaoSessao::padrao(
        usuario,
        dispositivo,
        Escopo::empresa_inteira(empresa),
        AutorizacoesEfetivas::consolidar(&papeis),
        Instante::agora(),
    )))
}
