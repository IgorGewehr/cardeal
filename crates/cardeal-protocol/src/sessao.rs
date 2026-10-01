//! Login por e-mail e senha (ADR-0016).

use cardeal_kernel::Id;
use serde::{Deserialize, Serialize};

/// Quem está logando — decide **como** o token volta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum TipoCliente {
    /// Navegador: o token vai **só** no cookie `HttpOnly`, nunca no corpo — um script
    /// injetado na página não tem como lê-lo.
    Navegador,
    /// Desktop nativo: o token volta no corpo e o cliente o envia em `Authorization: Bearer`.
    Nativo,
}

/// `POST /v1/sessao`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedidoLogin {
    /// E-mail da conta (comparado sem diferenciar maiúsculas).
    pub email: String,
    /// Senha em texto — só atravessa TLS; o servidor guarda apenas o hash Argon2id.
    pub senha: String,
    /// Navegador ou nativo.
    pub cliente: TipoCliente,
}

/// Uma empresa em que a conta pode entrar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EmpresaAcessivel {
    /// Identidade da empresa — vai na rota de cada comando/consulta.
    pub id: Id,
    /// Nome para o seletor de empresa.
    pub nome: String,
}

/// Resposta de um login bem-sucedido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RespostaLogin {
    /// Nome da pessoa, para a saudação.
    pub nome: String,
    /// As empresas acessíveis. Com uma só, a interface entra direto; com mais, oferece o
    /// seletor — trocar de empresa não exige novo login.
    pub empresas: Vec<EmpresaAcessivel>,
    /// O token, **só** para [`TipoCliente::Nativo`].
    pub token: Option<String>,
}
