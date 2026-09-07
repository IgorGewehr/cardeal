//! # cardeal-fiscal
//!
//! O cliente da API fiscal externa (ADR-0006, `docs/10-modulo-fiscal.md`) — o Cardeal não
//! implementa emissor fiscal próprio.
//!
//! ## O que este crate contém agora
//!
//! Uma fatia mínima, mirada no que `mod-compras` precisa para importar nota de entrada por
//! distribuição `DFe`:
//!
//! - [`PortaFiscal`] — a porta (síncrona nesta fase; ver o porquê no doc do módulo
//!   `porta`) com `distribuicao_dfe`/`baixar_xml`.
//! - [`FiscalSimulado`] — o adaptador em memória para testes e para `mod-compras` já
//!   funcionar de ponta a ponta antes do adaptador HTTP real existir.
//! - [`interpretar`] — lê os campos essenciais de um XML de NF-e 4.00 (`quick-xml`,
//!   sem validar contra o XSD oficial).
//! - [`ChaveAcesso`] — a chave de acesso de 44 dígitos.
//!
//! ## O que falta (ver `docs/17-roadmap.md`, Fase 3)
//!
//! `ApiFiscalHttp` (o adaptador real contra o `sefaz-api`), o restante da trait `PortaFiscal`
//! do doc 10 (emitir, cancelar, carta de correção, inutilizar, SPED, status do serviço), o
//! certificado digital (upload, guarda cifrada via `cardeal_storage::Segredo<T>`, validade),
//! e `mod-fiscal` (o módulo que amarra tudo isso ao despacho — documento fiscal como
//! agregado assíncrono próprio, fila durável, contingência). Notas levantadas sobre o
//! contrato real do `sefaz-api` (auth por Bearer token, certificado enviado em cada chamada
//! de emissão — o serviço não guarda nada —, NFC-e com contingência offline real,
//! `cStat` cru que precisa de tradução amigável) estão em
//! `docs/19-estado-e-processo.md` §1.2/§4.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod chave;
mod erros;
mod nfe;
mod porta;
mod simulado;

pub use chave::ChaveAcesso;
pub use erros::ErroFiscal;
pub use nfe::{interpretar, ItemNfe, NotaFiscalXml};
pub use porta::{Nsu, PortaFiscal, ResumoDfe, XmlNota};
pub use simulado::FiscalSimulado;
