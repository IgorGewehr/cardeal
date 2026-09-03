//! O escritor único e o group commit (`docs/07-persistencia-sqlite.md` §3 e §4,
//! [ADR-0012](../../adr/0012-escritor-unico-com-group-commit.md)).
//!
//! Uma thread, uma conexão. As tarefas chegam por uma fila; a thread recolhe o que chega em
//! uma janela de milissegundos, roda cada uma no seu próprio `SAVEPOINT`, e fecha o lote com
//! **um único `COMMIT`** — um `fsync` para até dezenas de operações.

use std::any::Any;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use cardeal_kernel::Versao;
use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use parking_lot::Mutex;
use rusqlite::{Connection, DropBehavior, TransactionBehavior};

use crate::contexto::{Confirmado, ContextoEscrita};
use crate::erros::ErroArmazenamento;
use crate::uow::UnidadeDeTrabalho;

type Resultado<T> = Result<T, ErroArmazenamento>;
type SaidaBoxed = Box<dyn Any + Send>;
type Trabalho = Box<dyn for<'a> FnOnce(&mut UnidadeDeTrabalho<'a>) -> Resultado<SaidaBoxed> + Send>;
type Resposta = Sender<Resultado<Confirmado<SaidaBoxed>>>;

pub(crate) enum Tarefa {
    /// Uma unidade de trabalho a executar no group commit.
    Trabalho {
        ctx: ContextoEscrita,
        trabalho: Trabalho,
        resposta: Resposta,
    },
    /// Sinal para a thread encerrar depois de drenar o lote atual.
    Encerrar,
}

#[derive(Debug, Default, Clone, Copy)]
struct MetricasInternas {
    lotes: u64,
    tarefas: u64,
    fsyncs: u64,
    maior_lote: u64,
    tarefas_com_erro: u64,
}

/// Métricas acumuladas do escritor — a profundidade da fila é a mais importante de
/// monitorar (`docs/07-persistencia-sqlite.md` §3, ADR-0012 "mitigações").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetricasEscritor {
    /// Lotes confirmados.
    pub lotes: u64,
    /// Tarefas processadas (com ou sem erro).
    pub tarefas: u64,
    /// Tarefas que falharam e foram isoladas no seu savepoint.
    pub tarefas_com_erro: u64,
    /// `fsync`s realizados (= lotes confirmados).
    pub fsyncs: u64,
    /// Maior lote já confirmado.
    pub maior_lote: u64,
    /// Tamanho médio de lote.
    pub tamanho_medio_lote: f64,
    /// Tarefas esperando na fila agora.
    pub fila_atual: usize,
}

/// O ponto de entrada de toda escrita do sistema.
pub struct Escritor {
    fila: Sender<Tarefa>,
    metricas: Arc<Mutex<MetricasInternas>>,
}

impl Escritor {
    pub(crate) fn iniciar(
        conn: Connection,
        versao_global: Arc<AtomicU64>,
        janela_ms: u64,
        maximo_lote: usize,
    ) -> (Self, std::thread::JoinHandle<()>) {
        let (fila_tx, fila_rx) = crossbeam_channel::bounded::<Tarefa>(512);
        let metricas = Arc::new(Mutex::new(MetricasInternas::default()));
        let metricas_thread = Arc::clone(&metricas);
        let janela = Duration::from_millis(janela_ms.max(1));
        let maximo = maximo_lote.max(1);

        let handle = std::thread::Builder::new()
            .name("cardeal-escritor".into())
            .spawn(move || {
                laco(
                    conn,
                    &fila_rx,
                    &versao_global,
                    &metricas_thread,
                    janela,
                    maximo,
                );
            })
            .expect("criar a thread do escritor");

        (
            Self {
                fila: fila_tx,
                metricas,
            },
            handle,
        )
    }

    /// Enfileira a unidade de trabalho, participa do group commit e devolve **após o
    /// `fsync` do commit**. É aqui que "confirmado" ganha significado.
    ///
    /// # Errors
    /// [`ErroArmazenamento::EscritorIndisponivel`] se a thread caiu;
    /// [`ErroArmazenamento::TarefaEntrouEmPanico`] se o fecho entrou em pânico;
    /// qualquer erro que o fecho devolva.
    pub fn executar<T, F>(&self, ctx: ContextoEscrita, f: F) -> Resultado<Confirmado<T>>
    where
        F: for<'a> FnOnce(&mut UnidadeDeTrabalho<'a>) -> Resultado<T> + Send + 'static,
        T: Send + 'static,
    {
        let (resp_tx, resp_rx) = crossbeam_channel::bounded(1);
        let trabalho: Trabalho = Box::new(move |uow| f(uow).map(|v| Box::new(v) as SaidaBoxed));

        self.fila
            .send(Tarefa::Trabalho {
                ctx,
                trabalho,
                resposta: resp_tx,
            })
            .map_err(|_| ErroArmazenamento::EscritorIndisponivel)?;

        let confirmado = resp_rx
            .recv()
            .map_err(|_| ErroArmazenamento::EscritorIndisponivel)??;

        let valor = *confirmado
            .valor
            .downcast::<T>()
            .map_err(|_| ErroArmazenamento::Sqlite("tipo de retorno inesperado".into()))?;
        Ok(Confirmado {
            valor,
            versao: confirmado.versao,
        })
    }

    /// Sinaliza a thread do escritor para encerrar após drenar o lote atual.
    pub(crate) fn encerrar(&self) {
        let _ = self.fila.send(Tarefa::Encerrar);
    }

    /// As métricas acumuladas.
    #[must_use]
    pub fn metricas(&self) -> MetricasEscritor {
        let m = *self.metricas.lock();
        MetricasEscritor {
            lotes: m.lotes,
            tarefas: m.tarefas,
            tarefas_com_erro: m.tarefas_com_erro,
            fsyncs: m.fsyncs,
            maior_lote: m.maior_lote,
            #[allow(clippy::cast_precision_loss)]
            tamanho_medio_lote: if m.lotes == 0 {
                0.0
            } else {
                m.tarefas as f64 / m.lotes as f64
            },
            fila_atual: self.fila.len(),
        }
    }
}

fn laco(
    mut conn: Connection,
    fila: &Receiver<Tarefa>,
    versao_global: &AtomicU64,
    metricas: &Mutex<MetricasInternas>,
    janela: Duration,
    maximo: usize,
) {
    'principal: while let Ok(primeira) = fila.recv() {
        if matches!(primeira, Tarefa::Encerrar) {
            break;
        }
        let mut lote = Vec::with_capacity(maximo.min(64));
        lote.push(primeira);
        let prazo = Instant::now() + janela;
        let mut encerrar = false;
        while lote.len() < maximo {
            match fila.recv_deadline(prazo) {
                Ok(Tarefa::Encerrar) | Err(RecvTimeoutError::Disconnected) => {
                    encerrar = true;
                    break;
                }
                Ok(t) => lote.push(t),
                Err(RecvTimeoutError::Timeout) => break,
            }
        }
        processar_lote(&mut conn, lote, versao_global, metricas);
        if encerrar {
            break 'principal;
        }
    }
    tracing::debug!("thread do escritor encerrada");
}

fn processar_lote(
    conn: &mut Connection,
    lote: Vec<Tarefa>,
    versao_global: &AtomicU64,
    metricas: &Mutex<MetricasInternas>,
) {
    // O laço já filtrou `Tarefa::Encerrar`; aqui só há trabalho.
    let trabalhos: Vec<(ContextoEscrita, Trabalho, Resposta)> = lote
        .into_iter()
        .filter_map(|t| match t {
            Tarefa::Trabalho {
                ctx,
                trabalho,
                resposta,
            } => Some((ctx, trabalho, resposta)),
            Tarefa::Encerrar => None,
        })
        .collect();
    if trabalhos.is_empty() {
        return;
    }

    let mut tx = match conn.transaction_with_behavior(TransactionBehavior::Immediate) {
        Ok(tx) => tx,
        Err(e) => {
            let err = ErroArmazenamento::sqlite(e);
            for (_, _, resposta) in trabalhos {
                let _ = resposta.send(Err(err.clone()));
            }
            return;
        }
    };
    tx.set_drop_behavior(DropBehavior::Rollback);

    let mut pendentes: Vec<(Resposta, Resultado<SaidaBoxed>)> = Vec::with_capacity(trabalhos.len());
    for (ctx, trabalho, resposta) in trabalhos {
        let r = executar_tarefa(&mut tx, &ctx, trabalho);
        pendentes.push((resposta, r));
    }

    match tx.commit() {
        Ok(()) => {
            let versao = Versao::nova(versao_global.fetch_add(1, Ordering::SeqCst) + 1);
            {
                let mut m = metricas.lock();
                m.lotes += 1;
                m.fsyncs += 1;
                let n = u64::try_from(pendentes.len()).unwrap_or(u64::MAX);
                m.tarefas += n;
                m.maior_lote = m.maior_lote.max(n);
                m.tarefas_com_erro +=
                    u64::try_from(pendentes.iter().filter(|(_, r)| r.is_err()).count())
                        .unwrap_or(0);
            }
            for (resposta, r) in pendentes {
                let _ = resposta.send(r.map(|valor| Confirmado { valor, versao }));
            }
        }
        Err(e) => {
            let err = ErroArmazenamento::sqlite(e);
            for (resposta, _) in pendentes {
                let _ = resposta.send(Err(err.clone()));
            }
        }
    }
}

fn executar_tarefa(
    tx: &mut rusqlite::Transaction<'_>,
    ctx: &ContextoEscrita,
    trabalho: Trabalho,
) -> Resultado<SaidaBoxed> {
    let mut sp = tx.savepoint().map_err(ErroArmazenamento::sqlite)?;
    sp.set_drop_behavior(DropBehavior::Rollback);

    let resultado = catch_unwind(AssertUnwindSafe(|| {
        let mut uow = UnidadeDeTrabalho::nova(&sp, ctx);
        trabalho(&mut uow)
    }));

    match resultado {
        Ok(Ok(saida)) => {
            sp.commit().map_err(ErroArmazenamento::sqlite)?;
            Ok(saida)
        }
        Ok(Err(e)) => {
            drop(sp); // ROLLBACK TO SAVEPOINT
            Err(e)
        }
        Err(_) => {
            drop(sp);
            Err(ErroArmazenamento::TarefaEntrouEmPanico)
        }
    }
}
