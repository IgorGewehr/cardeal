//! # cardeal-tributos
//!
//! A carga tributária de um produto, devolvida como **termos lineares sobre o preço**: cada
//! imposto vira um coeficiente (ICMS por dentro, PIS/COFINS sobre a receita sem o ICMS, IPI
//! por fora, IR sobre o lucro...). É a conta única que o preço sugerido (`mod-estoque`) e a
//! nota emitida (`mod-fiscal`) usam — ver `docs/superpowers/specs/2026-11-23-markup-por-carga-
//! tributaria-design.md` e ADR-0022.
//!
//! Função pura, sem I/O, aritmética exata. Dado faltando vira uma frase em português dizendo o
//! que preencher, nunca um número inventado. As tabelas legais moram aqui e devem ser
//! **conferidas com o contador**.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]

pub mod credito;
pub mod destino;
pub mod ncm;
pub mod preco;
pub mod reforma;
pub mod simples;

use cardeal_kernel::{Dinheiro, Percentual};
use serde::{Deserialize, Serialize};

pub use credito::{credito_do_item, CompraDoItem, CreditoDoItem};
pub use preco::{resolver_preco, Despesas, ParcelaDoPreco, PrecoResolvido};
pub use simples::{aliquota_efetiva, faixa_do_simples, AnexoSimples, SUBLIMITE};

/// A atividade da empresa no Lucro Presumido (decide a presunção do IRPJ e da CSLL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Atividade {
    /// Comércio: presunção de 8% (IRPJ) e 12% (CSLL).
    #[default]
    Comercio,
    /// Indústria: as mesmas presunções do comércio.
    Industria,
    /// Serviço em geral: 32% e 32%.
    Servico,
}

/// O regime de tributação da empresa, com o que a conta precisa saber dele.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Regime {
    /// MEI: o DAS é fixo (custo operacional), nada incide sobre o preço.
    Mei,
    /// Simples Nacional: anexo e receita bruta dos 12 meses anteriores (RBT12).
    Simples {
        /// O anexo.
        anexo: AnexoSimples,
        /// A receita bruta dos 12 meses anteriores.
        rbt12: Dinheiro,
    },
    /// Lucro Presumido.
    Presumido {
        /// A atividade (presunção do IRPJ/CSLL).
        atividade: Atividade,
        /// A receita média mensal (liga o adicional do IRPJ).
        receita_mensal: Dinheiro,
    },
    /// Lucro Real.
    Real {
        /// PIS/COFINS não cumulativos (1,65% + 7,6%) — o normal no Real.
        pis_cofins_nao_cumulativo: bool,
    },
    /// Um percentual único informado à mão (o comportamento anterior ao motor).
    Manual(Percentual),
}

/// Como o produto é tributado: o perfil fiscal efetivo (próprio por cima do herdado).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerfilTributario {
    /// CST do ICMS (2 dígitos, regime normal) ou CSOSN (3 dígitos, Simples).
    pub situacao_icms: Option<String>,
    /// Alíquota interna do ICMS.
    pub aliquota_icms: Option<Percentual>,
    /// Redução da base de cálculo do ICMS.
    pub reducao_base: Option<Percentual>,
    /// Fundo de combate à pobreza (somado ao ICMS, mesma base).
    pub aliquota_fcp: Option<Percentual>,
    /// MVA da substituição tributária (quando a empresa é a substituta).
    pub mva_st: Option<Percentual>,
    /// CST do PIS.
    pub cst_pis: Option<String>,
    /// CST da COFINS.
    pub cst_cofins: Option<String>,
    /// Alíquota do PIS (sem ela, a do regime).
    pub aliquota_pis: Option<Percentual>,
    /// Alíquota da COFINS (sem ela, a do regime).
    pub aliquota_cofins: Option<Percentual>,
    /// Alíquota do IPI na venda (indústria).
    pub aliquota_ipi: Option<Percentual>,
    /// NCM (8 dígitos).
    pub ncm: Option<String>,
}

/// Como um termo incide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Natureza {
    /// Dentro do preço: sai de cada real vendido.
    PorDentro,
    /// Somado ao preço, pago pelo cliente (IPI, ST do substituto).
    PorFora,
    /// Sobre o lucro (IRPJ/CSLL no Real): a margem antes dele tem de ser maior.
    SobreOLucro,
}

/// Qual imposto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Imposto {
    /// DAS do Simples.
    Das,
    /// ICMS próprio.
    Icms,
    /// Fundo de combate à pobreza.
    Fcp,
    /// PIS.
    Pis,
    /// COFINS.
    Cofins,
    /// IRPJ (com o adicional).
    Irpj,
    /// CSLL.
    Csll,
    /// IPI.
    Ipi,
    /// ICMS-ST cobrado pelo substituto.
    IcmsSt,
    /// Diferencial de alíquota ao consumidor de outro estado.
    Difal,
    /// CBS (reforma).
    Cbs,
    /// IBS (reforma).
    Ibs,
    /// Imposto Seletivo (reforma).
    Seletivo,
    /// Percentual informado à mão.
    Manual,
}

impl Imposto {
    /// O nome curto para a tela.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Das => "DAS",
            Self::Icms => "ICMS",
            Self::Fcp => "FCP",
            Self::Pis => "PIS",
            Self::Cofins => "COFINS",
            Self::Irpj => "IRPJ",
            Self::Csll => "CSLL",
            Self::Ipi => "IPI",
            Self::IcmsSt => "ICMS-ST",
            Self::Difal => "DIFAL",
            Self::Cbs => "CBS",
            Self::Ibs => "IBS",
            Self::Seletivo => "Imposto Seletivo",
            Self::Manual => "Impostos",
        }
    }
}

/// Um imposto como coeficiente sobre o preço, com a frase que explica de onde veio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Termo {
    /// Qual imposto.
    pub imposto: Imposto,
    /// Como incide.
    pub natureza: Natureza,
    /// O coeficiente (sobre o preço; sobre o lucro no `SobreOLucro`).
    pub coeficiente: Percentual,
    /// De onde veio, em português.
    pub explicacao: String,
}

/// A carga de um produto.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Carga {
    /// Os termos, na ordem em que aparecem para o usuário.
    pub termos: Vec<Termo>,
}

impl Carga {
    fn soma(&self, natureza: Natureza) -> Percentual {
        self.termos
            .iter()
            .filter(|t| t.natureza == natureza)
            .fold(Percentual::ZERO, |a, t| a + t.coeficiente)
    }

    /// O que sai de dentro do preço.
    #[must_use]
    pub fn por_dentro(&self) -> Percentual {
        self.soma(Natureza::PorDentro)
    }

    /// O que se soma ao preço.
    #[must_use]
    pub fn por_fora(&self) -> Percentual {
        self.soma(Natureza::PorFora)
    }

    /// O que incide sobre o lucro.
    #[must_use]
    pub fn sobre_o_lucro(&self) -> Percentual {
        self.soma(Natureza::SobreOLucro)
    }

    fn mais(
        &mut self,
        imposto: Imposto,
        natureza: Natureza,
        coeficiente: Percentual,
        explicacao: String,
    ) {
        if !coeficiente.e_zero() {
            self.termos.push(Termo {
                imposto,
                natureza,
                coeficiente,
                explicacao,
            });
        }
    }
}

/// O produto chega com o ICMS já pago antes (substituído): CSOSN 500 ou CST 60.
#[must_use]
pub fn e_substituido(p: &PerfilTributario) -> bool {
    matches!(p.situacao_icms.as_deref(), Some("500" | "60"))
}

/// PIS/COFINS monofásico (recolhidos pelo fabricante/importador): CST 04.
#[must_use]
pub fn e_monofasico(p: &PerfilTributario) -> bool {
    p.cst_pis.as_deref() == Some("04") || p.cst_cofins.as_deref() == Some("04")
}

/// `a × b`, exato (18% × 50% = 9%).
#[must_use]
pub fn vezes(a: Percentual, b: Percentual) -> Percentual {
    let un = i128::from(a.unidades_internas()) * i128::from(b.unidades_internas())
        / i128::from(Percentual::CEM.unidades_internas());
    Percentual::unidades(i64::try_from(un).unwrap_or(i64::MAX))
}

/// `p × (1 − partes ÷ 10 000)`: tira uma parcela da repartição (em centésimos de ponto).
fn sem_parcela(p: Percentual, partes: i64) -> Percentual {
    let un = i128::from(p.unidades_internas()) * i128::from(10_000 - partes) / 10_000;
    Percentual::unidades(i64::try_from(un).unwrap_or(0))
}

fn fmt(p: Percentual) -> String {
    p.formatar_com_simbolo()
}

/// O ICMS próprio e o FCP por dentro (`base` já com a redução), ou o que falta.
fn icms_proprio(p: &PerfilTributario) -> Result<(Percentual, Percentual), String> {
    let aliquota = p
        .aliquota_icms
        .ok_or_else(|| "informe a alíquota de ICMS do produto (ou da categoria)".to_owned())?;
    let base = p.reducao_base.unwrap_or(Percentual::ZERO).complemento();
    Ok((
        vezes(aliquota, base),
        vezes(p.aliquota_fcp.unwrap_or(Percentual::ZERO), base),
    ))
}

/// A ST que o substituto cobra por fora: `(1 + MVA) × alíquota − ICMS próprio`.
fn st_do_substituto(p: &PerfilTributario, proprio: Percentual) -> Result<Percentual, String> {
    let mva = p
        .mva_st
        .ok_or_else(|| "informe a MVA da substituição tributária do produto".to_owned())?;
    let aliquota = p
        .aliquota_icms
        .ok_or_else(|| "informe a alíquota de ICMS do produto (ou da categoria)".to_owned())?;
    let cheio = vezes(Percentual::CEM + mva, aliquota);
    Ok(if cheio.unidades_internas() > proprio.unidades_internas() {
        cheio - proprio
    } else {
        Percentual::ZERO
    })
}

/// A carga do produto no regime da empresa.
///
/// # Errors
/// A frase do que falta ou do que está inconsistente no perfil (CST no Simples, CSOSN no
/// normal, alíquota ausente, situação ainda não suportada).
pub fn carga(regime: &Regime, perfil: &PerfilTributario) -> Result<Carga, String> {
    let mut c = Carga::default();
    match *regime {
        Regime::Mei => {}
        Regime::Manual(p) => c.mais(
            Imposto::Manual,
            Natureza::PorDentro,
            p,
            format!("{} informado nos parâmetros de preço", fmt(p)),
        ),
        Regime::Simples { anexo, rbt12 } => simples_nacional(&mut c, anexo, rbt12, perfil)?,
        Regime::Presumido {
            atividade,
            receita_mensal,
        } => {
            regime_normal(&mut c, perfil, (pct(65, 2), pct(3, 0)), "cumulativos")?;
            let (p_irpj, p_csll) = match atividade {
                Atividade::Comercio | Atividade::Industria => (pct(8, 0), pct(12, 0)),
                Atividade::Servico => (pct(32, 0), pct(32, 0)),
            };
            c.mais(
                Imposto::Irpj,
                Natureza::PorDentro,
                vezes(pct(15, 0), p_irpj),
                format!(
                    "IRPJ 15% sobre o lucro presumido ({} da receita)",
                    fmt(p_irpj)
                ),
            );
            let lucro_presumido =
                receita_mensal.aplicar(p_irpj, cardeal_kernel::Arredondamento::MeioAcima);
            if lucro_presumido.em_centavos() > Dinheiro::reais(20_000).em_centavos() {
                c.mais(
                    Imposto::Irpj,
                    Natureza::PorDentro,
                    vezes(pct(10, 0), p_irpj),
                    "adicional de 10% do IRPJ: o lucro presumido passa de R$ 20 mil no mês".into(),
                );
            }
            c.mais(
                Imposto::Csll,
                Natureza::PorDentro,
                vezes(pct(9, 0), p_csll),
                format!(
                    "CSLL 9% sobre o lucro presumido ({} da receita)",
                    fmt(p_csll)
                ),
            );
        }
        Regime::Real {
            pis_cofins_nao_cumulativo,
        } => {
            let padrao = if pis_cofins_nao_cumulativo {
                (pct(165, 2), pct(76, 1))
            } else {
                (pct(65, 2), pct(3, 0))
            };
            let modo = if pis_cofins_nao_cumulativo {
                "não cumulativos"
            } else {
                "cumulativos"
            };
            regime_normal(&mut c, perfil, padrao, modo)?;
            c.mais(
                Imposto::Irpj,
                Natureza::SobreOLucro,
                pct(25, 0),
                "IRPJ 15% + adicional de 10% sobre o lucro (Lucro Real)".into(),
            );
            c.mais(
                Imposto::Csll,
                Natureza::SobreOLucro,
                pct(9, 0),
                "CSLL 9% sobre o lucro (Lucro Real)".into(),
            );
        }
    }
    Ok(c)
}

/// `inteiro,decimais` em pontos: `pct(165, 2)` = 1,65%.
fn pct(valor: i64, casas: u32) -> Percentual {
    Percentual::unidades(valor * 1_000_000 / 10_i64.pow(casas))
}

fn simples_nacional(
    c: &mut Carga,
    anexo: AnexoSimples,
    rbt12: Dinheiro,
    perfil: &PerfilTributario,
) -> Result<(), String> {
    let csosn = perfil.situacao_icms.clone().unwrap_or_else(|| "102".into());
    if csosn.len() != 3 {
        return Err(format!(
            "o perfil fiscal tem o CST {csosn} (regime normal), mas a empresa é do Simples — use um CSOSN (ex.: 102)"
        ));
    }
    let acima = rbt12.em_centavos() > SUBLIMITE.em_centavos();
    let efetiva = aliquota_efetiva(anexo, rbt12);
    let indice = simples::indice_da_faixa(anexo, rbt12);
    let mut retirar = 0;
    let mut motivos: Vec<&str> = Vec::new();
    if let Some((icms, pis_cofins)) = simples::reparticao(anexo, indice) {
        if acima {
            retirar += icms;
            motivos.push("sem a parte do ICMS, pago fora do DAS acima do sublimite");
        } else if csosn == "500" {
            retirar += icms;
            motivos.push("sem a parte do ICMS: já pago pelo fornecedor (ST)");
        }
        if e_monofasico(perfil) {
            retirar += pis_cofins;
            motivos.push("sem a parte do PIS/COFINS: monofásico");
        }
    }
    let das = sem_parcela(efetiva, retirar);
    let (_, teto) = faixa_do_simples(anexo, rbt12);
    let mut explicacao = format!(
        "DAS do Simples, {} (faixa até {}): alíquota efetiva {}",
        anexo.nome(),
        teto.formatar_com_simbolo(),
        fmt(efetiva)
    );
    for m in motivos {
        explicacao.push_str("; ");
        explicacao.push_str(m);
    }
    c.mais(Imposto::Das, Natureza::PorDentro, das, explicacao);

    match csosn.as_str() {
        "101" | "102" if acima => {
            let (icms, fcp) = icms_proprio(perfil)?;
            mais_icms(c, icms, fcp, "acima do sublimite, pago fora do DAS");
        }
        "201" | "202" => {
            let proprio = if acima {
                let (icms, fcp) = icms_proprio(perfil)?;
                mais_icms(c, icms, fcp, "acima do sublimite, pago fora do DAS");
                icms
            } else {
                perfil.aliquota_icms.unwrap_or(Percentual::ZERO)
            };
            let st = st_do_substituto(perfil, proprio)?;
            c.mais(
                Imposto::IcmsSt,
                Natureza::PorFora,
                st,
                format!("ICMS-ST cobrado do cliente (MVA {})", fmt(perfil.mva_st.unwrap_or_default())),
            );
        }
        "101" | "102" | "103" | "300" | "400" | "500" => {}
        outro => {
            return Err(format!(
                "o CSOSN {outro} ainda não entra no preço (use 101, 102, 103, 201, 202, 300, 400 ou 500)"
            ))
        }
    }
    Ok(())
}

fn mais_icms(c: &mut Carga, icms: Percentual, fcp: Percentual, contexto: &str) {
    c.mais(
        Imposto::Icms,
        Natureza::PorDentro,
        icms,
        format!("ICMS {} sobre o preço ({contexto})", fmt(icms)),
    );
    c.mais(
        Imposto::Fcp,
        Natureza::PorDentro,
        fcp,
        format!("fundo de combate à pobreza {}", fmt(fcp)),
    );
}

/// ICMS, FCP, ST do substituto, PIS/COFINS e IPI do regime normal; devolve o ICMS+FCP por
/// dentro (o que sai da base do PIS/COFINS).
fn regime_normal(
    c: &mut Carga,
    perfil: &PerfilTributario,
    (pis_padrao, cofins_padrao): (Percentual, Percentual),
    modo: &str,
) -> Result<Percentual, String> {
    let cst = perfil
        .situacao_icms
        .clone()
        .ok_or_else(|| "defina a situação do ICMS (CST) do produto ou da categoria".to_owned())?;
    if cst.len() != 2 {
        return Err(format!(
            "o perfil fiscal tem o CSOSN {cst} (Simples), mas a empresa é do regime normal — use um CST (ex.: 00)"
        ));
    }
    let (icms, fcp) = match cst.as_str() {
        "00" | "20" | "10" | "70" => icms_proprio(perfil)?,
        "40" | "41" | "50" | "60" => (Percentual::ZERO, Percentual::ZERO),
        outro => {
            return Err(format!(
            "o CST {outro} do ICMS ainda não entra no preço (use 00, 10, 20, 40, 41, 50, 60 ou 70)"
        ))
        }
    };
    let contexto = if perfil.reducao_base.is_some_and(|r| !r.e_zero()) {
        "com redução de base"
    } else {
        "por dentro"
    };
    mais_icms(c, icms, fcp, contexto);
    if matches!(cst.as_str(), "10" | "70") {
        let st = st_do_substituto(perfil, icms)?;
        c.mais(
            Imposto::IcmsSt,
            Natureza::PorFora,
            st,
            format!(
                "ICMS-ST cobrado do cliente (MVA {})",
                fmt(perfil.mva_st.unwrap_or_default())
            ),
        );
    }
    let icms_total = icms + fcp;
    let sem_icms = icms_total.complemento();
    for (imposto, cst, aliquota, padrao) in [
        (
            Imposto::Pis,
            &perfil.cst_pis,
            perfil.aliquota_pis,
            pis_padrao,
        ),
        (
            Imposto::Cofins,
            &perfil.cst_cofins,
            perfil.aliquota_cofins,
            cofins_padrao,
        ),
    ] {
        let cst = cst.as_deref().unwrap_or("01");
        if matches!(cst, "04" | "05" | "06" | "07" | "08" | "09") {
            continue;
        }
        let aliquota = aliquota.unwrap_or(padrao);
        c.mais(
            imposto,
            Natureza::PorDentro,
            vezes(aliquota, sem_icms),
            format!(
                "{} {} ({modo}) sobre a receita sem o ICMS",
                imposto.nome(),
                fmt(aliquota)
            ),
        );
    }
    if let Some(ipi) = perfil.aliquota_ipi {
        c.mais(
            Imposto::Ipi,
            Natureza::PorFora,
            ipi,
            format!("IPI {} somado ao preço, pago pelo cliente", fmt(ipi)),
        );
    }
    Ok(icms_total)
}
