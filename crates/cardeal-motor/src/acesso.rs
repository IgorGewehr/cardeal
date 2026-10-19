//! Autenticação por login/senha, a sessão de um usuário já autenticado por outro meio (a conta
//! do servidor, ADR-0016) e a sincronização do papel de administrador.

use std::collections::BTreeSet;

use cardeal_auth::{AutorizacoesEfetivas, EmissaoSessao, Escopo, RepositorioAuth, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
use cardeal_modkit::ConjuntoEfetivo;
use cardeal_storage::{Armazenamento, ContextoEscrita, ErroArmazenamento};
use rusqlite::OptionalExtension;

use crate::{como_erro_armazenamento, erro_armazenamento};

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
            let antes = (papel.permissoes.len(), papel.limites.len());
            papel.permissoes.extend(catalogo);
            // O teto de desconto de quem ainda não tem (o que o dono ajustou fica).
            for (chave, valor) in cardeal_auth::PapelDeFabrica::Administrador.limites_padrao() {
                papel.limites.entry(chave).or_insert(valor);
            }
            if (papel.permissoes.len(), papel.limites.len()) == antes {
                return Ok(());
            }
            papel.versao = papel.versao.proxima();
            repo.atualizar_papel(&papel)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?;
    Ok(())
}

/// Completa os papéis de fábrica já gravados (Gerente, Operador de Caixa…) com o que a política
/// de cada um concede do catálogo atual e ainda falta neles — ex.: o caixa passou a ver e
/// cadastrar o cliente da venda — e com os limites padrão que ainda não têm (o teto de
/// desconto). São imutáveis para o usuário (`sistema = 1`; quem quer outro
/// conjunto copia o papel), então acompanhar a política é seguro. Só acrescenta, como o
/// [`sincronizar_papel_admin`].
pub(crate) fn sincronizar_papeis_de_fabrica(
    arm: &Armazenamento,
    empresa: Id,
    conjunto: &ConjuntoEfetivo,
) -> Resultado<()> {
    let catalogo: Vec<String> = conjunto
        .permissoes
        .iter()
        .map(|p| (*p).to_string())
        .collect();
    if catalogo.is_empty() {
        return Ok(());
    }
    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            for fabrica in cardeal_auth::PapelDeFabrica::TODOS {
                if fabrica == cardeal_auth::PapelDeFabrica::Administrador {
                    continue;
                }
                let ids: Vec<Vec<u8>> = {
                    let mut stmt = uow
                        .conexao()
                        .prepare(
                            "SELECT id FROM nucleo_papel
                             WHERE nome = ?1 AND sistema = 1 AND (empresa = ?2 OR empresa IS NULL)",
                        )
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                    let linhas = stmt
                        .query_map(
                            rusqlite::params![fabrica.nome(), empresa.em_bytes().as_slice()],
                            |r| r.get::<_, Vec<u8>>(0),
                        )
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                    linhas
                        .collect::<rusqlite::Result<_>>()
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?
                };
                let concede = fabrica
                    .politica()
                    .aplicar(catalogo.iter().map(String::as_str));
                let limites = fabrica.limites_padrao();
                for id in ids.into_iter().filter_map(|b| <[u8; 16]>::try_from(b).ok()) {
                    let mut repo = RepositorioAuth::novo(uow);
                    repo.acrescentar_permissoes_de_fabrica(
                        Id::de_bytes(id),
                        concede.iter().map(String::as_str),
                    )
                    .map_err(|e| como_erro_armazenamento(&e))?;
                    repo.acrescentar_limites_ausentes(Id::de_bytes(id), &limites)
                        .map_err(|e| como_erro_armazenamento(&e))?;
                }
            }
            Ok(())
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
