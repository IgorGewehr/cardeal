//! O DFC (demonstrativo do fluxo de caixa) pelo método direto: de onde o dinheiro entrou e
//! para onde saiu no período, separado em atividades operacionais, de investimento e de
//! financiamento, mais o saldo de abertura e o de fechamento do caixa.
//!
//! Cada lançamento realizado que mexe no caixa é aberto pela contraparte: o valor que cada
//! conta de contrapartida carrega vira entrada ou saída na linha daquela conta (um
//! recebimento com juros dá "Recebimentos de clientes" **e** "Receitas financeiras", não uma
//! linha só). Transferência entre contas de caixa não tem contraparte e some. Domínio puro
//! sobre os movimentos já lidos do Razão.

use cardeal_kernel::{Dinheiro, Periodo};
use cardeal_ledger::{MovimentoDeCaixa, PapelConta};
use serde::{Deserialize, Serialize};

/// Uma linha do DFC.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaDfc {
    /// "Recebimentos de clientes", "Pagamentos a fornecedores"…
    pub rotulo: String,
    /// Positivo = entrou, negativo = saiu.
    pub valor: Dinheiro,
}

/// Um bloco de atividades.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlocoDfc {
    /// "Atividades operacionais", "… de investimento", "… de financiamento".
    pub nome: String,
    /// As linhas, da maior para a menor (em valor absoluto).
    pub linhas: Vec<LinhaDfc>,
    /// A soma do bloco.
    pub total: Dinheiro,
}

/// O DFC de um período.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dfc {
    /// O período.
    pub periodo: Periodo,
    /// O caixa no início do período.
    pub saldo_inicial: Dinheiro,
    /// Operacional, investimento e financiamento, nesta ordem.
    pub blocos: Vec<BlocoDfc>,
    /// A variação do caixa no período (soma dos blocos).
    pub variacao: Dinheiro,
    /// O caixa no fim do período.
    pub saldo_final: Dinheiro,
}

fn bloco_e_rotulo(papel: Option<&str>, grupo: Option<&str>, nome: &str) -> (usize, String) {
    let p = |x: PapelConta| papel == Some(x.to_string().as_str());
    let rotulo = if p(PapelConta::ClientesAReceber) {
        "Recebimentos de clientes".to_owned()
    } else if p(PapelConta::Fornecedores) {
        "Pagamentos a fornecedores".to_owned()
    } else if p(PapelConta::SalariosAPagar) {
        "Salários e encargos".to_owned()
    } else if p(PapelConta::ImpostosARecolher) {
        "Impostos recolhidos".to_owned()
    } else if p(PapelConta::ReceitaFinanceira) {
        "Receitas financeiras".to_owned()
    } else if p(PapelConta::DespesaFinanceira) {
        "Despesas financeiras".to_owned()
    } else if p(PapelConta::Emprestimos) {
        "Empréstimos".to_owned()
    } else if p(PapelConta::Capital) {
        "Aportes de capital".to_owned()
    } else if p(PapelConta::Retiradas) {
        "Retiradas e pró-labore".to_owned()
    } else if p(PapelConta::Aplicacoes) {
        "Aplicações financeiras".to_owned()
    } else if matches!(
        papel,
        Some(
            "ReceitaVendas"
                | "ReceitaServicos"
                | "ReceitaAlugueis"
                | "ReceitaHospedagem"
                | "OutrasReceitas"
                | "DescontosConcedidos"
                | "DevolucoesVenda"
        )
    ) {
        "Vendas e receitas recebidas à vista".to_owned()
    } else if matches!(
        papel,
        Some(
            "Cmv"
                | "CustoServico"
                | "DespesaPessoal"
                | "DespesaAdministrativa"
                | "DespesaComercial"
                | "Ocupacao"
                | "TaxasCartao"
                | "ImpostosSobreVenda"
                | "Perdas"
                | "QuebraCaixa"
        )
    ) {
        "Custos e despesas pagos à vista".to_owned()
    } else {
        nome.to_owned()
    };
    let bloco = match grupo {
        Some("Investimento") => 1,
        Some("Financiamento") => 2,
        _ => 0,
    };
    (bloco, rotulo)
}

/// Monta o DFC a partir do saldo de abertura e dos movimentos de caixa do período.
#[must_use]
pub fn montar(periodo: Periodo, abertura: Dinheiro, movimentos: &[MovimentoDeCaixa]) -> Dfc {
    let mut linhas: [Vec<LinhaDfc>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut soma = |bloco: usize, rotulo: String, valor: Dinheiro| {
        if let Some(l) = linhas[bloco].iter_mut().find(|l| l.rotulo == rotulo) {
            l.valor += valor;
        } else {
            linhas[bloco].push(LinhaDfc { rotulo, valor });
        }
    };
    for m in movimentos {
        for c in &m.contrapartidas {
            let (bloco, rotulo) =
                bloco_e_rotulo(c.papel.as_deref(), c.grupo_fluxo.as_deref(), &c.nome);
            // O que a contrapartida carrega é, com o sinal trocado, o que entrou no caixa.
            soma(bloco, rotulo, -c.valor);
        }
    }
    let nomes = [
        "Atividades operacionais",
        "Atividades de investimento",
        "Atividades de financiamento",
    ];
    let mut variacao = Dinheiro::ZERO;
    let blocos = linhas
        .into_iter()
        .zip(nomes)
        .map(|(mut ls, nome)| {
            ls.retain(|l| !l.valor.e_zero());
            ls.sort_by_key(|l| std::cmp::Reverse(l.valor.abs()));
            let total: Dinheiro = ls.iter().map(|l| l.valor).sum();
            variacao += total;
            BlocoDfc {
                nome: nome.to_owned(),
                linhas: ls,
                total,
            }
        })
        .collect();
    Dfc {
        periodo,
        saldo_inicial: abertura,
        blocos,
        variacao,
        saldo_final: abertura + variacao,
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Data, Id};
    use cardeal_ledger::ContrapartidaDeCaixa;

    use super::*;

    fn periodo() -> Periodo {
        Periodo::novo(
            Data::de_ymd(2026, 10, 1).unwrap(),
            Data::de_ymd(2026, 10, 31).unwrap(),
        )
    }

    fn contra(papel: &str, grupo: Option<&str>, valor: i64) -> ContrapartidaDeCaixa {
        ContrapartidaDeCaixa {
            codigo: "x".into(),
            nome: papel.into(),
            papel: Some(papel.into()),
            grupo_fluxo: grupo.map(Into::into),
            valor: Dinheiro::reais(valor),
        }
    }

    fn mov(caixa: i64, contrapartidas: Vec<ContrapartidaDeCaixa>) -> MovimentoDeCaixa {
        MovimentoDeCaixa {
            lancamento: Id::novo(),
            data: Data::de_ymd(2026, 10, 10).unwrap(),
            caixa: Dinheiro::reais(caixa),
            contrapartidas,
        }
    }

    #[test]
    fn recebimento_com_juros_abre_em_duas_linhas_e_fecha_o_caixa() {
        let movs = vec![
            // Recebeu 105: baixa de 100 de cliente + 5 de juros.
            mov(
                105,
                vec![
                    contra("ClientesAReceber", None, -100),
                    contra("ReceitaFinanceira", None, -5),
                ],
            ),
            // Pagou 40 a fornecedor.
            mov(-40, vec![contra("Fornecedores", None, 40)]),
            // Tomou empréstimo de 1.000 (financiamento) e comprou aplicação de 300.
            mov(
                1_000,
                vec![contra("Emprestimos", Some("Financiamento"), -1_000)],
            ),
            mov(-300, vec![contra("Aplicacoes", Some("Investimento"), 300)]),
            // Transferência entre contas de caixa: sem contraparte.
            mov(0, vec![]),
        ];
        let d = montar(periodo(), Dinheiro::reais(500), &movs);
        let operacional = &d.blocos[0];
        assert_eq!(operacional.total, Dinheiro::reais(65));
        let rot: Vec<(&str, i64)> = operacional
            .linhas
            .iter()
            .map(|l| (l.rotulo.as_str(), l.valor.em_centavos() / 100))
            .collect();
        assert_eq!(
            rot,
            [
                ("Recebimentos de clientes", 100),
                ("Pagamentos a fornecedores", -40),
                ("Receitas financeiras", 5)
            ]
        );
        assert_eq!(d.blocos[1].total, Dinheiro::reais(-300));
        assert_eq!(d.blocos[2].total, Dinheiro::reais(1_000));
        assert_eq!(d.variacao, Dinheiro::reais(765));
        assert_eq!(d.saldo_final, Dinheiro::reais(1_265));
    }

    #[test]
    fn variacao_e_sempre_a_soma_do_caixa_dos_movimentos() {
        // Cada contrapartida carrega, com o sinal trocado, o que entrou: a soma bate.
        let movs = vec![
            mov(
                70,
                vec![contra("ReceitaVendas", None, -100), contra("Cmv", None, 30)],
            ),
            mov(-12, vec![contra("DespesaAdministrativa", None, 12)]),
        ];
        let d = montar(periodo(), Dinheiro::ZERO, &movs);
        let esperado: Dinheiro = movs.iter().map(|m| m.caixa).sum();
        assert_eq!(d.variacao, esperado);
        assert_eq!(d.saldo_final, esperado);
    }
}
