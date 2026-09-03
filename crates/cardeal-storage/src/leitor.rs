//! O pool de leitura — conexões somente leitura sobre o snapshot do WAL.
//!
//! `docs/07-persistencia-sqlite.md` §5: leitores usam snapshot do WAL — **nunca bloqueiam
//! e nunca são bloqueados**. Cada `consultar` pega uma conexão do pool e a devolve ao fim.

use cardeal_kernel::Versao;
use crossbeam_channel::{Receiver, Sender};
use rusqlite::Connection;

use crate::erros::ErroArmazenamento;

type Resultado<T> = Result<T, ErroArmazenamento>;

/// Pega uma conexão emprestada do pool e a devolve automaticamente no `Drop`.
struct Emprestada<'a> {
    conn: Option<Connection>,
    devolver: &'a Sender<Connection>,
}

impl Drop for Emprestada<'_> {
    fn drop(&mut self) {
        if let Some(c) = self.conn.take() {
            let _ = self.devolver.send(c);
        }
    }
}

/// O pool de conexões de leitura.
pub struct Leitor {
    disponiveis: Receiver<Connection>,
    devolver: Sender<Connection>,
}

impl Leitor {
    pub(crate) fn novo(conexoes: Vec<Connection>) -> Self {
        let (tx, rx) = crossbeam_channel::bounded(conexoes.len().max(1));
        for c in conexoes {
            let _ = tx.send(c);
        }
        Self {
            disponiveis: rx,
            devolver: tx,
        }
    }

    /// Executa `f` com uma conexão de leitura. Bloqueia só se todas estiverem em uso.
    ///
    /// # Errors
    /// Propaga o erro do fecho; [`ErroArmazenamento::EscritorIndisponivel`] se o pool foi
    /// fechado.
    ///
    /// # Panics
    /// Não entra em pânico: a conexão emprestada está sempre presente durante o fecho.
    pub fn consultar<T, F>(&self, f: F) -> Resultado<T>
    where
        F: FnOnce(&Connection) -> Resultado<T>,
    {
        let conn = self
            .disponiveis
            .recv()
            .map_err(|_| ErroArmazenamento::EscritorIndisponivel)?;
        let emprestada = Emprestada {
            conn: Some(conn),
            devolver: &self.devolver,
        };
        f(emprestada.conn.as_ref().expect("conexão emprestada"))
    }

    /// Como [`Self::consultar`], mas garantindo enxergar pelo menos a versão `minima`
    /// ("read your writes" — `docs/07-persistencia-sqlite.md` §5).
    ///
    /// Em WAL, toda consulta fora de transação explícita já parte do último snapshot
    /// confirmado, então hoje isto delega a [`Self::consultar`]. O parâmetro fica na
    /// assinatura para o dia em que o pool ganhar réplicas com atraso.
    ///
    /// # Errors
    /// Igual a [`Self::consultar`].
    pub fn consultar_apos<T, F>(&self, _minima: Versao, f: F) -> Resultado<T>
    where
        F: FnOnce(&Connection) -> Resultado<T>,
    {
        self.consultar(f)
    }
}
