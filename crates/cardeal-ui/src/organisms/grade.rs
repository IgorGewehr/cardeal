//! Camada 3 (organisms) — `Grade`. `docs/12-ui-ux.md` §7: tabela virtualizada, cabeçalho
//! em `superficie_2`, **linha inteira** selecionável (não uma coluna só) e realçada no
//! hover — a peça de toda tela de listagem (contas a receber/pagar, produtos, ordens,
//! notas).
//!
//! Construída sobre `egui_extras::TableBuilder` (virtualização já resolvida lá) com a
//! aparência do design system por cima.

use egui::Ui;
use egui_extras::{Column, TableBuilder};

use crate::atoms::Rotulo;
use crate::tokens::{AlturaLinha, TemaUi};

/// Uma coluna da grade: rótulo do cabeçalho e largura inicial (`Column::auto` se `None`).
pub struct ColunaGrade {
    /// O texto do cabeçalho.
    pub rotulo: &'static str,
    /// Largura inicial em pixels; `None` deixa a coluna se ajustar ao conteúdo.
    pub largura_inicial: Option<f32>,
}

impl ColunaGrade {
    #[must_use]
    /// Uma coluna com largura automática.
    pub const fn nova(rotulo: &'static str) -> Self {
        Self {
            rotulo,
            largura_inicial: None,
        }
    }

    #[must_use]
    /// Fixa a largura inicial (o usuário ainda pode redimensionar).
    pub const fn largura(mut self, px: f32) -> Self {
        self.largura_inicial = Some(px);
        self
    }
}

/// Uma tabela densa, redimensionável, com seleção de linha inteira.
#[must_use]
pub struct Grade {
    colunas: Vec<ColunaGrade>,
    altura_linha: AlturaLinha,
    selecionada: Option<usize>,
    selecionavel: bool,
}

impl Grade {
    /// Uma grade com as colunas dadas, altura de linha confortável por padrão.
    pub fn nova(colunas: Vec<ColunaGrade>) -> Self {
        Self {
            colunas,
            altura_linha: AlturaLinha::Confortavel,
            selecionada: None,
            selecionavel: false,
        }
    }

    /// Sobrescreve a altura de linha padrão (`docs/12-ui-ux.md` §4).
    pub const fn altura_linha(mut self, a: AlturaLinha) -> Self {
        self.altura_linha = a;
        self
    }

    /// Torna as linhas clicáveis e destaca `indice` (se algum). `mostrar` passa a devolver
    /// o índice da linha clicada.
    pub const fn selecionavel(mut self, indice: Option<usize>) -> Self {
        self.selecionavel = true;
        self.selecionada = indice;
        self
    }

    /// Desenha a grade. `total_linhas` é a contagem exata (só as linhas visíveis são
    /// montadas); `linha` preenche cada coluna na ordem declarada. Devolve o índice da
    /// linha clicada, quando a grade é `selecionavel`.
    pub fn mostrar(
        self,
        ui: &mut Ui,
        total_linhas: usize,
        linha: impl FnMut(usize, &mut egui_extras::TableRow<'_, '_>),
    ) -> Option<usize> {
        ui.scope(|ui| self.desenhar(ui, total_linhas, linha)).inner
    }

    fn desenhar(
        self,
        ui: &mut Ui,
        total_linhas: usize,
        mut linha: impl FnMut(usize, &mut egui_extras::TableRow<'_, '_>),
    ) -> Option<usize> {
        // Realce de linha on-brand: hover = `superficie_hover` sutil (não o cinza forte de
        // fábrica), linha selecionada = tint da marca com contorno. O `egui_extras` já pinta
        // esses fundos por linha — aqui só troca as cores (num `scope`, para não vazar para
        // o resto da tela).
        let cores = ui.cores();
        {
            let v = ui.visuals_mut();
            v.widgets.hovered.bg_fill = cores.superficie_hover;
            v.selection.bg_fill = cores.rubro_ativo;
            v.selection.stroke = egui::Stroke::new(1.0_f32, cores.rubro);
        }

        let mut builder = TableBuilder::new(ui)
            .striped(false)
            .resizable(true)
            .sense(if self.selecionavel {
                egui::Sense::click()
            } else {
                egui::Sense::hover()
            })
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center));

        for coluna in &self.colunas {
            builder = builder.column(
                coluna
                    .largura_inicial
                    .map_or_else(Column::auto, Column::initial),
            );
        }

        let colunas = &self.colunas;
        let selecionada = self.selecionada;
        let mut clicada = None;

        builder
            .header(28.0, |mut cabecalho| {
                for coluna in colunas {
                    cabecalho.col(|ui| {
                        ui.add(Rotulo::campo(coluna.rotulo));
                    });
                }
            })
            .body(|corpo| {
                corpo.rows(self.altura_linha.pixels(), total_linhas, |mut row| {
                    let indice = row.index();
                    if self.selecionavel {
                        row.set_selected(selecionada == Some(indice));
                    }
                    linha(indice, &mut row);
                    if self.selecionavel && row.response().clicked() {
                        clicada = Some(indice);
                    }
                });
            });

        clicada
    }
}
