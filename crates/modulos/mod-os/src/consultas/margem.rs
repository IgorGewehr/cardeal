//! Margem das OS: receita menos o custo das peças aplicadas e da mão de obra apontada (horas ×
//! custo-hora do técnico, ver [`crate::custo_hora`]) — de um período (o cartão da Visão geral do
//! Financeiro), de uma OS só (o diálogo de baixa da parcela da OS) e a prevista no orçamento.

#[cfg(feature = "sqlite")]
use std::collections::HashMap;

use crate::custo_hora::TabelaCustoHora;

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Dinheiro, Id};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use super::{blob, id_de, itens_peca_da_ordem, persist};

/// O custo-hora da equipe da empresa (o padrão da oficina guarda o técnico vazio).
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
#[cfg(feature = "sqlite")]
pub fn tabela_custo_hora(conexao: &Connection, empresa: Id) -> Resultado<TabelaCustoHora> {
    let mut stmt = conexao
        .prepare("SELECT tecnico, custo FROM os_custo_hora WHERE empresa = ?1")
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, Dinheiro::centavos(r.get(1)?)))
        })
        .map_err(persist)?;
    let mut tabela = TabelaCustoHora::default();
    for linha in linhas {
        let (tecnico, custo) = linha.map_err(persist)?;
        if tecnico.is_empty() {
            tabela.padrao = Some(custo);
        } else {
            tabela.tecnicos.push((id_de(tecnico), custo));
        }
    }
    Ok(tabela)
}

/// Os segundos apontados (encerrados) de cada técnico numa OS.
#[cfg(feature = "sqlite")]
fn segundos_por_tecnico(conexao: &Connection, ordem: Id) -> Resultado<HashMap<Id, i64>> {
    let mut stmt = conexao
        .prepare(
            "SELECT tecnico, SUM(fim - inicio) FROM os_apontamento_tempo
             WHERE ordem_servico = ?1 AND fim IS NOT NULL GROUP BY tecnico",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(ordem)], |r| {
            Ok((
                id_de(r.get::<_, Vec<u8>>(0)?),
                r.get::<_, i64>(1)? / 1_000_000,
            ))
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<HashMap<_, _>>>()
        .map_err(persist)
}

/// O custo-hora da equipe: o padrão da oficina e o de cada técnico.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CustosHora;

impl Consulta for CustosHora {
    type Saida = TabelaCustoHora;
    const PERMISSAO: &'static str = "os.margem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        tabela_custo_hora(conexao, ctx.empresa)
    }
}

/// Receita, custo das peças e da mão de obra e margem das OS faturadas num período — "quanto
/// as OS deram de verdade no mês". A data é a do faturamento (o passo `Faturada` da linha do
/// tempo); OS faturadas antes da linha do tempo existir ficam de fora. O custo das peças é o
/// das aplicadas e não estornadas (o mesmo CMV do lançamento de faturamento); o da mão de obra
/// é o tempo apontado × custo-hora de cada técnico (técnico sem custo-hora não entra, e a
/// consulta avisa).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MargemDasOrdensNoPeriodo {
    /// O período (datas de faturamento).
    pub periodo: cardeal_kernel::Periodo,
}

/// O que [`MargemDasOrdensNoPeriodo`] devolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct MargemDasOrdens {
    /// Quantas OS faturadas no período.
    pub ordens: u32,
    /// Soma do total faturado.
    pub receita: cardeal_kernel::Dinheiro,
    /// Soma do custo das peças aplicadas.
    pub custo_pecas: cardeal_kernel::Dinheiro,
    /// Soma do custo da mão de obra apontada (horas × custo-hora).
    pub custo_mao_de_obra: cardeal_kernel::Dinheiro,
    /// Houve tempo apontado de técnico sem custo-hora (ficou fora do custo).
    pub mao_de_obra_sem_custo: bool,
}

impl MargemDasOrdens {
    /// Receita menos custo das peças e da mão de obra.
    #[must_use]
    pub fn margem(&self) -> cardeal_kernel::Dinheiro {
        self.receita - self.custo_pecas - self.custo_mao_de_obra
    }
}

impl Consulta for MargemDasOrdensNoPeriodo {
    type Saida = MargemDasOrdens;
    const PERMISSAO: &'static str = "os.ordem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        // O instante (micros UTC) do começo do primeiro dia e do fim do último, em Brasília.
        let meia_noite = |d: cardeal_kernel::Data| {
            cardeal_kernel::Instante::de_data_hora(
                d,
                cardeal_kernel::Hora::MEIA_NOITE,
                cardeal_kernel::Fuso::BRASILIA,
            )
            .em_micros()
        };
        let de = meia_noite(self.periodo.de);
        let ate = meia_noite(self.periodo.ate.mais_dias(1));
        let mut stmt = conexao
            .prepare(
                "SELECT o.id, o.valor_total
                 FROM os_ordem_servico o
                 WHERE o.empresa = ?1 AND o.estado = 'Faturada'
                   AND (SELECT MAX(h.momento) FROM os_historico h
                        WHERE h.ordem_servico = o.id AND h.estado = 'Faturada')
                       BETWEEN ?2 AND ?3 - 1",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(rusqlite::params![blob(ctx.empresa), de, ate], |r| {
                Ok((id_de(r.get::<_, Vec<u8>>(0)?), r.get::<_, i64>(1)?))
            })
            .map_err(persist)?;
        let tabela = tabela_custo_hora(conexao, ctx.empresa)?;
        let mut m = MargemDasOrdens::default();
        for linha in linhas {
            let (id, total) = linha.map_err(persist)?;
            m.ordens += 1;
            m.receita += cardeal_kernel::Dinheiro::centavos(total);
            for item in itens_peca_da_ordem(conexao, id)? {
                m.custo_pecas += item.total_custo();
            }
            let (custo, faltou) = tabela.custo_apontado(&segundos_por_tecnico(conexao, id)?);
            m.custo_mao_de_obra += custo;
            m.mao_de_obra_sem_custo |= faltou;
        }
        Ok(m)
    }
}

/// Receita, custo das peças e margem de uma OS — o que o Financeiro mostra ao receber a
/// parcela dela ("essa OS rendeu quanto?"). O custo é o das peças aplicadas e não estornadas;
/// peça ainda não aplicada entra em [`MargemDaOrdemServico::pecas_pendentes`], não no custo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MargemDaOrdem {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

/// O que [`MargemDaOrdem`] devolve.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MargemDaOrdemServico {
    /// O número da OS.
    pub numero: u64,
    /// O total da OS.
    pub receita: Dinheiro,
    /// O custo das peças aplicadas.
    pub custo_pecas: Dinheiro,
    /// O custo da mão de obra apontada (horas × custo-hora de cada técnico).
    pub custo_mao_de_obra: Dinheiro,
    /// Houve tempo apontado de técnico sem custo-hora (ficou fora do custo).
    pub mao_de_obra_sem_custo: bool,
    /// Peças orçadas ainda não aplicadas (sem custo real ainda).
    pub pecas_pendentes: u32,
}

impl MargemDaOrdemServico {
    /// Receita menos custo das peças e da mão de obra.
    #[must_use]
    pub fn margem(&self) -> Dinheiro {
        self.receita - self.custo_pecas - self.custo_mao_de_obra
    }

    /// A margem em % da receita (inteiro, arredondado para baixo); `None` sem receita.
    #[must_use]
    pub fn percentual(&self) -> Option<i64> {
        self.receita
            .e_positivo()
            .then(|| self.margem().em_centavos() * 100 / self.receita.em_centavos())
    }
}

impl Consulta for MargemDaOrdem {
    type Saida = Option<MargemDaOrdemServico>;
    const PERMISSAO: &'static str = "os.ordem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let ordem = conexao
            .query_row(
                "SELECT numero, valor_total FROM os_ordem_servico WHERE empresa = ?1 AND id = ?2",
                [blob(ctx.empresa), blob(self.ordem_servico)],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(persist)?;
        let Some((numero, total)) = ordem else {
            return Ok(None);
        };
        let mut m = MargemDaOrdemServico {
            numero: u64::try_from(numero).unwrap_or_default(),
            receita: Dinheiro::centavos(total),
            custo_pecas: Dinheiro::ZERO,
            custo_mao_de_obra: Dinheiro::ZERO,
            mao_de_obra_sem_custo: false,
            pecas_pendentes: 0,
        };
        let tabela = tabela_custo_hora(conexao, ctx.empresa)?;
        (m.custo_mao_de_obra, m.mao_de_obra_sem_custo) =
            tabela.custo_apontado(&segundos_por_tecnico(conexao, self.ordem_servico)?);
        for item in itens_peca_da_ordem(conexao, self.ordem_servico)? {
            if item.estornada {
                continue;
            }
            if item.aplicada {
                m.custo_pecas += item.total_custo();
            } else {
                m.pecas_pendentes += 1;
            }
        }
        Ok(Some(m))
    }
}

/// O painel de margem **prevista** de uma OS, ainda no orçamento: peças (ao custo real quando já
/// aplicadas, senão ao custo médio de hoje) e a mão de obra, descontando operação, impostos,
/// comissão e taxas com os mesmos percentuais do preço sugerido.
///
/// A mão de obra custa **horas × custo-hora do técnico** (o dele, senão o padrão da oficina): as
/// horas orçadas do item, ou, sem elas, o tempo que o técnico já apontou na OS. Sem horas ou
/// sem custo-hora, ela entra na receita mas fica **fora das margens** (`itens_sem_custo`) —
/// contá-la como lucro de 100% inflaria a margem.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MargemPrevistaDaOrdem {
    /// A ordem de serviço.
    pub ordem_servico: Id,
}

/// O que [`MargemPrevistaDaOrdem`] devolve.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MargemPrevista {
    /// O painel (peças com custo; mão de obra fora das margens).
    pub painel: mod_estoque::precificacao::PainelDeMargem,
    /// A margem mínima configurada.
    pub margem_minima: cardeal_kernel::Percentual,
    /// A margem líquida das peças está abaixo da mínima.
    pub abaixo_do_minimo: bool,
    /// Alguma peça ainda não aplicada teve o custo estimado pelo custo médio atual.
    pub custo_estimado: bool,
}

impl Consulta for MargemPrevistaDaOrdem {
    type Saida = MargemPrevista;
    const PERMISSAO: &'static str = "os.margem.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use cardeal_kernel::{Arredondamento, Dinheiro, Percentual};
        use mod_estoque::precificacao::{painel_de_margem, ItemDeMargem};

        let pecas = itens_peca_da_ordem(conexao, self.ordem_servico)?;
        let maos = super::itens_mao_de_obra_da_ordem(conexao, self.ordem_servico)?;
        let ids: Vec<Id> = pecas.iter().map(|p| p.produto).collect();
        let sugestoes = mod_estoque::sugerir_precos(conexao, ctx, None, Some(&ids))?;
        let encargos = mod_estoque::encargos_da_empresa(conexao, ctx)?;

        let mut custo_estimado = false;
        let mut itens: Vec<ItemDeMargem> = Vec::new();
        for p in pecas.iter().filter(|p| !p.estornada) {
            let atual = sugestoes
                .iter()
                .find(|s| s.produto == p.produto)
                .map(|s| (s.nome.clone(), s.custo, s.imposto, s.comissao));
            let custo_unitario = if p.aplicada {
                Some(Dinheiro::de_total(
                    cardeal_kernel::Quantidade::unidades(1),
                    p.custo_unitario,
                    Arredondamento::MeioAcima,
                ))
            } else {
                // O combinado com o fornecedor na encomenda vale mais que o custo médio de hoje.
                let combinado = p
                    .encomenda
                    .as_ref()
                    .and_then(|e| e.custo_previsto)
                    .map(|c| {
                        Dinheiro::de_total(
                            cardeal_kernel::Quantidade::unidades(1),
                            c,
                            Arredondamento::MeioAcima,
                        )
                    });
                let c = combinado.or_else(|| atual.as_ref().and_then(|(_, c, _, _)| *c));
                custo_estimado |= c.is_some();
                c
            };
            let receita = if p.coberto_garantia {
                Dinheiro::ZERO
            } else {
                Dinheiro::de_total(p.quantidade, p.preco_unitario, Arredondamento::MeioAcima)
            };
            // Imposto e comissão são os do produto; operação e taxa são da empresa.
            let encargos_da_peca =
                atual
                    .as_ref()
                    .map_or(encargos.total(), |(_, _, imposto, comissao)| {
                        encargos.total() - encargos.imposto - encargos.comissao
                            + *imposto
                            + *comissao
                    });
            itens.push(ItemDeMargem {
                rotulo: atual.map_or_else(|| "Peça".to_owned(), |(n, _, _, _)| n),
                quantidade: p.quantidade,
                receita,
                custo_unitario,
                encargos: encargos_da_peca,
            });
        }
        let tabela = tabela_custo_hora(conexao, ctx.empresa)?;
        let apontado = segundos_por_tecnico(conexao, self.ordem_servico)?;
        for m in &maos {
            let horas = m.horas.filter(|h| h.e_positiva()).or_else(|| {
                apontado
                    .get(&m.tecnico)
                    .map(|s| crate::custo_hora::horas_de(*s))
                    .filter(|h| h.e_positiva())
            });
            let item = match (horas, tabela.do_tecnico(m.tecnico)) {
                (Some(horas), Some(custo_hora)) => ItemDeMargem {
                    rotulo: m.descricao.clone(),
                    quantidade: horas,
                    receita: m.valor,
                    custo_unitario: Some(custo_hora),
                    encargos: encargos.total(),
                },
                _ => ItemDeMargem {
                    rotulo: m.descricao.clone(),
                    quantidade: cardeal_kernel::Quantidade::unidades(1),
                    receita: m.valor,
                    custo_unitario: None,
                    encargos: encargos.total(),
                },
            };
            itens.push(item);
        }
        let painel = painel_de_margem(&itens);
        let abaixo = painel
            .margem_liquida_pct
            .is_some_and(|p: Percentual| p < encargos.margem_minima);
        Ok(MargemPrevista {
            painel,
            margem_minima: encargos.margem_minima,
            abaixo_do_minimo: abaixo,
            custo_estimado,
        })
    }
}
