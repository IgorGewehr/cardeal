//! Uma tarefa do motor em segundo plano (a transmissão fiscal, a busca de notas na SEFAZ).
//!
//! Uma thread por vez roda a tarefa com o executor de tarefas do motor (clones baratos do
//! escritor e do leitor — a tela continua respondendo). Cada rodada que termina aumenta a
//! [`TarefaDeFundo::geracao`]: a tela que mostra o resultado compara com a que viu e recarrega.
//! A vigia ([`TarefaDeFundo::vigiar`]) confere, de tempos em tempos, se há o que fazer.
//!
//! No motor remoto não há executor: lá quem roda é o servidor.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use cardeal_cliente::{Motor, Sessao};
use eframe::egui;

struct Estado {
    rodando: bool,
    ultima_vigia: Option<Instant>,
    geracao: u64,
    ultimo: Option<Result<String, String>>,
}

/// Uma tarefa do motor que roda em segundo plano, uma rodada por vez.
pub struct TarefaDeFundo {
    tarefa: &'static str,
    permissao: &'static str,
    estado: Mutex<Estado>,
}

impl TarefaDeFundo {
    /// A tarefa `tarefa`, que só roda para quem tem `permissao`.
    pub const fn nova(tarefa: &'static str, permissao: &'static str) -> Self {
        Self {
            tarefa,
            permissao,
            estado: Mutex::new(Estado {
                rodando: false,
                ultima_vigia: None,
                geracao: 0,
                ultimo: None,
            }),
        }
    }

    fn estado(&self) -> MutexGuard<'_, Estado> {
        self.estado
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Dispara uma rodada numa thread, se nenhuma estiver rodando. `false` se não disparou.
    pub fn disparar(&'static self, ctx: &egui::Context, motor: &Motor, sessao: &Sessao) -> bool {
        if !sessao.concede(self.permissao) {
            return false;
        }
        let Some(executor) = motor.executor_de_tarefas(sessao) else {
            return false;
        };
        {
            let mut e = self.estado();
            if e.rodando {
                return false;
            }
            e.rodando = true;
        }
        let ctx = ctx.clone();
        let iniciou = std::thread::Builder::new()
            .name(self.tarefa.into())
            .spawn(move || {
                let r = executor.executar(self.tarefa).map_err(|e| e.mensagem);
                if let Err(m) = &r {
                    tracing::warn!(tarefa = self.tarefa, erro = %m, "tarefa de fundo");
                }
                let mut e = self.estado();
                e.rodando = false;
                e.geracao += 1;
                e.ultimo = Some(r);
                drop(e);
                ctx.request_repaint();
            })
            .is_ok();
        if !iniciou {
            self.estado().rodando = false;
        }
        iniciou
    }

    /// Se há uma rodada em andamento.
    pub fn rodando(&self) -> bool {
        self.estado().rodando
    }

    /// Quantas rodadas já terminaram — a tela recarrega quando muda.
    pub fn geracao(&self) -> u64 {
        self.estado().geracao
    }

    /// O resultado da última rodada (o resumo, ou por que não deu).
    pub fn ultimo_resultado(&self) -> Option<Result<String, String>> {
        self.estado().ultimo.clone()
    }

    /// Chamada a cada quadro da sessão: uma vez por `intervalo`, se `precisa` (uma consulta
    /// barata) diz que há o que fazer, dispara. Pede um quadro novo para o próximo intervalo.
    pub fn vigiar(
        &'static self,
        ctx: &egui::Context,
        motor: &Motor,
        sessao: &Sessao,
        intervalo: Duration,
        precisa: impl FnOnce() -> bool,
    ) {
        if !sessao.concede(self.permissao) {
            return;
        }
        {
            let mut e = self.estado();
            let cedo = e.ultima_vigia.is_some_and(|t| t.elapsed() < intervalo);
            if cedo || e.rodando {
                drop(e);
                ctx.request_repaint_after(intervalo);
                return;
            }
            e.ultima_vigia = Some(Instant::now());
        }
        if precisa() {
            self.disparar(ctx, motor, sessao);
        }
        ctx.request_repaint_after(intervalo);
    }
}
