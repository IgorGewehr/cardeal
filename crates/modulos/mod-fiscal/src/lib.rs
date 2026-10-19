//! # mod-fiscal
//!
//! O fiscal da empresa sobre o **`sefaz-api`**, o motor fiscal próprio da operação (ADR-0006,
//! `docs/modulos/fiscal.md`). Esta primeira fatia da onda 7 é a configuração — o que precisa
//! existir antes da primeira nota:
//!
//! - o **certificado digital A1**: conferido no envio (senha, CNPJ igual ao da empresa,
//!   validade), regravado se usar RC2 e guardado **cifrado** no cofre da instalação
//!   ([`cardeal_storage::Cofre`]) — a base só tem texto cifrado;
//! - o **ambiente** (homologação/produção);
//! - a **conexão**: o status do serviço da SEFAZ da UF da empresa, ponta a ponta;
//! - as **pendências** (sem certificado, vencendo, dados fiscais incompletos).
//!
//! Os recursos que o motor injeta ([`cardeal_modkit::Recursos`]): o `Cofre` e o
//! [`ProvedorFiscal`] (quem fala com a API — o `sefaz-api` de verdade, o simulado nos testes,
//! ou "não configurado"). [`porta_da_empresa`] junta os dois e devolve a porta fiscal pronta
//! para a empresa da sessão — é o que a emissão (próximas fatias) vai usar.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod comandos;
mod consultas;
mod documento;
mod erros;
mod explicacao;
mod manifesto;
#[cfg(feature = "sqlite")]
mod migracoes;
#[cfg(feature = "sqlite")]
mod modulo;
#[cfg(feature = "sqlite")]
mod montagem;
mod nfce;
mod provedor;
#[cfg(feature = "sqlite")]
mod sql;
#[cfg(feature = "sqlite")]
mod sql_documento;
mod tipos;
mod tributacao;

pub use cardeal_fiscal::{AmbienteFiscal, ModeloFiscal, StatusSefaz};
pub use comandos::{DefinirAmbienteFiscal, InstalarCertificado, RemoverCertificado};
pub use consultas::{SituacaoFiscal, StatusDaSefaz};
pub use documento::{
    decidir, decidir_evento, espera, pedido_da_anulacao, Desfecho, DocumentoResumo,
    EstadoDocumento, Mudanca, PedidoDocumento, ResumoFiscal,
};
pub use erros::ErroModuloFiscal;
pub use explicacao::explicar;
pub use manifesto::{ID, MANIFESTO};
#[cfg(feature = "sqlite")]
pub use modulo::ModuloFiscal;
#[cfg(feature = "sqlite")]
pub use montagem::{marcar_homologacao, DESCRICAO_HOMOLOGACAO};
pub use nfce::{
    AnularDocumentoDaVenda, CancelarNfce, ConfiguracaoDaNfce, ConfiguracaoNfce, ConfigurarNfce,
    DetalheDocumento, DocumentoDaVenda, DocumentoFiscal, DocumentosDasVendas, DocumentosFiscais,
    EmitirNfceDoCupom, InutilizarNumero, ReenviarDocumento, ResumoDaFilaFiscal, TAREFA_TRANSMITIR,
};
pub use provedor::ProvedorFiscal;
#[cfg(feature = "sqlite")]
pub use provedor::{porta_da_empresa, porta_da_empresa_no_ambiente};
pub use tipos::{
    CertificadoResumo, PendenciaFiscal, SituacaoFiscalDaEmpresa, ALERTA_VENCIMENTO_DIAS,
};
pub use tributacao::{tributar, TributacaoDoItem};
