//! # cardeal-fiscal
//!
//! O cliente da API fiscal externa (ADR-0006, `docs/10-modulo-fiscal.md`) — o Cardeal não
//! implementa emissor fiscal próprio.
//!
//! O provedor é o **`sefaz-api`**, o motor fiscal próprio da operação
//! (`docs/modulos/fiscal.md`); o contrato dele está em [`ApiFiscalHttp`].
//!
//! ## O que este crate contém
//!
//! - [`PortaFiscal`] — a porta (síncrona nesta fase; ver o porquê no doc do módulo `porta`):
//!   distribuição `DFe` (que `mod-compras` usa) e o status do serviço da SEFAZ.
//! - [`ApiFiscalHttp`] — o adaptador real, cliente do `sefaz-api` ([`ConfigApiFiscal`]: URL e
//!   chave por variável de ambiente).
//! - [`FiscalSimulado`] — o adaptador em memória para testes (autoriza; fica fora do ar sob
//!   comando).
//! - [`ler_certificado_a1`]/[`preparar_certificado_a1`] — abre o `.pfx`, confere a senha, lê
//!   titular, CNPJ e validade, e regrava os arquivos com RC2 (o TLS do `sefaz-api` recusa).
//! - [`interpretar`] — lê os campos essenciais de um XML de NF-e 4.00 (`quick-xml`,
//!   sem validar contra o XSD oficial).
//! - [`ChaveAcesso`] — a chave de acesso de 44 dígitos.
//!
//! ## O que falta (onda 7, `docs/17-roadmap.md`)
//!
//! O restante da trait `PortaFiscal` do doc 10 (emitir, consultar, cancelar, carta de
//! correção, inutilizar) e o documento fiscal como agregado assíncrono em `mod-fiscal` (fila
//! durável, numeração, contingência). O `sefaz-api` não tem distribuição `DFe`, manifestação
//! nem SPED — [`ApiFiscalHttp`] responde [`ErroFiscal::NaoSuportadoPeloProvedor`] para elas.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod certificado;
mod chave;
mod der;
mod dfe;
mod emissao;
mod erros;
mod nfe;
mod porta;
mod sefaz_api;
mod simulado;

pub use certificado::{
    ler_certificado_a1, preparar_certificado_a1, CertificadoPreparado, InfoCertificado,
};
pub use chave::ChaveAcesso;
pub use dfe::{
    ler_evento, ler_resumo_nfe, DocumentoDfe, EventoDfe, Manifestacao, PaginaDfe, ResumoNfe,
    SituacaoDoResumo, TipoDfe,
};
pub use emissao::{chave_de_contingencia, EntradaEmContingencia, NotaEmContingencia};
pub use emissao::{
    Consumidor, ContribuicaoItem, Crt, Csc, DocumentoConsumidor, Emitente, EnderecoEmitente,
    IcmsItem, ImpostosItem, ItemDocumento, MeioPagamento, Nfce, PagamentoDocumento,
    PedidoInutilizacao, RetornoSefaz, SituacaoRetorno,
};
pub use emissao::{Destinatario, IndicadorIe, Nfe};
pub use erros::ErroFiscal;
pub use nfe::{cfop_de_venda, descrever_cfop, ClassificacaoEntrada, FinalidadeNfe};
pub use nfe::{interpretar, Duplicata, ItemNfe, NotaFiscalXml};
pub use porta::{AmbienteFiscal, ModeloFiscal, Nsu, PortaFiscal, StatusSefaz};
pub use sefaz_api::{ApiFiscalHttp, CertificadoA1, ConfigApiFiscal};
pub use simulado::FiscalSimulado;
