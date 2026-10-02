//! A fachada que as telas usam: [`Motor`] e [`Sessao`], com o motor **no próprio processo**
//! (monoposto, ADR-0009) ou **num servidor** (ADR-0016). A tela chama
//! `motor.executar(sessao, nome, &comando)` igual nos dois casos.

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Consulta};
use mod_empresa::{
    AtualizarDadosEmpresa, DadosDaEmpresa, DefinirContatoEmpresa, DefinirLogoEmpresa,
    EmpresaResumo, IdentidadeDaEmpresa, IdentidadeVisual, PapeisDaEmpresa, PapelResumo,
    UsuarioResumo, UsuariosDaEmpresa,
};
use serde::de::DeserializeOwned;
use serde::Serialize;

#[cfg(feature = "local")]
use cardeal_motor::{MotorLocal, SessaoLocal};

#[cfg(not(target_arch = "wasm32"))]
use crate::remoto::{Remoto, SessaoRemota};

/// Onde o motor está.
pub enum Motor {
    /// No próprio processo — o arquivo `.db` é local.
    #[cfg(feature = "local")]
    Local(MotorLocal),
    /// Num `cardeal-server`, pela rede.
    #[cfg(not(target_arch = "wasm32"))]
    Remoto(Remoto),
}

/// Uma sessão autenticada.
pub enum Sessao {
    /// Do motor local.
    #[cfg(feature = "local")]
    Local(SessaoLocal),
    /// De uma empresa no servidor.
    #[cfg(not(target_arch = "wasm32"))]
    Remota(SessaoRemota),
}

impl Sessao {
    /// O usuário autenticado.
    #[must_use]
    pub fn usuario(&self) -> Id {
        match self {
            #[cfg(feature = "local")]
            Self::Local(s) => s.usuario(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remota(s) => s.usuario(),
        }
    }

    /// Se a sessão concede a permissão.
    #[must_use]
    pub fn concede(&self, permissao: &str) -> bool {
        match self {
            #[cfg(feature = "local")]
            Self::Local(s) => s.concede(permissao),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remota(s) => s.concede(permissao),
        }
    }
}

#[cfg(feature = "local")]
impl From<MotorLocal> for Motor {
    fn from(m: MotorLocal) -> Self {
        Self::Local(m)
    }
}

#[cfg(feature = "local")]
impl From<SessaoLocal> for Sessao {
    fn from(s: SessaoLocal) -> Self {
        Self::Local(s)
    }
}

#[cfg(feature = "local")]
fn sessao_de_outro_motor() -> Erro {
    Erro::novo(
        CodigoErro::SESSAO_INVALIDA,
        "esta sessão é de outro motor — entre de novo",
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn so_no_local(o_que: &str) -> Erro {
    Erro::novo(
        CodigoErro::ESTADO_INVALIDO,
        format!("{o_que} só existe com o banco local — no servidor, peça ao administrador"),
    )
}

impl Motor {
    /// Abre o motor local sobre um arquivo (monoposto).
    ///
    /// # Errors
    /// Erro de abertura/migração.
    #[cfg(feature = "local")]
    pub fn abrir(
        caminho: &std::path::Path,
        modulos: &[&dyn cardeal_modkit::Modulo],
        pedido: &cardeal_modkit::PedidoAtivacao,
    ) -> Resultado<Self> {
        let motor = MotorLocal::abrir(caminho, modulos, pedido)?;
        // Faxina do monoposto ao abrir (no servidor ela roda no despejo da empresa): respostas
        // idempotentes e alterações do tempo real com mais de uma semana. Falhar aqui não
        // impede ninguém de trabalhar — tenta de novo na próxima abertura.
        let semana = cardeal_kernel::Instante::agora().mais_segundos(-7 * 24 * 60 * 60);
        let _ = motor.podar_idempotencia(semana);
        let _ = motor.podar_alteracoes(semana);
        Ok(Self::Local(motor))
    }

    /// Verdadeiro se a base local ainda não tem empresa (assistente de primeiro acesso). No
    /// servidor, a empresa já nasce provisionada: sempre `false`.
    ///
    /// # Errors
    /// Erro de leitura.
    pub fn precisa_de_configuracao_inicial(&self) -> Resultado<bool> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.precisa_de_configuracao_inicial(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(_) => Ok(false),
        }
    }

    /// O primeiro acesso da base local: empresa, plano de contas e administrador.
    ///
    /// # Errors
    /// Senha fora da política; erro de escrita; no servidor, não se aplica.
    #[cfg(feature = "local")]
    pub fn configurar_inicial(
        &self,
        razao_social: &str,
        cnpj: &str,
        admin_login: &str,
        admin_nome: &str,
        admin_senha: &str,
    ) -> Resultado<Id> {
        match self {
            Self::Local(m) => {
                m.configurar_inicial(razao_social, cnpj, admin_login, admin_nome, admin_senha)
            }
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(_) => Err(so_no_local("O primeiro acesso")),
        }
    }

    /// O motor local, quando é o caso (primeiro acesso, ferramentas que só existem nele).
    #[cfg(feature = "local")]
    #[must_use]
    pub const fn local(&self) -> Option<&MotorLocal> {
        match self {
            Self::Local(m) => Some(m),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(_) => None,
        }
    }

    /// Verdadeiro quando o motor está num servidor.
    #[must_use]
    pub const fn e_remoto(&self) -> bool {
        match self {
            #[cfg(feature = "local")]
            Self::Local(_) => false,
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(_) => true,
        }
    }

    /// Executa um comando.
    ///
    /// # Errors
    /// Sem permissão, erro de domínio, sem conexão (remoto).
    pub fn executar<C>(&self, sessao: &Sessao, nome: &str, comando: &C) -> Resultado<C::Saida>
    where
        C: Comando + Serialize,
        C::Saida: DeserializeOwned,
    {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => m.executar(s, nome, comando),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.executar(nome, comando),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// Executa uma consulta.
    ///
    /// # Errors
    /// Sem permissão, erro de domínio, sem conexão (remoto).
    pub fn consultar<Q>(&self, sessao: &Sessao, nome: &str, consulta: &Q) -> Resultado<Q::Saida>
    where
        Q: Consulta + Serialize,
        Q::Saida: DeserializeOwned,
    {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => m.consultar(s, nome, consulta),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.consultar(nome, consulta),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// Autentica um segundo usuário (supervisor que libera uma ação do operador, `F8` do PDV).
    ///
    /// # Errors
    /// Credencial inválida; no servidor, ainda não suportado.
    pub fn autenticar(&self, login: &str, senha: &str) -> Resultado<Sessao> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.autenticar(login, senha).map(Sessao::Local),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(_) => {
                let _ = (login, senha);
                Err(so_no_local("A liberação por supervisor"))
            }
        }
    }

    /// Os dados cadastrais da empresa.
    ///
    /// # Errors
    /// Erro de leitura; sem conexão.
    pub fn empresa_resumo(&self) -> Resultado<EmpresaResumo> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.empresa_resumo(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(r) => r.consultar("empresa.dados.v1", &DadosDaEmpresa),
        }
    }

    /// A identidade visual (cabeçalho dos PDFs).
    ///
    /// # Errors
    /// Erro de leitura; sem conexão.
    pub fn identidade_visual(&self) -> Resultado<IdentidadeVisual> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.identidade_visual(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(r) => r.consultar("empresa.identidade_visual.v1", &IdentidadeDaEmpresa),
        }
    }

    /// Os usuários da empresa.
    ///
    /// # Errors
    /// Erro de leitura; sem conexão.
    pub fn usuarios(&self) -> Resultado<Vec<UsuarioResumo>> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.usuarios(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(r) => r.consultar("empresa.usuarios.v1", &UsuariosDaEmpresa),
        }
    }

    /// Os papéis da empresa.
    ///
    /// # Errors
    /// Erro de leitura; sem conexão.
    pub fn papeis(&self) -> Resultado<Vec<PapelResumo>> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.papeis(),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(r) => r.consultar("empresa.papeis.v1", &PapeisDaEmpresa),
        }
    }

    /// Atualiza razão social, nome fantasia e regime.
    ///
    /// # Errors
    /// Erro de escrita; sem permissão; sem conexão.
    pub fn atualizar_empresa(
        &self,
        razao_social: &str,
        nome_fantasia: &str,
        regime: &str,
    ) -> Resultado<()> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.atualizar_empresa(razao_social, nome_fantasia, regime),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(r) => r.executar(
                "empresa.atualizar_dados.v1",
                &AtualizarDadosEmpresa {
                    razao_social: razao_social.to_owned(),
                    nome_fantasia: nome_fantasia.to_owned(),
                    regime: regime.to_owned(),
                },
            ),
        }
    }

    /// Grava telefone, e-mail, site e endereço de exibição.
    ///
    /// # Errors
    /// Erro de escrita; sem permissão; sem conexão.
    pub fn definir_contato_empresa(
        &self,
        telefone: &str,
        email: &str,
        site: &str,
        endereco: &str,
    ) -> Resultado<()> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.definir_contato_empresa(telefone, email, site, endereco),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(r) => r.executar(
                "empresa.definir_contato.v1",
                &DefinirContatoEmpresa {
                    telefone: telefone.to_owned(),
                    email: email.to_owned(),
                    site: site.to_owned(),
                    endereco: endereco.to_owned(),
                },
            ),
        }
    }

    /// Grava (ou remove) a logo.
    ///
    /// # Errors
    /// Não é PNG / grande demais; erro de escrita; sem conexão.
    pub fn definir_logo_empresa(&self, png: Option<&[u8]>) -> Resultado<()> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.definir_logo_empresa(png),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(r) => r.executar(
                "empresa.definir_logo.v1",
                &DefinirLogoEmpresa {
                    png: png.map(<[u8]>::to_vec),
                },
            ),
        }
    }
}
