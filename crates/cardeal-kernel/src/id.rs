//! Identidade.
//!
//! Toda entidade do Cardeal é identificada por um [`Id`] — um UUID versão 7, ordenado no
//! tempo. Ver [`ADR-0013`](../../../docs/adr/0013-uuidv7-como-identidade.md).
//!
//! As três propriedades que motivam a escolha:
//!
//! 1. **Um terminal offline gera identidade sem falar com o servidor.** É requisito do
//!    modo autônomo (`docs/03-pilar-resiliencia.md` §3).
//! 2. **É ordenado no tempo.** O índice B-tree cresce à direita, sem a fragmentação que o
//!    UUIDv4 causa — diferença de dezenas de por cento em base grande.
//! 3. **16 bytes fixos**, comparação barata, sem colisão prática.
//!
//! O número que o **usuário** vê (`numero` das tabelas) é outra coisa: um sequencial por
//! empresa, atribuído no servidor no momento do commit.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::erro::{CodigoErro, Erro, Resultado};

/// Identificador de entidade. UUID versão 7.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(Uuid);

impl Id {
    /// O identificador nulo. Usado como sentinela em construção; nunca persistido.
    pub const NULO: Self = Self(Uuid::nil());

    /// Gera um novo identificador a partir do relógio atual.
    #[must_use]
    pub fn novo() -> Self {
        Self(Uuid::now_v7())
    }

    /// Constrói a partir dos 16 bytes, como vêm do banco.
    #[must_use]
    pub const fn de_bytes(bytes: [u8; 16]) -> Self {
        Self(Uuid::from_bytes(bytes))
    }

    /// Os 16 bytes, para gravar no banco.
    #[must_use]
    pub const fn em_bytes(self) -> [u8; 16] {
        *self.0.as_bytes()
    }

    /// Os 16 bytes emprestados.
    #[must_use]
    pub fn como_bytes(&self) -> &[u8; 16] {
        self.0.as_bytes()
    }

    /// Verdadeiro se for o identificador nulo.
    #[must_use]
    pub fn e_nulo(self) -> bool {
        self.0.is_nil()
    }

    /// Lê a partir do texto com hifens.
    ///
    /// # Errors
    /// Devolve [`CodigoErro::ENTRADA_INVALIDA`] se o texto não for um UUID.
    pub fn de_str(texto: &str) -> Resultado<Self> {
        Uuid::parse_str(texto.trim()).map(Self).map_err(|_| {
            Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                format!("\"{texto}\" não é um identificador válido"),
            )
        })
    }

    /// Os oito primeiros caracteres, para exibir em tela e em log sem ocupar 36 colunas.
    #[must_use]
    pub fn curto(self) -> String {
        self.0.simple().to_string()[..8].to_string()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0.hyphenated())
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.curto())
    }
}

impl FromStr for Id {
    type Err = Erro;
    fn from_str(s: &str) -> Resultado<Self> {
        Self::de_str(s)
    }
}

impl From<Uuid> for Id {
    fn from(u: Uuid) -> Self {
        Self(u)
    }
}

impl From<Id> for Uuid {
    fn from(id: Id) -> Self {
        id.0
    }
}

/// Chave de idempotência de um comando.
///
/// Gerada **no cliente**, estável entre retentativas. É o que garante que reenviar uma venda
/// depois de uma queda de rede não cria duas vendas. Ver `docs/03-pilar-resiliencia.md` §2.2.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ChaveIdempotencia(Id);

impl ChaveIdempotencia {
    /// Gera uma chave nova. Chame **uma vez** por intenção do usuário, não por tentativa.
    #[must_use]
    pub fn nova() -> Self {
        Self(Id::novo())
    }

    /// Constrói a partir de um identificador existente.
    #[must_use]
    pub const fn de_id(id: Id) -> Self {
        Self(id)
    }

    /// O identificador subjacente.
    #[must_use]
    pub const fn id(self) -> Id {
        self.0
    }

    /// Os 16 bytes, para gravar no banco.
    #[must_use]
    pub const fn em_bytes(self) -> [u8; 16] {
        self.0.em_bytes()
    }
}

impl fmt::Display for ChaveIdempotencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl fmt::Debug for ChaveIdempotencia {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Chave({})", self.0.curto())
    }
}

/// Versão global do banco, incrementada a cada commit do escritor único.
///
/// Serve a dois propósitos: bloqueio otimista de entidade e leitura consistente
/// ("read your writes") — o cliente que acabou de escrever a versão N só lê de uma conexão
/// que já enxerga N. Ver `docs/07-persistencia-sqlite.md` §5.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Versao(u64);

impl Versao {
    /// Versão zero — entidade ainda não persistida.
    pub const ZERO: Self = Self(0);
    /// Primeira versão de uma entidade recém-criada.
    pub const INICIAL: Self = Self(1);

    /// Constrói a partir do número.
    #[must_use]
    pub const fn nova(v: u64) -> Self {
        Self(v)
    }

    /// O número da versão.
    #[must_use]
    pub const fn numero(self) -> u64 {
        self.0
    }

    /// A próxima versão.
    #[must_use]
    pub const fn proxima(self) -> Self {
        Self(self.0 + 1)
    }
}

impl fmt::Display for Versao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.0)
    }
}

impl fmt::Debug for Versao {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Versao({})", self.0)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ids_sao_ordenados_no_tempo() {
        let mut anteriores = Vec::new();
        for _ in 0..64 {
            let id = Id::novo();
            assert!(!id.e_nulo());
            anteriores.push(id);
        }
        // UUIDv7 gerado em sequência é monotonicamente não decrescente.
        let mut ordenados = anteriores.clone();
        ordenados.sort_unstable();
        assert_eq!(anteriores, ordenados);
    }

    #[test]
    fn ida_e_volta_de_bytes_e_texto() {
        let id = Id::novo();
        assert_eq!(Id::de_bytes(id.em_bytes()), id);
        assert_eq!(Id::de_str(&id.to_string()).unwrap(), id);
        assert_eq!(id.curto().len(), 8);
        assert!(Id::de_str("nao-e-um-uuid").is_err());
    }

    #[test]
    fn versoes() {
        assert_eq!(Versao::INICIAL.proxima(), Versao::nova(2));
        assert!(Versao::ZERO < Versao::INICIAL);
        assert_eq!(Versao::nova(41).to_string(), "v41");
    }
}
