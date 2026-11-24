//! O crédito de ICMS/PIS/COFINS na entrada do regime normal (spec do markup, §5.3): sai do custo
//! da mercadoria e vira "Impostos a recuperar" no Razão. A regra é de `cardeal_tributos::
//! credito_do_item`; aqui só se lê o que a nota disse (o XML guardado) e o perfil de saída do
//! produto. Simples e MEI não creditam: para eles nada muda.

use std::collections::HashMap;

use cardeal_kernel::{Dinheiro, Erro, Id, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::Connection;

use crate::nota::ItemNotaEntrada;

/// O regime do motor, ou `None` quando a empresa não credita (MEI/Simples).
fn regime_que_credita(c: &Connection, ctx: &Ctx) -> Resultado<Option<cardeal_tributos::Regime>> {
    let cadastral = mod_estoque::tributacao_simples::regime_cadastral(c, ctx.empresa);
    Ok(Some(match cadastral.as_str() {
        "LucroReal" => cardeal_tributos::Regime::Real {
            pis_cofins_nao_cumulativo: true,
        },
        "LucroPresumido" => cardeal_tributos::Regime::Presumido {
            atividade: cardeal_tributos::Atividade::Comercio,
            receita_mensal: Dinheiro::ZERO,
        },
        _ => return Ok(None),
    }))
}

/// O crédito de cada item da nota (por id do item); vazio se a empresa não credita ou a nota
/// não tem o XML (lançada à mão).
///
/// # Errors
/// Erro de persistência.
pub(crate) fn creditos_da_nota(
    c: &Connection,
    ctx: &Ctx,
    nota: Id,
    itens: &[ItemNotaEntrada],
) -> Resultado<HashMap<Id, Dinheiro>> {
    let mut saida = HashMap::new();
    let Some(regime) = regime_que_credita(c, ctx)? else {
        return Ok(saida);
    };
    let Some(xml) = crate::repositorio::xml_da_nota_por_conexao(c, nota)?
        .and_then(|x| cardeal_fiscal::interpretar(&x).ok())
    else {
        return Ok(saida);
    };
    let produtos: Vec<Id> = itens.iter().filter_map(|i| i.produto_casado).collect();
    let perfis = mod_estoque::produtos_para_nota(c, ctx.organizacao, &produtos)?;
    for (i, item) in itens.iter().enumerate() {
        let Some(cadastro) = item.produto_casado.and_then(|p| perfis.get(&p)) else {
            continue;
        };
        let Some(xi) = xml
            .itens
            .get(i)
            .filter(|xi| xi.codigo_fornecedor == item.codigo_fornecedor)
        else {
            continue;
        };
        let credito = cardeal_tributos::credito_do_item(
            &regime,
            &cadastro.fiscal.perfil_tributario(None),
            &cardeal_tributos::CompraDoItem {
                valor: item.valor_bruto() - item.valor_desconto,
                icms_destacado: xi.tributos.valor_icms,
                credito_sn: xi.tributos.credito_sn,
            },
        )
        .total();
        if credito.e_positivo() {
            saida.insert(item.id, credito);
        }
    }
    Ok(saida)
}

/// Lança `D Impostos a recuperar / C Estoque de mercadorias` pelo total dos créditos — o título
/// já debitou o estoque pelo valor cheio da nota. Origem `compras/credito/<nota>`, que o
/// estorno da entrada desfaz. Sem crédito, nada.
///
/// # Errors
/// Erro do Razão.
pub(crate) fn lancar_credito(
    creditos: &HashMap<Id, Dinheiro>,
    com_titulo: bool,
    nota: &crate::nota::NotaEntrada,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    use cardeal_ledger::{
        ConstrutorLancamento, Contas, Origem, PapelConta, Razao, RepositorioRazao,
    };
    let total = creditos.values().fold(Dinheiro::ZERO, |a, c| a + *c);
    if !com_titulo || !total.e_positivo() {
        return Ok(());
    }
    let mut repo = RepositorioRazao::novo(uow);
    let contas = Contas::nova(&repo, ctx.empresa);
    let recuperar = contas
        .papel(PapelConta::ImpostosARecuperar)
        .map_err(|e| Erro::de_dominio(&e))?;
    let estoque = contas
        .papel(PapelConta::EstoqueMercadorias)
        .map_err(|e| Erro::de_dominio(&e))?;
    let historico = format!(
        "Crédito de impostos da nota de entrada {}/{}",
        nota.numero, nota.serie
    );
    let lanc = ConstrutorLancamento::novo(ctx.empresa, nota.data_emissao, historico)
        .origem(Origem::modulo("compras").tipo("credito").agregado(nota.id))
        .liquidacao(nota.data_emissao)
        .criado_por(ctx.usuario, ctx.dispositivo)
        .agora(ctx.agora)
        .debitar(recuperar, total)
        .creditar(estoque, total)
        .construir()
        .map_err(|e| Erro::de_dominio(&e))?;
    Razao::registrar(&mut repo, lanc).map_err(|e| Erro::de_dominio(&e))?;
    Ok(())
}

/// Estorna o crédito lançado na entrada da nota (se houve).
///
/// # Errors
/// Erro do Razão.
pub(crate) fn estornar_credito(
    nota: Id,
    historico: &str,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let mut repo = cardeal_ledger::RepositorioRazao::novo(uow);
    let vivos = repo
        .lancamentos_vivos_da_origem(ctx.empresa, "compras", "credito", nota)
        .map_err(|e| Erro::de_dominio(&e))?;
    for l in vivos {
        cardeal_ledger::Razao::estornar(&mut repo, l, historico, ctx.hoje())
            .map_err(|e| Erro::de_dominio(&e))?;
    }
    Ok(())
}
