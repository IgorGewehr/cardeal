//! O formulário do perfil fiscal (origem, CST/CSOSN, CFOPs, alíquotas, MVA-ST, CEST,
//! benefício). Um só bloco, usado no grupo (aba Estrutura), no produto (editar) e na alteração
//! em lote — campo em branco significa "herda" (ou "não mexe", no lote).

use super::*;
use cardeal_kernel::Percentual;
use mod_estoque::fiscal::{CamposFiscais, LinhaFiscal};

/// Os campos como o usuário digita: texto.
#[derive(Default, Clone)]
pub(super) struct FormFiscal {
    pub(super) origem: String,
    pub(super) cst_icms: String,
    pub(super) cfop_saida_dentro: String,
    pub(super) cfop_saida_fora: String,
    pub(super) cfop_entrada: String,
    pub(super) aliquota_icms: String,
    pub(super) reducao_base: String,
    pub(super) mva_st: String,
    pub(super) cst_pis: String,
    pub(super) cst_cofins: String,
    pub(super) aliquota_pis: String,
    pub(super) aliquota_cofins: String,
    pub(super) cest: String,
    pub(super) beneficio: String,
}

fn pct(p: Option<Percentual>) -> String {
    p.map(|p| p.formatar(2)).unwrap_or_default()
}

fn le_pct(nome: &str, t: &str) -> Result<Option<Percentual>, String> {
    let t = t.trim();
    if t.is_empty() {
        return Ok(None);
    }
    Percentual::de_str(t)
        .map(Some)
        .map_err(|_| format!("{nome}: digite um número, como 18 ou 12,5"))
}

fn texto(t: &str) -> Option<String> {
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

impl FormFiscal {
    pub(super) fn de(c: &CamposFiscais) -> Self {
        Self {
            origem: c.origem.map(|o| o.to_string()).unwrap_or_default(),
            cst_icms: c.cst_icms.clone().unwrap_or_default(),
            cfop_saida_dentro: c.cfop_saida_dentro_uf.clone().unwrap_or_default(),
            cfop_saida_fora: c.cfop_saida_fora_uf.clone().unwrap_or_default(),
            cfop_entrada: c.cfop_entrada.clone().unwrap_or_default(),
            aliquota_icms: pct(c.aliquota_icms),
            reducao_base: pct(c.reducao_base),
            mva_st: pct(c.mva_st),
            cst_pis: c.cst_pis.clone().unwrap_or_default(),
            cst_cofins: c.cst_cofins.clone().unwrap_or_default(),
            aliquota_pis: pct(c.aliquota_pis),
            aliquota_cofins: pct(c.aliquota_cofins),
            cest: c.cest.clone().unwrap_or_default(),
            beneficio: c.beneficio_fiscal.clone().unwrap_or_default(),
        }
    }

    /// Monta e valida os campos; erro com o nome do campo ruim.
    pub(super) fn para(&self) -> Result<CamposFiscais, String> {
        let origem = match self.origem.trim() {
            "" => None,
            t => Some(
                t.parse::<u8>()
                    .map_err(|_| "Origem da mercadoria: use um número de 0 a 8".to_owned())?,
            ),
        };
        let c = CamposFiscais {
            origem,
            cst_icms: texto(&self.cst_icms),
            cfop_saida_dentro_uf: texto(&self.cfop_saida_dentro),
            cfop_saida_fora_uf: texto(&self.cfop_saida_fora),
            cfop_entrada: texto(&self.cfop_entrada),
            aliquota_icms: le_pct("Alíquota de ICMS", &self.aliquota_icms)?,
            reducao_base: le_pct("Redução de base", &self.reducao_base)?,
            mva_st: le_pct("MVA da ST", &self.mva_st)?,
            cst_pis: texto(&self.cst_pis),
            cst_cofins: texto(&self.cst_cofins),
            aliquota_pis: le_pct("Alíquota de PIS", &self.aliquota_pis)?,
            aliquota_cofins: le_pct("Alíquota de COFINS", &self.aliquota_cofins)?,
            cest: texto(&self.cest),
            beneficio_fiscal: texto(&self.beneficio),
        };
        c.validar()?;
        Ok(c)
    }

    /// Desenha os campos em três colunas.
    pub(super) fn mostrar(&mut self, ui: &mut egui::Ui) {
        ui.columns(3, |c| {
            c[0].add(Campo::novo("Origem (0 a 8)", &mut self.origem).marcador("0"));
            c[1].add(Campo::novo("CST/CSOSN do ICMS", &mut self.cst_icms).marcador("102"));
            c[2].add(Campo::novo("CEST", &mut self.cest).marcador("7 dígitos"));
        });
        ui.add_space(Espaco::E8);
        ui.columns(3, |c| {
            c[0].add(Campo::novo("CFOP saída na UF", &mut self.cfop_saida_dentro).marcador("5102"));
            c[1].add(
                Campo::novo("CFOP saída fora da UF", &mut self.cfop_saida_fora).marcador("6102"),
            );
            c[2].add(Campo::novo("CFOP de entrada", &mut self.cfop_entrada).marcador("1102"));
        });
        ui.add_space(Espaco::E8);
        ui.columns(3, |c| {
            c[0].add(Campo::novo("Alíquota de ICMS (%)", &mut self.aliquota_icms).marcador("18"));
            c[1].add(Campo::novo("Redução de base (%)", &mut self.reducao_base));
            c[2].add(Campo::novo("MVA da ST (%)", &mut self.mva_st));
        });
        ui.add_space(Espaco::E8);
        ui.columns(4, |c| {
            c[0].add(Campo::novo("CST do PIS", &mut self.cst_pis).marcador("01"));
            c[1].add(Campo::novo("CST da COFINS", &mut self.cst_cofins).marcador("01"));
            c[2].add(Campo::novo("Alíquota de PIS (%)", &mut self.aliquota_pis).marcador("0,65"));
            c[3].add(
                Campo::novo("Alíquota de COFINS (%)", &mut self.aliquota_cofins).marcador("3"),
            );
        });
        ui.add_space(Espaco::E8);
        ui.add(Campo::novo("Benefício fiscal", &mut self.beneficio));
    }
}

/// A tabela do perfil vigente: cada campo, o valor e de onde ele veio.
pub(super) fn tabela_efetiva(ui: &mut egui::Ui, linhas: &[LinhaFiscal]) {
    Grade::nova(vec![
        ColunaGrade::nova("Campo").largura(220.0),
        ColunaGrade::nova("Valor vigente").largura(160.0),
        ColunaGrade::nova("Definido em").largura(200.0),
    ])
    .id_salt("perfil-fiscal-efetivo")
    .mostrar(ui, linhas.len(), |i, row| {
        let l = &linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(l.campo.clone()));
        });
        row.col(|ui| match &l.valor {
            Some(v) => {
                ui.add(Rotulo::interface(v.clone()));
            }
            None => {
                ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
            }
        });
        row.col(|ui| match &l.origem {
            Some(o) if o == "Produto" => {
                ui.add(Etiqueta::info("neste produto"));
            }
            Some(o) => {
                ui.add(Etiqueta::neutra(o.clone()));
            }
            None => {
                ui.add(Rotulo::interface("não definido").cor(ui.cores().texto_fraco));
            }
        });
    });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn campos_em_branco_herdam_e_o_que_foi_digitado_e_validado() {
        let vazio = FormFiscal::default();
        assert!(vazio.para().unwrap().e_vazio());

        let mut f = FormFiscal {
            cst_icms: "102".into(),
            aliquota_icms: "18,5".into(),
            cfop_saida_dentro: "5102".into(),
            ..FormFiscal::default()
        };
        let c = f.para().unwrap();
        assert_eq!(c.aliquota_icms, Some(Percentual::milesimos(18_500)));
        // Ida e volta: o que foi lido volta ao mesmo texto digitável.
        assert_eq!(FormFiscal::de(&c).aliquota_icms, "18,50");

        f.cfop_saida_dentro = "6102".into();
        assert!(f.para().unwrap_err().contains("começar com 5"));
        f.cfop_saida_dentro.clear();
        f.aliquota_icms = "abc".into();
        assert!(f.para().unwrap_err().contains("ICMS"));
        f.aliquota_icms.clear();
        f.origem = "x".into();
        assert!(f.para().unwrap_err().contains("Origem"));
    }
}
