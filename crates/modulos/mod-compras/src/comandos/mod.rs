//! Os comandos de compras (`docs/modulos/compras.md` §5). Um arquivo, um comando — mais a
//! importação de um XML que chegou da SEFAZ ([`importar_nota_da_sefaz`]), que a busca de notas
//! (a tarefa de `crate::dfe`, fora de transação) chama nota a nota.

mod confirmar_entrada;
mod criar_produto_do_item;
mod definir_preferencias_compras;
mod estornar_entrada;
mod importar_nota_de_arquivo_xml;
mod lancar_nota_manual;
mod vincular_produto_manual;

pub use confirmar_entrada::{ConfirmarEntrada, EntradaConfirmada};
pub use criar_produto_do_item::{CriarProdutoDoItem, ProdutoDoItemCriado};
pub use definir_preferencias_compras::DefinirPreferenciasCompras;
pub use estornar_entrada::{EntradaFoiEstornada, EstornarEntrada, MOTIVO_MINIMO_ESTORNO};
pub use importar_nota_de_arquivo_xml::ImportarNotaDeArquivoXml;
pub use lancar_nota_manual::{ItemNotaManual, LancarNotaManual};
pub use vincular_produto_manual::VincularProdutoManual;

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Cnpj, Dinheiro, Erro, Resultado, Versao};
#[cfg(feature = "sqlite")]
use cardeal_ledger::{Contraparte, PapelConta};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use mod_clientes::{
    ConstrutorPessoa, DocumentoPessoa, Papel, RepositorioClientes, TipoDocumento, TipoPessoa,
};
#[cfg(feature = "sqlite")]
use mod_estoque::{
    completar_ncm_se_vazio, registrar_entrada_comum, DadosEntrada, RepositorioEstoque,
};
#[cfg(feature = "sqlite")]
use mod_financeiro::{
    baixar_pagamento_comum, lancar_titulo_comum, DadosLancamentoTitulo, EspecieTitulo,
    MeioPagamento,
};
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::casamento::{casar, ResultadoCasamento};
#[cfg(feature = "sqlite")]
use crate::erros::ErroCompras;
#[cfg(feature = "sqlite")]
use crate::eventos::{EntradaAConferir, NotaConfirmada};
use crate::nota::EstadoNotaEntrada;
#[cfg(feature = "sqlite")]
use crate::nota::ParcelaCompra;
#[cfg(feature = "sqlite")]
use crate::nota::{conferir_parcelas, EstadoCasamento, ItemNotaEntrada, NotaEntrada};
#[cfg(feature = "sqlite")]
use crate::preferencias::{PreferenciasCompras, RateioPor};
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioCompras;

#[cfg(feature = "sqlite")]
pub(crate) fn carregar_nota(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<NotaEntrada> {
    RepositorioCompras::novo(uow)
        .buscar_nota(id)?
        .ok_or_else(|| Erro::nao_encontrado("nota de entrada"))
}

/// Acha a pessoa fornecedor pelo CNPJ do emitente da nota; cria um cadastro mínimo
/// (`Juridica`, papel `Fornecedor`) se ainda não existir — "casamento automático: fornecedor
/// por CNPJ" (`docs/modulos/compras.md` §5). Chamada direta a `mod-clientes`, mesma
/// transação (`docs/contratos-internos.md` §7 regra 2).
#[cfg(feature = "sqlite")]
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
    /// O título a pagar gerado, se a nota já saiu confirmada sozinha (preferência de
    /// confirmação automática) e a preferência mandou gerar título. `None` se a nota ficou
    /// `AConferir`, ou se confirmou sem gerar título.
    pub titulo: Option<Id>,
    /// Verdadeiro se `titulo` já nasceu com baixa completa (`pago_no_ato_padrao`).
    /// Irrelevante (`false`) quando `titulo` é `None`.
    pub titulo_pago: bool,
}

/// Interpreta um XML já baixado da SEFAZ (a busca de notas, `crate::dfe`) e importa a nota —
/// um envelope fino sobre [`importar_nota_interpretada`].
///
/// # Errors
/// Erro de domínio (`ErroFiscal::XmlInvalido` já vem tratado pelo chamador — este recebe o
/// XML já interpretado) ou de infraestrutura (SQLite).
#[cfg(feature = "sqlite")]
pub fn importar_nota_da_sefaz(
    nota_xml: &cardeal_fiscal::NotaFiscalXml,
    xml_bruto: &str,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<RelatorioImportacao> {
    importar_nota_interpretada(nota_xml, xml_bruto, ctx, uow)
}

/// O cabeçalho da nota a partir do XML. O total é o que o fornecedor cobra (o `vNF`): produtos −
/// descontos + ICMS-ST + IPI + frete/seguro/outras despesas.
#[cfg(feature = "sqlite")]
fn nota_do_xml(nota_xml: &cardeal_fiscal::NotaFiscalXml, fornecedor: Id, ctx: &Ctx) -> NotaEntrada {
    let despesas = nota_xml.valor_frete + nota_xml.valor_seguro + nota_xml.valor_outras_despesas;
    let valor_nos_itens: Dinheiro = nota_xml
        .itens
        .iter()
        .map(cardeal_fiscal::ItemNfe::valor_na_nota)
        .sum();
    NotaEntrada {
        id: Id::novo(),
        empresa: ctx.empresa,
        fornecedor,
        chave_acesso: Some(nota_xml.chave_acesso.como_str().to_string()),
        numero: nota_xml.numero.clone(),
        serie: nota_xml.serie.clone(),
        data_emissao: nota_xml.emissao,
        valor_produtos: nota_xml.itens.iter().map(|i| i.valor_total).sum(),
        valor_frete: nota_xml.valor_frete,
        valor_seguro: nota_xml.valor_seguro,
        valor_outras_despesas: nota_xml.valor_outras_despesas,
        valor_total: valor_nos_itens + despesas,
        estado: EstadoNotaEntrada::AConferir,
        alerta: alerta_de_cfop(nota_xml),
        versao: Versao::INICIAL,
    }
}

/// As duplicatas do XML como parcelas da nota.
#[cfg(feature = "sqlite")]
fn parcelas_do_xml(nota_xml: &cardeal_fiscal::NotaFiscalXml) -> Vec<ParcelaCompra> {
    nota_xml
        .duplicatas
        .iter()
        .map(|d| ParcelaCompra {
            numero: d.numero.clone(),
            vencimento: d.vencimento,
            valor: d.valor,
        })
        .collect()
}

/// O aviso quando as duplicatas não servem como estão (não fecham com o total, vencem antes
/// da emissão): a nota não se confirma sozinha, e quem confere acerta as parcelas.
#[cfg(feature = "sqlite")]
fn alerta_de_parcelas(parcelas: &[ParcelaCompra], nota: &NotaEntrada) -> Option<String> {
    if parcelas.is_empty() {
        return None;
    }
    conferir_parcelas(parcelas, nota.valor_total, nota.data_emissao)
        .err()
        .map(|e| format!("Confira as parcelas antes de dar entrada — duplicatas do XML: {e}."))
}

/// O aviso para quem confere quando algum item tem CFOP que não é venda de mercadoria: a
/// remessa de um aparelho para conserto, a bonificação, a consignação e o faturamento de
/// entrega futura não podem virar estoque com conta a pagar sem alguém olhar.
#[cfg(feature = "sqlite")]
fn alerta_de_cfop(nota_xml: &cardeal_fiscal::NotaFiscalXml) -> Option<String> {
    let cfops = nota_xml.cfops_fora_de_venda();
    if cfops.is_empty() {
        return None;
    }
    let lista: Vec<String> = cfops
        .iter()
        .map(|c| format!("CFOP {c} ({})", cardeal_fiscal::descrever_cfop(c)))
        .collect();
    Some(format!(
        "Confira antes de dar entrada: {} — não é venda do fornecedor.",
        lista.join(", ")
    ))
}

/// Recusa o XML que não é uma compra da empresa: emitido por ela, para outro destinatário,
/// devolução de cliente ou nota de entrada do emitente (lição do `gestao-raiz`).
///
/// # Errors
/// [`ErroCompras::NaoECompra`] com o motivo.
#[cfg(feature = "sqlite")]
fn exigir_compra(
    nota_xml: &cardeal_fiscal::NotaFiscalXml,
    cnpj_empresa: &cardeal_kernel::Cnpj,
) -> Resultado<()> {
    match nota_xml.classificar_para(cnpj_empresa).motivo_da_recusa() {
        None => Ok(()),
        Some(motivo) => Err(Erro::de_dominio(&crate::erros::ErroCompras::NaoECompra(
            motivo,
        ))),
    }
}

/// O miolo comum de toda importação de nota por XML — chamado por
/// [`importar_nota_da_sefaz`] (XML baixado da distribuição `DFe`, via `PortaFiscal`) e por
/// [`ImportarNotaDeArquivoXml`](crate::ImportarNotaDeArquivoXml) (XML lido de um arquivo
/// local, sem `PortaFiscal` nenhum — a única diferença entre os dois é de onde o XML veio;
/// depois de interpretado, é o mesmo `NotaFiscalXml` e o mesmo fluxo). Cria a `NotaEntrada`
/// em conferência, rodando a cascata de casamento (GTIN → regra aprendida → NCM+similaridade
/// → nada) em cada item. Idempotente por `(empresa, chave_acesso)` — reimportar o mesmo XML
/// (mesma chave) nunca duplica a nota, só devolve o relatório da que já existe. Se a
/// preferência `confirma_automaticamente_quando_tudo_casa` estiver ligada e **todos** os
/// itens casarem por GTIN ou regra aprendida (nunca por sugestão), confirma a entrada
/// sozinha — e dá baixa no título na hora se `pago_no_ato_padrao` também mandar.
///
/// # Errors
/// Erro de domínio ou de infraestrutura (SQLite).
#[cfg(feature = "sqlite")]
pub(crate) fn importar_nota_interpretada(
    nota_xml: &cardeal_fiscal::NotaFiscalXml,
    xml_bruto: &str,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<RelatorioImportacao> {
    let chave = nota_xml.chave_acesso.como_str();
    if let Some(existente) =
        RepositorioCompras::novo(uow).buscar_nota_por_chave(ctx.empresa, chave)?
    {
        // A nota importada antes de o XML ser guardado ganha o XML agora.
        RepositorioCompras::novo(uow).guardar_xml(existente, xml_bruto)?;
        let nota = carregar_nota(uow, existente)?;
        let pendentes = RepositorioCompras::novo(uow).contar_itens_nao_casados(nota.id)?;
        return Ok(RelatorioImportacao {
            nota_entrada: nota.id,
            estado: nota.estado,
            itens_nao_casados: pendentes,
            titulo: None,
            titulo_pago: false,
        });
    }

    let cnpj_empresa = RepositorioCompras::novo(uow).cnpj_da_empresa(ctx.empresa)?;
    exigir_compra(nota_xml, &cnpj_empresa)?;

    let fornecedor = resolver_fornecedor(
        &nota_xml.fornecedor_cnpj,
        &nota_xml.fornecedor_nome,
        ctx,
        uow,
    )?;
    let preferencias = RepositorioCompras::novo(uow).preferencias(ctx.empresa)?;

    let mut nota = nota_do_xml(nota_xml, fornecedor, ctx);
    let parcelas = parcelas_do_xml(nota_xml);
    if let Some(aviso) = alerta_de_parcelas(&parcelas, &nota) {
        nota.alerta = Some(match nota.alerta.take() {
            Some(cfop) => format!("{cfop} {aviso}"),
            None => aviso,
        });
    }
    let despesas = nota_xml.valor_frete + nota_xml.valor_seguro + nota_xml.valor_outras_despesas;

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
    let mut itens = montar_itens(
        nota.id,
        fornecedor,
        despesas,
        &pesos,
        nota_xml.itens.iter().map(|i| {
            (
                i.codigo_fornecedor.as_str(),
                i.descricao.as_str(),
                i.ncm.as_str(),
                i.codigo_barras.as_deref(),
                i.quantidade,
                i.valor_unitario,
            )
        }),
        ctx,
        uow,
    )?;
    // Desconto e impostos não recuperáveis do XML entram no custo de cada item.
    for (item, xml) in itens.iter_mut().zip(&nota_xml.itens) {
        item.valor_desconto = xml.valor_desconto;
        item.valor_impostos = xml.valor_icms_st + xml.valor_ipi;
    }

    {
        let mut repo = RepositorioCompras::novo(uow);
        repo.inserir_nota(&nota)?;
        repo.guardar_xml(nota.id, xml_bruto)?;
        for item in &itens {
            repo.inserir_item(item)?;
        }
        repo.substituir_parcelas(nota.id, &parcelas)?;
    }

    let itens_nao_casados = itens
        .iter()
        .filter(|i| i.estado_casamento != EstadoCasamento::Casado)
        .count();

    // Nota com aviso (CFOP fora de venda) sempre passa pela conferência.
    if nota.alerta.is_none() {
        if let Some(relatorio) =
            tentar_confirmar_automaticamente(nota.id, &preferencias, itens_nao_casados, ctx, uow)?
        {
            return Ok(relatorio);
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
        titulo: None,
        titulo_pago: false,
    })
}

/// Confirma a entrada sozinha quando a preferência `confirma_automaticamente_quando_tudo_casa`
/// permite (todo item já `Casado`, nunca por `SugestaoForte`) e há um `local_padrao`
/// configurado — compartilhado entre [`importar_nota_interpretada`] e
/// [`LancarNotaManual`](crate::LancarNotaManual), que só diferem em como os itens chegaram.
/// `Ok(None)` = não confirmou (preferência desligada, itens pendentes, ou sem local padrão) —
/// o chamador segue o caminho normal (`AConferir` + evento).
#[cfg(feature = "sqlite")]
fn tentar_confirmar_automaticamente(
    nota_id: Id,
    preferencias: &PreferenciasCompras,
    itens_nao_casados: usize,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Option<RelatorioImportacao>> {
    if !preferencias.confirma_automaticamente_quando_tudo_casa || itens_nao_casados != 0 {
        return Ok(None);
    }
    let Some(local) = preferencias.local_padrao else {
        return Ok(None);
    };
    let confirmacao = confirmar_entrada_comum(
        nota_id,
        local,
        preferencias.gera_titulo_a_pagar,
        preferencias.pago_no_ato_padrao,
        ctx,
        uow,
    )?;
    let confirmada = carregar_nota(uow, nota_id)?;
    Ok(Some(RelatorioImportacao {
        nota_entrada: nota_id,
        estado: confirmada.estado,
        itens_nao_casados: 0,
        titulo: confirmacao.titulo,
        titulo_pago: confirmacao.pago,
    }))
}

/// Rateia as despesas acessórias entre os itens (nunca perde centavo,
/// `Dinheiro::ratear_por_pesos`) e roda a cascata de casamento em cada um — o miolo comum
/// entre [`importar_nota_da_sefaz`] (itens vindos do XML) e
/// [`LancarNotaManual`](crate::LancarNotaManual) (itens digitados).
#[allow(clippy::type_complexity)] // a tupla espelha os campos de `ItemNfe`/`ItemNotaManual`
#[cfg(feature = "sqlite")]
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
            Option<&'a str>,
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
    for ((codigo_fornecedor, descricao, ncm, codigo_barras, quantidade, valor_unitario), rateio) in
        campos.zip(rateios)
    {
        itens.push(montar_item_casado(
            nota_id,
            fornecedor,
            codigo_fornecedor,
            descricao,
            ncm,
            codigo_barras,
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
/// compartilhado entre [`importar_nota_interpretada`] (campos vindos do XML, `codigo_barras`
/// = `cEAN`) e [`LancarNotaManual`](crate::LancarNotaManual) (campos digitados,
/// `codigo_barras` opcional) — os dois só diferem na origem do dado, não na lógica de
/// casamento/rateio.
#[allow(clippy::too_many_arguments)]
#[cfg(feature = "sqlite")]
fn montar_item_casado(
    nota_id: Id,
    fornecedor: Id,
    codigo_fornecedor: &str,
    descricao: &str,
    ncm: &str,
    codigo_barras: Option<&str>,
    quantidade: cardeal_kernel::Quantidade,
    valor_unitario: cardeal_kernel::Preco,
    rateio: Dinheiro,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ItemNotaEntrada> {
    // GTIN primeiro: mais forte que a regra aprendida (é o fabricante identificando o
    // produto, não uma inferência por fornecedor+código) — `crate::casamento` §cascata.
    let produto_por_gtin = match codigo_barras.filter(|g| !g.is_empty()) {
        Some(gtin) => RepositorioEstoque::novo(uow)
            .buscar_produto_por_codigo_barras(ctx.organizacao, gtin)?
            .map(|p| p.id),
        None => None,
    };
    let regra = RepositorioCompras::novo(uow).buscar_regra_casamento(
        ctx.empresa,
        fornecedor,
        codigo_fornecedor,
    )?;
    let candidatos = RepositorioEstoque::novo(uow).produtos_por_ncm(ctx.organizacao, ncm)?;
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
        codigo_barras: codigo_barras.filter(|g| !g.is_empty()).map(str::to_owned),
        ncm: ncm.to_string(),
        quantidade,
        valor_unitario,
        valor_rateio: rateio,
        valor_desconto: Dinheiro::ZERO,
        valor_impostos: Dinheiro::ZERO,
        estado_casamento: EstadoCasamento::NaoCasado,
    };
    match casar(produto_por_gtin, regra, descricao, &candidatos_ref) {
        ResultadoCasamento::Certo(p) => item.vincular(p),
        ResultadoCasamento::Sugestao(p) => item.sugerir(p),
        ResultadoCasamento::Nenhum => {}
    }
    Ok(item)
}

/// O que [`confirmar_entrada_comum`] devolve: o título gerado (se algum) e se ele já nasceu
/// pago.
#[derive(Debug, Clone, Copy)]
pub struct ConfirmacaoEntrada {
    /// O título a pagar gerado, se `gerar_titulo` mandou gerar um.
    pub titulo: Option<Id>,
    /// Verdadeiro se `titulo` já nasceu com baixa completa (`pago_no_ato`). Sempre `false`
    /// quando `titulo` é `None` — nada para dar baixa.
    pub pago: bool,
}

/// Confirma a entrada de uma nota com todos os itens casados: consome
/// `mod_estoque::registrar_entrada_comum` por item (custo já com rateio embutido) e,
/// opcionalmente, gera o título a pagar no financeiro com
/// `mod_financeiro::lancar_titulo_comum` — D Estoque / C Fornecedores por parcela
/// (`docs/modulos/compras.md` §7), o mesmo receituário simples que o helper já produz para
/// a espécie `Pagar` quando `papel_resultado = EstoqueMercadorias`. As parcelas do título são
/// as da nota (as duplicatas do XML, `cobr/dup`); sem elas, uma parcela vencendo na emissão.
/// Para informar outras, [`confirmar_entrada_com_parcelas`].
///
/// Quando `gerar_titulo` e `pago_no_ato` são ambos verdadeiros — a compra já foi paga à
/// vista, na hora —, a mesma transação também dá baixa completa no título recém-criado, na
/// data da confirmação (`ctx.hoje()`), via `mod_financeiro::baixar_pagamento_comum`: o título
/// nunca fica de fato "em aberto" um instante sequer no Contas a Pagar. `pago_no_ato` sem
/// `gerar_titulo` não faz nada (não há título nenhum para baixar).
///
/// # Errors
/// [`ErroCompras::ItemNaoCasado`] se algum item não estiver `Casado`;
/// [`ErroCompras::NotaJaConfirmada`] se a nota já não aceitar confirmação; erro de domínio do
/// financeiro se a baixa (`pago_no_ato`) não puder ser aplicada (não deveria acontecer: o
/// título acabou de nascer, sem baixa nenhuma ainda).
///
/// # Panics
/// Nunca, na prática: o `.expect()` interno em `produto_casado` só executa depois de já ter
/// confirmado, logo acima, que nenhum item está fora de `Casado` — a checagem de
/// `ItemNaoCasado` é exatamente o que garante que todo item aqui tem produto vinculado.
#[cfg(feature = "sqlite")]
pub fn confirmar_entrada_comum(
    nota_id: Id,
    local: Id,
    gerar_titulo: bool,
    pago_no_ato: bool,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ConfirmacaoEntrada> {
    confirmar_entrada_com_parcelas(nota_id, local, gerar_titulo, pago_no_ato, None, ctx, uow)
}

/// As parcelas do título a pagar: as informadas (vazias = uma só, na emissão) ou, sem elas, as
/// da nota; conferidas contra o total e gravadas na nota como as usadas. A compra paga no ato
/// tem uma parcela só, quitada na hora — as duplicatas ficam de lado.
#[cfg(feature = "sqlite")]
fn cronograma_da_entrada(
    nota: &NotaEntrada,
    informadas: Option<Vec<ParcelaCompra>>,
    pago_no_ato: bool,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Option<Vec<(cardeal_kernel::Data, Dinheiro)>>> {
    if pago_no_ato {
        return Ok(None);
    }
    let parcelas = match informadas {
        Some(p) => p,
        None => RepositorioCompras::novo(uow).parcelas_da_nota(nota.id)?,
    };
    if parcelas.is_empty() {
        return Ok(None);
    }
    conferir_parcelas(&parcelas, nota.valor_total, nota.data_emissao)
        .map_err(|e| Erro::de_dominio(&e))?;
    RepositorioCompras::novo(uow).substituir_parcelas(nota.id, &parcelas)?;
    Ok(Some(
        parcelas.iter().map(|p| (p.vencimento, p.valor)).collect(),
    ))
}

/// Como [`confirmar_entrada_comum`], com as parcelas do título informadas por quem confirma
/// (`None` = as da nota).
///
/// # Errors
/// Os de [`confirmar_entrada_comum`]; [`ErroCompras::ParcelasNaoFecham`] e
/// [`ErroCompras::ParcelaInvalida`].
///
/// # Panics
/// Nunca, na prática — ver [`confirmar_entrada_comum`].
#[cfg(feature = "sqlite")]
pub fn confirmar_entrada_com_parcelas(
    nota_id: Id,
    local: Id,
    gerar_titulo: bool,
    pago_no_ato: bool,
    parcelas: Option<Vec<ParcelaCompra>>,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ConfirmacaoEntrada> {
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
        // A peça cadastrada às pressas (sem NCM) ganha o NCM da nota que a trouxe.
        completar_ncm_se_vazio(produto, &item.ncm, uow)?;
    }

    let (titulo, pago) = if gerar_titulo {
        let cronograma = cronograma_da_entrada(&nota, parcelas, pago_no_ato, uow)?;
        let gravado = lancar_titulo_comum(
            DadosLancamentoTitulo {
                especie: EspecieTitulo::Pagar,
                contraparte: Some(Contraparte::Fornecedor(nota.fornecedor)),
                valor_total: nota.valor_total,
                emissao: nota.data_emissao,
                parcelas: 1,
                primeiro_vencimento: nota.data_emissao,
                intervalo_dias: 0,
                cronograma,
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

        let pago = if pago_no_ato {
            let parcela = gravado
                .parcelas
                .first()
                .copied()
                .expect("lancar_titulo_comum com parcelas: 1 sempre grava uma parcela");
            // Pago no ato a um fornecedor é virtualmente sempre Pix/transferência, nunca
            // dinheiro do caixa físico do balcão — não há campo de meio de pagamento na UI
            // de compras hoje, então assume Pix (mesmo papel de conta que um TED/boleto
            // pago pelo banco: `MeioPagamento::Pix`/`Cartao` são equivalentes aqui).
            baixar_pagamento_comum(
                parcela,
                nota.valor_total,
                ctx.hoje(),
                MeioPagamento::Pix,
                None,
                ctx,
                uow,
            )?;
            true
        } else {
            false
        };

        (Some(gravado.titulo), pago)
    } else {
        (None, false)
    };

    RepositorioCompras::novo(uow).atualizar_nota(&nota)?;

    uow.publicar(NotaConfirmada {
        nota_entrada: nota.id,
        fornecedor: nota.fornecedor,
        valor_total: nota.valor_total,
        titulo,
        pago,
    })
    .map_err(|e| Erro::de_dominio(&e))?;

    Ok(ConfirmacaoEntrada { titulo, pago })
}
