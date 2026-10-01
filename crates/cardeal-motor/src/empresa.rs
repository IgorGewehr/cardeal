//! Dados cadastrais e identidade visual da empresa — `nucleo_empresa`/`nucleo_configuracao`.
//!
//! `nucleo` não é um módulo com `Comando`/`Consulta` no despacho; a tela de Configurações fala
//! com ele direto pelo motor, como já fazem `configurar_inicial` e `autenticar`.

use base64::Engine as _;
use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_storage::{ContextoEscrita, ErroArmazenamento};

use crate::erro_armazenamento;
use crate::motor::MotorLocal;

/// O engine base64 usado para guardar a logo da empresa em `nucleo_configuracao`.
const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

impl MotorLocal {
    // `nucleo`/`auth` não são módulos com `Comando`/`Consulta` no despacho; a tela de
    // Configurações fala com eles direto pelo motor, como já fazem `configurar_inicial` e
    // `autenticar`.

    /// Os dados cadastrais da empresa.
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn empresa_resumo(&self) -> Resultado<EmpresaResumo> {
        self.arm
            .leitor()
            .consultar(|c| {
                c.query_row(
                    "SELECT razao_social, nome_fantasia, cnpj, regime, perfil
                     FROM nucleo_empresa LIMIT 1",
                    [],
                    |r| {
                        Ok(EmpresaResumo {
                            razao_social: r.get(0)?,
                            nome_fantasia: r.get(1)?,
                            cnpj: r.get(2)?,
                            regime: r.get(3)?,
                            perfil: r.get(4)?,
                        })
                    },
                )
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)
    }

    /// Atualiza razão social, nome fantasia e regime tributário da empresa.
    ///
    /// # Errors
    /// Erro de escrita do armazenamento.
    pub fn atualizar_empresa(
        &self,
        razao_social: &str,
        nome_fantasia: &str,
        regime: &str,
    ) -> Resultado<()> {
        let (razao_social, nome_fantasia, regime) = (
            razao_social.to_string(),
            nome_fantasia.to_string(),
            regime.to_string(),
        );
        let ctx = ContextoEscrita::novo(self.empresa, Id::novo(), self.dispositivo, Id::novo());
        self.arm
            .escritor()
            .executar(ctx, move |uow| {
                uow.conexao()
                    .execute(
                        "UPDATE nucleo_empresa
                         SET razao_social = ?1, nome_fantasia = ?2, regime = ?3",
                        rusqlite::params![razao_social, nome_fantasia, regime],
                    )
                    .map(|_| ())
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)
            .map(|_| ())
    }

    /// A identidade visual da empresa para documentos (orçamento em PDF etc.): dados de
    /// `nucleo_empresa` + as chaves `empresa.*` de `nucleo_configuracao` (telefone, e-mail,
    /// site, endereço de exibição e a logo em base64).
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn identidade_visual(&self) -> Resultado<IdentidadeVisual> {
        let base = self.empresa_resumo()?;
        let empresa = self.empresa;
        let config: std::collections::HashMap<String, String> = self
            .arm
            .leitor()
            .consultar(move |c| {
                let mut stmt = c
                    .prepare(
                        "SELECT chave, valor FROM nucleo_configuracao
                         WHERE empresa = ?1 AND chave LIKE 'empresa.%'",
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                let linhas = stmt
                    .query_map([empresa.em_bytes().as_slice()], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                linhas
                    .collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)?;

        let get = |k: &str| config.get(k).cloned().unwrap_or_default();
        let logo_png = config
            .get("empresa.logo_png_b64")
            .and_then(|b64| BASE64.decode(b64).ok())
            .filter(|v| !v.is_empty());

        Ok(IdentidadeVisual {
            razao_social: base.razao_social,
            nome_fantasia: base.nome_fantasia,
            cnpj: base.cnpj,
            endereco: get("empresa.endereco_exibicao"),
            telefone: get("empresa.telefone"),
            email: get("empresa.email"),
            site: get("empresa.site"),
            logo_png,
        })
    }

    /// Grava telefone, e-mail, site e endereço de exibição da empresa (chaves
    /// `nucleo_configuracao`).
    ///
    /// # Errors
    /// Erro de escrita do armazenamento.
    pub fn definir_contato_empresa(
        &self,
        telefone: &str,
        email: &str,
        site: &str,
        endereco: &str,
    ) -> Resultado<()> {
        let pares = [
            ("empresa.telefone", telefone.trim().to_string()),
            ("empresa.email", email.trim().to_string()),
            ("empresa.site", site.trim().to_string()),
            ("empresa.endereco_exibicao", endereco.trim().to_string()),
        ];
        self.gravar_config(pares.into_iter().collect())
    }

    /// Grava (ou remove, com `None`) a logo da empresa. Valida assinatura PNG e teto de
    /// tamanho (512 KB).
    ///
    /// # Errors
    /// [`CodigoErro::VALOR_INVALIDO`] se não é um PNG ou passa do teto; erro de escrita.
    pub fn definir_logo_empresa(&self, png: Option<&[u8]>) -> Resultado<()> {
        let empresa = self.empresa;
        let valor = match png {
            None => None,
            Some(bytes) => {
                const TETO: usize = 512 * 1024;
                if bytes.len() > TETO {
                    return Err(Erro::novo(
                        CodigoErro::VALOR_INVALIDO,
                        "a logo passa de 512 KB — use uma imagem menor",
                    ));
                }
                if !bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
                    return Err(Erro::novo(
                        CodigoErro::VALOR_INVALIDO,
                        "a logo precisa ser um arquivo PNG",
                    ));
                }
                Some(BASE64.encode(bytes))
            }
        };
        let ctx = ContextoEscrita::novo(empresa, Id::novo(), self.dispositivo, Id::novo());
        self.arm
            .escritor()
            .executar(ctx, move |uow| {
                match valor {
                    Some(b64) => uow.conexao().execute(
                        "INSERT INTO nucleo_configuracao (empresa, chave, valor)
                         VALUES (?1, 'empresa.logo_png_b64', ?2)
                         ON CONFLICT(empresa, chave) DO UPDATE SET valor = excluded.valor",
                        rusqlite::params![empresa.em_bytes().as_slice(), b64],
                    ),
                    None => uow.conexao().execute(
                        "DELETE FROM nucleo_configuracao
                         WHERE empresa = ?1 AND chave = 'empresa.logo_png_b64'",
                        [empresa.em_bytes().as_slice()],
                    ),
                }
                .map(|_| ())
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)
            .map(|_| ())
    }

    fn gravar_config(&self, pares: Vec<(&'static str, String)>) -> Resultado<()> {
        let empresa = self.empresa;
        let ctx = ContextoEscrita::novo(empresa, Id::novo(), self.dispositivo, Id::novo());
        self.arm
            .escritor()
            .executar(ctx, move |uow| {
                for (chave, valor) in &pares {
                    uow.conexao()
                        .execute(
                            "INSERT INTO nucleo_configuracao (empresa, chave, valor)
                             VALUES (?1, ?2, ?3)
                             ON CONFLICT(empresa, chave) DO UPDATE SET valor = excluded.valor",
                            rusqlite::params![empresa.em_bytes().as_slice(), chave, valor],
                        )
                        .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                }
                Ok(())
            })
            .map_err(erro_armazenamento)
            .map(|_| ())
    }
}

/// Dados cadastrais da empresa, para a tela de Configurações.
#[derive(Debug, Clone)]
pub struct EmpresaResumo {
    /// Razão social.
    pub razao_social: String,
    /// Nome fantasia.
    pub nome_fantasia: String,
    /// CNPJ (não editável pela tela — muda por processo formal).
    pub cnpj: String,
    /// Regime tributário (`MEI`/`SimplesNacional`/`LucroPresumido`/`LucroReal`).
    pub regime: String,
    /// Perfil de operação.
    pub perfil: String,
}

/// A identidade visual da empresa para documentos (orçamento em PDF etc.).
#[derive(Debug, Clone, Default)]
pub struct IdentidadeVisual {
    /// Razão social.
    pub razao_social: String,
    /// Nome fantasia.
    pub nome_fantasia: String,
    /// CNPJ.
    pub cnpj: String,
    /// Endereço de exibição (uma linha).
    pub endereco: String,
    /// Telefone.
    pub telefone: String,
    /// E-mail.
    pub email: String,
    /// Site.
    pub site: String,
    /// Bytes do PNG da logo, se cadastrada.
    pub logo_png: Option<Vec<u8>>,
}
