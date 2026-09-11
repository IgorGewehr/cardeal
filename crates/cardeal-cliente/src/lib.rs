//! # cardeal-cliente
//!
//! O `MotorLocal`: a ponte entre uma UI (hoje `cardeal-desktop`) e o motor do Cardeal **sem
//! processo separado nem socket** — o modo monoposto de `docs/adr/0009-transporte-in-process.md`.
//!
//! Decisão pragmática desta fase (anotada, não uma reversão da ADR): em vez do caminho "zero
//! serialização" que a ADR descreve como ideal, este crate chama exatamente
//! `Despachante::executar_comando`/`executar_consulta` (que já serializam com `postcard`
//! internamente) — sem socket, no mesmo processo. Microssegundos de serialização são
//! irrelevantes para uma ferramenta de balcão, e um único caminho de serialização reduz o
//! risco de divergência que a própria ADR-0009 aponta como contra de manter dois caminhos.
//!
//! `MotorLocal::autenticar` é o item (f) de `docs/19-estado-e-processo.md` §4 que faltava —
//! montar `AutorizacoesEfetivas` a partir dos papéis do usuário + `ConjuntoEfetivo` da empresa
//! e emitir uma `Sessao` — só que aqui em vez de em `cardeal-server`, porque este é o caminho
//! monoposto sem HTTP.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]
#![allow(clippy::result_large_err)] // `Erro`/`ErroArmazenamento` são grandes de propósito (detalhes ao usuário)

use std::collections::BTreeSet;
use std::path::Path;

use base64::Engine as _;
use cardeal_auth::{
    AutorizacoesEfetivas, EmissaoSessao, Escopo, Papel, PoliticaSenha, RepositorioAuth, Sessao,
    Usuario,
};
use cardeal_kernel::{CodigoErro, Erro, Fuso, Id, Instante, Resultado};
use cardeal_ledger::semear_plano_padrao;
use cardeal_modkit::{
    Ambiente, Comando, ConjuntoEfetivo, Consulta, Despachante, Modulo, PedidoAtivacao,
    RegistroModulos,
};
use cardeal_storage::{Armazenamento, ConfigArmazenamento, ContextoEscrita, ErroArmazenamento};
use rusqlite::OptionalExtension;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// O engine base64 usado para guardar a logo da empresa em `nucleo_configuracao`.
const BASE64: base64::engine::GeneralPurpose = base64::engine::general_purpose::STANDARD;

fn erro_armazenamento(e: ErroArmazenamento) -> Erro {
    match e {
        ErroArmazenamento::Dominio(e) => e,
        outro => Erro::de_dominio(&outro),
    }
}

fn erro_registro(e: &cardeal_modkit::ErroRegistro) -> Erro {
    Erro::novo(CodigoErro::ESTADO_INVALIDO, e.to_string())
}

/// Encaixa um erro de domínio qualquer (`ErroDominio`) no formato que `Escritor`/`Leitor`
/// esperam, para usar dentro de um fecho de `executar`/`consultar`.
fn como_erro_armazenamento<E: cardeal_kernel::ErroDominio>(e: &E) -> ErroArmazenamento {
    ErroArmazenamento::Dominio(Erro::de_dominio(e))
}

/// O motor rodando embutido no processo do desktop: armazenamento, despacho e o conjunto
/// efetivo de módulos de uma única empresa.
pub struct MotorLocal {
    arm: Armazenamento,
    despachante: Despachante,
    ambiente: Ambiente,
    conjunto: ConjuntoEfetivo,
    empresa: Id,
    dispositivo: Id,
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
        let arm = Armazenamento::abrir(ConfigArmazenamento::arquivo(caminho))
            .map_err(erro_armazenamento)?;

        let mut conjuntos = vec![cardeal_ledger::migracoes::conjunto()];
        conjuntos.extend(modulos.iter().map(|m| m.migracoes()));
        arm.migrar(&conjuntos).map_err(erro_armazenamento)?;

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

        let despachante = Despachante::construir(modulos)?;

        let mut registro = RegistroModulos::novo();
        for m in modulos {
            registro
                .registrar(m.manifesto())
                .map_err(|e| erro_registro(&e))?;
        }
        let conjunto = registro.resolver(pedido).map_err(|e| erro_registro(&e))?;

        // Auto-heal: uma base criada numa versão anterior tem o papel "Administrador" com um
        // catálogo de permissões defasado — o admin fica travado fora das telas novas com
        // "Sem permissão para ...". A cada abertura, completa esse papel com o que faltar do
        // catálogo atual dos módulos ativos (só adiciona; nunca remove).
        if empresa_persistida.is_some() {
            sincronizar_papel_admin(&arm, empresa, &conjunto)?;
        }

        let ambiente = Ambiente::novo(empresa, conjunto.clone()).com_fuso(Fuso::BRASILIA);

        Ok(Self {
            arm,
            despachante,
            ambiente,
            conjunto,
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
    /// administrador com todas as permissões que o conjunto efetivo já resolveu.
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
    ) -> Resultado<()> {
        let empresa = self.empresa;
        let permissoes: Vec<String> = self
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
                Ok(())
            })
            .map_err(erro_armazenamento)?;
        Ok(())
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
        let saida = self.despachante.executar_comando(
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
        let saida = self.despachante.executar_consulta(
            nome,
            &carga,
            &sessao.sessao,
            &self.ambiente,
            self.arm.leitor(),
        )?;
        postcard::from_bytes(&saida)
            .map_err(|e| Erro::novo(CodigoErro::VALOR_INVALIDO, e.to_string()))
    }

    // ── Configurações ────────────────────────────────────────────────────
    //
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

    /// Todos os usuários cadastrados (id, login, nome, ativo). `id` existe desde sempre em
    /// `nucleo_usuario` — só não era selecionado porque a tela de Configurações (única
    /// consumidora até aqui) não precisava dele; o comprovante de OS precisa para resolver
    /// `tecnico_responsavel`/`aprovado_por` num nome exibível.
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn usuarios(&self) -> Resultado<Vec<UsuarioResumo>> {
        self.arm
            .leitor()
            .consultar(|c| {
                let mut stmt = c
                    .prepare("SELECT id, login, nome, ativo FROM nucleo_usuario ORDER BY nome")
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                let linhas = stmt
                    .query_map([], |r| {
                        let id_bytes = r.get::<_, Vec<u8>>(0)?;
                        Ok(UsuarioResumo {
                            id: <[u8; 16]>::try_from(id_bytes).map_or(Id::NULO, Id::de_bytes),
                            login: r.get(1)?,
                            nome: r.get(2)?,
                            ativo: r.get::<_, i64>(3)? != 0,
                        })
                    })
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                linhas
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)
    }

    /// Os papéis da empresa (nome, descrição, nº de permissões).
    ///
    /// # Errors
    /// Erro de leitura do armazenamento.
    pub fn papeis(&self) -> Resultado<Vec<PapelResumo>> {
        self.arm
            .leitor()
            .consultar(|c| {
                let mut stmt = c
                    .prepare(
                        "SELECT p.nome, p.descricao, p.sistema,
                                (SELECT COUNT(*) FROM nucleo_papel_permissao pp WHERE pp.papel = p.id)
                         FROM nucleo_papel p
                         ORDER BY p.sistema DESC, p.nome",
                    )
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                let linhas = stmt
                    .query_map([], |r| {
                        Ok(PapelResumo {
                            nome: r.get(0)?,
                            descricao: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                            sistema: r.get::<_, i64>(2)? != 0,
                            permissoes: usize::try_from(r.get::<_, i64>(3)?).unwrap_or(0),
                        })
                    })
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))?;
                linhas
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
            })
            .map_err(erro_armazenamento)
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

/// Um usuário, resumido para a lista de Configurações.
#[derive(Debug, Clone)]
pub struct UsuarioResumo {
    /// Identidade.
    pub id: Id,
    /// Login.
    pub login: String,
    /// Nome.
    pub nome: String,
    /// Se está ativo.
    pub ativo: bool,
}

/// Um papel, resumido para a lista de Configurações.
#[derive(Debug, Clone)]
pub struct PapelResumo {
    /// Nome.
    pub nome: String,
    /// Descrição.
    pub descricao: String,
    /// Papel de fábrica (imutável).
    pub sistema: bool,
    /// Quantas permissões concede.
    pub permissoes: usize,
}

/// Completa o papel "Administrador" da empresa com toda permissão do catálogo atual que
/// ainda não esteja nele. Idempotente: se nada falta, não escreve. Só adiciona — uma
/// permissão retirada de propósito não volta a menos que o catálogo a redeclare.
fn sincronizar_papel_admin(
    arm: &Armazenamento,
    empresa: Id,
    conjunto: &ConjuntoEfetivo,
) -> Resultado<()> {
    let catalogo: BTreeSet<String> = conjunto
        .permissoes
        .iter()
        .map(|p| (*p).to_string())
        .collect();
    if catalogo.is_empty() {
        return Ok(());
    }

    let alvo = arm
        .leitor()
        .consultar(|c| {
            c.query_row(
                "SELECT id FROM nucleo_papel
                 WHERE empresa = ?1 AND nome = 'Administrador' AND sistema = 0
                 LIMIT 1",
                [empresa.em_bytes().as_slice()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(|e| ErroArmazenamento::Sqlite(e.to_string()))
        })
        .map_err(erro_armazenamento)?
        .and_then(|bytes| <[u8; 16]>::try_from(bytes).ok())
        .map(Id::de_bytes);
    let Some(papel_id) = alvo else {
        return Ok(());
    };

    let ctx = ContextoEscrita::novo(empresa, Id::novo(), Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            let mut repo = RepositorioAuth::novo(uow);
            let Some(mut papel) = repo
                .papel_por_id(papel_id)
                .map_err(|e| como_erro_armazenamento(&e))?
            else {
                return Ok(());
            };
            let antes = papel.permissoes.len();
            papel.permissoes.extend(catalogo);
            if papel.permissoes.len() == antes {
                return Ok(());
            }
            papel.versao = papel.versao.proxima();
            repo.atualizar_papel(&papel)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?;
    Ok(())
}

/// Busca o usuário, autentica, persiste o efeito colateral (bloqueio/tentativas) e — só em
/// caso de sucesso — monta a `Sessao` a partir dos papéis do usuário cruzados com o conjunto
/// efetivo. Função livre porque não depende de nada de `MotorLocal` além do armazenamento.
fn autenticar_usuario(
    arm: &Armazenamento,
    empresa: Id,
    login: &str,
    senha: &str,
) -> Resultado<Sessao> {
    let login = login.to_string();
    let mut usuario = arm
        .leitor()
        .consultar(|c| {
            cardeal_auth::consultas::usuario_por_login(c, &login)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?
        .ok_or_else(|| Erro::novo(CodigoErro::NAO_ENCONTRADO, "login ou senha inválidos"))?;

    let agora = Instante::agora();
    let resultado = usuario.autenticar(senha, agora);

    let usuario_final = usuario.clone();
    let ctx = ContextoEscrita::novo(empresa, usuario.id, Id::novo(), Id::novo());
    arm.escritor()
        .executar(ctx, move |uow| {
            RepositorioAuth::novo(uow)
                .atualizar_credenciais(&usuario_final)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?;

    resultado.map_err(|e| Erro::de_dominio(&e))?;

    let papeis = arm
        .leitor()
        .consultar(|c| {
            cardeal_auth::consultas::papeis_do_usuario(c, usuario.id, empresa)
                .map_err(|e| como_erro_armazenamento(&e))
        })
        .map_err(erro_armazenamento)?;
    let autorizacoes = AutorizacoesEfetivas::consolidar(&papeis);

    Ok(Sessao::abrir(EmissaoSessao::padrao(
        usuario.id,
        Id::novo(),
        Escopo::empresa_inteira(empresa),
        autorizacoes,
        agora,
    )))
}
