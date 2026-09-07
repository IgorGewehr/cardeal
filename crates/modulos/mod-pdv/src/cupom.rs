//! Cupom, item e pagamento — a venda de balcão. `docs/modulos/pdv.md` §3 e §4. Domínio puro.
//!
//! Preço **congelado no item**, como em `mod-vendas::ItemVenda` (mesma disciplina, §11.1 de
//! lá) — o item de cupom não reaproveita o tipo de vendas porque cupom não tem orçamento nem
//! reserva, só a máquina de estado mais simples `EmAndamento → Finalizado`/`Cancelado`.

#![allow(clippy::too_many_arguments)]

use cardeal_kernel::{Arredondamento, Dinheiro, Id, Percentual, Preco, Quantidade};
use serde::{Deserialize, Serialize};

use crate::erros::ErroPdv;

/// O estado de um [`Cupom`] (`docs/modulos/pdv.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoCupom {
    /// Em montagem no balcão.
    EmAndamento,
    /// Pago e encerrado — irreversível nesta fatia (sem estorno pós-fechamento ainda).
    Finalizado,
    /// Cancelado antes de finalizar — nenhum lançamento chegou a existir.
    Cancelado,
}

impl EstadoCupom {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::EmAndamento => "EmAndamento",
            Self::Finalizado => "Finalizado",
            Self::Cancelado => "Cancelado",
        }
    }
}

/// Uma linha do cupom, com preço e desconto já congelados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCupom {
    /// Identidade.
    pub id: Id,
    /// O produto.
    pub produto: Id,
    /// A variação de grade.
    pub variacao: Option<Id>,
    /// A quantidade (pode vir da balança — aqui sempre digitada nesta fatia).
    pub quantidade: Quantidade,
    /// O preço unitário resolvido no momento da inclusão — **congelado**.
    pub preco_unitario: Preco,
    /// O desconto em pontos percentuais (`F5`, `AplicarDescontoItem`).
    pub desconto_percentual: Percentual,
    /// O valor do desconto, arredondado por item.
    pub desconto_valor: Dinheiro,
    /// `quantidade × preco_unitario − desconto_valor`.
    pub total_item: Dinheiro,
    /// `F7` marca o item cancelado — some do total, mas a linha continua na tabela (auditável).
    pub cancelado: bool,
    /// O motivo do cancelamento, quando `cancelado`.
    pub motivo_cancelamento: Option<String>,
}

impl ItemCupom {
    /// Monta um item calculando desconto e total.
    ///
    /// # Errors
    /// [`ErroPdv::QuantidadeInvalida`].
    pub fn novo(
        produto: Id,
        variacao: Option<Id>,
        quantidade: Quantidade,
        preco_unitario: Preco,
    ) -> Result<Self, ErroPdv> {
        if !quantidade.e_positiva() {
            return Err(ErroPdv::QuantidadeInvalida);
        }
        let total_item = Dinheiro::de_total(quantidade, preco_unitario, Arredondamento::MeioAcima);
        Ok(Self {
            id: Id::novo(),
            produto,
            variacao,
            quantidade,
            preco_unitario,
            desconto_percentual: Percentual::ZERO,
            desconto_valor: Dinheiro::ZERO,
            total_item,
            cancelado: false,
            motivo_cancelamento: None,
        })
    }

    /// O total bruto antes do desconto — constante ao longo da vida do item (o desconto não
    /// altera preço nem quantidade, só subtrai).
    #[must_use]
    pub fn total_bruto(&self) -> Dinheiro {
        self.total_item + self.desconto_valor
    }

    /// Aplica (ou substitui) o desconto do item, recusando acima do teto do papel.
    ///
    /// # Errors
    /// [`ErroPdv::DescontoAcimaDoLimite`].
    pub fn aplicar_desconto(
        &mut self,
        desconto_percentual: Percentual,
        limite_desconto: Percentual,
    ) -> Result<(), ErroPdv> {
        if desconto_percentual > limite_desconto {
            return Err(ErroPdv::DescontoAcimaDoLimite {
                pedido: desconto_percentual,
                limite: limite_desconto,
            });
        }
        let bruto = self.total_bruto();
        self.desconto_percentual = desconto_percentual;
        self.desconto_valor = bruto.aplicar(desconto_percentual, Arredondamento::MeioAcima);
        self.total_item = bruto - self.desconto_valor;
        Ok(())
    }

    /// Cancela o item (`F7`) — nunca some da tabela, só do total.
    ///
    /// # Errors
    /// [`ErroPdv::ItemJaCancelado`].
    pub fn cancelar(&mut self, motivo: String) -> Result<(), ErroPdv> {
        if self.cancelado {
            return Err(ErroPdv::ItemJaCancelado);
        }
        self.cancelado = true;
        self.motivo_cancelamento = Some(motivo);
        Ok(())
    }
}

/// A forma como uma parcela do pagamento foi feita — resolve para uma conta do Razão
/// distinta cada uma (`docs/modulos/pdv.md` §7). `Carteira` (venda a prazo, "fiado") fica
/// para quando existir um jeito de gerar o `Titulo` correspondente sem duplicar o desenho de
/// `mod-vendas`; TEF fica para quando `PortaTef` existir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FormaPagamentoPdv {
    /// Dinheiro em espécie.
    Dinheiro,
    /// Pix — `ValoresEmTransito` até a conciliação bancária confirmar.
    Pix,
    /// Cartão de débito.
    Debito,
    /// Cartão de crédito.
    Credito,
}

/// Uma parcela do pagamento de um cupom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PagamentoCupom {
    /// Identidade.
    pub id: Id,
    /// A forma de pagamento.
    pub forma: FormaPagamentoPdv,
    /// O valor recebido nesta forma — o troco em dinheiro já vem descontado (§10: o troco é
    /// calculado e mostrado na tela, mas não é dinheiro que a empresa fica).
    pub valor: Dinheiro,
}

fn soma_itens(itens: &[ItemCupom]) -> (Dinheiro, Dinheiro) {
    let ativos = itens.iter().filter(|i| !i.cancelado);
    let desconto = ativos.clone().map(|i| i.desconto_valor).sum();
    let total = ativos.map(|i| i.total_item).sum();
    (desconto, total)
}

/// Um cupom de venda de balcão.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cupom {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// A sessão de caixa (`financeiro_sessao_caixa`) — precisa estar `Aberta`.
    pub sessao_caixa: Id,
    /// O terminal que originou a venda.
    pub terminal: Id,
    /// O número dentro da faixa do terminal.
    pub numero_terminal: i64,
    /// A série fiscal do terminal.
    pub serie_fiscal: i64,
    /// O cliente identificado — opcional, venda sem identificação é o padrão.
    pub cliente: Option<Id>,
    /// Quem operou a venda.
    pub operador: Id,
    /// Quando o cupom foi aberto (primeiro item bipado).
    pub abertura: cardeal_kernel::Instante,
    /// A tabela de preço usada na resolução dos itens.
    pub tabela_preco: Id,
    /// O local de estoque de onde os itens saem ao finalizar.
    pub local_expedicao: Id,
    /// Os itens.
    pub itens: Vec<ItemCupom>,
    /// Os pagamentos — só preenchidos no `FinalizarVenda`.
    pub pagamentos: Vec<PagamentoCupom>,
    /// Soma dos itens antes de desconto (só os não cancelados).
    pub subtotal: Dinheiro,
    /// Soma dos descontos de item (só os não cancelados).
    pub desconto: Dinheiro,
    /// `subtotal − desconto`.
    pub total: Dinheiro,
    /// O estado.
    pub estado: EstadoCupom,
    /// Versão para bloqueio otimista.
    pub versao: cardeal_kernel::Versao,
}

impl Cupom {
    /// Abre um cupom `EmAndamento`, sem itens.
    #[must_use]
    pub fn novo(
        empresa: Id,
        sessao_caixa: Id,
        terminal: Id,
        numero_terminal: i64,
        serie_fiscal: i64,
        cliente: Option<Id>,
        operador: Id,
        abertura: cardeal_kernel::Instante,
        tabela_preco: Id,
        local_expedicao: Id,
    ) -> Self {
        Self {
            id: Id::novo(),
            empresa,
            sessao_caixa,
            terminal,
            numero_terminal,
            serie_fiscal,
            cliente,
            operador,
            abertura,
            tabela_preco,
            local_expedicao,
            itens: Vec::new(),
            pagamentos: Vec::new(),
            subtotal: Dinheiro::ZERO,
            desconto: Dinheiro::ZERO,
            total: Dinheiro::ZERO,
            estado: EstadoCupom::EmAndamento,
            versao: cardeal_kernel::Versao::INICIAL,
        }
    }

    /// Acrescenta um item. Só em `EmAndamento`.
    ///
    /// # Errors
    /// [`ErroPdv::CupomEmEstadoInvalido`].
    pub fn adicionar_item(&mut self, item: ItemCupom) -> Result<(), ErroPdv> {
        self.exigir(EstadoCupom::EmAndamento)?;
        self.itens.push(item);
        self.recalcular();
        Ok(())
    }

    /// Recalcula `subtotal`/`desconto`/`total` a partir dos itens não cancelados.
    pub fn recalcular(&mut self) {
        let (desconto, total_liquido) = soma_itens(&self.itens);
        self.subtotal = self
            .itens
            .iter()
            .filter(|i| !i.cancelado)
            .map(ItemCupom::total_bruto)
            .sum();
        self.desconto = desconto;
        self.total = total_liquido;
        self.versao = self.versao.proxima();
    }

    /// `EmAndamento → Finalizado` — irreversível nesta fatia.
    ///
    /// # Errors
    /// [`ErroPdv::CupomEmEstadoInvalido`], [`ErroPdv::CupomSemItens`].
    pub fn finalizar(&mut self) -> Result<(), ErroPdv> {
        self.exigir(EstadoCupom::EmAndamento)?;
        if !self.itens.iter().any(|i| !i.cancelado) {
            return Err(ErroPdv::CupomSemItens);
        }
        self.transicionar(EstadoCupom::Finalizado);
        Ok(())
    }

    /// `EmAndamento → Cancelado` — nenhum lançamento chega a existir (§7).
    ///
    /// # Errors
    /// [`ErroPdv::CupomJaFinalizado`] a partir de `Finalizado`;
    /// [`ErroPdv::CupomEmEstadoInvalido`] de `Cancelado`.
    pub fn cancelar(&mut self) -> Result<(), ErroPdv> {
        match self.estado {
            EstadoCupom::EmAndamento => {
                self.transicionar(EstadoCupom::Cancelado);
                Ok(())
            }
            EstadoCupom::Finalizado => Err(ErroPdv::CupomJaFinalizado),
            EstadoCupom::Cancelado => Err(ErroPdv::CupomEmEstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "EmAndamento",
            }),
        }
    }

    fn transicionar(&mut self, novo: EstadoCupom) {
        self.estado = novo;
        self.versao = self.versao.proxima();
    }

    fn exigir(&self, estado: EstadoCupom) -> Result<(), ErroPdv> {
        if self.estado == estado {
            Ok(())
        } else {
            Err(ErroPdv::CupomEmEstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: estado.rotulo(),
            })
        }
    }
}

/// Como os pagamentos de um cupom se decompõem para o receituário: quanto entrou líquido em
/// cada conta de destino, já sem o troco em dinheiro (`docs/modulos/pdv.md` §10 — o troco é
/// calculado na tela, não persistido; esta fatia exige que a soma dos `valor` informados já
/// feche exatamente com o total, sem excesso).
///
/// # Errors
/// [`ErroPdv::SomaDePagamentosDivergente`] se a soma não bate com `total` ao centavo;
/// [`ErroPdv::PagamentoSemFormas`] se a lista vier vazia.
pub fn validar_pagamentos(pagamentos: &[PagamentoCupom], total: Dinheiro) -> Result<(), ErroPdv> {
    if pagamentos.is_empty() {
        return Err(ErroPdv::PagamentoSemFormas);
    }
    let soma: Dinheiro = pagamentos.iter().map(|p| p.valor).sum();
    if soma != total {
        return Err(ErroPdv::SomaDePagamentosDivergente {
            recebido: soma,
            total,
        });
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(qtd: i64, preco: i64) -> ItemCupom {
        ItemCupom::novo(
            Id::novo(),
            None,
            Quantidade::unidades(qtd),
            Preco::reais(preco),
        )
        .unwrap()
    }

    #[test]
    fn item_calcula_desconto_e_total() {
        let mut i = item(2, 10); // 20,00
        i.aplicar_desconto(Percentual::pontos(10), Percentual::pontos(20))
            .unwrap();
        assert_eq!(i.total_bruto(), Dinheiro::reais(20));
        assert_eq!(i.desconto_valor, Dinheiro::reais(2));
        assert_eq!(i.total_item, Dinheiro::reais(18));
    }

    #[test]
    fn desconto_acima_do_limite_e_recusado() {
        let mut i = item(1, 10);
        let erro = i
            .aplicar_desconto(Percentual::pontos(12), Percentual::pontos(5))
            .unwrap_err();
        assert!(matches!(erro, ErroPdv::DescontoAcimaDoLimite { .. }));
    }

    #[test]
    fn item_cancelado_some_do_total_mas_continua_na_lista() {
        let mut c = Cupom::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            1,
            1,
            None,
            Id::novo(),
            cardeal_kernel::Instante::agora(),
            Id::novo(),
            Id::novo(),
        );
        c.adicionar_item(item(2, 10)).unwrap();
        let id_item2 = {
            let i2 = item(1, 5);
            let id = i2.id;
            c.adicionar_item(i2).unwrap();
            id
        };
        assert_eq!(c.total, Dinheiro::reais(25));

        let item2 = c.itens.iter_mut().find(|i| i.id == id_item2).unwrap();
        item2
            .cancelar("cliente desistiu do item".to_string())
            .unwrap();
        c.recalcular();

        assert_eq!(c.itens.len(), 2); // continua na tabela
        assert_eq!(c.total, Dinheiro::reais(20)); // some do total
    }

    #[test]
    fn finalizar_exige_ao_menos_um_item_ativo() {
        let mut c = Cupom::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            1,
            1,
            None,
            Id::novo(),
            cardeal_kernel::Instante::agora(),
            Id::novo(),
            Id::novo(),
        );
        assert!(matches!(c.finalizar().unwrap_err(), ErroPdv::CupomSemItens));
        c.adicionar_item(item(1, 10)).unwrap();
        c.finalizar().unwrap();
        assert_eq!(c.estado, EstadoCupom::Finalizado);
    }

    #[test]
    fn cancelar_depois_de_finalizado_e_recusado() {
        let mut c = Cupom::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            1,
            1,
            None,
            Id::novo(),
            cardeal_kernel::Instante::agora(),
            Id::novo(),
            Id::novo(),
        );
        c.adicionar_item(item(1, 10)).unwrap();
        c.finalizar().unwrap();
        assert_eq!(c.cancelar().unwrap_err(), ErroPdv::CupomJaFinalizado);
    }

    #[test]
    fn pagamentos_tem_de_fechar_exatamente_com_o_total() {
        let pagamentos = vec![
            PagamentoCupom {
                id: Id::novo(),
                forma: FormaPagamentoPdv::Dinheiro,
                valor: Dinheiro::reais(20),
            },
            PagamentoCupom {
                id: Id::novo(),
                forma: FormaPagamentoPdv::Debito,
                valor: Dinheiro::centavos(1134),
            },
        ];
        validar_pagamentos(&pagamentos, Dinheiro::centavos(3134)).unwrap();
        assert!(matches!(
            validar_pagamentos(&pagamentos, Dinheiro::reais(32)).unwrap_err(),
            ErroPdv::SomaDePagamentosDivergente { .. }
        ));
        assert!(matches!(
            validar_pagamentos(&[], Dinheiro::reais(10)).unwrap_err(),
            ErroPdv::PagamentoSemFormas
        ));
    }
}
