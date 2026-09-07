//! As consultas do financeiro: leitura autorizada sobre `financeiro_titulo`/
//! `financeiro_parcela`, sem passar por SQL cru na tela — `docs/modulos/financeiro.md` §6/§10
//! (Contas a Receber/Pagar) e §6 do doc 12 (o Pulso usa a mesma consulta para "a receber
//! hoje"/"exige ação hoje").

use std::collections::HashMap;

use cardeal_kernel::{Competencia, Data, Dinheiro, Id, Periodo, Resultado};
use cardeal_ledger::Contraparte;
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::categoria::CategoriaFinanceira;
use crate::recorrencia::Recorrencia;
use crate::repositorio::{
    blob, categoria_de_linha, contraparte_join, data_de, especie_de, especie_txt, estado_de, id_de,
    persist, recorrencia_de_linha,
};
use crate::titulo::{EspecieTitulo, EstadoParcela};

/// Uma parcela em aberto, já com o suficiente para a grade de Contas a Receber/Pagar sem
/// consulta adicional: vencimento, contraparte, valores e estado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemTituloEmAberto {
    /// A parcela.
    pub parcela: Id,
    /// O título dono.
    pub titulo: Id,
    /// Ordinal da parcela dentro do título.
    pub numero: u16,
    /// A receber ou a pagar.
    pub especie: EspecieTitulo,
    /// Com quem.
    pub contraparte: Contraparte,
    /// Quando vence.
    pub vencimento: Data,
    /// O valor original da parcela.
    pub valor_original: Dinheiro,
    /// Quanto já foi baixado do principal.
    pub valor_baixado: Dinheiro,
    /// O estado atual (`Aberta`/`Parcial` — nunca outro, a consulta já filtra).
    pub estado: EstadoParcela,
}

impl ItemTituloEmAberto {
    /// O saldo ainda em aberto (`valor_original - valor_baixado`).
    #[must_use]
    pub fn saldo(&self) -> Dinheiro {
        self.valor_original - self.valor_baixado
    }
}

/// As parcelas em aberto (`Aberta`/`Parcial`) de uma espécie, por vencimento. Sem cursor real
/// ainda (`docs/09-protocolo-api.md` §5 previa `Pagina<T>`) — um teto de 500 linhas é
/// suficiente para o volume de uma PME e evita construir paginação por chave antes de haver
/// um caso de uso que precise dela.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn titulos_em_aberto(
    conexao: &Connection,
    empresa: Id,
    especie: EspecieTitulo,
) -> Resultado<Vec<ItemTituloEmAberto>> {
    let mut stmt = conexao
        .prepare(
            "SELECT p.id, p.titulo, p.numero, t.contraparte_tipo, t.contraparte_id,
                    p.vencimento, p.valor, p.valor_baixado, p.estado
             FROM financeiro_parcela p
             JOIN financeiro_titulo t ON t.id = p.titulo
             WHERE t.empresa = ?1 AND t.especie = ?2 AND p.estado IN ('Aberta','Parcial')
             ORDER BY p.vencimento ASC
             LIMIT 500",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(params![blob(empresa), especie_txt(especie)], |r| {
            let contraparte = contraparte_join(r.get::<_, String>(3)?.as_str(), r.get(4)?);
            Ok(ItemTituloEmAberto {
                parcela: id_de(r.get(0)?),
                titulo: id_de(r.get(1)?),
                numero: u16::try_from(r.get::<_, i64>(2)?).unwrap_or(1),
                especie,
                contraparte,
                vencimento: data_de(r.get(5)?),
                valor_original: Dinheiro::centavos(r.get(6)?),
                valor_baixado: Dinheiro::centavos(r.get(7)?),
                estado: estado_de(&r.get::<_, String>(8)?),
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Lista as parcelas a receber em aberto (`Aberta`/`Parcial`), por vencimento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitulosAReceberEmAberto;

impl Consulta for TitulosAReceberEmAberto {
    type Saida = Vec<ItemTituloEmAberto>;
    const PERMISSAO: &'static str = "financeiro.receber.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        titulos_em_aberto(conexao, ctx.empresa, EspecieTitulo::Receber)
    }
}

/// Lista as parcelas a pagar em aberto (`Aberta`/`Parcial`), por vencimento.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TitulosAPagarEmAberto;

impl Consulta for TitulosAPagarEmAberto {
    type Saida = Vec<ItemTituloEmAberto>;
    const PERMISSAO: &'static str = "financeiro.pagar.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        titulos_em_aberto(conexao, ctx.empresa, EspecieTitulo::Pagar)
    }
}

/// As categorias ativas de uma empresa, por nome.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn categorias_ativas(conexao: &Connection, empresa: Id) -> Resultado<Vec<CategoriaFinanceira>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, empresa, nome, especie, ativa
             FROM financeiro_categoria WHERE empresa = ?1 AND ativa = 1 ORDER BY nome",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], categoria_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Lista as categorias financeiras ativas — alimenta o seletor de categoria na tela de
/// lançamento (`docs/modulos/financeiro.md` §11.9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Categorias;

impl Consulta for Categorias {
    type Saida = Vec<CategoriaFinanceira>;
    const PERMISSAO: &'static str = "financeiro.categoria.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        categorias_ativas(conexao, ctx.empresa)
    }
}

/// Um total agregado por categoria, espécie e mês — a matéria-prima dos gráficos de custo/
/// receita recorrente (`docs/modulos/financeiro.md` §11.9): quanto cada categoria de custo
/// pesou, quanto cada categoria de receita (projeto/cliente de `SaaS`) rendeu, mês a mês. Somar
/// consecutivos e comparar (para churn, variação mês a mês) fica para a camada de análise —
/// esta consulta só entrega os fatos já agrupados, em memória, sem SQL de agregação.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemTotalPorCategoria {
    /// A categoria — `None` agrupa tudo que não foi categorizado.
    pub categoria: Option<Id>,
    /// A receber (receita) ou a pagar (custo).
    pub especie: EspecieTitulo,
    /// O mês de referência (mês da baixa, não da emissão — é dinheiro que efetivamente
    /// entrou ou saiu).
    pub competencia: Competencia,
    /// A soma do que foi efetivamente baixado (recebido/pago) neste mês, nesta categoria.
    pub total_baixado: Dinheiro,
}

/// Totais por categoria/espécie/mês, agregados sobre as baixas (não estornadas) cuja `data`
/// cai dentro de `periodo`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TotalPorCategoriaNoPeriodo {
    /// A janela de datas de baixa a considerar.
    pub periodo: Periodo,
}

impl Consulta for TotalPorCategoriaNoPeriodo {
    type Saida = Vec<ItemTotalPorCategoria>;
    const PERMISSAO: &'static str = "financeiro.categoria.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT t.categoria, t.especie, b.data, b.valor_recebido
                 FROM financeiro_baixa b
                 JOIN financeiro_parcela p ON p.id = b.parcela
                 JOIN financeiro_titulo t ON t.id = p.titulo
                 WHERE t.empresa = ?1 AND b.estornada_em IS NULL
                       AND b.data BETWEEN ?2 AND ?3",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                params![
                    blob(ctx.empresa),
                    i64::from(self.periodo.de.em_dias()),
                    i64::from(self.periodo.ate.em_dias()),
                ],
                |r| {
                    let categoria = r.get::<_, Option<Vec<u8>>>(0)?.map(id_de);
                    let especie = especie_de(&r.get::<_, String>(1)?);
                    let data = data_de(r.get::<_, i64>(2)?);
                    let valor = Dinheiro::centavos(r.get::<_, i64>(3)?);
                    Ok((categoria, especie, data, valor))
                },
            )
            .map_err(persist)?;

        let mut totais: HashMap<(Option<Id>, EspecieTitulo, Competencia), Dinheiro> =
            HashMap::new();
        for linha in linhas {
            let (categoria, especie, data, valor) = linha.map_err(persist)?;
            *totais
                .entry((categoria, especie, data.competencia()))
                .or_insert(Dinheiro::ZERO) += valor;
        }

        Ok(totais
            .into_iter()
            .map(
                |((categoria, especie, competencia), total_baixado)| ItemTotalPorCategoria {
                    categoria,
                    especie,
                    competencia,
                    total_baixado,
                },
            )
            .collect())
    }
}

/// Uma conta de resultado (Receita ou Despesa) analítica do plano — o que pode ser escolhido
/// como `conta_contrapartida` de uma recorrência (`docs/modulos/financeiro.md` §11.7).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemContaResultado {
    /// A conta do Razão.
    pub conta: Id,
    /// Código contábil ("3.1.01").
    pub codigo: String,
    /// Nome legível ("Aluguéis e condomínios").
    pub nome: String,
}

/// Lista as contas de resultado analíticas ativas da espécie pedida — Receitas para uma
/// recorrência a receber, Despesas para uma a pagar. Alimenta o seletor de
/// `conta_contrapartida` no formulário de recorrência; não há entidade de "plano de contas"
/// exposta, então a consulta lê `razao_conta` direto (mesma base — `mod-financeiro` depende
/// de `razao`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContasDeResultado {
    /// `Receber` devolve as contas de Receita; `Pagar`, as de Despesa.
    pub especie: EspecieTitulo,
}

impl Consulta for ContasDeResultado {
    type Saida = Vec<ItemContaResultado>;
    const PERMISSAO: &'static str = "financeiro.recorrencia.criar";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let natureza = match self.especie {
            EspecieTitulo::Receber => "Receita",
            EspecieTitulo::Pagar => "Despesa",
        };
        let mut stmt = conexao
            .prepare(
                "SELECT id, codigo, nome
                 FROM razao_conta
                 WHERE empresa = ?1 AND natureza = ?2 AND tipo = 'Analitica' AND ativa = 1
                 ORDER BY codigo ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(params![blob(ctx.empresa), natureza], |r| {
                Ok(ItemContaResultado {
                    conta: id_de(r.get(0)?),
                    codigo: r.get(1)?,
                    nome: r.get(2)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Lista as regras de recorrência da empresa — ativas e inativas — para a tela de gestão
/// (`docs/modulos/financeiro.md` §11.7). Ordena por descrição.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recorrencias;

impl Consulta for Recorrencias {
    type Saida = Vec<Recorrencia>;
    const PERMISSAO: &'static str = "financeiro.recorrencia.criar";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, empresa, descricao, especie, contraparte_tipo, contraparte_id,
                        tipo_valor, valor_fixo, indice, media_ultimos_n, periodicidade,
                        dia_referencia, expressao_cron, inicio, fim, conta_contrapartida,
                        centro_custo, categoria, antecedencia_geracao_dias, ativa, versao
                 FROM financeiro_recorrencia WHERE empresa = ?1 ORDER BY descricao ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], recorrencia_de_linha)
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Um caixa físico da empresa, com a sessão aberta (se houver) — a entrada da tela de PDV.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemCaixa {
    /// O caixa.
    pub caixa: Id,
    /// Nome exibido ("Caixa 1", "Cofre").
    pub nome: String,
    /// A conta do Razão vinculada.
    pub conta_razao: Id,
    /// A sessão em aberto, quando existe — o `sessao_caixa` que o PDV precisa.
    pub sessao_aberta: Option<Id>,
}

/// Lista os caixas ativos da empresa e, para cada um, a sessão aberta (no máximo uma).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Caixas;

impl Consulta for Caixas {
    type Saida = Vec<ItemCaixa>;
    const PERMISSAO: &'static str = "financeiro.caixa.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT c.id, c.nome, c.conta_razao,
                        (SELECT s.id FROM financeiro_sessao_caixa s
                         WHERE s.caixa = c.id AND s.estado = 'Aberta' LIMIT 1)
                 FROM financeiro_caixa c
                 WHERE c.empresa = ?1 AND c.ativo = 1
                 ORDER BY c.nome ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemCaixa {
                    caixa: id_de(r.get(0)?),
                    nome: r.get(1)?,
                    conta_razao: id_de(r.get(2)?),
                    sessao_aberta: r.get::<_, Option<Vec<u8>>>(3)?.map(id_de),
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Contas analíticas ativas elegíveis como conta do Razão de um caixa (papel `Caixa` ou
/// código no grupo `1.1`). Alimenta o cadastro de caixa da tela de PDV.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContasDeCaixa;

impl Consulta for ContasDeCaixa {
    type Saida = Vec<ItemContaResultado>;
    const PERMISSAO: &'static str = "financeiro.caixa.cadastrar";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, codigo, nome
                 FROM razao_conta
                 WHERE empresa = ?1 AND tipo = 'Analitica' AND ativa = 1
                       AND (papel_padrao = 'Caixa' OR codigo LIKE '1.1%')
                 ORDER BY codigo ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(ItemContaResultado {
                    conta: id_de(r.get(0)?),
                    codigo: r.get(1)?,
                    nome: r.get(2)?,
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

// ─── Fluxo de caixa e contas bancárias ───────────────────────────────────────
//
// Contas a Receber/Pagar são obrigações **futuras** ainda não liquidadas. Fluxo de caixa e
// extrato bancário são o oposto: o registro **realizado** do que entrou e saiu, direto do
// Razão (`razao_partida` sobre as contas do grupo "1.1 Disponível", só lançamentos
// `Realizado`). Livro de auditoria — nunca projeção.

/// Uma conta do grupo "Disponível" (caixa ou banco) com o saldo realizado.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemContaDisponivel {
    /// A conta do Razão.
    pub conta: Id,
    /// Código contábil ("1.1.01", "1.1.05").
    pub codigo: String,
    /// Nome ("Caixa", "Nubank — cc 1234").
    pub nome: String,
    /// Saldo realizado (soma das partidas de lançamentos `Realizado`/`Estornado`).
    pub saldo: Dinheiro,
    /// Verdadeiro se é a conta de caixa físico (papel `Caixa`), falso para bancos.
    pub e_caixa: bool,
}

/// Lista as contas do grupo "1.1 Disponível" (analíticas, ativas) com saldo realizado —
/// alimenta a aba "Contas bancárias" e o seletor do extrato.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContasDisponiveis;

impl Consulta for ContasDisponiveis {
    type Saida = Vec<ItemContaDisponivel>;
    const PERMISSAO: &'static str = "financeiro.banco.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT rc.id, rc.codigo, rc.nome, rc.papel_padrao,
                        COALESCE((
                            SELECT SUM(rp.valor) FROM razao_partida rp
                            JOIN razao_lancamento rl ON rl.id = rp.lancamento
                            WHERE rp.conta = rc.id AND rl.estado IN ('Realizado','Estornado')
                        ), 0)
                 FROM razao_conta rc
                 WHERE rc.empresa = ?1 AND rc.tipo = 'Analitica' AND rc.ativa = 1
                       AND rc.codigo LIKE '1.1.%'
                 ORDER BY rc.codigo ASC",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                let papel: Option<String> = r.get(3)?;
                Ok(ItemContaDisponivel {
                    conta: id_de(r.get(0)?),
                    codigo: r.get(1)?,
                    nome: r.get(2)?,
                    e_caixa: papel.as_deref() == Some("Caixa"),
                    saldo: Dinheiro::centavos(r.get::<_, i64>(4)?),
                })
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// Um movimento realizado de uma conta do Disponível.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemMovimentoDisponivel {
    /// Data da liquidação.
    pub data: Data,
    /// Histórico do lançamento.
    pub historico: String,
    /// A conta movimentada.
    pub conta_nome: String,
    /// O valor com sinal: entrada positiva, saída negativa.
    pub valor: Dinheiro,
}

/// O extrato realizado (livro-caixa) de todas as contas do Disponível, ou de uma só, num
/// período. Só lançamentos `Realizado` — o que efetivamente entrou/saiu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtratoDisponivel {
    /// A janela de datas de liquidação.
    pub periodo: Periodo,
    /// Quando `Some`, restringe a essa conta.
    pub conta: Option<Id>,
}

impl Consulta for ExtratoDisponivel {
    type Saida = Vec<ItemMovimentoDisponivel>;
    const PERMISSAO: &'static str = "financeiro.banco.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT rl.liquidacao, rl.historico, rc.nome, rp.valor
                 FROM razao_partida rp
                 JOIN razao_lancamento rl ON rl.id = rp.lancamento
                 JOIN razao_conta rc ON rc.id = rp.conta
                 WHERE rc.empresa = ?1 AND rc.codigo LIKE '1.1.%'
                       AND rl.estado = 'Realizado' AND rl.liquidacao IS NOT NULL
                       AND rl.liquidacao BETWEEN ?2 AND ?3
                       AND (?4 IS NULL OR rp.conta = ?4)
                 ORDER BY rl.liquidacao ASC, rl.numero ASC
                 LIMIT 1000",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                params![
                    blob(ctx.empresa),
                    i64::from(self.periodo.de.em_dias()),
                    i64::from(self.periodo.ate.em_dias()),
                    self.conta.map(blob),
                ],
                |r| {
                    Ok(ItemMovimentoDisponivel {
                        data: data_de(r.get::<_, i64>(0)?),
                        historico: r.get(1)?,
                        conta_nome: r.get(2)?,
                        valor: Dinheiro::centavos(r.get::<_, i64>(3)?),
                    })
                },
            )
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}
