//! O perfil fiscal do produto: origem, CST/CSOSN, CFOPs, alíquotas, MVA-ST, CEST e benefício.
//! Cada campo é opcional e **herdado**: vale o do produto, senão o do grupo mais próximo acima.
//! Domínio puro. É a estimativa que o sistema guarda para não consultar a API fiscal item a
//! item; a autoridade no momento da emissão continua sendo a API fiscal (doc 10 §6, ADR-0006).
//!
//! NCM não está aqui: é o campo obrigatório do próprio produto (`estoque_produto.ncm`).

use cardeal_kernel::Percentual;
use serde::{Deserialize, Serialize};

/// Os campos fiscais de um produto ou de um grupo. `None` = herda.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CamposFiscais {
    /// Origem da mercadoria (tabela da NF-e, 0 a 8).
    pub origem: Option<u8>,
    /// CST do ICMS (2 dígitos) ou CSOSN do Simples (3 dígitos).
    pub cst_icms: Option<String>,
    /// CFOP de saída dentro da UF (4 dígitos, 5xxx).
    pub cfop_saida_dentro_uf: Option<String>,
    /// CFOP de saída para fora da UF (6xxx).
    pub cfop_saida_fora_uf: Option<String>,
    /// CFOP de entrada (1xxx, 2xxx ou 3xxx).
    pub cfop_entrada: Option<String>,
    /// Alíquota de ICMS.
    pub aliquota_icms: Option<Percentual>,
    /// Redução da base de cálculo do ICMS.
    pub reducao_base: Option<Percentual>,
    /// MVA da substituição tributária.
    pub mva_st: Option<Percentual>,
    /// CST do PIS (2 dígitos).
    pub cst_pis: Option<String>,
    /// CST da COFINS (2 dígitos).
    pub cst_cofins: Option<String>,
    /// Alíquota de PIS.
    pub aliquota_pis: Option<Percentual>,
    /// Alíquota de COFINS.
    pub aliquota_cofins: Option<Percentual>,
    /// CEST (7 dígitos), quando o NCM exige substituição tributária.
    pub cest: Option<String>,
    /// Código do benefício fiscal.
    pub beneficio_fiscal: Option<String>,
}

fn so_digitos(t: &str, tamanhos: &[usize]) -> bool {
    t.chars().all(|c| c.is_ascii_digit()) && tamanhos.contains(&t.len())
}

impl CamposFiscais {
    /// Nenhum campo preenchido.
    #[must_use]
    pub fn e_vazio(&self) -> bool {
        *self == Self::default()
    }

    /// Valida formatos (dígitos e tamanhos) e faixas (0%..100%).
    ///
    /// # Errors
    /// Mensagem para o usuário com o nome do campo ruim.
    pub fn validar(&self) -> Result<(), String> {
        let texto = |nome: &str, v: &Option<String>, tam: &[usize], dica: &str| match v {
            Some(t) if !so_digitos(t, tam) => Err(format!("{nome}: {dica}")),
            _ => Ok(()),
        };
        let pct = |nome: &str, v: Option<Percentual>| match v {
            Some(p) if !p.e_fracao_valida() => Err(format!("{nome}: use um valor entre 0% e 100%")),
            _ => Ok(()),
        };
        if self.origem.is_some_and(|o| o > 8) {
            return Err("Origem da mercadoria: use de 0 a 8".to_owned());
        }
        texto(
            "CST/CSOSN do ICMS",
            &self.cst_icms,
            &[2, 3],
            "2 dígitos (CST) ou 3 (CSOSN)",
        )?;
        texto(
            "CFOP de saída na UF",
            &self.cfop_saida_dentro_uf,
            &[4],
            "4 dígitos",
        )?;
        texto(
            "CFOP de saída fora da UF",
            &self.cfop_saida_fora_uf,
            &[4],
            "4 dígitos",
        )?;
        texto("CFOP de entrada", &self.cfop_entrada, &[4], "4 dígitos")?;
        texto("CST do PIS", &self.cst_pis, &[2], "2 dígitos")?;
        texto("CST da COFINS", &self.cst_cofins, &[2], "2 dígitos")?;
        texto("CEST", &self.cest, &[7], "7 dígitos")?;
        pct("Alíquota de ICMS", self.aliquota_icms)?;
        pct("Redução de base", self.reducao_base)?;
        pct("MVA da ST", self.mva_st)?;
        pct("Alíquota de PIS", self.aliquota_pis)?;
        pct("Alíquota de COFINS", self.aliquota_cofins)?;
        // CFOP começa pelo sentido da operação.
        for (nome, v, iniciais) in [
            ("CFOP de saída na UF", &self.cfop_saida_dentro_uf, "5"),
            ("CFOP de saída fora da UF", &self.cfop_saida_fora_uf, "6"),
        ] {
            if v.as_deref().is_some_and(|t| !t.starts_with(iniciais)) {
                return Err(format!("{nome}: deve começar com {iniciais}"));
            }
        }
        if self
            .cfop_entrada
            .as_deref()
            .is_some_and(|t| !t.starts_with(['1', '2', '3']))
        {
            return Err("CFOP de entrada: deve começar com 1, 2 ou 3".to_owned());
        }
        Ok(())
    }

    /// Mescla `remendo` por cima: cada campo preenchido no remendo substitui o daqui.
    pub fn mesclar(&mut self, remendo: &Self) {
        macro_rules! campo {
            ($($f:ident),*) => {$( if remendo.$f.is_some() { self.$f.clone_from(&remendo.$f); } )*};
        }
        campo!(
            origem,
            cst_icms,
            cfop_saida_dentro_uf,
            cfop_saida_fora_uf,
            cfop_entrada,
            aliquota_icms,
            reducao_base,
            mva_st,
            cst_pis,
            cst_cofins,
            aliquota_pis,
            aliquota_cofins,
            cest,
            beneficio_fiscal
        );
    }

    /// Cada campo com o texto de exibição, na ordem da tela.
    #[must_use]
    pub fn lista(&self) -> [(&'static str, Option<String>); 14] {
        let p = |v: Option<Percentual>| v.map(|p| format!("{}%", p.formatar(2)));
        [
            ("Origem da mercadoria", self.origem.map(|o| o.to_string())),
            ("CST/CSOSN do ICMS", self.cst_icms.clone()),
            ("CFOP de saída na UF", self.cfop_saida_dentro_uf.clone()),
            ("CFOP de saída fora da UF", self.cfop_saida_fora_uf.clone()),
            ("CFOP de entrada", self.cfop_entrada.clone()),
            ("Alíquota de ICMS", p(self.aliquota_icms)),
            ("Redução de base", p(self.reducao_base)),
            ("MVA da ST", p(self.mva_st)),
            ("CST do PIS", self.cst_pis.clone()),
            ("CST da COFINS", self.cst_cofins.clone()),
            ("Alíquota de PIS", p(self.aliquota_pis)),
            ("Alíquota de COFINS", p(self.aliquota_cofins)),
            ("CEST", self.cest.clone()),
            ("Benefício fiscal", self.beneficio_fiscal.clone()),
        ]
    }

    /// Serializa para a coluna `perfil_fiscal`; vazio vira `None` (coluna nula).
    #[must_use]
    pub fn para_blob(&self) -> Option<Vec<u8>> {
        (!self.e_vazio())
            .then(|| postcard::to_stdvec(self).ok())
            .flatten()
    }

    /// Lê da coluna; ausente ou ilegível vira vazio (herda tudo).
    #[must_use]
    pub fn de_blob(blob: Option<&[u8]>) -> Self {
        blob.and_then(|b| postcard::from_bytes(b).ok())
            .unwrap_or_default()
    }
}

/// Um campo no perfil efetivo, com quem o definiu.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaFiscal {
    /// O nome do campo.
    pub campo: String,
    /// O valor vigente; `None` = ninguém definiu.
    pub valor: Option<String>,
    /// De onde veio: "Produto" ou o nome do grupo.
    pub origem: Option<String>,
}

/// O perfil efetivo: para cada campo, o primeiro valor da cadeia (produto, depois o grupo
/// mais próximo e seus ancestrais). `cadeia` vem do mais específico para o mais geral.
#[must_use]
pub fn efetivo(cadeia: &[(String, CamposFiscais)]) -> Vec<LinhaFiscal> {
    let listas: Vec<_> = cadeia.iter().map(|(n, c)| (n, c.lista())).collect();
    CamposFiscais::default()
        .lista()
        .iter()
        .enumerate()
        .map(|(i, (campo, _))| {
            let achado = listas
                .iter()
                .find_map(|(nome, l)| l[i].1.clone().map(|v| (v, (*nome).clone())));
            LinhaFiscal {
                campo: (*campo).to_owned(),
                valor: achado.as_ref().map(|a| a.0.clone()),
                origem: achado.map(|a| a.1),
            }
        })
        .collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn valida_formatos_e_sentido_do_cfop() {
        let ok = CamposFiscais {
            origem: Some(0),
            cst_icms: Some("102".into()),
            cfop_saida_dentro_uf: Some("5102".into()),
            cfop_saida_fora_uf: Some("6102".into()),
            cfop_entrada: Some("1102".into()),
            cest: Some("1700100".into()),
            aliquota_icms: Some(Percentual::pontos(18)),
            ..CamposFiscais::default()
        };
        assert!(ok.validar().is_ok());
        let ruim = |f: &dyn Fn(&mut CamposFiscais)| {
            let mut c = ok.clone();
            f(&mut c);
            c.validar().unwrap_err()
        };
        assert!(ruim(&|c| c.origem = Some(9)).contains("Origem"));
        assert!(ruim(&|c| c.cst_icms = Some("1".into())).contains("CST/CSOSN"));
        assert!(ruim(&|c| c.cfop_saida_dentro_uf = Some("6102".into())).contains("começar com 5"));
        assert!(ruim(&|c| c.cfop_entrada = Some("5102".into())).contains("1, 2 ou 3"));
        assert!(ruim(&|c| c.cest = Some("12".into())).contains("CEST"));
        assert!(ruim(&|c| c.aliquota_icms = Some(Percentual::pontos(101))).contains("ICMS"));
        assert!(ruim(&|c| c.cst_pis = Some("ab".into())).contains("PIS"));
    }

    #[test]
    fn mesclar_so_troca_o_que_o_remendo_preenche() {
        let mut a = CamposFiscais {
            cst_icms: Some("102".into()),
            cfop_entrada: Some("1102".into()),
            ..CamposFiscais::default()
        };
        a.mesclar(&CamposFiscais {
            cfop_entrada: Some("1403".into()),
            aliquota_pis: Some(Percentual::milesimos(650)),
            ..CamposFiscais::default()
        });
        assert_eq!(a.cst_icms.as_deref(), Some("102"));
        assert_eq!(a.cfop_entrada.as_deref(), Some("1403"));
        assert_eq!(a.aliquota_pis, Some(Percentual::milesimos(650)));
    }

    #[test]
    fn efetivo_pega_o_mais_proximo_e_diz_de_onde_veio() {
        let produto = CamposFiscais {
            cfop_saida_dentro_uf: Some("5405".into()),
            ..CamposFiscais::default()
        };
        let sub = CamposFiscais {
            cst_icms: Some("500".into()),
            cfop_saida_dentro_uf: Some("5102".into()),
            ..CamposFiscais::default()
        };
        let raiz = CamposFiscais {
            origem: Some(0),
            cst_icms: Some("102".into()),
            ..CamposFiscais::default()
        };
        let e = efetivo(&[
            ("Produto".into(), produto),
            ("Refrigerantes".into(), sub),
            ("Bebidas".into(), raiz),
        ]);
        let linha = |campo: &str| e.iter().find(|l| l.campo == campo).unwrap();
        assert_eq!(linha("CFOP de saída na UF").valor.as_deref(), Some("5405"));
        assert_eq!(
            linha("CFOP de saída na UF").origem.as_deref(),
            Some("Produto")
        );
        assert_eq!(
            linha("CST/CSOSN do ICMS").origem.as_deref(),
            Some("Refrigerantes")
        );
        assert_eq!(
            linha("Origem da mercadoria").origem.as_deref(),
            Some("Bebidas")
        );
        assert_eq!(linha("CEST").valor, None);
    }

    #[test]
    fn blob_vai_e_volta_e_vazio_vira_nulo() {
        assert_eq!(CamposFiscais::default().para_blob(), None);
        let c = CamposFiscais {
            cest: Some("1700100".into()),
            ..CamposFiscais::default()
        };
        assert_eq!(CamposFiscais::de_blob(c.para_blob().as_deref()), c);
        assert!(CamposFiscais::de_blob(Some(&[255, 255, 255])).e_vazio());
    }
}
