//! Transferência entre locais do mesmo CNPJ (`docs/modulos/estoque.md` §11.6): a saída na
//! origem e a entrada no destino são uma escrita só — a mercadoria nunca "some" entre dois
//! locais. Domínio puro: a validação do documento (locais diferentes, itens com quantidade
//! positiva, um produto repetido soma numa linha só).
//!
//! **Fora desta fatia, de propósito:** o local técnico `EmTransito` com confirmação de
//! recebimento. Entre locais do mesmo CNPJ (loja e depósito) a mercadoria muda de lugar na hora;
//! a etapa em trânsito só faz sentido entre filiais, e lá ela vem junto da NF-e de transferência
//! (onda fiscal).

use cardeal_kernel::{Id, Quantidade};
use serde::{Deserialize, Serialize};

use crate::erros::ErroEstoque;

/// Um produto e a quantidade a transferir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemTransferencia {
    /// O produto.
    pub produto: Id,
    /// A quantidade (positiva).
    pub quantidade: Quantidade,
}

/// Uma transferência validada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transferencia {
    /// Identidade.
    pub id: Id,
    /// A empresa (CNPJ).
    pub empresa: Id,
    /// De onde sai.
    pub origem: Id,
    /// Para onde vai.
    pub destino: Id,
    /// Uma observação livre (vazia vira `None`).
    pub observacao: Option<String>,
    /// Os itens, um por produto, na ordem em que apareceram.
    pub itens: Vec<ItemTransferencia>,
}

impl Transferencia {
    /// Valida e monta a transferência.
    ///
    /// # Errors
    /// [`ErroEstoque::LocalIgual`], [`ErroEstoque::TransferenciaSemItens`],
    /// [`ErroEstoque::QuantidadeInvalida`].
    pub fn nova(
        empresa: Id,
        origem: Id,
        destino: Id,
        itens: &[ItemTransferencia],
        observacao: &str,
    ) -> Result<Self, ErroEstoque> {
        if origem == destino {
            return Err(ErroEstoque::LocalIgual);
        }
        if itens.is_empty() {
            return Err(ErroEstoque::TransferenciaSemItens);
        }
        let mut agrupados: Vec<ItemTransferencia> = Vec::with_capacity(itens.len());
        for item in itens {
            if !item.quantidade.e_positiva() {
                return Err(ErroEstoque::QuantidadeInvalida);
            }
            match agrupados.iter_mut().find(|i| i.produto == item.produto) {
                Some(i) => i.quantidade += item.quantidade,
                None => agrupados.push(*item),
            }
        }
        let observacao = Some(observacao.trim().to_owned()).filter(|o| !o.is_empty());
        Ok(Self {
            id: Id::novo(),
            empresa,
            origem,
            destino,
            observacao,
            itens: agrupados,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(produto: Id, qtd: i64) -> ItemTransferencia {
        ItemTransferencia {
            produto,
            quantidade: Quantidade::unidades(qtd),
        }
    }

    #[test]
    fn valida_locais_itens_e_quantidades() {
        let (empresa, loja, deposito, prod) = (Id::novo(), Id::novo(), Id::novo(), Id::novo());
        assert_eq!(
            Transferencia::nova(empresa, loja, loja, &[item(prod, 1)], "").unwrap_err(),
            ErroEstoque::LocalIgual
        );
        assert_eq!(
            Transferencia::nova(empresa, loja, deposito, &[], "").unwrap_err(),
            ErroEstoque::TransferenciaSemItens
        );
        assert_eq!(
            Transferencia::nova(empresa, loja, deposito, &[item(prod, 0)], "").unwrap_err(),
            ErroEstoque::QuantidadeInvalida
        );
        let feita = Transferencia::nova(
            empresa,
            loja,
            deposito,
            &[item(prod, 2)],
            "  reposição da loja ",
        )
        .unwrap();
        assert_eq!(feita.observacao.as_deref(), Some("reposição da loja"));
        assert!(
            Transferencia::nova(empresa, loja, deposito, &[item(prod, 2)], "  ")
                .unwrap()
                .observacao
                .is_none()
        );
    }

    #[test]
    fn produto_repetido_soma_numa_linha() {
        let (p, q) = (Id::novo(), Id::novo());
        let t = Transferencia::nova(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            &[item(p, 2), item(q, 1), item(p, 3)],
            "",
        )
        .unwrap();
        assert_eq!(t.itens, [item(p, 5), item(q, 1)]);
    }
}
