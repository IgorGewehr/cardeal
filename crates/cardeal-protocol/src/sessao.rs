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

/// `GET /v1/e/{empresa}/sessao`: quem a conta é **dentro** de uma empresa — para a interface
/// mostrar o nome e esconder o que o papel não permite. O servidor continua decidindo cada
/// comando; isto é só conveniência de tela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InfoSessao {
    /// O usuário da conta nesta empresa.
    pub usuario: Id,
    /// As permissões concedidas pelos papéis dele.
    pub permissoes: Vec<String>,
}

/// `POST /v1/sessao/senha`: a própria conta troca a senha. As **outras** sessões da conta são
/// encerradas (se a senha vazou, quem entrou com ela cai); a sessão de quem trocou continua.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedidoTrocaSenha {
    /// A senha atual — prova de que é a própria pessoa, não só alguém com a sessão aberta.
    pub atual: String,
    /// A senha nova (política: mínimo 8 caracteres, fora da lista de senhas comuns).
    pub nova: String,
}

/// `POST /v1/e/{empresa}/membros`: o administrador põe alguém para trabalhar na empresa.
/// Um e-mail que já tem conta (um contador, um funcionário de outra loja) só ganha o vínculo;
/// um e-mail novo exige `senha_inicial`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PedidoNovoMembro {
    /// E-mail da pessoa — é o login dela.
    pub email: String,
    /// Nome exibido.
    pub nome: String,
    /// O papel nesta empresa.
    pub papel: cardeal_auth::PapelDeFabrica,
    /// Senha da conta nova (ignorada se o e-mail já tem conta).
    pub senha_inicial: Option<String>,
}

/// Resposta de [`PedidoNovoMembro`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembroAdicionado {
    /// O usuário da pessoa nesta empresa.
    pub usuario: Id,
    /// Se a conta foi criada agora (senão, era uma conta existente que ganhou o vínculo).
    pub conta_nova: bool,
}
