//! Limites quantitativos de papel — a parte "quanto", não "se".
//!
//! Ver `docs/08-seguranca-permissoes.md` §3.4: alguns poderes são quantitativos
//! (desconto máximo, teto de pagamento, cancelamentos por turno). Exceder o limite **não é
//! erro** — dispara autorização de supervisor na hora, registrada vinculando quem pediu a
//! quem autorizou. Este tipo só carrega o teto; a decisão de barrar ou escalar é do comando.

use cardeal_kernel::{Dinheiro, Percentual};

/// O teto de um limite quantitativo, na dimensão que o comando entende.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValorLimite {
    /// Teto em dinheiro — ex.: `financeiro.pagar.teto`.
    Dinheiro(Dinheiro),
    /// Teto percentual — ex.: `vendas.desconto_maximo`.
    Percentual(Percentual),
    /// Teto de contagem, tipicamente por turno — ex.: `pdv.cancelamento_por_turno`.
    Contagem(u32),
    /// Teto em dias — ex.: `vendas.prazo_maximo`.
    Dias(u32),
    /// Sem teto nessa dimensão: o papel pode qualquer valor.
    Ilimitado,
}

impl ValorLimite {
    /// Verdadeiro se `valor` cabe neste teto. Um limite de outra dimensão nunca restringe
    /// dinheiro, então devolve `true` — quem consulta é responsável por usar a dimensão
    /// certa.
    #[must_use]
    pub fn comporta_dinheiro(self, valor: Dinheiro) -> bool {
        match self {
            Self::Dinheiro(teto) => valor <= teto,
            _ => true,
        }
    }

    /// Verdadeiro se `valor` cabe neste teto percentual.
    #[must_use]
    pub fn comporta_percentual(self, valor: Percentual) -> bool {
        match self {
            Self::Percentual(teto) => valor <= teto,
            _ => true,
        }
    }

    /// Verdadeiro se `valor` cabe neste teto de contagem.
    #[must_use]
    pub fn comporta_contagem(self, valor: u32) -> bool {
        match self {
            Self::Contagem(teto) => valor <= teto,
            _ => true,
        }
    }

    /// Verdadeiro se `valor` cabe neste teto de dias.
    #[must_use]
    pub fn comporta_dias(self, valor: u32) -> bool {
        match self {
            Self::Dias(teto) => valor <= teto,
            _ => true,
        }
    }

    /// O mais permissivo entre dois limites — usado ao consolidar os vários papéis de um
    /// mesmo usuário: quem acumula papéis fica com o maior teto de cada dimensão.
    /// Limites de dimensões divergentes na mesma chave são configuração inconsistente;
    /// nesse caso mantém-se o primeiro.
    #[must_use]
    pub fn mais_permissivo(self, outro: Self) -> Self {
        match (self, outro) {
            (Self::Ilimitado, _) | (_, Self::Ilimitado) => Self::Ilimitado,
            (Self::Dinheiro(a), Self::Dinheiro(b)) => Self::Dinheiro(a.max(b)),
            (Self::Percentual(a), Self::Percentual(b)) => Self::Percentual(a.max(b)),
            (Self::Contagem(a), Self::Contagem(b)) => Self::Contagem(a.max(b)),
            (Self::Dias(a), Self::Dias(b)) => Self::Dias(a.max(b)),
            (a, _) => a,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn teto_de_dinheiro_barra_acima_e_aceita_no_limite() {
        let teto = ValorLimite::Dinheiro(Dinheiro::reais(5_000));
        assert!(teto.comporta_dinheiro(Dinheiro::reais(5_000)));
        assert!(teto.comporta_dinheiro(Dinheiro::reais(4_999)));
        assert!(!teto.comporta_dinheiro(Dinheiro::reais(5_001)));
    }

    #[test]
    fn dimensao_errada_nao_restringe() {
        let teto = ValorLimite::Contagem(3);
        assert!(teto.comporta_dinheiro(Dinheiro::reais(1_000_000)));
    }

    #[test]
    fn ilimitado_comporta_qualquer_coisa() {
        let l = ValorLimite::Ilimitado;
        assert!(l.comporta_dinheiro(Dinheiro::reais(999_999)));
        assert!(l.comporta_contagem(9_999));
        assert!(l.comporta_dias(3_650));
    }

    #[test]
    fn mais_permissivo_pega_o_maior_teto() {
        let a = ValorLimite::Percentual(Percentual::pontos(5));
        let b = ValorLimite::Percentual(Percentual::pontos(20));
        assert_eq!(a.mais_permissivo(b), b);
        assert_eq!(b.mais_permissivo(a), b);
    }

    #[test]
    fn mais_permissivo_com_ilimitado_vence() {
        let a = ValorLimite::Dinheiro(Dinheiro::reais(5_000));
        assert_eq!(
            a.mais_permissivo(ValorLimite::Ilimitado),
            ValorLimite::Ilimitado
        );
        assert_eq!(
            ValorLimite::Ilimitado.mais_permissivo(a),
            ValorLimite::Ilimitado
        );
    }
}
