//! O plano de contas visto pelo financeiro: a árvore para a tela, contas novas (até 5 níveis),
//! os modelos por segmento e o vínculo **categoria → conta** que faz um lançamento cair na conta
//! certa (e a DRE separar aluguel de energia de pró-labore).

use cardeal_kernel::Id;
use cardeal_ledger::ModeloDePlano;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
use cardeal_modkit::{Comando, Consulta};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
#[cfg(feature = "sqlite")]
use rusqlite::{params, Connection, OptionalExtension};

#[cfg(feature = "sqlite")]
use crate::repositorio::{blob, id_de, persist};

/// O máximo de níveis do plano.
pub const NIVEL_MAXIMO: u8 = 5;

/// Uma conta do plano para a árvore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoPlano {
    /// A conta.
    pub id: Id,
    /// O código hierárquico.
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// `Ativo`, `Passivo`, `PatrimonioLiquido`, `Receita` ou `Despesa`.
    pub natureza: String,
    /// Agrupadora (recebe filhas) ou analítica (recebe lançamento).
    pub sintetica: bool,
    /// O nível (1 = raiz).
    pub nivel: u8,
    /// O pai.
    pub pai: Option<Id>,
    /// O papel semântico gravado na conta, se houver.
    pub papel: Option<String>,
    /// Ativa.
    pub ativa: bool,
}

#[cfg(feature = "sqlite")]
fn chave_de_codigo(c: &str) -> Vec<u32> {
    c.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// O plano inteiro, em ordem de código (pré-ordem natural, `1`, `1.1`, `1.1.01`, `1.2`, …).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PlanoDeContas;

impl Consulta for PlanoDeContas {
    type Saida = Vec<NoPlano>;
    const PERMISSAO: &'static str = "financeiro.plano.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT id, codigo, nome, natureza, tipo, nivel, pai, papel_padrao, ativa
                 FROM razao_conta WHERE empresa = ?1",
            )
            .map_err(persist)?;
        let mut nos = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok(NoPlano {
                    id: id_de(r.get::<_, Vec<u8>>(0)?),
                    codigo: r.get(1)?,
                    nome: r.get(2)?,
                    natureza: r.get(3)?,
                    sintetica: r.get::<_, String>(4)? == "Sintetica",
                    nivel: u8::try_from(r.get::<_, i64>(5)?).unwrap_or(1),
                    pai: r.get::<_, Option<Vec<u8>>>(6)?.map(id_de),
                    papel: r.get(7)?,
                    ativa: r.get::<_, i64>(8)? != 0,
                })
            })
            .map_err(persist)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)?;
        nos.sort_by_key(|n| chave_de_codigo(&n.codigo));
        Ok(nos)
    }
}

/// Quais categorias lançam em qual conta: pares `(conta, categoria)`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CategoriasPorConta;

impl Consulta for CategoriasPorConta {
    type Saida = Vec<(Id, Id)>;
    const PERMISSAO: &'static str = "financeiro.plano.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let mut stmt = conexao
            .prepare(
                "SELECT conta, id FROM financeiro_categoria
                 WHERE empresa = ?1 AND ativa = 1 AND conta IS NOT NULL",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(ctx.empresa)], |r| {
                Ok((
                    id_de(r.get::<_, Vec<u8>>(0)?),
                    id_de(r.get::<_, Vec<u8>>(1)?),
                ))
            })
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}

/// A conta onde a categoria lança, se ela está vinculada a uma conta analítica, ativa e da
/// natureza certa para a espécie (`"Despesa"` para pagar, `"Receita"` para receber).
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
#[cfg(feature = "sqlite")]
pub fn conta_da_categoria(
    conexao: &Connection,
    empresa: Id,
    categoria: Id,
    natureza: &str,
) -> Resultado<Option<Id>> {
    conexao
        .query_row(
            "SELECT r.id FROM financeiro_categoria c JOIN razao_conta r ON r.id = c.conta
             WHERE c.id = ?1 AND c.empresa = ?2 AND r.tipo = 'Analitica' AND r.ativa = 1
               AND r.natureza = ?3",
            params![blob(categoria), blob(empresa), natureza],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .optional()
        .map(|o| o.map(id_de))
        .map_err(persist)
}

/// Liga cada categoria sem conta à conta analítica de mesmo nome (sem diferenciar maiúsculas),
/// respeitando a natureza da espécie. Devolve quantas ligou.
///
/// # Errors
/// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
#[cfg(feature = "sqlite")]
pub fn vincular_categorias_por_nome(conexao: &Connection, empresa: Id) -> Resultado<u32> {
    let mut stmt = conexao
        .prepare(
            "SELECT c.id, r.id FROM financeiro_categoria c
             JOIN razao_conta r ON lower(r.nome) = lower(c.nome)
             WHERE c.empresa = ?1 AND r.empresa = ?1 AND c.ativa = 1 AND c.conta IS NULL
               AND r.tipo = 'Analitica' AND r.ativa = 1 AND r.natureza IN ('Receita','Despesa')
               AND (c.especie IS NULL
                    OR (c.especie = 'Pagar' AND r.natureza = 'Despesa')
                    OR (c.especie = 'Receber' AND r.natureza = 'Receita'))",
        )
        .map_err(persist)?;
    let pares: Vec<(Vec<u8>, Vec<u8>)> = stmt
        .query_map([blob(empresa)], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(persist)?
        .collect::<rusqlite::Result<_>>()
        .map_err(persist)?;
    let mut ligadas = 0;
    for (cat, conta) in pares {
        // Uma categoria pode casar com mais de uma conta homônima: a primeira vence.
        ligadas += u32::try_from(
            conexao
                .execute(
                    "UPDATE financeiro_categoria SET conta = ?2 WHERE id = ?1 AND conta IS NULL",
                    params![cat, conta],
                )
                .map_err(persist)?,
        )
        .unwrap_or(0);
    }
    Ok(ligadas)
}

/// Cria uma conta nova no plano, filha de uma agrupadora.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarContaDoPlano {
    /// A conta pai (precisa ser agrupadora e ter menos de 5 níveis).
    pub pai: Id,
    /// O nome.
    pub nome: String,
    /// `true` cria uma agrupadora (recebe filhas); `false`, uma conta que recebe lançamento.
    pub agrupadora: bool,
}

/// A conta criada.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContaDoPlanoCriada {
    /// A conta.
    pub conta: Id,
    /// O código que recebeu.
    pub codigo: String,
}

impl Comando for CriarContaDoPlano {
    type Saida = ContaDoPlanoCriada;
    const PERMISSAO: &'static str = "financeiro.plano.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        use cardeal_ledger::{CodigoConta, Conta, Natureza, PortaRazao, RepositorioRazao};

        let nome = self.nome.trim().to_owned();
        if nome.is_empty() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "dê um nome para a conta",
            ));
        }
        let natureza_txt: String = uow
            .conexao()
            .query_row(
                "SELECT natureza FROM razao_conta WHERE empresa = ?1 AND id = ?2",
                params![blob(ctx.empresa), blob(self.pai)],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?
            .ok_or_else(|| Erro::nao_encontrado("conta pai"))?;
        let natureza = match natureza_txt.as_str() {
            "Ativo" => Natureza::Ativo,
            "Passivo" => Natureza::Passivo,
            "PatrimonioLiquido" => Natureza::PatrimonioLiquido,
            "Receita" => Natureza::Receita,
            _ => Natureza::Despesa,
        };
        let mut repo = RepositorioRazao::novo(uow);
        let pai = repo
            .info_conta(self.pai)
            .map_err(|e| Erro::de_dominio(&e))?
            .ok_or_else(|| Erro::nao_encontrado("conta pai"))?;
        let pai_codigo = CodigoConta::novo(pai.codigo.clone());
        if pai_codigo.nivel() >= NIVEL_MAXIMO {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                format!("o plano tem no máximo {NIVEL_MAXIMO} níveis"),
            ));
        }
        let ultimo = repo
            .ultimo_codigo_filho(ctx.empresa, self.pai)
            .map_err(|e| Erro::de_dominio(&e))?;
        let codigo = pai_codigo.proximo_filho(ultimo.as_ref());
        let conta = if self.agrupadora {
            Conta::abrir_filha_agrupadora(self.pai, &pai, codigo, nome, natureza, "financeiro")
        } else {
            Conta::abrir_filha(
                self.pai,
                &pai,
                codigo,
                nome,
                natureza,
                None,
                None,
                "financeiro",
            )
        }
        .map_err(|e| Erro::de_dominio(&e))?;
        let codigo_txt = conta.codigo.to_string();
        repo.inserir_conta(&conta)
            .map_err(|e| Erro::de_dominio(&e))?;
        Ok(ContaDoPlanoCriada {
            conta: conta.id,
            codigo: codigo_txt,
        })
    }
}

/// Acrescenta ao plano as contas de um modelo de segmento e liga as categorias de mesmo nome.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AplicarModeloDePlano {
    /// O segmento.
    pub modelo: ModeloDePlano,
}

/// O que o modelo fez.
#[derive(Debug, Serialize, Deserialize)]
pub struct ModeloAplicado {
    /// Os códigos das contas criadas (as que já existiam ficam como estão).
    pub criadas: Vec<String>,
    /// Quantas categorias passaram a lançar numa conta nova.
    pub categorias_vinculadas: u32,
}

impl Comando for AplicarModeloDePlano {
    type Saida = ModeloAplicado;
    const PERMISSAO: &'static str = "financeiro.plano.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        use cardeal_ledger::{contas_do_modelo, CodigoConta, Conta, PortaRazao, RepositorioRazao};

        let mut criadas = Vec::new();
        {
            let mut repo = RepositorioRazao::novo(uow);
            for c in contas_do_modelo(self.modelo) {
                if repo
                    .conta_por_codigo(ctx.empresa, c.codigo)
                    .map_err(|e| Erro::de_dominio(&e))?
                    .is_some()
                {
                    continue;
                }
                let pai_codigo = CodigoConta::novo(&c.codigo[..c.codigo.rfind('.').unwrap_or(0)]);
                let pai_id = repo
                    .conta_por_codigo(ctx.empresa, pai_codigo.como_str())
                    .map_err(|e| Erro::de_dominio(&e))?
                    .ok_or_else(|| {
                        Erro::nao_encontrado("conta pai do modelo no plano (plano base ausente?)")
                    })?;
                let pai = repo
                    .info_conta(pai_id)
                    .map_err(|e| Erro::de_dominio(&e))?
                    .ok_or_else(|| Erro::nao_encontrado("conta pai do modelo"))?;
                let conta = Conta::abrir_filha(
                    pai_id,
                    &pai,
                    CodigoConta::novo(c.codigo),
                    c.nome.to_owned(),
                    c.natureza,
                    None,
                    c.papel,
                    "financeiro",
                )
                .map_err(|e| Erro::de_dominio(&e))?;
                repo.inserir_conta(&conta)
                    .map_err(|e| Erro::de_dominio(&e))?;
                criadas.push(c.codigo.to_owned());
            }
        }
        let categorias_vinculadas = vincular_categorias_por_nome(uow.conexao(), ctx.empresa)?;
        Ok(ModeloAplicado {
            criadas,
            categorias_vinculadas,
        })
    }
}

/// Faz as receitas/despesas lançadas com a categoria caírem numa conta do plano. `conta: None`
/// desfaz o vínculo (volta à conta genérica do papel).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VincularCategoriaAConta {
    /// A categoria.
    pub categoria: Id,
    /// A conta analítica de receita (categoria de receber) ou despesa (de pagar).
    pub conta: Option<Id>,
}

impl Comando for VincularCategoriaAConta {
    type Saida = ();
    const PERMISSAO: &'static str = "financeiro.plano.editar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let conn = uow.conexao();
        let especie: Option<Option<String>> = conn
            .query_row(
                "SELECT especie FROM financeiro_categoria WHERE empresa = ?1 AND id = ?2",
                params![blob(ctx.empresa), blob(self.categoria)],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?;
        let especie = especie.ok_or_else(|| Erro::nao_encontrado("categoria"))?;
        if let Some(conta) = self.conta {
            let (natureza, tipo, ativa): (String, String, i64) = conn
                .query_row(
                    "SELECT natureza, tipo, ativa FROM razao_conta WHERE empresa = ?1 AND id = ?2",
                    params![blob(ctx.empresa), blob(conta)],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()
                .map_err(persist)?
                .ok_or_else(|| Erro::nao_encontrado("conta"))?;
            let natureza_ok = match especie.as_deref() {
                Some("Pagar") => natureza == "Despesa",
                Some("Receber") => natureza == "Receita",
                _ => natureza == "Despesa" || natureza == "Receita",
            };
            if tipo != "Analitica" || ativa == 0 || !natureza_ok {
                return Err(Erro::novo(
                    CodigoErro::REGRA_VIOLADA,
                    "escolha uma conta ativa que recebe lançamento, de despesa (categoria de pagar) ou de receita (categoria de receber)",
                ));
            }
        }
        conn.execute(
            "UPDATE financeiro_categoria SET conta = ?3 WHERE empresa = ?1 AND id = ?2",
            params![
                blob(ctx.empresa),
                blob(self.categoria),
                self.conta.map(blob)
            ],
        )
        .map_err(persist)?;
        Ok(())
    }
}
