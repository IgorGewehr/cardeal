//! O que as consultas devolvem.

use cardeal_fiscal::AmbienteFiscal;
use cardeal_kernel::{Data, Fuso, Instante};
use serde::{Deserialize, Serialize};

/// A quantos dias do vencimento do certificado o aviso aparece (`docs/10` §4).
pub const ALERTA_VENCIMENTO_DIAS: i32 = 30;

/// O certificado instalado — só a parte pública; o arquivo e a senha ficam no cofre.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CertificadoResumo {
    /// O titular (razão social do e-CNPJ).
    pub titular: String,
    /// O CNPJ do titular, formatado.
    pub cnpj: String,
    /// A autoridade certificadora.
    pub emissor: String,
    /// O número de série (hexadecimal).
    pub numero_serie: String,
    /// Início da validade.
    pub valido_de: Instante,
    /// Fim da validade.
    pub valido_ate: Instante,
    /// Se o arquivo usava RC2 e foi regravado na instalação.
    pub convertido: bool,
    /// Quando foi instalado.
    pub instalado_em: Instante,
}

impl CertificadoResumo {
    /// Dias até vencer (negativo: venceu há tantos dias).
    #[must_use]
    pub fn dias_para_vencer(&self, hoje: Data, fuso: Fuso) -> i32 {
        hoje.dias_ate(self.valido_ate.data(fuso))
    }
}

/// O que falta para a empresa poder emitir.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PendenciaFiscal {
    /// IE, endereço ou código IBGE do município faltando.
    DadosFiscaisIncompletos,
    /// Nenhum certificado instalado.
    SemCertificado,
    /// A emissão de NFC-e está ligada, mas sem o CSC.
    SemCsc,
    /// O certificado venceu.
    CertificadoVencido {
        /// Quando venceu.
        em: Data,
    },
    /// O certificado vence em até [`ALERTA_VENCIMENTO_DIAS`] dias.
    CertificadoVencendo {
        /// Quantos dias faltam.
        dias: i32,
    },
}

impl PendenciaFiscal {
    /// A frase para a tela.
    #[must_use]
    pub fn mensagem(&self) -> String {
        match self {
            Self::DadosFiscaisIncompletos => {
                "Complete os dados fiscais da empresa: inscrição estadual e endereço com o código IBGE do município."
                    .into()
            }
            Self::SemCertificado => "Instale o certificado digital A1 da empresa.".into(),
            Self::SemCsc => "Cadastre o CSC da NFC-e (identificador e token da SEFAZ).".into(),
            Self::CertificadoVencido { em } => format!(
                "O certificado digital venceu em {} — sem ele nenhuma nota é emitida. Instale o novo.",
                em.formatar()
            ),
            Self::CertificadoVencendo { dias: 0 } => {
                "O certificado digital vence hoje — renove e instale o novo.".into()
            }
            Self::CertificadoVencendo { dias: 1 } => {
                "O certificado digital vence amanhã — renove e instale o novo.".into()
            }
            Self::CertificadoVencendo { dias } => {
                format!("O certificado digital vence em {dias} dias — renove com a certificadora.")
            }
        }
    }

    /// Verdadeiro se impede a emissão (o vencendo ainda emite).
    #[must_use]
    pub const fn impede_emissao(&self) -> bool {
        !matches!(self, Self::CertificadoVencendo { .. })
    }
}

/// A situação fiscal da empresa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SituacaoFiscalDaEmpresa {
    /// O certificado instalado.
    pub certificado: Option<CertificadoResumo>,
    /// Homologação ou produção.
    pub ambiente: AmbienteFiscal,
    /// O que falta, do mais grave ao mais leve.
    pub pendencias: Vec<PendenciaFiscal>,
}

impl SituacaoFiscalDaEmpresa {
    /// Verdadeiro se nada impede a emissão.
    #[must_use]
    pub fn pronta_para_emitir(&self) -> bool {
        !self.pendencias.iter().any(PendenciaFiscal::impede_emissao)
    }
}

/// Monta as pendências a partir do certificado, dos dados fiscais e de hoje.
pub(crate) fn pendencias(
    certificado: Option<&CertificadoResumo>,
    dados_completos: bool,
    falta_csc: bool,
    hoje: Data,
    fuso: Fuso,
) -> Vec<PendenciaFiscal> {
    let mut lista = Vec::new();
    if falta_csc {
        lista.push(PendenciaFiscal::SemCsc);
    }
    match certificado {
        None => lista.push(PendenciaFiscal::SemCertificado),
        Some(c) => {
            let dias = c.dias_para_vencer(hoje, fuso);
            if dias < 0 {
                lista.push(PendenciaFiscal::CertificadoVencido {
                    em: c.valido_ate.data(fuso),
                });
            } else if dias <= ALERTA_VENCIMENTO_DIAS {
                lista.push(PendenciaFiscal::CertificadoVencendo { dias });
            }
        }
    }
    if !dados_completos {
        lista.push(PendenciaFiscal::DadosFiscaisIncompletos);
    }
    lista.sort_by_key(|p| !p.impede_emissao());
    lista
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Hora;

    use super::*;

    fn certificado(vence: Data) -> CertificadoResumo {
        CertificadoResumo {
            titular: "EMPRESA".into(),
            cnpj: "11.222.333/0001-81".into(),
            emissor: "AC".into(),
            numero_serie: "01".into(),
            valido_de: Instante::EPOCA,
            valido_ate: Instante::de_data_hora(
                vence,
                Hora::de_hms(12, 0, 0).unwrap(),
                Fuso::BRASILIA,
            ),
            convertido: false,
            instalado_em: Instante::EPOCA,
        }
    }

    #[test]
    fn pendencias_por_situacao() {
        let hoje = Data::de_ymd(2026, 10, 14).unwrap();
        let f = Fuso::BRASILIA;
        assert_eq!(
            pendencias(None, false, false, hoje, f),
            vec![
                PendenciaFiscal::SemCertificado,
                PendenciaFiscal::DadosFiscaisIncompletos
            ]
        );
        assert!(
            pendencias(Some(&certificado(hoje.mais_dias(31))), true, false, hoje, f).is_empty()
        );
        assert_eq!(
            pendencias(Some(&certificado(hoje.mais_dias(31))), true, true, hoje, f),
            vec![PendenciaFiscal::SemCsc]
        );
        let vencendo = pendencias(
            Some(&certificado(hoje.mais_dias(30))),
            false,
            false,
            hoje,
            f,
        );
        // O que impede a emissão vem primeiro.
        assert_eq!(
            vencendo,
            vec![
                PendenciaFiscal::DadosFiscaisIncompletos,
                PendenciaFiscal::CertificadoVencendo { dias: 30 }
            ]
        );
        assert_eq!(
            pendencias(Some(&certificado(hoje.mais_dias(-1))), true, false, hoje, f),
            vec![PendenciaFiscal::CertificadoVencido {
                em: hoje.mais_dias(-1)
            }]
        );
        assert!(PendenciaFiscal::CertificadoVencendo { dias: 1 }
            .mensagem()
            .contains("amanhã"));
        let situacao = SituacaoFiscalDaEmpresa {
            certificado: None,
            ambiente: AmbienteFiscal::Homologacao,
            pendencias: vec![PendenciaFiscal::CertificadoVencendo { dias: 5 }],
        };
        assert!(situacao.pronta_para_emitir());
    }
}
