//! Renderiza um cenário fictício do PDV e salva um PNG — conferência visual sem banco.
//!
//! ```text
//! cargo run -p cardeal-desktop --example pdv_demo --features demo -- venda /tmp/pdv.png [escuro]
//! ```
//!
//! Cenários: veja `tela_pdv::demo::CENARIOS`. Sem o argumento de saída, abre a janela e
//! deixa aberta (para mexer com o teclado).

// O exemplo só desenha: as ações que falam com o motor ficam sem uso aqui.
#[allow(dead_code)]
#[path = "../src/tela_pdv/mod.rs"]
mod tela_pdv;

use cardeal_ui::tokens::{instalar_estilo, instalar_fontes, Tema};
use eframe::egui;

/// Quadros até o layout assentar (grade, animações) antes de pedir a captura.
const QUADROS_ANTES_DA_CAPTURA: u32 = 12;

struct App {
    estado: tela_pdv::EstadoTelaPdv,
    destino: Option<std::path::PathBuf>,
    quadros: u32,
    pediu: bool,
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            tela_pdv::demo::desenhar_quadro(ui, &mut self.estado);
        });

        let Some(destino) = &self.destino else { return };
        self.quadros += 1;
        if self.quadros >= QUADROS_ANTES_DA_CAPTURA && !self.pediu {
            self.pediu = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        let captura = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = captura {
            let [w, h] = img.size;
            image::save_buffer(
                destino,
                img.as_raw(),
                u32::try_from(w).unwrap_or(0),
                u32::try_from(h).unwrap_or(0),
                image::ColorType::Rgba8,
            )
            .expect("gravando o PNG");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cenario = args.first().map_or("venda", String::as_str);
    let Some(estado) = tela_pdv::demo::estado(cenario) else {
        eprintln!(
            "cenário desconhecido: {cenario}\nopções: {:?}",
            tela_pdv::demo::CENARIOS
        );
        std::process::exit(2);
    };
    let destino = args.get(1).map(std::path::PathBuf::from);
    let tema = if args.get(2).is_some_and(|a| a == "escuro") {
        Tema::Escuro
    } else {
        Tema::Claro
    };

    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PDV — demonstração",
        opcoes,
        Box::new(move |cc| {
            instalar_fontes(&cc.egui_ctx);
            instalar_estilo(&cc.egui_ctx, tema);
            Ok(Box::new(App {
                estado,
                destino,
                quadros: 0,
                pediu: false,
            }))
        }),
    )
}
