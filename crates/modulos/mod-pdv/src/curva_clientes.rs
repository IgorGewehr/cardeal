//! A curva de clientes: quem compra quanto (a curva ABC dos últimos 12 meses), há quanto tempo,
//! com que frequência, quem sumiu — em relação ao **ritmo dele** — e como paga
//! (`mod_financeiro::pagamento`). As compras são as vendas do PDV com o cliente identificado e os
//! pedidos faturados de vendas.
//!
//! Lição do `gestao-raiz` (`client-crm.service.ts`): lá o "valor" do cliente sai de faixas fixas
//! (R$ 100, R$ 500... R$ 10 mil gastos) e o "risco de churn" de dias fixos sem comprar (30, 60, 90,
//! 180) — as mesmas para um atacadista e para uma lojinha —, gravados no cadastro e recalculados em
//! lote. Aqui:
//! - a classe é **relativa à carteira da empresa**: A são os clientes que fazem 80% do
//!   faturamento, B os 15% seguintes, C o resto;
//! - "sumiu" é medido contra o **intervalo habitual do próprio cliente** (o dobro dele, e pelo
//!   menos 30 dias): quem compra toda semana sumiu em 30 dias; quem compra a cada 3 meses, não;
//! - a pontualidade entra (o que importa a quem vende fiado);
//! - calculada na hora, nunca fica velha.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Data, Dinheiro, Id};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use mod_financeiro::pagamento::{PerfilDePagamento, SeloDePagamento};
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Uma compra de um cliente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompraDoCliente {
    /// O cliente.
    pub cliente: Id,
    /// O dia.
    pub data: Data,
    /// O valor.
    pub valor: Dinheiro,
}

/// A classe na curva ABC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ClasseDoCliente {
    /// Os que fazem os primeiros 80% do faturamento.
    A,
    /// Os 15% seguintes.
    B,
    /// O resto.
    C,
}

/// A situação do cliente em relação ao ritmo dele.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SituacaoDoCliente {
    /// A primeira compra foi há até 60 dias.
    Novo,
    /// Compra no ritmo de sempre.
    Ativo,
    /// Passou do dobro do intervalo habitual (e de 30 dias) sem comprar.
    Sumido,
    /// Uma compra só, há mais de 60 dias.
    Esporadico,
}

impl SituacaoDoCliente {
    /// Para a tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Novo => "Novo",
            Self::Ativo => "Ativo",
            Self::Sumido => "Sumido",
            Self::Esporadico => "Esporádico",
        }
    }
}

/// Um cliente na curva.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClienteNaCurva {
    /// O cliente.
    pub cliente: Id,
    /// O nome.
    pub nome: String,
    /// O que comprou nos últimos 12 meses.
    pub faturamento: Dinheiro,
    /// Quantas compras.
    pub compras: u32,
    /// O dia da última.
    pub ultima_compra: Data,
    /// O intervalo médio entre as compras, em dias (`None` com uma compra só).
    pub intervalo_dias: Option<u32>,
    /// Há quantos dias não compra.
    pub dias_sem_comprar: u32,
    /// A classe.
    pub classe: ClasseDoCliente,
    /// A situação.
    pub situacao: SituacaoDoCliente,
    /// Como paga (`None` sem a permissão de ver o a receber).
    pub pagamento: Option<PerfilDePagamento>,
    /// O que fazer, quando há.
    pub acao: Option<String>,
}

/// Monta a curva a partir das compras dos últimos 12 meses (as mais antigas são ignoradas).
#[must_use]
pub fn montar_curva(
    compras: &[CompraDoCliente],
    hoje: Data,
    pagamentos: &[PerfilDePagamento],
) -> Vec<ClienteNaCurva> {
    let inicio = hoje.mais_meses(-12);
    let mut por_cliente: std::collections::BTreeMap<Id, Vec<CompraDoCliente>> =
        std::collections::BTreeMap::new();
    for c in compras
        .iter()
        .filter(|c| c.data >= inicio && c.data <= hoje)
    {
        por_cliente.entry(c.cliente).or_default().push(*c);
    }
    let mut curva: Vec<ClienteNaCurva> = por_cliente
        .into_iter()
        .map(|(cliente, mut cs)| {
            cs.sort_by_key(|c| c.data);
            let faturamento: Dinheiro = cs.iter().map(|c| c.valor).sum();
            let primeira = cs[0].data;
            let ultima = cs[cs.len() - 1].data;
            // Compras no mesmo dia contam como uma visita para o ritmo.
            let mut dias: Vec<Data> = cs.iter().map(|c| c.data).collect();
            dias.dedup();
            let intervalo = (dias.len() > 1).then(|| {
                let total = primeira.dias_ate(ultima);
                u32::try_from(total / i32::try_from(dias.len() - 1).unwrap_or(1)).unwrap_or(0)
            });
            let sem_comprar = u32::try_from(ultima.dias_ate(hoje).max(0)).unwrap_or(0);
            let desde_a_primeira = primeira.dias_ate(hoje);
            let situacao = if desde_a_primeira <= 60 {
                SituacaoDoCliente::Novo
            } else {
                match intervalo {
                    Some(i) if sem_comprar > (2 * i).max(30) => SituacaoDoCliente::Sumido,
                    Some(_) => SituacaoDoCliente::Ativo,
                    None => SituacaoDoCliente::Esporadico,
                }
            };
            ClienteNaCurva {
                cliente,
                nome: String::new(),
                faturamento,
                compras: u32::try_from(cs.len()).unwrap_or(u32::MAX),
                ultima_compra: ultima,
                intervalo_dias: intervalo,
                dias_sem_comprar: sem_comprar,
                classe: ClasseDoCliente::C,
                situacao,
                pagamento: pagamentos.iter().find(|p| p.cliente == cliente).cloned(),
                acao: None,
            }
        })
        .collect();
    curva.sort_by(|a, b| {
        b.faturamento
            .cmp(&a.faturamento)
            .then(a.cliente.cmp(&b.cliente))
    });
    let total: i128 = curva
        .iter()
        .map(|c| i128::from(c.faturamento.em_centavos()))
        .sum();
    let mut acumulado: i128 = 0;
    for c in &mut curva {
        // A classe é a de quem começa a faixa: o cliente que cruza os 80% ainda é A.
        let antes = acumulado;
        acumulado += i128::from(c.faturamento.em_centavos());
        c.classe = if total == 0 || antes * 100 < total * 80 {
            ClasseDoCliente::A
        } else if antes * 100 < total * 95 {
            ClasseDoCliente::B
        } else {
            ClasseDoCliente::C
        };
        c.acao = acao(c);
    }
    curva
}

fn acao(c: &ClienteNaCurva) -> Option<String> {
    if let Some(p) = &c.pagamento {
        if p.selo == SeloDePagamento::Inadimplente {
            return Some(format!(
                "Cobrar: {} vencido há {} dias",
                p.vencido.formatar_com_simbolo(),
                p.maior_atraso_dias
            ));
        }
    }
    match (c.situacao, c.classe, c.intervalo_dias) {
        (SituacaoDoCliente::Sumido, ClasseDoCliente::A | ClasseDoCliente::B, Some(i)) => {
            Some(format!(
                "Procurar: não compra há {} dias (costuma voltar em {i})",
                c.dias_sem_comprar
            ))
        }
        (SituacaoDoCliente::Novo, _, None) => Some("Acompanhar a primeira compra".into()),
        _ => None,
    }
}

/// A curva de clientes dos últimos 12 meses.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct CurvaDeClientes;

impl Consulta for CurvaDeClientes {
    type Saida = Vec<ClienteNaCurva>;
    const PERMISSAO: &'static str = "pdv.clientes.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<ClienteNaCurva>> {
        let hoje = ctx.hoje();
        let inicio = hoje.mais_meses(-12);
        let mut compras = sqlite::compras_no_pdv(c, ctx.empresa, inicio, ctx.fuso)?;
        compras.extend(
            mod_vendas::vendas_por_cliente(c, ctx.empresa, inicio)?
                .into_iter()
                .map(|(cliente, data, valor)| CompraDoCliente {
                    cliente,
                    data,
                    valor,
                }),
        );
        let pagamentos = if ctx.concede("financeiro.receber.ver") {
            mod_financeiro::pagamento::perfis_de_pagamento(c, ctx.empresa, &[], hoje)?
        } else {
            Vec::new()
        };
        let mut curva = montar_curva(&compras, hoje, &pagamentos);
        let ids: Vec<Id> = curva.iter().map(|c| c.cliente).collect();
        let nomes = mod_clientes::nomes_das_pessoas(c, &ids)?;
        for item in &mut curva {
            item.nome = nomes
                .get(&item.cliente)
                .cloned()
                .unwrap_or_else(|| "—".into());
        }
        Ok(curva)
    }
}

#[cfg(feature = "sqlite")]
mod sqlite {
    use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Fuso, Id, Instante, Resultado};
    use rusqlite::{params, Connection};

    use super::CompraDoCliente;

    /// As vendas finalizadas (não estornadas) do PDV com o cliente identificado, desde `desde`.
    pub(super) fn compras_no_pdv(
        c: &Connection,
        empresa: Id,
        desde: Data,
        fuso: Fuso,
    ) -> Resultado<Vec<CompraDoCliente>> {
        let falha = |e: rusqlite::Error| Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string());
        let meia_noite = cardeal_kernel::Hora::MEIA_NOITE;
        let de = Instante::de_data_hora(desde, meia_noite, fuso).em_micros();
        let mut stmt = c
            .prepare(
                "SELECT cliente, abertura, total FROM pdv_cupom
                 WHERE empresa = ?1 AND estado = 'Finalizado' AND cliente IS NOT NULL
                   AND abertura >= ?2
                   AND NOT EXISTS (SELECT 1 FROM pdv_estorno e WHERE e.cupom = pdv_cupom.id)",
            )
            .map_err(falha)?;
        let v = stmt
            .query_map(params![empresa.em_bytes().as_slice(), de], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?
            .into_iter()
            .map(|(cliente, quando, total)| CompraDoCliente {
                cliente: <[u8; 16]>::try_from(cliente).map_or(Id::NULO, Id::de_bytes),
                data: Instante::de_micros(quando).data(fuso),
                valor: Dinheiro::centavos(total),
            })
            .collect();
        Ok(v)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn hoje() -> Data {
        Data::de_ymd(2026, 10, 26).unwrap()
    }

    fn compra(cliente: Id, dias_atras: i32, reais: i64) -> CompraDoCliente {
        CompraDoCliente {
            cliente,
            data: Data::de_dias(hoje().em_dias() - dias_atras),
            valor: Dinheiro::reais(reais),
        }
    }

    #[test]
    fn a_classe_e_relativa_a_carteira_e_o_sumico_ao_ritmo_do_cliente() {
        let (grande, medio, pequeno, novo) = (Id::novo(), Id::novo(), Id::novo(), Id::novo());
        let mut compras = Vec::new();
        // O grande compra toda semana, mas sumiu há 40 dias: sumido (o ritmo dele é 7).
        for semana in 0..20 {
            compras.push(compra(grande, 40 + semana * 7, 1_000));
        }
        // O médio compra a cada 90 dias e a última foi há 80: ativo.
        for trimestre in 0..3 {
            compras.push(compra(medio, 80 + trimestre * 90, 1_000));
        }
        compras.push(compra(pequeno, 200, 50));
        compras.push(compra(novo, 10, 100));
        // Fora da janela de 12 meses: não conta.
        compras.push(compra(pequeno, 400, 99_999));
        let curva = montar_curva(&compras, hoje(), &[]);
        let de = |id| curva.iter().find(|c| c.cliente == id).unwrap();
        assert_eq!(curva[0].cliente, grande);
        assert_eq!(de(grande).classe, ClasseDoCliente::A);
        assert_eq!(de(grande).situacao, SituacaoDoCliente::Sumido);
        assert_eq!(de(grande).intervalo_dias, Some(7));
        assert!(de(grande).acao.as_deref().unwrap().starts_with("Procurar"));
        assert_eq!(de(medio).situacao, SituacaoDoCliente::Ativo);
        assert_eq!(de(medio).classe, ClasseDoCliente::B);
        assert_eq!(de(pequeno).situacao, SituacaoDoCliente::Esporadico);
        assert_eq!(de(pequeno).faturamento, Dinheiro::reais(50));
        assert_eq!(de(pequeno).classe, ClasseDoCliente::C);
        assert_eq!(de(novo).situacao, SituacaoDoCliente::Novo);
    }

    #[test]
    fn o_inadimplente_e_cobrado_antes_de_tudo() {
        let cliente = Id::novo();
        let perfil = mod_financeiro::pagamento::avaliar(
            cliente,
            &[mod_financeiro::pagamento::ParcelaDoCliente {
                vencimento: Data::de_ymd(2026, 8, 1).unwrap(),
                valor: Dinheiro::reais(300),
                quitada_em: None,
                em_aberto: Dinheiro::reais(300),
            }],
            hoje(),
        );
        let curva = montar_curva(&[compra(cliente, 90, 300)], hoje(), &[perfil]);
        assert_eq!(
            curva[0].acao.as_deref(),
            Some("Cobrar: R$ 300,00 vencido há 86 dias")
        );
    }
}
