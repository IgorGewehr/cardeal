//! Os parâmetros de preço de uma empresa (CNPJ): regime, custo operacional, comissão, taxa
//! de pagamento e margens. Um conjunto por empresa; sem linha gravada vale o padrão
//! derivado do regime cadastrado em `nucleo_empresa`.

use cardeal_kernel::Percentual;
use serde::{Deserialize, Serialize};

use crate::precificacao::{AnexoSimples, RegimeImposto};

/// Qual custo alimenta o preço sugerido.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CustoDeReferencia {
    /// O custo médio móvel em estoque (o maior entre os locais).
    #[default]
    CustoMedio,
    /// O custo da entrada mais recente do produto.
    UltimaCompra,
    /// O maior entre o custo médio e o da última compra (protege a margem quando o
    /// fornecedor reajusta).
    MaiorDosDois,
}

impl CustoDeReferencia {
    /// O texto gravado no banco.
    #[must_use]
    pub const fn texto(self) -> &'static str {
        match self {
            Self::CustoMedio => "CustoMedio",
            Self::UltimaCompra => "UltimaCompra",
            Self::MaiorDosDois => "MaiorDosDois",
        }
    }

    /// Lê o texto gravado; valor desconhecido vale o padrão.
    #[must_use]
    pub fn de_texto(t: &str) -> Self {
        match t {
            "UltimaCompra" => Self::UltimaCompra,
            "MaiorDosDois" => Self::MaiorDosDois,
            _ => Self::CustoMedio,
        }
    }

    /// Escolhe o custo do preço. Sem custo médio (zero) ou sem entrada registrada, usa o que
    /// existir; devolve também qual referência valeu de fato (`CustoMedio` ou `UltimaCompra`).
    #[must_use]
    pub fn escolher<T: Ord + Copy>(self, medio: Option<T>, ultima: Option<T>) -> (Option<T>, Self) {
        match (self, medio, ultima) {
            (Self::CustoMedio, Some(m), _) | (Self::MaiorDosDois, Some(m), None) => {
                (Some(m), Self::CustoMedio)
            }
            (Self::UltimaCompra, _, Some(u)) | (Self::MaiorDosDois, None, Some(u)) => {
                (Some(u), Self::UltimaCompra)
            }
            (Self::MaiorDosDois, Some(m), Some(u)) => {
                if u > m {
                    (Some(u), Self::UltimaCompra)
                } else {
                    (Some(m), Self::CustoMedio)
                }
            }
            // Sem a referência pedida: cai no que houver.
            (_, Some(m), None) => (Some(m), Self::CustoMedio),
            (_, None, Some(u)) => (Some(u), Self::UltimaCompra),
            (_, None, None) => (None, Self::CustoMedio),
        }
    }
}

/// Parâmetros de formação de preço da empresa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParametrosPreco {
    /// Como se calcula o imposto sobre a venda.
    pub regime: RegimeImposto,
    /// Custo operacional informado (usado quando não é derivado).
    pub custo_operacional: Percentual,
    /// `true`: o custo operacional vem do Razão (despesas operacionais ÷ faturamento dos
    /// últimos 12 meses) e `custo_operacional` fica só como reserva sem histórico.
    pub custo_operacional_derivado: bool,
    /// Comissão média de vendedor.
    pub comissao: Percentual,
    /// Taxa média de cartão/PIX/boleto.
    pub taxa_pagamento: Percentual,
    /// Margem líquida para o produto sem margem própria nem herdada; `None` = obrigar a
    /// informar a margem.
    pub margem_padrao: Option<Percentual>,
    /// Abaixo desta margem líquida o PDV e o orçamento alertam.
    pub margem_minima: Percentual,
    /// O preço sugerido é obrigatório (não só orientativo) no PDV.
    pub preco_obrigatorio: bool,
    /// Qual custo alimenta o preço (médio, última compra ou o maior dos dois).
    #[serde(default)]
    pub custo_referencia: CustoDeReferencia,
}

impl ParametrosPreco {
    /// O padrão para o regime cadastral da empresa (`MEI`, `SimplesNacional`,
    /// `LucroPresumido`, `LucroReal`): Simples cai no Anexo I (comércio), os de lucro
    /// presumido/real começam com 0% manual — o contador informa.
    #[must_use]
    pub fn padrao_para(regime_cadastral: &str) -> Self {
        let regime = match regime_cadastral {
            "MEI" => RegimeImposto::Mei,
            "SimplesNacional" => RegimeImposto::Simples(AnexoSimples::I),
            _ => RegimeImposto::Manual(Percentual::ZERO),
        };
        Self {
            regime,
            custo_operacional: Percentual::ZERO,
            custo_operacional_derivado: true,
            comissao: Percentual::ZERO,
            taxa_pagamento: Percentual::ZERO,
            margem_padrao: None,
            margem_minima: Percentual::ZERO,
            preco_obrigatorio: false,
            custo_referencia: CustoDeReferencia::CustoMedio,
        }
    }

    /// Valida os percentuais (0% a 100%).
    ///
    /// # Errors
    /// Uma mensagem para o usuário dizendo qual campo está fora da faixa.
    pub fn validar(&self) -> Result<(), String> {
        let campos = [
            ("custo operacional", Some(self.custo_operacional)),
            ("comissão", Some(self.comissao)),
            ("taxa de pagamento", Some(self.taxa_pagamento)),
            ("margem padrão", self.margem_padrao),
            ("margem mínima", Some(self.margem_minima)),
            (
                "imposto informado",
                match self.regime {
                    RegimeImposto::Manual(p) => Some(p),
                    _ => None,
                },
            ),
        ];
        for (nome, p) in campos {
            if let Some(p) = p {
                if !p.e_fracao_valida() {
                    return Err(format!("{nome}: informe um percentual entre 0% e 100%"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn padrao_acompanha_o_regime_cadastral() {
        assert_eq!(
            ParametrosPreco::padrao_para("MEI").regime,
            RegimeImposto::Mei
        );
        assert_eq!(
            ParametrosPreco::padrao_para("SimplesNacional").regime,
            RegimeImposto::Simples(AnexoSimples::I)
        );
        assert!(matches!(
            ParametrosPreco::padrao_para("LucroPresumido").regime,
            RegimeImposto::Manual(_)
        ));
    }

    #[test]
    fn escolha_do_custo_de_referencia() {
        use CustoDeReferencia::{CustoMedio as M, MaiorDosDois as G, UltimaCompra as U};
        assert_eq!(M.escolher(Some(50), Some(60)), (Some(50), M));
        assert_eq!(U.escolher(Some(50), Some(60)), (Some(60), U));
        assert_eq!(G.escolher(Some(50), Some(60)), (Some(60), U));
        assert_eq!(G.escolher(Some(70), Some(60)), (Some(70), M));
        assert_eq!(G.escolher(Some(60), Some(60)), (Some(60), M));
        // sem entrada: cai no médio, qualquer que seja a referência
        assert_eq!(U.escolher(Some(50), None), (Some(50), M));
        assert_eq!(G.escolher(Some(50), None), (Some(50), M));
        // sem custo médio mas com entrada (ex.: saldo zerado)
        assert_eq!(M.escolher(None, Some(60)), (Some(60), U));
        assert_eq!(G.escolher::<i64>(None, None), (None, M));
    }

    #[test]
    fn texto_do_custo_de_referencia_ida_e_volta() {
        for c in [
            CustoDeReferencia::CustoMedio,
            CustoDeReferencia::UltimaCompra,
            CustoDeReferencia::MaiorDosDois,
        ] {
            assert_eq!(CustoDeReferencia::de_texto(c.texto()), c);
        }
        assert_eq!(
            CustoDeReferencia::de_texto("x"),
            CustoDeReferencia::CustoMedio
        );
    }

    #[test]
    fn percentual_fora_de_faixa_e_recusado_com_o_nome_do_campo() {
        let mut p = ParametrosPreco::padrao_para("SimplesNacional");
        assert!(p.validar().is_ok());
        p.comissao = Percentual::pontos(101);
        assert!(p.validar().unwrap_err().contains("comissão"));
        p.comissao = Percentual::ZERO;
        p.margem_padrao = Some(Percentual::pontos(-1));
        assert!(p.validar().unwrap_err().contains("margem padrão"));
    }
}
