//! Bloqueio progressivo por tentativas de login malsucedidas.
//!
//! Ver `docs/08-seguranca-permissoes.md` §2.1: "5 tentativas → 1 min, 10 → 15 min, com
//! desbloqueio pelo admin." Puro domínio — quem chama decide o instante (relógio real ou
//! controlado em teste) e é responsável por persistir o estado entre chamadas.

use cardeal_kernel::Instante;

/// O estado de tentativas de acesso de uma credencial (usuário no sistema, ou operador
/// de PIN num terminal — o mesmo mecanismo serve aos dois, ver
/// `docs/08-seguranca-permissoes.md` §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Bloqueio {
    /// Tentativas malsucedidas consecutivas desde o último sucesso ou desbloqueio.
    pub tentativas: u32,
    /// Se bloqueado, até quando.
    pub bloqueado_ate: Option<Instante>,
}

impl Bloqueio {
    /// O estado inicial: sem tentativas, sem bloqueio.
    pub const NOVO: Self = Self {
        tentativas: 0,
        bloqueado_ate: None,
    };

    /// Verdadeiro se o acesso está bloqueado no instante informado.
    #[must_use]
    pub fn esta_bloqueado(&self, agora: Instante) -> bool {
        self.bloqueado_ate.is_some_and(|ate| agora < ate)
    }

    /// Quantos segundos faltam para o desbloqueio, se houver bloqueio ativo.
    #[must_use]
    pub fn segundos_restantes(&self, agora: Instante) -> Option<i64> {
        self.bloqueado_ate
            .filter(|ate| agora < *ate)
            .map(|ate| agora.micros_ate(ate) / 1_000_000)
    }

    /// Registra uma tentativa malsucedida e aplica o bloqueio progressivo, se atingiu o
    /// limiar. Chamar isto enquanto já bloqueado é seguro — apenas estende o bloqueio de
    /// acordo com a nova contagem.
    pub fn registrar_falha(&mut self, agora: Instante) {
        self.tentativas += 1;
        self.bloqueado_ate = Self::duracao_para(self.tentativas).map(|s| agora.mais_segundos(s));
    }

    /// Registra um acesso bem-sucedido: zera a contagem e qualquer bloqueio.
    pub fn registrar_sucesso(&mut self) {
        *self = Self::NOVO;
    }

    /// Um administrador libera o acesso manualmente, antes do prazo natural
    /// (`docs/08-seguranca-permissoes.md` §2.2: "3 erros = bloqueio até liberação do
    /// gerente" no caso do PIN de operador).
    pub fn desbloquear_pelo_administrador(&mut self) {
        *self = Self::NOVO;
    }

    /// A duração do bloqueio, em segundos, para um dado número de tentativas — `None`
    /// abaixo do primeiro limiar.
    const fn duracao_para(tentativas: u32) -> Option<i64> {
        match tentativas {
            0..=4 => None,
            5..=9 => Some(60),
            _ => Some(15 * 60),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn t(segundos: i64) -> Instante {
        Instante::de_micros(segundos * 1_000_000)
    }

    #[test]
    fn nao_bloqueia_antes_do_quinto_erro() {
        let mut b = Bloqueio::NOVO;
        for _ in 0..4 {
            b.registrar_falha(t(0));
        }
        assert!(!b.esta_bloqueado(t(0)));
    }

    #[test]
    fn bloqueia_um_minuto_no_quinto_erro() {
        let mut b = Bloqueio::NOVO;
        for _ in 0..5 {
            b.registrar_falha(t(0));
        }
        assert!(b.esta_bloqueado(t(0)));
        assert!(b.esta_bloqueado(t(59)));
        assert!(!b.esta_bloqueado(t(61)));
    }

    #[test]
    fn bloqueia_quinze_minutos_no_decimo_erro() {
        let mut b = Bloqueio::NOVO;
        for _ in 0..10 {
            b.registrar_falha(t(0));
        }
        assert!(b.esta_bloqueado(t(14 * 60)));
        assert!(!b.esta_bloqueado(t(16 * 60)));
    }

    #[test]
    fn sucesso_zera_tudo() {
        let mut b = Bloqueio::NOVO;
        for _ in 0..5 {
            b.registrar_falha(t(0));
        }
        assert!(b.esta_bloqueado(t(0)));
        b.registrar_sucesso();
        assert!(!b.esta_bloqueado(t(0)));
        assert_eq!(b.tentativas, 0);
    }

    #[test]
    fn admin_desbloqueia_antes_do_prazo() {
        let mut b = Bloqueio::NOVO;
        for _ in 0..10 {
            b.registrar_falha(t(0));
        }
        assert!(b.esta_bloqueado(t(60)));
        b.desbloquear_pelo_administrador();
        assert!(!b.esta_bloqueado(t(60)));
    }

    #[test]
    fn segundos_restantes_conta_corretamente() {
        let mut b = Bloqueio::NOVO;
        for _ in 0..5 {
            b.registrar_falha(t(0));
        }
        assert_eq!(b.segundos_restantes(t(10)), Some(50));
        assert_eq!(b.segundos_restantes(t(61)), None);
    }
}
