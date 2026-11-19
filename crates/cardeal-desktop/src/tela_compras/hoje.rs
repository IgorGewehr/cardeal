//! O painel **"Para resolver hoje"** no topo de Compras: o que espera alguém — notas a
//! conferir, pedidos atrasados, rascunhos sem aprovar e peças de OS esperando compra —, cada um
//! com o botão que leva aonde se resolve. Some quando não há nada.

use super::*;

/// O que está pendente.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct Pendencias {
    pub(super) notas_a_conferir: usize,
    pub(super) pedidos_atrasados: usize,
    pub(super) rascunhos: usize,
    pub(super) pecas_de_os: usize,
}

impl Pendencias {
    pub(super) const fn vazia(&self) -> bool {
        self.notas_a_conferir == 0
            && self.pedidos_atrasados == 0
            && self.rascunhos == 0
            && self.pecas_de_os == 0
    }
}

/// O que o usuário pediu no painel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IrPara {
    Notas,
    Pedidos,
    /// Um pedido novo já com o radar de reposição (estoque + peças das OS).
    PedidoDeReposicao,
}

/// Conta as pendências do que a tela já carregou.
pub(super) fn contar(estado: &EstadoTelaCompras, hoje: Data) -> Pendencias {
    let (pedidos_atrasados, rascunhos) = pedidos::resumo(&estado.pedidos, hoje);
    Pendencias {
        notas_a_conferir: estado
            .notas
            .iter()
            .filter(|n| {
                matches!(
                    n.estado,
                    EstadoNotaEntrada::AConferir | EstadoNotaEntrada::Conferida
                )
            })
            .count(),
        pedidos_atrasados,
        rascunhos,
        pecas_de_os: estado.pecas_de_os_esperando,
    }
}

pub(super) fn painel(ui: &mut egui::Ui, p: Pendencias) -> Option<IrPara> {
    if p.vazia() {
        return None;
    }
    let mut ir = None;
    Painel::novo()
        .realce(Tom::Atencao)
        .titulo("Para resolver hoje")
        .mostrar(ui, |ui| {
            let mut linha = |ui: &mut egui::Ui, n: usize, texto: &str, botao: &str, destino| {
                if n == 0 {
                    return;
                }
                ui.horizontal(|ui| {
                    ui.add(Etiqueta::atencao(n.to_string()));
                    ui.add(Rotulo::interface(texto));
                    if ui.add(Botao::fantasma(botao).pequeno()).clicked() {
                        ir = Some(destino);
                    }
                });
            };
            linha(
                ui,
                p.notas_a_conferir,
                "nota(s) de entrada a conferir e confirmar",
                "Ver notas",
                IrPara::Notas,
            );
            linha(
                ui,
                p.pedidos_atrasados,
                "pedido(s) com a entrega combinada vencida",
                "Ver pedidos",
                IrPara::Pedidos,
            );
            linha(
                ui,
                p.rascunhos,
                "pedido(s) em rascunho, sem aprovar",
                "Ver pedidos",
                IrPara::Pedidos,
            );
            linha(
                ui,
                p.pecas_de_os,
                "peça(s) de OS esperando compra",
                "Montar pedido",
                IrPara::PedidoDeReposicao,
            );
        });
    ui.add_space(Espaco::E12);
    ir
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn sem_pendencia_o_painel_some() {
        assert!(Pendencias::default().vazia());
        assert!(!Pendencias {
            pecas_de_os: 1,
            ..Pendencias::default()
        }
        .vazia());
    }
}
