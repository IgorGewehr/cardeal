//! O que os testes de ponta a ponta do servidor compartilham: um servidor com duas empresas
//! (Ana e Beto) e chamadas HTTP pelo roteador, sem socket.
#![allow(dead_code)] // cada arquivo de teste usa uma parte

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use cardeal_kernel::{Erro, Id};
use cardeal_protocol::{
    rota_comando, rota_consulta, PedidoLogin, RespostaLogin, TipoCliente, CABECALHO_IDEMPOTENCIA,
    CABECALHO_PROTOCOLO, ROTA_SESSAO, TIPO_POSTCARD, VERSAO,
};
use cardeal_server::provisionamento::{provisionar, NovaEmpresa};
use cardeal_server::{roteador, ConfigServidor, Servidor};
use mod_clientes::{CriarPessoa, Papel, PessoasPorPapel, TipoPessoa};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tower::ServiceExt as _;

pub const SENHA: &str = "senha-forte-123";

pub struct Ambiente {
    pub _pasta: tempfile::TempDir,
    pub servidor: Arc<Servidor>,
    pub app: Router,
    pub empresa_ana: Id,
    pub empresa_beto: Id,
}

pub fn ambiente(ociosidade: Duration) -> Ambiente {
    ambiente_com(|c| c.ociosidade = ociosidade)
}

pub fn ambiente_com(ajuste: impl FnOnce(&mut ConfigServidor)) -> Ambiente {
    let pasta = tempfile::tempdir().unwrap();
    let mut config = ConfigServidor::em(pasta.path());
    ajuste(&mut config);
    let servidor = Servidor::abrir(config).unwrap();
    let nova = |empresa: &str, email: &str| NovaEmpresa {
        razao_social: empresa.into(),
        cnpj: "11222333000181".into(),
        email: email.into(),
        nome: "Pessoa".into(),
        senha: SENHA.into(),
    };
    let a = provisionar(
        servidor.config(),
        servidor.plano(),
        servidor.diretorio(),
        &nova("Loja da Ana", "ana@x.com"),
    )
    .unwrap();
    let b = provisionar(
        servidor.config(),
        servidor.plano(),
        servidor.diretorio(),
        &nova("Loja do Beto", "beto@x.com"),
    )
    .unwrap();
    Ambiente {
        _pasta: pasta,
        app: roteador(Arc::clone(&servidor)),
        servidor,
        empresa_ana: a.empresa,
        empresa_beto: b.empresa,
    }
}

pub struct Resposta {
    pub status: StatusCode,
    pub cabecalhos: axum::http::HeaderMap,
    pub corpo: Vec<u8>,
}

impl Resposta {
    pub fn valor<T: DeserializeOwned>(&self) -> T {
        assert_eq!(
            self.status,
            StatusCode::OK,
            "esperava 200, veio {}: {:?}",
            self.status,
            self.erro_opcional()
        );
        postcard::from_bytes(&self.corpo).unwrap()
    }
    pub fn erro_opcional(&self) -> Option<Erro> {
        postcard::from_bytes(&self.corpo).ok()
    }
    pub fn erro(&self) -> Erro {
        self.erro_opcional().expect("corpo de erro em postcard")
    }
}

pub async fn chamar(
    app: &Router,
    metodo: Method,
    uri: &str,
    token: Option<&str>,
    extras: &[(&str, String)],
    corpo: Vec<u8>,
) -> Resposta {
    let mut req = Request::builder()
        .method(metodo)
        .uri(uri)
        .header(header::CONTENT_TYPE, TIPO_POSTCARD)
        .header(CABECALHO_PROTOCOLO, VERSAO.to_string());
    if let Some(t) = token {
        req = req.header(header::AUTHORIZATION, format!("Bearer {t}"));
    }
    for (k, v) in extras {
        req = req.header(*k, v);
    }
    let resp = app
        .clone()
        .oneshot(req.body(Body::from(corpo)).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let cabecalhos = resp.headers().clone();
    let corpo = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap()
        .to_vec();
    Resposta {
        status,
        cabecalhos,
        corpo,
    }
}

pub async fn login(app: &Router, email: &str, senha: &str, cliente: TipoCliente) -> Resposta {
    let pedido = PedidoLogin {
        email: email.into(),
        senha: senha.into(),
        cliente,
    };
    chamar(
        app,
        Method::POST,
        ROTA_SESSAO,
        None,
        &[],
        postcard::to_stdvec(&pedido).unwrap(),
    )
    .await
}

pub async fn token(app: &Router, email: &str) -> String {
    login(app, email, SENHA, TipoCliente::Nativo)
        .await
        .valor::<RespostaLogin>()
        .token
        .unwrap()
}

pub fn criar_pessoa(nome: &str) -> CriarPessoa {
    CriarPessoa {
        tipo: TipoPessoa::Fisica,
        nome: nome.into(),
        nome_fantasia: None,
        papel_inicial: Papel::Cliente,
        documento_tipo: None,
        documento_numero: None,
        data_nascimento: None,
        endereco: None,
        contato: None,
    }
}

pub async fn comando<C: Serialize>(
    app: &Router,
    token: &str,
    empresa: Id,
    nome: &str,
    c: &C,
    chave: Id,
) -> Resposta {
    chamar(
        app,
        Method::POST,
        &rota_comando(empresa, nome),
        Some(token),
        &[(CABECALHO_IDEMPOTENCIA, chave.to_string())],
        postcard::to_stdvec(c).unwrap(),
    )
    .await
}

pub async fn clientes(app: &Router, token: &str, empresa: Id) -> Resposta {
    let q = PessoasPorPapel {
        papel: Papel::Cliente,
        busca: None,
    };
    chamar(
        app,
        Method::POST,
        &rota_consulta(empresa, "clientes.pessoas_por_papel.v1"),
        Some(token),
        &[],
        postcard::to_stdvec(&q).unwrap(),
    )
    .await
}
