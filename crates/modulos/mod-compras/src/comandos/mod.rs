//! Os comandos de compras (`docs/modulos/compras.md` §5). Um arquivo, um comando — mais a
//! parte que não é comando: a varredura da distribuição `DFe` é uma tarefa agendada no spec
//! (§5, sequência), não uma ação de usuário, então vive aqui como função `pub` comum
//! ([`verificar_notas_na_sefaz`]/[`importar_nota_da_sefaz`]), chamável direto por quem tiver
//! um `PortaFiscal` em mãos (hoje, só teste de integração com `FiscalSimulado`; quando
//! `cardeal-modkit::Ctx::porta()` existir, um agendador de verdade chama a mesma função —
//! ver `docs/19-estado-e-processo.md` §1.2/§4).

mod confirmar_entrada;
mod definir_preferencias_compras;
mod lancar_nota_manual;
mod vincular_produto_manual;

pub use confirmar_entrada::{ConfirmarEntrada, EntradaConfirmada};
pub use definir_preferencias_compras::DefinirPreferenciasCompras;
pub use lancar_nota_manual::{ItemNotaManual, LancarNotaManual};
pub use vincular_produto_manual::VincularProdutoManual;

use cardeal_fiscal::{Nsu, PortaFiscal};
use cardeal_kernel::{Cnpj, Dinheiro, Erro, Id, Resultado, Versao};
use cardeal_ledger::{Contraparte, PapelConta};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;
use mod_clientes::{
    ConstrutorPessoa, DocumentoPessoa, Papel, RepositorioClientes, TipoDocumento, TipoPessoa,
};
use mod_estoque::{registrar_entrada_comum, DadosEntrada, RepositorioEstoque};
use mod_financeiro::{lancar_titulo_comum, DadosLancamentoTitulo, EspecieTitulo};
use serde::{Deserialize, Serialize};

use crate::casamento::{casar, ResultadoCasamento};
use crate::erros::ErroCompras;
use crate::eventos::{EntradaAConferir, NotaConfirmada};
use crate::nota::{EstadoCasamento, EstadoNotaEntrada, ItemNotaEntrada, NotaEntrada};
use crate::preferencias::RateioPor;
use crate::repositorio::RepositorioCompras;

fn carregar_nota(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<NotaEntrada> {
    RepositorioCompras::novo(uow)
        .buscar_nota(id)?
        .ok_or_else(|| Erro::nao_encontrado("nota de entrada"))
}

/// Acha a pessoa fornecedor pelo CNPJ do emitente da nota; cria um cadastro mínimo
/// (`Juridica`, papel `Fornecedor`) se ainda não existir — "casamento automático: fornecedor
/// por CNPJ" (`docs/modulos/compras.md` §5). Chamada direta a `mod-clientes`, mesma
/// transação (`docs/contratos-internos.md` §7 regra 2).
fn resolver_fornecedor(
    cnpj: &Cnpj,
    razao_social: &str,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Id> {
    let numero = cnpj.sem_mascara();
    if let Some(pessoa) = RepositorioClientes::novo(uow).buscar_documento(
        ctx.empresa,
        TipoDocumento::Cnpj,
        &numero,
    )? {
        return Ok(pessoa);
    }

    let pessoa = ConstrutorPessoa::nova(
        ctx.empresa,
        TipoPessoa::Juridica,
        razao_social,
        Papel::Fornecedor,
        ctx.agora,
        ctx.hoje(),
    )
    .construir()
    .map_err(|e| Erro::de_dominio(&e))?;
    let documento = DocumentoPessoa::novo(ctx.empresa, pessoa.id, TipoDocumento::Cnpj, &numero)
        .map_err(|e| Erro::de_dominio(&e))?;

    let mut repo = RepositorioClientes::novo(uow);
    repo.inserir_pessoa(&pessoa)?;
    repo.inserir_documento(&documento)?;
    Ok(pessoa.id)
}

/// O que uma importação (manual ou automática) devolve.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RelatorioImportacao {
    /// A nota criada (ou já existente, se a chave já tinha sido importada — idempotente).
    pub nota_entrada: Id,
    /// O estado em que a nota ficou.
    pub estado: EstadoNotaEntrada,
    /// Quantos itens ainda precisam de casamento confirmado.
    pub itens_nao_casados: usize,
}

/// Interpreta um XML já baixado e cria a `NotaEntrada` em conferência, rodando a cascata de
/// casamento em cada item. Idempotente por `(empresa, chave_acesso)`. Se a preferência
/// `confirma_automaticamente_quando_tudo_casa` estiver ligada e **todos** os itens casarem
/// por regra aprendida (nunca por sugestão), confirma a entrada sozinha.
///
/// # Errors
/// Erro de domínio (`ErroFiscal::XmlInvalido` já vem tratado pelo chamador — este recebe o
/// XML já interpretado) ou de infraestrutura (SQLite).
pub fn importar_nota_da_sefaz(
    nota_xml: &cardeal_fiscal::NotaFiscalXml,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<RelatorioImportacao> {
    let chave = nota_xml.chave_acesso.como_str();
    if let Some(existente) =
        RepositorioCompras::novo(uow).buscar_nota_por_chave(ctx.empresa, chave)?
    {
        let nota = carregar_nota(uow, existente)?;
        let pendentes = RepositorioCompras::novo(uow).contar_itens_nao_casados(nota.id)?;
        return Ok(RelatorioImportacao {
            nota_entrada: nota.id,
            estado: nota.estado,
            itens_nao_casados: pendentes,
        });
    }

    let fornecedor = resolver_fornecedor(
        &nota_xml.fornecedor_cnpj,
        &nota_xml.fornecedor_nome,
        ctx,
        uow,
    )?;
    let preferencias = RepositorioCompras::novo(uow).preferencias(ctx.empresa)?;

    let valor_produtos: Dinheiro = nota_xml.itens.iter().map(|i| i.valor_total).sum();
    let despesas = nota_xml.valor_frete + nota_xml.valor_seguro + nota_xml.valor_outras_despesas;

    let nota = NotaEntrada {
        id: Id::novo(),
        empresa: ctx.empresa,
        fornecedor,
        chave_acesso: Some(chave.to_string()),
        numero: nota_xml.numero.clone(),
        serie: nota_xml.serie.clone(),
        data_emissao: nota_xml.emissao,
        valor_produtos,
        valor_frete: nota_xml.valor_frete,
        valor_seguro: nota_xml.valor_seguro,
        valor_outras_despesas: nota_xml.valor_outras_despesas,
        valor_total: valor_produtos + despesas,
        estado: EstadoNotaEntrada::AConferir,
        versao: Versao::INICIAL,
    };

    let pesos: Vec<i64> = match preferencias.rateio_por {
        RateioPor::Valor => nota_xml
            .itens
            .iter()
            .map(|i| i.valor_total.em_centavos())
            .collect(),
        RateioPor::Peso => nota_xml
            .itens
            .iter()
            .map(|i| i.quantidade.unidades_internas())
            .collect(),
    };
    let itens = montar_itens(
        nota.id,
        fornecedor,
        despesas,
        &pesos,
        nota_xml.itens.iter().map(|i| {
            (
                i.codigo_fornecedor.as_str(),
                i.descricao.as_str(),
                i.ncm.as_str(),
                i.quantidade,
                i.valor_unitario,
            )
        }),
        ctx,
        uow,
    )?;

    {
        let mut repo = RepositorioCompras::novo(uow);
        repo.inserir_nota(&nota)?;
        for item in &itens {
            repo.inserir_item(item)?;
        }
    }

    let itens_nao_casados = itens
        .iter()
        .filter(|i| i.estado_casamento != EstadoCasamento::Casado)
        .count();

    if preferencias.confirma_automaticamente_quando_tudo_casa && itens_nao_casados == 0 {
        if let Some(local) = preferencias.local_padrao {
            confirmar_entrada_comum(nota.id, local, preferencias.gera_titulo_a_pagar, ctx, uow)?;
            let confirmada = carregar_nota(uow, nota.id)?;
            return Ok(RelatorioImportacao {
                nota_entrada: nota.id,
                estado: confirmada.estado,
                itens_nao_casados: 0,
            });
        }
    }

    uow.publicar(EntradaAConferir {
        nota_entrada: nota.id,
        fornecedor,
        total_itens: itens.len(),
    })
    .map_err(|e| Erro::de_dominio(&e))?;

    Ok(RelatorioImportacao {
        nota_entrada: nota.id,
        estado: EstadoNotaEntrada::AConferir,
        itens_nao_casados,
    })
}

/// Rateia as despesas acessórias entre os itens (nunca perde centavo,
/// `Dinheiro::ratear_por_pesos`) e roda a cascata de casamento em cada um — o miolo comum
/// entre [`importar_nota_da_sefaz`] (itens vindos do XML) e
/// [`LancarNotaManual`](crate::LancarNotaManual) (itens digitados).
fn montar_itens<'a>(
    nota_id: Id,
    fornecedor: Id,
    despesas: Dinheiro,
    pesos: &[i64],
    campos: impl Iterator<
        Item = (
            &'a str,
            &'a str,
            &'a str,
            cardeal_kernel::Quantidade,
            cardeal_kernel::Preco,
        ),
    >,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Vec<ItemNotaEntrada>> {
    let rateios = if despesas.e_positivo() && pesos.iter().any(|&p| p > 0) {
        despesas.ratear_por_pesos(pesos)
    } else {
        vec![Dinheiro::ZERO; pesos.len()]
    };

    let mut itens = Vec::with_capacity(pesos.len());
    for ((codigo_fornecedor, descricao, ncm, quantidade, valor_unitario), rateio) in
        campos.zip(rateios)
    {
        itens.push(montar_item_casado(
            nota_id,
            fornecedor,
            codigo_fornecedor,
            descricao,
            ncm,
            quantidade,
            valor_unitario,
            rateio,
            ctx,
            uow,
        )?);
    }
    Ok(itens)
}

/// Monta um item de nota já rodado pela cascata de casamento (`crate::casamento::casar`) —
/// compartilhado entre [`importar_nota_da_sefaz`] (campos vindos do XML) e
/// [`LancarNotaManual`](crate::LancarNotaManual) (campos digitados) — os dois só diferem na
/// origem do dado, não na lógica de casamento/rateio.
#[allow(clippy::too_many_arguments)]
fn montar_item_casado(
    nota_id: Id,
    fornecedor: Id,
    codigo_fornecedor: &str,
    descricao: &str,
    ncm: &str,
    quantidade: cardeal_kernel::Quantidade,
    valor_unitario: cardeal_kernel::Preco,
    rateio: Dinheiro,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ItemNotaEntrada> {
    let regra = RepositorioCompras::novo(uow).buscar_regra_casamento(
        ctx.empresa,
        fornecedor,
        codigo_fornecedor,
    )?;
    let candidatos = RepositorioEstoque::novo(uow).produtos_por_ncm(ctx.empresa, ncm)?;
    let candidatos_ref: Vec<(Id, &str)> = candidatos
        .iter()
        .map(|(id, nome)| (*id, nome.as_str()))
        .collect();

    let mut item = ItemNotaEntrada {
        id: Id::novo(),
        nota_entrada: nota_id,
        produto_casado: None,
        codigo_fornecedor: codigo_fornecedor.to_string(),
        descricao_fornecedor: descricao.to_string(),
        ncm: ncm.to_string(),
        quantidade,
        valor_unitario,
        valor_rateio: rateio,
        estado_casamento: EstadoCasamento::NaoCasado,
    };
    match casar(regra, descricao, &candidatos_ref) {
        ResultadoCasamento::Certo(p) => item.vincular(p),
        ResultadoCasamento::Sugestao(p) => item.sugerir(p),
        ResultadoCasamento::Nenhum => {}
    }
    Ok(item)
}

/// Varre a distribuição `DFe` desde o último NSU visto, baixa e importa cada nota nova.
/// Idempotente: reimportar a mesma chave nunca duplica (`importar_nota_da_sefaz`).
///
/// # Errors
/// Erro de domínio ou de infraestrutura — de qualquer nota individual (interrompe a
/// varredura na primeira falha; o NSU só avança para as notas processadas com sucesso).
pub fn verificar_notas_na_sefaz(
    porta: &dyn PortaFiscal,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Vec<RelatorioImportacao>> {
    let cnpj_empresa = RepositorioCompras::novo(uow).cnpj_da_empresa(ctx.empresa)?;
    let ultimo_nsu = RepositorioCompras::novo(uow).ultimo_nsu(ctx.empresa)?;

    let resumos = porta.distribuicao_dfe(&cnpj_empresa, Nsu(ultimo_nsu))?;
    let mut relatorios = Vec::with_capacity(resumos.len());
    let mut maior_nsu = ultimo_nsu;

    for resumo in resumos {
        maior_nsu = maior_nsu.max(resumo.nsu.0);
        if RepositorioCompras::novo(uow)
            .buscar_nota_por_chave(ctx.empresa, resumo.chave_acesso.como_str())?
            .is_some()
        {
            continue;
        }
        let xml_bruto = porta.baixar_xml(&resumo.chave_acesso)?;
        let nota_xml =
            cardeal_fiscal::interpretar(&xml_bruto.0).map_err(|e| Erro::de_dominio(&e))?;
        relatorios.push(importar_nota_da_sefaz(&nota_xml, ctx, uow)?);
    }

    RepositorioCompras::novo(uow).atualizar_ultimo_nsu(ctx.empresa, maior_nsu)?;
    Ok(relatorios)
}

/// Confirma a entrada de uma nota com todos os itens casados: consome
/// `mod_estoque::registrar_entrada_comum` por item (custo já com rateio embutido) e,
/// opcionalmente, gera o título a pagar no financeiro com
/// `mod_financeiro::lancar_titulo_comum` — D Estoque / C Fornecedores por parcela
/// (`docs/modulos/compras.md` §7), o mesmo receituário simples que o helper já produz para
/// a espécie `Pagar` quando `papel_resultado = EstoqueMercadorias`. Sem parcelamento por
/// duplicata ainda (assume um único vencimento na emissão) — o XML de condições de
/// pagamento (`cobr/dup`) não é lido nesta versão.
///
/// # Errors
/// [`ErroCompras::ItemNaoCasado`] se algum item não estiver `Casado`;
/// [`ErroCompras::NotaJaConfirmada`] se a nota já não aceitar confirmação.
///
/// # Panics
/// Nunca, na prática: o `.expect()` interno em `produto_casado` só executa depois de já ter
/// confirmado, logo acima, que nenhum item está fora de `Casado` — a checagem de
/// `ItemNaoCasado` é exatamente o que garante que todo item aqui tem produto vinculado.
pub fn confirmar_entrada_comum(
    nota_id: Id,
    local: Id,
    gerar_titulo: bool,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Option<Id>> {
    let mut nota = carregar_nota(uow, nota_id)?;
    let itens = RepositorioCompras::novo(uow).itens_da_nota(nota_id)?;
    let nao_casados = itens
        .iter()
        .filter(|i| i.estado_casamento != EstadoCasamento::Casado)
        .count();
    if nao_casados > 0 {
        return Err(Erro::de_dominio(&ErroCompras::ItemNaoCasado(nao_casados)));
    }

    nota.confirmar().map_err(|e| Erro::de_dominio(&e))?;

    for item in &itens {
        let produto = item
            .produto_casado
            .expect("todos os itens estão Casado, checado acima");
        registrar_entrada_comum(
            DadosEntrada {
                produto,
                local,
                quantidade: item.quantidade,
                custo_unitario: item.custo_final_unitario(),
                origem_modulo: "compras",
                origem_id: Some(nota.id),
            },
            ctx,
            uow,
        )?;
    }

    let titulo = if gerar_titulo {
        let gravado = lancar_titulo_comum(
            DadosLancamentoTitulo {
                especie: EspecieTitulo::Pagar,
                contraparte: Contraparte::Fornecedor(nota.fornecedor),
                valor_total: nota.valor_total,
                emissao: nota.data_emissao,
                parcelas: 1,
                primeiro_vencimento: nota.data_emissao,
                intervalo_dias: 0,
                observacao: Some(format!("Nota de entrada {}/{}", nota.numero, nota.serie)),
                categoria: None,
                origem_modulo: "compras",
                origem_id: Some(nota.id),
                papel_contraparte: PapelConta::Fornecedores,
                papel_resultado: PapelConta::EstoqueMercadorias,
            },
            ctx,
            uow,
        )?;
        Some(gravado.titulo)
    } else {
        None
    };

    RepositorioCompras::novo(uow).atualizar_nota(&nota)?;

    uow.publicar(NotaConfirmada {
        nota_entrada: nota.id,
        fornecedor: nota.fornecedor,
        valor_total: nota.valor_total,
        titulo,
    })
    .map_err(|e| Erro::de_dominio(&e))?;

    Ok(titulo)
}
