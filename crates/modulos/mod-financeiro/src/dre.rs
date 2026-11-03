//! A DRE gerencial: o resultado do período lido direto do Razão, na estrutura que o dono do
//! negócio reconhece — Receita Bruta › Deduções › Receita Líquida › CMV/CSV › Lucro Bruto ›
//! Despesas Operacionais › EBITDA › Resultado Financeiro › Lucro Líquido.
//!
//! O cálculo é puro: [`montar`] recebe as linhas de resultado já lidas
//! ([`cardeal_ledger::LinhaResultado`]) e devolve a demonstração. Cada valor é um
//! *contribuição com sinal* (o que entra soma, o que sai subtrai), então qualquer subtotal é
//! a soma das linhas acima dele — e o Lucro Líquido bate com receitas menos despesas do
//! Razão por construção (há um teste que prova isso para qualquer mistura de contas).

use cardeal_kernel::{Dinheiro, Id, Percentual, Periodo};
use cardeal_ledger::{LinhaResultado, Natureza, PapelConta};
use serde::{Deserialize, Serialize};

/// Como a linha é lida na tela.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TipoLinhaDre {
    /// Uma conta do plano (detalhe).
    Conta,
    /// Total de um bloco (Receita Bruta, Deduções, CMV…).
    Bloco,
    /// Resultado acumulado até ali (Receita Líquida, Lucro Bruto, EBITDA, Lucro Líquido).
    Resultado,
}

/// Uma linha da DRE.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaDre {
    /// O rótulo.
    pub rotulo: String,
    /// O código da conta, quando é [`TipoLinhaDre::Conta`].
    pub codigo: Option<String>,
    /// Como exibir.
    pub tipo: TipoLinhaDre,
    /// Contribuição com sinal para o resultado.
    pub valor: Dinheiro,
}

/// A DRE de um período.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dre {
    /// O período apurado.
    pub periodo: Periodo,
    /// As linhas, em ordem de exibição.
    pub linhas: Vec<LinhaDre>,
    /// Receita bruta (vendas + serviços + aluguéis + hospedagem).
    pub receita_bruta: Dinheiro,
    /// Receita líquida (bruta − deduções).
    pub receita_liquida: Dinheiro,
    /// Lucro bruto (líquida − CMV/CSV).
    pub lucro_bruto: Dinheiro,
    /// Resultado operacional antes do financeiro.
    pub ebitda: Dinheiro,
    /// Lucro líquido do período.
    pub lucro_liquido: Dinheiro,
}

impl Dre {
    /// O valor da linha de [`TipoLinhaDre::Resultado`] ou [`TipoLinhaDre::Bloco`] pelo rótulo.
    #[must_use]
    pub fn valor_de(&self, rotulo: &str) -> Option<Dinheiro> {
        self.linhas
            .iter()
            .find(|l| l.tipo != TipoLinhaDre::Conta && l.rotulo == rotulo)
            .map(|l| l.valor)
    }

    /// Margem líquida: lucro líquido sobre a receita líquida; `None` sem receita.
    #[must_use]
    pub fn margem_liquida(&self) -> Option<Percentual> {
        razao(self.lucro_liquido, self.receita_liquida)
    }

    /// Margem bruta: lucro bruto sobre a receita líquida; `None` sem receita.
    #[must_use]
    pub fn margem_bruta(&self) -> Option<Percentual> {
        razao(self.lucro_bruto, self.receita_liquida)
    }
}

fn razao(parte: Dinheiro, todo: Dinheiro) -> Option<Percentual> {
    (!todo.e_zero()).then(|| Percentual::da_razao(parte.em_centavos(), todo.em_centavos()))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Bloco {
    ReceitaBruta,
    Deducoes,
    Cmv,
    OutrasReceitas,
    DespesasOperacionais,
    ReceitaFinanceira,
    DespesaFinanceira,
}

fn bloco_de(l: &LinhaResultado) -> Bloco {
    use PapelConta as P;
    let papel = l.papel.as_deref().unwrap_or("");
    let e = |p: P| papel == p.to_string();
    if e(P::ReceitaVendas)
        || e(P::ReceitaServicos)
        || e(P::ReceitaAlugueis)
        || e(P::ReceitaHospedagem)
    {
        Bloco::ReceitaBruta
    } else if e(P::DescontosConcedidos) || e(P::DevolucoesVenda) || e(P::ImpostosSobreVenda) {
        Bloco::Deducoes
    } else if e(P::Cmv) || e(P::CustoServico) {
        Bloco::Cmv
    } else if e(P::ReceitaFinanceira) {
        Bloco::ReceitaFinanceira
    } else if e(P::DespesaFinanceira) {
        Bloco::DespesaFinanceira
    } else if l.natureza == Natureza::Receita {
        // OutrasReceitas e qualquer receita sem papel conhecido.
        Bloco::OutrasReceitas
    } else {
        // Pessoal, administrativa, comercial, ocupação, taxas, perdas, quebra e o resto.
        Bloco::DespesasOperacionais
    }
}

/// Contribuição com sinal de uma linha: o valor natural já vem positivo para o
/// comportamento normal da conta (receita creditada, despesa debitada); despesa e redutora
/// de receita entram negativas.
fn contribuicao(l: &LinhaResultado, bloco: Bloco) -> Dinheiro {
    match (bloco, l.natureza) {
        // Redutoras de receita (4.4/4.5) são contas de natureza Receita com saldo devedor:
        // `valor` já vem negativo, que é o sinal certo. O imposto sobre venda é despesa.
        (Bloco::Deducoes, Natureza::Despesa)
        | (Bloco::Cmv | Bloco::DespesasOperacionais | Bloco::DespesaFinanceira, _) => -l.valor,
        _ => l.valor,
    }
}

/// O rótulo do bloco da DRE em que a conta entra.
#[must_use]
pub fn rotulo_do_bloco(l: &LinhaResultado) -> &'static str {
    match bloco_de(l) {
        Bloco::ReceitaBruta => "Receita bruta",
        Bloco::Deducoes => "(−) Deduções",
        Bloco::Cmv => "(−) CMV / custo do serviço",
        Bloco::OutrasReceitas => "(+) Outras receitas",
        Bloco::DespesasOperacionais => "(−) Despesas operacionais",
        Bloco::ReceitaFinanceira => "(+) Receitas financeiras",
        Bloco::DespesaFinanceira => "(−) Despesas financeiras",
    }
}

/// Se a conta soma no resultado (receitas) ou subtrai (deduções, custos, despesas).
#[must_use]
pub fn soma_no_resultado(l: &LinhaResultado) -> bool {
    matches!(
        bloco_de(l),
        Bloco::ReceitaBruta | Bloco::OutrasReceitas | Bloco::ReceitaFinanceira
    )
}

/// O realizado da conta no sentido em que se orça: positivo no comportamento normal dela
/// (a receita vendida, a despesa gasta, o desconto concedido).
#[must_use]
pub fn no_sentido_do_orcado(l: &LinhaResultado) -> Dinheiro {
    let c = contribuicao(l, bloco_de(l));
    if soma_no_resultado(l) {
        c
    } else {
        -c
    }
}

/// A linha de resultado de uma conta com o valor orçado (positivo) no lugar do realizado —
/// para montar a DRE do orçamento com a mesma regra de sinais.
#[must_use]
pub fn linha_orcada(conta: &LinhaResultado, orcado: Dinheiro) -> LinhaResultado {
    let mut l = conta.clone();
    let contribuicao_desejada = if soma_no_resultado(conta) {
        orcado
    } else {
        -orcado
    };
    // `contribuicao` devolve `valor` ou `−valor` conforme o bloco e a natureza.
    l.valor = orcado;
    if contribuicao(&l, bloco_de(&l)) != contribuicao_desejada {
        l.valor = -orcado;
    }
    l
}

/// Monta a DRE do período a partir do resultado por conta.
#[must_use]
pub fn montar(periodo: Periodo, linhas: &[LinhaResultado]) -> Dre {
    montar_mantendo(periodo, linhas, &[])
}

/// Como [`montar`], mantendo as linhas das contas `sempre` mesmo zeradas (as orçadas, para o
/// orçado × realizado mostrar a conta orçada que não teve movimento).
#[must_use]
pub fn montar_mantendo(periodo: Periodo, linhas: &[LinhaResultado], sempre: &[Id]) -> Dre {
    let mut dre = Dre {
        periodo,
        linhas: Vec::new(),
        receita_bruta: Dinheiro::ZERO,
        receita_liquida: Dinheiro::ZERO,
        lucro_bruto: Dinheiro::ZERO,
        ebitda: Dinheiro::ZERO,
        lucro_liquido: Dinheiro::ZERO,
    };

    let bloco = |dre: &mut Dre, rotulo: &str, qual: Bloco| -> Dinheiro {
        let mut itens: Vec<(&LinhaResultado, Dinheiro)> = linhas
            .iter()
            .filter(|l| bloco_de(l) == qual)
            .map(|l| (l, contribuicao(l, qual)))
            .filter(|(l, v)| !v.e_zero() || sempre.contains(&l.conta))
            .collect();
        itens.sort_by(|a, b| a.0.codigo.cmp(&b.0.codigo));
        let total: Dinheiro = itens.iter().map(|(_, v)| *v).sum();
        dre.linhas.push(LinhaDre {
            rotulo: rotulo.to_owned(),
            codigo: None,
            tipo: TipoLinhaDre::Bloco,
            valor: total,
        });
        for (l, v) in itens {
            dre.linhas.push(LinhaDre {
                rotulo: l.nome.clone(),
                codigo: Some(l.codigo.clone()),
                tipo: TipoLinhaDre::Conta,
                valor: v,
            });
        }
        total
    };
    let resultado = |dre: &mut Dre, rotulo: &str, valor: Dinheiro| {
        dre.linhas.push(LinhaDre {
            rotulo: rotulo.to_owned(),
            codigo: None,
            tipo: TipoLinhaDre::Resultado,
            valor,
        });
    };

    let bruta = bloco(&mut dre, "Receita bruta", Bloco::ReceitaBruta);
    let deducoes = bloco(&mut dre, "(−) Deduções", Bloco::Deducoes);
    let liquida = bruta + deducoes;
    resultado(&mut dre, "Receita líquida", liquida);

    let cmv = bloco(&mut dre, "(−) CMV / custo do serviço", Bloco::Cmv);
    let lucro_bruto = liquida + cmv;
    resultado(&mut dre, "Lucro bruto", lucro_bruto);

    let outras = bloco(&mut dre, "(+) Outras receitas", Bloco::OutrasReceitas);
    let despesas = bloco(
        &mut dre,
        "(−) Despesas operacionais",
        Bloco::DespesasOperacionais,
    );
    let ebitda = lucro_bruto + outras + despesas;
    resultado(&mut dre, "EBITDA", ebitda);

    let rec_fin = bloco(
        &mut dre,
        "(+) Receitas financeiras",
        Bloco::ReceitaFinanceira,
    );
    let desp_fin = bloco(
        &mut dre,
        "(−) Despesas financeiras",
        Bloco::DespesaFinanceira,
    );
    let lucro = ebitda + rec_fin + desp_fin;
    resultado(&mut dre, "Lucro líquido", lucro);

    dre.receita_bruta = bruta;
    dre.receita_liquida = liquida;
    dre.lucro_bruto = lucro_bruto;
    dre.ebitda = ebitda;
    dre.lucro_liquido = lucro;
    dre
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Data, Id};
    use proptest::prelude::*;

    use super::*;

    fn periodo() -> Periodo {
        Periodo::novo(
            Data::de_ymd(2026, 10, 1).unwrap(),
            Data::de_ymd(2026, 10, 31).unwrap(),
        )
    }

    fn linha(codigo: &str, nome: &str, nat: Natureza, papel: PapelConta, v: i64) -> LinhaResultado {
        LinhaResultado {
            conta: Id::novo(),
            codigo: codigo.to_owned(),
            nome: nome.to_owned(),
            natureza: nat,
            papel: Some(papel.to_string()),
            valor: Dinheiro::centavos(v),
        }
    }

    fn exemplo() -> Vec<LinhaResultado> {
        use Natureza::{Despesa, Receita};
        use PapelConta as P;
        vec![
            linha("4.1", "Vendas", Receita, P::ReceitaVendas, 10_000_000),
            linha("4.2", "Serviços", Receita, P::ReceitaServicos, 2_000_000),
            // Redutoras de receita: natureza Receita, saldo devedor ⇒ valor negativo.
            linha(
                "4.4",
                "Descontos",
                Receita,
                P::DescontosConcedidos,
                -200_000,
            ),
            linha("4.5", "Devoluções", Receita, P::DevolucoesVenda, -300_000),
            linha("5.8", "Impostos", Despesa, P::ImpostosSobreVenda, 700_000),
            linha("5.1", "CMV", Despesa, P::Cmv, 5_000_000),
            linha("5.3", "Pessoal", Despesa, P::DespesaPessoal, 1_500_000),
            linha("5.6", "Ocupação", Despesa, P::Ocupacao, 500_000),
            linha("4.6", "Rec. fin.", Receita, P::ReceitaFinanceira, 30_000),
            linha("5.9", "Desp. fin.", Despesa, P::DespesaFinanceira, 80_000),
        ]
    }

    #[test]
    fn estrutura_e_subtotais_batem_com_a_conta_de_cabeca() {
        let d = montar(periodo(), &exemplo());
        assert_eq!(d.receita_bruta, Dinheiro::reais(120_000));
        // 120.000 − (2.000 + 3.000 + 7.000) = 108.000
        assert_eq!(d.receita_liquida, Dinheiro::reais(108_000));
        assert_eq!(d.lucro_bruto, Dinheiro::reais(58_000));
        // − 15.000 − 5.000
        assert_eq!(d.ebitda, Dinheiro::reais(38_000));
        // + 300 − 800
        assert_eq!(d.lucro_liquido, Dinheiro::reais(37_500));
        assert_eq!(d.valor_de("(−) Deduções"), Some(Dinheiro::reais(-12_000)));
        assert_eq!(d.margem_bruta().unwrap().formatar(1), "53,7");
    }

    #[test]
    fn periodo_vazio_nao_inventa_margem() {
        let d = montar(periodo(), &[]);
        assert_eq!(d.lucro_liquido, Dinheiro::ZERO);
        assert_eq!(d.margem_liquida(), None);
    }

    #[test]
    fn conta_sem_papel_conhecido_nao_some_do_resultado() {
        let mut ls = exemplo();
        ls.push(LinhaResultado {
            conta: Id::novo(),
            codigo: "5.99".into(),
            nome: "Misteriosa".into(),
            natureza: Natureza::Despesa,
            papel: None,
            valor: Dinheiro::reais(1_000),
        });
        let d = montar(periodo(), &ls);
        assert_eq!(d.lucro_liquido, Dinheiro::reais(36_500));
    }

    proptest! {
        #[test]
        fn lucro_liquido_e_sempre_receitas_menos_despesas(
            valores in proptest::collection::vec((0usize..12, -5_000_000i64..5_000_000), 0..40)
        ) {
            use Natureza::{Despesa, Receita};
            use PapelConta as P;
            let tabela = [
                (Receita, P::ReceitaVendas), (Receita, P::ReceitaServicos),
                (Receita, P::DescontosConcedidos), (Receita, P::DevolucoesVenda),
                (Receita, P::ReceitaFinanceira), (Receita, P::OutrasReceitas),
                (Despesa, P::Cmv), (Despesa, P::DespesaPessoal), (Despesa, P::Ocupacao),
                (Despesa, P::ImpostosSobreVenda), (Despesa, P::DespesaFinanceira),
                (Despesa, P::Perdas),
            ];
            let ls: Vec<LinhaResultado> = valores.iter().enumerate().map(|(i, (k, v))| {
                let (nat, papel) = tabela[*k];
                linha(&format!("9.{i}"), "x", nat, papel, *v)
            }).collect();
            let esperado: i64 = ls.iter().map(|l| match l.natureza {
                Natureza::Receita => l.valor.em_centavos(),
                _ => -l.valor.em_centavos(),
            }).sum();
            prop_assert_eq!(montar(periodo(), &ls).lucro_liquido.em_centavos(), esperado);
        }
    }
}
