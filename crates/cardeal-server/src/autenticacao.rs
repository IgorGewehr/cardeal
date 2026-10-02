//! Login e logout de conta — independente de HTTP, para ser testado direto.
//!
//! - **Anti-enumeração.** E-mail inexistente, senha errada e conta sem empresa respondem o
//!   mesmo `CREDENCIAL_INVALIDA`, e um e-mail inexistente também paga um Argon2id (contra um
//!   hash fictício): o tempo de resposta não revela se a conta existe.
//! - **Bloqueio progressivo** por conta (`cardeal_auth::Bloqueio`, 5→1 min, 10→15 min).
//! - **RAM limitada.** Quem chama segura uma licença do semáforo de logins durante o hash.

use std::sync::OnceLock;
use std::time::Duration;

use cardeal_auth::{hash_senha, verificar_senha, HashDeSenha, PoliticaSenha};
use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
use cardeal_protocol::{EmpresaAcessivel, RespostaLogin};

use crate::diretorio::{Diretorio, SessaoPersistida};
use crate::token;

fn credencial_invalida() -> Erro {
    Erro::novo(CodigoErro::CREDENCIAL_INVALIDA, "e-mail ou senha inválidos")
}

fn hash_ficticio() -> &'static HashDeSenha {
    static HASH: OnceLock<HashDeSenha> = OnceLock::new();
    HASH.get_or_init(|| {
        hash_senha("cardeal-hash-ficticio-para-tempo-constante")
            .expect("Argon2id com parâmetros constantes não falha")
    })
}

/// Um login aceito: o token (a ser entregue por cookie ou corpo) e a resposta.
pub struct LoginAceito {
    /// O token em texto — existe só aqui e no cliente.
    pub token: String,
    /// A resposta, ainda sem o token no corpo.
    pub resposta: RespostaLogin,
}

/// Confere e-mail e senha e abre uma sessão. **Bloqueante** (Argon2id + diretório).
///
/// # Errors
/// `CREDENCIAL_INVALIDA` (genérico) ou `CONTA_BLOQUEADA` (com o horário de liberação — o
/// usuário legítimo precisa saber); falha do diretório.
pub fn entrar(
    diretorio: &Diretorio,
    email: &str,
    senha: &str,
    validade: Duration,
) -> Resultado<LoginAceito> {
    let agora = Instante::agora();
    let Some(mut conta) = diretorio.conta_por_email(email)? else {
        let _ = verificar_senha(senha, hash_ficticio());
        return Err(credencial_invalida());
    };
    if let Some(ate) = conta.bloqueio.bloqueado_ate.filter(|ate| agora < *ate) {
        return Err(Erro::de_dominio(&cardeal_auth::ErroAuth::ContaBloqueada {
            ate,
        }));
    }

    if !verificar_senha(senha, &conta.senha) {
        conta.bloqueio.registrar_falha(agora);
        diretorio.gravar_bloqueio(conta.id, conta.bloqueio)?;
        return match conta.bloqueio.bloqueado_ate.filter(|ate| agora < *ate) {
            Some(ate) => Err(Erro::de_dominio(&cardeal_auth::ErroAuth::ContaBloqueada {
                ate,
            })),
            None => Err(credencial_invalida()),
        };
    }
    if conta.bloqueio.tentativas > 0 {
        conta.bloqueio.registrar_sucesso();
        diretorio.gravar_bloqueio(conta.id, conta.bloqueio)?;
    }

    let empresas: Vec<EmpresaAcessivel> = diretorio
        .vinculos(conta.id)?
        .into_iter()
        .map(|v| EmpresaAcessivel {
            id: v.empresa,
            nome: v.nome,
        })
        .collect();
    if empresas.is_empty() {
        return Err(credencial_invalida());
    }

    let (texto, hash) = token::gerar();
    let segundos = i64::try_from(validade.as_secs()).unwrap_or(i64::MAX);
    diretorio.criar_sessao(
        &hash,
        SessaoPersistida {
            id: Id::novo(),
            conta: conta.id,
            expira_em: agora.mais_segundos(segundos),
        },
    )?;
    Ok(LoginAceito {
        token: texto,
        resposta: RespostaLogin {
            nome: conta.nome,
            empresas,
            token: None,
        },
    })
}

/// Encerra a sessão deste token (idempotente: token desconhecido não é erro).
///
/// # Errors
/// Falha do diretório.
pub fn sair(diretorio: &Diretorio, token: &str) -> Resultado<()> {
    diretorio.encerrar_sessao(&token::hash(token))
}

/// Valida uma senha nova de conta pela política padrão e devolve o hash.
///
/// # Errors
/// `ENTRADA_INVALIDA` se a senha viola a política.
pub fn hash_de_senha_nova(senha: &str) -> Resultado<HashDeSenha> {
    PoliticaSenha::PADRAO
        .validar(senha)
        .and_then(|()| hash_senha(senha))
        .map_err(|e| Erro::de_dominio(&e).no_campo("senha"))
}

/// Troca a senha da conta dona do token: confere a atual, valida a nova, grava, e encerra as
/// outras sessões da conta (a deste token continua). Devolve os hashes encerrados.
/// **Bloqueante** (Argon2id ×2).
///
/// # Errors
/// `SESSAO_INVALIDA`; `CREDENCIAL_INVALIDA` se a senha atual não confere; a política da nova.
pub(crate) fn trocar_senha(
    diretorio: &Diretorio,
    conta: Id,
    token: &str,
    atual: &str,
    nova: &str,
) -> Resultado<Vec<token::HashToken>> {
    let Some(registro) = diretorio.conta_por_id(conta)? else {
        return Err(Erro::novo(
            CodigoErro::SESSAO_INVALIDA,
            "conta não encontrada",
        ));
    };
    if !verificar_senha(atual, &registro.senha) {
        return Err(
            Erro::novo(CodigoErro::CREDENCIAL_INVALIDA, "a senha atual não confere")
                .no_campo("atual"),
        );
    }
    if atual == nova {
        return Err(
            Erro::novo(CodigoErro::ENTRADA_INVALIDA, "a senha nova é igual à atual")
                .no_campo("nova"),
        );
    }
    let hash = hash_de_senha_nova(nova).map_err(|e| e.no_campo("nova"))?;
    diretorio.gravar_senha(conta, &hash)?;
    diretorio.encerrar_outras_sessoes(conta, &token::hash(token))
}
