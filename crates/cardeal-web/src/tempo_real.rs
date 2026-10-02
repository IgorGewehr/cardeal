//! O tempo real no navegador: o `EventSource` nativo (reconecta sozinho e devolve o
//! `Last-Event-ID`, de graça) traduzido para os mesmos [`AvisoTempoReal`] do desktop. Mesma
//! origem: o cookie `HttpOnly` vai sozinho, nenhum token passa pelo JavaScript.

use std::cell::RefCell;
use std::rc::Rc;

use cardeal_cliente::remoto::AvisoTempoReal;
use cardeal_kernel::Id;
use cardeal_protocol::{rota_eventos, EVENTO_MUDOU, EVENTO_RECARREGAR, EVENTO_SESSAO_ENCERRADA};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast as _;
use web_sys::{EventSource, MessageEvent};

/// O acompanhamento de uma empresa; fecha a conexão ao ser solto.
pub struct Acompanhamento {
    fonte: EventSource,
    avisos: Rc<RefCell<Vec<AvisoTempoReal>>>,
    _ouvintes: Vec<Closure<dyn FnMut(MessageEvent)>>,
}

impl Acompanhamento {
    /// Conecta. `None` se o navegador recusar (sem `EventSource`): a tela segue funcionando,
    /// só sem atualização automática.
    pub fn abrir(ctx: &egui::Context, empresa: Id) -> Option<Self> {
        let fonte = EventSource::new(&rota_eventos(empresa)).ok()?;
        let avisos = Rc::new(RefCell::new(Vec::new()));
        let mut ouvintes = Vec::new();
        for nome in [EVENTO_MUDOU, EVENTO_RECARREGAR, EVENTO_SESSAO_ENCERRADA] {
            let (avisos, ctx) = (Rc::clone(&avisos), ctx.clone());
            let ouvinte = Closure::<dyn FnMut(MessageEvent)>::new(move |ev: MessageEvent| {
                let data = ev.data().as_string().unwrap_or_default();
                if let Some(a) = AvisoTempoReal::de_evento(nome, &data) {
                    avisos.borrow_mut().push(a);
                    ctx.request_repaint();
                }
            });
            fonte
                .add_event_listener_with_callback(nome, ouvinte.as_ref().unchecked_ref())
                .ok()?;
            ouvintes.push(ouvinte);
        }
        Some(Self {
            fonte,
            avisos,
            _ouvintes: ouvintes,
        })
    }

    /// Os avisos que chegaram desde o último quadro.
    pub fn drenar(&self) -> Vec<AvisoTempoReal> {
        std::mem::take(&mut *self.avisos.borrow_mut())
    }
}

impl Drop for Acompanhamento {
    fn drop(&mut self) {
        self.fonte.close();
    }
}
