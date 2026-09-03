//! Juros, multa, desconto e o plano de uma baixa — tudo calculado, nunca materializado.
//!
//! `docs/modulos/financeiro.md` §11.1: **juros e multa nunca são materializados fora da
//! baixa**. A parcela guarda só a política e a taxa; o valor devido em uma data qualquer é
//! derivado por [`Parcela::situacao_em`]. Isso evita milhares de lançamentos diários de
//! "atualização" e mantém o Razão auditável (`docs/05-nucleo-financeiro.md` §4.1).

use cardeal_kernel::{Arredondamento, Data, Dinheiro, Percentual};

use crate::erros::ErroFinanceiro;
use crate::titulo::{EstadoParcela, Parcela, PoliticaJuros};

/// Base de dias usada para converter a taxa mensal em fração diária.
const DIAS_NO_MES: i64 = 30;

/// A posição de uma parcela em uma data de referência: quanto de principal, juros, multa e
/// desconto, e o total a pagar. Read-model puro — recalculado a cada leitura, nunca gravado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SituacaoParcela {
    /// A data para a qual a situação foi calculada.
    pub referencia: Data,
    /// Dias corridos de atraso (0 se em dia ou adiantada).
    pub dias_atraso: i32,
    /// O principal ainda em aberto.
    pub saldo_principal: Dinheiro,
    /// Juros de mora acumulados até `referencia`.
    pub juros: Dinheiro,
    /// Multa por atraso (aplicada uma única vez, a partir do 1º dia).
    pub multa: Dinheiro,
    /// Desconto por antecipação disponível em `referencia` (0 se fora do prazo ou em atraso).
    pub desconto: Dinheiro,
}

impl SituacaoParcela {
    /// O total que quita a parcela em `referencia`: principal + juros + multa − desconto.
    #[must_use]
    pub fn total_devido(&self) -> Dinheiro {
        (self.saldo_principal + self.juros + self.multa - self.desconto).nao_negativo()
    }

    /// Juros + multa somados — a parte que credita Receita financeira no receituário.
    #[must_use]
    pub fn encargos(&self) -> Dinheiro {
        self.juros + self.multa
    }

    /// Verdadeiro se a parcela está em atraso na data de referência.
    #[must_use]
    pub const fn em_atraso(&self) -> bool {
        self.dias_atraso > 0
    }
}

/// Como uma baixa se decompõe: quanto do dinheiro recebido abate principal, quanto cobre
/// encargos e quanto foi desconto. É o que o receituário transforma em partidas
/// (`crate::receituario`) e o que a máquina de estado da parcela consome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlanoBaixa {
    /// A data da baixa.
    pub data: Data,
    /// O dinheiro que efetivamente entrou/saiu (principal + juros + multa − desconto).
    pub valor_recebido: Dinheiro,
    /// A parte que abate o `valor_baixado` da parcela.
    pub principal: Dinheiro,
    /// Juros de mora cobrados nesta baixa.
    pub juros: Dinheiro,
    /// Multa cobrada nesta baixa.
    pub multa: Dinheiro,
    /// Desconto por antecipação concedido nesta baixa.
    pub desconto: Dinheiro,
    /// Verdadeiro se a baixa zera o saldo da parcela.
    pub quita: bool,
}

impl PlanoBaixa {
    /// O estado para o qual a parcela transita depois desta baixa.
    #[must_use]
    pub const fn estado_resultante(&self) -> EstadoParcela {
        if self.quita {
            EstadoParcela::Quitada
        } else {
            EstadoParcela::Parcial
        }
    }
}

impl Parcela {
    /// Calcula a situação da parcela em uma data (sem alterá-la).
    #[must_use]
    pub fn situacao_em(&self, referencia: Data) -> SituacaoParcela {
        let saldo = self.saldo_principal();
        let dias_atraso = self.vencimento.dias_ate(referencia).max(0);

        let juros = self.juros_em(saldo, dias_atraso);
        let multa = if dias_atraso >= 1 {
            saldo.aplicar(
                self.multa.unwrap_or(Percentual::ZERO),
                Arredondamento::MeioAcima,
            )
        } else {
            Dinheiro::ZERO
        };
        let desconto = self.desconto_em(referencia, dias_atraso, saldo);

        SituacaoParcela {
            referencia,
            dias_atraso,
            saldo_principal: saldo,
            juros,
            multa,
            desconto,
        }
    }

    fn juros_em(&self, saldo: Dinheiro, dias_atraso: i32) -> Dinheiro {
        if dias_atraso < 1 {
            return Dinheiro::ZERO;
        }
        let Some(taxa) = self.taxa_juros else {
            return Dinheiro::ZERO;
        };
        let dias = i64::from(dias_atraso);
        let unidades = match self.politica_juros {
            PoliticaJuros::Nenhum => return Dinheiro::ZERO,
            PoliticaJuros::SimplesDiario => i128::from(taxa.unidades_internas()) * i128::from(dias),
            PoliticaJuros::SimplesMensal => {
                i128::from(taxa.unidades_internas()) * i128::from(dias) / i128::from(DIAS_NO_MES)
            }
        };
        let unidades = i64::try_from(unidades).unwrap_or(i64::MAX);
        saldo.aplicar(Percentual::unidades(unidades), Arredondamento::MeioAcima)
    }

    fn desconto_em(&self, referencia: Data, dias_atraso: i32, saldo: Dinheiro) -> Dinheiro {
        if dias_atraso > 0 {
            return Dinheiro::ZERO;
        }
        match (self.desconto_ate_data, self.desconto_valor) {
            (Some(ate), Some(valor)) if referencia <= ate && valor.e_positivo() => valor.min(saldo),
            _ => Dinheiro::ZERO,
        }
    }

    /// Monta o plano de uma baixa de `valor_informado` na data `referencia`.
    ///
    /// Numa baixa **total** (o valor bate com o total devido) o desconto por antecipação é
    /// concedido e a parcela é quitada. Numa baixa **parcial** não há desconto e o dinheiro
    /// cobre primeiro os encargos (juros, depois multa) e só o restante abate o principal —
    /// assim o saldo em aberto reflete a dívida de verdade.
    ///
    /// # Errors
    /// - [`ErroFinanceiro::ParcelaNaoBaixavel`] se o estado não aceita baixa.
    /// - [`ErroFinanceiro::BaixaZerada`] se o valor informado não é positivo.
    /// - [`ErroFinanceiro::ValorSuperaSaldo`] se o valor passa do total devido na data.
    pub fn planejar_baixa(
        &self,
        referencia: Data,
        valor_informado: Dinheiro,
    ) -> Result<PlanoBaixa, ErroFinanceiro> {
        if !self.estado.aceita_baixa() {
            return Err(ErroFinanceiro::ParcelaNaoBaixavel(self.estado));
        }
        if !valor_informado.e_positivo() {
            return Err(ErroFinanceiro::BaixaZerada);
        }

        let sit = self.situacao_em(referencia);
        let devido = sit.total_devido();
        if valor_informado > devido {
            return Err(ErroFinanceiro::ValorSuperaSaldo {
                recebido: valor_informado,
                devido,
            });
        }

        if valor_informado == devido {
            return Ok(PlanoBaixa {
                data: referencia,
                valor_recebido: valor_informado,
                principal: sit.saldo_principal,
                juros: sit.juros,
                multa: sit.multa,
                desconto: sit.desconto,
                quita: true,
            });
        }

        // Baixa parcial: sem desconto, encargos primeiro.
        let juros = sit.juros.min(valor_informado);
        let multa = sit.multa.min(valor_informado - juros);
        let principal = valor_informado - juros - multa;
        let quita = (self.valor_baixado + principal) >= self.valor;

        Ok(PlanoBaixa {
            data: referencia,
            valor_recebido: valor_informado,
            principal,
            juros,
            multa,
            desconto: Dinheiro::ZERO,
            quita,
        })
    }

    /// Aplica um plano de baixa à parcela: soma o principal ao `valor_baixado`, avança o
    /// estado e incrementa a versão. Não toca no Razão — isso é do comando.
    pub fn aplicar_baixa(&mut self, plano: &PlanoBaixa) {
        self.valor_baixado += plano.principal;
        self.estado = plano.estado_resultante();
        self.versao = self.versao.proxima();
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Fuso, Id, Versao};
    use proptest::prelude::*;

    use super::*;
    use crate::titulo::{ConstrutorTitulo, EspecieTitulo};
    use cardeal_ledger::Contraparte;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn parcela_de(valor: Dinheiro, vencimento: Data) -> Parcela {
        Parcela {
            id: Id::novo(),
            empresa: Id::novo(),
            titulo: Id::novo(),
            numero: 1,
            vencimento,
            valor,
            estado: EstadoParcela::Aberta,
            valor_baixado: Dinheiro::ZERO,
            lancamento: None,
            nosso_numero: None,
            politica_juros: PoliticaJuros::Nenhum,
            taxa_juros: None,
            multa: None,
            desconto_ate_data: None,
            desconto_valor: None,
            versao: Versao::INICIAL,
        }
    }

    #[test]
    fn em_dia_nao_tem_juros_nem_multa() {
        let p = parcela_de(Dinheiro::reais(100), hoje().mais_dias(5));
        let sit = p.situacao_em(hoje());
        assert_eq!(sit.dias_atraso, 0);
        assert_eq!(sit.juros, Dinheiro::ZERO);
        assert_eq!(sit.multa, Dinheiro::ZERO);
        assert_eq!(sit.total_devido(), Dinheiro::reais(100));
    }

    #[test]
    fn juros_diario_e_multa_unica() {
        let mut p = parcela_de(Dinheiro::reais(100), hoje().mais_dias(-10));
        p.politica_juros = PoliticaJuros::SimplesDiario;
        p.taxa_juros = Some(Percentual::milesimos(100)); // 0,1% ao dia
        p.multa = Some(Percentual::pontos(2)); // 2%

        let sit = p.situacao_em(hoje());
        assert_eq!(sit.dias_atraso, 10);
        // 0,1%/dia * 10 dias = 1% de 100 = R$ 1,00
        assert_eq!(sit.juros, Dinheiro::reais(1));
        // multa 2% de 100 = R$ 2,00, uma única vez
        assert_eq!(sit.multa, Dinheiro::reais(2));
        assert_eq!(sit.total_devido(), Dinheiro::centavos(10_300));
    }

    #[test]
    fn desconto_por_antecipacao_so_vale_no_prazo() {
        let mut p = parcela_de(Dinheiro::reais(100), hoje().mais_dias(20));
        p.desconto_ate_data = Some(hoje().mais_dias(5));
        p.desconto_valor = Some(Dinheiro::reais(3));

        assert_eq!(p.situacao_em(hoje()).desconto, Dinheiro::reais(3));
        assert_eq!(p.situacao_em(hoje()).total_devido(), Dinheiro::reais(97));
        // Depois da data-limite, sem desconto.
        assert_eq!(p.situacao_em(hoje().mais_dias(10)).desconto, Dinheiro::ZERO);
        // Em atraso, jamais.
        assert_eq!(p.situacao_em(hoje().mais_dias(25)).desconto, Dinheiro::ZERO);
    }

    #[test]
    fn baixa_total_quita_e_concede_desconto() {
        let mut p = parcela_de(Dinheiro::reais(100), hoje().mais_dias(20));
        p.desconto_ate_data = Some(hoje().mais_dias(5));
        p.desconto_valor = Some(Dinheiro::reais(3));

        let plano = p.planejar_baixa(hoje(), Dinheiro::reais(97)).unwrap();
        assert!(plano.quita);
        assert_eq!(plano.principal, Dinheiro::reais(100));
        assert_eq!(plano.desconto, Dinheiro::reais(3));
        p.aplicar_baixa(&plano);
        assert_eq!(p.estado, EstadoParcela::Quitada);
        assert_eq!(p.saldo_principal(), Dinheiro::ZERO);
    }

    #[test]
    fn baixa_parcial_cobre_encargos_primeiro() {
        let mut p = parcela_de(Dinheiro::reais(100), hoje().mais_dias(-30));
        p.politica_juros = PoliticaJuros::SimplesDiario;
        p.taxa_juros = Some(Percentual::milesimos(100)); // 0,1%/dia -> 3% em 30 dias
        p.multa = Some(Percentual::pontos(2));
        // juros 3,00 + multa 2,00 + principal 100,00 = 105,00 devido
        let plano = p.planejar_baixa(hoje(), Dinheiro::reais(10)).unwrap();
        assert_eq!(plano.juros, Dinheiro::reais(3));
        assert_eq!(plano.multa, Dinheiro::reais(2));
        assert_eq!(plano.principal, Dinheiro::reais(5));
        assert!(!plano.quita);
        p.aplicar_baixa(&plano);
        assert_eq!(p.estado, EstadoParcela::Parcial);
        assert_eq!(p.valor_baixado, Dinheiro::reais(5));
    }

    #[test]
    fn baixa_acima_do_devido_e_recusada() {
        let p = parcela_de(Dinheiro::reais(100), hoje());
        let erro = p.planejar_baixa(hoje(), Dinheiro::reais(101)).unwrap_err();
        assert!(matches!(erro, ErroFinanceiro::ValorSuperaSaldo { .. }));
    }

    #[test]
    fn parcela_quitada_nao_aceita_baixa() {
        let mut p = parcela_de(Dinheiro::reais(100), hoje());
        p.estado = EstadoParcela::Quitada;
        let erro = p.planejar_baixa(hoje(), Dinheiro::reais(1)).unwrap_err();
        assert!(matches!(
            erro,
            ErroFinanceiro::ParcelaNaoBaixavel(EstadoParcela::Quitada)
        ));
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        /// Sequência de baixas parciais nunca ultrapassa o principal e sempre converge para
        /// quitada quando a soma alcança o valor da parcela.
        #[test]
        fn baixas_parciais_conservam_o_principal(
            valor_centavos in 100i64..=5_000_000,
            fatias in prop::collection::vec(1i64..=100, 1..8),
        ) {
            let total = Dinheiro::centavos(valor_centavos);
            let tcp = ConstrutorTitulo::novo(
                Id::novo(),
                EspecieTitulo::Receber,
                Contraparte::Cliente(Id::novo()),
                total,
                hoje(),
            )
            .construir()
            .unwrap();
            let mut p = tcp.parcelas.into_iter().next().unwrap();

            // Sem juros/multa: total devido == saldo principal em qualquer data.
            let mut baixado = Dinheiro::ZERO;
            for peso in fatias {
                if !p.estado.aceita_baixa() {
                    break;
                }
                let saldo = p.saldo_principal();
                let pedido = saldo.fracao(peso, 100, Arredondamento::Cima).min(saldo).max(Dinheiro::centavos(1));
                let plano = p.planejar_baixa(hoje(), pedido).unwrap();
                prop_assert_eq!(plano.juros, Dinheiro::ZERO);
                prop_assert_eq!(plano.desconto, Dinheiro::ZERO);
                p.aplicar_baixa(&plano);
                baixado += plano.principal;
                prop_assert!(p.valor_baixado <= p.valor);
                prop_assert_eq!(p.valor_baixado, baixado);
            }
            if p.valor_baixado == p.valor {
                prop_assert_eq!(p.estado, EstadoParcela::Quitada);
            }
        }
    }
}
