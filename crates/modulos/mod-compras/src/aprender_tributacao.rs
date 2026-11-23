//! A nota de compra ensina a tributação (spec do markup, §5.3): o fornecedor já diz, no XML, se
//! o ICMS veio pago por substituição (CSOSN 500, CST 60, ST cobrado) e se o PIS/COFINS é
//! monofásico (CST 04). Isso alimenta o perfil do produto **que ainda não tem perfil nenhum**
//! (nem próprio nem herdado) e vai para "Tributação a conferir" com o de antes, para desfazer.
//! Produto com perfil nunca é sobrescrito: se a nota o contradiz, vira só um aviso na lista.

use cardeal_kernel::{Id, Resultado};
use cardeal_modkit::Ctx;
use rusqlite::Connection;

use mod_estoque::fiscal::CamposFiscais;

/// As situações (CST/CSOSN) em que o ICMS veio pago antes ou cobrado nesta nota (ST).
const COM_ST: &[&str] = &["10", "30", "60", "70", "201", "202", "203", "500"];

/// Aprende com os itens casados da nota confirmada.
///
/// # Errors
/// Erro de persistência.
pub(crate) fn aprender_da_nota(c: &Connection, ctx: &Ctx, nota: Id, rotulo: &str) -> Resultado<()> {
    let itens = crate::repositorio::itens_da_nota_por_conexao(c, nota)?;
    let produtos: Vec<Id> = itens.iter().filter_map(|i| i.produto_casado).collect();
    if produtos.is_empty() {
        return Ok(());
    }
    let Some(xml) = crate::repositorio::xml_da_nota_por_conexao(c, nota)?
        .and_then(|x| cardeal_fiscal::interpretar(&x).ok())
    else {
        // Nota lançada à mão: não há o que o fornecedor disse.
        return Ok(());
    };
    let perfis = mod_estoque::produtos_para_nota(c, ctx.organizacao, &produtos)?;
    let simples = !matches!(
        mod_estoque::parametros_da_empresa(c, ctx)?.regime,
        mod_estoque::precificacao::RegimeImposto::Manual(_)
    );
    let origem = format!("nota {rotulo}");
    for (i, item) in itens.iter().enumerate() {
        let Some(produto) = item.produto_casado else {
            continue;
        };
        let Some(cadastro) = perfis.get(&produto) else {
            continue;
        };
        let Some(xi) = xml
            .itens
            .get(i)
            .filter(|xi| xi.codigo_fornecedor == item.codigo_fornecedor)
        else {
            continue;
        };
        let t = &xi.tributos;
        let veio_st = (t.valor_st + t.valor_fcp_st).e_positivo()
            || t.situacao.as_deref().is_some_and(|s| COM_ST.contains(&s));
        let disse_monofasico =
            t.cst_pis.as_deref() == Some("04") || t.cst_cofins.as_deref() == Some("04");
        let efetivo = &cadastro.fiscal;
        let sem_perfil =
            efetivo.cst_icms.is_none() && efetivo.cst_pis.is_none() && efetivo.cst_cofins.is_none();
        if sem_perfil {
            let monofasico = disse_monofasico || cardeal_tributos::ncm::ncm_monofasico(&xi.ncm);
            let mut aprendido = CamposFiscais::default();
            let mut motivos = Vec::new();
            if veio_st {
                aprendido.cst_icms = Some(if simples { "500" } else { "60" }.into());
                motivos.push("ICMS já pago pelo fornecedor (ST)");
            }
            if monofasico {
                aprendido.cst_pis = Some("04".into());
                aprendido.cst_cofins = Some("04".into());
                motivos.push(if disse_monofasico {
                    "PIS/COFINS monofásico (CST 04 na nota)"
                } else {
                    "PIS/COFINS monofásico (pelo NCM)"
                });
            }
            if motivos.is_empty() {
                continue;
            }
            aprendido.origem = Some(t.origem);
            mod_estoque::aprender_tributacao_do_produto(
                c,
                ctx.organizacao,
                produto,
                &aprendido,
                t.cest.as_deref(),
                &format!("aprendido da nota de compra: {}", motivos.join("; ")),
                &origem,
                ctx.agora,
            )?;
        } else if veio_st {
            let tributario = efetivo.perfil_tributario(None);
            if !cardeal_tributos::e_substituido(&tributario) && efetivo.cst_icms.is_some() {
                mod_estoque::registrar_a_conferir(
                    c,
                    ctx.organizacao,
                    mod_estoque::AlvoTributacao::Produto(produto),
                    &format!(
                        "a nota diz que o ICMS já veio pago (ST, {}), mas o produto está como {}; \
                         o perfil não foi mudado — confira",
                        t.situacao.as_deref().unwrap_or("ST cobrado"),
                        efetivo.cst_icms.as_deref().unwrap_or("-")
                    ),
                    &origem,
                    efetivo,
                    efetivo,
                    ctx.agora,
                )?;
            }
        }
    }
    Ok(())
}
