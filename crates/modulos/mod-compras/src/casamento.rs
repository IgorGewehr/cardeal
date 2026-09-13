//! O casamento de produto em cascata (`docs/modulos/compras.md` §5, "o gargalo real").
//!
//! Ordem de tentativa: (1) GTIN/EAN do item (`det/prod/cEAN` do XML) batendo com
//! `estoque_produto.codigo_barras` — casamento certo, mais forte que a regra aprendida
//! porque é o próprio fabricante identificando o produto, não uma inferência por
//! fornecedor+código; (2) `RegraCasamentoAprendida` — fornecedor + código do fornecedor já
//! visto antes, casamento certo; (3) NCM igual + similaridade de descrição ≥ 0,82 —
//! sugestão forte, não confirma sozinha; (4) nada — o item fica `NaoCasado` para vínculo
//! manual, que grava uma regra nova e nunca mais pergunta.
//!
//! GTIN entrou nesta fatia porque `estoque_produto.codigo_barras` (o que este arquivo previa
//! como "fora desta fatia, até existir") já existe: `mod_estoque::Produto::codigo_barras`
//! junto com `RepositorioEstoque::buscar_produto_por_codigo_barras`, com índice
//! `UNIQUE(empresa, codigo_barras)`. Quem resolve o GTIN pelo repositório de estoque é o
//! chamador (`crate::comandos::montar_item_casado`); esta função só decide a prioridade
//! entre os candidatos já resolvidos.

use cardeal_kernel::texto;
use cardeal_kernel::{Id, Instante};

/// O limiar de similaridade de descrição que caracteriza uma sugestão forte
/// (`docs/modulos/compras.md` §5).
pub const LIMIAR_SUGESTAO_FORTE: f32 = 0.82;

/// Uma regra de casamento aprendida: `(empresa, fornecedor, codigo_fornecedor)` → produto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegraCasamentoAprendida {
    /// A empresa.
    pub empresa: Id,
    /// O fornecedor.
    pub fornecedor: Id,
    /// O código do item no cadastro do fornecedor.
    pub codigo_fornecedor: String,
    /// O produto do estoque correspondente.
    pub produto: Id,
    /// Quando foi aprendida.
    pub aprendido_em: Instante,
}

/// O resultado da cascata para um item.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultadoCasamento {
    /// Casamento certo (regra aprendida).
    Certo(Id),
    /// Sugestão forte (NCM + similaridade) — precisa de confirmação humana.
    Sugestao(Id),
    /// Nenhuma correspondência.
    Nenhum,
}

/// Roda a cascata de casamento para um item.
///
/// `produto_por_gtin` é o que `RepositorioEstoque::buscar_produto_por_codigo_barras` devolveu
/// para o `cEAN` do item (`None` se o item não trouxe GTIN, ou nenhum produto do estoque tem
/// esse código cadastrado). `regra_aprendida` é o que
/// `RepositorioCompras::buscar_regra_casamento` devolveu para `(empresa, fornecedor,
/// codigo_fornecedor)`. `candidatos_mesmo_ncm` é a lista de produtos do estoque com o mesmo
/// NCM do item, para a etapa de similaridade — o chamador filtra por NCM antes de chamar
/// (evita comparar contra o catálogo inteiro).
#[must_use]
pub fn casar(
    produto_por_gtin: Option<Id>,
    regra_aprendida: Option<Id>,
    descricao_fornecedor: &str,
    candidatos_mesmo_ncm: &[(Id, &str)],
) -> ResultadoCasamento {
    if let Some(produto) = produto_por_gtin {
        return ResultadoCasamento::Certo(produto);
    }
    if let Some(produto) = regra_aprendida {
        return ResultadoCasamento::Certo(produto);
    }

    let mut melhor: Option<(Id, f32)> = None;
    for (produto, nome_produto) in candidatos_mesmo_ncm {
        let score = texto::similaridade(descricao_fornecedor, nome_produto);
        if melhor.is_none_or(|(_, atual)| score > atual) {
            melhor = Some((*produto, score));
        }
    }

    match melhor {
        Some((produto, score)) if score >= LIMIAR_SUGESTAO_FORTE => {
            ResultadoCasamento::Sugestao(produto)
        }
        _ => ResultadoCasamento::Nenhum,
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn gtin_vence_ate_a_regra_aprendida() {
        let por_gtin = Id::novo();
        let por_regra = Id::novo();
        let resultado = casar(Some(por_gtin), Some(por_regra), "qualquer coisa", &[]);
        assert_eq!(resultado, ResultadoCasamento::Certo(por_gtin));
    }

    #[test]
    fn regra_aprendida_vence_sobre_similaridade() {
        let produto = Id::novo();
        let resultado = casar(None, Some(produto), "qualquer coisa", &[]);
        assert_eq!(resultado, ResultadoCasamento::Certo(produto));
    }

    #[test]
    fn descricao_parecida_gera_sugestao_forte() {
        // O limiar de 0,82 é exigente de propósito (o próprio doctest de
        // `cardeal_kernel::texto::similaridade` mostra "Coca-Cola 2L" x "COCA COLA 2 L"
        // batendo só acima de 0,5) — só maiúsculas/minúsculas e acento (que `chave_busca`
        // normaliza por completo, sem mexer em espaço) garante ficar acima do limiar sem
        // depender de calibrar o algoritmo de similaridade aqui.
        let produto = Id::novo();
        let candidatos = [(produto, "Refrigerante Cola 2L")];
        let resultado = casar(None, None, "REFRIGERANTE COLA 2L", &candidatos);
        assert!(matches!(resultado, ResultadoCasamento::Sugestao(p) if p == produto));
    }

    #[test]
    fn descricao_diferente_nao_sugere_nada() {
        let candidatos = [(Id::novo(), "Parafuso sextavado M6")];
        let resultado = casar(None, None, "REFRIGERANTE GUARANA LATA", &candidatos);
        assert_eq!(resultado, ResultadoCasamento::Nenhum);
    }
}
