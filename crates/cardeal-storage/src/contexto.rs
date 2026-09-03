//! Contexto de uma escrita e o invólucro do resultado confirmado.

use cardeal_kernel::{ChaveIdempotencia, Id, Instante, Versao};

/// Quem está escrevendo, de onde, e quando — carregado em toda unidade de trabalho.
/// Ver `docs/contratos-internos.md` §2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextoEscrita {
    /// A empresa.
    pub empresa: Id,
    /// O usuário responsável.
    pub usuario: Id,
    /// O dispositivo de origem.
    pub dispositivo: Id,
    /// A sessão.
    pub sessao: Id,
    /// O instante da unidade de trabalho (relógio real ou controlado em teste).
    pub agora: Instante,
    /// A chave de idempotência do comando, quando houver.
    pub chave: Option<ChaveIdempotencia>,
    /// O id de correlação, para rastrear a operação ponta a ponta.
    pub correlacao: Id,
}

impl ContextoEscrita {
    /// Um contexto mínimo com `agora` e correlação preenchidos automaticamente.
    #[must_use]
    pub fn novo(empresa: Id, usuario: Id, dispositivo: Id, sessao: Id) -> Self {
        Self {
            empresa,
            usuario,
            dispositivo,
            sessao,
            agora: Instante::agora(),
            chave: None,
            correlacao: Id::novo(),
        }
    }

    /// Fixa a chave de idempotência.
    #[must_use]
    pub const fn com_chave(mut self, chave: ChaveIdempotencia) -> Self {
        self.chave = Some(chave);
        self
    }

    /// Fixa o instante (para testes com relógio controlado).
    #[must_use]
    pub const fn em(mut self, agora: Instante) -> Self {
        self.agora = agora;
        self
    }
}

/// O resultado de uma escrita confirmada: o valor produzido e a versão global após o commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Confirmado<T> {
    /// O valor devolvido pela unidade de trabalho.
    pub valor: T,
    /// A versão global do armazenamento imediatamente após o commit deste lote.
    pub versao: Versao,
}

impl<T> Confirmado<T> {
    /// Descarta a versão e devolve só o valor.
    pub fn valor(self) -> T {
        self.valor
    }
}
