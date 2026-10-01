//! # cardeal-motor
//!
//! O motor de **uma** empresa: abre e migra a base, autentica e despacha comandos/consultas
//! pelo `Despachante`. Dois anfitriões o usam: `cardeal-desktop` (monoposto — o motor roda no
//! próprio processo, `docs/adr/0009-transporte-in-process.md`) e `cardeal-server` (uma frota
//! destes, um por empresa, aberta sob demanda — `docs/adr/0016-servidor-multi-tenant-e-cliente-web.md`).
//!
//! Decisão pragmática (anotada, não uma reversão da ADR-0009): em vez do caminho "zero
//! serialização" que a ADR descreve como ideal, o motor chama exatamente
//! `Despachante::executar_comando`/`executar_consulta` (que já serializam com `postcard`
//! internamente). Microssegundos de serialização são irrelevantes para uma ferramenta de
//! balcão, e um único caminho de serialização é o mesmo que a rede usa — sem divergência entre
//! o modo local e o modo servidor.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)] // `Erro`/`ErroArmazenamento` são grandes de propósito (detalhes ao usuário)

mod acesso;
mod empresa;
mod motor;
mod plano;

pub use acesso::{PapelResumo, UsuarioResumo};
pub use empresa::{EmpresaResumo, IdentidadeVisual};
pub use motor::{MotorLocal, SessaoLocal};
pub use plano::Plano;

use cardeal_kernel::{CodigoErro, Erro};
use cardeal_storage::ErroArmazenamento;

fn erro_armazenamento(e: ErroArmazenamento) -> Erro {
    match e {
        ErroArmazenamento::Dominio(e) => e,
        outro => Erro::de_dominio(&outro),
    }
}

fn erro_registro(e: &cardeal_modkit::ErroRegistro) -> Erro {
    Erro::novo(CodigoErro::ESTADO_INVALIDO, e.to_string())
}

/// Encaixa um erro de domínio qualquer (`ErroDominio`) no formato que `Escritor`/`Leitor`
/// esperam, para usar dentro de um fecho de `executar`/`consultar`.
fn como_erro_armazenamento<E: cardeal_kernel::ErroDominio>(e: &E) -> ErroArmazenamento {
    ErroArmazenamento::Dominio(Erro::de_dominio(e))
}
