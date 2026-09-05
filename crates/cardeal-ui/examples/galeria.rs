//! Galeria viva do design system Rubro — `docs/12-ui-ux.md` §7.
//!
//! `cargo run -p cardeal-ui --example galeria`. Mostra cada camada já construída para
//! conferência visual — o mais próximo de um "teste" de pixels antes de screenshot-diff.

use cardeal_modkit::Icone;
use cardeal_ui::atoms::{desenhar_icone, superficie_clicavel, Botao, CampoTexto, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{CabecalhoTela, Campo, CartaoKpi, EstadoVazio, LinhaDeAcao, Severidade};
use cardeal_ui::tokens::{instalar_estilo, instalar_fontes, Espaco, Rubro, Tema, TemaUi};
use cardeal_kernel::Dinheiro;
use eframe::egui;

fn main() -> eframe::Result<()> {
    eframe::run_native(
        "Galeria Rubro",
        eframe::NativeOptions::default(),
        Box::new(|cc| {
            instalar_fontes(&cc.egui_ctx);
            instalar_estilo(&cc.egui_ctx, Tema::Claro);
            Ok(Box::new(Galeria::default()))
        }),
    )
}

#[derive(Default)]
struct Galeria {
    tema: Tema,
    campo: String,
    campo_erro: String,
    item_ativo: usize,
}

impl eframe::App for Galeria {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            CabecalhoTela::novo("Galeria Rubro").mostrar(ui, |ui| {
                if ui.add(Botao::secundario("Alternar tema").atalho("Ctrl+Shift+D")).clicked() {
                    self.tema = self.tema.alternado();
                    instalar_estilo(ui.ctx(), self.tema);
                }
            });

            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.add(Rotulo::titulo_secao("Botões"));
                ui.horizontal_wrapped(|ui| {
                    ui.add(Botao::primario("Finalizar venda").atalho("F2"));
                    ui.add(Botao::secundario("Cancelar"));
                    ui.add(Botao::fantasma("Ver detalhes"));
                    ui.add(Botao::destrutivo("Excluir"));
                    ui.add(Botao::primario("Desabilitado").habilitado(false));
                });
                ui.add_space(Espaco::E8);
                ui.add(Botao::primario("Botão de largura cheia").preenche_largura());

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Tipografia"));
                ui.add(Rotulo::titulo_tela("Título de tela"));
                ui.add(Rotulo::titulo_secao("Título de seção"));
                ui.add(Rotulo::campo("Rótulo de campo"));
                ui.add(Rotulo::interface("Texto de interface padrão"));
                ui.add(ValorDinheiro::novo(Dinheiro::reais(1234)).com_sinal());
                ui.add(ValorDinheiro::novo(Dinheiro::reais(-560)).com_sinal());
                ui.add(Rotulo::codigo("35260912345678901234550010000123451987654321"));

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Campos"));
                ui.add(CampoTexto::novo(&mut self.campo).rotulo("Campo simples").marcador("digite algo"));
                ui.add_space(Espaco::E8);
                ui.add(
                    Campo::novo("Campo com erro", &mut self.campo_erro)
                        .erro(Some("CNPJ inválido — 14 dígitos")),
                );

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Cartões de KPI"));
                ui.columns(3, |cols| {
                    cols[0].add(CartaoKpi::novo("A receber", Dinheiro::reais(12480)).variacao("7 títulos"));
                    cols[1].add(CartaoKpi::novo("A pagar", Dinheiro::reais(-8420)).variacao("3 títulos"));
                    cols[2].add(CartaoKpi::novo("Saldo", Dinheiro::reais(48230)).variacao("▲ 4,2% vs ontem"));
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Linha de ação"));
                LinhaDeAcao::novo(Severidade::Critica, "3 títulos vencem hoje", "Cobrar")
                    .valor(Dinheiro::reais(8420))
                    .subtitulo("Fornecedor Alfa, Beta, Gama")
                    .mostrar(ui);

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Navegação (linha clicável)"));
                for (i, nome) in ["Pulso", "Ordens de Serviço", "Estoque"].iter().enumerate() {
                    if superficie_clicavel(ui, self.item_ativo == i, 40.0, |ui| {
                        desenhar_icone(ui, Icone::Estoque, 18.0, ui.cores().texto);
                        ui.add_space(Espaco::E12);
                        ui.add(Rotulo::interface(*nome));
                    })
                    .clicked()
                    {
                        self.item_ativo = i;
                    }
                }

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Estado vazio"));
                EstadoVazio::novo(Icone::Estoque, "Nenhum produto cadastrado ainda.")
                    .acao("Cadastrar produto")
                    .mostrar(ui);

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Paleta Rubro"));
                ui.horizontal(|ui| {
                    for cor in [
                        Rubro::R50, Rubro::R100, Rubro::R200, Rubro::R300, Rubro::R400,
                        Rubro::R500, Rubro::R600, Rubro::R700, Rubro::R800, Rubro::R900,
                    ] {
                        let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(32.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 4.0, cor);
                    }
                });
            });
        });
    }
}
