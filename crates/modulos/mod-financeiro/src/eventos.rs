//! Os eventos de domínio que o financeiro publica na outbox (`docs/modulos/financeiro.md`
//! §8). Cada um vai para `nucleo_outbox` **na mesma transação** do comando, via
//! [`UnidadeDeTrabalho::publicar`](cardeal_storage::UnidadeDeTrabalho::publicar).
//!
//! Nomes versionados (`"financeiro.<substantivo>_<participio>.v1"`) — assinantes casam pelo
//! nome, nunca pelo tipo Rust.

use cardeal_kernel::{Dinheiro, Id};
use cardeal_storage::EventoDominio;
use serde::Serialize;

/// Um título (a receber ou a pagar) foi lançado, com todas as suas parcelas.
#[derive(Debug, Serialize)]
pub struct TituloLancado {
    /// O título.
    pub titulo: Id,
    /// `"Receber"` ou `"Pagar"`.
    pub especie: &'static str,
    /// O valor original (soma das parcelas).
    pub valor: Dinheiro,
    /// Quantas parcelas.
    pub parcelas: u16,
}

impl EventoDominio for TituloLancado {
    const TIPO: &'static str = "financeiro.titulo_lancado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.titulo)
    }
}

/// Uma parcela recebeu baixa (total ou parcial).
#[derive(Debug, Serialize)]
pub struct ParcelaBaixada {
    /// A parcela baixada.
    pub parcela: Id,
    /// O título dono.
    pub titulo: Id,
    /// O dinheiro que efetivamente entrou (principal + juros + multa − desconto).
    pub valor_recebido: Dinheiro,
    /// Verdadeiro se a baixa quitou a parcela.
    pub quitada: bool,
    /// O lançamento `Realizado` gerado.
    pub lancamento: Id,
}

impl EventoDominio for ParcelaBaixada {
    const TIPO: &'static str = "financeiro.parcela_baixada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.titulo)
    }
}
