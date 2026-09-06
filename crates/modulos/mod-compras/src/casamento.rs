//! O casamento de produto em cascata (`docs/modulos/compras.md` §5, "o gargalo real").
//!
//! Ordem de tentativa: (1) `RegraCasamentoAprendida` — fornecedor + código do fornecedor já
//! visto antes, casamento certo; (2) NCM igual + similaridade de descrição ≥ 0,82 —
//! sugestão forte, não confirma sozinha; (3) nada — o item fica `NaoCasado` para vínculo
//! manual, que grava uma regra nova e nunca mais pergunta.
//!
//! **Fora desta fatia:** casamento por GTIN/EAN (§5 item 1) — o cadastro de código de barras
//! do estoque (`estoque_codigo_barras`) ainda não foi wired (ver `mod-estoque` "o que
//! falta"); quando existir, entra como a primeira tentativa, antes da regra aprendida.

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

/// Roda a cascata de casamento (menos GTIN — ver o doc do módulo) para um item.
///
/// `regra_aprendida` é o que `RepositorioCompras::buscar_regra_casamento` devolveu para
/// `(empresa, fornecedor, codigo_fornecedor)`. `candidatos_mesmo_ncm` é a lista de produtos
/// do estoque com o mesmo NCM do item, para a etapa de similaridade — o chamador filtra por
/// NCM antes de chamar (evita comparar contra o catálogo inteiro).
#[must_use]
pub fn casar(
    regra_aprendida: Option<Id>,
    descricao_fornecedor: &str,
    candidatos_mesmo_ncm: &[(Id, &str)],
) -> ResultadoCasamento {
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
    fn regra_aprendida_vence_sempre() {
        let produto = Id::novo();
        let resultado = casar(Some(produto), "qualquer coisa", &[]);
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
        let resultado = casar(None, "REFRIGERANTE COLA 2L", &candidatos);
        assert!(matches!(resultado, ResultadoCasamento::Sugestao(p) if p == produto));
    }

    #[test]
    fn descricao_diferente_nao_sugere_nada() {
        let candidatos = [(Id::novo(), "Parafuso sextavado M6")];
        let resultado = casar(None, "REFRIGERANTE GUARANA LATA", &candidatos);
        assert_eq!(resultado, ResultadoCasamento::Nenhum);
    }
}
