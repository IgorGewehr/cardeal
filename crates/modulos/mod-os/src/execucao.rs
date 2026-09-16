//! Os itens de um orçamento de OS: peça e mão de obra.
//!
//! `docs/modulos/os.md` §3. `ItemPeca` nasce com `custo_unitario` zerado e `aplicada =
//! false` — o custo real só é conhecido quando `AplicarPeca` consome o estoque de verdade
//! (`docs/modulos/os.md` §5: "grava `custo_unitario` devolvido"); até lá, o item é só a
//! cotação do orçamento. `coberto_garantia` fica sempre `false` nesta versão — só
//! `AcionarGarantia` (ainda não implementado) o liga.

use cardeal_kernel::{Arredondamento, Dinheiro, Id, Preco, Quantidade};
use serde::{Deserialize, Serialize};

use crate::erros::ErroOs;

/// Uma peça no orçamento de uma ordem de serviço.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPeca {
    /// Identidade.
    pub id: Id,
    /// A ordem de serviço dona.
    pub ordem_servico: Id,
    /// O produto (peça) do estoque.
    pub produto: Id,
    /// A quantidade orçada.
    pub quantidade: Quantidade,
    /// O preço cobrado do cliente por unidade.
    pub preco_unitario: Preco,
    /// O custo unitário real, preenchido só quando a peça é aplicada.
    pub custo_unitario: Preco,
    /// Se coberta por garantia (não gera receita quando faturada).
    pub coberto_garantia: bool,
    /// Se já foi de fato consumida do estoque (`AplicarPeca`).
    pub aplicada: bool,
    /// O local de estoque de onde a peça foi consumida — preenchido só quando aplicada;
    /// é o que permite devolver a peça ao estoque certo se a OS for cancelada depois
    /// (`docs/modulos/os.md` §11 regra 5).
    pub local: Option<Id>,
    /// O lote/peça rastreável específico consumido (o código do post-it), quando o técnico
    /// identificou qual unidade física aplicou — `None` quando a aplicação foi só pelo saldo
    /// agregado do produto, sem rastro individual.
    pub lote: Option<Id>,
    /// Se o consumo desta peça já foi estornado (devolvido ao estoque) por um cancelamento
    /// de OS depois de aplicada — nunca `true` sem `aplicada` também `true`.
    pub estornada: bool,
}

impl ItemPeca {
    /// Orça uma peça (ainda não aplicada ao estoque).
    #[must_use]
    pub fn novo(
        ordem_servico: Id,
        produto: Id,
        quantidade: Quantidade,
        preco_unitario: Preco,
    ) -> Self {
        Self {
            id: Id::novo(),
            ordem_servico,
            produto,
            quantidade,
            preco_unitario,
            custo_unitario: Preco::ZERO,
            coberto_garantia: false,
            aplicada: false,
            local: None,
            lote: None,
            estornada: false,
        }
    }

    /// O total cobrado do cliente por este item (zero se coberto por garantia).
    #[must_use]
    pub fn total_cobrado(&self) -> Dinheiro {
        if self.coberto_garantia {
            return Dinheiro::ZERO;
        }
        Dinheiro::de_total(
            self.quantidade,
            self.preco_unitario,
            Arredondamento::MeioAcima,
        )
    }

    /// O custo real deste item, já aplicado (zero se ainda não foi aplicado, ou se o consumo
    /// já foi estornado — um cancelamento de OS não deixa custo fantasma).
    #[must_use]
    pub fn total_custo(&self) -> Dinheiro {
        if !self.aplicada || self.estornada {
            return Dinheiro::ZERO;
        }
        Dinheiro::de_total(
            self.quantidade,
            self.custo_unitario,
            Arredondamento::MeioAcima,
        )
    }

    /// Marca a peça como aplicada, gravando o custo unitário real devolvido pelo estoque, o
    /// local de onde saiu (para poder devolver depois, se a OS for cancelada) e o
    /// lote/peça rastreável específico consumido, quando o técnico identificou um.
    ///
    /// # Errors
    /// [`ErroOs::PecaJaAplicada`] se já havia sido aplicada.
    pub fn aplicar(
        &mut self,
        custo_unitario: Preco,
        local: Id,
        lote: Option<Id>,
    ) -> Result<(), ErroOs> {
        if self.aplicada {
            return Err(ErroOs::PecaJaAplicada);
        }
        self.custo_unitario = custo_unitario;
        self.aplicada = true;
        self.local = Some(local);
        self.lote = lote;
        Ok(())
    }

    /// Estorna o consumo desta peça (devolve ao estoque) — chamado por
    /// `CancelarOrdemServico` quando a OS é cancelada depois de peças já aplicadas
    /// (`docs/modulos/os.md` §11 regra 5: "peça já aplicada precisa de estorno explícito").
    ///
    /// # Errors
    /// [`ErroOs::PecaNaoAplicadaOuJaEstornada`] se a peça nunca foi aplicada, ou já foi
    /// estornada antes.
    pub fn estornar(&mut self) -> Result<(), ErroOs> {
        if !self.aplicada || self.estornada {
            return Err(ErroOs::PecaNaoAplicadaOuJaEstornada);
        }
        self.estornada = true;
        Ok(())
    }
}

/// Um item de mão de obra no orçamento de uma ordem de serviço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemMaoDeObra {
    /// Identidade.
    pub id: Id,
    /// A ordem de serviço dona.
    pub ordem_servico: Id,
    /// Descrição do serviço ("Diagnóstico + troca de bateria").
    pub descricao: String,
    /// O valor cobrado.
    pub valor: Dinheiro,
    /// O técnico que prestou o serviço.
    pub tecnico: Id,
    /// Horas trabalhadas, quando registradas.
    pub horas: Option<Quantidade>,
}

impl ItemMaoDeObra {
    /// Registra um item de mão de obra.
    ///
    /// # Errors
    /// [`ErroOs::DescricaoDeServicoVazia`].
    pub fn novo(
        ordem_servico: Id,
        descricao: impl Into<String>,
        valor: Dinheiro,
        tecnico: Id,
        horas: Option<Quantidade>,
    ) -> Result<Self, ErroOs> {
        let descricao = descricao.into().trim().to_string();
        if descricao.is_empty() {
            return Err(ErroOs::DescricaoDeServicoVazia);
        }
        Ok(Self {
            id: Id::novo(),
            ordem_servico,
            descricao,
            valor,
            tecnico,
            horas,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn item_peca_cobra_ate_aplicar_o_custo() {
        let mut item = ItemPeca::novo(
            Id::novo(),
            Id::novo(),
            Quantidade::unidades(2),
            Preco::reais(160),
        );
        assert_eq!(item.total_cobrado(), Dinheiro::reais(320));
        assert_eq!(item.total_custo(), Dinheiro::ZERO);

        let local = Id::novo();
        item.aplicar(Preco::reais(90), local, None).unwrap();
        assert!(item.aplicada);
        assert_eq!(item.local, Some(local));
        assert_eq!(item.total_custo(), Dinheiro::reais(180));
        assert_eq!(
            item.aplicar(Preco::reais(90), local, None).unwrap_err(),
            ErroOs::PecaJaAplicada
        );
    }

    #[test]
    fn item_peca_estornada_nao_conta_custo() {
        let mut item = ItemPeca::novo(
            Id::novo(),
            Id::novo(),
            Quantidade::unidades(1),
            Preco::reais(100),
        );
        assert_eq!(
            item.estornar().unwrap_err(),
            ErroOs::PecaNaoAplicadaOuJaEstornada
        );

        let lote = Id::novo();
        item.aplicar(Preco::reais(70), Id::novo(), Some(lote))
            .unwrap();
        assert_eq!(item.lote, Some(lote));
        assert_eq!(item.total_custo(), Dinheiro::reais(70));

        item.estornar().unwrap();
        assert!(item.estornada);
        assert_eq!(item.total_custo(), Dinheiro::ZERO);
        assert_eq!(
            item.estornar().unwrap_err(),
            ErroOs::PecaNaoAplicadaOuJaEstornada
        );
    }

    #[test]
    fn item_peca_coberto_por_garantia_nao_cobra() {
        let mut item = ItemPeca::novo(
            Id::novo(),
            Id::novo(),
            Quantidade::unidades(1),
            Preco::reais(320),
        );
        item.coberto_garantia = true;
        assert_eq!(item.total_cobrado(), Dinheiro::ZERO);
    }

    #[test]
    fn descricao_de_servico_vazia_e_recusada() {
        let erro = ItemMaoDeObra::novo(Id::novo(), "  ", Dinheiro::reais(80), Id::novo(), None)
            .unwrap_err();
        assert_eq!(erro, ErroOs::DescricaoDeServicoVazia);
    }
}
