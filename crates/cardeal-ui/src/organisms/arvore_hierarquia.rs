//! Camada 3 (organisms) — `ArvoreHierarquia`: uma árvore de registros (estrutura mercadológica,
//! plano de contas) em linhas selecionáveis, com recuo por nível e nós que recolhem.
//! `docs/12-ui-ux.md` §7.
//!
//! Os nós chegam **já em pré-ordem** (cada pai antes dos filhos) com o `nivel` de cada um
//! (1 = raiz); o componente só decide o que está visível e desenha. Quem guarda o estado de
//! recolhido é a tela (`&mut HashSet<Id>`), para sobreviver a recargas dos dados.

use std::collections::HashSet;

use cardeal_kernel::Id;
use egui::Ui;

use crate::atoms::{BotaoChevron, Rotulo};
use crate::molecules::ItemDeLista;
use crate::tokens::Espaco;

/// Um nó da árvore.
#[derive(Debug, Clone)]
pub struct NoArvore {
    /// Identidade do registro.
    pub id: Id,
    /// A profundidade: 1 = raiz.
    pub nivel: u8,
    /// O texto principal.
    pub rotulo: String,
    /// Texto secundário sob o rótulo (código, contagem…).
    pub apoio: Option<String>,
    /// Tem filhos (mostra o chevron).
    pub tem_filhos: bool,
}

/// O que aconteceu no quadro.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RespostaArvore {
    /// O nó em que se clicou (seleção).
    pub clicado: Option<Id>,
}

/// Uma árvore de linhas selecionáveis.
#[must_use]
pub struct ArvoreHierarquia<'a> {
    nos: &'a [NoArvore],
    recolhidos: &'a mut HashSet<Id>,
    selecionado: Option<Id>,
}

impl<'a> ArvoreHierarquia<'a> {
    /// A árvore dos `nos` (pré-ordem), guardando em `recolhidos` o que está fechado.
    pub fn nova(nos: &'a [NoArvore], recolhidos: &'a mut HashSet<Id>) -> Self {
        Self {
            nos,
            recolhidos,
            selecionado: None,
        }
    }

    /// O nó destacado.
    pub const fn selecionado(mut self, id: Option<Id>) -> Self {
        self.selecionado = id;
        self
    }

    /// Desenha. `traco` monta o conteúdo à direita de cada linha (valores, etiquetas).
    pub fn mostrar(self, ui: &mut Ui, mut traco: impl FnMut(&mut Ui, &NoArvore)) -> RespostaArvore {
        let mut resposta = RespostaArvore::default();
        // Enquanto `Some(n)`, os nós de nível > n estão escondidos por um ancestral fechado.
        let mut escondendo_abaixo_de: Option<u8> = None;
        for no in self.nos {
            if let Some(n) = escondendo_abaixo_de {
                if no.nivel > n {
                    continue;
                }
                escondendo_abaixo_de = None;
            }
            let fechado = no.tem_filhos && self.recolhidos.contains(&no.id);
            if fechado {
                escondendo_abaixo_de = Some(no.nivel);
            }

            let mut item = ItemDeLista::novo(no.rotulo.clone())
                .recuo(no.nivel)
                .selecionado(self.selecionado == Some(no.id));
            if let Some(a) = &no.apoio {
                item = item.subtitulo(a.clone());
            }
            let mut alternar = false;
            let linha = item.mostrar(ui, |ui| traco(ui, no));
            // O chevron ocupa o degrau reservado pelo `recuo` (um a mais que a profundidade).
            if no.tem_filhos {
                let rect = linha.rect;
                let lado = 28.0;
                let x =
                    rect.left() + Espaco::E4 + f32::from(no.nivel.saturating_sub(1)) * Espaco::E24;
                let area = egui::Rect::from_min_size(
                    egui::pos2(x - 2.0, rect.center().y - lado / 2.0),
                    egui::vec2(lado, lado),
                );
                let mut filho = ui.new_child(egui::UiBuilder::new().max_rect(area));
                let chevron = BotaoChevron::novo(false);
                let r = filho.add(if fechado {
                    chevron
                } else {
                    chevron.para_baixo()
                });
                alternar = r.clicked();
            }
            if alternar {
                if fechado {
                    self.recolhidos.remove(&no.id);
                } else {
                    self.recolhidos.insert(no.id);
                }
            } else if linha.clicked() {
                resposta.clicado = Some(no.id);
            }
        }
        if self.nos.is_empty() {
            ui.add(Rotulo::apoio("Nada cadastrado ainda."));
        }
        resposta
    }
}
