//! Projeção por categoria e mês: para onde vai (ou de onde vem) o dinheiro da empresa — o que
//! já foi pago, o que está em aberto e o que as recorrências ainda vão gerar. É a aba "Custos"
//! do Financeiro: "quanto vou gastar com aluguel, pró-labore e energia nos próximos meses, e
//! por quê".

#[cfg(feature = "sqlite")]
use std::collections::HashMap;

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Competencia, Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Data, Periodo};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::recorrencia::Recorrencia;
#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, data_de, especie_txt, id_de, persist, recorrencia_de_linha};
use crate::titulo::EspecieTitulo;

/// Uma categoria num mês: o realizado (baixas), o em aberto (saldo das parcelas que vencem no
/// mês; as já vencidas contam no mês corrente) e o que as recorrências ainda vão gerar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemProjecaoCategoria {
    /// A categoria; `None` = sem categoria.
    pub categoria: Option<Id>,
    /// O mês.
    pub competencia: Competencia,
    /// Já pago/recebido no mês.
    pub realizado: Dinheiro,
    /// Saldo em aberto que vence no mês (ou já venceu, no mês corrente).
    pub em_aberto: Dinheiro,
    /// Ocorrências de recorrências que ainda não viraram título.
    pub recorrente: Dinheiro,
}

impl ItemProjecaoCategoria {
    /// Realizado + em aberto + recorrente: o custo (ou a receita) total esperado no mês.
    #[must_use]
    pub fn total(&self) -> Dinheiro {
        self.realizado + self.em_aberto + self.recorrente
    }
}

/// A projeção de uma espécie (a pagar = custos; a receber = receitas) mês a mês, por
/// categoria, a partir de `de` por `meses` meses (1..=24).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ProjecaoPorCategoria {
    /// Custos (`Pagar`) ou receitas (`Receber`).
    pub especie: EspecieTitulo,
    /// O primeiro mês.
    pub de: Competencia,
    /// Quantos meses.
    pub meses: u8,
}

impl Consulta for ProjecaoPorCategoria {
    type Saida = Vec<ItemProjecaoCategoria>;
    const PERMISSAO: &'static str = "financeiro.projecao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let meses = i32::from(self.meses.clamp(1, 24));
        let janela = Periodo::novo(
            self.de.primeiro_dia(),
            self.de.primeiro_dia().mais_meses(meses - 1).fim_do_mes(),
        );
        let mut acc = Acumulador::default();
        realizado(conexao, ctx.empresa, self.especie, janela, &mut acc)?;
        em_aberto(
            conexao,
            ctx.empresa,
            self.especie,
            janela,
            ctx.hoje(),
            &mut acc,
        )?;
        for r in recorrencias_ativas(conexao, ctx.empresa)? {
            if r.especie == self.especie {
                projetar_recorrencia(&r, janela, ctx.hoje(), &mut acc);
            }
        }
        Ok(acc.itens())
    }
}

#[derive(Default)]
#[cfg(feature = "sqlite")]
struct Acumulador(HashMap<(Option<Id>, Competencia), ItemProjecaoCategoria>);

#[cfg(feature = "sqlite")]
impl Acumulador {
    #[cfg(feature = "sqlite")]
    fn item(
        &mut self,
        categoria: Option<Id>,
        competencia: Competencia,
    ) -> &mut ItemProjecaoCategoria {
        self.0
            .entry((categoria, competencia))
            .or_insert(ItemProjecaoCategoria {
                categoria,
                competencia,
                realizado: Dinheiro::ZERO,
                em_aberto: Dinheiro::ZERO,
                recorrente: Dinheiro::ZERO,
            })
    }

    #[cfg(feature = "sqlite")]
    fn itens(self) -> Vec<ItemProjecaoCategoria> {
        let mut v: Vec<_> = self.0.into_values().collect();
        v.sort_by_key(|i| (i.competencia, i.categoria));
        v
    }
}

#[cfg(feature = "sqlite")]
fn realizado(
    conexao: &Connection,
    empresa: Id,
    especie: EspecieTitulo,
    janela: Periodo,
    acc: &mut Acumulador,
) -> Resultado<()> {
    let mut stmt = conexao
        .prepare(
            "SELECT t.categoria, b.data, b.valor_recebido
             FROM financeiro_baixa b
             JOIN financeiro_parcela p ON p.id = b.parcela
             JOIN financeiro_titulo t ON t.id = p.titulo
             WHERE t.empresa = ?1 AND t.especie = ?2 AND b.estornada_em IS NULL
                   AND b.data BETWEEN ?3 AND ?4",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(
            params![
                blob(empresa),
                especie_txt(especie),
                i64::from(janela.de.em_dias()),
                i64::from(janela.ate.em_dias()),
            ],
            |r| {
                Ok((
                    r.get::<_, Option<Vec<u8>>>(0)?.map(id_de),
                    data_de(r.get(1)?),
                    Dinheiro::centavos(r.get(2)?),
                ))
            },
        )
        .map_err(persist)?;
    for linha in linhas {
        let (categoria, data, valor) = linha.map_err(persist)?;
        acc.item(categoria, data.competencia()).realizado += valor;
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
fn em_aberto(
    conexao: &Connection,
    empresa: Id,
    especie: EspecieTitulo,
    janela: Periodo,
    hoje: Data,
    acc: &mut Acumulador,
) -> Resultado<()> {
    let mut stmt = conexao
        .prepare(
            "SELECT t.categoria, p.vencimento, p.valor - p.valor_baixado
             FROM financeiro_parcela p
             JOIN financeiro_titulo t ON t.id = p.titulo
             WHERE t.empresa = ?1 AND t.especie = ?2 AND p.estado IN ('Aberta','Parcial')
                   AND p.vencimento <= ?3",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(
            params![
                blob(empresa),
                especie_txt(especie),
                i64::from(janela.ate.em_dias()),
            ],
            |r| {
                Ok((
                    r.get::<_, Option<Vec<u8>>>(0)?.map(id_de),
                    data_de(r.get(1)?),
                    Dinheiro::centavos(r.get(2)?),
                ))
            },
        )
        .map_err(persist)?;
    for linha in linhas {
        let (categoria, vencimento, saldo) = linha.map_err(persist)?;
        // O que já venceu e não foi pago continua devido: conta no mês corrente.
        let quando = vencimento.max(hoje);
        if janela.contem(quando) {
            acc.item(categoria, quando.competencia()).em_aberto += saldo;
        }
    }
    Ok(())
}

#[cfg(feature = "sqlite")]
fn recorrencias_ativas(conexao: &Connection, empresa: Id) -> Resultado<Vec<Recorrencia>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, empresa, descricao, especie, contraparte_tipo, contraparte_id,
                    tipo_valor, valor_fixo, indice, media_ultimos_n, periodicidade,
                    dia_referencia, expressao_cron, inicio, fim, conta_contrapartida,
                    centro_custo, categoria, antecedencia_geracao_dias, ativa, versao
             FROM financeiro_recorrencia WHERE empresa = ?1 AND ativa = 1",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], recorrencia_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// As ocorrências que a materialização ainda não gerou (depois de `hoje` + antecedência) e
/// caem na janela. Recorrência de valor não fixo fica de fora — não há valor para projetar.
#[cfg(feature = "sqlite")]
fn projetar_recorrencia(r: &Recorrencia, janela: Periodo, hoje: Data, acc: &mut Acumulador) {
    let Ok(valor) = r.valor_de() else { return };
    let gerada_ate = hoje.mais_dias(i32::from(r.antecedencia_geracao_dias));
    let de = janela.de.max(gerada_ate.mais_dias(1));
    if de > janela.ate {
        return;
    }
    for data in r
        .ocorrencias(Periodo::novo(de, janela.ate))
        .unwrap_or_default()
    {
        acc.item(r.categoria, data.competencia()).recorrente += valor;
    }
}
