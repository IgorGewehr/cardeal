//! O protocolo do cliente remoto **sem I/O**: monta pedidos e interpreta respostas.
//!
//! Quem envia é um [`Transporte`](super::Transporte) — `reqwest` no desktop, `fetch` no
//! navegador. Separar as duas coisas é o que deixa o mesmo protocolo (rotas, cabeçalhos,
//! idempotência, leitura de erro) servir a um cliente síncrono e a um assíncrono, testado uma
//! vez só.

use cardeal_kernel::{ChaveIdempotencia, CodigoErro, Erro, Id, Resultado};
use cardeal_protocol::{
    rota_comando, rota_consulta, rota_sessao_empresa, CABECALHO_IDEMPOTENCIA, CABECALHO_PROTOCOLO,
    ROTA_SESSAO, TIPO_POSTCARD, VERSAO,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

/// O verbo HTTP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metodo {
    /// `GET`.
    Get,
    /// `POST`.
    Post,
    /// `DELETE`.
    Delete,
}

/// Um pedido pronto para enviar.
#[derive(Debug, Clone)]
pub struct Pedido {
    /// O verbo.
    pub metodo: Metodo,
    /// O caminho, relativo à base do servidor (`/v1/...`).
    pub caminho: String,
    /// Cabeçalhos além dos que o transporte põe (autorização).
    pub cabecalhos: Vec<(&'static str, String)>,
    /// O corpo, em `postcard`.
    pub corpo: Vec<u8>,
}

/// Uma resposta recebida.
#[derive(Debug, Clone)]
pub struct Resposta {
    /// O status HTTP.
    pub status: u16,
    /// O corpo.
    pub corpo: Vec<u8>,
}

fn base(metodo: Metodo, caminho: String, corpo: Vec<u8>) -> Pedido {
    Pedido {
        metodo,
        caminho,
        cabecalhos: vec![
            (CABECALHO_PROTOCOLO, VERSAO.to_string()),
            ("content-type", TIPO_POSTCARD.to_owned()),
        ],
        corpo,
    }
}

/// Serializa uma carga em `postcard`.
///
/// # Errors
/// `VALOR_INVALIDO` se o tipo não serializa (não acontece com os tipos do domínio).
pub fn carga<T: Serialize>(valor: &T) -> Resultado<Vec<u8>> {
    postcard::to_stdvec(valor).map_err(|e| Erro::novo(CodigoErro::VALOR_INVALIDO, e.to_string()))
}

/// O pedido de login.
///
/// # Errors
/// Falha de serialização.
pub fn login(pedido: &cardeal_protocol::PedidoLogin) -> Resultado<Pedido> {
    Ok(base(Metodo::Post, ROTA_SESSAO.to_owned(), carga(pedido)?))
}

/// O pedido de logout.
#[must_use]
pub fn logout() -> Pedido {
    base(Metodo::Delete, ROTA_SESSAO.to_owned(), Vec::new())
}

/// O pedido da sessão numa empresa.
#[must_use]
pub fn sessao_empresa(empresa: Id) -> Pedido {
    base(Metodo::Get, rota_sessao_empresa(empresa), Vec::new())
}

/// Um comando: a mesma `chave` em toda tentativa — é o que torna o reenvio seguro.
#[must_use]
pub fn comando(empresa: Id, nome: &str, carga: Vec<u8>, chave: ChaveIdempotencia) -> Pedido {
    let mut p = base(Metodo::Post, rota_comando(empresa, nome), carga);
    p.cabecalhos
        .push((CABECALHO_IDEMPOTENCIA, chave.id().to_string()));
    p
}

/// Uma consulta.
#[must_use]
pub fn consulta(empresa: Id, nome: &str, carga: Vec<u8>) -> Pedido {
    base(Metodo::Post, rota_consulta(empresa, nome), carga)
}

/// Lê uma resposta: 2xx → o valor; senão, o `Erro` que o servidor mandou (o mesmo que a tela
/// mostraria no monoposto).
///
/// # Errors
/// O erro do servidor, ou `ENTRADA_INVALIDA` se o corpo não é o esperado.
pub fn interpretar<T: DeserializeOwned>(r: &Resposta) -> Resultado<T> {
    if (200..300).contains(&r.status) {
        return postcard::from_bytes(&r.corpo).map_err(|e| {
            Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                format!("resposta do servidor ilegível ({e}) — versões diferentes?"),
            )
        });
    }
    Err(postcard::from_bytes::<Erro>(&r.corpo).unwrap_or_else(|_| {
        Erro::novo(
            CodigoErro::FALHA_INTERNA,
            format!("o servidor respondeu {} sem detalhes", r.status),
        )
    }))
}

/// Uma resposta vazia de sucesso (`204`) também é sucesso.
///
/// # Errors
/// O erro do servidor.
pub fn interpretar_vazio(r: &Resposta) -> Resultado<()> {
    if (200..300).contains(&r.status) {
        Ok(())
    } else {
        interpretar::<()>(r)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn comando_leva_versao_e_chave_e_rota_da_empresa() {
        let e = Id::novo();
        let chave = ChaveIdempotencia::nova();
        let p = comando(e, "os.abrir_ordem.v1", vec![1, 2], chave);
        assert_eq!(p.metodo, Metodo::Post);
        assert_eq!(p.caminho, format!("/v1/e/{e}/cmd/os.abrir_ordem.v1"));
        assert!(p
            .cabecalhos
            .contains(&(CABECALHO_IDEMPOTENCIA, chave.id().to_string())));
        assert!(p.cabecalhos.iter().any(|(k, _)| *k == CABECALHO_PROTOCOLO));
    }

    #[test]
    fn erro_do_servidor_chega_intacto_e_lixo_vira_erro_legivel() {
        let erro = Erro::novo(CodigoErro::ESTOQUE_INSUFICIENTE, "sem saldo").no_campo("qtd");
        let r = Resposta {
            status: 409,
            corpo: postcard::to_stdvec(&erro).unwrap(),
        };
        assert_eq!(interpretar::<u32>(&r).unwrap_err(), erro);
        let lixo = Resposta {
            status: 502,
            corpo: b"<html>".to_vec(),
        };
        assert_eq!(
            interpretar::<u32>(&lixo).unwrap_err().codigo,
            CodigoErro::FALHA_INTERNA
        );
        let ok = Resposta {
            status: 200,
            corpo: postcard::to_stdvec(&7u32).unwrap(),
        };
        assert_eq!(interpretar::<u32>(&ok).unwrap(), 7);
    }
}
