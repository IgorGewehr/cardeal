//! Os contratos de [`Comando`] e [`Consulta`]: o **tipo** de cada mensagem (o que ela devolve,
//! que permissão exige) existe sempre; a **execução** só com a feature `sqlite`.
//!
//! É o que deixa o cliente web (egui/WASM, ADR-0016) chamar um comando com o mesmo tipo que o
//! desktop usa — e saber de antemão a permissão, para esconder o botão de quem não a tem —
//! sem compilar uma linha de SQLite.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::permissao::Risco;

#[cfg(feature = "sqlite")]
use crate::despacho::Ctx;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;

/// O teto de segurança das consultas que devolvem uma lista **completa** (`LIMIT 50000` no
/// SQL delas). Não é paginação: é a garantia de que uma lista nunca é cortada em silêncio
/// num tamanho que uma empresa real alcança (antes eram 200–1000, e o 501º produto sumia do
/// estoque). Listas grandes pela rede usam as consultas paginadas (`Pagina`, `.v2`).
pub const TETO_LISTA_COMPLETA: u32 = 50_000;

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
    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida>;
}

/// Uma consulta: uma leitura, sem efeito. Roda sobre uma conexão do pool de leitura.
pub trait Consulta: DeserializeOwned + Send + 'static {
    /// O que a consulta devolve.
    type Saida: Serialize + Send + 'static;

    /// A permissão exigida.
    const PERMISSAO: &'static str;

    /// Executa a consulta.
    ///
    /// # Errors
    /// Qualquer erro de domínio do módulo.
    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida>;
}
