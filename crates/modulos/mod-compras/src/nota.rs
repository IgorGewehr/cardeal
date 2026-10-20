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
    /// Um aviso para quem confere (CFOP que não é venda, por exemplo). Nota com aviso nunca é
    /// confirmada sozinha.
    pub alerta: Option<String>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl NotaEntrada {
    fn transitar(&mut self, novo: EstadoNotaEntrada) {
        self.estado = novo;
        self.versao = self.versao.proxima();
    }

    /// Estorna a entrada: `Confirmada` → `AConferir`. A nota volta para a conferência, com os
    /// itens como estavam casados, para corrigir (revincular um produto, mudar as parcelas)
    /// e confirmar de novo.
    ///
    /// # Errors
    /// [`ErroCompras::EstadoDeNotaInvalido`] se a nota não está confirmada.
    pub fn estornar(&mut self) -> Result<(), ErroCompras> {
        if self.estado != EstadoNotaEntrada::Confirmada {
            return Err(ErroCompras::EstadoDeNotaInvalido {
                atual: self.estado.rotulo(),
                esperado: EstadoNotaEntrada::Confirmada.rotulo(),
            });
        }
        self.transitar(EstadoNotaEntrada::AConferir);
        Ok(())
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

/// Uma parcela de como o fornecedor cobra a nota: a duplicata do XML (`cobr/dup`) ou a que
/// quem confirma informou. Vira uma parcela do título a pagar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParcelaCompra {
    /// O número da duplicata (`"001"`…), vazio quando informada à mão.
    pub numero: String,
    /// O vencimento.
    pub vencimento: Data,
    /// O valor.
    pub valor: Dinheiro,
}

/// Confere as parcelas de uma nota: de 1 a 360, cada uma com valor, nenhuma vencendo antes
/// da emissão, e a soma fechando com o total da nota (o título a pagar é o total).
///
/// # Errors
/// [`ErroCompras::ParcelaInvalida`], [`ErroCompras::ParcelasNaoFecham`].
pub fn conferir_parcelas(
    parcelas: &[ParcelaCompra],
    total: Dinheiro,
    emissao: Data,
) -> Result<(), ErroCompras> {
    if parcelas.is_empty() || parcelas.len() > 360 {
        return Err(ErroCompras::ParcelaInvalida("de 1 a 360 parcelas"));
    }
    if parcelas.iter().any(|p| !p.valor.e_positivo()) {
        return Err(ErroCompras::ParcelaInvalida(
            "toda parcela precisa de valor maior que zero",
        ));
    }
    if parcelas.iter().any(|p| p.vencimento < emissao) {
        return Err(ErroCompras::ParcelaInvalida(
            "uma parcela vence antes da emissão da nota",
        ));
    }
    let soma: Dinheiro = parcelas.iter().map(|p| p.valor).sum();
    if soma != total {
        return Err(ErroCompras::ParcelasNaoFecham { soma, total });
    }
    Ok(())
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
    /// O código de barras (cEAN) como veio na nota, quando veio.
    pub codigo_barras: Option<String>,
    /// O NCM (8 dígitos).
    pub ncm: String,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O valor unitário, antes do rateio.
    pub valor_unitario: Preco,
    /// A fração do frete/seguro/outras despesas rateada para este item.
    pub valor_rateio: Dinheiro,
    /// O desconto do item na nota (`vDesc`).
    pub valor_desconto: Dinheiro,
    /// Os impostos da nota sobre o item que quem revende não recupera (ICMS-ST + FCP-ST + IPI).
    pub valor_impostos: Dinheiro,
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

    /// O custo total do item: bruto − desconto + rateio das despesas + ICMS-ST/IPI.
    #[must_use]
    pub fn custo_total(&self) -> Dinheiro {
        self.valor_bruto() - self.valor_desconto + self.valor_rateio + self.valor_impostos
    }

    /// O custo final unitário, com desconto, rateio e os impostos não recuperáveis embutidos —
    /// o que vai para `estoque.RegistrarEntrada` (`docs/modulos/compras.md` §11.3).
    #[must_use]
    pub fn custo_final_unitario(&self) -> Preco {
        preco_por_unidade(self.custo_total(), self.quantidade)
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
            alerta: None,
            versao: Versao::INICIAL,
        }
    }

    #[test]
    fn estornar_volta_a_nota_confirmada_para_a_conferencia() {
        let mut n = nota();
        assert!(matches!(
            n.estornar(),
            Err(ErroCompras::EstadoDeNotaInvalido { .. })
        ));
        n.confirmar().unwrap();
        n.estornar().unwrap();
        assert_eq!(n.estado, EstadoNotaEntrada::AConferir);
        n.confirmar().unwrap();
        assert_eq!(n.estado, EstadoNotaEntrada::Confirmada);
    }

    #[test]
    fn parcelas_fecham_com_o_total_e_nao_vencem_antes_da_emissao() {
        let emissao = Data::de_ymd(2026, 10, 1).unwrap();
        let p = |m, d, reais| ParcelaCompra {
            numero: String::new(),
            vencimento: Data::de_ymd(2026, m, d).unwrap(),
            valor: Dinheiro::reais(reais),
        };
        let total = Dinheiro::reais(100);
        assert!(conferir_parcelas(&[p(10, 31, 40), p(11, 30, 60)], total, emissao).is_ok());
        assert!(conferir_parcelas(&[p(10, 1, 100)], total, emissao).is_ok());
        assert!(matches!(
            conferir_parcelas(&[p(10, 31, 40), p(11, 30, 50)], total, emissao),
            Err(ErroCompras::ParcelasNaoFecham { .. })
        ));
        assert!(matches!(
            conferir_parcelas(&[p(9, 30, 100)], total, emissao),
            Err(ErroCompras::ParcelaInvalida(_))
        ));
        assert!(matches!(
            conferir_parcelas(&[p(10, 31, 100), p(11, 30, 0)], total, emissao),
            Err(ErroCompras::ParcelaInvalida(_))
        ));
        assert!(conferir_parcelas(&[], total, emissao).is_err());
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
            codigo_barras: None,
            ncm: "22021000".to_string(),
            quantidade: Quantidade::unidades(120),
            valor_unitario: Preco::reais(4),
            valor_rateio: Dinheiro::ZERO,
            valor_desconto: Dinheiro::ZERO,
            valor_impostos: Dinheiro::ZERO,
            estado_casamento: EstadoCasamento::NaoCasado,
        };
        assert_eq!(item.valor_bruto(), Dinheiro::reais(480));
        assert_eq!(item.custo_final_unitario(), Preco::reais(4));

        item.valor_rateio = Dinheiro::reais(120);
        // (480 + 120) / 120 = 5
        assert_eq!(item.custo_final_unitario(), Preco::reais(5));

        // Desconto de R$ 60 e ICMS-ST + IPI de R$ 180: (480 − 60 + 120 + 180) / 120 = 6
        item.valor_desconto = Dinheiro::reais(60);
        item.valor_impostos = Dinheiro::reais(180);
        assert_eq!(item.custo_total(), Dinheiro::reais(720));
        assert_eq!(item.custo_final_unitario(), Preco::reais(6));
    }

    #[test]
    fn vincular_e_sugerir_marcam_o_estado_certo() {
        let mut item = ItemNotaEntrada {
            id: Id::novo(),
            nota_entrada: Id::novo(),
            produto_casado: None,
            codigo_fornecedor: "X".to_string(),
            descricao_fornecedor: "X".to_string(),
            codigo_barras: None,
            ncm: "12345678".to_string(),
            quantidade: Quantidade::unidades(1),
            valor_unitario: Preco::reais(1),
            valor_rateio: Dinheiro::ZERO,
            valor_desconto: Dinheiro::ZERO,
            valor_impostos: Dinheiro::ZERO,
            estado_casamento: EstadoCasamento::NaoCasado,
        };
        let produto = Id::novo();
        item.sugerir(produto);
        assert_eq!(item.estado_casamento, EstadoCasamento::SugestaoForte);
        item.vincular(produto);
        assert_eq!(item.estado_casamento, EstadoCasamento::Casado);
    }
}
