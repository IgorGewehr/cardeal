//! O receituário contábil do estoque — só os movimentos que **não têm um módulo de
//! venda/consumo do outro lado** (`docs/modulos/estoque.md` §7).
//!
//! Consumo por venda, OS ou produção é lançado pelo módulo que consome: ele chama a saída,
//! recebe o `custo_medio` de volta ([`SaidaAplicada`](crate::SaidaAplicada)) e monta o
//! próprio lançamento. Aqui ficam ajuste de inventário, perda, transferência e entrada
//! avulsa.
//!
//! As contas chegam já resolvidas por papel ([`PapelConta`](cardeal_ledger::PapelConta)) —
//! o módulo não conhece código de conta (`docs/contratos-internos.md` §6, regra 4).

// As funções do receituário levam empresa + agregado + quantidade + valor + contas + data +
// autoria; agrupar isso em struct só empurraria a verbosidade para o call site.
#![allow(clippy::too_many_arguments)]

use cardeal_kernel::{Data, Dinheiro, Id, Instante, Quantidade};
use cardeal_ledger::{ConstrutorLancamento, LancamentoBalanceado, Origem};

use crate::erros::ErroEstoque;
use crate::inventario::AjusteInventario;

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

/// As contas do Razão que os lançamentos do estoque usam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContasEstoque {
    /// Estoque de mercadorias (1.3.01).
    pub estoque: Id,
    /// Perdas de estoque (5.x).
    pub perdas: Id,
    /// Outras receitas (4.6) — sobra de inventário, entrada avulsa.
    pub outras_receitas: Id,
}

fn origem(tipo: &str, id: Id) -> Origem {
    Origem::modulo("estoque").tipo(tipo).agregado(id)
}

fn lancamento_simples(
    empresa: Id,
    data: Data,
    historico: String,
    origem: Origem,
    autoria: Autoria,
    realizado: bool,
    debito: (Id, Dinheiro),
    credito: (Id, Dinheiro),
    quantidade: Option<Quantidade>,
) -> Result<LancamentoBalanceado, ErroEstoque> {
    let mut c = ConstrutorLancamento::novo(empresa, data, historico)
        .origem(origem)
        .criado_por(autoria.usuario, autoria.dispositivo)
        .agora(autoria.agora);
    if realizado {
        c = c.liquidacao(data);
    }
    c = c.debitar(debito.0, debito.1);
    if let Some(q) = quantidade {
        c = c.quantidade(q);
    }
    c = c.creditar(credito.0, credito.1);
    if let Some(q) = quantidade {
        c = c.quantidade(q);
    }
    c.construir().map_err(|_| ErroEstoque::QuantidadeInvalida)
}

/// Lançamento de um ajuste de inventário (`docs/modulos/estoque.md` §7).
///
/// - Sobra (`delta > 0`): D Estoque / C Outras receitas, `Realizado`.
/// - Falta (`delta < 0`): D Perdas / C Estoque, `Realizado`.
///
/// `valor` é `|delta| × custo_medio` já apurado pelo chamador. Um `delta` zero devolve `None`.
///
/// # Errors
/// [`ErroEstoque::QuantidadeInvalida`] se o construtor do Razão recusar (não deve ocorrer).
pub fn ajuste_de_inventario(
    empresa: Id,
    inventario: Id,
    ajuste: &AjusteInventario,
    valor: Dinheiro,
    contas: ContasEstoque,
    data: Data,
    autoria: Autoria,
) -> Result<Option<LancamentoBalanceado>, ErroEstoque> {
    if ajuste.delta.e_zero() || valor.e_zero() {
        return Ok(None);
    }
    let sobra = ajuste.delta.e_positiva();
    let (debito, credito, historico) = if sobra {
        (
            (contas.estoque, valor),
            (contas.outras_receitas, valor),
            "Ajuste de inventário — sobra na contagem".to_string(),
        )
    } else {
        (
            (contas.perdas, valor),
            (contas.estoque, valor),
            "Ajuste de inventário — falta na contagem".to_string(),
        )
    };
    lancamento_simples(
        empresa,
        data,
        historico,
        origem("inventario", inventario),
        autoria,
        true,
        debito,
        credito,
        Some(ajuste.delta.abs()),
    )
    .map(Some)
}

/// Lançamento de uma perda por vencimento/quebra: D Perdas / C Estoque, `Realizado`.
///
/// # Errors
/// [`ErroEstoque::QuantidadeInvalida`].
pub fn perda(
    empresa: Id,
    movimento: Id,
    quantidade: Quantidade,
    valor: Dinheiro,
    contas: ContasEstoque,
    data: Data,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroEstoque> {
    lancamento_simples(
        empresa,
        data,
        "Perda de estoque (vencimento/quebra)".to_string(),
        origem("perda", movimento),
        autoria,
        true,
        (contas.perdas, valor),
        (contas.estoque, valor),
        Some(quantidade),
    )
}

/// Lançamento de uma transferência entre locais da mesma empresa: D Estoque (destino) /
/// C Estoque (origem), `Realizado` (`docs/modulos/estoque.md` §7 e §11.6).
///
/// # Errors
/// [`ErroEstoque::LocalIgual`] se as contas de origem e destino coincidem;
/// [`ErroEstoque::QuantidadeInvalida`].
pub fn transferencia(
    empresa: Id,
    transferencia: Id,
    quantidade: Quantidade,
    valor: Dinheiro,
    conta_origem: Id,
    conta_destino: Id,
    data: Data,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroEstoque> {
    if conta_origem == conta_destino {
        return Err(ErroEstoque::LocalIgual);
    }
    lancamento_simples(
        empresa,
        data,
        "Transferência de estoque entre locais".to_string(),
        origem("transferencia", transferencia),
        autoria,
        true,
        (conta_destino, valor),
        (conta_origem, valor),
        Some(quantidade),
    )
}

/// Lançamento de uma entrada avulsa sem nota de compra (doação, brinde): D Estoque /
/// C Outras receitas, `Confirmado`.
///
/// # Errors
/// [`ErroEstoque::QuantidadeInvalida`].
pub fn entrada_avulsa(
    empresa: Id,
    movimento: Id,
    quantidade: Quantidade,
    valor: Dinheiro,
    contas: ContasEstoque,
    data: Data,
    autoria: Autoria,
) -> Result<LancamentoBalanceado, ErroEstoque> {
    lancamento_simples(
        empresa,
        data,
        "Entrada de estoque sem nota (doação/brinde)".to_string(),
        origem("entrada_avulsa", movimento),
        autoria,
        false,
        (contas.estoque, valor),
        (contas.outras_receitas, valor),
        Some(quantidade),
    )
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Fuso;
    use cardeal_ledger::EstadoLancamento;

    use super::*;

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

    fn contas() -> ContasEstoque {
        ContasEstoque {
            estoque: Id::novo(),
            perdas: Id::novo(),
            outras_receitas: Id::novo(),
        }
    }

    #[test]
    fn ajuste_de_sobra_credita_outras_receitas() {
        let ajuste = AjusteInventario {
            produto: Id::novo(),
            variacao: None,
            lote: None,
            delta: Quantidade::unidades(3),
        };
        let l = ajuste_de_inventario(
            Id::novo(),
            Id::novo(),
            &ajuste,
            Dinheiro::reais(12),
            contas(),
            hoje(),
            autoria(),
        )
        .unwrap()
        .unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().estado, EstadoLancamento::Realizado);
        assert_eq!(l.interno().total_debitos(), Dinheiro::reais(12));
    }

    #[test]
    fn ajuste_zero_nao_gera_lancamento() {
        let ajuste = AjusteInventario {
            produto: Id::novo(),
            variacao: None,
            lote: None,
            delta: Quantidade::ZERO,
        };
        assert!(ajuste_de_inventario(
            Id::novo(),
            Id::novo(),
            &ajuste,
            Dinheiro::ZERO,
            contas(),
            hoje(),
            autoria(),
        )
        .unwrap()
        .is_none());
    }

    #[test]
    fn transferencia_para_o_mesmo_local_e_recusada() {
        let conta = Id::novo();
        assert_eq!(
            transferencia(
                Id::novo(),
                Id::novo(),
                Quantidade::unidades(1),
                Dinheiro::reais(1),
                conta,
                conta,
                hoje(),
                autoria(),
            )
            .unwrap_err(),
            ErroEstoque::LocalIgual
        );
    }

    #[test]
    fn perda_e_entrada_avulsa_balanceiam() {
        let l = perda(
            Id::novo(),
            Id::novo(),
            Quantidade::unidades(5),
            Dinheiro::reais(20),
            contas(),
            hoje(),
            autoria(),
        )
        .unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().estado, EstadoLancamento::Realizado);

        let l = entrada_avulsa(
            Id::novo(),
            Id::novo(),
            Quantidade::unidades(2),
            Dinheiro::reais(8),
            contas(),
            hoje(),
            autoria(),
        )
        .unwrap();
        assert!(l.interno().esta_balanceado());
        assert_eq!(l.interno().estado, EstadoLancamento::Confirmado);
    }
}
