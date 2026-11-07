//! Os relatórios do dia a dia do caixa (Financeiro › Relatórios): o que entrou e saiu **por dia**,
//! as contas a receber e a pagar **por faixa de vencimento** e o **fluxo de caixa previsto** do
//! saldo de hoje em diante. As consultas trazem os dados; o agrupamento é puro e testado aqui.
//!
//! Lição do controle financeiro que o pessoal do lunno usava (doc `modulos/financeiro.md` §6.11):
//! lá, "Contas a pagar" de novembro mostrava "Vencidos: R$ 0,00" com 102 contas vencidas antes —
//! o filtro de período escondia o atraso. Aqui as faixas são de **todo** o em aberto: o que venceu
//! antes aparece sempre em "Vencidos".

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Data, Dinheiro, Id, Periodo};
use cardeal_ledger::Contraparte;
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::consultas::ItemTituloEmAberto;
use crate::titulo::EspecieTitulo;

/// Uma baixa (recebimento ou pagamento) de um período, com o título por trás dela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemBaixaNoPeriodo {
    /// A baixa.
    pub baixa: Id,
    /// O título.
    pub titulo: Id,
    /// Quando o dinheiro entrou ou saiu.
    pub data: Data,
    /// O que entrou ou saiu (principal + juros + multa − desconto), positivo.
    pub valor: Dinheiro,
    /// Com quem.
    pub contraparte: Option<Contraparte>,
    /// A observação do título ("Aluguel da loja").
    pub descricao: Option<String>,
    /// A categoria do título.
    pub categoria: Option<Id>,
    /// O módulo de origem (`"vendas"`, `"os"`, `"avulso"`…).
    pub origem_modulo: String,
}

/// Os recebimentos de um período (baixas não estornadas de títulos a receber), por data.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RecebimentosNoPeriodo {
    /// O período (data da baixa), inclusive nas pontas.
    pub periodo: Periodo,
}

impl Consulta for RecebimentosNoPeriodo {
    type Saida = Vec<ItemBaixaNoPeriodo>;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        sqlite::baixas(c, ctx.empresa, EspecieTitulo::Receber, self.periodo)
    }
}

/// Os pagamentos de um período (baixas não estornadas de títulos a pagar), por data.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PagamentosNoPeriodo {
    /// O período (data da baixa), inclusive nas pontas.
    pub periodo: Periodo,
}

impl Consulta for PagamentosNoPeriodo {
    type Saida = Vec<ItemBaixaNoPeriodo>;
    const PERMISSAO: &'static str = "financeiro.pagar.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Self::Saida> {
        sqlite::baixas(c, ctx.empresa, EspecieTitulo::Pagar, self.periodo)
    }
}

/// Um dia com as suas baixas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiaDeBaixas<'a> {
    /// O dia.
    pub data: Data,
    /// As baixas do dia, na ordem em que vieram.
    pub itens: Vec<&'a ItemBaixaNoPeriodo>,
    /// O total do dia.
    pub total: Dinheiro,
}

/// As baixas agrupadas por dia, do mais antigo ao mais recente, com o total de cada dia.
#[must_use]
pub fn por_dia(itens: &[ItemBaixaNoPeriodo]) -> Vec<DiaDeBaixas<'_>> {
    let mut ordenados: Vec<&ItemBaixaNoPeriodo> = itens.iter().collect();
    ordenados.sort_by_key(|i| i.data);
    let mut dias: Vec<DiaDeBaixas<'_>> = Vec::new();
    for i in ordenados {
        match dias.last_mut() {
            Some(d) if d.data == i.data => {
                d.total += i.valor;
                d.itens.push(i);
            }
            _ => dias.push(DiaDeBaixas {
                data: i.data,
                itens: vec![i],
                total: i.valor,
            }),
        }
    }
    dias
}

/// As faixas de vencimento das contas em aberto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum FaixaDeVencimento {
    /// Venceu antes de hoje.
    Vencidos,
    /// Vence hoje.
    Hoje,
    /// De amanhã a 7 dias.
    Proximos7,
    /// De 8 a 15 dias.
    De8a15,
    /// De 16 a 30 dias.
    De16a30,
    /// Depois de 30 dias.
    Depois30,
}

impl FaixaDeVencimento {
    /// Todas, na ordem de exibição.
    pub const TODAS: [Self; 6] = [
        Self::Vencidos,
        Self::Hoje,
        Self::Proximos7,
        Self::De8a15,
        Self::De16a30,
        Self::Depois30,
    ];

    /// O rótulo na tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Vencidos => "Vencidos",
            Self::Hoje => "Vencendo hoje",
            Self::Proximos7 => "Próximos 7 dias",
            Self::De8a15 => "De 8 a 15 dias",
            Self::De16a30 => "De 16 a 30 dias",
            Self::Depois30 => "Depois de 30 dias",
        }
    }

    /// A faixa de um vencimento visto de `hoje`.
    #[must_use]
    pub const fn de(vencimento: Data, hoje: Data) -> Self {
        let dias = hoje.dias_ate(vencimento);
        if dias < 0 {
            Self::Vencidos
        } else if dias == 0 {
            Self::Hoje
        } else if dias <= 7 {
            Self::Proximos7
        } else if dias <= 15 {
            Self::De8a15
        } else if dias <= 30 {
            Self::De16a30
        } else {
            Self::Depois30
        }
    }
}

/// Uma faixa com as suas parcelas (por vencimento) e o saldo em aberto.
#[derive(Debug, Clone)]
pub struct ParcelasDaFaixa<'a> {
    /// A faixa.
    pub faixa: FaixaDeVencimento,
    /// As parcelas em aberto, da que vence antes à que vence depois.
    pub itens: Vec<&'a ItemTituloEmAberto>,
    /// O saldo em aberto da faixa.
    pub total: Dinheiro,
}

/// As parcelas em aberto separadas pelas seis faixas — todas as faixas, mesmo vazias (a tela diz
/// "nada vencendo hoje" em vez de esconder a faixa).
#[must_use]
pub fn por_faixa(itens: &[ItemTituloEmAberto], hoje: Data) -> Vec<ParcelasDaFaixa<'_>> {
    let mut faixas: Vec<ParcelasDaFaixa<'_>> = FaixaDeVencimento::TODAS
        .iter()
        .map(|f| ParcelasDaFaixa {
            faixa: *f,
            itens: Vec::new(),
            total: Dinheiro::ZERO,
        })
        .collect();
    let mut ordenados: Vec<&ItemTituloEmAberto> = itens.iter().collect();
    ordenados.sort_by_key(|i| i.vencimento);
    for i in ordenados {
        let f = FaixaDeVencimento::de(i.vencimento, hoje);
        if let Some(fx) = faixas.iter_mut().find(|x| x.faixa == f) {
            fx.total += i.saldo();
            fx.itens.push(i);
        }
    }
    faixas
}

/// Uma linha do fluxo de caixa previsto.
#[derive(Debug, Clone)]
pub struct LinhaPrevista<'a> {
    /// A parcela.
    pub parcela: &'a ItemTituloEmAberto,
    /// O que deve entrar (a receber), ou zero.
    pub entrada: Dinheiro,
    /// O que deve sair (a pagar), ou zero.
    pub saida: Dinheiro,
    /// O caixa depois desta linha.
    pub saldo: Dinheiro,
    /// Já venceu e continua em aberto: entra no começo, como o que se espera resolver já.
    pub vencida: bool,
}

/// O fluxo de caixa previsto de hoje até `ate`: o saldo de hoje e cada parcela em aberto (a
/// receber entra, a pagar sai) pelo vencimento, com o saldo acumulado. As vencidas e em aberto
/// vêm primeiro, marcadas — são dinheiro esperado (ou devido) que o saldo de hoje ainda não viu.
#[must_use]
pub fn fluxo_previsto(
    saldo_de_hoje: Dinheiro,
    em_aberto: &[ItemTituloEmAberto],
    hoje: Data,
    ate: Data,
) -> Vec<LinhaPrevista<'_>> {
    let mut itens: Vec<&ItemTituloEmAberto> = em_aberto
        .iter()
        .filter(|i| i.vencimento <= ate && i.saldo().e_positivo())
        .collect();
    itens.sort_by_key(|i| (i.vencimento, i.especie == EspecieTitulo::Receber));
    let mut saldo = saldo_de_hoje;
    itens
        .into_iter()
        .map(|p| {
            let (entrada, saida) = match p.especie {
                EspecieTitulo::Receber => (p.saldo(), Dinheiro::ZERO),
                EspecieTitulo::Pagar => (Dinheiro::ZERO, p.saldo()),
            };
            saldo += entrada - saida;
            LinhaPrevista {
                parcela: p,
                entrada,
                saida,
                saldo,
                vencida: p.vencimento < hoje,
            }
        })
        .collect()
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use cardeal_kernel::{Dinheiro, Id, Periodo, Resultado};
    use rusqlite::{params, Connection};

    use super::ItemBaixaNoPeriodo;
    use crate::repositorio::{blob, contraparte_join_opt, data_de, especie_txt, id_de, persist};
    use crate::titulo::EspecieTitulo;

    pub(super) fn baixas(
        c: &Connection,
        empresa: Id,
        especie: EspecieTitulo,
        periodo: Periodo,
    ) -> Resultado<Vec<ItemBaixaNoPeriodo>> {
        let mut stmt = c
            .prepare(
                "SELECT b.id, t.id, b.data, b.valor_recebido, t.contraparte_tipo, t.contraparte_id,
                        t.observacao, t.categoria, t.origem_modulo
                 FROM financeiro_baixa b
                 JOIN financeiro_parcela p ON p.id = b.parcela
                 JOIN financeiro_titulo t ON t.id = p.titulo
                 WHERE b.empresa = ?1 AND t.especie = ?2 AND b.estornada_em IS NULL
                   AND b.data BETWEEN ?3 AND ?4
                 ORDER BY b.data, b.criado_em
                 LIMIT 50000",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                params![
                    blob(empresa),
                    especie_txt(especie),
                    i64::from(periodo.de.em_dias()),
                    i64::from(periodo.ate.em_dias())
                ],
                |r| {
                    Ok(ItemBaixaNoPeriodo {
                        baixa: id_de(r.get(0)?),
                        titulo: id_de(r.get(1)?),
                        data: data_de(r.get(2)?),
                        valor: Dinheiro::centavos(r.get(3)?),
                        contraparte: contraparte_join_opt(&r.get::<_, String>(4)?, r.get(5)?),
                        descricao: r.get(6)?,
                        categoria: r.get::<_, Option<Vec<u8>>>(7)?.map(id_de),
                        origem_modulo: r.get(8)?,
                    })
                },
            )
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        Ok(linhas)
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::titulo::EstadoParcela;

    fn d(dia: u32, mes: u32) -> Data {
        Data::de_ymd(2026, mes, dia).unwrap()
    }

    fn baixa(dia: u32, valor: i64) -> ItemBaixaNoPeriodo {
        ItemBaixaNoPeriodo {
            baixa: Id::novo(),
            titulo: Id::novo(),
            data: d(dia, 5),
            valor: Dinheiro::reais(valor),
            contraparte: None,
            descricao: None,
            categoria: None,
            origem_modulo: "avulso".into(),
        }
    }

    fn parcela(
        especie: EspecieTitulo,
        vencimento: Data,
        valor: i64,
        baixado: i64,
    ) -> ItemTituloEmAberto {
        ItemTituloEmAberto {
            parcela: Id::novo(),
            titulo: Id::novo(),
            numero: 1,
            especie,
            contraparte: None,
            vencimento,
            valor_original: Dinheiro::reais(valor),
            valor_baixado: Dinheiro::reais(baixado),
            estado: if baixado > 0 {
                EstadoParcela::Parcial
            } else {
                EstadoParcela::Aberta
            },
            descricao: None,
            origem_modulo: "avulso".into(),
            origem_id: None,
            categoria: None,
        }
    }

    #[test]
    fn por_dia_agrupa_e_soma_em_ordem() {
        let itens = [baixa(10, 120), baixa(3, 50), baixa(10, 1_200)];
        let dias = por_dia(&itens);
        assert_eq!(dias.len(), 2);
        assert_eq!(dias[0].data, d(3, 5));
        assert_eq!(dias[1].data, d(10, 5));
        assert_eq!(dias[1].itens.len(), 2);
        assert_eq!(dias[1].total, Dinheiro::reais(1_320));
    }

    #[test]
    fn faixas_cobrem_tudo_e_os_vencidos_aparecem_sempre() {
        let hoje = d(5, 11);
        let p = EspecieTitulo::Pagar;
        let itens = [
            parcela(p, d(10, 9), 300, 0),   // vencido há meses
            parcela(p, d(5, 11), 100, 40),  // hoje, parcial
            parcela(p, d(10, 11), 120, 0),  // 5 dias
            parcela(p, d(12, 11), 130, 0),  // 7 dias
            parcela(p, d(13, 11), 480, 0),  // 8 dias
            parcela(p, d(20, 11), 50, 0),   // 15 dias
            parcela(p, d(5, 12), 70, 0),    // 30 dias
            parcela(p, d(6, 12), 1_200, 0), // 31 dias
        ];
        let f = por_faixa(&itens, hoje);
        let totais: Vec<(FaixaDeVencimento, usize, i64)> = f
            .iter()
            .map(|x| (x.faixa, x.itens.len(), x.total.em_centavos() / 100))
            .collect();
        assert_eq!(
            totais,
            [
                (FaixaDeVencimento::Vencidos, 1, 300),
                (FaixaDeVencimento::Hoje, 1, 60),
                (FaixaDeVencimento::Proximos7, 2, 250),
                (FaixaDeVencimento::De8a15, 2, 530),
                (FaixaDeVencimento::De16a30, 1, 70),
                (FaixaDeVencimento::Depois30, 1, 1_200),
            ]
        );
        assert_eq!(
            f.iter().map(|x| x.total).sum::<Dinheiro>(),
            itens
                .iter()
                .map(ItemTituloEmAberto::saldo)
                .sum::<Dinheiro>()
        );
        // Sem nada: as seis faixas, vazias.
        assert_eq!(por_faixa(&[], hoje).len(), 6);
    }

    #[test]
    fn previsto_parte_do_saldo_de_hoje_e_poe_as_vencidas_primeiro() {
        let hoje = d(5, 11);
        let itens = [
            parcela(EspecieTitulo::Receber, d(14, 11), 1_000, 0),
            parcela(EspecieTitulo::Pagar, d(10, 11), 1_200, 0),
            parcela(EspecieTitulo::Pagar, d(1, 11), 100, 0), // vencida
            parcela(
                EspecieTitulo::Pagar,
                Data::de_ymd(2027, 1, 10).unwrap(),
                999,
                0,
            ), // fora (depois do fim)
            parcela(EspecieTitulo::Receber, d(10, 11), 500, 200),
        ];
        let l = fluxo_previsto(Dinheiro::reais(24_951), &itens, hoje, d(5, 12));
        let resumo: Vec<(bool, i64, i64, i64)> = l
            .iter()
            .map(|x| {
                (
                    x.vencida,
                    x.entrada.em_centavos() / 100,
                    x.saida.em_centavos() / 100,
                    x.saldo.em_centavos() / 100,
                )
            })
            .collect();
        assert_eq!(
            resumo,
            [
                (true, 0, 100, 24_851),
                // No mesmo dia, o que sai antes do que entra (o pior caso primeiro).
                (false, 0, 1_200, 23_651),
                (false, 300, 0, 23_951),
                (false, 1_000, 0, 24_951),
            ]
        );
    }
}
