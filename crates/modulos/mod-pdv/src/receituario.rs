//! O receituário contábil do PDV — cada venda de balcão termina num lançamento balanceado.
//!
//! `docs/modulos/pdv.md` §7. Um único lançamento `Realizado` por cupom: um débito por linha
//! de pagamento (cada forma resolve para sua própria conta), o CMV da baixa de estoque e o
//! desconto concedido, tudo junto — o mesmo desenho de `mod-vendas::faturar_pedido` e
//! `mod-os::faturar_ordem_servico`, generalizado para N formas de pagamento em vez de uma só.

use cardeal_kernel::{Data, Dinheiro, Id, Instante};
use cardeal_ledger::{ConstrutorLancamento, LancamentoBalanceado, Origem};

use crate::cupom::Cupom;
use crate::erros::ErroPdv;

/// Quem está postando, de onde e quando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Autoria {
    /// O usuário responsável.
    pub usuario: Id,
    /// O dispositivo de origem (o terminal).
    pub dispositivo: Id,
    /// O instante da unidade de trabalho.
    pub agora: Instante,
}

/// As contas do fechamento de um cupom — os destinos de pagamento vêm à parte
/// (`pagamentos_resolvidos`), porque cada cupom pode combinar formas diferentes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContasFinalizacao {
    /// Receita de vendas (4.1).
    pub receita: Id,
    /// Descontos concedidos (4.4).
    pub descontos: Id,
    /// CMV (5.1).
    pub cmv: Id,
    /// Estoque de mercadorias (1.3.01).
    pub estoque: Id,
}

fn origem_cupom(cupom: Id) -> Origem {
    Origem::modulo("pdv").tipo("cupom").agregado(cupom)
}

/// Monta o lançamento de finalização de um cupom: um débito por linha de pagamento (já
/// resolvida para a conta certa pelo comando), o desconto e o CMV, tudo num só lançamento
/// `Realizado` — a venda de balcão é sempre à vista no ato, nunca `Confirmado`/a prazo nesta
/// fatia (`FormaPagamentoPdv::Carteira` ainda não existe).
///
/// # Errors
/// [`ErroPdv::CupomSemItens`] se o cupom não tem itens ativos; propaga um desbalanceamento
/// improvável do construtor como o mesmo erro.
pub fn faturar_cupom(
    cupom: &Cupom,
    data: Data,
    pagamentos_resolvidos: &[(Id, Dinheiro)],
    cmv_total: Dinheiro,
    contas: ContasFinalizacao,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroPdv> {
    if !cupom.itens.iter().any(|i| !i.cancelado) {
        return Err(ErroPdv::CupomSemItens);
    }
    let bruto = cupom.total + cupom.desconto;
    let mut c =
        ConstrutorLancamento::novo(cupom.empresa, data, "Venda de balcão (PDV)".to_string())
            .origem(origem_cupom(cupom.id))
            .liquidacao(data)
            .criado_por(autoria.usuario, autoria.dispositivo)
            .agora(autoria.agora);
    for (conta, valor) in pagamentos_resolvidos {
        c = c.debitar(*conta, *valor);
    }
    c = c
        .debitar_se(contas.descontos, cupom.desconto)
        .creditar(contas.receita, bruto);
    if cmv_total.e_positivo() {
        c = c
            .debitar(contas.cmv, cmv_total)
            .creditar(contas.estoque, cmv_total);
    }
    c.construir().map_err(|_| ErroPdv::CupomSemItens)
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Fuso, Percentual, Preco, Quantidade};
    use cardeal_ledger::EstadoLancamento;

    use super::*;
    use crate::cupom::ItemCupom;

    fn hoje() -> Data {
        Data::hoje(Fuso::BRASILIA)
    }

    fn autoria() -> Autoria {
        Autoria {
            usuario: Id::novo(),
            dispositivo: Id::novo(),
            agora: Instante::agora(),
        }
    }

    fn contas() -> ContasFinalizacao {
        ContasFinalizacao {
            receita: Id::novo(),
            descontos: Id::novo(),
            cmv: Id::novo(),
            estoque: Id::novo(),
        }
    }

    fn cupom_com_item() -> Cupom {
        let mut c = Cupom::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            1,
            1,
            None,
            Id::novo(),
            Instante::agora(),
            Id::novo(),
            Id::novo(),
        );
        let mut item =
            ItemCupom::novo(Id::novo(), None, Quantidade::unidades(2), Preco::reais(50)).unwrap();
        item.aplicar_desconto(Percentual::pontos(10), Percentual::CEM)
            .unwrap();
        c.adicionar_item(item).unwrap();
        c.finalizar().unwrap();
        c
    }

    #[test]
    fn faturamento_com_uma_forma_balanceia() {
        let c = cupom_com_item(); // bruto 100, desconto 10, total 90
        let l = faturar_cupom(
            &c,
            hoje(),
            &[(Id::novo(), Dinheiro::reais(90))],
            Dinheiro::reais(60),
            contas(),
            autoria(),
        )
        .unwrap();
        let interno = l.interno();
        assert!(interno.esta_balanceado());
        assert_eq!(interno.estado, EstadoLancamento::Realizado);
        assert_eq!(interno.total_creditos(), Dinheiro::reais(160)); // receita 100 + estoque 60
    }

    #[test]
    fn faturamento_com_multiplas_formas_soma_os_debitos() {
        let c = cupom_com_item();
        let pagamentos = [
            (Id::novo(), Dinheiro::reais(50)),
            (Id::novo(), Dinheiro::reais(40)),
        ];
        let l =
            faturar_cupom(&c, hoje(), &pagamentos, Dinheiro::ZERO, contas(), autoria()).unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().partidas.len(), 4); // 2 pagamentos + desconto + receita
    }
}
