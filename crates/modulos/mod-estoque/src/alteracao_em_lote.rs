//! Alteração em lote do catálogo: mudar margem, grupo e situação de muitos produtos de uma
//! vez, **vendo antes** o que muda e **podendo desfazer** (a última, por 24 horas). Domínio
//! puro; o SQL e os comandos ficam em `repositorio_lote` e `comandos/`.
//!
//! Cada produto guarda o "antes" e o "depois" da alteração. Desfazer só restaura o produto que
//! ainda está como a alteração o deixou: se alguém mexeu nele depois, a edição mais nova
//! vence e o produto vai para a lista de ignorados.

use cardeal_kernel::{Id, Instante, Percentual};

use crate::fiscal::CamposFiscais;
use serde::{Deserialize, Serialize};

/// Quanto tempo depois da aplicação ainda dá para desfazer (24 horas, em microssegundos).
pub const JANELA_DESFAZER_MICROS: i64 = 24 * 60 * 60 * 1_000_000;

/// O máximo de produtos numa alteração (acima disso, divida por grupo).
pub const MAX_PRODUTOS: usize = 5_000;

/// Mudar um campo ou deixá-lo como está.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Mudanca<T> {
    /// Não mexe no campo.
    #[default]
    Manter,
    /// Põe este valor em todos.
    Definir(T),
}

impl<T: Clone> Mudanca<T> {
    /// O valor novo, ou o que já estava.
    #[must_use]
    pub fn sobre(&self, atual: T) -> T {
        match self {
            Self::Manter => atual,
            Self::Definir(v) => v.clone(),
        }
    }
}

impl<T> Mudanca<T> {
    /// Há mudança neste campo?
    #[must_use]
    pub const fn muda(&self) -> bool {
        matches!(self, Self::Definir(_))
    }
}

/// O que fazer com os campos fiscais **próprios** do produto.
#[allow(clippy::large_enum_variant)] // uma por alteração; encaixotar só espalharia `Box`
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum MudancaFiscal {
    /// Não mexe.
    #[default]
    Manter,
    /// Preenche só os campos informados, mantendo os demais como estão.
    Mesclar(CamposFiscais),
    /// Apaga os campos próprios: o produto volta a herdar tudo do grupo.
    Limpar,
}

/// O que se pode alterar em lote.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Alteracao {
    /// A margem líquida desejada do produto (`Definir(None)` = voltar a herdar do grupo).
    pub margem: Mudanca<Option<Percentual>>,
    /// O grupo (transferência de grupo).
    pub grupo: Mudanca<Id>,
    /// Ativo ou inativo.
    pub ativo: Mudanca<bool>,
    /// O NCM (8 dígitos).
    pub ncm: Mudanca<String>,
    /// Os campos fiscais próprios.
    pub fiscal: MudancaFiscal,
}

impl Alteracao {
    /// Não muda nada.
    #[must_use]
    pub fn e_vazia(&self) -> bool {
        !self.margem.muda()
            && !self.grupo.muda()
            && !self.ativo.muda()
            && !self.ncm.muda()
            && self.fiscal == MudancaFiscal::Manter
    }

    /// Mexe em dado fiscal (NCM ou campos fiscais)? Exige a permissão de perfil tributário.
    #[must_use]
    pub fn mexe_em_fiscal(&self) -> bool {
        self.ncm.muda() || self.fiscal != MudancaFiscal::Manter
    }

    /// Valida o que a alteração pede (formatos fiscais, NCM).
    ///
    /// # Errors
    /// Mensagem para o usuário.
    pub fn validar(&self) -> Result<(), String> {
        if let Mudanca::Definir(n) = &self.ncm {
            if n.len() != 8 || !n.chars().all(|c| c.is_ascii_digit()) {
                return Err("NCM: precisa ter 8 dígitos".to_owned());
            }
        }
        if let MudancaFiscal::Mesclar(c) = &self.fiscal {
            if c.e_vazio() {
                return Err("Preencha ao menos um campo fiscal".to_owned());
            }
            c.validar()?;
        }
        Ok(())
    }

    /// Os valores do produto depois da alteração.
    #[must_use]
    pub fn aplicar(&self, v: &ValoresProduto) -> ValoresProduto {
        let mut fiscal = v.fiscal.clone();
        match &self.fiscal {
            MudancaFiscal::Manter => {}
            MudancaFiscal::Mesclar(c) => fiscal.mesclar(c),
            MudancaFiscal::Limpar => fiscal = CamposFiscais::default(),
        }
        ValoresProduto {
            grupo: self.grupo.sobre(v.grupo),
            margem: self.margem.sobre(v.margem),
            ativo: self.ativo.sobre(v.ativo),
            ncm: self.ncm.sobre(v.ncm.clone()),
            fiscal,
        }
    }

    /// Texto curto dos campos mexidos, para o histórico ("margem, grupo").
    #[must_use]
    pub fn campos(&self) -> String {
        let mut c = Vec::new();
        if self.margem.muda() {
            c.push("margem");
        }
        if self.grupo.muda() {
            c.push("grupo");
        }
        if self.ativo.muda() {
            c.push("situação");
        }
        if self.ncm.muda() {
            c.push("NCM");
        }
        if self.fiscal != MudancaFiscal::Manter {
            c.push("campos fiscais");
        }
        c.join(", ")
    }
}

/// Os campos de um produto que o lote sabe mexer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ValoresProduto {
    /// O grupo.
    pub grupo: Id,
    /// A margem própria (`None` = herda).
    pub margem: Option<Percentual>,
    /// Ativo.
    pub ativo: bool,
    /// O NCM.
    pub ncm: String,
    /// Os campos fiscais próprios.
    pub fiscal: CamposFiscais,
}

/// Os critérios para escolher produtos — todos opcionais e combinados com "e". A hierarquia
/// é só um deles.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FiltroDeLote {
    /// Só estes grupos (a tela já expande a subárvore).
    pub grupos: Option<Vec<Id>>,
    /// Texto no nome.
    pub busca: Option<String>,
    /// Texto no fabricante.
    pub fabricante: Option<String>,
    /// Começa com estes dígitos de NCM.
    pub ncm_prefixo: Option<String>,
    /// `Some(true)` só ativos, `Some(false)` só inativos.
    pub ativo: Option<bool>,
    /// Só os que não têm margem própria.
    pub sem_margem_propria: bool,
    /// Só os que não têm nenhum campo fiscal próprio.
    pub sem_perfil_fiscal_proprio: bool,
    /// Custo médio mínimo, em centavos.
    pub custo_minimo: Option<i64>,
    /// Custo médio máximo, em centavos.
    pub custo_maximo: Option<i64>,
    /// Só os sem custo (nunca tiveram entrada).
    pub sem_custo: bool,
}

/// Por que uma alteração não pode ser desfeita.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NaoPodeDesfazer {
    /// Já foi desfeita.
    JaDesfeita,
    /// Passaram mais de 24 horas.
    PrazoVencido,
    /// Houve uma alteração em lote mais recente: desfaz-se uma de cada vez, da última para a primeira.
    NaoEAUltima,
}

impl std::fmt::Display for NaoPodeDesfazer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::JaDesfeita => "esta alteração já foi desfeita",
            Self::PrazoVencido => "só dá para desfazer nas 24 horas seguintes à alteração",
            Self::NaoEAUltima => {
                "só a última alteração em lote pode ser desfeita; desfaça as mais recentes antes"
            }
        })
    }
}

/// A alteração pode ser desfeita agora?
///
/// # Errors
/// [`NaoPodeDesfazer`] conforme o motivo.
pub fn pode_desfazer(
    criada_em: Instante,
    agora: Instante,
    ja_desfeita: bool,
    e_a_ultima: bool,
) -> Result<(), NaoPodeDesfazer> {
    if ja_desfeita {
        return Err(NaoPodeDesfazer::JaDesfeita);
    }
    if agora.em_micros() - criada_em.em_micros() > JANELA_DESFAZER_MICROS {
        return Err(NaoPodeDesfazer::PrazoVencido);
    }
    if !e_a_ultima {
        return Err(NaoPodeDesfazer::NaoEAUltima);
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;

    fn valores() -> ValoresProduto {
        ValoresProduto {
            grupo: Id::novo(),
            margem: Some(Percentual::pontos(10)),
            ativo: true,
            ncm: "22021000".into(),
            fiscal: CamposFiscais::default(),
        }
    }

    #[test]
    fn aplicar_mexe_so_nos_campos_definidos() {
        let v = valores();
        let novo_grupo = Id::novo();
        let a = Alteracao {
            grupo: Mudanca::Definir(novo_grupo),
            ..Alteracao::default()
        };
        let d = a.aplicar(&v);
        assert_eq!(d.grupo, novo_grupo);
        assert_eq!((d.margem, d.ativo), (v.margem, v.ativo));
        assert_eq!(a.campos(), "grupo");
        assert!(!a.e_vazia());
        assert!(Alteracao::default().e_vazia());
    }

    #[test]
    fn definir_none_limpa_a_margem_propria() {
        let a = Alteracao {
            margem: Mudanca::Definir(None),
            ..Alteracao::default()
        };
        assert_eq!(a.aplicar(&valores()).margem, None);
        let b = Alteracao {
            margem: Mudanca::Definir(Some(Percentual::pontos(30))),
            ativo: Mudanca::Definir(false),
            ..Alteracao::default()
        };
        let d = b.aplicar(&valores());
        assert_eq!(d.margem, Some(Percentual::pontos(30)));
        assert!(!d.ativo);
        assert_eq!(b.campos(), "margem, situação");
    }

    #[test]
    fn ncm_e_campos_fiscais_mesclam_ou_limpam() {
        let mut v = valores();
        v.fiscal.cst_icms = Some("102".into());
        let mescla = Alteracao {
            ncm: Mudanca::Definir("19059090".into()),
            fiscal: MudancaFiscal::Mesclar(CamposFiscais {
                cfop_entrada: Some("1102".into()),
                ..CamposFiscais::default()
            }),
            ..Alteracao::default()
        };
        assert!(mescla.validar().is_ok());
        assert!(mescla.mexe_em_fiscal());
        let d = mescla.aplicar(&v);
        assert_eq!(d.ncm, "19059090");
        assert_eq!(
            d.fiscal.cst_icms.as_deref(),
            Some("102"),
            "o que não foi pedido fica"
        );
        assert_eq!(d.fiscal.cfop_entrada.as_deref(), Some("1102"));
        assert_eq!(mescla.campos(), "NCM, campos fiscais");

        let limpa = Alteracao {
            fiscal: MudancaFiscal::Limpar,
            ..Alteracao::default()
        };
        assert!(limpa.aplicar(&d).fiscal.e_vazio());
        assert!(!limpa.e_vazia());
        // NCM errado e remendo vazio são recusados antes de gravar.
        let ncm_ruim = Alteracao {
            ncm: Mudanca::Definir("123".into()),
            ..Alteracao::default()
        };
        assert!(ncm_ruim.validar().unwrap_err().contains("NCM"));
        let vazio = Alteracao {
            fiscal: MudancaFiscal::Mesclar(CamposFiscais::default()),
            ..Alteracao::default()
        };
        assert!(vazio.validar().is_err());
        assert!(!Alteracao::default().mexe_em_fiscal());
    }

    #[test]
    fn desfazer_exige_ser_a_ultima_dentro_de_24h_e_nao_desfeita() {
        let t0 = Instante::de_micros(1_000_000);
        let dentro = Instante::de_micros(t0.em_micros() + JANELA_DESFAZER_MICROS);
        let fora = Instante::de_micros(t0.em_micros() + JANELA_DESFAZER_MICROS + 1);
        assert_eq!(pode_desfazer(t0, dentro, false, true), Ok(()));
        assert_eq!(
            pode_desfazer(t0, fora, false, true),
            Err(NaoPodeDesfazer::PrazoVencido)
        );
        assert_eq!(
            pode_desfazer(t0, dentro, true, true),
            Err(NaoPodeDesfazer::JaDesfeita)
        );
        assert_eq!(
            pode_desfazer(t0, dentro, false, false),
            Err(NaoPodeDesfazer::NaoEAUltima)
        );
    }
}
