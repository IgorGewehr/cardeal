//! # mod-compras
//!
//! Da nota de entrada conferida ao estoque atualizado e ao título a pagar gerado. Ver
//! `docs/modulos/compras.md` para a especificação funcional completa.
//!
//! ## O que este crate contém
//!
//! Domínio puro: [`NotaEntrada`]/[`EstadoNotaEntrada`] (a máquina de estados `AConferir` →
//! `Conferida`/`Confirmada`), [`ItemNotaEntrada`]/[`EstadoCasamento`], a cascata de
//! casamento de produto ([`casar`] — GTIN → regra aprendida → NCM + similaridade de
//! descrição → nada), e [`PreferenciasCompras`] (a decisão desta sessão para "vários ajustes
//! de preferências" que o usuário pediu — ver o doc do módulo `preferencias`; inclui
//! `pago_no_ato_padrao`, a resposta ao pedido de "permitir gerar ou não registro automático
//! no financeiro seja algo pago ou não").
//!
//! E a amarração ao motor ([`ModuloCompras`]): migrações (`compras_nota_entrada`/
//! `compras_item_nota_entrada`/`compras_regra_casamento`/`compras_preferencias`/
//! `compras_dfe_estado`/`compras_dfe_nota`), os comandos de despacho
//! (**[`DefinirPreferenciasCompras`]**, **[`VincularProdutoManual`]**, **[`ConfirmarEntrada`]**,
//! **[`ImportarNotaDeArquivoXml`]** — XML lido de um arquivo local pela UI —, ...) e a
//! **tarefa** [`TAREFA_BUSCAR_NA_SEFAZ`]: as notas de compra que a SEFAZ entrega ao CNPJ
//! (distribuição `DFe`), com a Ciência da Operação das que chegam só como resumo — fora de
//! qualquer transação, como a transmissão da NFC-e (ver o módulo `dfe`).
//! [`importar_nota_da_sefaz`] é a importação de um XML já interpretado, que a tarefa usa.
//!
//! O casamento **automático que o usuário pediu de ponta a ponta** funciona assim: a busca na
//! SEFAZ (ou, para um arquivo local, [`ImportarNotaDeArquivoXml`]) traz o XML, interpreta (`cardeal_fiscal::interpretar`), resolve (ou
//! cadastra) o fornecedor pelo CNPJ do emitente, roda a cascata de casamento por item, rateia
//! frete/seguro/outras despesas (`Dinheiro::ratear_por_pesos`, nunca perde centavo), e — se
//! as preferências da empresa permitirem e **todo** item tiver casado por GTIN ou regra
//! aprendida — confirma a entrada sozinha: `mod_estoque::registrar_entrada_comum` por item
//! e, opcionalmente, `mod_financeiro::lancar_titulo_comum` para o título a pagar (com baixa
//! completa na mesma transação, via `mod_financeiro::baixar_pagamento_comum`, quando
//! `pago_no_ato_padrao` também mandar). Caso contrário, a nota fica `AConferir` para um
//! humano revisar com `VincularProdutoManual` + `ConfirmarEntrada`.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 3)
//!
//! Cotação e pedido de compra (o doc original tem o spec; esta versão vai direto da nota ao
//! estoque, como o `gestao-raiz` faz — mas sem copiar a ausência de pedido formal dele, só
//! adiando); devolução ao fornecedor; e as manifestações conclusivas do destinatário
//! (confirmação, desconhecimento) — hoje só a ciência, automática.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod casamento;
mod comandos;
mod consultas;
mod dfe;
mod erros;
pub mod eventos;
mod manifesto;
#[cfg(feature = "sqlite")]
pub mod migracoes;
#[cfg(feature = "sqlite")]
mod modulo;
mod nota;
mod preferencias;
#[cfg(feature = "sqlite")]
mod repositorio;

pub use casamento::{casar, RegraCasamentoAprendida, ResultadoCasamento, LIMIAR_SUGESTAO_FORTE};
#[cfg(feature = "sqlite")]
pub use comandos::{
    confirmar_entrada_com_parcelas, confirmar_entrada_comum, importar_nota_da_sefaz,
};
pub use comandos::{
    ConfirmacaoEntrada, ConfirmarEntrada, DefinirPreferenciasCompras, EntradaConfirmada,
    EntradaFoiEstornada, EstornarEntrada, ImportarNotaDeArquivoXml, ItemNotaManual,
    LancarNotaManual, RelatorioImportacao, VincularProdutoManual, MOTIVO_MINIMO_ESTORNO,
};
pub use comandos::{CriarProdutoDoItem, ProdutoDoItemCriado};
pub use consultas::{
    ItemDoEstorno, ItemNota, ItensDaNota, NotaDeEntradaDoMes, NotasDeEntradaDoMes, NotasRecentes,
    ParcelasDaNota, PreviaDoEstornoDaEntrada, PreviaEstornoEntrada,
};
pub use dfe::{
    EstadoNaSefaz, HoraDeBuscarNaSefaz, ImportarDeNovoDaSefaz, NotaNaSefaz, NotasNaSefaz,
    SituacaoDaBusca, TAREFA_BUSCAR_NA_SEFAZ,
};
pub use erros::ErroCompras;
pub use manifesto::{manifesto, MANIFESTO};
#[cfg(feature = "sqlite")]
pub use modulo::ModuloCompras;
pub use nota::{
    conferir_parcelas, EstadoCasamento, EstadoNotaEntrada, ItemNotaEntrada, NotaEntrada,
    ParcelaCompra,
};
pub use preferencias::{PreferenciasCompras, RateioPor};
#[cfg(feature = "sqlite")]
pub use repositorio::RepositorioCompras;
