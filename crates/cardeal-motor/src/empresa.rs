//! A própria empresa e seus usuários, pelo motor: atalhos sobre o SQL do `mod-empresa`.
//!
//! O caminho canônico é o despacho (`empresa.dados.v1`, `empresa.definir_logo.v1`…), que é o
//! que o cliente remoto usa. Estes métodos existem para o monoposto chamar sem sessão — o
//! assistente de primeiro acesso e o cabeçalho de PDF — e executam **o mesmo SQL**
//! (`mod_empresa::sql`), sem cópia.

use cardeal_kernel::{Id, Resultado};
use cardeal_storage::{ContextoEscrita, ErroArmazenamento};
use mod_empresa::sql;
pub use mod_empresa::{EmpresaResumo, IdentidadeVisual, PapelResumo, UsuarioResumo};
use rusqlite::Connection;

use crate::erro_armazenamento;
use crate::motor::MotorLocal;

impl MotorLocal {
    fn ler<T: Send + 'static>(
        &self,
        f: impl FnOnce(&Connection) -> Resultado<T> + Send + 'static,
    ) -> Resultado<T> {
        self.arm
            .leitor()
            .consultar(move |c| f(c).map_err(ErroArmazenamento::Dominio))
            .map_err(erro_armazenamento)
    }

    fn gravar(
        &self,
        f: impl FnOnce(&Connection, Id) -> Resultado<()> + Send + 'static,
    ) -> Resultado<()> {
        let empresa = self.empresa;
        let ctx = ContextoEscrita::novo(empresa, Id::novo(), self.dispositivo, Id::novo());
        self.arm
            .escritor()
            .executar(ctx, move |uow| {
                f(uow.conexao(), empresa).map_err(ErroArmazenamento::Dominio)
            })
            .map(|_| ())
            .map_err(erro_armazenamento)
    }

    /// Os dados cadastrais da empresa.
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn empresa_resumo(&self) -> Resultado<EmpresaResumo> {
        let empresa = self.empresa;
        self.ler(move |c| sql::dados(c, empresa))
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
        let (r, n, g) = (
            razao_social.to_owned(),
            nome_fantasia.to_owned(),
            regime.to_owned(),
        );
        self.gravar(move |c, empresa| sql::atualizar_dados(c, empresa, &r, &n, &g))
    }

    /// A identidade visual da empresa para documentos (orçamento em PDF etc.).
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn identidade_visual(&self) -> Resultado<IdentidadeVisual> {
        let empresa = self.empresa;
        self.ler(move |c| sql::identidade(c, empresa))
    }

    /// Grava telefone, e-mail, site e endereço de exibição da empresa.
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
        let valores = [
            telefone.trim().to_owned(),
            email.trim().to_owned(),
            site.trim().to_owned(),
            endereco.trim().to_owned(),
        ];
        self.gravar(move |c, empresa| {
            sql::gravar_config(
                c,
                empresa,
                &[
                    ("empresa.telefone", &valores[0]),
                    ("empresa.email", &valores[1]),
                    ("empresa.site", &valores[2]),
                    ("empresa.endereco_exibicao", &valores[3]),
                ],
            )
        })
    }

    /// Grava (ou remove, com `None`) a logo da empresa. Valida assinatura PNG e teto de
    /// tamanho (512 KB).
    ///
    /// # Errors
    /// `VALOR_INVALIDO` se não é um PNG ou passa do teto; erro de escrita.
    pub fn definir_logo_empresa(&self, png: Option<&[u8]>) -> Resultado<()> {
        if let Some(bytes) = png {
            sql::validar_logo(bytes)?;
        }
        let png = png.map(<[u8]>::to_vec);
        self.gravar(move |c, empresa| sql::gravar_logo(c, empresa, png.as_deref()))
    }

    /// Todos os usuários cadastrados (id, login, nome, ativo).
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn usuarios(&self) -> Resultado<Vec<UsuarioResumo>> {
        let empresa = self.empresa;
        self.ler(move |c| sql::usuarios(c, empresa))
    }

    /// Os papéis da empresa (nome, descrição, nº de permissões).
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn papeis(&self) -> Resultado<Vec<PapelResumo>> {
        let empresa = self.empresa;
        self.ler(move |c| sql::papeis(c, empresa))
    }
}
