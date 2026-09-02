//! # cardeal-kernel
//!
//! Os tipos fundamentais do Cardeal. Este crate é o vocabulário compartilhado por todo o
//! sistema — do razão ao PDV, do servidor à interface.
//!
//! ## Regras deste crate
//!
//! 1. **Zero I/O.** Nada aqui lê arquivo, abre socket ou fala com banco.
//! 2. **Zero `async`.** Aritmética não precisa de runtime.
//! 3. **Zero dependência pesada.** Só `serde`, `uuid`, `smallvec` e `thiserror`.
//! 4. **Nenhum ponto flutuante em valor monetário.** Em lugar nenhum, nem intermediário.
//!
//! ## Os tipos numéricos e suas escalas
//!
//! | Tipo | Representação | Escala | Motivo |
//! |---|---|---|---|
//! | [`Dinheiro`] | `i64` | centavos (1e-2) | Valor monetário. Exato, soma sem erro. |
//! | [`Quantidade`] | `i64` | 1e-4 | A NF-e permite 4 casas em `qCom`. |
//! | [`Preco`] | `i64` | 1e-6 | Preço unitário (combustível, farmácia, granel). |
//! | [`Percentual`] | `i64` | 1e-6 ponto percentual | Alíquotas, descontos, juros. |
//!
//! Converter entre eles **exige um modo de [`Arredondamento`] explícito**. Não existe
//! conversão implícita, porque cada contexto fiscal arredonda de um jeito e errar isso
//! gera divergência de centavos na nota.
//!
//! ```
//! use cardeal_kernel::{Dinheiro, Quantidade, Preco, Arredondamento};
//!
//! let qtd   = Quantidade::de_str("2,5").unwrap();       // 2,5 kg
//! let preco = Preco::de_str("18,90").unwrap();          // R$ 18,90 / kg
//! let total = Dinheiro::de_total(qtd, preco, Arredondamento::MeioAcima);
//! assert_eq!(total, Dinheiro::centavos(4725));          // R$ 47,25
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
// `Erro` carrega `Detalhes` (resumo, causa, ações sugeridas em português) de propósito —
// é o que transforma uma falha em algo que a UI apresenta como conversa, não como stack
// trace (ver docs/09-protocolo-api.md §3). Isso o deixa maior que os ~128 bytes que o
// clippy::pedantic considera "grande"; boxar aqui só empurraria uma indireção para todo
// call site do projeto sem ganho real, já que erro é o caminho frio.
#![allow(clippy::result_large_err)]

pub mod dinheiro;
pub mod documento;
pub mod erro;
pub mod id;
pub mod percentual;
pub mod quantidade;
pub mod tempo;
pub mod texto;

pub use dinheiro::{Arredondamento, Dinheiro, Sinal};
pub use documento::{Cnpj, Cpf, Documento, InscricaoEstadual, Uf};
pub use erro::{AcaoSugerida, CodigoErro, Detalhes, Erro, ErroDominio, Resultado};
pub use id::{ChaveIdempotencia, Id, Versao};
pub use percentual::Percentual;
pub use quantidade::{Preco, Quantidade, Unidade};
pub use tempo::{Competencia, Data, DiaDaSemana, Fuso, Hora, Instante, Periodo};

/// Reexportações usadas por praticamente todo módulo do sistema.
///
/// ```
/// use cardeal_kernel::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        AcaoSugerida, Arredondamento, ChaveIdempotencia, Cnpj, CodigoErro, Competencia, Cpf, Data,
        Detalhes, DiaDaSemana, Dinheiro, Documento, Erro, ErroDominio, Fuso, Hora, Id,
        InscricaoEstadual, Instante, Percentual, Periodo, Preco, Quantidade, Resultado, Sinal, Uf,
        Unidade, Versao,
    };
}
