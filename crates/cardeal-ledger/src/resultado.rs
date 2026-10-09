//! Leitura do resultado por conta num período — o insumo da DRE gerencial e da apuração de
//! receita bruta (RBT12) que o motor de preço usa. Só lê: soma as partidas das contas de
//! `Receita` e `Despesa` por competência.
//!
//! Entram os lançamentos `Confirmado`, `Realizado` e `Estornado` (o estorno anula o
//! original, então a soma dos dois dá zero); `Previsto` fica de fora — é intenção, não fato.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Periodo;
use cardeal_kernel::{Dinheiro, Id};
#[cfg(feature = "sqlite")]
use rusqlite::Connection;

use crate::conta::Natureza;
#[cfg(feature = "sqlite")]
use crate::erros::ErroRazao;

/// Uma conta de resultado com o valor do período, no sentido natural da conta: receita é
/// crédito menos débito, despesa é débito menos crédito (positivo = comportamento normal).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaResultado {
    /// A conta.
    pub conta: Id,
    /// O código hierárquico (`"5.4"`).
    pub codigo: String,
    /// O nome.
    pub nome: String,
    /// `Receita` ou `Despesa`.
    pub natureza: Natureza,
    /// O papel semântico gravado na conta (o nome do [`PapelConta`](crate::PapelConta)),
    /// ou o do ancestral mais próximo que tenha um — contas abertas pelo usuário herdam o
    /// papel da sintética sob a qual nasceram.
    pub papel: Option<String>,
    /// O valor do período.
    pub valor: Dinheiro,
}

/// id, código, nome, natureza e papel gravado.
#[cfg(feature = "sqlite")]
type ContaDeResultado = (Vec<u8>, String, String, String, Option<String>);

#[cfg(feature = "sqlite")]
#[allow(clippy::needless_pass_by_value)]
fn persist(e: rusqlite::Error) -> ErroRazao {
    ErroRazao::FalhaDePersistencia(e.to_string())
}

#[cfg(feature = "sqlite")]
fn natureza_de(s: &str) -> Natureza {
    if s == "Receita" {
        Natureza::Receita
    } else {
        Natureza::Despesa
    }
}

#[cfg(feature = "sqlite")]
/// Soma o resultado de cada conta de receita/despesa com movimento na competência do
/// período. Contas sem movimento não aparecem. Ordenado por código.
///
/// # Errors
/// [`ErroRazao::FalhaDePersistencia`] em erro do SQLite.
pub fn resultado_por_conta(
    conexao: &Connection,
    empresa: Id,
    periodo: Periodo,
) -> Result<Vec<LinhaResultado>, ErroRazao> {
    let empresa_b = empresa.em_bytes().to_vec();

    // Papel herdado: todas as contas de resultado, para resolver por prefixo de código.
    let mut stmt = conexao
        .prepare(
            "SELECT id, codigo, nome, natureza, papel_padrao FROM razao_conta
             WHERE empresa = ?1 AND natureza IN ('Receita','Despesa')",
        )
        .map_err(persist)?;
    let contas: Vec<ContaDeResultado> = stmt
        .query_map([&empresa_b], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .map_err(persist)?
        .collect::<rusqlite::Result<_>>()
        .map_err(persist)?;
    let papel_do_codigo = |codigo: &str| -> Option<String> {
        let mut atual = codigo;
        loop {
            if let Some((_, _, _, _, Some(p))) = contas.iter().find(|c| c.1 == atual) {
                return Some(p.clone());
            }
            atual = &atual[..atual.rfind('.')?];
        }
    };

    let mut stmt = conexao
        .prepare(
            "SELECT rp.conta, SUM(rp.valor)
             FROM razao_partida rp
             JOIN razao_lancamento rl ON rl.id = rp.lancamento
             WHERE rl.empresa = ?1 AND rl.estado IN ('Confirmado','Realizado','Estornado')
               AND rl.competencia BETWEEN ?2 AND ?3
             GROUP BY rp.conta",
        )
        .map_err(persist)?;
    let somas: Vec<(Vec<u8>, i64)> = stmt
        .query_map(
            rusqlite::params![
                &empresa_b,
                i64::from(periodo.de.em_dias()),
                i64::from(periodo.ate.em_dias())
            ],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(persist)?
        .collect::<rusqlite::Result<_>>()
        .map_err(persist)?;

    let mut saida = Vec::new();
    for (conta_b, soma) in somas {
        let Some((_, codigo, nome, nat, _)) = contas.iter().find(|c| c.0 == conta_b) else {
            continue; // conta de balanço: não é resultado
        };
        let natureza = natureza_de(nat);
        // Partidas: débito positivo, crédito negativo.
        let valor = if natureza == Natureza::Receita {
            -soma
        } else {
            soma
        };
        saida.push(LinhaResultado {
            conta: <[u8; 16]>::try_from(conta_b).map_or(Id::NULO, Id::de_bytes),
            codigo: codigo.clone(),
            nome: nome.clone(),
            natureza,
            papel: papel_do_codigo(codigo),
            valor: Dinheiro::centavos(valor),
        });
    }
    saida.sort_by(|a, b| a.codigo.cmp(&b.codigo));
    Ok(saida)
}

/// O resultado resumido para a formação de preço: quanto a empresa faturou e quanto gastou
/// para operar. O custo da mercadoria, os impostos sobre a venda, as taxas de cartão e as
/// perdas ficam de fora das despesas — cada um tem o seu próprio termo no markup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResumoGerencial {
    /// Vendas, serviços, aluguéis, hospedagem e receitas avulsas (não financeiras), brutas.
    pub receita_bruta: Dinheiro,
    /// Pessoal, administrativas, comerciais, ocupação e financeiras (e despesa sem papel).
    pub despesas_operacionais: Dinheiro,
}

impl ResumoGerencial {
    /// Despesas operacionais sobre a receita bruta; `None` sem faturamento.
    #[must_use]
    pub fn custo_operacional_percentual(&self) -> Option<cardeal_kernel::Percentual> {
        (self.receita_bruta.e_positivo()).then(|| {
            cardeal_kernel::Percentual::da_razao(
                self.despesas_operacionais.em_centavos(),
                self.receita_bruta.em_centavos(),
            )
        })
    }
}

/// Resume as linhas de resultado para o preço (ver [`ResumoGerencial`]).
#[must_use]
pub fn resumir(linhas: &[LinhaResultado]) -> ResumoGerencial {
    use crate::conta::PapelConta as P;
    let n = |p: P| p.to_string();
    let receitas = [
        n(P::ReceitaVendas),
        n(P::ReceitaServicos),
        n(P::ReceitaAlugueis),
        n(P::ReceitaHospedagem),
        n(P::OutrasReceitas),
    ];
    let despesas = [
        n(P::DespesaPessoal),
        n(P::DespesaAdministrativa),
        n(P::DespesaComercial),
        n(P::Ocupacao),
        n(P::DespesaFinanceira),
    ];
    let fora = [
        n(P::Cmv),
        n(P::CustoServico),
        n(P::ImpostosSobreVenda),
        n(P::TaxasCartao),
        n(P::Perdas),
        n(P::QuebraCaixa),
    ];
    let mut r = ResumoGerencial {
        receita_bruta: Dinheiro::ZERO,
        despesas_operacionais: Dinheiro::ZERO,
    };
    for l in linhas {
        let papel = l.papel.as_deref().unwrap_or("");
        match l.natureza {
            Natureza::Receita if receitas.iter().any(|p| p == papel) => r.receita_bruta += l.valor,
            Natureza::Despesa if despesas.iter().any(|p| p == papel) => {
                r.despesas_operacionais += l.valor;
            }
            Natureza::Despesa if !fora.iter().any(|p| p == papel) => {
                r.despesas_operacionais += l.valor;
            }
            _ => {}
        }
    }
    r
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::conta::PapelConta;

    fn l(nat: Natureza, papel: Option<PapelConta>, reais: i64) -> LinhaResultado {
        LinhaResultado {
            conta: Id::novo(),
            codigo: "x".into(),
            nome: "x".into(),
            natureza: nat,
            papel: papel.map(|p| p.to_string()),
            valor: Dinheiro::reais(reais),
        }
    }

    #[test]
    fn resumo_separa_operacao_de_custo_imposto_e_taxa() {
        let r = resumir(&[
            l(Natureza::Receita, Some(PapelConta::ReceitaVendas), 1_000),
            l(Natureza::Receita, Some(PapelConta::ReceitaFinanceira), 50),
            l(Natureza::Despesa, Some(PapelConta::Cmv), 600),
            l(Natureza::Despesa, Some(PapelConta::ImpostosSobreVenda), 80),
            l(Natureza::Despesa, Some(PapelConta::TaxasCartao), 20),
            l(Natureza::Despesa, Some(PapelConta::DespesaPessoal), 150),
            l(Natureza::Despesa, Some(PapelConta::Ocupacao), 50),
            l(Natureza::Despesa, None, 10),
        ]);
        assert_eq!(r.receita_bruta, Dinheiro::reais(1_000));
        assert_eq!(r.despesas_operacionais, Dinheiro::reais(210));
        assert_eq!(r.custo_operacional_percentual().unwrap().formatar(0), "21");
        assert_eq!(
            ResumoGerencial {
                receita_bruta: Dinheiro::ZERO,
                despesas_operacionais: Dinheiro::reais(5)
            }
            .custo_operacional_percentual(),
            None
        );
    }
}
