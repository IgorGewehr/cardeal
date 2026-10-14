//! O DFC (demonstrativo do fluxo de caixa) pelo método direto: de onde o dinheiro entrou e
//! para onde saiu no período, separado em atividades operacionais, de investimento e de
//! financiamento, mais o saldo de abertura e o de fechamento do caixa. E pelo método
//! **indireto** ([`montar_indireto`]): do lucro líquido ao caixa, pelas variações das contas
//! patrimoniais — por que o lucro não virou dinheiro (ou virou mais do que ele).
//!
//! Cada lançamento realizado que mexe no caixa é aberto pela contraparte: o valor que cada
//! conta de contrapartida carrega vira entrada ou saída na linha daquela conta (um
//! recebimento com juros dá "Recebimentos de clientes" **e** "Receitas financeiras", não uma
//! linha só). Transferência entre contas de caixa não tem contraparte e some. Domínio puro
//! sobre os movimentos já lidos do Razão.

use cardeal_kernel::{Dinheiro, Periodo};
use cardeal_ledger::{MovimentoDeCaixa, Natureza, PapelConta, VariacaoPatrimonial};
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

/// O DFC pelo método indireto: o lucro líquido do período (o da DRE), ajustado pelas variações
/// das contas patrimoniais fora do caixa, chega à mesma variação do caixa do método direto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DfcIndireto {
    /// O período.
    pub periodo: Periodo,
    /// O caixa no início do período.
    pub saldo_inicial: Dinheiro,
    /// O lucro líquido da DRE (primeira linha das operacionais).
    pub lucro_liquido: Dinheiro,
    /// Operacional (lucro + ajustes), investimento e financiamento, nesta ordem.
    pub blocos: Vec<BlocoDfc>,
    /// O que sobra quando um lançamento tem competência num período e liquidação noutro (a
    /// DRE o conta num, o caixa no outro); zero no caso normal. Já está no bloco operacional.
    pub diferenca_de_datas: Dinheiro,
    /// A variação do caixa — a mesma do método direto.
    pub variacao: Dinheiro,
    /// O caixa no fim do período.
    pub saldo_final: Dinheiro,
}

/// O rótulo da linha "Lucro líquido do período".
pub const LUCRO_LIQUIDO: &str = "Lucro líquido do período";
/// O rótulo da diferença de datas entre competência e caixa.
pub const DIFERENCA_DE_DATAS: &str = "Diferença de datas (competência × caixa)";

/// Depreciação e amortização não saem do caixa: voltam ao lucro nas operacionais.
fn e_depreciacao(nome: &str) -> bool {
    let n = nome.to_lowercase();
    n.contains("deprecia") || n.contains("amortiza") || n.contains("exaust")
}

/// O bloco (0 operacional, 1 investimento, 2 financiamento) e o rótulo da variação de uma conta.
/// Vale, nesta ordem: depreciação (operacional); o grupo de fluxo gravado na conta; o papel;
/// o imobilizado pelo código (1.4); o resto é operacional.
fn bloco_da_variacao(v: &VariacaoPatrimonial) -> (usize, String) {
    if e_depreciacao(&v.nome) {
        return (0, "Depreciação e amortização (não sai do caixa)".to_owned());
    }
    let papel = v.papel.as_deref().unwrap_or("");
    let financiamento = match papel {
        "Emprestimos" => Some("Empréstimos"),
        "Capital" => Some("Aportes de capital"),
        "Retiradas" => Some("Retiradas e pró-labore"),
        "LucrosAcumulados" => Some("Lucros distribuídos"),
        _ => None,
    };
    let bloco = match v.grupo_fluxo.as_deref() {
        Some("Investimento") => 1,
        Some("Financiamento") => 2,
        _ if financiamento.is_some() => 2,
        _ if papel == "Aplicacoes" || v.codigo.starts_with("1.4") => 1,
        _ => 0,
    };
    let rotulo = match bloco {
        2 => financiamento.map_or_else(|| v.nome.clone(), str::to_owned),
        1 if papel == "Aplicacoes" => "Aplicações financeiras".to_owned(),
        1 if v.codigo.starts_with("1.4") => "Imobilizado (compras e vendas)".to_owned(),
        1 => v.nome.clone(),
        _ => match papel {
            "ClientesAReceber" | "CartoesAReceber" | "ChequesAReceber" => {
                "(Aumento) redução de contas a receber".to_owned()
            }
            "EstoqueMercadorias"
            | "EstoqueMateriaPrima"
            | "EstoqueEmProcesso"
            | "EstoqueAcabado" => "(Aumento) redução de estoques".to_owned(),
            "ImpostosARecuperar" | "AdiantamentoFornecedor" => {
                "(Aumento) redução de outros créditos".to_owned()
            }
            "Fornecedores" => "Aumento (redução) de fornecedores".to_owned(),
            "ImpostosARecolher" => "Aumento (redução) de impostos a recolher".to_owned(),
            "SalariosAPagar" => "Aumento (redução) de salários e encargos".to_owned(),
            "AdiantamentoCliente" | "CartoesARepassar" => {
                "Aumento (redução) de outras obrigações".to_owned()
            }
            _ if v.natureza == Natureza::Ativo => format!("(Aumento) redução de {}", v.nome),
            _ => format!("Aumento (redução) de {}", v.nome),
        },
    };
    (bloco, rotulo)
}

/// Monta o DFC indireto a partir do direto (o caixa: abertura, variação e fechamento), do
/// lucro líquido da DRE e das variações patrimoniais do mesmo período. Cada variação entra com
/// o sinal trocado: ativo que cresce prendeu dinheiro, passivo que cresce o soltou.
#[must_use]
pub fn montar_indireto(
    direto: &Dfc,
    lucro_liquido: Dinheiro,
    variacoes: &[VariacaoPatrimonial],
) -> DfcIndireto {
    let mut linhas: [Vec<LinhaDfc>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    for v in variacoes {
        let (bloco, rotulo) = bloco_da_variacao(v);
        let valor = -v.valor;
        if let Some(l) = linhas[bloco].iter_mut().find(|l| l.rotulo == rotulo) {
            l.valor += valor;
        } else {
            linhas[bloco].push(LinhaDfc { rotulo, valor });
        }
    }
    for ls in &mut linhas {
        ls.retain(|l| !l.valor.e_zero());
        ls.sort_by_key(|l| std::cmp::Reverse(l.valor.abs()));
    }
    let ajustes: Dinheiro = linhas.iter().flatten().map(|l| l.valor).sum();
    let diferenca_de_datas = direto.variacao - lucro_liquido - ajustes;
    linhas[0].insert(
        0,
        LinhaDfc {
            rotulo: LUCRO_LIQUIDO.to_owned(),
            valor: lucro_liquido,
        },
    );
    if !diferenca_de_datas.e_zero() {
        linhas[0].push(LinhaDfc {
            rotulo: DIFERENCA_DE_DATAS.to_owned(),
            valor: diferenca_de_datas,
        });
    }
    let nomes = [
        "Atividades operacionais",
        "Atividades de investimento",
        "Atividades de financiamento",
    ];
    let blocos = linhas
        .into_iter()
        .zip(nomes)
        .map(|(ls, nome)| BlocoDfc {
            nome: nome.to_owned(),
            total: ls.iter().map(|l| l.valor).sum(),
            linhas: ls,
        })
        .collect();
    DfcIndireto {
        periodo: direto.periodo,
        saldo_inicial: direto.saldo_inicial,
        lucro_liquido,
        blocos,
        diferenca_de_datas,
        variacao: direto.variacao,
        saldo_final: direto.saldo_final,
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

    fn var(
        codigo: &str,
        nome: &str,
        natureza: Natureza,
        papel: Option<&str>,
        valor: i64,
    ) -> VariacaoPatrimonial {
        VariacaoPatrimonial {
            codigo: codigo.into(),
            nome: nome.into(),
            natureza,
            papel: papel.map(Into::into),
            grupo_fluxo: None,
            valor: Dinheiro::reais(valor),
        }
    }

    fn direto(variacao: i64) -> Dfc {
        Dfc {
            periodo: periodo(),
            saldo_inicial: Dinheiro::reais(100),
            blocos: Vec::new(),
            variacao: Dinheiro::reais(variacao),
            saldo_final: Dinheiro::reais(100 + variacao),
        }
    }

    #[test]
    fn indireto_vai_do_lucro_ao_caixa_pelas_variacoes() {
        // Lucro de 300 (com 20 de depreciação); vendeu a prazo e o a receber subiu 250; o estoque
        // caiu 80; fornecedores subiu 40; comprou máquina de 500; tomou empréstimo de 600 e
        // pagou 100 de retirada. Caixa: 300 + 20 − 250 + 80 + 40 − 500 + 600 − 100 = 190.
        let variacoes = [
            var(
                "1.2.01",
                "Clientes a receber",
                Natureza::Ativo,
                Some("ClientesAReceber"),
                250,
            ),
            var(
                "1.3.01",
                "Mercadorias",
                Natureza::Ativo,
                Some("EstoqueMercadorias"),
                -80,
            ),
            var("1.4.01", "Máquinas", Natureza::Ativo, None, 500),
            var(
                "1.4.02",
                "Depreciação acumulada",
                Natureza::Ativo,
                None,
                -20,
            ),
            var(
                "2.1.01",
                "Fornecedores",
                Natureza::Passivo,
                Some("Fornecedores"),
                -40,
            ),
            var(
                "2.1.04",
                "Empréstimos",
                Natureza::Passivo,
                Some("Emprestimos"),
                -600,
            ),
            var(
                "3.3",
                "Retiradas",
                Natureza::PatrimonioLiquido,
                Some("Retiradas"),
                100,
            ),
        ];
        let d = montar_indireto(&direto(190), Dinheiro::reais(300), &variacoes);
        assert_eq!(d.diferenca_de_datas, Dinheiro::ZERO);
        let op = &d.blocos[0];
        assert_eq!(op.linhas[0].rotulo, LUCRO_LIQUIDO);
        assert_eq!(op.total, Dinheiro::reais(190)); // 300 + 20 − 250 + 80 + 40
        let valor = |b: usize, rotulo: &str| {
            d.blocos[b]
                .linhas
                .iter()
                .find(|l| l.rotulo == rotulo)
                .map(|l| l.valor)
        };
        assert_eq!(
            valor(0, "(Aumento) redução de contas a receber"),
            Some(Dinheiro::reais(-250))
        );
        assert_eq!(
            valor(0, "Depreciação e amortização (não sai do caixa)"),
            Some(Dinheiro::reais(20))
        );
        assert_eq!(d.blocos[1].total, Dinheiro::reais(-500));
        assert_eq!(
            valor(1, "Imobilizado (compras e vendas)"),
            Some(Dinheiro::reais(-500))
        );
        assert_eq!(d.blocos[2].total, Dinheiro::reais(500));
        let soma: Dinheiro = d.blocos.iter().map(|b| b.total).sum();
        assert_eq!(soma, d.variacao);
        assert_eq!(d.saldo_final, Dinheiro::reais(290));
    }

    #[test]
    fn diferenca_de_datas_aparece_em_vez_de_sumir() {
        // O caixa variou 50 a mais do que lucro + variações explicam (recebimento com
        // competência no mês anterior): vira uma linha à vista, e o total continua o do caixa.
        let d = montar_indireto(&direto(150), Dinheiro::reais(100), &[]);
        assert_eq!(d.diferenca_de_datas, Dinheiro::reais(50));
        assert_eq!(
            d.blocos[0].linhas.last().map(|l| l.rotulo.as_str()),
            Some(DIFERENCA_DE_DATAS)
        );
        assert_eq!(d.blocos[0].total, Dinheiro::reais(150));
    }
}
