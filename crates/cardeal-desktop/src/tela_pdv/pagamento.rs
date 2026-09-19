//! A conta do pagamento do PDV — lógica pura, sem UI.
//!
//! Regras (`docs/modulos/pdv.md` §11): a soma das formas tem que fechar **exatamente** com o
//! total antes de `F2` aceitar; e "se o valor em dinheiro exceder o total, o troco é
//! calculado e mostrado automaticamente — nunca exige conta de cabeça do operador nem do
//! cliente". Só o **dinheiro** dá troco: cartão e Pix pagam o valor exato.

use cardeal_kernel::Dinheiro;
use mod_pdv::{FormaPagamentoPdv, PagamentoInformado};

/// As formas de pagamento, na ordem das teclas `1..=4`.
pub const FORMAS: [(FormaPagamentoPdv, &str); 4] = [
    (FormaPagamentoPdv::Dinheiro, "Dinheiro"),
    (FormaPagamentoPdv::Pix, "Pix"),
    (FormaPagamentoPdv::Debito, "Cartão de débito"),
    (FormaPagamentoPdv::Credito, "Cartão de crédito"),
];

/// Índice do dinheiro em [`FORMAS`] — a única forma que dá troco.
const DINHEIRO: usize = 0;

/// Quanto foi informado em cada forma, na ordem de [`FORMAS`]. `ZERO` = não usada.
pub type Valores = [Dinheiro; 4];

/// Onde o pagamento está em relação ao total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Situacao {
    /// Ainda falta receber.
    Falta(Dinheiro),
    /// Fecha exato.
    Fecha,
    /// Passou do total e o excesso é dinheiro: devolve-se de troco.
    Troco(Dinheiro),
    /// Passou do total, mas o excesso não é dinheiro — não há como devolver.
    Excesso(Dinheiro),
}

impl Situacao {
    /// Se o `F2` pode finalizar a venda neste estado.
    #[must_use]
    pub const fn pode_finalizar(self) -> bool {
        matches!(self, Self::Fecha | Self::Troco(_))
    }
}

/// A soma de todas as formas.
#[must_use]
pub fn soma(valores: &Valores) -> Dinheiro {
    valores.iter().fold(Dinheiro::ZERO, |a, b| a + *b)
}

/// Compara o informado com o total.
#[must_use]
pub fn situacao(total: Dinheiro, valores: &Valores) -> Situacao {
    let pago = soma(valores);
    if pago < total {
        Situacao::Falta(total - pago)
    } else if pago == total {
        Situacao::Fecha
    } else {
        let excesso = pago - total;
        if valores[DINHEIRO] >= excesso {
            Situacao::Troco(excesso)
        } else {
            Situacao::Excesso(excesso)
        }
    }
}

/// Quanto falta receber **se** a forma `indice` fosse refeita do zero — o valor sugerido
/// quando o operador escolhe a forma e só aperta `Enter`.
#[must_use]
pub fn restante_para(total: Dinheiro, valores: &Valores, indice: usize) -> Dinheiro {
    let sem_esta = soma(valores) - valores[indice];
    let r = total - sem_esta;
    if r.e_negativo() {
        Dinheiro::ZERO
    } else {
        r
    }
}

/// Monta os pagamentos que o backend recebe: só as formas usadas, com o troco já
/// descontado do dinheiro (o backend exige soma exata).
#[must_use]
pub fn montar(total: Dinheiro, valores: &Valores) -> Vec<PagamentoInformado> {
    let troco = match situacao(total, valores) {
        Situacao::Troco(t) => t,
        _ => Dinheiro::ZERO,
    };
    FORMAS
        .iter()
        .zip(valores)
        .enumerate()
        .filter(|(_, (_, v))| **v > Dinheiro::ZERO)
        .map(|(i, ((forma, _), v))| PagamentoInformado {
            forma: *forma,
            valor: if i == DINHEIRO { *v - troco } else { *v },
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    fn r(n: i64) -> Dinheiro {
        Dinheiro::reais(n)
    }

    fn valores(dinheiro: i64, pix: i64, debito: i64, credito: i64) -> Valores {
        [r(dinheiro), r(pix), r(debito), r(credito)]
    }

    #[test]
    fn nada_informado_falta_o_total() {
        assert_eq!(
            situacao(r(30), &valores(0, 0, 0, 0)),
            Situacao::Falta(r(30))
        );
    }

    #[test]
    fn parcial_falta_a_diferenca() {
        assert_eq!(
            situacao(r(30), &valores(20, 0, 0, 0)),
            Situacao::Falta(r(10))
        );
    }

    #[test]
    fn exato_fecha_e_pode_finalizar() {
        let s = situacao(r(30), &valores(20, 10, 0, 0));
        assert_eq!(s, Situacao::Fecha);
        assert!(s.pode_finalizar());
    }

    #[test]
    fn dinheiro_a_mais_e_troco() {
        let s = situacao(r(30), &valores(50, 0, 0, 0));
        assert_eq!(s, Situacao::Troco(r(20)));
        assert!(s.pode_finalizar());
    }

    #[test]
    fn cartao_a_mais_nao_da_troco() {
        // 40 no débito para uma conta de 30: não há dinheiro para devolver.
        let s = situacao(r(30), &valores(0, 0, 40, 0));
        assert_eq!(s, Situacao::Excesso(r(10)));
        assert!(!s.pode_finalizar());
    }

    #[test]
    fn excesso_maior_que_o_dinheiro_recebido_nao_e_troco() {
        // 5 em dinheiro + 30 no cartão para 30: sobra 5, e o dinheiro (5) cobre o troco.
        assert_eq!(
            situacao(r(30), &valores(5, 0, 30, 0)),
            Situacao::Troco(r(5))
        );
        // 3 em dinheiro + 30 no cartão: sobra 3, cobre.
        assert_eq!(
            situacao(r(30), &valores(3, 0, 30, 0)),
            Situacao::Troco(r(3))
        );
        // 5 em dinheiro + 40 no cartão: sobra 15 > 5 em dinheiro → não há como devolver.
        assert_eq!(
            situacao(r(30), &valores(5, 0, 40, 0)),
            Situacao::Excesso(r(15))
        );
    }

    #[test]
    fn montar_desconta_o_troco_do_dinheiro_e_fecha_exato() {
        let total = r(30);
        let v = valores(50, 0, 0, 0);
        let p = montar(total, &v);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].valor, r(30)); // 50 recebidos − 20 de troco
        let soma: Dinheiro = p.iter().fold(Dinheiro::ZERO, |a, x| a + x.valor);
        assert_eq!(soma, total, "o backend exige soma exata");
    }

    #[test]
    fn montar_ignora_formas_zeradas_e_preserva_as_usadas() {
        let p = montar(r(30), &valores(20, 0, 10, 0));
        let formas: Vec<_> = p.iter().map(|x| x.forma).collect();
        assert_eq!(
            formas,
            vec![FormaPagamentoPdv::Dinheiro, FormaPagamentoPdv::Debito]
        );
        assert_eq!(p[0].valor, r(20));
        assert_eq!(p[1].valor, r(10));
    }

    #[test]
    fn restante_para_sugere_o_que_falta_sem_contar_a_propria_forma() {
        let v = valores(20, 0, 0, 0);
        // Escolher Pix: falta 10.
        assert_eq!(restante_para(r(30), &v, 1), r(10));
        // Refazer o Dinheiro: como se ele fosse zero, falta os 30 inteiros.
        assert_eq!(restante_para(r(30), &v, 0), r(30));
    }

    #[test]
    fn restante_para_nunca_e_negativo() {
        let v = valores(0, 0, 50, 0);
        assert_eq!(restante_para(r(30), &v, 1), Dinheiro::ZERO);
    }
}
