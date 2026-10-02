//! `POST /v1/e/{empresa}/cmd/{nome}` e `POST /v1/e/{empresa}/qry/{nome}`.
//!
//! Tudo o que toca disco roda numa única ida ao pool de bloqueio: resolver a sessão (cache),
//! conferir o vínculo, obter a empresa (abrindo se fria) e despachar.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use cardeal_kernel::{ChaveIdempotencia, CodigoErro, Erro, Id, Resultado};
use cardeal_protocol::{InfoSessao, CABECALHO_IDEMPOTENCIA};

use super::credencial::exigir_token;
use super::resposta::{bloqueante, ErroHttp, Postcard};
use crate::frota::EmpresaAberta;
use crate::Servidor;
use cardeal_motor::SessaoLocal;

pub(super) fn empresa_da_rota(texto: &str) -> Result<Id, ErroHttp> {
    texto.parse::<Id>().map_err(|_| {
        ErroHttp(Erro::novo(
            CodigoErro::ENTRADA_INVALIDA,
            "identificador de empresa inválido",
        ))
    })
}

fn chave(headers: &HeaderMap) -> Result<ChaveIdempotencia, ErroHttp> {
    headers
        .get(CABECALHO_IDEMPOTENCIA)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<Id>().ok())
        .map(ChaveIdempotencia::de_id)
        .ok_or_else(|| {
            ErroHttp(
                Erro::novo(
                    CodigoErro::CAMPO_OBRIGATORIO,
                    format!("todo comando exige o cabeçalho {CABECALHO_IDEMPOTENCIA} (UUID)"),
                )
                .no_campo(CABECALHO_IDEMPOTENCIA),
            )
        })
}

/// Sessão de conta → vínculo → empresa aberta → sessão do usuário nela. Bloqueante.
pub(super) fn entrar_na_empresa(
    servidor: &Servidor,
    token: &str,
    empresa: Id,
) -> Resultado<(Arc<EmpresaAberta>, Arc<SessaoLocal>)> {
    let conta = servidor.sessoes.resolver(&servidor.diretorio, token)?;
    let usuario = conta.usuario_em(empresa)?;
    let aberta = servidor.frota.obter(empresa)?;
    let sessao = aberta.sessao(conta.id, usuario)?;
    Ok((aberta, sessao))
}

pub(super) async fn comando(
    State(servidor): State<Arc<Servidor>>,
    Path((empresa, nome)): Path<(String, String)>,
    headers: HeaderMap,
    corpo: Bytes,
) -> Result<Postcard, ErroHttp> {
    let token = exigir_token(&headers)?;
    let chave = chave(&headers)?;
    let empresa = empresa_da_rota(&empresa)?;
    let s = Arc::clone(&servidor);
    let saida = bloqueante(move || {
        let (aberta, sessao) = entrar_na_empresa(&s, &token, empresa)?;
        aberta
            .motor()
            .executar_bruto(&sessao, &nome, &corpo, Some(chave))
    })
    .await?;
    // Já confirmado e no outbox: acorda quem acompanha a empresa (ver `tempo_real`).
    servidor.tempo_real.avisar(empresa);
    Ok(Postcard(saida))
}

pub(super) async fn consulta(
    State(servidor): State<Arc<Servidor>>,
    Path((empresa, nome)): Path<(String, String)>,
    headers: HeaderMap,
    corpo: Bytes,
) -> Result<Postcard, ErroHttp> {
    let token = exigir_token(&headers)?;
    let empresa = empresa_da_rota(&empresa)?;
    bloqueante(move || {
        let (aberta, sessao) = entrar_na_empresa(&servidor, &token, empresa)?;
        aberta.motor().consultar_bruto(&sessao, &nome, &corpo)
    })
    .await
    .map(Postcard)
}

/// Quem a conta é nesta empresa: usuário e permissões, para a interface.
pub(super) async fn sessao(
    State(servidor): State<Arc<Servidor>>,
    Path(empresa): Path<String>,
    headers: HeaderMap,
) -> Result<Postcard, ErroHttp> {
    let token = exigir_token(&headers)?;
    let empresa = empresa_da_rota(&empresa)?;
    let info = bloqueante(move || {
        let (_, sessao) = entrar_na_empresa(&servidor, &token, empresa)?;
        Ok(InfoSessao {
            usuario: sessao.usuario(),
            permissoes: sessao.permissoes().map(str::to_owned).collect(),
        })
    })
    .await?;
    super::resposta::postcard(&info)
}
