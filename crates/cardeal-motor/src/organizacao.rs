//! A organização: os CNPJs de uma base (ADR-0017). Uma base nasce com a matriz no primeiro
//! acesso; outros CNPJs entram por [`MotorLocal::adicionar_empresa`], cada um com o próprio
//! plano de contas e o próprio papel Administrador.

use cardeal_auth::{Papel, RepositorioAuth};
use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
use cardeal_ledger::semear_plano_padrao;
use cardeal_storage::{Armazenamento, ContextoEscrita, ErroArmazenamento};

use crate::acesso::{autenticar_usuario, sessao_de_usuario_ativo};
use crate::motor::{MotorLocal, SessaoLocal};
use crate::{como_erro_armazenamento, erro_armazenamento};

/// Quem pode cadastrar um CNPJ novo na organização.
pub const PERMISSAO_ORGANIZACAO: &str = "empresa.organizacao.gerenciar";

/// Um CNPJ da organização.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmpresaDaOrganizacao {
    /// O id da empresa.
    pub id: Id,
    /// Razão social.
    pub razao_social: String,
    /// CNPJ como cadastrado.
    pub cnpj: String,
    /// Verdadeiro para a matriz (a empresa principal da base).
    pub matriz: bool,
}

fn sqlite(e: &rusqlite::Error) -> ErroArmazenamento {
    ErroArmazenamento::Sqlite(e.to_string())
}

fn id_de(bytes: &[u8]) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

/// Os ids de todos os CNPJs da base, matriz primeiro.
pub(crate) fn ids_das_empresas(arm: &Armazenamento) -> Resultado<Vec<Id>> {
    arm.leitor()
        .consultar(|c| {
            let mut stmt = c
                .prepare("SELECT id FROM nucleo_empresa ORDER BY matriz IS NOT NULL, rowid")
                .map_err(|e| sqlite(&e))?;
            let ids = stmt
                .query_map([], |r| r.get::<_, Vec<u8>>(0))
                .map_err(|e| sqlite(&e))?
                .map(|b| b.map(|b| id_de(&b)))
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| sqlite(&e));
            ids
        })
        .map_err(erro_armazenamento)
}

impl MotorLocal {
    /// Os CNPJs da organização, matriz primeiro.
    ///
    /// # Errors
    /// Erro de leitura.
    pub fn empresas(&self) -> Resultado<Vec<EmpresaDaOrganizacao>> {
        self.arm
            .leitor()
            .consultar(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT id, razao_social, cnpj, matriz IS NULL FROM nucleo_empresa
                         ORDER BY matriz IS NOT NULL, rowid",
                    )
                    .map_err(|e| sqlite(&e))?;
                let lista = stmt
                    .query_map([], |r| {
                        Ok(EmpresaDaOrganizacao {
                            id: id_de(&r.get::<_, Vec<u8>>(0)?),
                            razao_social: r.get(1)?,
                            cnpj: r.get(2)?,
                            matriz: r.get(3)?,
                        })
                    })
                    .map_err(|e| sqlite(&e))?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| sqlite(&e));
                lista
            })
            .map_err(erro_armazenamento)
    }

    fn exigir_empresa(&self, empresa: Id) -> Resultado<()> {
        if empresa == self.empresa || ids_das_empresas(&self.arm)?.contains(&empresa) {
            Ok(())
        } else {
            Err(Erro::novo(
                CodigoErro::NAO_ENCONTRADO,
                "esta empresa não pertence a esta organização",
            ))
        }
    }

    /// Como [`Self::autenticar`], numa empresa específica da organização.
    ///
    /// # Errors
    /// Empresa de outra organização; credencial inválida; usuário sem papel na empresa.
    pub fn autenticar_em(&self, empresa: Id, login: &str, senha: &str) -> Resultado<SessaoLocal> {
        self.exigir_empresa(empresa)?;
        let sessao = autenticar_usuario(&self.arm, empresa, login, senha)?;
        sem_acesso_se_vazia(SessaoLocal { sessao })
    }

    /// Como [`Self::sessao_do_usuario`], numa empresa específica da organização.
    ///
    /// # Errors
    /// Empresa de outra organização; usuário inexistente/inativo; sem papel na empresa.
    pub fn sessao_do_usuario_em(
        &self,
        empresa: Id,
        usuario: Id,
        dispositivo: Id,
    ) -> Resultado<SessaoLocal> {
        self.exigir_empresa(empresa)?;
        let sessao = sessao_de_usuario_ativo(&self.arm, empresa, usuario, dispositivo)?;
        sem_acesso_se_vazia(SessaoLocal { sessao })
    }

    /// Cadastra outro CNPJ na organização: plano de contas padrão próprio, papel
    /// Administrador com o catálogo inteiro, atribuído a quem pediu. Exige
    /// [`PERMISSAO_ORGANIZACAO`] na empresa da sessão. Devolve o id da empresa nova.
    ///
    /// # Errors
    /// Sem permissão; CNPJ já cadastrado na organização; erro de escrita.
    pub fn adicionar_empresa(
        &self,
        sessao: &SessaoLocal,
        razao_social: &str,
        cnpj: &str,
    ) -> Resultado<Id> {
        if !sessao.concede(PERMISSAO_ORGANIZACAO) {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "sem permissão para cadastrar outro CNPJ na organização",
            ));
        }
        let razao_social = razao_social.trim().to_owned();
        let cnpj = cnpj.trim().to_owned();
        if razao_social.is_empty() || cnpj.is_empty() {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "informe a razão social e o CNPJ",
            ));
        }
        let so_digitos = |s: &str| s.chars().filter(char::is_ascii_digit).collect::<String>();
        if self
            .empresas()?
            .iter()
            .any(|e| so_digitos(&e.cnpj) == so_digitos(&cnpj))
        {
            return Err(Erro::novo(
                CodigoErro::DUPLICADO,
                "este CNPJ já está cadastrado na organização",
            ));
        }
        let (matriz, admin) = (self.empresa, sessao.usuario());
        let empresa = Id::novo();
        let permissoes: Vec<String> = self
            .plano
            .conjunto
            .permissoes
            .iter()
            .map(|p| (*p).to_string())
            .collect();
        let ctx = ContextoEscrita::novo(empresa, admin, self.dispositivo, Id::novo());
        self.arm
            .escritor()
            .executar(ctx, move |uow| {
                uow.conexao()
                    .execute(
                        "INSERT INTO nucleo_empresa
                           (id, razao_social, nome_fantasia, cnpj, regime, matriz, endereco, perfil, criado_em)
                         SELECT ?1, ?2, ?2, ?3, regime, ?4, '{}', perfil, 0
                         FROM nucleo_empresa WHERE id = ?4",
                        rusqlite::params![
                            empresa.em_bytes().as_slice(),
                            razao_social,
                            cnpj,
                            matriz.em_bytes().as_slice()
                        ],
                    )
                    .map_err(|e| sqlite(&e))?;
                semear_plano_padrao(uow, empresa).map_err(|e| como_erro_armazenamento(&e))?;
                let mut papel = Papel::novo(empresa, "Administrador");
                for p in permissoes {
                    papel = papel.com_permissao(p);
                }
                let mut repo = RepositorioAuth::novo(uow);
                repo.inserir_papel(&papel)
                    .map_err(|e| como_erro_armazenamento(&e))?;
                repo.atribuir_papel(admin, papel.id, empresa)
                    .map_err(|e| como_erro_armazenamento(&e))?;
                Ok(())
            })
            .map_err(erro_armazenamento)?;
        Ok(empresa)
    }
}

/// Uma sessão sem nenhuma permissão é "sem acesso a esta empresa" — melhor dizer na entrada
/// do que deixar cada tela recusar.
fn sem_acesso_se_vazia(s: SessaoLocal) -> Resultado<SessaoLocal> {
    if s.permissoes().next().is_none() {
        return Err(Erro::novo(
            CodigoErro::SEM_PERMISSAO,
            "este usuário não tem acesso a esta empresa",
        ));
    }
    Ok(s)
}
