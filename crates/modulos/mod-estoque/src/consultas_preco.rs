//! As consultas de formação de preço: os parâmetros da empresa e o preço sugerido de um
//! produto (ou de todos os de um grupo), com a decomposição de onde cada percentual veio.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Preco;
use cardeal_kernel::{Dinheiro, Id, Percentual};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::parametros_preco::ParametrosPreco;
use crate::precificacao::{Componentes, PerfilFiscal, PrecoCalculado};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Periodo, Resultado};

/// De onde veio a margem usada no cálculo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrigemMargem {
    /// Definida no próprio produto.
    Produto,
    /// Herdada de um grupo (o nome do grupo que a definiu).
    Grupo(String),
    /// A margem padrão da empresa.
    Padrao,
    /// Ninguém definiu.
    Nenhuma,
}

/// Os parâmetros de preço da empresa da sessão.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ParametrosDePreco;

impl Consulta for ParametrosDePreco {
    type Saida = ParametrosPreco;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        crate::repositorio_preco::parametros_preco(conexao, ctx.empresa)
    }
}

/// O preço de um produto, decomposto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SugestaoDePreco {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// O grupo.
    pub grupo: Id,
    /// O custo usado (informado ou o maior custo médio em estoque); `None` sem custo.
    pub custo: Option<Dinheiro>,
    /// A margem usada.
    pub margem: Option<Percentual>,
    /// De onde veio a margem.
    pub origem_margem: OrigemMargem,
    /// O perfil fiscal herdado.
    pub perfil_fiscal: PerfilFiscal,
    /// Faturamento dos últimos 12 meses (base da faixa do Simples).
    pub rbt12: Dinheiro,
    /// O custo operacional aplicado.
    pub custo_operacional: Percentual,
    /// `true` se o custo operacional veio do Razão; `false` se foi o informado.
    pub custo_operacional_derivado: bool,
    /// O imposto sobre a venda aplicado.
    pub imposto: Percentual,
    /// Os percentuais usados, quando todos estavam disponíveis.
    pub componentes: Option<Componentes>,
    /// O preço sugerido.
    pub sugerido: Option<PrecoCalculado>,
    /// O preço de empate (sem margem).
    pub minimo: Option<PrecoCalculado>,
    /// Por que não foi possível calcular, em português para o usuário.
    pub problema: Option<String>,
}

/// O preço sugerido de um produto. `custo` opcional recalcula com outro custo (por exemplo o
/// preço cotado numa compra); sem ele vale o maior custo médio em estoque.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PrecoSugeridoDoProduto {
    /// O produto.
    pub produto: Id,
    /// O custo a usar no lugar do custo médio.
    pub custo: Option<Dinheiro>,
}

impl Consulta for PrecoSugeridoDoProduto {
    type Saida = SugestaoDePreco;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let base = BaseDePreco::carregar(conexao, ctx)?;
        let linha = linha_do_produto(conexao, ctx, Some(self.produto), None)?
            .into_iter()
            .next()
            .ok_or_else(|| Erro::nao_encontrado("produto"))?;
        Ok(base.sugerir(&linha, self.custo))
    }
}

/// O preço sugerido de todos os produtos ativos de um grupo e de seus descendentes —
/// a base da revisão de preços em lote.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PrecosSugeridosDoGrupo {
    /// O grupo (a subárvore inteira entra); `None` = o catálogo todo.
    pub grupo: Option<Id>,
}

impl Consulta for PrecosSugeridosDoGrupo {
    type Saida = Vec<SugestaoDePreco>;
    const PERMISSAO: &'static str = "estoque.produto.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        sugerir_precos(conexao, ctx, self.grupo, None)
    }
}

/// O preço sugerido dos produtos ativos do recorte: a subárvore de `grupo` (ou o catálogo
/// todo) e, se informado, só os `produtos` listados. É a porta pública que outro módulo chama
/// (`vendas.AplicarPrecosSugeridos`) sem ler as tabelas de estoque.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite ou do Razão.
#[cfg(feature = "sqlite")]
pub fn sugerir_precos(
    conexao: &Connection,
    ctx: &Ctx,
    grupo: Option<Id>,
    produtos: Option<&[Id]>,
) -> Resultado<Vec<SugestaoDePreco>> {
    let base = BaseDePreco::carregar(conexao, ctx)?;
    let grupos = grupo.map(|g| {
        let mut todos = base.arvore.descendentes(g);
        todos.push(g);
        todos
    });
    let linhas = linha_do_produto(conexao, ctx, None, grupos.as_deref())?;
    Ok(linhas
        .iter()
        .filter(|l| produtos.is_none_or(|p| p.contains(&l.id)))
        .map(|l| base.sugerir(l, None))
        .collect())
}

#[cfg(feature = "sqlite")]
struct LinhaProduto {
    id: Id,
    nome: String,
    grupo: Id,
    margem: Option<Percentual>,
    custo: Preco,
}

#[cfg(feature = "sqlite")]
fn linha_do_produto(
    conexao: &Connection,
    ctx: &Ctx,
    produto: Option<Id>,
    grupos: Option<&[Id]>,
) -> Resultado<Vec<LinhaProduto>> {
    use crate::repositorio::{blob, id_de, persist};
    let mut stmt = conexao
        .prepare(
            "SELECT p.id, p.nome, p.grupo_produto, p.margem_desejada,
                    COALESCE((SELECT MAX(s.custo_medio) FROM estoque_saldo_local s
                              WHERE s.produto = p.id AND s.empresa = ?2), 0)
             FROM estoque_produto p
             WHERE p.empresa = ?1 AND p.ativo = 1
             ORDER BY p.nome ASC, p.id ASC",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(
            rusqlite::params![blob(ctx.organizacao), blob(ctx.empresa)],
            |r| {
                Ok(LinhaProduto {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    nome: r.get(1)?,
                    grupo: id_de(r.get::<_, Vec<u8>>(2)?),
                    margem: r.get::<_, Option<i64>>(3)?.map(Percentual::unidades),
                    custo: Preco::interna(r.get::<_, i64>(4)?),
                })
            },
        )
        .map_err(persist)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)?;
    Ok(linhas
        .into_iter()
        .filter(|l| produto.is_none_or(|p| p == l.id))
        .filter(|l| grupos.is_none_or(|g| g.contains(&l.grupo)))
        .collect())
}

/// Tudo que não muda de um produto para outro — carregado uma vez por consulta.
#[cfg(feature = "sqlite")]
struct BaseDePreco {
    parametros: ParametrosPreco,
    perfis: Vec<crate::repositorio_preco::PerfilDoGrupo>,
    arvore: crate::hierarquia::Arvore,
    rbt12: Dinheiro,
    opex_do_razao: Option<Percentual>,
}

#[cfg(feature = "sqlite")]
impl BaseDePreco {
    fn carregar(conexao: &Connection, ctx: &Ctx) -> Resultado<Self> {
        use crate::hierarquia::{Arvore, NoGrupo};
        use crate::repositorio_preco as sql;

        let parametros = sql::parametros_preco(conexao, ctx.empresa)?;
        let perfis = sql::perfis_dos_grupos(conexao, ctx.organizacao)?;
        let arvore = Arvore::de_nos(
            &perfis
                .iter()
                .map(|p| NoGrupo {
                    id: p.id,
                    pai: p.pai,
                })
                .collect::<Vec<_>>(),
        );
        let hoje = ctx.hoje();
        let janela = Periodo::novo(hoje.mais_meses(-12).mais_dias(1), hoje);
        let linhas = cardeal_ledger::resultado_por_conta(conexao, ctx.empresa, janela)
            .map_err(|e| Erro::novo(CodigoErro::FALHA_INTERNA, format!("estoque/preço: {e}")))?;
        let resumo = cardeal_ledger::resumir(&linhas);
        Ok(Self {
            parametros,
            perfis,
            arvore,
            rbt12: resumo.receita_bruta,
            opex_do_razao: resumo.custo_operacional_percentual(),
        })
    }

    fn perfil(&self, id: Id) -> Option<&crate::repositorio_preco::PerfilDoGrupo> {
        self.perfis.iter().find(|p| p.id == id)
    }

    /// A margem do produto, herdando do grupo mais próximo e, no fim, do padrão.
    fn margem(&self, produto: Option<Percentual>, grupo: Id) -> (Option<Percentual>, OrigemMargem) {
        if let Some(m) = produto {
            return (Some(m), OrigemMargem::Produto);
        }
        for g in self.arvore.ancestrais(grupo) {
            if let Some(p) = self.perfil(g) {
                if let Some(m) = p.margem {
                    return (Some(m), OrigemMargem::Grupo(p.nome.clone()));
                }
            }
        }
        match self.parametros.margem_padrao {
            Some(m) => (Some(m), OrigemMargem::Padrao),
            None => (None, OrigemMargem::Nenhuma),
        }
    }

    fn perfil_fiscal(&self, grupo: Id) -> PerfilFiscal {
        let mut perfil = PerfilFiscal::default();
        let (mut st, mut mono) = (None, None);
        for g in self.arvore.ancestrais(grupo) {
            if let Some(p) = self.perfil(g) {
                st = st.or(p.icms_st);
                mono = mono.or(p.monofasico);
            }
        }
        perfil.icms_st = st.unwrap_or(false);
        perfil.monofasico = mono.unwrap_or(false);
        perfil
    }

    fn sugerir(&self, l: &LinhaProduto, custo_informado: Option<Dinheiro>) -> SugestaoDePreco {
        use crate::precificacao::{carga_sobre_venda, preco_minimo, preco_sugerido};
        use cardeal_kernel::{Arredondamento, Quantidade};

        let custo = custo_informado.or_else(|| {
            let c = Dinheiro::de_total(Quantidade::unidades(1), l.custo, Arredondamento::MeioAcima);
            c.e_positivo().then_some(c)
        });
        let (margem, origem_margem) = self.margem(l.margem, l.grupo);
        let perfil_fiscal = self.perfil_fiscal(l.grupo);
        let imposto = carga_sobre_venda(self.parametros.regime, self.rbt12, perfil_fiscal);
        let (opex, derivado) = match (
            self.parametros.custo_operacional_derivado,
            self.opex_do_razao,
        ) {
            (true, Some(p)) => (p, true),
            _ => (self.parametros.custo_operacional, false),
        };
        let mut s = SugestaoDePreco {
            produto: l.id,
            nome: l.nome.clone(),
            grupo: l.grupo,
            custo,
            margem,
            origem_margem,
            perfil_fiscal,
            rbt12: self.rbt12,
            custo_operacional: opex,
            custo_operacional_derivado: derivado,
            imposto,
            componentes: None,
            sugerido: None,
            minimo: None,
            problema: None,
        };
        let Some(custo) = custo else {
            s.problema = Some(
                "o produto ainda não tem custo: dê entrada no estoque ou informe o custo".into(),
            );
            return s;
        };
        let Some(margem) = margem else {
            s.problema = Some(
                "defina a margem desejada do produto, do grupo ou a margem padrão da empresa"
                    .into(),
            );
            return s;
        };
        let componentes = Componentes {
            custo_operacional: opex,
            margem_liquida: margem,
            imposto,
            comissao: self.parametros.comissao,
            taxa_pagamento: self.parametros.taxa_pagamento,
        };
        s.componentes = Some(componentes);
        match (
            preco_sugerido(custo, &componentes),
            preco_minimo(custo, &componentes),
        ) {
            (Ok(a), Ok(b)) => {
                s.sugerido = Some(a);
                s.minimo = Some(b);
            }
            (Err(e), _) | (_, Err(e)) => s.problema = Some(e.to_string()),
        }
        s
    }
}
