//! A transmissão fiscal em segundo plano: a venda nunca espera a SEFAZ (`docs/10` §3).
//!
//! A tarefa `fiscal.transmitir_pendentes` roda em segundo plano ([`crate::fundo`]): dispara
//! depois de cada venda e, a cada minuto, se houver nota esperando ([`vigiar`]). Cada rodada
//! que termina aumenta a [`geracao`]: a tela que mostra a fila compara com a que viu e
//! recarrega.
//!
//! No motor remoto não há executor: lá quem transmite é o servidor.

use std::time::Duration;

use cardeal_cliente::{Motor, Sessao};
use eframe::egui;

use crate::fundo::TarefaDeFundo;

const INTERVALO_DA_VIGIA: Duration = Duration::from_secs(60);

static TRANSMISSAO: TarefaDeFundo =
    TarefaDeFundo::nova(mod_fiscal::TAREFA_TRANSMITIR, "fiscal.emitir");

/// Dispara uma rodada numa thread, se nenhuma estiver rodando. `false` se não disparou.
pub fn disparar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao) -> bool {
    TRANSMISSAO.disparar(ctx, motor, sessao)
}

/// Se há uma rodada em andamento.
pub fn rodando() -> bool {
    TRANSMISSAO.rodando()
}

/// Quantas rodadas já terminaram — a tela recarrega quando muda.
pub fn geracao() -> u64 {
    TRANSMISSAO.geracao()
}

/// O resultado da última rodada (o resumo, ou por que não deu para transmitir).
pub fn ultimo_resultado() -> Option<Result<String, String>> {
    TRANSMISSAO.ultimo_resultado()
}

/// Chamada a cada quadro da sessão: uma vez por minuto confere a fila e, se houver nota
/// esperando — na fila, conferindo ou em contingência — (ou XML a buscar na SEFAZ), transmite. Pede um quadro novo para o próximo minuto.
pub fn vigiar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao) {
    TRANSMISSAO.vigiar(ctx, motor, sessao, INTERVALO_DA_VIGIA, || {
        motor
            .consultar(
                sessao,
                "fiscal.resumo_da_fila.v1",
                &mod_fiscal::ResumoDaFilaFiscal,
            )
            .is_ok_and(|r| {
                r.pendentes
                    + r.incertas
                    + r.em_contingencia
                    + r.xmls_a_buscar
                    + r.cartas_pendentes
                    + r.nfse_na_fila
                    > 0
            })
    });
}

#[cfg(test)]
mod testes {
    use std::time::Instant;

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
