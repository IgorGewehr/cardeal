//! # cardeal-web
//!
//! O Cardeal no navegador (ADR-0016): o mesmo `cardeal-ui` do desktop, compilado para WASM e
//! desenhado num `<canvas>` com WebGL2, falando com o `cardeal-server` pelo mesmo protocolo
//! (`cardeal_cliente::remoto::protocolo`) — só o transporte muda: `fetch`, assíncrono.
//!
//! Esta é a primeira fatia: login, escolha de empresa e a lista de clientes. As telas do
//! desktop chegam aqui quando ganharem o modo assíncrono.

#![forbid(unsafe_code)]
#![warn(missing_docs, clippy::pedantic)]
#![allow(clippy::module_name_repetitions, clippy::must_use_candidate)]

#[cfg(target_arch = "wasm32")]
mod app;
#[cfg(target_arch = "wasm32")]
mod rede;
#[cfg(target_arch = "wasm32")]
mod telas;
#[cfg(target_arch = "wasm32")]
mod tempo_real;

/// Liga o app ao `<canvas id="cardeal">` da página.
///
/// # Errors
/// O canvas não existe ou o WebGL2 não está disponível.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn iniciar() -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsCast as _;
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id("cardeal"))
        .ok_or("canvas #cardeal ausente")?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    contexto_enxuto(&canvas)?;
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| Ok(Box::new(app::App::novo(&cc.egui_ctx)))),
        )
        .await
}

/// Cria o contexto WebGL2 **antes** do eframe, com os atributos que ele não expõe — o
/// navegador devolve este mesmo contexto quando o eframe pedir o seu:
///
/// - `antialias: false` — o egui já suaviza as bordas por feathering; o MSAA 4× padrão do
///   navegador quadruplicaria o buffer de tela à toa.
/// - `depth`/`stencil: false` — interface 2D não usa nenhum dos dois.
/// - `powerPreference: "low-power"` — GPU integrada: menos energia, menos calor no notebook.
#[cfg(target_arch = "wasm32")]
fn contexto_enxuto(canvas: &web_sys::HtmlCanvasElement) -> Result<(), wasm_bindgen::JsValue> {
    use wasm_bindgen::JsValue;
    let opcoes = js_sys::Object::new();
    for (chave, valor) in [
        ("antialias", JsValue::FALSE),
        ("depth", JsValue::FALSE),
        ("stencil", JsValue::FALSE),
        ("premultipliedAlpha", JsValue::TRUE),
        ("preserveDrawingBuffer", JsValue::FALSE),
        ("powerPreference", JsValue::from_str("low-power")),
    ] {
        js_sys::Reflect::set(&opcoes, &JsValue::from_str(chave), &valor)?;
    }
    canvas.get_context_with_context_options("webgl2", &opcoes)?;
    Ok(())
}
