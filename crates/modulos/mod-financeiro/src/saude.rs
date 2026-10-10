//! O score de saúde financeira (0 a 100): cinco sinais pesados e somados, com o semáforo e o
//! que mais puxa o resultado para baixo. Determinístico e explicável — cada ponto vem de uma
//! conta que o usuário pode conferir. Domínio puro sobre as entradas já apuradas.
//!
//! | Sinal | Peso | 0 pontos | 100 pontos |
//! |---|---|---|---|
//! | Liquidez (caixa + a receber em 30 dias ÷ a pagar em 30 dias) | 25 | ≤ 0,5 | ≥ 1,5 |
//! | Inadimplência (vencido há +30 dias ÷ a receber em aberto) | 20 | ≥ 20% | 0% |
//! | Margem EBITDA dos últimos 3 meses | 20 | ≤ 0% | ≥ 15% |
//! | Giro de estoque | 20 | sem giro | girou 1× por mês ou mais |
//! | Fluxo projetado em 30 dias (caixa + receber − pagar) | 15 | negativo | cobre um mês de contas |
//!
//! Sinal sem dados (empresa sem faturamento, sem módulo de estoque) fica de fora e os pesos dos
//! demais são reescalados: o score nunca premia a ausência de informação.

use cardeal_kernel::{Dinheiro, Percentual};
use serde::{Deserialize, Serialize};

/// O que o cálculo precisa saber.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EntradasDeSaude {
    /// Caixa e bancos hoje.
    pub caixa: Dinheiro,
    /// A receber que vence em até 30 dias (inclui o já vencido).
    pub a_receber_30d: Dinheiro,
    /// A pagar que vence em até 30 dias (inclui o já vencido).
    pub a_pagar_30d: Dinheiro,
    /// Inadimplência: vencido há mais de 30 dias ÷ a receber em aberto; `None` sem recebíveis.
    pub inadimplencia: Option<Percentual>,
    /// Margem EBITDA dos últimos 3 meses; `None` sem receita.
    pub margem_ebitda: Option<Percentual>,
    /// Giro mensal do estoque (1× = 100%); `None` sem estoque ou sem o módulo.
    pub giro_mensal: Option<Percentual>,
}

/// O semáforo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Semaforo {
    /// 70 ou mais.
    Verde,
    /// De 40 a 69.
    Amarelo,
    /// Menos de 40.
    Vermelho,
}

/// Um sinal do score.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SinalDeSaude {
    /// O nome.
    pub nome: String,
    /// O peso original.
    pub peso: u8,
    /// Os pontos de 0 a 100; `None` = sem dados.
    pub pontos: Option<u8>,
    /// O valor medido, em texto ("1,2×", "8,5%").
    pub medida: String,
    /// Uma frase sobre o que fazer quando este sinal está fraco.
    pub dica: String,
}

/// O resultado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Saude {
    /// O score de 0 a 100.
    pub score: u8,
    /// O semáforo.
    pub semaforo: Semaforo,
    /// Os cinco sinais.
    pub sinais: Vec<SinalDeSaude>,
}

impl Saude {
    /// Os sinais com dados, do que mais pesa para baixo ao que menos pesa.
    #[must_use]
    pub fn puxando_para_baixo(&self) -> Vec<&SinalDeSaude> {
        let mut v: Vec<&SinalDeSaude> = self
            .sinais
            .iter()
            .filter(|s| s.pontos.is_some_and(|p| p < 70))
            .collect();
        v.sort_by_key(|s| {
            std::cmp::Reverse(u32::from(100 - s.pontos.unwrap_or(100)) * u32::from(s.peso))
        });
        v
    }
}

/// Interpolação linear: `x` em `[zero, cheio]` vira 0..=100 (e `zero > cheio` inverte).
fn escala(x: i64, zero: i64, cheio: i64) -> u8 {
    if zero == cheio {
        return 0;
    }
    let p = (i128::from(x - zero) * 100) / i128::from(cheio - zero);
    u8::try_from(p.clamp(0, 100)).unwrap_or(0)
}

#[allow(clippy::cast_possible_truncation)] // clamp garante a faixa
fn razao_centesimos(a: Dinheiro, b: Dinheiro) -> i64 {
    // a ÷ b em centésimos (1,50 = 150).
    (i128::from(a.em_centavos()) * 100 / i128::from(b.em_centavos().max(1)))
        .clamp(-1_000_000, 1_000_000) as i64
}

/// Calcula o score.
#[must_use]
#[allow(clippy::too_many_lines)] // cinco sinais declarados em sequência, cada um com texto próprio
pub fn calcular(e: &EntradasDeSaude) -> Saude {
    let pct = |p: Percentual| format!("{}%", p.formatar(1));

    let liquidez = if e.a_pagar_30d.e_positivo() {
        let r = razao_centesimos(e.caixa + e.a_receber_30d, e.a_pagar_30d);
        (
            Some(escala(r, 50, 150)),
            format!("{},{:02}×", r / 100, (r % 100).abs()),
        )
    } else {
        (Some(100), "sem contas a pagar em 30 dias".to_owned())
    };
    let inadimplencia = e
        .inadimplencia
        .map_or((None, "sem recebíveis".to_owned()), |p| {
            let milesimos = p.unidades_internas() / 1_000; // 1e-3 ponto
            (Some(escala(milesimos, 20_000, 0)), pct(p))
        });
    let margem = e
        .margem_ebitda
        .map_or((None, "sem receita".to_owned()), |p| {
            let milesimos = p.unidades_internas() / 1_000;
            (Some(escala(milesimos, 0, 15_000)), pct(p))
        });
    let giro = e
        .giro_mensal
        .map_or((None, "sem dados de estoque".to_owned()), |p| {
            let milesimos = p.unidades_internas() / 1_000;
            (
                Some(escala(milesimos, 0, 100_000)),
                format!("{} do estoque por mês", pct(p)),
            )
        });
    let projetado = e.caixa + e.a_receber_30d - e.a_pagar_30d;
    let fluxo = if projetado.e_negativo() {
        (
            Some(0),
            format!("faltam {}", (-projetado).formatar_com_simbolo()),
        )
    } else if e.a_pagar_30d.e_positivo() {
        (
            Some(escala(razao_centesimos(projetado, e.a_pagar_30d), 0, 100)),
            format!("sobram {}", projetado.formatar_com_simbolo()),
        )
    } else {
        (
            Some(100),
            format!("sobram {}", projetado.formatar_com_simbolo()),
        )
    };

    let sinais = vec![
        SinalDeSaude {
            nome: "Liquidez".into(),
            peso: 25,
            pontos: liquidez.0,
            medida: liquidez.1,
            dica: "Receba o que está para vencer e negocie prazo nas contas a pagar.".into(),
        },
        SinalDeSaude {
            nome: "Inadimplência".into(),
            peso: 20,
            pontos: inadimplencia.0,
            medida: inadimplencia.1,
            dica: "Cobre os clientes vencidos há mais de 30 dias (aba Cobrança).".into(),
        },
        SinalDeSaude {
            nome: "Margem EBITDA (3 meses)".into(),
            peso: 20,
            pontos: margem.0,
            medida: margem.1,
            dica: "Reveja preços e margens (aba Preços) e corte despesas fixas.".into(),
        },
        SinalDeSaude {
            nome: "Giro de estoque".into(),
            peso: 20,
            pontos: giro.0,
            medida: giro.1,
            dica: "Venda ou devolva o que está parado (estoque, aba Análise).".into(),
        },
        SinalDeSaude {
            nome: "Fluxo projetado (30 dias)".into(),
            peso: 15,
            pontos: fluxo.0,
            medida: fluxo.1,
            dica: "Antecipe recebimentos ou adie compras até o caixa cobrir as contas.".into(),
        },
    ];
    let (soma, pesos) = sinais
        .iter()
        .fold((0_u32, 0_u32), |(s, p), x| match x.pontos {
            Some(pt) => (s + u32::from(pt) * u32::from(x.peso), p + u32::from(x.peso)),
            None => (s, p),
        });
    let score = (soma + pesos / 2)
        .checked_div(pesos)
        .map_or(0, |v| u8::try_from(v).unwrap_or(100));
    Saude {
        score,
        semaforo: match score {
            70..=u8::MAX => Semaforo::Verde,
            40..=69 => Semaforo::Amarelo,
            _ => Semaforo::Vermelho,
        },
        sinais,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn r(x: i64) -> Dinheiro {
        Dinheiro::reais(x)
    }

    #[test]
    fn empresa_saudavel_chega_perto_de_100() {
        let s = calcular(&EntradasDeSaude {
            caixa: r(20_000),
            a_receber_30d: r(30_000),
            a_pagar_30d: r(20_000),
            inadimplencia: Some(Percentual::pontos(1)),
            margem_ebitda: Some(Percentual::pontos(18)),
            giro_mensal: Some(Percentual::pontos(120)),
        });
        assert!(s.score >= 95, "{}", s.score);
        assert_eq!(s.semaforo, Semaforo::Verde);
        assert!(s.puxando_para_baixo().is_empty());
    }

    #[test]
    fn empresa_no_aperto_fica_vermelha_e_aponta_o_que_puxa_para_baixo() {
        let s = calcular(&EntradasDeSaude {
            caixa: r(1_000),
            a_receber_30d: r(2_000),
            a_pagar_30d: r(10_000),
            inadimplencia: Some(Percentual::pontos(25)),
            margem_ebitda: Some(Percentual::pontos(-3)),
            giro_mensal: Some(Percentual::ZERO),
        });
        assert_eq!(s.semaforo, Semaforo::Vermelho);
        assert!(s.score < 10, "{}", s.score);
        let nomes: Vec<&str> = s
            .puxando_para_baixo()
            .iter()
            .map(|x| x.nome.as_str())
            .collect();
        assert_eq!(
            nomes[0], "Liquidez",
            "o de maior peso e mais fraco primeiro"
        );
        assert!(nomes.contains(&"Fluxo projetado (30 dias)"));
    }

    #[test]
    fn sinal_sem_dados_sai_do_calculo_em_vez_de_premiar() {
        // Sem receita, sem recebíveis e sem estoque: sobram liquidez e fluxo.
        let s = calcular(&EntradasDeSaude {
            caixa: r(5_000),
            a_receber_30d: r(0),
            a_pagar_30d: r(10_000),
            ..EntradasDeSaude::default()
        });
        let sem: Vec<&str> = s
            .sinais
            .iter()
            .filter(|x| x.pontos.is_none())
            .map(|x| x.nome.as_str())
            .collect();
        assert_eq!(sem.len(), 3);
        // Liquidez 0,5× = 0 pontos; fluxo negativo = 0: score 0, não 60.
        assert_eq!(s.score, 0);
    }

    #[test]
    fn escala_linear_e_invertida() {
        assert_eq!(escala(100, 50, 150), 50);
        assert_eq!(escala(0, 50, 150), 0);
        assert_eq!(escala(999, 50, 150), 100);
        assert_eq!(
            escala(10_000, 20_000, 0),
            50,
            "inadimplência de 10% vale metade"
        );
    }
}
