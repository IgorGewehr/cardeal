//! Paginação por **cursor** (keyset), nunca `OFFSET` — `docs/09-protocolo-api.md` §5.
//!
//! O cursor carrega as chaves de ordenação da última linha entregue; a página seguinte começa
//! *depois* dela (`WHERE (nome, id) > (?, ?)`). Custo constante na página 1 e na 1000, e
//! estável com inserções concorrentes (uma linha nova não empurra nem duplica as outras).
//!
//! Para o cliente o cursor é opaco. Um cursor adulterado vira `VALOR_INVALIDO` — as chaves
//! sempre entram no SQL como parâmetros, nunca como texto.

use cardeal_kernel::{CodigoErro, Erro, Resultado};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// A posição depois da qual a próxima página começa. Opaco para o cliente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cursor(Vec<u8>);

impl Cursor {
    /// Um cursor a partir das chaves de ordenação da última linha.
    ///
    /// # Panics
    /// Nunca: as chaves são tipos simples do domínio (texto, número, `Id`).
    #[must_use]
    pub fn de<K: Serialize>(chave: &K) -> Self {
        Self(postcard::to_stdvec(chave).expect("chave de ordenação serializa"))
    }

    /// As chaves de volta.
    ///
    /// # Errors
    /// `VALOR_INVALIDO` se o cursor não é de uma página desta consulta.
    pub fn chave<K: DeserializeOwned>(&self) -> Resultado<K> {
        postcard::from_bytes(&self.0).map_err(|_| {
            Erro::novo(CodigoErro::VALOR_INVALIDO, "cursor de página inválido").no_campo("pagina")
        })
    }
}

/// O pedido de uma página.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedidoPagina {
    /// Começar depois desta posição; `None` = primeira página.
    pub apos: Option<Cursor>,
    /// Quantos itens; `0` = o padrão. Nunca passa de [`PedidoPagina::TETO`].
    pub limite: u16,
}

impl PedidoPagina {
    /// Itens por página quando o cliente não diz.
    pub const PADRAO: u16 = 60;
    /// O máximo por página, não importa o que o cliente peça.
    pub const TETO: u16 = 500;

    /// A primeira página, com `limite` itens.
    #[must_use]
    pub const fn primeira(limite: u16) -> Self {
        Self { apos: None, limite }
    }

    /// O limite aplicado: o padrão se zero, nunca acima do teto.
    #[must_use]
    pub const fn limite_efetivo(&self) -> usize {
        let l = if self.limite == 0 {
            Self::PADRAO
        } else {
            self.limite
        };
        (if l > Self::TETO { Self::TETO } else { l }) as usize
    }

    /// Quantas linhas pedir ao SQL: uma a mais que o limite, para saber se há próxima página
    /// sem contar a tabela.
    #[must_use]
    pub fn linhas_sql(&self) -> i64 {
        i64::try_from(self.limite_efetivo() + 1).unwrap_or(i64::MAX)
    }

    /// As chaves do cursor, se não é a primeira página.
    ///
    /// # Errors
    /// `VALOR_INVALIDO` para um cursor adulterado.
    pub fn chave<K: DeserializeOwned>(&self) -> Resultado<Option<K>> {
        self.apos.as_ref().map(Cursor::chave).transpose()
    }
}

/// Uma página de resultados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pagina<T> {
    /// Os itens, na ordem da consulta.
    pub itens: Vec<T>,
    /// De onde continuar; `None` = acabou.
    pub proximo: Option<Cursor>,
    /// Quantos itens há no total com o mesmo filtro — só na primeira página (contar custa).
    pub total: Option<u64>,
}

impl<T> Pagina<T> {
    /// Monta a página a partir de até `limite + 1` linhas ([`PedidoPagina::linhas_sql`]): a
    /// linha a mais só prova que existe uma próxima página, e não é entregue.
    pub fn montar<K: Serialize>(
        mut linhas: Vec<T>,
        pedido: &PedidoPagina,
        chave: impl Fn(&T) -> K,
        total: Option<u64>,
    ) -> Self {
        let limite = pedido.limite_efetivo();
        let ha_mais = linhas.len() > limite;
        linhas.truncate(limite);
        let proximo = if ha_mais {
            linhas.last().map(|u| Cursor::de(&chave(u)))
        } else {
            None
        };
        Self {
            itens: linhas,
            proximo,
            total,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn limite_tem_padrao_e_teto() {
        assert_eq!(PedidoPagina::default().limite_efetivo(), 60);
        assert_eq!(PedidoPagina::primeira(10).limite_efetivo(), 10);
        assert_eq!(PedidoPagina::primeira(60_000).limite_efetivo(), 500);
        assert_eq!(PedidoPagina::primeira(10).linhas_sql(), 11);
    }

    #[test]
    fn montar_entrega_o_limite_e_aponta_a_ultima_linha() {
        let pedido = PedidoPagina::primeira(3);
        let p = Pagina::montar(vec![1, 2, 3, 4], &pedido, |n| *n, Some(10));
        assert_eq!(p.itens, [1, 2, 3]);
        assert_eq!(p.proximo.unwrap().chave::<i32>().unwrap(), 3);
        let fim = Pagina::montar(vec![7, 8], &pedido, |n| *n, None);
        assert!(fim.proximo.is_none());
    }

    #[test]
    fn cursor_adulterado_e_recusado() {
        let p = PedidoPagina {
            apos: Some(Cursor(vec![0xff, 0xff, 0xff, 0xff, 0xff, 0xff])),
            limite: 10,
        };
        let e = p.chave::<(String, [u8; 16])>().unwrap_err();
        assert_eq!(e.codigo, CodigoErro::VALOR_INVALIDO);
    }
}
