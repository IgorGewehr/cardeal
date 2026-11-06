//! Os quadros de vencimento da Visão geral: do que vence **nesta semana**, **neste mês** e **no
//! próximo**, quanto estava previsto, quanto já foi recebido (ou pago) e quanto falta — e o que já
//! venceu e está em aberto ("em atraso"). É o "resumo semanal / mês corrente / próximo mês / contas
//! em atraso" de quem controla o caixa pelos vencimentos.
//!
//! Por vencimento, não pela data do pagamento: "realizado" é a parte já baixada das parcelas que
//! vencem no período (pagar adiantado conta no período do vencimento; o que se pagou hoje de uma
//! conta atrasada não). Entra o principal — juros e multa da baixa não; desconto deixa a parcela
//! quitada com o realizado abaixo do previsto. Parcela cancelada ou renegociada sai (as novas, da
//! renegociação, entram).

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Data, DiaDaSemana, Dinheiro};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::titulo::{EspecieTitulo, EstadoParcela};

/// Do que vence num período: previsto, realizado e pendente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuadroDoPeriodo {
    /// O primeiro dia.
    pub de: Data,
    /// O último dia.
    pub ate: Data,
    /// O valor das parcelas que vencem no período.
    pub previsto: Dinheiro,
    /// O que delas já foi baixado (principal).
    pub realizado: Dinheiro,
    /// O que delas ainda está em aberto.
    pub pendente: Dinheiro,
    /// Quantas parcelas vencem no período.
    pub parcelas: u32,
}

impl QuadroDoPeriodo {
    const fn novo(de: Data, ate: Data) -> Self {
        Self {
            de,
            ate,
            previsto: Dinheiro::ZERO,
            realizado: Dinheiro::ZERO,
            pendente: Dinheiro::ZERO,
            parcelas: 0,
        }
    }

    fn contem(&self, d: Data) -> bool {
        self.de <= d && d <= self.ate
    }
}

/// O que já venceu e está em aberto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QuadroDeAtraso {
    /// Quantas parcelas.
    pub parcelas: u32,
    /// O saldo em aberto delas.
    pub saldo: Dinheiro,
    /// O vencimento mais antigo.
    pub mais_antigo: Option<Data>,
}

/// Os quadros de uma espécie (a receber ou a pagar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuadrosDeVencimento {
    /// A receber ou a pagar.
    pub especie: EspecieTitulo,
    /// De segunda a domingo da semana de hoje.
    pub semana: QuadroDoPeriodo,
    /// O mês de hoje.
    pub mes: QuadroDoPeriodo,
    /// O mês seguinte.
    pub proximo_mes: QuadroDoPeriodo,
    /// Vencido antes de hoje e em aberto.
    pub atraso: QuadroDeAtraso,
}

/// Uma parcela como os quadros a enxergam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParcelaDoQuadro {
    /// Quando vence.
    pub vencimento: Data,
    /// O valor da parcela.
    pub valor: Dinheiro,
    /// O principal já baixado.
    pub baixado: Dinheiro,
    /// O estado.
    pub estado: EstadoParcela,
}

/// A segunda-feira da semana de `d` (a semana vai de segunda a domingo).
#[must_use]
pub const fn inicio_da_semana(d: Data) -> Data {
    let recuo = match d.dia_da_semana() {
        DiaDaSemana::Segunda => 0,
        DiaDaSemana::Terca => 1,
        DiaDaSemana::Quarta => 2,
        DiaDaSemana::Quinta => 3,
        DiaDaSemana::Sexta => 4,
        DiaDaSemana::Sabado => 5,
        DiaDaSemana::Domingo => 6,
    };
    d.mais_dias(-recuo)
}

/// Monta os quadros de uma espécie para `hoje`.
#[must_use]
pub fn montar(
    especie: EspecieTitulo,
    hoje: Data,
    parcelas: &[ParcelaDoQuadro],
) -> QuadrosDeVencimento {
    let segunda = inicio_da_semana(hoje);
    let proximo = hoje.fim_do_mes().mais_dias(1);
    let mut q = QuadrosDeVencimento {
        especie,
        semana: QuadroDoPeriodo::novo(segunda, segunda.mais_dias(6)),
        mes: QuadroDoPeriodo::novo(hoje.inicio_do_mes(), hoje.fim_do_mes()),
        proximo_mes: QuadroDoPeriodo::novo(proximo, proximo.fim_do_mes()),
        atraso: QuadroDeAtraso::default(),
    };
    for p in parcelas {
        let em_aberto = matches!(p.estado, EstadoParcela::Aberta | EstadoParcela::Parcial);
        if !em_aberto && p.estado != EstadoParcela::Quitada {
            continue;
        }
        let pendente = if em_aberto {
            p.valor - p.baixado
        } else {
            Dinheiro::ZERO
        };
        for quadro in [&mut q.semana, &mut q.mes, &mut q.proximo_mes] {
            if quadro.contem(p.vencimento) {
                quadro.previsto += p.valor;
                quadro.realizado += p.baixado;
                quadro.pendente += pendente;
                quadro.parcelas += 1;
            }
        }
        if em_aberto && p.vencimento < hoje && pendente.e_positivo() {
            q.atraso.parcelas += 1;
            q.atraso.saldo += pendente;
            q.atraso.mais_antigo = Some(
                q.atraso
                    .mais_antigo
                    .map_or(p.vencimento, |m| m.min(p.vencimento)),
            );
        }
    }
    q
}

/// Os quadros de vencimento a receber.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VencimentosAReceber {
    /// O dia de referência (a semana, o mês e o atraso partem dele).
    pub hoje: Data,
}

impl Consulta for VencimentosAReceber {
    type Saida = QuadrosDeVencimento;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<QuadrosDeVencimento> {
        sqlite::quadros(conexao, ctx.empresa, EspecieTitulo::Receber, self.hoje)
    }
}

/// Os quadros de vencimento a pagar.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VencimentosAPagar {
    /// O dia de referência.
    pub hoje: Data,
}

impl Consulta for VencimentosAPagar {
    type Saida = QuadrosDeVencimento;
    const PERMISSAO: &'static str = "financeiro.pagar.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<QuadrosDeVencimento> {
        sqlite::quadros(conexao, ctx.empresa, EspecieTitulo::Pagar, self.hoje)
    }
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use cardeal_kernel::{Data, Dinheiro, Id, Resultado};
    use rusqlite::{params, Connection};

    use super::{inicio_da_semana, montar, ParcelaDoQuadro, QuadrosDeVencimento};
    use crate::repositorio::{blob, persist};
    use crate::titulo::{EspecieTitulo, EstadoParcela};

    fn estado_de(s: &str) -> EstadoParcela {
        match s {
            "Aberta" => EstadoParcela::Aberta,
            "Parcial" => EstadoParcela::Parcial,
            "Quitada" => EstadoParcela::Quitada,
            "Renegociada" => EstadoParcela::Renegociada,
            _ => EstadoParcela::Cancelada,
        }
    }

    /// As parcelas que vencem da segunda desta semana (ou do dia 1º) ao fim do próximo mês, e as
    /// em aberto vencidas antes de hoje.
    pub(super) fn quadros(
        c: &Connection,
        empresa: Id,
        especie: EspecieTitulo,
        hoje: Data,
    ) -> Resultado<QuadrosDeVencimento> {
        let de = inicio_da_semana(hoje).min(hoje.inicio_do_mes());
        let ate = hoje.fim_do_mes().mais_dias(1).fim_do_mes();
        let mut stmt = c
            .prepare(
                "SELECT p.vencimento, p.valor, p.valor_baixado, p.estado
                 FROM financeiro_parcela p JOIN financeiro_titulo t ON t.id = p.titulo
                 WHERE t.empresa = ?1 AND t.especie = ?2
                   AND p.estado IN ('Aberta','Parcial','Quitada')
                   AND (p.vencimento BETWEEN ?3 AND ?4
                        OR (p.estado IN ('Aberta','Parcial') AND p.vencimento < ?5))",
            )
            .map_err(persist)?;
        let parcelas = stmt
            .query_map(
                params![
                    blob(empresa),
                    match especie {
                        EspecieTitulo::Receber => "Receber",
                        EspecieTitulo::Pagar => "Pagar",
                    },
                    i64::from(de.em_dias()),
                    i64::from(ate.em_dias()),
                    i64::from(hoje.em_dias())
                ],
                |r| {
                    Ok(ParcelaDoQuadro {
                        vencimento: Data::de_dias(i32::try_from(r.get::<_, i64>(0)?).unwrap_or(0)),
                        valor: Dinheiro::centavos(r.get(1)?),
                        baixado: Dinheiro::centavos(r.get(2)?),
                        estado: estado_de(&r.get::<_, String>(3)?),
                    })
                },
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        Ok(montar(especie, hoje, &parcelas))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(dia: u32, mes: u32) -> Data {
        Data::de_ymd(2026, mes, dia).unwrap()
    }

    fn p(dia: u32, mes: u32, valor: i64, baixado: i64, estado: EstadoParcela) -> ParcelaDoQuadro {
        ParcelaDoQuadro {
            vencimento: d(dia, mes),
            valor: Dinheiro::reais(valor),
            baixado: Dinheiro::reais(baixado),
            estado,
        }
    }

    #[test]
    fn semana_mes_proximo_e_atraso() {
        use EstadoParcela::{Aberta, Cancelada, Parcial, Quitada, Renegociada};
        // Quinta, 5 de novembro de 2026: a semana vai de 2 a 8.
        let hoje = d(5, 11);
        assert_eq!(inicio_da_semana(hoje), d(2, 11));
        assert_eq!(inicio_da_semana(d(8, 11)), d(2, 11));
        assert_eq!(inicio_da_semana(d(2, 11)), d(2, 11));
        let q = montar(
            EspecieTitulo::Receber,
            hoje,
            &[
                // Na semana: uma paga, uma pela metade (vencida ontem: também atrasada).
                p(3, 11, 100, 100, Quitada),
                p(4, 11, 200, 80, Parcial),
                // No mês, fora da semana.
                p(20, 11, 1_000, 0, Aberta),
                // No próximo mês.
                p(10, 12, 300, 0, Aberta),
                // Atrasada de outubro.
                p(15, 10, 50, 0, Aberta),
                // Fora: cancelada e renegociada.
                p(21, 11, 999, 0, Cancelada),
                p(22, 11, 999, 0, Renegociada),
            ],
        );
        assert_eq!(q.semana.de, d(2, 11));
        assert_eq!(q.semana.ate, d(8, 11));
        assert_eq!(q.semana.previsto, Dinheiro::reais(300));
        assert_eq!(q.semana.realizado, Dinheiro::reais(180));
        assert_eq!(q.semana.pendente, Dinheiro::reais(120));
        assert_eq!(q.semana.parcelas, 2);
        assert_eq!(q.mes.previsto, Dinheiro::reais(1_300));
        assert_eq!(q.mes.pendente, Dinheiro::reais(1_120));
        assert_eq!(q.proximo_mes.de, d(1, 12));
        assert_eq!(q.proximo_mes.ate, d(31, 12));
        assert_eq!(q.proximo_mes.previsto, Dinheiro::reais(300));
        assert_eq!(q.atraso.parcelas, 2);
        assert_eq!(q.atraso.saldo, Dinheiro::reais(170));
        assert_eq!(q.atraso.mais_antigo, Some(d(15, 10)));
    }

    #[test]
    fn a_semana_que_vira_o_mes_conta_nos_dois_meses_certos() {
        // Terça, 29 de dezembro: a semana vai de 28/12 a 3/1; o próximo mês é janeiro de 2027.
        let hoje = d(29, 12);
        let q = montar(
            EspecieTitulo::Pagar,
            hoje,
            &[ParcelaDoQuadro {
                vencimento: Data::de_ymd(2027, 1, 2).unwrap(),
                valor: Dinheiro::reais(40),
                baixado: Dinheiro::ZERO,
                estado: EstadoParcela::Aberta,
            }],
        );
        assert_eq!(q.semana.previsto, Dinheiro::reais(40));
        assert_eq!(q.mes.previsto, Dinheiro::ZERO);
        assert_eq!(q.proximo_mes.previsto, Dinheiro::reais(40));
        assert_eq!(q.atraso, QuadroDeAtraso::default());
    }
}
