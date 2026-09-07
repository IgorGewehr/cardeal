//! # mod-orcamentos
//!
//! O **orçamento comercial** como documento próprio — o que o balcão entrega ao cliente
//! antes (ou no lugar) de abrir uma OS: assunto, itens livres com preço e desconto,
//! validade, condições de pagamento e um PDF com a marca da empresa (gerado pela camada de
//! apresentação com `cardeal-pdf`).
//!
//! Não confundir com o "orçamento" **interno** de [`mod_os`] (peça + mão de obra amarrado à
//! máquina de estados da OS): aqui o orçamento é um documento comercial autônomo, com sua
//! própria numeração e ciclo `Rascunho → Enviado → Aprovado | Recusado | Expirado`, e um
//! aprovado pode ser **convertido** numa OS ([`ConverterOrcamentoEmOs`]).
//!
//! ## O que este crate contém
//!
//! Domínio puro: [`Orcamento`]/[`EstadoOrcamento`] e [`ItemOrcamento`] (com o cálculo de
//! total/desconto), [`erros`] e [`eventos`]. E a amarração ao motor ([`ModuloOrcamentos`]):
//! migrações (`orcamentos_orcamento`/`orcamentos_item`), oito comandos e três consultas.
//!
//! [`ConverterOrcamentoEmOs`] chama [`mod_os`] **direto, na mesma transação** — mesmo padrão
//! de `mod_os::FaturarOrdemServico` chamando `mod_financeiro` (`docs/contratos-internos.md`
//! §7 regra 2): a dependência é só num sentido (`mod-os` não conhece `mod-orcamentos`).

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)]

mod comandos;
mod consultas;
mod erros;
pub mod eventos;
mod item;
mod manifesto;
pub mod migracoes;
mod modulo;
mod orcamento;
mod repositorio;

pub use comandos::{
    CancelarOrcamento, ConverterOrcamentoEmOs, CriarOrcamento, DefinirItensOrcamento,
    DuplicarOrcamento, EditarOrcamento, EnviarOrcamento, NovoItemOrcamento, OrcamentoConvertido,
    OrcamentoCriado, RegistrarDecisaoOrcamento,
};
pub use consultas::{
    BuscarOrcamento, DetalheOrcamento, ItemOrcamentoLista, OrcamentosRecentes, ResumoOrcamentos,
    ResumoOrcamentosSaida,
};
pub use erros::ErroOrcamentos;
pub use item::ItemOrcamento;
pub use manifesto::{manifesto, MANIFESTO};
pub use modulo::ModuloOrcamentos;
pub use orcamento::{EstadoOrcamento, Orcamento};
pub use repositorio::RepositorioOrcamentos;
