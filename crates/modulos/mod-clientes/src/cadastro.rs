//! Documento, endereço e contato — as entidades satélite da [`Pessoa`](crate::Pessoa).
//!
//! `docs/modulos/clientes.md` §3 e §11.8: CPF/CNPJ são **validados por dígito verificador no
//! domínio, sem I/O** (a consulta à SEFAZ é confirmação adicional, não a única validação).

use cardeal_kernel::{texto, Documento, Id, Instante, Uf};
use serde::{Deserialize, Serialize};

use crate::erros::ErroClientes;

/// O tipo de um [`DocumentoPessoa`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoDocumento {
    /// CPF (pessoa física) — dígito verificador validado.
    Cpf,
    /// CNPJ (pessoa jurídica) — dígito verificador validado.
    Cnpj,
    /// Registro geral.
    Rg,
    /// Inscrição estadual.
    Ie,
    /// Inscrição municipal.
    Im,
    /// Passaporte.
    Passaporte,
}

/// Um documento vinculado a uma pessoa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentoPessoa {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// A pessoa.
    pub pessoa: Id,
    /// O tipo.
    pub tipo: TipoDocumento,
    /// O número, sem máscara (só dígitos para CPF/CNPJ).
    pub numero: String,
    /// Órgão emissor (RG).
    pub orgao_emissor: Option<String>,
    /// Quando a SEFAZ confirmou o cadastro, se confirmou.
    pub validado_sefaz_em: Option<Instante>,
}

impl DocumentoPessoa {
    /// Cria um documento, validando o dígito verificador quando o tipo é CPF ou CNPJ.
    ///
    /// # Errors
    /// [`ErroClientes::DocumentoInvalido`] se o CPF/CNPJ não passar na validação.
    pub fn novo(
        empresa: Id,
        pessoa: Id,
        tipo: TipoDocumento,
        numero: &str,
    ) -> Result<Self, ErroClientes> {
        let numero = match tipo {
            TipoDocumento::Cpf | TipoDocumento::Cnpj => {
                let doc = Documento::novo(numero).map_err(|_| {
                    ErroClientes::DocumentoInvalido("dígito verificador não confere")
                })?;
                let corresponde = match tipo {
                    TipoDocumento::Cpf => doc.e_fisica(),
                    _ => doc.e_juridica(),
                };
                if !corresponde {
                    return Err(ErroClientes::DocumentoInvalido(
                        "o número não corresponde ao tipo informado",
                    ));
                }
                doc.sem_mascara()
            }
            _ => {
                let limpo = numero.trim().to_string();
                if limpo.is_empty() {
                    return Err(ErroClientes::DocumentoInvalido("número vazio"));
                }
                limpo
            }
        };
        Ok(Self {
            id: Id::novo(),
            empresa,
            pessoa,
            tipo,
            numero,
            orgao_emissor: None,
            validado_sefaz_em: None,
        })
    }

    /// A chave de unicidade dentro da empresa: `(tipo, numero)` — espelha
    /// `UNIQUE(empresa, tipo, numero)` do `docs/modulos/clientes.md` §13.
    #[must_use]
    pub fn chave_unica(&self) -> (TipoDocumento, &str) {
        (self.tipo, self.numero.as_str())
    }
}

/// O tipo de um [`Endereco`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoEndereco {
    /// Endereço de cobrança.
    Cobranca,
    /// Endereço de entrega.
    Entrega,
    /// Endereço comercial.
    Comercial,
    /// Endereço residencial.
    Residencial,
}

/// Um endereço de uma pessoa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endereco {
    /// Identidade.
    pub id: Id,
    /// A pessoa.
    pub pessoa: Id,
    /// O tipo.
    pub tipo: TipoEndereco,
    /// Logradouro.
    pub logradouro: String,
    /// Número.
    pub numero: String,
    /// Complemento (opcional).
    pub complemento: Option<String>,
    /// Bairro.
    pub bairro: String,
    /// Cidade.
    pub cidade: String,
    /// UF (validada contra as 27 siglas).
    pub uf: Uf,
    /// CEP (8 dígitos, sem máscara).
    pub cep: String,
    /// Um único principal por tipo.
    pub principal: bool,
}

impl Endereco {
    /// Cria um endereço validando UF e CEP.
    ///
    /// # Errors
    /// [`ErroClientes::UfInvalida`], [`ErroClientes::CepInvalido`].
    #[allow(clippy::too_many_arguments)]
    pub fn novo(
        pessoa: Id,
        tipo: TipoEndereco,
        logradouro: impl Into<String>,
        numero: impl Into<String>,
        bairro: impl Into<String>,
        cidade: impl Into<String>,
        uf: &str,
        cep: &str,
    ) -> Result<Self, ErroClientes> {
        let uf = Uf::de_sigla(uf).map_err(|_| ErroClientes::UfInvalida)?;
        let cep = texto::somente_digitos(cep);
        if cep.len() != 8 {
            return Err(ErroClientes::CepInvalido);
        }
        Ok(Self {
            id: Id::novo(),
            pessoa,
            tipo,
            logradouro: logradouro.into(),
            numero: numero.into(),
            complemento: None,
            bairro: bairro.into(),
            cidade: cidade.into(),
            uf,
            cep,
            principal: false,
        })
    }
}

/// O tipo de um [`Contato`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoContato {
    /// Telefone fixo.
    Telefone,
    /// Celular.
    Celular,
    /// E-mail.
    Email,
    /// Número de `WhatsApp`.
    Whatsapp,
}

/// Um contato de uma pessoa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contato {
    /// Identidade.
    pub id: Id,
    /// A pessoa.
    pub pessoa: Id,
    /// O tipo.
    pub tipo: TipoContato,
    /// O valor, normalizado (só dígitos para telefone; minúsculas para e-mail).
    pub valor: String,
    /// Se é o contato principal.
    pub principal: bool,
}

impl Contato {
    /// Cria um contato validando o formato conforme o tipo.
    ///
    /// # Errors
    /// [`ErroClientes::ContatoInvalido`] com a causa.
    pub fn novo(pessoa: Id, tipo: TipoContato, valor: &str) -> Result<Self, ErroClientes> {
        let valor = match tipo {
            TipoContato::Email => {
                let v = valor.trim().to_lowercase();
                let bytes = v.as_bytes();
                let arroba = v.find('@');
                let ok = arroba.is_some_and(|i| {
                    i > 0
                        && v[i + 1..].contains('.')
                        && !v.ends_with('.')
                        && !v.contains(' ')
                        && bytes.last() != Some(&b'@')
                });
                if !ok {
                    return Err(ErroClientes::ContatoInvalido("e-mail em formato inválido"));
                }
                v
            }
            TipoContato::Telefone | TipoContato::Celular | TipoContato::Whatsapp => {
                let d = texto::somente_digitos(valor);
                if !(10..=13).contains(&d.len()) {
                    return Err(ErroClientes::ContatoInvalido(
                        "telefone precisa de 10 a 13 dígitos (com DDD)",
                    ));
                }
                d
            }
        };
        Ok(Self {
            id: Id::novo(),
            pessoa,
            tipo,
            valor,
            principal: false,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn cnpj_valido_e_normalizado() {
        let d = DocumentoPessoa::novo(
            Id::novo(),
            Id::novo(),
            TipoDocumento::Cnpj,
            "11.222.333/0001-81",
        )
        .unwrap();
        assert_eq!(d.numero, "11222333000181");
    }

    #[test]
    fn cpf_invalido_e_recusado() {
        let erro =
            DocumentoPessoa::novo(Id::novo(), Id::novo(), TipoDocumento::Cpf, "111.111.111-11")
                .unwrap_err();
        assert!(matches!(erro, ErroClientes::DocumentoInvalido(_)));
    }

    #[test]
    fn numero_de_cpf_num_campo_de_cnpj_e_recusado() {
        // "529.982.247-25" é um CPF válido; declarado como CNPJ, deve falhar.
        let erro = DocumentoPessoa::novo(
            Id::novo(),
            Id::novo(),
            TipoDocumento::Cnpj,
            "529.982.247-25",
        )
        .unwrap_err();
        assert!(matches!(erro, ErroClientes::DocumentoInvalido(_)));
    }

    #[test]
    fn rg_aceita_texto_livre_nao_vazio() {
        assert!(
            DocumentoPessoa::novo(Id::novo(), Id::novo(), TipoDocumento::Rg, "MG-12.345").is_ok()
        );
        assert!(DocumentoPessoa::novo(Id::novo(), Id::novo(), TipoDocumento::Rg, "   ").is_err());
    }

    #[test]
    fn endereco_valida_uf_e_cep() {
        assert!(Endereco::novo(
            Id::novo(),
            TipoEndereco::Comercial,
            "Rua A",
            "100",
            "Centro",
            "Belo Horizonte",
            "MG",
            "30110-010",
        )
        .is_ok());
        assert_eq!(
            Endereco::novo(
                Id::novo(),
                TipoEndereco::Comercial,
                "Rua A",
                "100",
                "Centro",
                "Cidade",
                "XX",
                "30110010",
            )
            .unwrap_err(),
            ErroClientes::UfInvalida
        );
        assert_eq!(
            Endereco::novo(
                Id::novo(),
                TipoEndereco::Comercial,
                "Rua A",
                "100",
                "Centro",
                "Cidade",
                "MG",
                "123",
            )
            .unwrap_err(),
            ErroClientes::CepInvalido
        );
    }

    #[test]
    fn contato_valida_email_e_telefone() {
        assert_eq!(
            Contato::novo(Id::novo(), TipoContato::Email, " Fulano@Loja.com ")
                .unwrap()
                .valor,
            "fulano@loja.com"
        );
        assert!(Contato::novo(Id::novo(), TipoContato::Email, "sem-arroba").is_err());
        assert!(Contato::novo(Id::novo(), TipoContato::Email, "a@b").is_err());

        assert_eq!(
            Contato::novo(Id::novo(), TipoContato::Celular, "(31) 98888-7777")
                .unwrap()
                .valor,
            "31988887777"
        );
        assert!(Contato::novo(Id::novo(), TipoContato::Telefone, "123").is_err());
    }
}
