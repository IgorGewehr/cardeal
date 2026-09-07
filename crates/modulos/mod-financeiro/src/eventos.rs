//! Os eventos de domínio que o financeiro publica na outbox (`docs/modulos/financeiro.md`
//! §8). Cada um vai para `nucleo_outbox` **na mesma transação** do comando, via
//! [`UnidadeDeTrabalho::publicar`](cardeal_storage::UnidadeDeTrabalho::publicar).
//!
//! Nomes versionados (`"financeiro.<substantivo>_<participio>.v1"`) — assinantes casam pelo
//! nome, nunca pelo tipo Rust.

use cardeal_kernel::{Data, Dinheiro, Id};
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

/// Uma baixa foi estornada — a parcela reabriu.
#[derive(Debug, Serialize)]
pub struct BaixaEstornada {
    /// A baixa estornada.
    pub baixa: Id,
    /// A parcela reaberta.
    pub parcela: Id,
    /// O lançamento de estorno gerado.
    pub lancamento_estorno: Id,
}

impl EventoDominio for BaixaEstornada {
    const TIPO: &'static str = "financeiro.baixa_estornada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.parcela)
    }
}

/// O saldo em aberto de um título foi renegociado num novo título.
#[derive(Debug, Serialize)]
pub struct TituloRenegociado {
    /// O título original.
    pub titulo_original: Id,
    /// O novo título, com as novas condições.
    pub titulo_novo: Id,
    /// O saldo consolidado nas novas condições.
    pub saldo: Dinheiro,
}

impl EventoDominio for TituloRenegociado {
    const TIPO: &'static str = "financeiro.titulo_renegociado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.titulo_original)
    }
}

/// Uma sessão de caixa foi aberta.
#[derive(Debug, Serialize)]
pub struct CaixaAberto {
    /// A sessão criada.
    pub sessao: Id,
    /// O caixa físico.
    pub caixa: Id,
    /// Quem abriu.
    pub operador: Id,
    /// O suprimento inicial (pode ser zero).
    pub valor_abertura: Dinheiro,
}

impl EventoDominio for CaixaAberto {
    const TIPO: &'static str = "financeiro.caixa_aberto.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.sessao)
    }
}

/// Uma sessão de caixa foi fechada (fechamento cego).
#[derive(Debug, Serialize)]
pub struct CaixaFechado {
    /// A sessão fechada.
    pub sessao: Id,
    /// O saldo que o sistema esperava, revelado só depois da contagem.
    pub valor_esperado: Dinheiro,
    /// O valor contado pelo operador.
    pub valor_contado: Dinheiro,
    /// `valor_contado - valor_esperado`; negativo = falta.
    pub quebra: Dinheiro,
}

impl EventoDominio for CaixaFechado {
    const TIPO: &'static str = "financeiro.caixa_fechado.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.sessao)
    }
}

/// Uma sangria foi registrada numa sessão de caixa.
#[derive(Debug, Serialize)]
pub struct SangriaRegistrada {
    /// A sessão de origem.
    pub sessao: Id,
    /// O valor retirado.
    pub valor: Dinheiro,
    /// O motivo informado.
    pub motivo: String,
}

impl EventoDominio for SangriaRegistrada {
    const TIPO: &'static str = "financeiro.sangria_registrada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.sessao)
    }
}

/// Uma ocorrência de [`crate::Recorrencia`] virou `Titulo` real.
#[derive(Debug, Serialize)]
pub struct RecorrenciaMaterializada {
    /// A regra de recorrência.
    pub recorrencia: Id,
    /// O título gerado.
    pub titulo: Id,
    /// O vencimento desta ocorrência.
    pub vencimento: Data,
    /// O valor gerado.
    pub valor: Dinheiro,
}

impl EventoDominio for RecorrenciaMaterializada {
    const TIPO: &'static str = "financeiro.recorrencia_materializada.v1";

    fn agregado(&self) -> Option<Id> {
        Some(self.recorrencia)
    }
}
