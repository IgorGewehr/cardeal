//! Tabela de preço e resolução do preço vigente.
//!
//! `docs/modulos/vendas.md` §3 e §11.1: o preço é **congelado no item** no momento em que
//! é adicionado — alterar a `RegraPreco` depois não muda pedidos já criados. Domínio puro.

use cardeal_kernel::{Data, Id, Preco, Quantidade, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroVendas;

/// O tipo de uma [`TabelaPreco`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoTabela {
    /// Preço de balcão/varejo.
    Venda,
    /// Preço de atacado.
    Atacado,
    /// Promoção com vigência.
    Promocional,
}

/// Uma tabela de preço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TabelaPreco {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O nome.
    pub nome: String,
    /// O tipo.
    pub tipo: TipoTabela,
    /// Início da vigência.
    pub vigente_de: Data,
    /// Fim da vigência (aberta se `None`).
    pub vigente_ate: Option<Data>,
    /// Se está ativa.
    pub ativa: bool,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl TabelaPreco {
    /// Verdadeiro se a tabela está ativa e vigente na data.
    #[must_use]
    pub fn vigente_em(&self, data: Data) -> bool {
        self.ativa && self.vigente_de <= data && self.vigente_ate.is_none_or(|ate| data <= ate)
    }
}

/// A que a [`RegraPreco`] se aplica: um produto específico ou um grupo inteiro.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlvoRegra {
    /// Um produto.
    Produto(Id),
    /// Um grupo de produtos.
    Grupo(Id),
}

/// Uma regra de preço dentro de uma tabela.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegraPreco {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// A tabela dona.
    pub tabela_preco: Id,
    /// A que se aplica (produto **xor** grupo).
    pub alvo: AlvoRegra,
    /// Quantidade mínima para a regra valer (faixa de atacado). `None` = qualquer.
    pub quantidade_minima: Option<Quantidade>,
    /// O preço unitário.
    pub preco: Preco,
    /// Início do período (só em tabela `Promocional`).
    pub periodo_de: Option<Data>,
    /// Fim do período (só em tabela `Promocional`).
    pub periodo_ate: Option<Data>,
}

impl RegraPreco {
    /// Verdadeiro se a regra cobre o produto/grupo, a quantidade e a data informados.
    #[must_use]
    pub fn cobre(&self, produto: Id, grupo: Id, quantidade: Quantidade, data: Data) -> bool {
        let alvo_ok = match self.alvo {
            AlvoRegra::Produto(p) => p == produto,
            AlvoRegra::Grupo(g) => g == grupo,
        };
        let qtd_ok = self.quantidade_minima.is_none_or(|min| quantidade >= min);
        let periodo_ok = self.periodo_de.is_none_or(|de| data >= de)
            && self.periodo_ate.is_none_or(|ate| data <= ate);
        alvo_ok && qtd_ok && periodo_ok
    }

    /// A especificidade da regra, para desempate: uma promoção com janela de vigência vence
    /// a regra permanente; regra por produto vence por grupo; faixa de quantidade maior
    /// vence a de menor. O `id` entra por último só para desempatar duas regras
    /// **igualmente específicas** (cadastro duplicado) de forma determinística — como `Id` é
    /// `UUIDv7` (ordenado no tempo), a mais recente vence, em vez de depender da ordem de
    /// leitura do SQLite (`regras_da_tabela` não tem `ORDER BY`).
    fn especificidade(&self) -> (u8, u8, i64, Id) {
        let promo = u8::from(self.periodo_de.is_some() || self.periodo_ate.is_some());
        let alvo = match self.alvo {
            AlvoRegra::Produto(_) => 1,
            AlvoRegra::Grupo(_) => 0,
        };
        let faixa = self
            .quantidade_minima
            .map_or(0, Quantidade::unidades_internas);
        (promo, alvo, faixa, self.id)
    }
}

/// Resolve o preço vigente de um item, entre as `regras` da `tabela`.
///
/// A regra escolhida é a **mais específica que cobre** a compra: por produto vence por
/// grupo, e faixa de quantidade maior vence a menor quando a quantidade a atinge
/// (`docs/modulos/vendas.md` §3, `RegraPreco`).
///
/// # Errors
/// - [`ErroVendas::PrecoNaoEncontrado`] se a tabela não está vigente ou nenhuma regra cobre.
pub fn preco_vigente(
    tabela: &TabelaPreco,
    regras: &[RegraPreco],
    produto: Id,
    grupo: Id,
    quantidade: Quantidade,
    data: Data,
) -> Result<Preco, ErroVendas> {
    if !tabela.vigente_em(data) {
        return Err(ErroVendas::PrecoNaoEncontrado);
    }
    regras
        .iter()
        .filter(|r| r.tabela_preco == tabela.id && r.cobre(produto, grupo, quantidade, data))
        .max_by_key(|r| r.especificidade())
        .map(|r| r.preco)
        .ok_or(ErroVendas::PrecoNaoEncontrado)
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Fuso;

    use super::*;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn tabela() -> TabelaPreco {
        TabelaPreco {
            id: Id::novo(),
            empresa: Id::novo(),
            nome: "Atacado".into(),
            tipo: TipoTabela::Atacado,
            vigente_de: hoje().mais_dias(-10),
            vigente_ate: None,
            ativa: true,
            versao: Versao::INICIAL,
        }
    }

    fn regra(tabela: Id, alvo: AlvoRegra, qtd_min: Option<i64>, preco: i64) -> RegraPreco {
        RegraPreco {
            id: Id::novo(),
            empresa: Id::novo(),
            tabela_preco: tabela,
            alvo,
            quantidade_minima: qtd_min.map(Quantidade::unidades),
            preco: Preco::reais(preco),
            periodo_de: None,
            periodo_ate: None,
        }
    }

    #[test]
    fn faixa_de_quantidade_maior_vence() {
        let t = tabela();
        let prod = Id::novo();
        let regras = vec![
            regra(t.id, AlvoRegra::Produto(prod), None, 10),
            regra(t.id, AlvoRegra::Produto(prod), Some(100), 8),
            regra(t.id, AlvoRegra::Produto(prod), Some(500), 7),
        ];
        assert_eq!(
            preco_vigente(
                &t,
                &regras,
                prod,
                Id::novo(),
                Quantidade::unidades(50),
                hoje()
            )
            .unwrap(),
            Preco::reais(10)
        );
        assert_eq!(
            preco_vigente(
                &t,
                &regras,
                prod,
                Id::novo(),
                Quantidade::unidades(120),
                hoje()
            )
            .unwrap(),
            Preco::reais(8)
        );
        assert_eq!(
            preco_vigente(
                &t,
                &regras,
                prod,
                Id::novo(),
                Quantidade::unidades(600),
                hoje()
            )
            .unwrap(),
            Preco::reais(7)
        );
    }

    #[test]
    fn regra_por_produto_vence_regra_por_grupo() {
        let t = tabela();
        let (prod, grupo) = (Id::novo(), Id::novo());
        let regras = vec![
            regra(t.id, AlvoRegra::Grupo(grupo), None, 10),
            regra(t.id, AlvoRegra::Produto(prod), None, 9),
        ];
        assert_eq!(
            preco_vigente(&t, &regras, prod, grupo, Quantidade::unidades(1), hoje()).unwrap(),
            Preco::reais(9)
        );
    }

    #[test]
    fn regras_igualmente_especificas_desempatam_pela_mais_recente_e_de_forma_estavel() {
        let t = tabela();
        let prod = Id::novo();
        let mais_velha = regra(t.id, AlvoRegra::Produto(prod), None, 10);
        let mut mais_nova = regra(t.id, AlvoRegra::Produto(prod), None, 8);
        // `Id::novo()` é UUIDv7 — gerar em sequência já garante `mais_nova > mais_velha`, mas
        // fixamos explicitamente para não depender de timing de teste.
        while mais_nova.id <= mais_velha.id {
            mais_nova.id = Id::novo();
        }

        let em_ordem = vec![mais_velha.clone(), mais_nova.clone()];
        let invertida = vec![mais_nova.clone(), mais_velha.clone()];
        let preco_em_ordem =
            preco_vigente(&t, &em_ordem, prod, Id::novo(), Quantidade::UM, hoje()).unwrap();
        let preco_invertido =
            preco_vigente(&t, &invertida, prod, Id::novo(), Quantidade::UM, hoje()).unwrap();

        assert_eq!(preco_em_ordem, preco_invertido);
        assert_eq!(preco_em_ordem, mais_nova.preco);
    }

    #[test]
    fn tabela_fora_de_vigencia_nao_da_preco() {
        let mut t = tabela();
        t.vigente_ate = Some(hoje().mais_dias(-1));
        let regras = vec![regra(t.id, AlvoRegra::Produto(Id::novo()), None, 5)];
        assert_eq!(
            preco_vigente(&t, &regras, Id::novo(), Id::novo(), Quantidade::UM, hoje()).unwrap_err(),
            ErroVendas::PrecoNaoEncontrado
        );
    }

    #[test]
    fn promocao_so_vale_dentro_do_periodo() {
        let t = tabela();
        let prod = Id::novo();
        let mut promo = regra(t.id, AlvoRegra::Produto(prod), None, 6);
        promo.periodo_de = Some(hoje().mais_dias(1));
        promo.periodo_ate = Some(hoje().mais_dias(5));
        let normal = regra(t.id, AlvoRegra::Produto(prod), None, 10);
        let regras = vec![promo, normal];
        assert_eq!(
            preco_vigente(&t, &regras, prod, Id::novo(), Quantidade::UM, hoje()).unwrap(),
            Preco::reais(10)
        );
        assert_eq!(
            preco_vigente(
                &t,
                &regras,
                prod,
                Id::novo(),
                Quantidade::UM,
                hoje().mais_dias(3)
            )
            .unwrap(),
            Preco::reais(6)
        );
    }
}
