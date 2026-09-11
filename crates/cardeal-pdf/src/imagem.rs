//! Decodifica os bytes de uma logo (PNG/JPEG) para uma [`Imagem`] RGB pronta para embutir,
//! achatando transparência sobre branco e limitando a resolução.

use image::GenericImageView;

use crate::documento::Imagem;
use crate::ErroPdf;

/// Largura máxima da logo depois de decodificada — mais que isto é reamostrado para baixo,
/// para o PDF não carregar um bitmap gigante à toa.
const LARGURA_MAX: u32 = 720;

/// Decodifica PNG/JPEG para RGB8, compondo qualquer transparência sobre branco.
///
/// # Errors
/// [`ErroPdf::Logo`] se os bytes não são uma imagem reconhecível.
pub fn decodificar_logo(bytes: &[u8]) -> Result<Imagem, ErroPdf> {
    let imagem = image::load_from_memory(bytes).map_err(|e| ErroPdf::Logo(e.to_string()))?;

    let imagem = if imagem.width() > LARGURA_MAX {
        let altura = (u64::from(LARGURA_MAX) * u64::from(imagem.height())
            / u64::from(imagem.width().max(1))) as u32;
        imagem.resize(
            LARGURA_MAX,
            altura.max(1),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        imagem
    };

    let (largura, altura) = imagem.dimensions();
    let rgba = imagem.to_rgba8();
    let mut rgb = Vec::with_capacity((largura * altura * 3) as usize);
    for px in rgba.pixels() {
        let [r, g, b, a] = px.0;
        let mistura = |canal: u8| -> u8 {
            let c = f32::from(canal) / 255.0;
            let alfa = f32::from(a) / 255.0;
            ((c * alfa + (1.0 - alfa)) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        };
        rgb.push(mistura(r));
        rgb.push(mistura(g));
        rgb.push(mistura(b));
    }
    Ok(Imagem::rgb8(largura, altura, rgb))
}
