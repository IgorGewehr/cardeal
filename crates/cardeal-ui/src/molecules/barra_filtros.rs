//! Camada 2 (molecules) — `BarraFiltros`: a barra que fica entre os indicadores e a
//! [`Grade`](crate::organisms::Grade) de toda listagem — busca por texto (com "✕" para
//! limpar) e, opcionalmente, um ou mais filtros (`SeletorOpcao`) ao lado.
//!
//! Existe porque cada tela de lista montava isto à mão (`ui.horizontal`, `set_max_width(360)` e
//! um `Campo`), e o resultado divergia: só a de clientes tinha o "✕", e na de OS o limite de
//! 360px valia para a linha **inteira**, espremendo o filtro de status contra o campo. Aqui a
//! largura da busca e a do filtro são separadas, e o comportamento é um só.

use egui::Ui;

use crate::atoms::Botao;
use crate::molecules::Campo;
use crate::tokens::Espaco;

/// Largura do campo de busca — o suficiente para "Buscar por nº, aparelho ou cliente".
const LARGURA_BUSCA: f32 = 360.0;

/// Largura de cada filtro ao lado da busca.
const LARGURA_FILTRO: f32 = 220.0;

type Filtro<'a> = Box<dyn FnOnce(&mut Ui) + 'a>;

/// A barra de busca + filtros de uma listagem.
#[must_use]
pub struct BarraFiltros<'a> {
    busca: &'a mut String,
    marcador: String,
    filtros: Vec<Filtro<'a>>,
}

impl<'a> BarraFiltros<'a> {
    /// Uma barra ligada ao texto de busca da tela.
    pub fn nova(busca: &'a mut String) -> Self {
        Self {
            busca,
            marcador: String::new(),
            filtros: Vec::new(),
        }
    }

    /// O texto de exemplo do campo ("Buscar por nome ou NCM").
    pub fn marcador(mut self, marcador: impl Into<String>) -> Self {
        self.marcador = marcador.into();
        self
    }

    /// Acrescenta um filtro à direita da busca — tipicamente um
    /// `SeletorOpcao::novo(..).sem_rotulo().mostrar(ui)`. Cada filtro recebe a sua própria coluna de
    /// [`LARGURA_FILTRO`] pixels; chame várias vezes para vários filtros.
    pub fn filtro(mut self, desenhar: impl FnOnce(&mut Ui) + 'a) -> Self {
        self.filtros.push(Box::new(desenhar));
        self
    }

    /// Desenha a barra e deixa um respiro embaixo. Devolve `true` no quadro em que o texto de
    /// busca mudou (digitado ou limpo) — para a tela que refaz a consulta no servidor.
    pub fn mostrar(self, ui: &mut Ui) -> bool {
        let Self {
            busca,
            marcador,
            filtros,
        } = self;
        let mut mudou = false;
        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.set_max_width(LARGURA_BUSCA);
                mudou |= ui.add(Campo::novo("", busca).marcador(marcador)).changed();
            });
            if !busca.is_empty() && ui.add(Botao::fantasma("✕").pequeno()).clicked() {
                busca.clear();
                mudou = true;
            }
            for filtro in filtros {
                ui.add_space(Espaco::E12);
                ui.scope(|ui| {
                    ui.set_max_width(LARGURA_FILTRO);
                    filtro(ui);
                });
            }
        });
        ui.add_space(Espaco::E12);
        mudou
    }
}
