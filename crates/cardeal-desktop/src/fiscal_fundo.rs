//! A transmissão fiscal em segundo plano: a venda nunca espera a SEFAZ (`docs/10` §3).
//!
//! Uma thread por vez roda a tarefa `fiscal.transmitir_pendentes` com o executor de tarefas do
//! motor (clones baratos do escritor e do leitor — a tela continua respondendo). Dispara depois
//! de cada venda e, a cada minuto, se houver nota esperando ([`vigiar`]). Cada rodada que
//! termina aumenta a [`geracao`]: a tela que mostra a fila compara com a que viu e recarrega.
//!
//! No motor remoto não há executor: lá quem transmite é o servidor.

use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use cardeal_cliente::{Motor, Sessao};
use eframe::egui;

const INTERVALO_DA_VIGIA: Duration = Duration::from_secs(60);

#[derive(Default)]
struct Estado {
    rodando: bool,
    ultima_vigia: Option<Instant>,
    geracao: u64,
    ultimo: Option<Result<String, String>>,
}

static ESTADO: Mutex<Estado> = Mutex::new(Estado {
    rodando: false,
    ultima_vigia: None,
    geracao: 0,
    ultimo: None,
});

fn estado() -> MutexGuard<'static, Estado> {
    ESTADO
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Dispara uma rodada numa thread, se nenhuma estiver rodando. `false` se não disparou.
pub fn disparar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao) -> bool {
    if !sessao.concede("fiscal.emitir") {
        return false;
    }
    let Some(executor) = motor.executor_de_tarefas(sessao) else {
        return false;
    };
    {
        let mut e = estado();
        if e.rodando {
            return false;
        }
        e.rodando = true;
    }
    let ctx = ctx.clone();
    let iniciou = std::thread::Builder::new()
        .name("transmissao-fiscal".into())
        .spawn(move || {
            let r = executor
                .executar(mod_fiscal::TAREFA_TRANSMITIR)
                .map_err(|e| e.mensagem);
            if let Err(m) = &r {
                tracing::warn!(erro = %m, "transmissão fiscal");
            }
            let mut e = estado();
            e.rodando = false;
            e.geracao += 1;
            e.ultimo = Some(r);
            drop(e);
            ctx.request_repaint();
        })
        .is_ok();
    if !iniciou {
        estado().rodando = false;
    }
    iniciou
}

/// Se há uma rodada em andamento.
pub fn rodando() -> bool {
    estado().rodando
}

/// Quantas rodadas já terminaram — a tela recarrega quando muda.
pub fn geracao() -> u64 {
    estado().geracao
}

/// O resultado da última rodada (o resumo, ou por que não deu para transmitir).
pub fn ultimo_resultado() -> Option<Result<String, String>> {
    estado().ultimo.clone()
}

/// Chamada a cada quadro da sessão: uma vez por minuto confere a fila e, se houver nota
/// esperando, transmite. Pede um quadro novo para o próximo minuto.
pub fn vigiar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao) {
    if !sessao.concede("fiscal.emitir") {
        return;
    }
    {
        let mut e = estado();
        let cedo = e
            .ultima_vigia
            .is_some_and(|t| t.elapsed() < INTERVALO_DA_VIGIA);
        if cedo || e.rodando {
            drop(e);
            ctx.request_repaint_after(INTERVALO_DA_VIGIA);
            return;
        }
        e.ultima_vigia = Some(Instant::now());
    }
    let esperando = motor
        .consultar(
            sessao,
            "fiscal.resumo_da_fila.v1",
            &mod_fiscal::ResumoDaFilaFiscal,
        )
        .is_ok_and(|r| r.pendentes + r.incertas > 0);
    if esperando {
        disparar(ctx, motor, sessao);
    }
    ctx.request_repaint_after(INTERVALO_DA_VIGIA);
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn roda_numa_thread_e_avisa_quando_termina() {
        let t = motor_de_teste();
        let ctx = egui::Context::default();
        let antes = geracao();
        assert!(disparar(&ctx, &t.motor, &t.sessao));
        let limite = Instant::now() + Duration::from_secs(10);
        while geracao() == antes && Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(geracao() > antes, "a rodada terminou");
        assert!(!rodando());
        assert_eq!(ultimo_resultado(), Some(Ok("nada na fila".to_string())));
    }
}
