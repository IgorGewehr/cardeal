//! Como o cliente paga — o selo e a nota de pagamento (`docs/modulos/clientes.md` §3, o "score
//! interno de pagamento"), calculados na hora a partir das parcelas a receber dele.
//!
//! Lição do `gestao-raiz` (`client-crm.service.ts`): lá a nota de "lealdade", "valor" e "risco de
//! churn" sai de faixas fixas (R$ 100, R$ 500... R$ 10 mil gastos; 30, 60, 90 dias sem comprar),
//! as mesmas para qualquer empresa, gravadas no cadastro e recalculadas em lote (ficam velhas), e
//! **não olham se o cliente paga em dia** — que é o que importa a quem vende fiado. Aqui:
//! - só conta a venda **a prazo** (vencimento depois da emissão): a venda à vista quitada na hora
//!   faria de todo mundo um bom pagador;
//! - pago até 3 dias depois do vencimento conta como em dia (o boleto compensa em D+1, D+2);
//! - a nota (0 a 1000) é a fração do valor pago em dia nos últimos 12 meses, menos 10 pontos por
//!   dia do maior atraso em aberto;
//! - o selo diz o que fazer: "Inadimplente" (algo vencido há mais de 30 dias), "Atenção" (vencido
//!   há menos, ou menos de 80% pago em dia), "Bom pagador" (95% ou mais em dia e nada vencido),
//!   "Regular", "Sem histórico".
//! - calculado na hora: nunca fica velho.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Data, Dinheiro, Id};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Pago até tantos dias depois do vencimento ainda é em dia.
pub const TOLERANCIA_DIAS: i32 = 3;

/// Uma parcela a prazo do cliente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParcelaDoCliente {
    /// O vencimento.
    pub vencimento: Data,
    /// O valor da parcela.
    pub valor: Dinheiro,
    /// Quando foi quitada (a data da baixa que a quitou), se foi.
    pub quitada_em: Option<Data>,
    /// O que falta receber.
    pub em_aberto: Dinheiro,
}

/// O selo de pagamento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SeloDePagamento {
    /// 95% ou mais pago em dia, nada vencido.
    BomPagador,
    /// Paga, com algum atraso.
    Regular,
    /// Algo vencido há até 30 dias, ou menos de 80% pago em dia.
    Atencao,
    /// Algo vencido há mais de 30 dias.
    Inadimplente,
    /// Nunca comprou a prazo (ou nada venceu ainda).
    SemHistorico,
}

impl SeloDePagamento {
    /// Para a tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::BomPagador => "Bom pagador",
            Self::Regular => "Regular",
            Self::Atencao => "Atenção",
            Self::Inadimplente => "Inadimplente",
            Self::SemHistorico => "Sem histórico",
        }
    }
}

/// Como um cliente paga.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfilDePagamento {
    /// O cliente.
    pub cliente: Id,
    /// O selo.
    pub selo: SeloDePagamento,
    /// A nota, de 0 a 1000 (`None` sem histórico).
    pub nota: Option<u16>,
    /// Parcelas quitadas que venceram nos últimos 12 meses.
    pub pagas: u32,
    /// Delas, as pagas em dia.
    pub pagas_em_dia: u32,
    /// O atraso médio das pagas com atraso, em dias.
    pub atraso_medio_dias: u32,
    /// O que está vencido e em aberto.
    pub vencido: Dinheiro,
    /// Há quantos dias está vencida a parcela em aberto mais antiga.
    pub maior_atraso_dias: u32,
    /// O que está em aberto e ainda não venceu.
    pub a_vencer: Dinheiro,
}

/// Avalia as parcelas a prazo de um cliente em `hoje`.
#[must_use]
pub fn avaliar(cliente: Id, parcelas: &[ParcelaDoCliente], hoje: Data) -> PerfilDePagamento {
    let um_ano_antes = hoje.mais_meses(-12);
    let (mut pagas, mut em_dia, mut dias_de_atraso) = (0_u32, 0_u32, 0_i64);
    let (mut valor_pago, mut valor_em_dia) = (0_i64, 0_i64);
    let (mut vencido, mut a_vencer, mut maior_atraso) = (Dinheiro::ZERO, Dinheiro::ZERO, 0_i32);
    for p in parcelas {
        if let Some(quitada) = p.quitada_em {
            if p.vencimento < um_ano_antes || p.vencimento > hoje {
                continue;
            }
            pagas += 1;
            valor_pago += p.valor.em_centavos();
            let atraso = p.vencimento.dias_ate(quitada);
            if atraso <= TOLERANCIA_DIAS {
                em_dia += 1;
                valor_em_dia += p.valor.em_centavos();
            } else {
                dias_de_atraso += i64::from(atraso);
            }
            continue;
        }
        if p.em_aberto.e_zero() {
            continue;
        }
        if p.vencimento < hoje {
            vencido += p.em_aberto;
            maior_atraso = maior_atraso.max(p.vencimento.dias_ate(hoje));
        } else {
            a_vencer += p.em_aberto;
        }
    }
    let atrasadas = pagas - em_dia;
    let atraso_medio_dias = if atrasadas > 0 {
        u32::try_from(dias_de_atraso / i64::from(atrasadas)).unwrap_or(u32::MAX)
    } else {
        0
    };
    let maior_atraso_dias = u32::try_from(maior_atraso.max(0)).unwrap_or(0);
    let sem_historico = pagas == 0 && vencido.e_zero();
    let pontualidade = if valor_pago > 0 {
        // em milésimos
        i64::try_from(i128::from(valor_em_dia) * 1000 / i128::from(valor_pago)).unwrap_or(0)
    } else {
        1000
    };
    let nota = (!sem_historico).then(|| {
        let n = (pontualidade - i64::from(maior_atraso_dias) * 10).clamp(0, 1000);
        u16::try_from(n).unwrap_or(0)
    });
    let selo = if sem_historico {
        SeloDePagamento::SemHistorico
    } else if maior_atraso_dias > 30 {
        SeloDePagamento::Inadimplente
    } else if maior_atraso_dias > 0 || pontualidade < 800 {
        SeloDePagamento::Atencao
    } else if pontualidade >= 950 {
        SeloDePagamento::BomPagador
    } else {
        SeloDePagamento::Regular
    };
    PerfilDePagamento {
        cliente,
        selo,
        nota,
        pagas,
        pagas_em_dia: em_dia,
        atraso_medio_dias,
        vencido,
        maior_atraso_dias,
        a_vencer,
    }
}

/// Como pagam os clientes (os pedidos; vazio = todos os que compraram a prazo).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PerfisDePagamento {
    /// Os clientes.
    pub clientes: Vec<Id>,
}

impl Consulta for PerfisDePagamento {
    type Saida = Vec<PerfilDePagamento>;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<PerfilDePagamento>> {
        perfis_de_pagamento(c, ctx.empresa, &self.clientes, ctx.hoje())
    }
}

/// Os perfis de pagamento dos `clientes` (vazio = todos os que compraram a prazo) — a porta
/// pública para outro módulo (a curva de clientes do PDV).
///
/// # Errors
/// Erro do SQLite.
#[cfg(feature = "sqlite")]
pub fn perfis_de_pagamento(
    c: &Connection,
    empresa: Id,
    clientes: &[Id],
    hoje: Data,
) -> Resultado<Vec<PerfilDePagamento>> {
    use cardeal_kernel::{CodigoErro, Erro};
    use rusqlite::params;
    let falha = |e: rusqlite::Error| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string());
    // As parcelas a prazo (vencimento depois da emissão) de títulos a receber de clientes, não
    // canceladas nem renegociadas, com a data da última baixa válida.
    let mut stmt = c
        .prepare(
            "SELECT t.contraparte_id, p.vencimento, p.valor, p.valor_baixado, p.estado,
                    (SELECT MAX(b.data) FROM financeiro_baixa b
                      WHERE b.parcela = p.id AND b.estornada_em IS NULL)
             FROM financeiro_parcela p
             JOIN financeiro_titulo t ON t.id = p.titulo
             WHERE t.empresa = ?1 AND t.especie = 'Receber' AND t.contraparte_tipo = 'Cliente'
               AND t.cancelado_em IS NULL AND p.estado IN ('Aberta','Parcial','Quitada')
               AND p.vencimento > t.emissao",
        )
        .map_err(falha)?;
    let linhas = stmt
        .query_map(params![empresa.em_bytes().as_slice()], |r| {
            Ok((
                r.get::<_, Vec<u8>>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, Option<i64>>(5)?,
            ))
        })
        .map_err(falha)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(falha)?;
    let dia = |d: i64| Data::de_dias(i32::try_from(d).unwrap_or(0));
    let mut por_cliente: std::collections::BTreeMap<Id, Vec<ParcelaDoCliente>> =
        std::collections::BTreeMap::new();
    for (cliente, vencimento, valor, baixado, estado, ultima_baixa) in linhas {
        let cliente = <[u8; 16]>::try_from(cliente).map_or(Id::NULO, Id::de_bytes);
        if !clientes.is_empty() && !clientes.contains(&cliente) {
            continue;
        }
        let quitada = estado == "Quitada";
        por_cliente
            .entry(cliente)
            .or_default()
            .push(ParcelaDoCliente {
                vencimento: dia(vencimento),
                valor: Dinheiro::centavos(valor),
                quitada_em: quitada.then(|| ultima_baixa.map_or(dia(vencimento), dia)),
                em_aberto: if quitada {
                    Dinheiro::ZERO
                } else {
                    Dinheiro::centavos(valor - baixado)
                },
            });
    }
    let mut perfis: Vec<PerfilDePagamento> = por_cliente
        .into_iter()
        .map(|(cliente, parcelas)| avaliar(cliente, &parcelas, hoje))
        .collect();
    // Quem foi pedido e não comprou a prazo: sem histórico.
    for cliente in clientes {
        if !perfis.iter().any(|p| p.cliente == *cliente) {
            perfis.push(avaliar(*cliente, &[], hoje));
        }
    }
    Ok(perfis)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn d(dia: u32) -> Data {
        Data::de_ymd(2026, 10, dia).unwrap()
    }

    fn paga(vencimento: Data, quitada: Data, reais: i64) -> ParcelaDoCliente {
        ParcelaDoCliente {
            vencimento,
            valor: Dinheiro::reais(reais),
            quitada_em: Some(quitada),
            em_aberto: Dinheiro::ZERO,
        }
    }

    fn aberta(vencimento: Data, reais: i64) -> ParcelaDoCliente {
        ParcelaDoCliente {
            vencimento,
            valor: Dinheiro::reais(reais),
            quitada_em: None,
            em_aberto: Dinheiro::reais(reais),
        }
    }

    #[test]
    fn quem_paga_em_dia_e_bom_pagador_e_a_tolerancia_e_de_3_dias() {
        let hoje = d(26);
        let p = avaliar(
            Id::novo(),
            &[
                paga(d(1), d(1), 100),
                paga(d(5), d(8), 100),
                aberta(d(30), 50),
            ],
            hoje,
        );
        assert_eq!(p.selo, SeloDePagamento::BomPagador);
        assert_eq!(p.nota, Some(1000));
        assert_eq!((p.pagas, p.pagas_em_dia), (2, 2));
        assert_eq!(p.a_vencer, Dinheiro::reais(50));
        assert!(p.vencido.e_zero());
    }

    #[test]
    fn o_atraso_em_aberto_pesa_e_passa_de_30_dias_e_inadimplencia() {
        let hoje = d(26);
        let atencao = avaliar(
            Id::novo(),
            &[paga(d(1), d(1), 100), aberta(d(16), 80)],
            hoje,
        );
        assert_eq!(atencao.selo, SeloDePagamento::Atencao);
        assert_eq!(atencao.maior_atraso_dias, 10);
        assert_eq!(atencao.nota, Some(900));
        assert_eq!(atencao.vencido, Dinheiro::reais(80));
        let calote = avaliar(
            Id::novo(),
            &[aberta(Data::de_ymd(2026, 9, 1).unwrap(), 80)],
            hoje,
        );
        assert_eq!(calote.selo, SeloDePagamento::Inadimplente);
        assert_eq!(calote.maior_atraso_dias, 55);
    }

    #[test]
    fn quem_paga_sempre_atrasado_e_atencao_e_sem_prazo_e_sem_historico() {
        let hoje = d(26);
        let atrasado = avaliar(
            Id::novo(),
            &[paga(d(1), d(15), 100), paga(d(2), d(2), 100)],
            hoje,
        );
        // 50% do valor em dia: abaixo de 80%.
        assert_eq!(atrasado.selo, SeloDePagamento::Atencao);
        assert_eq!(atrasado.nota, Some(500));
        assert_eq!(atrasado.atraso_medio_dias, 14);
        let novo = avaliar(Id::novo(), &[aberta(d(30), 100)], hoje);
        assert_eq!(novo.selo, SeloDePagamento::SemHistorico);
        assert_eq!(novo.nota, None);
    }
}
