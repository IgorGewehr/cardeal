//! `POST /v1/e/{empresa}/membros` e `DELETE /v1/e/{empresa}/membros/{usuario}`: o
//! administrador põe e tira gente da empresa. Duas bases mudam — a conta/vínculo no
//! diretório e o usuário/papel na base da empresa (pelo despacho, com auditoria) —, então a
//! permissão é conferida **antes** de tocar em qualquer uma.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use cardeal_kernel::{ChaveIdempotencia, CodigoErro, Erro, Id, Resultado};
use cardeal_protocol::{MembroAdicionado, PedidoNovoMembro};

use super::credencial::exigir_token;
use super::despacho::{empresa_da_rota, entrar_na_empresa};
use super::resposta::{bloqueante, ler, postcard, ErroHttp};
use crate::diretorio::normalizar_email;
use crate::{autenticacao, Servidor};

const PERMISSAO: &str = "empresa.usuarios.gerenciar";

fn exigir_permissao(concede: bool) -> Resultado<()> {
    if concede {
        Ok(())
    } else {
        Err(Erro::novo(
            CodigoErro::SEM_PERMISSAO,
            "só quem gerencia os usuários da empresa pode fazer isso",
        ))
    }
}

pub(super) async fn adicionar(
    State(servidor): State<Arc<Servidor>>,
    Path(empresa): Path<String>,
    headers: HeaderMap,
    corpo: Bytes,
) -> Result<Response, ErroHttp> {
    let token = exigir_token(&headers)?;
    let empresa = empresa_da_rota(&empresa)?;
    let pedido: PedidoNovoMembro = ler(&corpo)?;
    // Conta nova = um Argon2id: o mesmo semáforo do login segura o pico de memória.
    let _licenca = servidor
        .logins
        .acquire()
        .await
        .map_err(|e| ErroHttp(Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())))?;
    let s = Arc::clone(&servidor);
    let feito = bloqueante(move || {
        let (aberta, sessao) = entrar_na_empresa(&s, &token, empresa)?;
        exigir_permissao(sessao.concede(PERMISSAO))?;

        let email = normalizar_email(&pedido.email);
        if !email.contains('@') {
            return Err(Erro::novo(CodigoErro::VALOR_INVALIDO, "e-mail inválido").no_campo("email"));
        }
        let (conta, conta_nova) = if let Some(c) = s.diretorio.conta_por_email(&email)? {
            (c.id, false)
        } else {
            let senha = pedido.senha_inicial.as_deref().ok_or_else(|| {
                Erro::novo(
                    CodigoErro::CAMPO_OBRIGATORIO,
                    "conta nova precisa de senha inicial",
                )
                .no_campo("senha_inicial")
            })?;
            let hash =
                autenticacao::hash_de_senha_nova(senha).map_err(|e| e.no_campo("senha_inicial"))?;
            (s.diretorio.criar_conta(&email, &pedido.nome, &hash)?, true)
        };
        let carga = postcard::to_stdvec(&mod_empresa::AdicionarUsuario {
            login: email,
            nome: pedido.nome,
            papel: pedido.papel,
        })
        .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
        let saida = aberta.motor().executar_bruto(
            &sessao,
            "empresa.adicionar_usuario.v1",
            &carga,
            Some(ChaveIdempotencia::nova()),
        )?;
        let usuario: Id = postcard::from_bytes(&saida)
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
        s.diretorio.vincular(conta, empresa, usuario)?;
        s.sessoes.esquecer_conta(conta);
        tracing::info!(%empresa, %conta, %usuario, conta_nova, "membro adicionado");
        Ok(MembroAdicionado {
            usuario,
            conta_nova,
        })
    })
    .await?;
    Ok(postcard(&feito)?.into_response())
}

pub(super) async fn remover(
    State(servidor): State<Arc<Servidor>>,
    Path((empresa, usuario)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, ErroHttp> {
    let token = exigir_token(&headers)?;
    let empresa = empresa_da_rota(&empresa)?;
    let usuario: Id = usuario.parse().map_err(|_| {
        ErroHttp(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            "identificador de usuário inválido",
        ))
    })?;
    let s = Arc::clone(&servidor);
    bloqueante(move || {
        let (aberta, sessao) = entrar_na_empresa(&s, &token, empresa)?;
        exigir_permissao(sessao.concede(PERMISSAO))?;
        let carga = postcard::to_stdvec(&mod_empresa::DesativarUsuario { usuario })
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string()))?;
        aberta.motor().executar_bruto(
            &sessao,
            "empresa.desativar_usuario.v1",
            &carga,
            Some(ChaveIdempotencia::nova()),
        )?;
        aberta.esquecer_usuario(usuario);
        if let Some(conta) = s.diretorio.desvincular(empresa, usuario)? {
            s.sessoes.esquecer_conta(conta);
        }
        tracing::info!(%empresa, %usuario, "membro removido");
        Ok(())
    })
    .await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
