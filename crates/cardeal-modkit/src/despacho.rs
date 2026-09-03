//! O despacho: a trait [`Modulo`], os traits [`Comando`]/[`Consulta`], o [`Registro`] onde
//! um módulo declara seus manipuladores e o [`Despachante`] que os executa.
//!
//! `docs/04-pilar-modularidade.md` §3 e `docs/contratos-internos.md` §4. O fluxo de um
//! comando é sempre o mesmo (`docs/15-convencoes-codigo.md` §3):
//!
//! 1. O transporte entrega `(nome, carga)` ao [`Despachante`].
//! 2. O despachante confere que o **módulo dono está ativo** na empresa e chama
//!    [`cardeal_auth::autorizar`] — permissão **e** escopo (`docs/08 §3.5`: o servidor é a
//!    fonte de verdade).
//! 3. Passa um fecho a [`Escritor::executar`]: desserializa a carga no tipo concreto `C`,
//!    roda `C::executar(&Ctx, &mut UnidadeDeTrabalho)` dentro do `SAVEPOINT` da tarefa,
//!    serializa a saída.
//! 4. Um erro de domínio desfaz o `SAVEPOINT` e sobe intacto (via
//!    [`ErroArmazenamento::Dominio`](cardeal_storage::ErroArmazenamento)).
//!
//! Consultas seguem o mesmo caminho, mas sobre o [`Leitor`](cardeal_storage::Leitor) e sem
//! transação.
//!
//! ## O que ainda não está aqui
//!
//! `Ctx::contas()` (espera um resolvedor não-genérico em `cardeal-ledger`), `Registro`
//! para assinaturas de evento / tarefas agendadas / portas / itens do Pulso, replay de
//! idempotência, e a gravação automática de auditoria quando `Comando::AUDITA` — hoje o
//! flag é só metadado do registro.

use std::collections::HashMap;
use std::sync::Arc;

use cardeal_auth::{autorizar, AutorizacoesEfetivas, Escopo, Sessao, ValorLimite};
use cardeal_kernel::{CodigoErro, Data, Erro, Fuso, Id, Instante, Resultado};
use cardeal_storage::{
    ConjuntoMigracoes, ContextoEscrita, ErroArmazenamento, Escritor, Leitor, UnidadeDeTrabalho,
};
use rusqlite::Connection;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::manifesto::Manifesto;
use crate::permissao::Risco;
use crate::registro::ConjuntoEfetivo;

/// O contexto de execução de um comando ou consulta: quem, onde, quando — e o que a
/// empresa tem ativo. Ver `docs/contratos-internos.md` §4.
#[derive(Debug, Clone)]
pub struct Ctx {
    /// A empresa.
    pub empresa: Id,
    /// O usuário responsável.
    pub usuario: Id,
    /// O dispositivo de origem.
    pub dispositivo: Id,
    /// A sessão.
    pub sessao: Id,
    /// O instante da operação.
    pub agora: Instante,
    /// O fuso da empresa.
    pub fuso: Fuso,
    /// O id de correlação, para rastrear a operação ponta a ponta.
    pub correlacao: Id,
    conjunto: Arc<ConjuntoEfetivo>,
    autorizacoes: Arc<AutorizacoesEfetivas>,
}

impl Ctx {
    /// A data corrente, no fuso da empresa.
    #[must_use]
    pub fn hoje(&self) -> Data {
        self.agora.data(self.fuso)
    }

    /// Verdadeiro se o módulo está ativo na empresa.
    #[must_use]
    pub fn ativo(&self, modulo: &str) -> bool {
        self.conjunto.modulo_ativo(modulo)
    }

    /// Verdadeiro se o submódulo está ativo na empresa.
    #[must_use]
    pub fn submodulo_ativo(&self, modulo: &str, submodulo: &str) -> bool {
        self.conjunto.submodulo_ativo(modulo, submodulo)
    }

    /// Verdadeiro se a sessão concede a permissão — para o comando checar um poder
    /// secundário (ex.: ver custo enquanto vende).
    #[must_use]
    pub fn concede(&self, permissao: &str) -> bool {
        self.autorizacoes.concede(permissao)
    }

    /// O limite quantitativo associado a uma chave (`docs/08 §3.4`) — o comando decide
    /// entre barrar e escalar ao supervisor.
    #[must_use]
    pub fn limite(&self, chave: &str) -> Option<ValorLimite> {
        self.autorizacoes.limite(chave)
    }

    /// O conjunto efetivo de módulos da empresa.
    #[must_use]
    pub fn conjunto(&self) -> &ConjuntoEfetivo {
        &self.conjunto
    }
}

/// Um comando: uma intenção de mudar o estado. `docs/15-convencoes-codigo.md` §3 e §6.
///
/// A macro `#[comando(...)]` (a nascer em `cardeal-protocol`) preencherá `PERMISSAO`,
/// `RISCO` e `AUDITA`; por ora o módulo implementa o trait à mão.
pub trait Comando: DeserializeOwned + Send + 'static {
    /// O que o comando devolve em caso de sucesso.
    type Saida: Serialize + Send + 'static;

    /// A permissão exigida — sem isto, não compila (`docs/08 §3.5`).
    const PERMISSAO: &'static str;

    /// O risco da ação, para a interface e a auditoria.
    const RISCO: Risco = Risco::Medio;

    /// Se verdadeiro, a operação deve ser registrada em `nucleo_auditoria`.
    const AUDITA: bool = false;

    /// Executa o comando dentro da unidade de trabalho do escritor.
    ///
    /// # Errors
    /// Qualquer erro de domínio do módulo — desfaz o `SAVEPOINT` da tarefa.
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida>;
}

/// Uma consulta: uma leitura, sem efeito. Roda sobre uma conexão do pool do [`Leitor`].
pub trait Consulta: DeserializeOwned + Send + 'static {
    /// O que a consulta devolve.
    type Saida: Serialize + Send + 'static;

    /// A permissão exigida.
    const PERMISSAO: &'static str;

    /// Executa a consulta.
    ///
    /// # Errors
    /// Qualquer erro de domínio do módulo.
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida>;
}

/// Um módulo de negócio, do ponto de vista do motor. Ver `docs/contratos-internos.md` §4.
pub trait Modulo: Send + Sync + 'static {
    /// O manifesto estático do módulo.
    fn manifesto(&self) -> &'static Manifesto;

    /// As migrações de esquema que o módulo traz.
    fn migracoes(&self) -> ConjuntoMigracoes;

    /// Declara os comandos e consultas do módulo no [`Registro`].
    ///
    /// # Errors
    /// Um erro de configuração do módulo (raro; a maioria dos módulos devolve `Ok`).
    fn registrar(&self, registro: &mut Registro) -> Resultado<()>;
}

type ExecutorComando = Arc<
    dyn Fn(&Ctx, &mut UnidadeDeTrabalho, &[u8]) -> Result<Vec<u8>, ErroArmazenamento> + Send + Sync,
>;

type ExecutorConsulta =
    Arc<dyn Fn(&Ctx, &Connection, &[u8]) -> Result<Vec<u8>, ErroArmazenamento> + Send + Sync>;

struct EntradaComando {
    permissao: &'static str,
    risco: Risco,
    audita: bool,
    executor: ExecutorComando,
}

struct EntradaConsulta {
    permissao: &'static str,
    executor: ExecutorConsulta,
}

/// Onde um módulo declara seus manipuladores. Estilo encadeável; erros de declaração
/// (nome repetido) são acumulados e checados por [`Despachante::construir`].
#[derive(Default)]
pub struct Registro {
    comandos: HashMap<&'static str, EntradaComando>,
    consultas: HashMap<&'static str, EntradaConsulta>,
    erros: Vec<String>,
}

impl Registro {
    /// Um registro vazio.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Declara um comando sob o nome versionado `nome` (ex.: `"financeiro.baixar_parcela.v1"`).
    pub fn comando<C: Comando>(&mut self, nome: &'static str) -> &mut Self {
        let executor: ExecutorComando =
            Arc::new(|ctx: &Ctx, uow: &mut UnidadeDeTrabalho, carga: &[u8]| {
                let cmd: C = postcard::from_bytes(carga).map_err(carga_invalida)?;
                let saida = cmd.executar(ctx, uow)?;
                postcard::to_stdvec(&saida).map_err(saida_invalida)
            });
        if self
            .comandos
            .insert(
                nome,
                EntradaComando {
                    permissao: C::PERMISSAO,
                    risco: C::RISCO,
                    audita: C::AUDITA,
                    executor,
                },
            )
            .is_some()
        {
            self.erros
                .push(format!("comando \"{nome}\" declarado duas vezes"));
        }
        self
    }

    /// Declara uma consulta sob o nome `nome`.
    pub fn consulta<Q: Consulta>(&mut self, nome: &'static str) -> &mut Self {
        let executor: ExecutorConsulta =
            Arc::new(|ctx: &Ctx, conexao: &Connection, carga: &[u8]| {
                let q: Q = postcard::from_bytes(carga).map_err(carga_invalida)?;
                let saida = q.executar(ctx, conexao)?;
                postcard::to_stdvec(&saida).map_err(saida_invalida)
            });
        if self
            .consultas
            .insert(
                nome,
                EntradaConsulta {
                    permissao: Q::PERMISSAO,
                    executor,
                },
            )
            .is_some()
        {
            self.erros
                .push(format!("consulta \"{nome}\" declarada duas vezes"));
        }
        self
    }
}

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(carga_invalida)`
fn carga_invalida(e: postcard::Error) -> ErroArmazenamento {
    Erro::novo(
        CodigoErro::ENTRADA_INVALIDA,
        format!("carga do comando/consulta inválida: {e}"),
    )
    .into()
}

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(saida_invalida)`
fn saida_invalida(e: postcard::Error) -> ErroArmazenamento {
    Erro::novo(
        CodigoErro::FALHA_INTERNA,
        format!("falha ao serializar a saída: {e}"),
    )
    .into()
}

/// O que a empresa tem ativo — resolvido uma vez (por `RegistroModulos::resolver`) e
/// reusado em cada despacho.
#[derive(Debug, Clone)]
pub struct Ambiente {
    empresa: Id,
    conjunto: Arc<ConjuntoEfetivo>,
    fuso: Fuso,
}

impl Ambiente {
    /// Monta o ambiente de uma empresa a partir do seu conjunto efetivo. Fuso padrão:
    /// Brasília (`docs/06 §1`).
    #[must_use]
    pub fn novo(empresa: Id, conjunto: ConjuntoEfetivo) -> Self {
        Self {
            empresa,
            conjunto: Arc::new(conjunto),
            fuso: Fuso::BRASILIA,
        }
    }

    /// Fixa o fuso da empresa.
    #[must_use]
    pub const fn com_fuso(mut self, fuso: Fuso) -> Self {
        self.fuso = fuso;
        self
    }

    /// A empresa.
    #[must_use]
    pub const fn empresa(&self) -> Id {
        self.empresa
    }
}

/// Executa comandos e consultas por nome, resolvendo autorização e transação. Construído
/// uma vez no boot a partir dos módulos compilados.
pub struct Despachante {
    comandos: HashMap<&'static str, EntradaComando>,
    consultas: HashMap<&'static str, EntradaConsulta>,
}

impl std::fmt::Debug for Despachante {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Despachante")
            .field("comandos", &self.comandos.len())
            .field("consultas", &self.consultas.len())
            .finish()
    }
}

impl Despachante {
    /// Coleta os manipuladores de todos os módulos.
    ///
    /// # Errors
    /// [`CodigoErro::DUPLICADO`] se dois módulos declaram o mesmo nome de comando/consulta,
    /// ou o erro que um `Modulo::registrar` devolver.
    pub fn construir(modulos: &[&dyn Modulo]) -> Resultado<Self> {
        let mut registro = Registro::novo();
        for m in modulos {
            m.registrar(&mut registro)?;
        }
        if let Some(erro) = registro.erros.first() {
            return Err(Erro::novo(CodigoErro::DUPLICADO, erro.clone()));
        }
        Ok(Self {
            comandos: registro.comandos,
            consultas: registro.consultas,
        })
    }

    /// Quantos comandos e consultas foram registrados.
    #[must_use]
    pub fn total(&self) -> (usize, usize) {
        (self.comandos.len(), self.consultas.len())
    }

    /// Verdadeiro se um comando com esse nome está registrado.
    #[must_use]
    pub fn tem_comando(&self, nome: &str) -> bool {
        self.comandos.contains_key(nome)
    }

    /// O risco e o flag de auditoria de um comando — o transporte usa para decidir
    /// confirmação extra e a auditoria para saber se registra a operação.
    #[must_use]
    pub fn metadados_comando(&self, nome: &str) -> Option<(Risco, bool)> {
        self.comandos.get(nome).map(|e| (e.risco, e.audita))
    }

    /// Executa um comando: confere módulo ativo, autoriza, roda na transação do escritor.
    /// A carga e a saída são `postcard`.
    ///
    /// # Errors
    /// - [`CodigoErro::NAO_ENCONTRADO`] se o comando não existe.
    /// - [`CodigoErro::ESTADO_INVALIDO`] se o módulo dono está inativo na empresa.
    /// - O erro de [`cardeal_auth::autorizar`] (sem permissão / fora do escopo).
    /// - Qualquer erro de domínio do próprio comando.
    pub fn executar_comando(
        &self,
        nome: &str,
        carga: &[u8],
        sessao: &Sessao,
        ambiente: &Ambiente,
        escritor: &Escritor,
    ) -> Resultado<Vec<u8>> {
        let entrada = self
            .comandos
            .get(nome)
            .ok_or_else(|| nao_encontrado("comando", nome))?;

        verificar_acesso(entrada.permissao, sessao, ambiente)?;

        let ctx = montar_ctx(sessao, ambiente);
        let executor = Arc::clone(&entrada.executor);
        let carga = carga.to_vec();

        let mut cte = ContextoEscrita::novo(
            ambiente.empresa,
            sessao.usuario,
            sessao.dispositivo,
            sessao.id,
        );
        cte.agora = ctx.agora;
        cte.correlacao = ctx.correlacao;

        match escritor.executar(cte, move |uow| executor(&ctx, uow, &carga)) {
            Ok(confirmado) => Ok(confirmado.valor),
            Err(ErroArmazenamento::Dominio(e)) => Err(e),
            Err(outro) => Err(Erro::de_dominio(&outro)),
        }
    }

    /// Executa uma consulta: confere módulo ativo, autoriza, roda sobre o pool de leitura.
    ///
    /// # Errors
    /// Como [`Self::executar_comando`], menos a parte transacional.
    pub fn executar_consulta(
        &self,
        nome: &str,
        carga: &[u8],
        sessao: &Sessao,
        ambiente: &Ambiente,
        leitor: &Leitor,
    ) -> Resultado<Vec<u8>> {
        let entrada = self
            .consultas
            .get(nome)
            .ok_or_else(|| nao_encontrado("consulta", nome))?;

        verificar_acesso(entrada.permissao, sessao, ambiente)?;

        let ctx = montar_ctx(sessao, ambiente);
        let executor = Arc::clone(&entrada.executor);

        match leitor.consultar(move |conexao| executor(&ctx, conexao, carga)) {
            Ok(saida) => Ok(saida),
            Err(ErroArmazenamento::Dominio(e)) => Err(e),
            Err(outro) => Err(Erro::de_dominio(&outro)),
        }
    }
}

fn verificar_acesso(permissao: &str, sessao: &Sessao, ambiente: &Ambiente) -> Resultado<()> {
    let modulo = permissao.split('.').next().unwrap_or(permissao);
    if !ambiente.conjunto.modulo_ativo(modulo) {
        return Err(Erro::novo(
            CodigoErro::ESTADO_INVALIDO,
            format!("o módulo \"{modulo}\" não está ativo nesta empresa"),
        ));
    }
    autorizar(
        sessao,
        permissao,
        &Escopo::empresa_inteira(ambiente.empresa),
    )
    .map_err(|e| Erro::de_dominio(&e))
}

fn montar_ctx(sessao: &Sessao, ambiente: &Ambiente) -> Ctx {
    Ctx {
        empresa: ambiente.empresa,
        usuario: sessao.usuario,
        dispositivo: sessao.dispositivo,
        sessao: sessao.id,
        agora: Instante::agora(),
        fuso: ambiente.fuso,
        correlacao: Id::novo(),
        conjunto: Arc::clone(&ambiente.conjunto),
        autorizacoes: Arc::new(sessao.autorizacoes().clone()),
    }
}

fn nao_encontrado(especie: &str, nome: &str) -> Erro {
    Erro::novo(
        CodigoErro::NAO_ENCONTRADO,
        format!("{especie} \"{nome}\" não está registrado"),
    )
}

#[cfg(test)]
mod testes;
