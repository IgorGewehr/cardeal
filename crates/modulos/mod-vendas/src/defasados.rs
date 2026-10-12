//! Preços defasados: compara o preço que a tabela pratica com o que o estoque calcula hoje
//! (custo + operação + imposto + comissão + taxas + margem desejada) e diz quem ficou para trás
//! — o custo subiu, a margem caiu, e ninguém reajustou a tabela. Leitura apenas.

use cardeal_kernel::{Dinheiro, Id, Percentual};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;

/// Quantos pontos abaixo da margem desejada ainda se aceita como "no alvo".
pub const TOLERANCIA_ABAIXO: Percentual = Percentual::pontos(1);
/// Quantos pontos acima da margem desejada ainda se aceita como "no alvo".
pub const TOLERANCIA_ACIMA: Percentual = Percentual::pontos(5);

/// Como o preço praticado se compara ao calculado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SituacaoDoPreco {
    /// Abaixo do preço de empate: cada venda perde dinheiro.
    Prejuizo,
    /// Margem atual mais de 1 ponto abaixo da desejada.
    AbaixoDoAlvo,
    /// Margem dentro da faixa (desejada − 1 ponto a desejada + 5 pontos).
    NoAlvo,
    /// Margem atual mais de 5 pontos acima da desejada (pode estar perdendo venda).
    AcimaDoAlvo,
    /// Não dá para comparar (sem custo ou sem margem); o motivo em português.
    SemCalculo(String),
}

impl SituacaoDoPreco {
    /// `true` para as duas situações que pedem reajuste (`Prejuizo` e `AbaixoDoAlvo`).
    #[must_use]
    pub const fn e_defasado(&self) -> bool {
        matches!(self, Self::Prejuizo | Self::AbaixoDoAlvo)
    }

    /// A urgência para ordenar listas: prejuízo primeiro, no alvo por último.
    #[must_use]
    pub const fn ordem(&self) -> u8 {
        match self {
            Self::Prejuizo => 0,
            Self::AbaixoDoAlvo => 1,
            Self::SemCalculo(_) => 2,
            Self::AcimaDoAlvo => 3,
            Self::NoAlvo => 4,
        }
    }
}

/// Classifica um preço praticado. `minimo` é o preço de empate; as margens são líquidas sobre o
/// preço. Prejuízo = preço abaixo do empate ou margem atual negativa.
#[must_use]
pub fn classificar(
    preco_atual: Dinheiro,
    minimo: Option<Dinheiro>,
    margem_atual: Option<Percentual>,
    margem_desejada: Option<Percentual>,
    problema: Option<&str>,
) -> SituacaoDoPreco {
    let (Some(minimo), Some(atual), Some(desejada)) = (minimo, margem_atual, margem_desejada)
    else {
        return SituacaoDoPreco::SemCalculo(
            problema
                .unwrap_or("sem custo ou sem margem para calcular")
                .to_owned(),
        );
    };
    if preco_atual < minimo || atual.unidades_internas() < 0 {
        SituacaoDoPreco::Prejuizo
    } else if atual < desejada - TOLERANCIA_ABAIXO {
        SituacaoDoPreco::AbaixoDoAlvo
    } else if atual > desejada + TOLERANCIA_ACIMA {
        SituacaoDoPreco::AcimaDoAlvo
    } else {
        SituacaoDoPreco::NoAlvo
    }
}

/// Um produto da tabela com o preço praticado e o que o estoque calcula.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecoDefasado {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// O preço da regra permanente da tabela.
    pub preco_atual: Dinheiro,
    /// O custo de referência de hoje.
    pub custo: Option<Dinheiro>,
    /// A margem líquida que o preço atual deixa.
    pub margem_atual: Option<Percentual>,
    /// A margem desejada (produto, grupo ou padrão).
    pub margem_desejada: Option<Percentual>,
    /// O preço sugerido hoje (sem arredondamento de prateleira).
    pub preco_sugerido: Option<Dinheiro>,
    /// O preço de empate hoje.
    pub preco_minimo: Option<Dinheiro>,
    /// A situação.
    pub situacao: SituacaoDoPreco,
}

/// Os produtos com preço permanente numa tabela, comparados com o preço calculado hoje:
/// prejuízo e abaixo do alvo primeiro.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PrecosDefasados {
    /// A tabela de preço.
    pub tabela_preco: Id,
}

impl Consulta for PrecosDefasados {
    type Saida = Vec<PrecoDefasado>;
    const PERMISSAO: &'static str = "vendas.tabela_preco.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        use cardeal_kernel::{Arredondamento, Preco, Quantidade};
        use mod_estoque::precificacao::margem_do_preco;

        use crate::repositorio::{blob, id_de, persist};

        // Só a regra permanente do produto (sem faixa nem período); se houver mais de uma, vale
        // a mais recente, como em `regra_permanente_do_produto`.
        let mut stmt = conexao
            .prepare(
                "SELECT produto, preco FROM vendas_regra_preco
                 WHERE tabela_preco = ?1 AND empresa = ?2 AND produto IS NOT NULL
                   AND quantidade_minima IS NULL AND periodo_de IS NULL AND periodo_ate IS NULL
                 ORDER BY id ASC",
            )
            .map_err(persist)?;
        let mut regras: Vec<(Id, Preco)> = Vec::new();
        let linhas = stmt
            .query_map(
                rusqlite::params![blob(self.tabela_preco), blob(ctx.empresa)],
                |r| {
                    Ok((
                        id_de(r.get::<_, Vec<u8>>(0)?),
                        Preco::interna(r.get::<_, i64>(1)?),
                    ))
                },
            )
            .map_err(persist)?;
        for l in linhas {
            let (produto, preco) = l.map_err(persist)?;
            match regras.iter_mut().find(|(p, _)| *p == produto) {
                Some(existente) => existente.1 = preco,
                None => regras.push((produto, preco)),
            }
        }
        let produtos: Vec<Id> = regras.iter().map(|(p, _)| *p).collect();
        let sugestoes = mod_estoque::sugerir_precos(conexao, ctx, None, Some(&produtos))?;

        let mut saida: Vec<PrecoDefasado> = regras
            .iter()
            .filter_map(|(produto, preco)| {
                let s = sugestoes.iter().find(|s| s.produto == *produto)?;
                let preco_atual =
                    Dinheiro::de_total(Quantidade::unidades(1), *preco, Arredondamento::MeioAcima);
                let margem_atual = match (s.custo, &s.componentes) {
                    (Some(custo), Some(c)) => margem_do_preco(preco_atual, custo, c),
                    _ => None,
                };
                let minimo = s.minimo.map(|m| m.preco);
                Some(PrecoDefasado {
                    produto: *produto,
                    nome: s.nome.clone(),
                    preco_atual,
                    custo: s.custo,
                    margem_atual,
                    margem_desejada: s.margem,
                    preco_sugerido: s.sugerido.map(|m| m.preco),
                    preco_minimo: minimo,
                    situacao: classificar(
                        preco_atual,
                        minimo,
                        margem_atual,
                        s.margem,
                        s.problema.as_deref(),
                    ),
                })
            })
            .collect();
        saida.sort_by(|a, b| (a.situacao.ordem(), &a.nome).cmp(&(b.situacao.ordem(), &b.nome)));
        Ok(saida)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const MINIMO: Dinheiro = Dinheiro::centavos(5_000);

    fn c(preco: i64, margem: i64) -> SituacaoDoPreco {
        classificar(
            Dinheiro::centavos(preco),
            Some(MINIMO),
            Some(Percentual::pontos(margem)),
            Some(Percentual::pontos(20)),
            None,
        )
    }

    #[test]
    fn abaixo_do_empate_ou_margem_negativa_e_prejuizo() {
        assert_eq!(c(4_999, 5), SituacaoDoPreco::Prejuizo);
        assert_eq!(c(5_000, -1), SituacaoDoPreco::Prejuizo);
        // no empate exato (margem 0) não é prejuízo, é abaixo do alvo
        assert_eq!(c(5_000, 0), SituacaoDoPreco::AbaixoDoAlvo);
    }

    #[test]
    fn limites_da_faixa_do_alvo_sao_inclusivos() {
        assert_eq!(c(6_000, 18), SituacaoDoPreco::AbaixoDoAlvo); // 20 − 2
        assert_eq!(c(6_000, 19), SituacaoDoPreco::NoAlvo); // 20 − 1 (limite)
        assert_eq!(c(6_000, 20), SituacaoDoPreco::NoAlvo);
        assert_eq!(c(6_000, 25), SituacaoDoPreco::NoAlvo); // 20 + 5 (limite)
        assert_eq!(c(6_000, 26), SituacaoDoPreco::AcimaDoAlvo);
    }

    #[test]
    fn sem_calculo_copia_o_problema_ou_um_texto_padrao() {
        let s = classificar(Dinheiro::reais(10), None, None, None, Some("sem custo"));
        assert_eq!(s, SituacaoDoPreco::SemCalculo("sem custo".into()));
        let s = classificar(Dinheiro::reais(10), None, None, None, None);
        assert!(matches!(s, SituacaoDoPreco::SemCalculo(m) if !m.is_empty()));
        assert!(!s_defasado(&SituacaoDoPreco::NoAlvo));
        assert!(
            s_defasado(&SituacaoDoPreco::Prejuizo) && s_defasado(&SituacaoDoPreco::AbaixoDoAlvo)
        );
    }

    fn s_defasado(s: &SituacaoDoPreco) -> bool {
        s.e_defasado()
    }
}
