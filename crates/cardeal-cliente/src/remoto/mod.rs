//! O cliente remoto: o motor está num `cardeal-server` (ADR-0016), do outro lado da rede.
//!
//! - [`protocolo`] monta pedidos e lê respostas, sem I/O — serve ao desktop e ao navegador.
//! - [`Remoto`] é o cliente **síncrono** do desktop, sobre um [`Transporte`].
//! - [`tempo_real`] lê o fluxo de alterações; [`Remoto::acompanhar`] o mantém conectado.
//!
//! Queda de rede é rotina: um pedido que não chegou (ou cuja resposta não voltou) é reenviado
//! até [`TENTATIVAS`] vezes. Para comando isso é seguro porque toda tentativa leva a **mesma**
//! chave de idempotência — se a primeira chegou e só a resposta se perdeu, o servidor devolve
//! a resposta original em vez de vender duas vezes.

pub mod protocolo;
pub mod tempo_real;

#[cfg(not(target_arch = "wasm32"))]
mod acompanhamento;

#[cfg(not(target_arch = "wasm32"))]
mod nativo;

use std::collections::BTreeSet;
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use cardeal_kernel::{ChaveIdempotencia, CodigoErro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Consulta};
use cardeal_protocol::{EmpresaAcessivel, InfoSessao, PedidoLogin, RespostaLogin, TipoCliente};
use serde::de::DeserializeOwned;
use serde::Serialize;

#[cfg(not(target_arch = "wasm32"))]
pub use acompanhamento::Assinatura;
#[cfg(not(target_arch = "wasm32"))]
pub use nativo::TransporteHttp;
pub use protocolo::{Metodo, Pedido, Resposta};
pub use tempo_real::{AvisoTempoReal, LeitorSse};

/// Quantas vezes um pedido é tentado antes de desistir com `SEM_CONEXAO`.
pub const TENTATIVAS: u32 = 3;

/// Quem leva um [`Pedido`] ao servidor e traz a [`Resposta`].
pub trait Transporte: Send + Sync {
    /// Envia. `Err` só para falha de **rede** (não chegou / não voltou); um erro do servidor é
    /// uma `Resposta` com status de erro.
    ///
    /// # Errors
    /// A descrição da falha de rede.
    fn enviar(&self, pedido: &Pedido, token: Option<&str>) -> Result<Resposta, String>;

    /// Abre o fluxo de tempo real (um `GET` que não termina) em `caminho`, continuando de
    /// `ultimo_id`. O padrão é não ter: um transporte de teste não precisa.
    ///
    /// # Errors
    /// A descrição da falha de rede.
    fn abrir_fluxo(
        &self,
        caminho: &str,
        token: Option<&str>,
        ultimo_id: Option<u64>,
    ) -> Result<Fluxo, String> {
        let _ = (caminho, token, ultimo_id);
        Err("este transporte não acompanha alterações".into())
    }
}

/// O resultado de [`Transporte::abrir_fluxo`].
pub enum Fluxo {
    /// Conectado: os bytes do `text/event-stream`.
    Aberto(Box<dyn std::io::Read + Send>),
    /// O servidor recusou (sessão, permissão, versão) — a resposta de erro.
    Recusado(Resposta),
}

/// O que a conta pode fazer numa empresa — espelho, para a interface, do que o servidor decide.
#[derive(Debug, Clone)]
pub struct SessaoRemota {
    usuario: Id,
    permissoes: BTreeSet<String>,
}

impl SessaoRemota {
    /// O usuário da conta nesta empresa.
    #[must_use]
    pub const fn usuario(&self) -> Id {
        self.usuario
    }

    /// Se o papel concede a permissão (só para mostrar/esconder — quem decide é o servidor).
    #[must_use]
    pub fn concede(&self, permissao: &str) -> bool {
        self.permissoes.contains(permissao)
    }
}

impl From<InfoSessao> for SessaoRemota {
    fn from(i: InfoSessao) -> Self {
        Self {
            usuario: i.usuario,
            permissoes: i.permissoes.into_iter().collect(),
        }
    }
}

/// Uma conexão autenticada com o servidor.
pub struct Remoto {
    transporte: std::sync::Arc<dyn Transporte>,
    token: String,
    nome: String,
    empresas: Vec<EmpresaAcessivel>,
    empresa: Option<Id>,
}

fn sem_empresa() -> Erro {
    Erro::novo(
        CodigoErro::ESTADO_INVALIDO,
        "escolha uma empresa antes de continuar",
    )
}

impl Remoto {
    /// Faz login com e-mail e senha.
    ///
    /// # Errors
    /// Credencial inválida, conta bloqueada, sem conexão.
    pub fn entrar(transporte: Box<dyn Transporte>, email: &str, senha: &str) -> Resultado<Self> {
        let pedido = protocolo::login(&PedidoLogin {
            email: email.to_owned(),
            senha: senha.to_owned(),
            cliente: TipoCliente::Nativo,
        })?;
        // Login não é reenviado: uma senha errada repetida conta como três tentativas.
        let resposta = transporte.enviar(&pedido, None).map_err(sem_conexao)?;
        let login: RespostaLogin = protocolo::interpretar(&resposta)?;
        let token = login.token.ok_or_else(|| {
            Erro::novo(
                CodigoErro::FALHA_INTERNA,
                "o servidor não devolveu o token de sessão",
            )
        })?;
        Ok(Self {
            transporte: transporte.into(),
            token,
            nome: login.nome,
            empresa: (login.empresas.len() == 1).then(|| login.empresas[0].id),
            empresas: login.empresas,
        })
    }

    /// O nome da pessoa.
    #[must_use]
    pub fn nome(&self) -> &str {
        &self.nome
    }

    /// As empresas em que a conta entra.
    #[must_use]
    pub fn empresas(&self) -> &[EmpresaAcessivel] {
        &self.empresas
    }

    /// A empresa escolhida, se já houver.
    #[must_use]
    pub const fn empresa(&self) -> Option<Id> {
        self.empresa
    }

    /// Entra numa empresa e traz a sessão (usuário + permissões) dela.
    ///
    /// # Errors
    /// Sem acesso à empresa; sem conexão.
    pub fn escolher_empresa(&mut self, empresa: Id) -> Resultado<SessaoRemota> {
        let info: InfoSessao = self.pedir(&protocolo::sessao_empresa(empresa))?;
        self.empresa = Some(empresa);
        Ok(info.into())
    }

    /// Executa um comando na empresa escolhida.
    ///
    /// # Errors
    /// O erro de domínio do servidor; `SEM_CONEXAO` depois de [`TENTATIVAS`].
    pub fn executar<C>(&self, nome: &str, comando: &C) -> Resultado<C::Saida>
    where
        C: Comando + Serialize,
        C::Saida: DeserializeOwned,
    {
        let empresa = self.empresa.ok_or_else(sem_empresa)?;
        // Uma chave por intenção — gerada aqui, fora do laço de tentativas.
        let chave = ChaveIdempotencia::nova();
        self.pedir(&protocolo::comando(
            empresa,
            nome,
            protocolo::carga(comando)?,
            chave,
        ))
    }

    /// Executa uma consulta na empresa escolhida.
    ///
    /// # Errors
    /// O erro de domínio do servidor; `SEM_CONEXAO` depois de [`TENTATIVAS`].
    pub fn consultar<Q>(&self, nome: &str, consulta: &Q) -> Resultado<Q::Saida>
    where
        Q: Consulta + Serialize,
        Q::Saida: DeserializeOwned,
    {
        let empresa = self.empresa.ok_or_else(sem_empresa)?;
        self.pedir(&protocolo::consulta(
            empresa,
            nome,
            protocolo::carga(consulta)?,
        ))
    }

    /// Cadastra outro CNPJ na organização da empresa escolhida (ADR-0017); quem pede vira
    /// administrador dele, e ele já entra na lista de [`Self::empresas`].
    ///
    /// # Errors
    /// Sem permissão; CNPJ já cadastrado; sem conexão.
    pub fn adicionar_empresa(
        &mut self,
        razao_social: &str,
        cnpj: &str,
    ) -> Resultado<EmpresaAcessivel> {
        let empresa = self.empresa.ok_or_else(sem_empresa)?;
        let nova: EmpresaAcessivel =
            self.pedir(&protocolo::adicionar_empresa(empresa, razao_social, cnpj)?)?;
        self.empresas.push(nova.clone());
        Ok(nova)
    }

    /// Troca a senha da conta. As outras sessões da conta caem; esta continua. Não é
    /// reenviado em falha de rede (uma senha atual errada repetida contaria como tentativas).
    ///
    /// # Errors
    /// Senha atual errada, nova fora da política, sem conexão.
    pub fn trocar_senha(&self, atual: &str, nova: &str) -> Resultado<()> {
        let pedido = protocolo::trocar_senha(atual, nova)?;
        let resposta = self
            .transporte
            .enviar(&pedido, Some(&self.token))
            .map_err(sem_conexao)?;
        protocolo::interpretar_vazio(&resposta)
    }

    /// Encerra a sessão no servidor (melhor esforço: sem rede, o token expira sozinho).
    pub fn sair(self) {
        let _ = self
            .transporte
            .enviar(&protocolo::logout(), Some(&self.token));
    }

    fn pedir<T: DeserializeOwned>(&self, pedido: &Pedido) -> Resultado<T> {
        let resposta = self.com_retentativa(pedido)?;
        protocolo::interpretar(&resposta)
    }

    fn com_retentativa(&self, pedido: &Pedido) -> Resultado<Resposta> {
        let mut ultima = String::new();
        let tentativas = if pedido.reenviavel { TENTATIVAS } else { 1 };
        for tentativa in 0..tentativas {
            if tentativa > 0 {
                espera(tentativa);
            }
            match self.transporte.enviar(pedido, Some(&self.token)) {
                Ok(r) => return Ok(r),
                Err(e) => {
                    tracing::warn!(tentativa, caminho = %pedido.caminho, erro = %e, "falha de rede");
                    ultima = e;
                }
            }
        }
        Err(sem_conexao(ultima))
    }
}

#[allow(clippy::needless_pass_by_value)]
fn sem_conexao(detalhe: String) -> Erro {
    Erro::novo(
        CodigoErro::SEM_CONEXAO,
        format!("sem conexão com o servidor — verifique a internet ({detalhe})"),
    )
}

/// Recuo entre tentativas: 200 ms, 600 ms.
#[cfg(not(target_arch = "wasm32"))]
fn espera(tentativa: u32) {
    std::thread::sleep(Duration::from_millis(
        200 * u64::from(3u32.pow(tentativa - 1)),
    ));
}

#[cfg(target_arch = "wasm32")]
fn espera(_tentativa: u32) {}
