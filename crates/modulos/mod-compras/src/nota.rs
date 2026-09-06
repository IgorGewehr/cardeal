//! Nota de entrada e seus itens — a peça central do módulo.
//!
//! `docs/modulos/compras.md` §3 e §4. Domínio puro: as transições só validam e mutam
//! `self`; quem chama estoque/financeiro é o comando.

use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Preco, Quantidade, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroCompras;

/// O estado de uma [`NotaEntrada`] (`docs/modulos/compras.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoNotaEntrada {
    /// Acabou de chegar (XML importado); itens ainda sendo casados/conferidos.
    AConferir,
    /// Todos os itens casados e a quantidade validada.
    Conferida,
    /// Entrada confirmada: estoque atualizado, título a pagar gerado (se a preferência
    /// mandar).
    Confirmada,
    /// Devolvida ao fornecedor, total ou parcialmente. Terminal para o valor devolvido.
    Devolvida,
}

impl EstadoNotaEntrada {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::AConferir => "AConferir",
            Self::Conferida => "Conferida",
            Self::Confirmada => "Confirmada",
            Self::Devolvida => "Devolvida",
        }
    }
}

/// O estado do casamento de um [`ItemNotaEntrada`] com um produto do estoque
/// (`docs/modulos/compras.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoCasamento {
    /// Nenhuma correspondência encontrada ainda.
    NaoCasado,
    /// Casamento por NCM + similaridade de descrição — precisa de confirmação humana.
    SugestaoForte,
    /// Casamento certo: por `RegraCasamentoAprendida` ou confirmado manualmente.
    Casado,
}

/// Uma nota de entrada — de um fornecedor, com os itens a conferir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotaEntrada {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O fornecedor (papel `Fornecedor` em `clientes_pessoa`).
    pub fornecedor: Id,
    /// A chave de acesso (44 dígitos), quando veio de XML/DFe.
    pub chave_acesso: Option<String>,
    /// Número da nota.
    pub numero: String,
    /// Série.
    pub serie: String,
    /// Data de emissão.
    pub data_emissao: Data,
    /// Soma dos itens, sem despesas acessórias.
    pub valor_produtos: Dinheiro,
    /// Frete.
    pub valor_frete: Dinheiro,
    /// Seguro.
    pub valor_seguro: Dinheiro,
    /// Outras despesas acessórias.
    pub valor_outras_despesas: Dinheiro,
    /// `valor_produtos + valor_frete + valor_seguro + valor_outras_despesas`.
    pub valor_total: Dinheiro,
    /// O estado atual.
    pub estado: EstadoNotaEntrada,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl NotaEntrada {
    fn transitar(&mut self, novo: EstadoNotaEntrada) {
        self.estado = novo;
        self.versao = self.versao.proxima();
    }

    /// Confirma a entrada: `AConferir`/`Conferida` → `Confirmada`.
    ///
    /// # Errors
    /// [`ErroCompras::NotaJaConfirmada`] se já confirmada ou devolvida.
    pub fn confirmar(&mut self) -> Result<(), ErroCompras> {
        if !matches!(
            self.estado,
            EstadoNotaEntrada::AConferir | EstadoNotaEntrada::Conferida
        ) {
            return Err(ErroCompras::NotaJaConfirmada);
        }
        self.transitar(EstadoNotaEntrada::Confirmada);
        Ok(())
    }
}

/// Um item de uma [`NotaEntrada`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemNotaEntrada {
    /// Identidade.
    pub id: Id,
    /// A nota dona.
    pub nota_entrada: Id,
    /// O produto casado, quando houver (sugerido ou confirmado).
    pub produto_casado: Option<Id>,
    /// O código do item no cadastro do fornecedor.
    pub codigo_fornecedor: String,
    /// A descrição, como veio do fornecedor.
    pub descricao_fornecedor: String,
    /// O NCM (8 dígitos).
    pub ncm: String,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O valor unitário, antes do rateio.
    pub valor_unitario: Preco,
    /// A fração do frete/seguro/outras despesas rateada para este item.
    pub valor_rateio: Dinheiro,
    /// O estado do casamento com um produto do estoque.
    pub estado_casamento: EstadoCasamento,
}

impl ItemNotaEntrada {
    /// O valor bruto do item, sem rateio (`quantidade × valor_unitario`).
    #[must_use]
    pub fn valor_bruto(&self) -> Dinheiro {
        Dinheiro::de_total(
            self.quantidade,
            self.valor_unitario,
            Arredondamento::MeioAcima,
        )
    }

    /// O custo final unitário, já com o rateio embutido — o que vai para
    /// `estoque.RegistrarEntrada` (`docs/modulos/compras.md` §11.3).
    #[must_use]
    pub fn custo_final_unitario(&self) -> Preco {
        let total = self.valor_bruto() + self.valor_rateio;
        preco_por_unidade(total, self.quantidade)
    }

    /// Vincula (ou revincula) o item a um produto do estoque, marcando `Casado`.
    pub fn vincular(&mut self, produto: Id) {
        self.produto_casado = Some(produto);
        self.estado_casamento = EstadoCasamento::Casado;
    }

    /// Aplica uma sugestão forte (NCM + similaridade), sem confirmar — o usuário ainda
    /// decide.
    pub fn sugerir(&mut self, produto: Id) {
        self.produto_casado = Some(produto);
        self.estado_casamento = EstadoCasamento::SugestaoForte;
    }
}

/// `total / quantidade`, expresso na escala de [`Preco`] (1e-6) — não existe no kernel uma
/// divisão `Dinheiro / Quantidade` pronta (só o caminho inverso, `Dinheiro::de_total`), então
/// esta função faz a conta em `i128` na escala mais fina, com arredondamento meio-para-cima,
/// mesma técnica de `mod_estoque::custo_medio_movel`. `quantidade <= 0` devolve
/// [`Preco::ZERO`] — não deveria acontecer (item de nota sempre tem quantidade positiva),
/// mas evita divisão por zero em vez de expor um `panic` de item malformado.
fn preco_por_unidade(total: Dinheiro, quantidade: Quantidade) -> Preco {
    let qtd = quantidade.unidades_internas();
    if qtd <= 0 {
        return Preco::ZERO;
    }
    let num = i128::from(total.em_centavos()) * 100_000_000;
    let den = i128::from(qtd);
    let arredondado = (2 * num + den) / (2 * den);
    Preco::interna(i64::try_from(arredondado).unwrap_or(i64::MAX))
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Fuso;

    use super::*;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn nota() -> NotaEntrada {
        NotaEntrada {
            id: Id::novo(),
            empresa: Id::novo(),
            fornecedor: Id::novo(),
            chave_acesso: None,
            numero: "45982".to_string(),
            serie: "1".to_string(),
            data_emissao: hoje(),
            valor_produtos: Dinheiro::reais(3_220),
            valor_frete: Dinheiro::reais(180),
            valor_seguro: Dinheiro::ZERO,
            valor_outras_despesas: Dinheiro::ZERO,
            valor_total: Dinheiro::reais(3_400),
            estado: EstadoNotaEntrada::AConferir,
            versao: Versao::INICIAL,
        }
    }

    #[test]
    fn confirmar_e_idempotente_falha_na_segunda_vez() {
        let mut n = nota();
        n.confirmar().unwrap();
        assert_eq!(n.estado, EstadoNotaEntrada::Confirmada);
        assert_eq!(n.confirmar().unwrap_err(), ErroCompras::NotaJaConfirmada);
    }

    #[test]
    fn custo_final_unitario_embute_o_rateio() {
        let mut item = ItemNotaEntrada {
            id: Id::novo(),
            nota_entrada: Id::novo(),
            produto_casado: None,
            codigo_fornecedor: "REF-8821".to_string(),
            descricao_fornecedor: "REFRIG COLA PET 2L".to_string(),
            ncm: "22021000".to_string(),
            quantidade: Quantidade::unidades(120),
            valor_unitario: Preco::reais(4),
            valor_rateio: Dinheiro::ZERO,
            estado_casamento: EstadoCasamento::NaoCasado,
        };
        assert_eq!(item.valor_bruto(), Dinheiro::reais(480));
        assert_eq!(item.custo_final_unitario(), Preco::reais(4));

        item.valor_rateio = Dinheiro::reais(120);
        // (480 + 120) / 120 = 5
        assert_eq!(item.custo_final_unitario(), Preco::reais(5));
    }

    #[test]
    fn vincular_e_sugerir_marcam_o_estado_certo() {
        let mut item = ItemNotaEntrada {
            id: Id::novo(),
            nota_entrada: Id::novo(),
            produto_casado: None,
            codigo_fornecedor: "X".to_string(),
            descricao_fornecedor: "X".to_string(),
            ncm: "12345678".to_string(),
            quantidade: Quantidade::unidades(1),
            valor_unitario: Preco::reais(1),
            valor_rateio: Dinheiro::ZERO,
            estado_casamento: EstadoCasamento::NaoCasado,
        };
        let produto = Id::novo();
        item.sugerir(produto);
        assert_eq!(item.estado_casamento, EstadoCasamento::SugestaoForte);
        item.vincular(produto);
        assert_eq!(item.estado_casamento, EstadoCasamento::Casado);
    }
}
