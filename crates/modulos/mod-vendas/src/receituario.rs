//! O receituário contábil de vendas — cada evento termina num lançamento balanceado.
//!
//! `docs/modulos/vendas.md` §7. O CMV usa o `custo_medio` que `estoque` devolve (o tipo
//! `SaidaAplicada` do `mod-estoque`); aqui só a montagem do
//! [`LancamentoBalanceado`](cardeal_ledger::LancamentoBalanceado), com as contas já
//! resolvidas por papel pelo comando (`docs/contratos-internos.md` §6, regra 4).

#![allow(clippy::too_many_arguments)]

use cardeal_kernel::{Data, Dinheiro, Id, Instante};
use cardeal_ledger::{
    ConstrutorLancamento, Contraparte, EstadoLancamento, LancamentoBalanceado, Origem,
};

use crate::comissao::Comissao;
use crate::devolucao::Devolucao;
use crate::erros::ErroVendas;
use crate::pedido::Pedido;

/// Quem está postando, de onde e quando.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Autoria {
    /// O usuário responsável.
    pub usuario: Id,
    /// O dispositivo de origem.
    pub dispositivo: Id,
    /// O instante da unidade de trabalho.
    pub agora: Instante,
}

/// As contas do faturamento de um pedido.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContasFaturamento {
    /// Caixa (à vista) ou Clientes a receber (a prazo).
    pub destino: Id,
    /// Receita de vendas (4.1).
    pub receita: Id,
    /// Descontos concedidos (4.4).
    pub descontos: Id,
    /// CMV (5.1).
    pub cmv: Id,
    /// Estoque de mercadorias (1.3.01).
    pub estoque: Id,
}

/// As contas da comissão.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContasComissao {
    /// Despesas comerciais (5.5).
    pub despesas_comerciais: Id,
    /// Comissões a pagar (2.1.03, subconta).
    pub comissoes_a_pagar: Id,
}

fn origem_pedido(pedido: Id) -> Origem {
    Origem::modulo("vendas").tipo("pedido").agregado(pedido)
}

/// Monta o lançamento do faturamento de um pedido: receita, desconto e CMV em um só
/// lançamento balanceado (`docs/modulos/vendas.md` §7).
///
/// - À vista: D Caixa / C Receita, `Realizado`.
/// - A prazo: D Clientes a receber / C Receita, `Confirmado` (o `Titulo` nasce no financeiro
///   pelo evento `vendas.pedido_faturado.v1`).
/// - Sempre: D Descontos concedidos (se houver) e D CMV / C Estoque.
///
/// # Errors
/// [`ErroVendas::PedidoSemItens`] se o pedido não tem itens; propaga um desbalanceamento
/// improvável do construtor como [`ErroVendas::PedidoSemItens`].
pub fn faturar_pedido(
    pedido: &Pedido,
    a_vista: bool,
    cmv_total: Dinheiro,
    contas: ContasFaturamento,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroVendas> {
    if pedido.itens.is_empty() {
        return Err(ErroVendas::PedidoSemItens);
    }
    let bruto = pedido.total + pedido.desconto_total;
    let historico = format!(
        "Faturamento do pedido — {}",
        if a_vista { "à vista" } else { "a prazo" }
    );
    let mut c = ConstrutorLancamento::novo(pedido.empresa, pedido.data, historico)
        .origem(origem_pedido(pedido.id))
        .criado_por(autoria.usuario, autoria.dispositivo)
        .agora(autoria.agora);
    c = if a_vista {
        c.liquidacao(pedido.data)
            .debitar(contas.destino, pedido.total)
    } else {
        c.estado(EstadoLancamento::Confirmado)
            .debitar(contas.destino, pedido.total)
            .contraparte(Contraparte::Cliente(pedido.cliente))
    };
    c = c
        .debitar_se(contas.descontos, pedido.desconto_total)
        .creditar(contas.receita, bruto);
    if cmv_total.e_positivo() {
        c = c
            .debitar(contas.cmv, cmv_total)
            .creditar(contas.estoque, cmv_total);
    }
    c.construir().map_err(|_| ErroVendas::PedidoSemItens)
}

/// Monta o lançamento de estorno de uma devolução (`Realizado`):
/// D Devoluções de venda / C Caixa|Clientes, e D Estoque / C CMV pelo custo reposto.
///
/// # Errors
/// [`ErroVendas::DevolucaoVazia`] se não há valor a estornar.
pub fn concluir_devolucao(
    devolucao: &Devolucao,
    a_vista: bool,
    cmv_reposto: Dinheiro,
    contas: ContasFaturamento,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroVendas> {
    if !devolucao.valor_total.e_positivo() {
        return Err(ErroVendas::DevolucaoVazia);
    }
    let mut c = ConstrutorLancamento::novo(
        devolucao.empresa,
        devolucao.data,
        "Devolução de venda".to_string(),
    )
    .origem(
        Origem::modulo("vendas")
            .tipo("devolucao")
            .agregado(devolucao.id),
    )
    .liquidacao(devolucao.data)
    .criado_por(autoria.usuario, autoria.dispositivo)
    .agora(autoria.agora)
    .debitar(contas.descontos, devolucao.valor_total)
    .creditar(contas.destino, devolucao.valor_total);
    if !a_vista {
        c = c.contraparte(Contraparte::Cliente(devolucao.cliente));
    }
    if cmv_reposto.e_positivo() {
        c = c
            .debitar(contas.estoque, cmv_reposto)
            .creditar(contas.cmv, cmv_reposto);
    }
    c.construir().map_err(|_| ErroVendas::DevolucaoVazia)
}

/// Lançamento da apuração de comissão: D Despesas comerciais / C Comissões a pagar,
/// `Confirmado`.
///
/// # Errors
/// [`ErroVendas::ComissaoEmEstadoInvalido`] se o valor não é positivo.
pub fn apurar_comissao(
    comissao: &Comissao,
    contas: ContasComissao,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroVendas> {
    if !comissao.valor.e_positivo() {
        return Err(ErroVendas::ComissaoEmEstadoInvalido { atual: "sem valor" });
    }
    ConstrutorLancamento::novo(
        comissao.empresa,
        comissao.competencia,
        "Comissão de venda apurada".to_string(),
    )
    .origem(
        Origem::modulo("vendas")
            .tipo("comissao")
            .agregado(comissao.id),
    )
    .estado(EstadoLancamento::Confirmado)
    .criado_por(autoria.usuario, autoria.dispositivo)
    .agora(autoria.agora)
    .debitar(contas.despesas_comerciais, comissao.valor)
    .creditar(contas.comissoes_a_pagar, comissao.valor)
    .contraparte(Contraparte::Funcionario(comissao.vendedor))
    .construir()
    .map_err(|_| ErroVendas::ComissaoEmEstadoInvalido { atual: "sem valor" })
}

/// Lançamento do pagamento de comissão: D Comissões a pagar / C Bancos, `Realizado`.
///
/// # Errors
/// [`ErroVendas::ComissaoEmEstadoInvalido`] se o valor não é positivo.
pub fn pagar_comissao(
    comissao: &Comissao,
    conta_banco: Id,
    contas: ContasComissao,
    data: Data,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroVendas> {
    if !comissao.valor.e_positivo() {
        return Err(ErroVendas::ComissaoEmEstadoInvalido { atual: "sem valor" });
    }
    ConstrutorLancamento::novo(comissao.empresa, data, "Pagamento de comissão".to_string())
        .origem(
            Origem::modulo("vendas")
                .tipo("comissao")
                .agregado(comissao.id),
        )
        .liquidacao(data)
        .criado_por(autoria.usuario, autoria.dispositivo)
        .agora(autoria.agora)
        .debitar(contas.comissoes_a_pagar, comissao.valor)
        .contraparte(Contraparte::Funcionario(comissao.vendedor))
        .creditar(conta_banco, comissao.valor)
        .construir()
        .map_err(|_| ErroVendas::ComissaoEmEstadoInvalido { atual: "sem valor" })
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Fuso, Percentual, Preco, Quantidade};

    use super::*;
    use crate::pedido::ItemVenda;

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

    fn contas_fat() -> ContasFaturamento {
        ContasFaturamento {
            destino: Id::novo(),
            receita: Id::novo(),
            descontos: Id::novo(),
            cmv: Id::novo(),
            estoque: Id::novo(),
        }
    }

    fn pedido() -> Pedido {
        let mut p = Pedido::novo(
            Id::novo(),
            Id::novo(),
            Id::novo(),
            hoje(),
            Id::novo(),
            Id::novo(),
            Id::novo(),
            Id::novo(),
        );
        p.adicionar_item(
            ItemVenda::novo(
                Id::novo(),
                None,
                Quantidade::unidades(100),
                Preco::reais(10),
                Percentual::pontos(5),
                Percentual::pontos(20),
            )
            .unwrap(),
        )
        .unwrap();
        p.confirmar().unwrap();
        p.faturar().unwrap();
        p
    }

    #[test]
    fn faturamento_a_vista_balanceia_com_receita_desconto_e_cmv() {
        let p = pedido(); // bruto 1000, desconto 50, total 950
        let l = faturar_pedido(&p, true, Dinheiro::reais(400), contas_fat(), autoria()).unwrap();
        let interno = l.interno();
        assert!(interno.esta_balanceado());
        assert_eq!(interno.estado, EstadoLancamento::Realizado);
        assert_eq!(interno.total_creditos(), Dinheiro::reais(1400)); // receita 1000 + estoque 400
    }

    #[test]
    fn faturamento_a_prazo_e_confirmado_e_tem_contraparte() {
        let p = pedido();
        let l = faturar_pedido(&p, false, Dinheiro::reais(400), contas_fat(), autoria()).unwrap();
        assert_eq!(l.interno().estado, EstadoLancamento::Confirmado);
        assert!(l
            .interno()
            .partidas
            .iter()
            .any(|part| matches!(part.contraparte, Some(Contraparte::Cliente(_)))));
    }

    #[test]
    fn comissao_apurada_e_paga_balanceiam() {
        let p = pedido();
        let c =
            crate::comissao::Comissao::apurar(&p, Percentual::pontos(3), hoje(), Dinheiro::ZERO)
                .unwrap();
        let contas = ContasComissao {
            despesas_comerciais: Id::novo(),
            comissoes_a_pagar: Id::novo(),
        };
        let l = apurar_comissao(&c, contas, autoria()).unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().estado, EstadoLancamento::Confirmado);

        let l = pagar_comissao(&c, Id::novo(), contas, hoje(), autoria()).unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().estado, EstadoLancamento::Realizado);
    }

    #[test]
    fn devolucao_estorna_e_repoe_estoque() {
        let p = pedido();
        let linhas: Vec<_> = p.itens.iter().map(|i| (i.id, i.quantidade)).collect();
        let d = Devolucao::solicitar(&p, &linhas, "avaria", hoje()).unwrap();
        let l =
            concluir_devolucao(&d, true, Dinheiro::reais(400), contas_fat(), autoria()).unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().estado, EstadoLancamento::Realizado);
    }
}
