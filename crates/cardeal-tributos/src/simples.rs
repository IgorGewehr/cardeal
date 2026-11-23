//! As tabelas do Simples Nacional (LC 123/2006, Anexos I a V, redação da LC 155/2016) e a
//! repartição dos tributos dos Anexos I e II. **Conferir com o contador** a cada mudança legal.

use cardeal_kernel::{Dinheiro, Percentual};
use serde::{Deserialize, Serialize};

/// O sublimite estadual (LC 123 art. 13-A): acima dele, ICMS e ISS saem do DAS.
pub const SUBLIMITE: Dinheiro = Dinheiro::reais(3_600_000);

/// Os anexos do Simples Nacional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AnexoSimples {
    /// Comércio.
    I,
    /// Indústria.
    II,
    /// Serviços (instalação, reparos, manutenção…).
    III,
    /// Serviços (construção, vigilância, limpeza…).
    IV,
    /// Serviços intelectuais.
    V,
}

impl AnexoSimples {
    /// O nome curto ("Anexo I").
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::I => "Anexo I",
            Self::II => "Anexo II",
            Self::III => "Anexo III",
            Self::IV => "Anexo IV",
            Self::V => "Anexo V",
        }
    }
}

/// (teto da faixa em reais, alíquota nominal em centésimos de ponto, parcela a deduzir em reais)
type Faixa = (i64, i64, i64);

const TETOS: [i64; 6] = [180_000, 360_000, 720_000, 1_800_000, 3_600_000, 4_800_000];

const fn tabela(a: AnexoSimples) -> [Faixa; 6] {
    // alíquota em centésimos de ponto percentual: 4,00% = 400.
    match a {
        AnexoSimples::I => [
            (TETOS[0], 400, 0),
            (TETOS[1], 730, 5_940),
            (TETOS[2], 950, 13_860),
            (TETOS[3], 1_070, 22_500),
            (TETOS[4], 1_430, 87_300),
            (TETOS[5], 1_900, 378_000),
        ],
        AnexoSimples::II => [
            (TETOS[0], 450, 0),
            (TETOS[1], 780, 5_940),
            (TETOS[2], 1_000, 13_860),
            (TETOS[3], 1_120, 22_500),
            (TETOS[4], 1_470, 85_500),
            (TETOS[5], 3_000, 720_000),
        ],
        AnexoSimples::III => [
            (TETOS[0], 600, 0),
            (TETOS[1], 1_120, 9_360),
            (TETOS[2], 1_350, 17_640),
            (TETOS[3], 1_600, 35_640),
            (TETOS[4], 2_100, 125_640),
            (TETOS[5], 3_300, 648_000),
        ],
        AnexoSimples::IV => [
            (TETOS[0], 450, 0),
            (TETOS[1], 900, 8_100),
            (TETOS[2], 1_020, 12_420),
            (TETOS[3], 1_400, 39_780),
            (TETOS[4], 2_200, 183_780),
            (TETOS[5], 3_300, 828_000),
        ],
        AnexoSimples::V => [
            (TETOS[0], 1_550, 0),
            (TETOS[1], 1_800, 4_500),
            (TETOS[2], 1_950, 9_900),
            (TETOS[3], 2_050, 17_100),
            (TETOS[4], 2_300, 62_100),
            (TETOS[5], 3_050, 540_000),
        ],
    }
}

/// O índice (0..=5) da faixa para a receita dos 12 meses; acima do teto de R$ 4,8 mi
/// continua na última (o negócio já passou do limite do Simples — o sistema avisa à parte).
/// O índice (0..=5) da faixa para a receita dos 12 meses.
#[must_use]
pub fn indice_da_faixa(a: AnexoSimples, rbt12: Dinheiro) -> usize {
    let reais = rbt12.em_centavos() / 100;
    tabela(a)
        .iter()
        .position(|(teto, _, _)| reais <= *teto)
        .unwrap_or(5)
}

/// A faixa do Simples (índice 0 a 5) para o faturamento dos 12 meses, e o teto dela.
#[must_use]
pub fn faixa_do_simples(anexo: AnexoSimples, rbt12: Dinheiro) -> (usize, Dinheiro) {
    let i = indice_da_faixa(anexo, rbt12);
    (i, Dinheiro::reais(tabela(anexo)[i].0))
}

/// A alíquota efetiva do Simples para o faturamento dos últimos 12 meses.
///
/// Sem histórico (`rbt12` zero, empresa nova) vale a alíquota nominal da primeira faixa.
#[must_use]
pub fn aliquota_efetiva(anexo: AnexoSimples, rbt12: Dinheiro) -> Percentual {
    let (_, nominal, deduzir) = tabela(anexo)[indice_da_faixa(anexo, rbt12)];
    let nominal_un = nominal * 10_000; // centésimos de ponto → 1e-6 ponto
    if rbt12.em_centavos() <= 0 || deduzir == 0 {
        return Percentual::unidades(nominal_un);
    }
    // efetiva = nominal − deduzir ÷ rbt12  (em pontos percentuais)
    let desconto = i128::from(deduzir) * 100 * 100 * 1_000_000 / i128::from(rbt12.em_centavos());
    Percentual::unidades(nominal_un - i64::try_from(desconto).unwrap_or(0))
}

/// A repartição da alíquota efetiva (centésimos de ponto sobre 100%: 34,00% = 3 400) que é
/// ICMS e que é PIS+COFINS, por anexo e faixa (LC 123, Anexos I e II, "Percentual de
/// Repartição dos Tributos"). Nos Anexos III a V (serviço) não há dedução carregada.
#[must_use]
pub const fn reparticao(anexo: AnexoSimples, indice: usize) -> Option<(i64, i64)> {
    match (anexo, indice) {
        // Anexo I: COFINS 12,74 + PIS 2,76 = 15,50; ICMS 34 / 33,5; na 6ª, sem ICMS.
        (AnexoSimples::I, 0 | 1) => Some((3_400, 1_550)),
        (AnexoSimples::I, 2..=4) => Some((3_350, 1_550)),
        (AnexoSimples::I, _) => Some((0, 3_440)),
        // Anexo II: COFINS 11,51 + PIS 2,49 = 14,00; ICMS 32; na 6ª, COFINS 20,96 + PIS 4,54.
        (AnexoSimples::II, 0..=4) => Some((3_200, 1_400)),
        (AnexoSimples::II, _) => Some((0, 2_550)),
        _ => None,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn efetiva_nunca_passa_da_nominal() {
        for a in [
            AnexoSimples::I,
            AnexoSimples::II,
            AnexoSimples::III,
            AnexoSimples::IV,
            AnexoSimples::V,
        ] {
            for reais in [50_000, 200_000, 500_000, 1_000_000, 2_500_000, 3_500_000] {
                let e = aliquota_efetiva(a, Dinheiro::reais(reais));
                let (_, nominal, _) = tabela(a)[indice_da_faixa(a, Dinheiro::reais(reais))];
                assert!(
                    e.unidades_internas() <= nominal * 10_000,
                    "{a:?} em {reais}"
                );
            }
        }
    }

    #[test]
    fn reparticao_soma_menos_de_100_por_cento() {
        for a in [AnexoSimples::I, AnexoSimples::II] {
            for i in 0..6 {
                let (icms, pc) = reparticao(a, i).unwrap();
                assert!(icms + pc < 10_000);
            }
        }
        assert!(reparticao(AnexoSimples::III, 0).is_none());
    }
}
