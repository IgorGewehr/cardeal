//! `POST /v1/e/{empresa}/empresas`: cadastra outro CNPJ na organização da `empresa`
//! (ADR-0017). Duas bases mudam: o CNPJ, o plano de contas e o admin na base da organização,
//! e o registro e o vínculo no diretório. Quem pede vira administrador do CNPJ novo e já o vê
//! no seletor.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::HeaderMap;
use cardeal_protocol::{EmpresaAcessivel, PedidoNovaEmpresa};

use super::credencial::exigir_token;
use super::despacho::{empresa_da_rota, entrar_na_empresa};
use super::resposta::{bloqueante, ler, ErroHttp, Postcard};
use crate::Servidor;

pub(super) async fn adicionar(
    State(servidor): State<Arc<Servidor>>,
    Path(empresa): Path<String>,
    headers: HeaderMap,
    corpo: Bytes,
) -> Result<Postcard, ErroHttp> {
    let token = exigir_token(&headers)?;
    let empresa = empresa_da_rota(&empresa)?;
    let pedido: PedidoNovaEmpresa = ler(&corpo)?;
    let s = Arc::clone(&servidor);
    let nova = bloqueante(move || {
        let conta = s.sessoes.resolver(&s.diretorio, &token)?;
        let (base, usuario) = conta.acesso_em(empresa)?;
        let (aberta, sessao) = entrar_na_empresa(&s, &token, empresa)?;
        // O motor confere a permissão e o CNPJ repetido antes de gravar qualquer coisa.
        let nova = aberta
            .motor()
            .adicionar_empresa(&sessao, &pedido.razao_social, &pedido.cnpj)?;
        let nome = pedido.razao_social.trim().to_owned();
        s.diretorio.registrar_empresa_na_base(nova, &nome, base)?;
        s.diretorio.vincular(conta.conta, nova, usuario)?;
        s.sessoes.esquecer_conta(conta.conta);
        tracing::info!(%base, %nova, conta = %conta.conta, "CNPJ adicionado à organização");
        Ok(EmpresaAcessivel { id: nova, nome })
    })
    .await?;
    servidor.tempo_real.avisar(empresa);
    super::resposta::postcard(&nova)
}
