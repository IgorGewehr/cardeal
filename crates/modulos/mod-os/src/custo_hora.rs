//! O custo-hora da equipe técnica — quanto custa uma hora de cada técnico (salário, encargos,
//! benefícios ÷ horas produtivas) e o **padrão da oficina** para quem não tem o seu. É o que
//! faltava para a mão de obra entrar nas margens: sem ele, o serviço era receita sem custo e
//! ficava fora da conta (nunca "lucro de 100%"). Domínio puro.

use std::collections::HashMap;

use cardeal_kernel::{Arredondamento, Dinheiro, Id, Preco, Quantidade};
use serde::{Deserialize, Serialize};

/// O custo-hora de cada técnico e o padrão.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabelaCustoHora {
    /// O padrão da oficina (quem não tem o seu).
    pub padrao: Option<Dinheiro>,
    /// O custo-hora próprio de cada técnico (usuário).
    pub tecnicos: Vec<(Id, Dinheiro)>,
}

impl TabelaCustoHora {
    /// O custo-hora de um técnico: o dele, senão o padrão.
    #[must_use]
    pub fn do_tecnico(&self, tecnico: Id) -> Option<Dinheiro> {
        self.tecnicos
            .iter()
            .find(|(t, _)| *t == tecnico)
            .map(|(_, c)| *c)
            .or(self.padrao)
    }

    /// O custo das horas apontadas, técnico a técnico (segundos por técnico). Devolve o custo
    /// e se algum técnico ficou sem custo-hora (o tempo dele não entra).
    #[must_use]
    pub fn custo_apontado(&self, segundos_por_tecnico: &HashMap<Id, i64>) -> (Dinheiro, bool) {
        let mut total = Dinheiro::ZERO;
        let mut faltou = false;
        for (tecnico, segundos) in segundos_por_tecnico {
            match self.do_tecnico(*tecnico) {
                Some(c) => total += custo_do_tempo(*segundos, c),
                None => faltou |= *segundos > 0,
            }
        }
        (total, faltou)
    }
}

/// Horas (com 4 casas) a partir de segundos.
#[must_use]
pub fn horas_de(segundos: i64) -> Quantidade {
    // Quantidade guarda 1e-4: horas × 10⁴ = segundos × 10⁴ / 3600.
    Quantidade::interna(segundos.saturating_mul(10_000) / 3_600)
}

/// O custo de um tempo ao custo-hora, arredondado ao centavo.
#[must_use]
pub fn custo_do_tempo(segundos: i64, custo_hora: Dinheiro) -> Dinheiro {
    custo_das_horas(horas_de(segundos), custo_hora)
}

/// O custo de `horas` ao custo-hora, arredondado ao centavo.
#[must_use]
pub fn custo_das_horas(horas: Quantidade, custo_hora: Dinheiro) -> Dinheiro {
    Dinheiro::de_total(horas, preco_da_hora(custo_hora), Arredondamento::MeioAcima)
}

/// O custo-hora como preço unitário (por hora).
#[must_use]
pub fn preco_da_hora(custo_hora: Dinheiro) -> Preco {
    // Dinheiro em centavos; Preco com 6 casas: centavos × 10⁴.
    Preco::interna(custo_hora.em_centavos().saturating_mul(10_000))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn proprio_vence_o_padrao_e_sem_nenhum_e_none() {
        let (a, b) = (Id::novo(), Id::novo());
        let mut t = TabelaCustoHora {
            padrao: None,
            tecnicos: vec![(a, Dinheiro::reais(40))],
        };
        assert_eq!(t.do_tecnico(a), Some(Dinheiro::reais(40)));
        assert_eq!(t.do_tecnico(b), None);
        t.padrao = Some(Dinheiro::reais(25));
        assert_eq!(t.do_tecnico(b), Some(Dinheiro::reais(25)));
    }

    #[test]
    fn tempo_vira_custo_proporcional() {
        // 1h30 a R$ 40/h = R$ 60; 20 min a R$ 30/h = R$ 10.
        assert_eq!(
            custo_do_tempo(5_400, Dinheiro::reais(40)),
            Dinheiro::reais(60)
        );
        assert_eq!(
            custo_do_tempo(1_200, Dinheiro::reais(30)),
            Dinheiro::reais(10)
        );
        assert_eq!(horas_de(5_400), Quantidade::de_str("1,5").unwrap());
        assert_eq!(
            custo_das_horas(Quantidade::de_str("2,5").unwrap(), Dinheiro::reais(32)),
            Dinheiro::reais(80)
        );
    }

    #[test]
    fn custo_apontado_soma_quem_tem_e_avisa_quem_falta() {
        let (a, b) = (Id::novo(), Id::novo());
        let t = TabelaCustoHora {
            padrao: None,
            tecnicos: vec![(a, Dinheiro::reais(40))],
        };
        let tempos = HashMap::from([(a, 3_600), (b, 1_800)]);
        assert_eq!(t.custo_apontado(&tempos), (Dinheiro::reais(40), true));
        let so_a = HashMap::from([(a, 1_800)]);
        assert_eq!(t.custo_apontado(&so_a), (Dinheiro::reais(20), false));
    }
}
