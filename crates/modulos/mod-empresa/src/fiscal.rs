//! Os dados fiscais da empresa — o que vai no bloco do emitente de toda NF-e/NFC-e:
//! inscrição estadual e municipal e o endereço com o código IBGE do município
//! (`docs/modulos/fiscal.md` §3). Ficam em `nucleo_empresa` (`ie`, `im`, `endereco` em JSON).

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{CodigoErro, Erro, InscricaoEstadual, Uf};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// O endereço do emitente, como a NF-e o pede.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnderecoFiscal {
    /// Logradouro (rua, avenida...).
    pub logradouro: String,
    /// Número ("S/N" quando não há).
    pub numero: String,
    /// Complemento.
    pub complemento: String,
    /// Bairro.
    pub bairro: String,
    /// Nome do município.
    pub municipio: String,
    /// Código IBGE do município (7 dígitos — os 2 primeiros são o da UF).
    pub codigo_municipio: String,
    /// Sigla da UF.
    pub uf: String,
    /// CEP (8 dígitos).
    pub cep: String,
}

/// Os dados fiscais da empresa.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DadosFiscais {
    /// Inscrição estadual (ou `ISENTO`).
    pub inscricao_estadual: String,
    /// Inscrição municipal (para NFS-e; vazia se a empresa não presta serviço).
    pub inscricao_municipal: String,
    /// O endereço do emitente.
    pub endereco: EnderecoFiscal,
}

fn digitos(texto: &str) -> String {
    texto.chars().filter(char::is_ascii_digit).collect()
}

impl DadosFiscais {
    /// Normaliza (apara espaços, CEP/IBGE só com dígitos, UF e IE em maiúsculas) e confere.
    ///
    /// # Errors
    /// [`CodigoErro::CAMPO_OBRIGATORIO`]/[`CodigoErro::VALOR_INVALIDO`], com o campo marcado.
    pub fn normalizado(mut self) -> Result<Self, Erro> {
        let e = &mut self.endereco;
        for campo in [
            &mut e.logradouro,
            &mut e.numero,
            &mut e.complemento,
            &mut e.bairro,
            &mut e.municipio,
        ] {
            *campo = campo.trim().to_string();
        }
        e.uf = e.uf.trim().to_ascii_uppercase();
        e.cep = digitos(&e.cep);
        e.codigo_municipio = digitos(&e.codigo_municipio);
        self.inscricao_estadual = self.inscricao_estadual.trim().to_ascii_uppercase();
        self.inscricao_municipal = self.inscricao_municipal.trim().to_string();

        let obrigatorio = |valor: &str, campo: &str| {
            if valor.is_empty() {
                Err(Erro::obrigatorio(campo))
            } else {
                Ok(())
            }
        };
        obrigatorio(&self.inscricao_estadual, "inscricao_estadual")?;
        obrigatorio(&e.logradouro, "logradouro")?;
        obrigatorio(&e.numero, "numero")?;
        obrigatorio(&e.bairro, "bairro")?;
        obrigatorio(&e.municipio, "municipio")?;
        let uf = Uf::de_sigla(&e.uf)
            .map_err(|_| Erro::novo(CodigoErro::VALOR_INVALIDO, "UF inválida").no_campo("uf"))?;
        if e.cep.len() != 8 {
            return Err(
                Erro::novo(CodigoErro::VALOR_INVALIDO, "O CEP tem 8 dígitos").no_campo("cep"),
            );
        }
        if e.codigo_municipio.len() != 7 {
            return Err(Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                "O código IBGE do município tem 7 dígitos",
            )
            .no_campo("codigo_municipio"));
        }
        if !e
            .codigo_municipio
            .starts_with(&format!("{:02}", uf.codigo_ibge()))
        {
            return Err(Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                format!(
                    "O código IBGE {} não é de um município de {}",
                    e.codigo_municipio,
                    uf.sigla()
                ),
            )
            .no_campo("codigo_municipio"));
        }
        let ie = InscricaoEstadual::nova(&self.inscricao_estadual, uf)
            .map_err(|erro| erro.no_campo("inscricao_estadual"))?;
        self.inscricao_estadual = ie.para_nfe();
        Ok(self)
    }

    /// Verdadeiro se tudo o que a NF-e exige do emitente está preenchido.
    #[must_use]
    pub fn completos(&self) -> bool {
        self.clone().normalizado().is_ok()
    }

    /// A UF do emitente, se o endereço tiver uma válida.
    #[must_use]
    pub fn uf(&self) -> Option<Uf> {
        Uf::de_sigla(&self.endereco.uf).ok()
    }
}

/// Grava os dados fiscais (substitui os anteriores).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirDadosFiscaisEmpresa {
    /// Os dados.
    pub dados: DadosFiscais,
}

impl Comando for DefinirDadosFiscaisEmpresa {
    type Saida = DadosFiscais;
    const PERMISSAO: &'static str = "empresa.dados.editar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<DadosFiscais> {
        let dados = self.dados.normalizado()?;
        crate::sql::gravar_dados_fiscais(uow.conexao(), ctx.empresa, &dados)?;
        Ok(dados)
    }
}

/// Os dados fiscais da empresa (vazios se ainda não preenchidos).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct DadosFiscaisDaEmpresa;

impl Consulta for DadosFiscaisDaEmpresa {
    type Saida = DadosFiscais;
    const PERMISSAO: &'static str = "empresa.dados.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<DadosFiscais> {
        crate::sql::dados_fiscais(c, ctx.empresa)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    pub(crate) fn exemplo() -> DadosFiscais {
        DadosFiscais {
            inscricao_estadual: " 255.123.456 ".into(),
            inscricao_municipal: "12345".into(),
            endereco: EnderecoFiscal {
                logradouro: "Rua das Flores ".into(),
                numero: "100".into(),
                complemento: String::new(),
                bairro: "Centro".into(),
                municipio: "Piratuba".into(),
                codigo_municipio: "4213104".into(),
                uf: "sc".into(),
                cep: "89667-000".into(),
            },
        }
    }

    #[test]
    fn normaliza_e_confere() {
        let d = exemplo().normalizado().unwrap();
        assert_eq!(d.inscricao_estadual, "255123456");
        assert_eq!(d.endereco.uf, "SC");
        assert_eq!(d.endereco.cep, "89667000");
        assert_eq!(d.endereco.logradouro, "Rua das Flores");
        assert_eq!(d.uf(), Some(Uf::Sc));
        assert!(d.completos());

        let mut isento = exemplo();
        isento.inscricao_estadual = "isento".into();
        assert_eq!(isento.normalizado().unwrap().inscricao_estadual, "ISENTO");
    }

    #[test]
    fn aponta_o_campo_errado() {
        let campo = |f: fn(&mut DadosFiscais)| {
            let mut d = exemplo();
            f(&mut d);
            d.normalizado().unwrap_err().campo
        };
        assert_eq!(
            campo(|d| d.endereco.cep = "123".into()).as_deref(),
            Some("cep")
        );
        assert_eq!(
            campo(|d| d.endereco.uf = "XX".into()).as_deref(),
            Some("uf")
        );
        // 35... é de SP, não de SC.
        assert_eq!(
            campo(|d| d.endereco.codigo_municipio = "3550308".into()).as_deref(),
            Some("codigo_municipio")
        );
        assert_eq!(
            campo(|d| d.endereco.bairro.clear()).as_deref(),
            Some("bairro")
        );
        assert_eq!(
            campo(|d| d.inscricao_estadual = "1".into()).as_deref(),
            Some("inscricao_estadual")
        );
        assert!(!DadosFiscais::default().completos());
    }
}
