//! [`MotorLocal`]: abertura, primeiro acesso, autenticação e despacho.

use std::path::Path;
use std::sync::Arc;

use cardeal_auth::{Papel, PoliticaSenha, RepositorioAuth, Sessao, Usuario};
use cardeal_kernel::{ChaveIdempotencia, CodigoErro, Erro, Fuso, Id, Instante, Resultado};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{Ambiente, Comando, Consulta, Modulo, PedidoAtivacao};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use rusqlite::OptionalExtension;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::acesso::{autenticar_usuario, sessao_de_usuario_ativo, sincronizar_papel_admin};
use crate::plano::Plano;
use crate::{como_erro_armazenamento, erro_armazenamento};

/// O motor rodando embutido no processo do desktop: armazenamento, despacho e o conjunto
/// efetivo de módulos de uma única empresa.
pub struct MotorLocal {
    pub(crate) arm: Armazenamento,
    pub(crate) plano: Arc<Plano>,
    pub(crate) ambiente: Ambiente,
    pub(crate) empresa: Id,
    pub(crate) dispositivo: Id,
}

/// Uma sessão autenticada — o que uma tela guarda depois do login para chamar `executar`/
/// `consultar`.
pub struct SessaoLocal {
    sessao: Sessao,
}

impl SessaoLocal {
    /// O id do usuário autenticado.
    #[must_use]
    pub const fn usuario(&self) -> Id {
        self.sessao.usuario
    }

    /// Se a sessão concede esta permissão — para conferir um supervisor que se identificou por
    /// cima do operador (`docs/modulos/pdv.md` §11 regra 3) antes de agir em nome dele.
    #[must_use]
    pub fn concede(&self, permissao: &str) -> bool {
        self.sessao.autorizacoes().concede(permissao)
    }

    /// Todas as permissões concedidas, em ordem.
    pub fn permissoes(&self) -> impl Iterator<Item = &str> {
        self.sessao.autorizacoes().iter()
    }
}

impl MotorLocal {
    /// Abre (criando se preciso) o arquivo local, migra o núcleo + o razão + os módulos
    /// dados, e resolve o conjunto efetivo da empresa a partir do pedido de ativação.
    ///
    /// A empresa é lida de `nucleo_empresa` se já existir (base reaberta numa sessão
    /// posterior) ou gerada agora — só é persistida de fato quando [`Self::configurar_inicial`]
    /// gravar a linha. Monoposto: uma base só tem uma empresa.
    ///
    /// # Errors
    /// Erro de abertura/migração do armazenamento, ou manifesto/dependência inválida ao
    /// resolver o conjunto efetivo.
    pub fn abrir(
        caminho: &Path,
        modulos: &[&dyn Modulo],
        pedido: &PedidoAtivacao,
    ) -> Resultado<Self> {
        Self::abrir_com(ConfigArmazenamento::arquivo(caminho), modulos, pedido)
    }

    /// Como [`Self::abrir`], com a configuração de armazenamento explícita.
    ///
    /// # Errors
    /// Como [`Self::abrir`].
    pub fn abrir_com(
        cfg: ConfigArmazenamento,
        modulos: &[&dyn Modulo],
        pedido: &PedidoAtivacao,
    ) -> Resultado<Self> {
        Self::abrir_com_plano(cfg, &Plano::preparar(modulos, pedido)?)
    }

    /// Abre uma base sobre um [`Plano`] já preparado — o caminho do servidor multi-tenant, que
    /// prepara o plano uma vez e abre cada empresa com [`ConfigArmazenamento::servidor`]
    /// (ADR-0016).
    ///
    /// # Errors
    /// Erro de abertura/migração do armazenamento.
    pub fn abrir_com_plano(cfg: ConfigArmazenamento, plano: &Arc<Plano>) -> Resultado<Self> {
        let arm = Armazenamento::abrir(cfg).map_err(erro_armazenamento)?;
        arm.migrar(&plano.migracoes).map_err(erro_armazenamento)?;

        let empresa_persistida = arm
            .leitor()
            .consultar(|c| {
                c.query_row("SELECT id FROM nucleo_empresa LIMIT 1", [], |r| {
                    r.get::<_, Vec<u8>>(0)
                })
                .optional()
                .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)?
            .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
            .map(Id::de_bytes);
        let empresa = empresa_persistida.unwrap_or_else(Id::novo);

        // Auto-heal: uma base criada numa versão anterior tem o papel "Administrador" com um
        // catálogo de permissões defasado — o admin fica travado fora das telas novas com
        // "Sem permissão para ...". A cada abertura, completa esse papel com o que faltar do
        // catálogo atual dos módulos ativos (só adiciona; nunca remove).
        if empresa_persistida.is_some() {
            sincronizar_papel_admin(&arm, empresa, &plano.conjunto)?;
        }

        Ok(Self {
            arm,
            ambiente: ambiente_de(empresa, plano),
            plano: Arc::clone(plano),
            empresa,
            dispositivo: Id::novo(),
        })
    }

    /// Verdadeiro se a base é nova — nenhuma empresa cadastrada ainda, então a tela deve
    /// oferecer o assistente de primeiro acesso em vez do login.
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn precisa_de_configuracao_inicial(&self) -> Resultado<bool> {
        let total: i64 = self
            .arm
            .leitor()
            .consultar(|c| {
                c.query_row("SELECT COUNT(*) FROM nucleo_empresa", [], |r| r.get(0))
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)?;
        Ok(total == 0)
    }

    /// O primeiro acesso: cadastra a empresa, semeia o plano de contas padrão, e cria o
    /// administrador com todas as permissões que o conjunto efetivo já resolveu. Devolve o id
    /// do administrador criado.
    ///
    /// # Errors
    /// Erro de escrita, ou [`cardeal_auth::ErroAuth`] se a senha viola a política.
    pub fn configurar_inicial(
        &self,
        razao_social: &str,
        cnpj: &str,
        admin_login: &str,
        admin_nome: &str,
        admin_senha: &str,
    ) -> Resultado<Id> {
        let empresa = self.empresa;
        let permissoes: Vec<String> = self
            .plano
            .conjunto
            .permissoes
            .iter()
            .map(|p| (*p).to_string())
            .collect();
        let razao_social = razao_social.to_string();
        let cnpj = cnpj.to_string();
        let admin_login = admin_login.to_string();
        let admin_nome = admin_nome.to_string();
        let admin_senha = admin_senha.to_string();

        let ctx = ContextoEscrita::novo(empresa, Id::novo(), self.dispositivo, Id::novo());
        self.arm
            .escritor()
            .executar(ctx, move |uow| {
                uow.conexao()
                    .execute(
                        "INSERT INTO nucleo_empresa
                           (id, razao_social, nome_fantasia, cnpj, regime, endereco, perfil, criado_em)
                         VALUES (?1,?2,?2,?3,'SimplesNacional','{}','comercio',0)",
                        rusqlite::params![empresa.em_bytes().as_slice(), razao_social, cnpj],
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                semear_plano_padrao(uow, empresa).map_err(|e| como_erro_armazenamento(&e))?;

                let agora = uow.agora();
                let mut papel = Papel::novo(empresa, "Administrador");
                for p in permissoes {
                    papel = papel.com_permissao(p);
                }
                let usuario = Usuario::novo(admin_login, admin_nome, &admin_senha, PoliticaSenha::PADRAO, agora)
                    .map_err(|e| como_erro_armazenamento(&e))?;

                let mut repo = RepositorioAuth::novo(uow);
                repo.inserir_papel(&papel).map_err(|e| como_erro_armazenamento(&e))?;
                repo.inserir_usuario(&usuario).map_err(|e| como_erro_armazenamento(&e))?;
                repo.atribuir_papel(usuario.id, papel.id, empresa).map_err(|e| como_erro_armazenamento(&e))?;
                Ok(usuario.id)
            })
            .map_err(erro_armazenamento)
            .map(|c| c.valor)
    }

    /// Autentica um login/senha e monta a sessão, cruzando os papéis do usuário com o
    /// conjunto efetivo da empresa — o item que faltava para o login rodar fora de
    /// `cardeal-server` (`docs/19-estado-e-processo.md` §4, item f).
    ///
    /// # Errors
    /// [`cardeal_auth::ErroAuth`] em credencial inválida, conta inativa ou bloqueada.
    pub fn autenticar(&self, login: &str, senha: &str) -> Resultado<SessaoLocal> {
        let sessao = autenticar_usuario(&self.arm, self.empresa, login, senha)?;
        Ok(SessaoLocal { sessao })
    }

    /// A sessão de um usuário **já autenticado por outro meio** — a conta do servidor
    /// multi-tenant (ADR-0016), cuja senha mora no diretório, não nesta base. Confere que o
    /// usuário existe e está ativo, e cruza os papéis dele com o conjunto efetivo, exatamente
    /// como [`Self::autenticar`] faz depois de conferir a senha.
    ///
    /// # Errors
    /// `SESSAO_INVALIDA` se o usuário não existe ou está inativo; erro de leitura.
    pub fn sessao_do_usuario(&self, usuario: Id, dispositivo: Id) -> Resultado<SessaoLocal> {
        let sessao = sessao_de_usuario_ativo(&self.arm, self.empresa, usuario, dispositivo)?;
        Ok(SessaoLocal { sessao })
    }

    /// Fixa o id da empresa **antes** do primeiro acesso — o servidor multi-tenant escolhe o id
    /// (é o nome do arquivo e a chave do diretório) e a base tem de nascer com ele.
    ///
    /// # Errors
    /// `ESTADO_INVALIDO` se a base já tem empresa cadastrada; erro de leitura.
    pub fn definir_empresa_nova(&mut self, empresa: Id) -> Resultado<()> {
        if !self.precisa_de_configuracao_inicial()? {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "a base já tem empresa cadastrada — o id não pode mais mudar",
            ));
        }
        self.empresa = empresa;
        self.ambiente = ambiente_de(empresa, &self.plano);
        Ok(())
    }

    /// Apaga as respostas idempotentes gravadas antes de `antes_de` — reenvios depois disso
    /// executam de novo. Devolve quantas apagou.
    ///
    /// # Errors
    /// Erro de escrita.
    pub fn podar_idempotencia(&self, antes_de: Instante) -> Resultado<usize> {
        let ctx = ContextoEscrita::novo(self.empresa, Id::NULO, self.dispositivo, Id::novo());
        self.arm
            .escritor()
            .executar(ctx, move |uow| {
                uow.conexao()
                    .execute(
                        "DELETE FROM nucleo_idempotencia WHERE criado_em < ?1",
                        [antes_de.em_micros()],
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map(|c| c.valor)
            .map_err(erro_armazenamento)
    }

    /// A empresa desta base.
    #[must_use]
    pub const fn empresa(&self) -> Id {
        self.empresa
    }

    /// Executa um comando já serializado em `postcard`, com chave de idempotência, e devolve a
    /// saída também em `postcard` — o caminho da rede, onde os bytes chegam prontos e voltam
    /// sem passar por um tipo concreto (o servidor não conhece os módulos, só os nomes).
    ///
    /// # Errors
    /// Sem permissão, módulo inativo, chave reaproveitada ou erro de domínio do comando.
    pub fn executar_bruto(
        &self,
        sessao: &SessaoLocal,
        nome: &str,
        carga: &[u8],
        chave: Option<ChaveIdempotencia>,
    ) -> Resultado<Vec<u8>> {
        self.plano.despachante.executar_comando_idempotente(
            nome,
            carga,
            chave,
            &sessao.sessao,
            &self.ambiente,
            self.arm.escritor(),
        )
    }

    /// Executa uma consulta já serializada em `postcard` — par de [`Self::executar_bruto`].
    ///
    /// # Errors
    /// Sem permissão, módulo inativo ou erro de domínio da consulta.
    pub fn consultar_bruto(
        &self,
        sessao: &SessaoLocal,
        nome: &str,
        carga: &[u8],
    ) -> Resultado<Vec<u8>> {
        self.plano.despachante.executar_consulta(
            nome,
            carga,
            &sessao.sessao,
            &self.ambiente,
            self.arm.leitor(),
        )
    }

    /// Executa um comando na transação do escritor, autorizado pela sessão.
    ///
    /// # Errors
    /// Sem permissão, módulo inativo, ou erro de domínio do próprio comando.
    pub fn executar<C>(&self, sessao: &SessaoLocal, nome: &str, comando: &C) -> Resultado<C::Saida>
    where
        C: Comando + Serialize,
        C::Saida: DeserializeOwned,
    {
        let carga = postcard::to_stdvec(comando)
            .map_err(|e| Erro::novo(CodigoErro::VALOR_INVALIDO, e.to_string()))?;
        let saida = self.plano.despachante.executar_comando(
            nome,
            &carga,
            &sessao.sessao,
            &self.ambiente,
            self.arm.escritor(),
        )?;
        postcard::from_bytes(&saida)
            .map_err(|e| Erro::novo(CodigoErro::VALOR_INVALIDO, e.to_string()))
    }

    /// Executa uma consulta sobre o pool de leitura, autorizada pela sessão.
    ///
    /// # Errors
    /// Sem permissão, módulo inativo, ou erro de domínio da própria consulta.
    pub fn consultar<Q>(
        &self,
        sessao: &SessaoLocal,
        nome: &str,
        consulta: &Q,
    ) -> Resultado<Q::Saida>
    where
        Q: Consulta + Serialize,
        Q::Saida: DeserializeOwned,
    {
        let carga = postcard::to_stdvec(consulta)
            .map_err(|e| Erro::novo(CodigoErro::VALOR_INVALIDO, e.to_string()))?;
        let saida = self.plano.despachante.executar_consulta(
            nome,
            &carga,
            &sessao.sessao,
            &self.ambiente,
            self.arm.leitor(),
        )?;
        postcard::from_bytes(&saida)
            .map_err(|e| Erro::novo(CodigoErro::VALOR_INVALIDO, e.to_string()))
    }
}

fn ambiente_de(empresa: Id, plano: &Plano) -> Ambiente {
    Ambiente::compartilhado(empresa, Arc::clone(&plano.conjunto)).com_fuso(Fuso::BRASILIA)
}
