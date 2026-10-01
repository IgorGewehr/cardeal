//! Provisionar uma empresa: criar a base dela, o administrador, e dar acesso a uma conta.
//!
//! Hoje é operação de bastidor (CLI `cardeal-server provisionar`). O autoatendimento ("crie sua
//! conta") vem depois, com verificação de e-mail — reaproveitando esta mesma função.

use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use std::sync::Arc;

use cardeal_motor::{MotorLocal, Plano};
use cardeal_storage::ConfigArmazenamento;

use crate::autenticacao::hash_de_senha_nova;
use crate::config::ConfigServidor;
use crate::diretorio::Diretorio;
use crate::token;

/// Os dados de uma empresa nova e de quem a administra.
#[derive(Debug, Clone)]
pub struct NovaEmpresa {
    /// Razão social.
    pub razao_social: String,
    /// CNPJ.
    pub cnpj: String,
    /// E-mail da conta administradora. Se a conta já existe (um contador com várias
    /// empresas), ganha acesso a esta também e a senha dela não muda.
    pub email: String,
    /// Nome da pessoa.
    pub nome: String,
    /// Senha da conta — só usada se a conta ainda não existe.
    pub senha: String,
}

/// O resultado de um provisionamento.
#[derive(Debug, Clone, Copy)]
pub struct Provisionada {
    /// A empresa criada.
    pub empresa: Id,
    /// A conta administradora.
    pub conta: Id,
}

/// Cria a empresa. **Bloqueante.**
///
/// # Errors
/// Senha fora da política, falha de disco/diretório, erro de migração da base nova.
pub fn provisionar(
    config: &ConfigServidor,
    plano: &Arc<Plano>,
    diretorio: &Diretorio,
    nova: &NovaEmpresa,
) -> Resultado<Provisionada> {
    let conta = match diretorio.conta_por_email(&nova.email)? {
        Some(c) => c.id,
        None => {
            diretorio.criar_conta(&nova.email, &nova.nome, &hash_de_senha_nova(&nova.senha)?)?
        }
    };

    let empresa = Id::novo();
    std::fs::create_dir_all(config.pasta_empresas())
        .map_err(|e| Erro::novo(CodigoErro::FALHA_DE_DISCO, e.to_string()))?;
    let usuario = {
        let mut motor = MotorLocal::abrir_com_plano(
            ConfigArmazenamento::servidor(config.caminho_empresa(empresa)),
            plano,
        )?;
        motor.definir_empresa_nova(empresa)?;
        motor.configurar_inicial(
            &nova.razao_social,
            &nova.cnpj,
            &login_na_empresa(&nova.email),
            &nova.nome,
            &token::senha_inutilizavel(),
        )?
        // O motor é fechado aqui: a frota reabre sob demanda.
    };

    diretorio.registrar_empresa(empresa, &nova.razao_social)?;
    diretorio.vincular(conta, empresa, usuario)?;
    tracing::info!(%empresa, %conta, "empresa provisionada");
    Ok(Provisionada { empresa, conta })
}

/// O login do usuário dentro da base da empresa: o próprio e-mail normalizado — legível na
/// auditoria e único por construção.
fn login_na_empresa(email: &str) -> String {
    crate::diretorio::normalizar_email(email)
}
