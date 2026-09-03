//! Usuário — uma pessoa que acessa o sistema.
//!
//! Ver `docs/08-seguranca-permissoes.md` §2. Domínio puro: as transições de autenticação,
//! troca de senha e bloqueio, sem banco. Quem chama fornece o instante (relógio real ou
//! controlado) e persiste o estado entre chamadas — mesmo contrato de [`Bloqueio`].
//!
//! [`Bloqueio`]: crate::Bloqueio

use cardeal_kernel::{Id, Instante, Versao};

use crate::bloqueio::Bloqueio;
use crate::erros::ErroAuth;
use crate::senha::{hash_senha, verificar_senha, HashDeSenha, PoliticaSenha};

/// Uma pessoa com acesso ao sistema. Corresponde a `nucleo_usuario` (`docs/06 §2`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Usuario {
    /// Identidade.
    pub id: Id,
    /// Login único, minúsculo, sem espaço.
    pub login: String,
    /// Nome exibido.
    pub nome: String,
    /// E-mail, opcional.
    pub email: Option<String>,
    /// Hash Argon2id da senha (nunca a senha).
    pub hash_senha: HashDeSenha,
    /// Quando a senha foi trocada pela última vez.
    pub senha_trocada_em: Instante,
    /// Verdadeiro enquanto o usuário não trocar a senha definida por um admin.
    pub exige_troca_senha: bool,
    /// TOTP configurado (`docs/08 §2.1` — opcional).
    pub mfa_habilitado: bool,
    /// Conta ativa. Uma conta inativa nunca autentica.
    pub ativo: bool,
    /// Estado de tentativas e bloqueio progressivo.
    pub bloqueio: Bloqueio,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Usuario {
    /// Cria um usuário com uma senha inicial já validada contra a política. A senha fica
    /// marcada para troca no primeiro acesso.
    ///
    /// # Errors
    /// [`ErroAuth::SenhaCurta`]/[`ErroAuth::SenhaComum`] se a senha viola a política;
    /// [`ErroAuth::FalhaDeHash`] em erro interno do Argon2.
    pub fn novo(
        login: impl Into<String>,
        nome: impl Into<String>,
        senha_inicial: &str,
        politica: PoliticaSenha,
        agora: Instante,
    ) -> Result<Self, ErroAuth> {
        politica.validar(senha_inicial)?;
        Ok(Self {
            id: Id::novo(),
            login: normalizar_login(&login.into()),
            nome: nome.into(),
            email: None,
            hash_senha: hash_senha(senha_inicial)?,
            senha_trocada_em: agora,
            exige_troca_senha: true,
            mfa_habilitado: false,
            ativo: true,
            bloqueio: Bloqueio::NOVO,
            versao: Versao::INICIAL,
        })
    }

    /// Tenta autenticar com uma senha. Em sucesso, zera o bloqueio; em falha, registra a
    /// tentativa e aplica o bloqueio progressivo.
    ///
    /// A mensagem de erro é deliberadamente genérica ([`ErroAuth::CredencialInvalida`])
    /// para conta inexistente, inativa ou senha errada — não ajuda a enumerar usuários
    /// (`docs/08 §2.1`). A exceção é o bloqueio ativo, que é informado com o horário de
    /// liberação porque o usuário legítimo precisa saber.
    ///
    /// # Errors
    /// [`ErroAuth::ContaBloqueada`] se há bloqueio ativo (ou se esta tentativa o disparou);
    /// [`ErroAuth::CredencialInvalida`] caso contrário.
    pub fn autenticar(&mut self, senha: &str, agora: Instante) -> Result<(), ErroAuth> {
        if let Some(ate) = self.bloqueio.bloqueado_ate.filter(|ate| agora < *ate) {
            return Err(ErroAuth::ContaBloqueada { ate });
        }

        if self.ativo && verificar_senha(senha, &self.hash_senha) {
            self.bloqueio.registrar_sucesso();
            return Ok(());
        }

        self.bloqueio.registrar_falha(agora);
        match self.bloqueio.bloqueado_ate.filter(|ate| agora < *ate) {
            Some(ate) => Err(ErroAuth::ContaBloqueada { ate }),
            None => Err(ErroAuth::CredencialInvalida),
        }
    }

    /// O próprio usuário troca a senha, provando a atual.
    ///
    /// # Errors
    /// [`ErroAuth::SenhaAtualIncorreta`] se a senha atual não confere;
    /// [`ErroAuth::SenhaCurta`]/[`ErroAuth::SenhaComum`] se a nova viola a política;
    /// [`ErroAuth::FalhaDeHash`] em erro interno do Argon2.
    pub fn trocar_senha(
        &mut self,
        atual: &str,
        nova: &str,
        politica: PoliticaSenha,
        agora: Instante,
    ) -> Result<(), ErroAuth> {
        if !verificar_senha(atual, &self.hash_senha) {
            return Err(ErroAuth::SenhaAtualIncorreta);
        }
        politica.validar(nova)?;
        self.hash_senha = hash_senha(nova)?;
        self.senha_trocada_em = agora;
        self.exige_troca_senha = false;
        self.bloqueio.registrar_sucesso();
        Ok(())
    }

    /// Um admin redefine a senha do usuário (esquecimento, primeiro acesso). A nova senha
    /// fica marcada para troca no próximo login e o bloqueio é limpo.
    ///
    /// # Errors
    /// [`ErroAuth::SenhaCurta`]/[`ErroAuth::SenhaComum`] se a nova viola a política;
    /// [`ErroAuth::FalhaDeHash`] em erro interno do Argon2.
    pub fn redefinir_senha(
        &mut self,
        nova: &str,
        politica: PoliticaSenha,
        agora: Instante,
    ) -> Result<(), ErroAuth> {
        politica.validar(nova)?;
        self.hash_senha = hash_senha(nova)?;
        self.senha_trocada_em = agora;
        self.exige_troca_senha = true;
        self.bloqueio.desbloquear_pelo_administrador();
        Ok(())
    }

    /// Um admin libera o acesso antes do prazo natural do bloqueio (`docs/08 §2.2`).
    pub fn desbloquear(&mut self) {
        self.bloqueio.desbloquear_pelo_administrador();
    }

    /// Desativa a conta (funcionário desligado). Reversível por [`reativar`](Self::reativar),
    /// mas nunca apagada — a auditoria e o razão referenciam o id.
    pub fn desativar(&mut self) {
        self.ativo = false;
    }

    /// Reativa uma conta desativada.
    pub fn reativar(&mut self) {
        self.ativo = true;
    }
}

/// Normaliza um login: minúsculas e sem espaço nas pontas.
fn normalizar_login(bruto: &str) -> String {
    bruto.trim().to_lowercase()
}

#[cfg(test)]
mod testes {
    use super::*;

    const SENHA_BOA: &str = "Corr3io-Azul-Serra!";
    const OUTRA_BOA: &str = "Trem-Bao-de-Minas-99";

    fn t(segundos: i64) -> Instante {
        Instante::EPOCA.mais_segundos(segundos)
    }

    fn usuario() -> Usuario {
        Usuario::novo(
            "Joao",
            "João da Silva",
            SENHA_BOA,
            PoliticaSenha::PADRAO,
            t(0),
        )
        .unwrap()
    }

    #[test]
    fn novo_usuario_normaliza_login_e_exige_troca() {
        let u = usuario();
        assert_eq!(u.login, "joao");
        assert!(u.exige_troca_senha);
        assert!(u.ativo);
    }

    #[test]
    fn novo_usuario_recusa_senha_fraca() {
        let erro = Usuario::novo("x", "X", "123456", PoliticaSenha::PADRAO, t(0)).unwrap_err();
        assert!(matches!(
            erro,
            ErroAuth::SenhaCurta { .. } | ErroAuth::SenhaComum
        ));
    }

    #[test]
    fn autentica_com_a_senha_certa() {
        let mut u = usuario();
        assert!(u.autenticar(SENHA_BOA, t(1)).is_ok());
        assert_eq!(u.bloqueio.tentativas, 0);
    }

    #[test]
    fn senha_errada_e_credencial_invalida_generica() {
        let mut u = usuario();
        let erro = u.autenticar("errada demais", t(1)).unwrap_err();
        assert!(matches!(erro, ErroAuth::CredencialInvalida));
        assert_eq!(u.bloqueio.tentativas, 1);
    }

    #[test]
    fn conta_inativa_nao_vaza_que_existe() {
        let mut u = usuario();
        u.desativar();
        let erro = u.autenticar(SENHA_BOA, t(1)).unwrap_err();
        assert!(matches!(erro, ErroAuth::CredencialInvalida));
    }

    #[test]
    fn cinco_erros_bloqueiam_e_a_sexta_tentativa_reporta_bloqueio() {
        let mut u = usuario();
        for _ in 0..4 {
            let _ = u.autenticar("errada", t(1));
        }
        let erro = u.autenticar("errada", t(1)).unwrap_err();
        assert!(matches!(erro, ErroAuth::ContaBloqueada { .. }));
        // Mesmo com a senha certa, segue bloqueada dentro do prazo.
        let erro = u.autenticar(SENHA_BOA, t(2)).unwrap_err();
        assert!(matches!(erro, ErroAuth::ContaBloqueada { .. }));
    }

    #[test]
    fn admin_desbloqueia_e_o_acesso_volta() {
        let mut u = usuario();
        for _ in 0..5 {
            let _ = u.autenticar("errada", t(1));
        }
        u.desbloquear();
        assert!(u.autenticar(SENHA_BOA, t(2)).is_ok());
    }

    #[test]
    fn trocar_senha_exige_a_atual_correta() {
        let mut u = usuario();
        assert!(matches!(
            u.trocar_senha("nao e a atual", OUTRA_BOA, PoliticaSenha::PADRAO, t(1)),
            Err(ErroAuth::SenhaAtualIncorreta)
        ));
        u.trocar_senha(SENHA_BOA, OUTRA_BOA, PoliticaSenha::PADRAO, t(1))
            .unwrap();
        assert!(!u.exige_troca_senha);
        assert!(u.autenticar(OUTRA_BOA, t(2)).is_ok());
        assert!(u.autenticar(SENHA_BOA, t(3)).is_err());
    }

    #[test]
    fn trocar_senha_recusa_nova_fraca() {
        let mut u = usuario();
        assert!(matches!(
            u.trocar_senha(SENHA_BOA, "password", PoliticaSenha::PADRAO, t(1)),
            Err(ErroAuth::SenhaComum)
        ));
    }

    #[test]
    fn admin_redefine_senha_e_volta_a_exigir_troca() {
        let mut u = usuario();
        u.trocar_senha(SENHA_BOA, OUTRA_BOA, PoliticaSenha::PADRAO, t(1))
            .unwrap();
        assert!(!u.exige_troca_senha);
        u.redefinir_senha(SENHA_BOA, PoliticaSenha::PADRAO, t(2))
            .unwrap();
        assert!(u.exige_troca_senha);
        assert!(u.autenticar(SENHA_BOA, t(3)).is_ok());
    }
}
