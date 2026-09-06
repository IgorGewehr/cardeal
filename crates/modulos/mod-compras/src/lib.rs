//! # mod-compras
//!
//! Da nota de entrada conferida ao estoque atualizado e ao título a pagar gerado. Ver
//! `docs/modulos/compras.md` para a especificação funcional completa.
//!
//! ## O que este crate contém
//!
//! Domínio puro: [`NotaEntrada`]/[`EstadoNotaEntrada`] (a máquina de estados `AConferir` →
//! `Conferida`/`Confirmada`), [`ItemNotaEntrada`]/[`EstadoCasamento`], a cascata de
//! casamento de produto ([`casar`] — regra aprendida → NCM + similaridade de descrição →
//! nada), e [`PreferenciasCompras`] (a decisão desta sessão para "vários ajustes de
//! preferências" que o usuário pediu — ver o doc do módulo `preferencias`).
//!
//! E a amarração ao motor ([`ModuloCompras`]): migrações (`compras_nota_entrada`/
//! `compras_item_nota_entrada`/`compras_regra_casamento`/`compras_preferencias`/
//! `compras_estado_dfe`) e três comandos de despacho (**[`DefinirPreferenciasCompras`]**,
//! **[`VincularProdutoManual`]**, **[`ConfirmarEntrada`]**) — mais duas funções `pub` que
//! **não** são comandos de despacho: [`importar_nota_da_sefaz`] e
//! [`verificar_notas_na_sefaz`]. O porquê de não serem `Comando`: elas precisam de um
//! `PortaFiscal` (`cardeal-fiscal`), e `cardeal_modkit::Ctx::porta()` — a forma de injetar
//! uma porta dentro de um comando — ainda está listado como "adiado"
//! (`docs/contratos-internos.md` §4). Até lá, `verificar_notas_na_sefaz` é chamada direto
//! por quem tiver a porta em mãos (hoje, só teste de integração com `FiscalSimulado`;
//! quando um agendador de tarefas existir, ele chama a mesma função — é exatamente o que a
//! sequência "Tarefa agendada (1h)" do `docs/modulos/compras.md` §5 já descrevia).
//!
//! O casamento **automático que o usuário pediu de ponta a ponta** funciona assim:
//! `verificar_notas_na_sefaz` varre a distribuição `DFe` desde o último NSU visto, baixa e
//! interpreta o XML de cada nota nova (`cardeal_fiscal::interpretar`), resolve (ou cadastra)
//! o fornecedor pelo CNPJ do emitente, roda a cascata de casamento por item, rateia
//! frete/seguro/outras despesas (`Dinheiro::ratear_por_pesos`, nunca perde centavo), e — se
//! as preferências da empresa permitirem e **todo** item tiver casado por regra aprendida —
//! confirma a entrada sozinha: `mod_estoque::registrar_entrada_comum` por item e,
//! opcionalmente, `mod_financeiro::lancar_titulo_comum` para o título a pagar. Caso
//! contrário, a nota fica `AConferir` para um humano revisar com `VincularProdutoManual` +
//! `ConfirmarEntrada`.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 3)
//!
//! Cotação e pedido de compra (o doc original tem o spec; esta versão vai direto da nota ao
//! estoque, como o `gestao-raiz` faz — mas sem copiar a ausência de pedido formal dele, só
//! adiando); devolução ao fornecedor; casamento por GTIN (depende de
//! `estoque_codigo_barras`, ainda não wired em `mod-estoque`); leitura de condições de
//! pagamento do XML (`cobr/dup`) — hoje `ConfirmarEntrada` sempre gera um título de parcela
//! única vencendo na emissão; `ApiFiscalHttp` de verdade (hoje só `FiscalSimulado`); e
//! `Ctx::porta()` em `cardeal-modkit`, que destrava `verificar_notas_na_sefaz` virar um
//! comando de despacho de verdade em vez de função chamada por fora.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod casamento;
mod consultas;
mod comandos;
mod erros;
pub mod eventos;
mod manifesto;
pub mod migracoes;
mod modulo;
mod nota;
mod preferencias;
mod repositorio;

pub use casamento::{casar, RegraCasamentoAprendida, ResultadoCasamento, LIMIAR_SUGESTAO_FORTE};
pub use consultas::{ItemNota, NotasRecentes};
pub use comandos::{
    confirmar_entrada_comum, importar_nota_da_sefaz, verificar_notas_na_sefaz, ConfirmarEntrada,
    DefinirPreferenciasCompras, EntradaConfirmada, ItemNotaManual, LancarNotaManual,
    RelatorioImportacao, VincularProdutoManual,
};
pub use erros::ErroCompras;
pub use manifesto::{manifesto, MANIFESTO};
pub use modulo::ModuloCompras;
pub use nota::{EstadoCasamento, EstadoNotaEntrada, ItemNotaEntrada, NotaEntrada};
pub use preferencias::{PreferenciasCompras, RateioPor};
pub use repositorio::RepositorioCompras;
