//! Cupom, item e pagamento — a venda de balcão. `docs/modulos/pdv.md` §3 e §4. Domínio puro.
//!
//! Preço **congelado no item**, como em `mod-vendas::ItemVenda` (mesma disciplina, §11.1 de
//! lá) — o item de cupom não reaproveita o tipo de vendas porque cupom não tem orçamento nem
//! reserva, só a máquina de estado mais simples `EmAndamento → Finalizado`/`Cancelado`, e o
//! estorno da venda já paga, `Finalizado → Estornado`.

#![allow(clippy::too_many_arguments)]

use cardeal_kernel::{Arredondamento, Dinheiro, Id, Percentual, Preco, Quantidade};
use serde::{Deserialize, Serialize};

use crate::erros::ErroPdv;

/// O estado de um [`Cupom`] (`docs/modulos/pdv.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoCupom {
    /// Em montagem no balcão.
    EmAndamento,
    /// Pago e encerrado. Só se desfaz pelo estorno ([`Cupom::estornar`]).
    Finalizado,
    /// Cancelado antes de finalizar — nenhum lançamento chegou a existir.
    Cancelado,
    /// Finalizado e depois desfeito (`EstornarVenda`): o lançamento da venda foi estornado e a
    /// mercadoria voltou ao estoque.
    Estornado,
}

impl EstadoCupom {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::EmAndamento => "EmAndamento",
            Self::Finalizado => "Finalizado",
            Self::Cancelado => "Cancelado",
            Self::Estornado => "Estornado",
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
    /// A prazo — fiado, crediário, boleto: vira conta a receber do cliente identificado, com
    /// vencimento e parcelas ([`CondicaoPrazo`]).
    Prazo,
}

/// A condição da parte a prazo de uma venda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CondicaoPrazo {
    /// O vencimento da primeira parcela.
    pub primeiro_vencimento: cardeal_kernel::Data,
    /// Quantas parcelas (1 a 24).
    pub parcelas: u16,
    /// Dias entre as parcelas.
    pub intervalo_dias: i32,
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
    /// A condição, quando a forma é [`FormaPagamentoPdv::Prazo`].
    pub prazo: Option<CondicaoPrazo>,
}

/// O desconto dado no total da venda (`Shift+F5`), além do desconto de cada item: "10% na
/// compra" ou "fecha em R$ 100". Enquanto a venda está aberta ele vale sobre o que os itens
/// somam (então acompanha item bipado ou cancelado); ao finalizar, é rateado entre os itens
/// ([`Cupom::ratear_desconto_no_total`]) e passa a morar neles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DescontoNoTotal {
    /// Um percentual sobre o total dos itens (já com o desconto de cada um).
    Percentual(Percentual),
    /// Um valor fixo.
    Valor(Dinheiro),
}

impl DescontoNoTotal {
    /// Quanto o desconto tira de `liquido` — nunca mais que ele.
    #[must_use]
    pub fn valor_sobre(self, liquido: Dinheiro) -> Dinheiro {
        match self {
            Self::Percentual(p) => liquido.aplicar(p, Arredondamento::MeioAcima),
            Self::Valor(v) => v,
        }
        .min(liquido)
        .max(Dinheiro::ZERO)
    }

    /// Valor positivo; percentual entre 0 e 100.
    #[must_use]
    pub fn valido(self) -> bool {
        match self {
            Self::Percentual(p) => p > Percentual::ZERO && p <= Percentual::CEM,
            Self::Valor(v) => v.e_positivo(),
        }
    }
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
    /// O desconto da venda: o dos itens (só os não cancelados) mais o desconto no total.
    pub desconto: Dinheiro,
    /// O desconto no total, enquanto a venda está aberta (ao finalizar, vai para os itens).
    pub desconto_no_total: Option<DescontoNoTotal>,
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
            desconto_no_total: None,
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

    /// Identifica (ou remove a identificação de) o cliente da venda. Só em `EmAndamento`: um
    /// cupom finalizado ou cancelado já foi ao fiscal e ao razão com o cliente que tinha.
    ///
    /// # Errors
    /// [`ErroPdv::CupomEmEstadoInvalido`].
    pub fn identificar_cliente(&mut self, cliente: Option<Id>) -> Result<(), ErroPdv> {
        self.exigir(EstadoCupom::EmAndamento)?;
        self.cliente = cliente;
        Ok(())
    }

    /// Recalcula `subtotal`/`desconto`/`total` a partir dos itens não cancelados e do
    /// desconto no total.
    pub fn recalcular(&mut self) {
        let (desconto, total_liquido) = soma_itens(&self.itens);
        self.subtotal = self
            .itens
            .iter()
            .filter(|i| !i.cancelado)
            .map(ItemCupom::total_bruto)
            .sum();
        let no_total = self
            .desconto_no_total
            .map_or(Dinheiro::ZERO, |d| d.valor_sobre(total_liquido));
        self.desconto = desconto + no_total;
        self.total = total_liquido - no_total;
        self.versao = self.versao.proxima();
    }

    /// Quanto o desconto no total tira agora.
    #[must_use]
    pub fn valor_do_desconto_no_total(&self) -> Dinheiro {
        let (_, liquido) = soma_itens(&self.itens);
        self.desconto_no_total
            .map_or(Dinheiro::ZERO, |d| d.valor_sobre(liquido))
    }

    /// Dá (ou tira, com `None`) o desconto no total. Só em `EmAndamento`; recusa o que passar
    /// do teto do papel — o teto vale para a venda inteira ([`Cupom::conferir_teto`]).
    ///
    /// # Errors
    /// [`ErroPdv::CupomEmEstadoInvalido`], [`ErroPdv::DescontoInvalido`],
    /// [`ErroPdv::DescontoDaVendaAcimaDoLimite`].
    pub fn definir_desconto_no_total(
        &mut self,
        desconto: Option<DescontoNoTotal>,
        limite: Percentual,
    ) -> Result<(), ErroPdv> {
        self.exigir(EstadoCupom::EmAndamento)?;
        if desconto.is_some_and(|d| !d.valido()) {
            return Err(ErroPdv::DescontoInvalido);
        }
        let anterior = self.desconto_no_total;
        self.desconto_no_total = desconto;
        self.recalcular();
        if let Err(e) = self.conferir_teto(limite) {
            self.desconto_no_total = anterior;
            self.recalcular();
            return Err(e);
        }
        Ok(())
    }

    /// O desconto da venda inteira (itens + total) cabe no teto do papel, sobre o valor bruto
    /// dos itens? Cada item já respeita o teto sozinho; o desconto no total soma por cima, e
    /// item cancelado depois de um desconto em valor sobe o percentual — por isso a
    /// finalização confere de novo.
    ///
    /// # Errors
    /// [`ErroPdv::DescontoDaVendaAcimaDoLimite`].
    pub fn conferir_teto(&self, limite: Percentual) -> Result<(), ErroPdv> {
        let maximo = self.subtotal.aplicar(limite, Arredondamento::MeioAcima);
        if self.desconto > maximo {
            return Err(ErroPdv::DescontoDaVendaAcimaDoLimite {
                desconto: self.desconto,
                maximo,
                limite,
            });
        }
        Ok(())
    }

    /// Distribui o desconto no total entre os itens ativos, na proporção do valor de cada um
    /// (o centavo que sobra vai para os maiores), e o esquece: daí em diante o desconto mora
    /// nos itens — o que a NFC-e (desconto por item), o comprovante e o estorno enxergam. O
    /// total não muda. Chamado ao finalizar.
    pub fn ratear_desconto_no_total(&mut self) {
        let extra = self.valor_do_desconto_no_total();
        self.desconto_no_total = None;
        if extra.e_positivo() {
            let ativos: Vec<usize> = (0..self.itens.len())
                .filter(|&i| !self.itens[i].cancelado)
                .collect();
            let pesos: Vec<i64> = ativos
                .iter()
                .map(|&i| self.itens[i].total_item.em_centavos())
                .collect();
            if pesos.iter().sum::<i64>() > 0 {
                for (&i, parte) in ativos.iter().zip(extra.ratear_por_pesos(&pesos)) {
                    let item = &mut self.itens[i];
                    item.desconto_valor += parte;
                    item.total_item -= parte;
                }
            }
        }
        self.recalcular();
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
            EstadoCupom::Finalizado | EstadoCupom::Estornado => Err(ErroPdv::CupomJaFinalizado),
            EstadoCupom::Cancelado => Err(ErroPdv::CupomEmEstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "EmAndamento",
            }),
        }
    }

    /// `Finalizado → Estornado` — a venda desfeita depois de paga (`EstornarVenda`).
    ///
    /// # Errors
    /// [`ErroPdv::VendaJaEstornada`]; [`ErroPdv::CupomEmEstadoInvalido`] fora de `Finalizado`.
    pub fn estornar(&mut self) -> Result<(), ErroPdv> {
        if self.estado == EstadoCupom::Estornado {
            return Err(ErroPdv::VendaJaEstornada);
        }
        self.exigir(EstadoCupom::Finalizado)?;
        self.transicionar(EstadoCupom::Estornado);
        Ok(())
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
    let a_prazo: Vec<_> = pagamentos
        .iter()
        .filter(|p| p.forma == FormaPagamentoPdv::Prazo)
        .collect();
    if a_prazo.len() > 1 {
        return Err(ErroPdv::CondicaoDePrazoInvalida(
            "informe a parte a prazo numa forma só",
        ));
    }
    for p in a_prazo {
        let Some(c) = p.prazo else {
            return Err(ErroPdv::CondicaoDePrazoInvalida(
                "informe o vencimento e as parcelas",
            ));
        };
        if !(1..=24).contains(&c.parcelas) {
            return Err(ErroPdv::CondicaoDePrazoInvalida("de 1 a 24 parcelas"));
        }
        if c.parcelas > 1 && !(1..=365).contains(&c.intervalo_dias) {
            return Err(ErroPdv::CondicaoDePrazoInvalida(
                "o intervalo entre as parcelas vai de 1 a 365 dias",
            ));
        }
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

    fn cupom_vazio() -> Cupom {
        Cupom::novo(
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
        )
    }

    #[test]
    fn desconto_no_total_acompanha_os_itens_e_respeita_o_teto_da_venda() {
        let mut c = cupom_vazio();
        c.adicionar_item(item(1, 70)).unwrap();
        c.adicionar_item(item(1, 30)).unwrap();
        let teto = Percentual::pontos(10);
        // 10% sobre R$ 100: cabe.
        c.definir_desconto_no_total(
            Some(DescontoNoTotal::Percentual(Percentual::pontos(10))),
            teto,
        )
        .unwrap();
        assert_eq!(
            (c.desconto, c.total),
            (Dinheiro::reais(10), Dinheiro::reais(90))
        );
        // R$ 15 passa do teto (máximo R$ 10): recusado e o anterior fica.
        let erro = c
            .definir_desconto_no_total(Some(DescontoNoTotal::Valor(Dinheiro::reais(15))), teto)
            .unwrap_err();
        assert!(matches!(
            erro,
            ErroPdv::DescontoDaVendaAcimaDoLimite { maximo, .. } if maximo == Dinheiro::reais(10)
        ));
        assert_eq!(c.total, Dinheiro::reais(90));
        // Um item a mais: o percentual acompanha.
        c.adicionar_item(item(1, 50)).unwrap();
        assert_eq!(c.total, Dinheiro::reais(135));
        // Valor maior que os itens não deixa o total negativo; inválidos são recusados.
        assert_eq!(
            DescontoNoTotal::Valor(Dinheiro::reais(500)).valor_sobre(Dinheiro::reais(20)),
            Dinheiro::reais(20)
        );
        assert!(!DescontoNoTotal::Valor(Dinheiro::ZERO).valido());
        assert!(!DescontoNoTotal::Percentual(Percentual::pontos(101)).valido());
        // Tirar.
        c.definir_desconto_no_total(None, teto).unwrap();
        assert_eq!(c.total, Dinheiro::reais(150));
    }

    #[test]
    fn o_rateio_leva_o_desconto_para_os_itens_sem_mudar_o_total() {
        let mut c = cupom_vazio();
        c.adicionar_item(item(1, 10)).unwrap();
        c.adicionar_item(item(1, 10)).unwrap();
        c.adicionar_item(item(1, 10)).unwrap();
        c.definir_desconto_no_total(
            Some(DescontoNoTotal::Valor(Dinheiro::reais(1))),
            Percentual::CEM,
        )
        .unwrap();
        let (desconto, total) = (c.desconto, c.total);
        c.ratear_desconto_no_total();
        assert_eq!((c.desconto, c.total), (desconto, total));
        assert!(c.desconto_no_total.is_none());
        let partes: Vec<i64> = c
            .itens
            .iter()
            .map(|i| i.desconto_valor.em_centavos())
            .collect();
        assert_eq!(
            partes.iter().sum::<i64>(),
            100,
            "o centavo que sobra não some: {partes:?}"
        );
        assert!(c
            .itens
            .iter()
            .all(|i| i.total_item + i.desconto_valor == Dinheiro::reais(10)));
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
    fn estornar_so_a_venda_finalizada_e_uma_vez() {
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
        assert!(matches!(
            c.estornar().unwrap_err(),
            ErroPdv::CupomEmEstadoInvalido { .. }
        ));
        c.finalizar().unwrap();
        c.estornar().unwrap();
        assert_eq!(c.estado, EstadoCupom::Estornado);
        assert!(matches!(
            c.estornar().unwrap_err(),
            ErroPdv::VendaJaEstornada
        ));
        assert!(matches!(
            c.cancelar().unwrap_err(),
            ErroPdv::CupomJaFinalizado
        ));
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
    fn cliente_so_muda_com_o_cupom_em_andamento() {
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
        let cliente = Id::novo();
        c.identificar_cliente(Some(cliente)).unwrap();
        assert_eq!(c.cliente, Some(cliente));
        c.identificar_cliente(None).unwrap();
        assert_eq!(c.cliente, None);

        c.cancelar().unwrap();
        assert!(c.identificar_cliente(Some(cliente)).is_err());
    }

    #[test]
    fn pagamentos_tem_de_fechar_exatamente_com_o_total() {
        let pagamentos = vec![
            PagamentoCupom {
                id: Id::novo(),
                forma: FormaPagamentoPdv::Dinheiro,
                valor: Dinheiro::reais(20),
                prazo: None,
            },
            PagamentoCupom {
                id: Id::novo(),
                forma: FormaPagamentoPdv::Debito,
                valor: Dinheiro::centavos(1134),
                prazo: None,
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
