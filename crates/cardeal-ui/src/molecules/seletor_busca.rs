//! Camada 2 (molecules) — `SeletorBusca`: um lookup por texto (nome, e o que mais a tela
//! quiser comparar) com resultados clicáveis abaixo do campo — substitui o
//! [`crate::molecules::SeletorOpcao`] nos pontos em que a lista é grande demais para rolar
//! num combo (cliente da OS, peça do orçamento): digitar filtra, clicar seleciona.
//!
//! Nasce da revisão de UI/UX de 2026-09-13 (abertura de OS e aplicação de peça): o backend de
//! estoque está ganhando, em paralelo, um código curto de rastreabilidade escrito num post-it
//! físico (`docs/modulos/estoque.md`) — quando esse campo existir, a tela só precisa somar o
//! código ao texto comparado em [`OpcaoBusca::subtitulo`]/no filtro; o componente em si (texto
//! → lista clicável) já fica pronto para esse uso hoje, buscando por nome.
//!
//! Componente controlado, no mesmo espírito de [`SeletorOpcao`]: a tela guarda `busca` e
//! `selecionado`, o componente só lê/escreve os dois.

use egui::{Response, Ui};

use crate::atoms::{Botao, CampoTexto, Rotulo};
use crate::molecules::ItemDeLista;
use crate::tokens::{Espaco, TemaUi};

/// Uma opção buscável: o valor selecionável, um título (comparado à busca) e um subtítulo
/// opcional (também comparado — nome + código, nome + documento…).
#[must_use]
pub struct OpcaoBusca<T> {
    valor: T,
    titulo: String,
    subtitulo: Option<String>,
}

impl<T> OpcaoBusca<T> {
    /// Uma opção só com título.
    pub fn nova(valor: T, titulo: impl Into<String>) -> Self {
        Self {
            valor,
            titulo: titulo.into(),
            subtitulo: None,
        }
    }

    /// Acrescenta o subtítulo — também entra na busca (`docs`: pensado para caber um código
    /// curto de peça ali quando o backend o expuser).
    pub fn subtitulo(mut self, subtitulo: impl Into<String>) -> Self {
        self.subtitulo = Some(subtitulo.into());
        self
    }
}

/// Um campo de busca com lista de resultados clicável — o "combo grande" do design system.
#[must_use]
pub struct SeletorBusca<'a, T> {
    rotulo: String,
    busca: &'a mut String,
    selecionado: &'a mut Option<T>,
    opcoes: Vec<OpcaoBusca<T>>,
    marcador: String,
    max_resultados: usize,
}

impl<'a, T: PartialEq + Copy> SeletorBusca<'a, T> {
    /// Um seletor com rótulo, ligado a `busca` (o texto digitado) e `selecionado`.
    pub fn novo(
        rotulo: impl Into<String>,
        busca: &'a mut String,
        selecionado: &'a mut Option<T>,
    ) -> Self {
        Self {
            rotulo: rotulo.into(),
            busca,
            selecionado,
            opcoes: Vec::new(),
            marcador: "Digite para buscar…".to_owned(),
            max_resultados: 8,
        }
    }

    /// As opções buscáveis.
    pub fn opcoes(mut self, opcoes: Vec<OpcaoBusca<T>>) -> Self {
        self.opcoes = opcoes;
        self
    }

    /// Texto de exemplo do campo de busca.
    pub fn marcador(mut self, s: impl Into<String>) -> Self {
        self.marcador = s.into();
        self
    }

    /// Quantos resultados mostrar no máximo (padrão 8) — o resto só aparece refinando a
    /// busca, não rolando uma lista enorme.
    pub const fn max_resultados(mut self, n: usize) -> Self {
        self.max_resultados = n;
        self
    }

    /// Desenha o seletor: chip da seleção atual (com "Trocar") ou campo de busca + resultados.
    pub fn mostrar(self, ui: &mut Ui) -> Response {
        let Self {
            rotulo,
            busca,
            selecionado,
            opcoes,
            marcador,
            max_resultados,
        } = self;
        let cores = ui.cores();

        ui.vertical(|ui| {
            ui.add(Rotulo::campo(&rotulo));
            ui.add_space(Espaco::E4);

            if let Some(atual) = *selecionado {
                let opcao = opcoes.iter().find(|o| o.valor == atual);
                egui::Frame::none()
                    .fill(cores.superficie_2)
                    .rounding(crate::tokens::Raio::CAMPO)
                    .inner_margin(egui::vec2(12.0_f32, 10.0_f32))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width().max(0.0));
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.add(Rotulo::interface(opcao.map_or_else(
                                    || "Selecionado".to_owned(),
                                    |o| o.titulo.clone(),
                                )));
                                if let Some(sub) = opcao.and_then(|o| o.subtitulo.clone()) {
                                    ui.add(Rotulo::campo(sub));
                                }
                            });
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.add(Botao::fantasma("Trocar").pequeno()).clicked() {
                                        *selecionado = None;
                                        busca.clear();
                                    }
                                },
                            );
                        });
                    })
                    .response
            } else {
                let resp = ui.add(CampoTexto::novo(busca).marcador(marcador));

                let termo = busca.trim().to_lowercase();
                if !termo.is_empty() {
                    let achados: Vec<&OpcaoBusca<T>> = opcoes
                        .iter()
                        .filter(|o| {
                            o.titulo.to_lowercase().contains(&termo)
                                || o.subtitulo
                                    .as_ref()
                                    .is_some_and(|s| s.to_lowercase().contains(&termo))
                        })
                        .take(max_resultados)
                        .collect();

                    ui.add_space(Espaco::E4);
                    if achados.is_empty() {
                        ui.add(Rotulo::campo("Nenhum resultado."));
                    } else {
                        egui::Frame::none()
                            .fill(cores.superficie)
                            .stroke(egui::Stroke::new(1.0_f32, cores.borda))
                            .rounding(crate::tokens::Raio::CARTAO)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width().max(0.0));
                                for opcao in achados {
                                    let item = ItemDeLista::novo(opcao.titulo.clone());
                                    let item = match &opcao.subtitulo {
                                        Some(s) => item.subtitulo(s.clone()),
                                        None => item,
                                    };
                                    if item.mostrar(ui, |_| {}).clicked() {
                                        *selecionado = Some(opcao.valor);
                                    }
                                }
                            });
                    }
                }
                resp
            }
        })
        .inner
    }
}
