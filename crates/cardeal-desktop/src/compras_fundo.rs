//! A busca de notas de compra na SEFAZ em segundo plano (`compras.buscar_notas_na_sefaz`): a
//! vigia confere, a cada minuto, se a hora da próxima consulta chegou — a SEFAZ só aceita uma
//! nova uma hora depois da última que deixou a empresa em dia — e busca. O botão "Buscar na
//! SEFAZ" de Compras dispara na hora; antes da hora, a tarefa responde quando a SEFAZ aceita.

use std::time::Duration;

use cardeal_cliente::{Motor, Sessao};
use eframe::egui;
use mod_compras::HoraDeBuscarNaSefaz;

use crate::fundo::TarefaDeFundo;

const INTERVALO_DA_VIGIA: Duration = Duration::from_secs(60);

static NOTAS_DA_SEFAZ: TarefaDeFundo = TarefaDeFundo::nova(
    mod_compras::TAREFA_BUSCAR_NA_SEFAZ,
    "compras.entrada.importar",
);

/// Dispara uma busca numa thread, se nenhuma estiver rodando. `false` se não disparou.
pub fn disparar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao) -> bool {
    NOTAS_DA_SEFAZ.disparar(ctx, motor, sessao)
}

/// Se há uma busca em andamento.
pub fn rodando() -> bool {
    NOTAS_DA_SEFAZ.rodando()
}

/// Quantas buscas já terminaram — a tela de Compras recarrega quando muda.
pub fn geracao() -> u64 {
    NOTAS_DA_SEFAZ.geracao()
}

/// O resultado da última busca.
pub fn ultimo_resultado() -> Option<Result<String, String>> {
    NOTAS_DA_SEFAZ.ultimo_resultado()
}

/// Chamada a cada quadro da sessão: uma vez por minuto, busca se a hora chegou.
pub fn vigiar(ctx: &egui::Context, motor: &Motor, sessao: &Sessao) {
    NOTAS_DA_SEFAZ.vigiar(ctx, motor, sessao, INTERVALO_DA_VIGIA, || {
        motor
            .consultar(
                sessao,
                "compras.hora_de_buscar_na_sefaz.v1",
                &HoraDeBuscarNaSefaz,
            )
            .unwrap_or(false)
    });
}
