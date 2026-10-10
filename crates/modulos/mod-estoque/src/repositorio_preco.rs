//! SQL da formação de preço: parâmetros por empresa, margem e perfil fiscal por grupo,
//! margem por produto. Funções livres sobre `&Connection` — servem aos comandos (pela
//! conexão da `UnidadeDeTrabalho`) e às consultas (pelo leitor).

use cardeal_kernel::{Id, Percentual, Resultado};
use rusqlite::{params, Connection, OptionalExtension};

use crate::fiscal::CamposFiscais;
use crate::parametros_preco::ParametrosPreco;
use crate::precificacao::{AnexoSimples, RegimeImposto};
use crate::repositorio::{blob, id_de, persist};

/// O que cada grupo diz sobre preço (`None` = herda do pai).
#[derive(Debug, Clone)]
pub struct PerfilDoGrupo {
    /// O grupo.
    pub id: Id,
    /// O nome.
    pub nome: String,
    /// O pai.
    pub pai: Option<Id>,
    /// Margem líquida desejada.
    pub margem: Option<Percentual>,
    /// ICMS por substituição tributária.
    pub icms_st: Option<bool>,
    /// PIS/COFINS monofásico.
    pub monofasico: Option<bool>,
    /// Os campos fiscais próprios do grupo (o que ele define; o resto herda).
    pub fiscal: CamposFiscais,
}

fn anexo_txt(a: AnexoSimples) -> &'static str {
    match a {
        AnexoSimples::I => "I",
        AnexoSimples::II => "II",
        AnexoSimples::III => "III",
        AnexoSimples::IV => "IV",
        AnexoSimples::V => "V",
    }
}

fn anexo_de(s: &str) -> AnexoSimples {
    match s {
        "II" => AnexoSimples::II,
        "III" => AnexoSimples::III,
        "IV" => AnexoSimples::IV,
        "V" => AnexoSimples::V,
        _ => AnexoSimples::I,
    }
}

fn pct_opt(v: Option<i64>) -> Option<Percentual> {
    v.map(Percentual::unidades)
}

/// Os parâmetros da empresa; sem linha gravada, o padrão do regime cadastral.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn parametros_preco(conn: &Connection, empresa: Id) -> Resultado<ParametrosPreco> {
    let linha = conn
        .query_row(
            "SELECT regime, anexo, imposto_manual, custo_operacional, custo_operacional_derivado,
                    comissao, taxa_pagamento, margem_padrao, margem_minima, preco_obrigatorio
             FROM estoque_parametros_preco WHERE empresa = ?1",
            [blob(empresa)],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)? != 0,
                    r.get::<_, i64>(5)?,
                    r.get::<_, i64>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, i64>(8)?,
                    r.get::<_, i64>(9)? != 0,
                ))
            },
        )
        .optional()
        .map_err(persist)?;
    let Some(l) = linha else {
        let regime: Option<String> = conn
            .query_row(
                "SELECT regime FROM nucleo_empresa WHERE id = ?1",
                [blob(empresa)],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?;
        return Ok(ParametrosPreco::padrao_para(
            regime.as_deref().unwrap_or("SimplesNacional"),
        ));
    };
    let regime = match l.0.as_str() {
        "Mei" => RegimeImposto::Mei,
        "Manual" => RegimeImposto::Manual(Percentual::unidades(l.2)),
        _ => RegimeImposto::Simples(anexo_de(&l.1)),
    };
    Ok(ParametrosPreco {
        regime,
        custo_operacional: Percentual::unidades(l.3),
        custo_operacional_derivado: l.4,
        comissao: Percentual::unidades(l.5),
        taxa_pagamento: Percentual::unidades(l.6),
        margem_padrao: pct_opt(l.7),
        margem_minima: Percentual::unidades(l.8),
        preco_obrigatorio: l.9,
    })
}

/// Grava (insere ou substitui) os parâmetros da empresa.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn salvar_parametros_preco(
    conn: &Connection,
    empresa: Id,
    p: &ParametrosPreco,
) -> Resultado<()> {
    let (regime, anexo, manual) = match p.regime {
        RegimeImposto::Mei => ("Mei", "I", 0),
        RegimeImposto::Simples(a) => ("Simples", anexo_txt(a), 0),
        RegimeImposto::Manual(v) => ("Manual", "I", v.unidades_internas()),
    };
    conn.execute(
        "INSERT INTO estoque_parametros_preco
            (empresa, regime, anexo, imposto_manual, custo_operacional, custo_operacional_derivado,
             comissao, taxa_pagamento, margem_padrao, margem_minima, preco_obrigatorio, versao)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1)
         ON CONFLICT(empresa) DO UPDATE SET
            regime = ?2, anexo = ?3, imposto_manual = ?4, custo_operacional = ?5,
            custo_operacional_derivado = ?6, comissao = ?7, taxa_pagamento = ?8,
            margem_padrao = ?9, margem_minima = ?10, preco_obrigatorio = ?11,
            versao = versao + 1",
        params![
            blob(empresa),
            regime,
            anexo,
            manual,
            p.custo_operacional.unidades_internas(),
            i64::from(p.custo_operacional_derivado),
            p.comissao.unidades_internas(),
            p.taxa_pagamento.unidades_internas(),
            p.margem_padrao.map(Percentual::unidades_internas),
            p.margem_minima.unidades_internas(),
            i64::from(p.preco_obrigatorio),
        ],
    )
    .map_err(persist)?;
    Ok(())
}

/// Margem e perfil fiscal de todos os grupos da organização.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn perfis_dos_grupos(conn: &Connection, organizacao: Id) -> Resultado<Vec<PerfilDoGrupo>> {
    let mut stmt = conn
        .prepare(
            "SELECT id, nome, pai, margem_desejada, icms_st, monofasico, perfil_fiscal
             FROM estoque_grupo_produto WHERE empresa = ?1",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(organizacao)], |r| {
            Ok(PerfilDoGrupo {
                id: id_de(r.get::<_, Vec<u8>>(0)?),
                nome: r.get(1)?,
                pai: r.get::<_, Option<Vec<u8>>>(2)?.map(id_de),
                margem: pct_opt(r.get::<_, Option<i64>>(3)?),
                icms_st: r.get::<_, Option<i64>>(4)?.map(|v| v != 0),
                monofasico: r.get::<_, Option<i64>>(5)?.map(|v| v != 0),
                fiscal: CamposFiscais::de_blob(r.get::<_, Option<Vec<u8>>>(6)?.as_deref()),
            })
        })
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Define (ou limpa, com `None`) a margem desejada de um grupo. `false` se não existe.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn definir_margem_do_grupo(
    conn: &Connection,
    organizacao: Id,
    grupo: Id,
    margem: Option<Percentual>,
) -> Resultado<bool> {
    let n = conn
        .execute(
            "UPDATE estoque_grupo_produto SET margem_desejada = ?3, versao = versao + 1
             WHERE empresa = ?1 AND id = ?2",
            params![
                blob(organizacao),
                blob(grupo),
                margem.map(Percentual::unidades_internas)
            ],
        )
        .map_err(persist)?;
    Ok(n > 0)
}

/// Define (ou limpa) o perfil fiscal de um grupo. `false` se não existe.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn definir_perfil_fiscal_do_grupo(
    conn: &Connection,
    organizacao: Id,
    grupo: Id,
    icms_st: Option<bool>,
    monofasico: Option<bool>,
) -> Resultado<bool> {
    let n = conn
        .execute(
            "UPDATE estoque_grupo_produto SET icms_st = ?3, monofasico = ?4, versao = versao + 1
             WHERE empresa = ?1 AND id = ?2",
            params![
                blob(organizacao),
                blob(grupo),
                icms_st.map(i64::from),
                monofasico.map(i64::from)
            ],
        )
        .map_err(persist)?;
    Ok(n > 0)
}

/// Define (ou limpa) a margem própria de um produto. `false` se não existe.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn definir_margem_do_produto(
    conn: &Connection,
    organizacao: Id,
    produto: Id,
    margem: Option<Percentual>,
) -> Resultado<bool> {
    let n = conn
        .execute(
            "UPDATE estoque_produto SET margem_desejada = ?3, versao = versao + 1
             WHERE empresa = ?1 AND id = ?2",
            params![
                blob(organizacao),
                blob(produto),
                margem.map(Percentual::unidades_internas)
            ],
        )
        .map_err(persist)?;
    Ok(n > 0)
}

/// Define os campos fiscais próprios de um grupo (vazio = herda tudo). `false` se não existe.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn definir_campos_fiscais_do_grupo(
    conn: &Connection,
    organizacao: Id,
    grupo: Id,
    campos: &CamposFiscais,
) -> Resultado<bool> {
    let n = conn
        .execute(
            "UPDATE estoque_grupo_produto SET perfil_fiscal = ?3, versao = versao + 1
             WHERE empresa = ?1 AND id = ?2",
            params![blob(organizacao), blob(grupo), campos.para_blob()],
        )
        .map_err(persist)?;
    Ok(n > 0)
}

/// Define os campos fiscais próprios de um produto (vazio = herda tudo). `false` se não existe.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn definir_campos_fiscais_do_produto(
    conn: &Connection,
    organizacao: Id,
    produto: Id,
    campos: &CamposFiscais,
) -> Resultado<bool> {
    let n = conn
        .execute(
            "UPDATE estoque_produto SET perfil_fiscal = ?3, versao = versao + 1
             WHERE empresa = ?1 AND id = ?2",
            params![blob(organizacao), blob(produto), campos.para_blob()],
        )
        .map_err(persist)?;
    Ok(n > 0)
}
