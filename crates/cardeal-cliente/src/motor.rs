//! A fachada que as telas usam: [`Motor`] e [`Sessao`], com o motor **no próprio processo**
//! (monoposto, ADR-0009) ou **num servidor** (ADR-0016). A tela chama
//! `motor.executar(sessao, nome, &comando)` igual nos dois casos.

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Consulta};
use cardeal_protocol::SegundoFatorProposto;
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
/// Um CNPJ do grupo em que a pessoa entra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmpresaDoGrupo {
    /// O id da empresa.
    pub id: Id,
    /// O nome (razão social no local; o nome do vínculo no servidor).
    pub nome: String,
    /// O CNPJ (só no local).
    pub cnpj: Option<String>,
    /// A matriz.
    pub matriz: bool,
}

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
    /// Injeta os recursos da instalação (cofre, provedor fiscal) no motor local. No remoto
    /// não faz nada: os recursos são do servidor.
    #[must_use]
    #[cfg(feature = "local")]
    pub fn com_recursos(self, recursos: cardeal_modkit::Recursos) -> Self {
        match self {
            Self::Local(m) => Self::Local(m.com_recursos(recursos)),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(r) => {
                let _ = recursos;
                Self::Remoto(r)
            }
        }
    }

    /// Um executor de tarefas para rodar numa thread própria (a transmissão fiscal). `None` no
    /// remoto: lá as tarefas rodam no servidor.
    #[must_use]
    #[cfg(feature = "local")]
    pub fn executor_de_tarefas(&self, sessao: &Sessao) -> Option<cardeal_motor::ExecutorDeTarefas> {
        match (self, sessao) {
            (Self::Local(m), Sessao::Local(s)) => Some(m.executor_de_tarefas(s)),
            #[cfg(not(target_arch = "wasm32"))]
            _ => None,
        }
    }

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

    /// Os CNPJs do grupo em que a pessoa da sessão entra (no banco local, os da organização em
    /// que ela tem papel; no servidor, os vínculos da conta), matriz primeiro.
    ///
    /// # Errors
    /// Erro de leitura.
    pub fn empresas_do_grupo(&self, sessao: &Sessao) -> Resultado<Vec<EmpresaDoGrupo>> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => Ok(m
                .empresas()?
                .into_iter()
                .filter(|e| {
                    e.id == s.empresa()
                        || m.sessao_do_usuario_em(e.id, s.usuario(), Id::novo())
                            .is_ok()
                })
                .map(|e| EmpresaDoGrupo {
                    id: e.id,
                    nome: e.razao_social,
                    cnpj: Some(e.cnpj),
                    matriz: e.matriz,
                })
                .collect()),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => Ok(r
                .empresas()
                .iter()
                .map(|e| EmpresaDoGrupo {
                    id: e.id,
                    nome: e.nome.clone(),
                    cnpj: None,
                    matriz: false,
                })
                .collect()),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// A empresa (CNPJ) em que a sessão age.
    #[must_use]
    pub fn empresa_da_sessao(&self, sessao: &Sessao) -> Option<Id> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(_), Sessao::Local(s)) => Some(s.empresa()),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.empresa(),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => None,
        }
    }

    /// A sessão da mesma pessoa em outro CNPJ do grupo — trocar de empresa sem novo login.
    ///
    /// # Errors
    /// Sem papel na empresa; empresa de fora do grupo; sem conexão.
    pub fn trocar_empresa(&mut self, sessao: &Sessao, empresa: Id) -> Resultado<Sessao> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => {
                let nova = m.sessao_do_usuario_em(empresa, s.usuario(), Id::novo())?;
                m.ativar_empresa(empresa);
                Ok(Sessao::Local(nova))
            }
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.escolher_empresa(empresa).map(Sessao::Remota),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// Cadastra outro CNPJ no grupo (plano de contas próprio; quem pede vira o administrador
    /// dele). Exige `empresa.organizacao.gerenciar`. Devolve o id.
    ///
    /// # Errors
    /// Sem permissão; CNPJ já cadastrado; sem conexão.
    pub fn adicionar_empresa(
        &mut self,
        sessao: &Sessao,
        razao_social: &str,
        cnpj: &str,
    ) -> Resultado<Id> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => m.adicionar_empresa(s, razao_social, cnpj),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.adicionar_empresa(razao_social, cnpj).map(|e| e.id),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// Uma consulta em **outro** CNPJ do grupo, com as permissões que a pessoa tem lá — o que o
    /// painel consolidado usa, empresa por empresa.
    ///
    /// # Errors
    /// Sem papel ou sem permissão naquela empresa; o erro da consulta; sem conexão.
    pub fn consultar_em<Q>(
        &self,
        sessao: &Sessao,
        empresa: Id,
        nome: &str,
        consulta: &Q,
    ) -> Resultado<Q::Saida>
    where
        Q: Consulta + Serialize,
        Q::Saida: DeserializeOwned,
    {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => {
                let la = m.sessao_do_usuario_em(empresa, s.usuario(), Id::novo())?;
                m.consultar(&la, nome, consulta)
            }
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.consultar_em(empresa, nome, consulta),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// O **login** no banco local: a senha e, para quem tem verificação em duas etapas, o
    /// código do aplicativo (ou de recuperação). No servidor o login é o da conta
    /// (`remoto::Remoto::entrar`).
    ///
    /// # Errors
    /// `SEGUNDO_FATOR_NECESSARIO` (falta o código); credencial inválida; conta bloqueada.
    pub fn entrar(&self, login: &str, senha: &str, codigo: Option<&str>) -> Resultado<Sessao> {
        match self {
            #[cfg(feature = "local")]
            Self::Local(m) => m.entrar(login, senha, codigo).map(Sessao::Local),
            #[cfg(not(target_arch = "wasm32"))]
            Self::Remoto(_) => {
                let _ = (login, senha, codigo);
                Err(so_no_local("O login local"))
            }
        }
    }

    /// A pessoa da sessão tem verificação em duas etapas?
    ///
    /// # Errors
    /// Erro de leitura; sem conexão.
    pub fn segundo_fator_ativo(&self, sessao: &Sessao) -> Resultado<bool> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => m.segundo_fator_ativo(s),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.segundo_fator_ativo(),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// Um segredo novo para a pessoa da sessão levar ao aplicativo autenticador.
    ///
    /// # Errors
    /// Erro de leitura; sem conexão.
    pub fn propor_segundo_fator(&self, sessao: &Sessao) -> Resultado<SegundoFatorProposto> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => m.propor_segundo_fator(s),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.propor_segundo_fator(),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// Ativa a verificação em duas etapas da pessoa da sessão (o primeiro código confirma o
    /// segredo). Devolve os códigos de recuperação, para mostrar uma vez.
    ///
    /// # Errors
    /// Código que não confere; já ativa; sem conexão.
    pub fn ativar_segundo_fator(
        &self,
        sessao: &Sessao,
        segredo: &str,
        codigo: &str,
    ) -> Resultado<Vec<String>> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => m.ativar_segundo_fator(s, segredo, codigo),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.ativar_segundo_fator(segredo, codigo),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// Desativa a da pessoa da sessão: a senha e um código.
    ///
    /// # Errors
    /// Senha ou código que não confere; sem conexão.
    pub fn desativar_segundo_fator(
        &self,
        sessao: &Sessao,
        senha: &str,
        codigo: &str,
    ) -> Resultado<()> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => m.desativar_segundo_fator(s, senha, codigo),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(r), _) => r.desativar_segundo_fator(senha, codigo),
            #[cfg(feature = "local")]
            #[allow(unreachable_patterns)]
            _ => Err(sessao_de_outro_motor()),
        }
    }

    /// O administrador desliga a verificação em duas etapas de outro usuário desta base.
    ///
    /// # Errors
    /// Sem permissão; no servidor, as contas cuidam da própria.
    pub fn desligar_segundo_fator_de(&self, sessao: &Sessao, usuario: Id) -> Resultado<()> {
        match (self, sessao) {
            #[cfg(feature = "local")]
            (Self::Local(m), Sessao::Local(s)) => m.desligar_segundo_fator_de(s, usuario),
            #[cfg(not(target_arch = "wasm32"))]
            (Self::Remoto(_), _) => {
                let _ = usuario;
                Err(so_no_local(
                    "Desligar a verificação em duas etapas de outra pessoa",
                ))
            }
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
