//! Camada 1 (atoms) — `CodigoQr`: um QR Code desenhado em módulos — o que o celular lê (o
//! aplicativo autenticador na ativação da verificação em duas etapas, um Pix…).
//!
//! Sempre escuro sobre claro, com a margem de 4 módulos que o padrão pede, nos dois temas: o
//! leitor de QR do celular falha com o código invertido (claro sobre escuro) do tema escuro —
//! por isso as duas cores são fixas, não tokens.

// Módulo de desenho: índices e contagens de módulos viram `f32`.
#![allow(clippy::cast_precision_loss)]

use egui::{Color32, Pos2, Rect, Response, Sense, Ui, Vec2, Widget};
use qrcodegen::{QrCode, QrCodeEcc};

/// O escuro dos módulos (não um token: o QR precisa do contraste máximo em qualquer tema).
const ESCURO: Color32 = Color32::from_rgb(17, 17, 20);
/// O claro do fundo e da margem.
const CLARO: Color32 = Color32::WHITE;
/// A margem que o padrão pede, em módulos.
const MARGEM: i32 = 4;

/// Um QR Code de um texto.
#[must_use]
pub struct CodigoQr<'a> {
    texto: &'a str,
    lado: f32,
}

impl<'a> CodigoQr<'a> {
    /// O QR Code do texto, com 200 px de lado.
    pub const fn novo(texto: &'a str) -> Self {
        Self { texto, lado: 200.0 }
    }

    /// O lado, em pixels (com a margem).
    pub const fn lado(mut self, px: f32) -> Self {
        self.lado = px;
        self
    }
}

impl Widget for CodigoQr<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(self.lado), Sense::hover());
        let Ok(qr) = QrCode::encode_text(self.texto, QrCodeEcc::Medium) else {
            return resp;
        };
        if !ui.is_rect_visible(rect) {
            return resp;
        }
        let n = qr.size();
        let modulo = self.lado / (n + 2 * MARGEM) as f32;
        let painter = ui.painter();
        painter.rect_filled(rect, 0.0, CLARO);
        for y in 0..n {
            for x in 0..n {
                if qr.get_module(x, y) {
                    let canto = Pos2::new(
                        rect.min.x + (x + MARGEM) as f32 * modulo,
                        rect.min.y + (y + MARGEM) as f32 * modulo,
                    );
                    // Meio pixel a mais junta os módulos vizinhos sem fresta.
                    painter.rect_filled(
                        Rect::from_min_size(canto, Vec2::splat(modulo + 0.5)),
                        0.0,
                        ESCURO,
                    );
                }
            }
        }
        resp.on_hover_text(self.texto)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_qr_de_um_otpauth_cabe_e_desenha_sem_panico() {
        let uri = "otpauth://totp/Cardeal:maria%40loja.com?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Cardeal";
        assert!(QrCode::encode_text(uri, QrCodeEcc::Medium).is_ok());
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.add(CodigoQr::novo(uri).lado(180.0));
            });
        });
    }
}
