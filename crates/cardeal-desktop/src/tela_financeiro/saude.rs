//! O cartão "Saúde financeira" da Visão geral: o score de 0 a 100, o semáforo e o que mais
//! puxa para baixo — com a dica do que fazer.

use super::*;
use cardeal_kernel::Percentual;
use mod_financeiro::saude::{Saude, Semaforo};
use mod_financeiro::SaudeFinanceira;

impl EstadoTelaFinanceiro {
    /// Carrega o score. O giro vem do estoque (outro módulo): se o usuário não tem acesso a ele,
    /// o sinal simplesmente sai do cálculo.
    pub(super) fn carregar_saude(&mut self, motor: &Motor, sessao: &Sessao) {
        let giro = motor
            .consultar(
                sessao,
                "estoque.analise_de_estoque.v1",
                &mod_estoque::AnaliseDeEstoque { dias: 90 },
            )
            .ok()
            .and_then(|itens| {
                let saidas: i64 = itens.iter().map(|i| i.valor_saidas.em_centavos()).sum();
                let estoque: i64 = itens.iter().map(|i| i.valor_em_estoque.em_centavos()).sum();
                // Janela de 90 dias → por mês (÷ 3).
                (estoque > 0).then(|| Percentual::da_razao(saidas / 3, estoque))
            });
        self.saude = motor
            .consultar(
                sessao,
                "financeiro.saude.v1",
                &SaudeFinanceira { giro_mensal: giro },
            )
            .ok();
    }
}

fn tom(s: Semaforo) -> Tom {
    match s {
        Semaforo::Verde => Tom::Positivo,
        Semaforo::Amarelo => Tom::Atencao,
        Semaforo::Vermelho => Tom::Negativo,
    }
}

fn texto(s: Semaforo) -> &'static str {
    match s {
        Semaforo::Verde => "saudável",
        Semaforo::Amarelo => "atenção",
        Semaforo::Vermelho => "em risco",
    }
}

pub(super) fn cartao_saude(ui: &mut egui::Ui, saude: &Saude) {
    Painel::novo()
        .compacto()
        .realce(tom(saude.semaforo))
        .mostrar(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(Rotulo::sobrelinha("Saúde financeira"));
                ui.add_space(Espaco::E8);
                ui.add(Etiqueta::nova(texto(saude.semaforo), tom(saude.semaforo)));
            });
            ui.horizontal(|ui| {
                ui.add(Rotulo::novo(
                    cardeal_ui::tokens::Papel::ValorDestaque,
                    format!("{}", saude.score),
                ));
                ui.add(Rotulo::apoio(" de 100"));
            });
            ui.add_space(Espaco::E8);
            ui.columns(5, |c| {
                for (i, s) in saude.sinais.iter().enumerate().take(5) {
                    c[i].add(Rotulo::campo(s.nome.clone()));
                    match s.pontos {
                        Some(p) => {
                            c[i].add(Rotulo::interface(format!("{p} pts")));
                        }
                        None => {
                            c[i].add(Rotulo::interface("sem dados").cor(c[i].cores().texto_fraco));
                        }
                    }
                    c[i].add(Rotulo::apoio(s.medida.clone()));
                }
            });
            let fracos = saude.puxando_para_baixo();
            if let Some(pior) = fracos.first() {
                ui.add_space(Espaco::E8);
                ui.add(Rotulo::apoio(format!(
                    "O que mais puxa para baixo: {}. {}",
                    pior.nome, pior.dica
                )));
            }
        });
    ui.add_space(Espaco::E12);
}
