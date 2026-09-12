//! Camada 3 (organisms) — `Grade`. `docs/12-ui-ux.md` §7: tabela virtualizada, cabeçalho
//! em `superficie_2`, **linha inteira** selecionável (não uma coluna só) e realçada no
//! hover — a peça de toda tela de listagem (contas a receber/pagar, produtos, ordens,
//! notas).
//!
//! Construída sobre `egui_extras::TableBuilder` (virtualização já resolvida lá) com a
//! aparência do design system por cima.
//!
//! **Revisão de 2026-09-11** (evidência: capturas de tela reais do usuário mostrando a
//! tabela de OS com 4 colunas finas ocupando uma fração da janela 1920×1080, o resto em
//! branco): a coluna sem largura explícita usava `Column::auto()`, que dimensiona **só**
//! pelo conteúdo — nenhuma coluna reivindicava o espaço sobrando, então a tabela inteira
//! ficava estreita mesmo com a janela grande. Trocado por `Column::remainder()` (com um
//! mínimo), que consome o restante da largura disponível — exatamente o que uma coluna como
//! "Equipamento"/"Nome"/"Descrição" deveria fazer. Ganhou também: fundo diferenciado + linha
//! divisória sob o cabeçalho (antes tinha o mesmo peso visual do corpo), zebra sutil entre
//! linhas (`faint_bg_color`, já mapeado para `superficie_2` em `instalar_estilo`), moldura
//! com borda arredondada (a tabela não flutuava mais solta contra o fundo da tela) e
//! alinhamento à direita configurável por coluna (`ColunaGrade::numero`) para número/dinheiro
//! não ficarem colados à esquerda do texto.

use egui::{CursorIcon, Response, Rounding, Sense, Stroke, Ui};
use egui_extras::{Column, TableBuilder};

use crate::atoms::Rotulo;
use crate::tokens::{ativar, AlturaLinha, Espaco, Mov, Raio, TemaUi};

/// Altura do cabeçalho da grade — acompanha a escala tipográfica atual (era 28px fixo,
/// pequeno demais para o `RotuloCampo` maior da revisão de 2026-09-11).
const ALTURA_CABECALHO: f32 = 40.0;

/// Largura mínima de uma coluna sem largura fixa (`Column::remainder`), para que ela nunca
/// colapse a ponto de truncar o próprio cabeçalho num redimensionamento agressivo.
const LARGURA_MINIMA_FLEXIVEL: f32 = 140.0;

/// Alinhamento horizontal do conteúdo de uma coluna. `docs/12-ui-ux.md` §7: números e
/// dinheiro alinham à direita para escanear (e comparar) uma coluna de cima a baixo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Alinhamento {
    #[default]
    Esquerda,
    Direita,
}

/// Direção de ordenação de uma coluna — devolvida pela tela em [`Grade::ordenacao`] para
/// desenhar a seta ▲/▼ ao lado do rótulo da coluna ativa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direcao {
    /// Do menor para o maior.
    Ascendente,
    /// Do maior para o menor.
    Descendente,
}

impl Direcao {
    /// A direção oposta — a tela normalmente chama isto ao receber de volta um clique na
    /// coluna que já era a ativa (`RespostaGrade::coluna_clicada`), para alternar o sentido.
    #[must_use]
    pub const fn invertida(self) -> Self {
        match self {
            Self::Ascendente => Self::Descendente,
            Self::Descendente => Self::Ascendente,
        }
    }

    const fn seta(self) -> &'static str {
        match self {
            Self::Ascendente => "▲",
            Self::Descendente => "▼",
        }
    }
}

/// Uma coluna da grade: rótulo do cabeçalho e largura inicial (`Column::remainder` se
/// `None` — a coluna reparte o espaço sobrando da tabela).
pub struct ColunaGrade {
    /// O texto do cabeçalho.
    pub rotulo: &'static str,
    /// Largura inicial em pixels; `None` deixa a coluna repartir o espaço sobrando.
    pub largura_inicial: Option<f32>,
    alinhamento: Alinhamento,
}

impl ColunaGrade {
    #[must_use]
    /// Uma coluna que reparte o espaço sobrando da tabela (alinhada à esquerda).
    pub const fn nova(rotulo: &'static str) -> Self {
        Self {
            rotulo,
            largura_inicial: None,
            alinhamento: Alinhamento::Esquerda,
        }
    }

    #[must_use]
    /// Fixa a largura inicial (o usuário ainda pode redimensionar).
    pub const fn largura(mut self, px: f32) -> Self {
        self.largura_inicial = Some(px);
        self
    }

    #[must_use]
    /// Marca a coluna como numérica: conteúdo alinhado à direita (`docs/12-ui-ux.md` §7).
    /// Use em Nº, quantidade, dinheiro e duração — qualquer coluna que se compara na
    /// vertical.
    pub const fn numero(mut self) -> Self {
        self.alinhamento = Alinhamento::Direita;
        self
    }
}

/// A linha de uma [`Grade`] passada para o callback de `mostrar`/`desenhar`. Mesma API de
/// uso que o `egui_extras::TableRow` cru (`row.col(|ui| ...)`) — só acrescenta o alinhamento
/// por coluna, então nenhum call-site existente precisou mudar.
///
/// Três parâmetros de vida distintos de propósito: os dois primeiros são os do `TableRow`
/// interno (invariantes — vêm de `&mut Ui`, não podem ser unificados com nada), o terceiro é
/// o das colunas (emprestadas de `Grade`, escopo bem mais longo). Reaproveitar um só nome
/// para os três forçava o borrow checker a igualá-los e exigia `'static` de `colunas`.
pub struct LinhaGrade<'t, 'r, 'c> {
    linha: egui_extras::TableRow<'t, 'r>,
    colunas: &'c [ColunaGrade],
    indice_coluna: usize,
}

impl LinhaGrade<'_, '_, '_> {
    /// Desenha a próxima célula da linha, na ordem das colunas declaradas.
    pub fn col(&mut self, conteudo: impl FnOnce(&mut Ui)) -> (egui::Rect, egui::Response) {
        let alinhamento = self
            .colunas
            .get(self.indice_coluna)
            .map_or(Alinhamento::Esquerda, |c| c.alinhamento);
        self.indice_coluna += 1;
        match alinhamento {
            Alinhamento::Esquerda => self.linha.col(conteudo),
            Alinhamento::Direita => self.linha.col(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    conteudo(ui);
                });
            }),
        }
    }

    /// Marca a linha inteira como selecionada (repassa ao `TableRow` cru).
    pub fn set_selected(&mut self, selecionada: bool) {
        self.linha.set_selected(selecionada);
    }

    /// A `Response` da linha inteira (`.clicked()`, `.hovered()`...).
    #[must_use]
    pub fn response(&self) -> egui::Response {
        self.linha.response()
    }
}

/// O que aconteceu na [`Grade`] neste frame — devolvido por [`Grade::mostrar`].
///
/// `Grade` é um componente controlado: ela não reordena os próprios dados nem alterna
/// direção sozinha. `coluna_clicada` só avisa qual cabeçalho foi clicado; a tela decide a
/// nova direção (tipicamente invertendo a atual, se for a mesma coluna, `Direcao::invertida`)
/// e ordena o seu vetor de dados, devolvendo o novo estado no próximo frame via
/// [`Grade::ordenacao`] — o mesmo padrão já usado por `selecionavel`/`selecionada`.
#[derive(Debug, Clone, Copy, Default)]
pub struct RespostaGrade {
    /// Índice da linha clicada, quando a grade é `selecionavel`.
    pub linha_clicada: Option<usize>,
    /// Índice da coluna cujo cabeçalho foi clicado.
    pub coluna_clicada: Option<usize>,
}

/// Uma tabela densa, redimensionável, com seleção de linha inteira.
#[must_use]
pub struct Grade {
    colunas: Vec<ColunaGrade>,
    altura_linha: AlturaLinha,
    selecionada: Option<usize>,
    selecionavel: bool,
    ordenacao: Option<(usize, Direcao)>,
}

impl Grade {
    /// Uma grade com as colunas dadas, altura de linha confortável por padrão.
    pub fn nova(colunas: Vec<ColunaGrade>) -> Self {
        Self {
            colunas,
            altura_linha: AlturaLinha::Confortavel,
            selecionada: None,
            selecionavel: false,
            ordenacao: None,
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

    /// Marca qual coluna (índice) e direção está ordenando a tabela agora — desenha a seta
    /// ▲/▼ ao lado do rótulo dela. Componente controlado: a tela guarda esse estado e o
    /// devolve aqui a cada frame, veja [`RespostaGrade`].
    pub const fn ordenacao(mut self, o: Option<(usize, Direcao)>) -> Self {
        self.ordenacao = o;
        self
    }

    /// Desenha a grade. `total_linhas` é a contagem exata (só as linhas visíveis são
    /// montadas); `linha` preenche cada coluna na ordem declarada. Devolve o que foi
    /// clicado neste frame — linha (quando `selecionavel`) e/ou cabeçalho de coluna (quando
    /// alguma coluna foi clicada para ordenar).
    pub fn mostrar(
        self,
        ui: &mut Ui,
        total_linhas: usize,
        linha: impl FnMut(usize, &mut LinhaGrade<'_, '_, '_>),
    ) -> RespostaGrade {
        ui.scope(|ui| self.desenhar(ui, total_linhas, linha)).inner
    }

    fn desenhar(
        self,
        ui: &mut Ui,
        total_linhas: usize,
        mut linha: impl FnMut(usize, &mut LinhaGrade<'_, '_, '_>),
    ) -> RespostaGrade {
        let cores = ui.cores();

        // Moldura: a grade ganha borda + cantos arredondados próprios (antes flutuava solta
        // contra o fundo da tela, sem nenhum limite visual — uma das duas queixas mais
        // graves das capturas de tela de 2026-09-11).
        egui::Frame::none()
            .fill(cores.superficie)
            .stroke(Stroke::new(1.0_f32, cores.borda))
            .rounding(Raio::CARTAO)
            .show(ui, |ui| {
                ui.set_width(ui.available_width().max(0.0));

                // Realce de linha on-brand: hover = `superficie_hover` sutil (não o cinza
                // forte de fábrica), linha selecionada = tint da marca com contorno. Zebra
                // (`striped`) usa `faint_bg_color`, já mapeado para `superficie_2` em
                // `instalar_estilo` — as três camadas (zebra/hover/seleção) compõem porque
                // `egui_extras` pinta zebra primeiro e sobrepõe hover/seleção por cima.
                {
                    let v = ui.visuals_mut();
                    v.widgets.hovered.bg_fill = cores.superficie_hover;
                    v.selection.bg_fill = cores.rubro_ativo;
                    v.selection.stroke = Stroke::new(1.0_f32, cores.rubro);
                }

                // Fundo do cabeçalho + linha divisória, pintados **antes** da tabela: o
                // `TableBuilder` do `egui_extras` não pinta nada atrás do cabeçalho por
                // conta própria, então antes ele saía com o mesmo peso visual do corpo — a
                // outra queixa grave das capturas ("cabeçalho quase igual ao corpo").
                let topo = ui.cursor().left_top();
                let faixa_cabecalho = egui::Rect::from_min_size(
                    topo,
                    egui::vec2(ui.available_width(), ALTURA_CABECALHO),
                );
                ui.painter().rect_filled(
                    faixa_cabecalho,
                    Rounding {
                        nw: Raio::CARTAO,
                        ne: Raio::CARTAO,
                        sw: 0.0,
                        se: 0.0,
                    },
                    cores.superficie_2,
                );
                ui.painter().hline(
                    faixa_cabecalho.x_range(),
                    faixa_cabecalho.bottom(),
                    Stroke::new(1.0_f32, cores.borda),
                );

                let mut builder = TableBuilder::new(ui)
                    .striped(true)
                    .resizable(true)
                    .sense(if self.selecionavel {
                        egui::Sense::click()
                    } else {
                        egui::Sense::hover()
                    })
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Center));

                for coluna in &self.colunas {
                    builder = builder.column(coluna.largura_inicial.map_or_else(
                        || Column::remainder().at_least(LARGURA_MINIMA_FLEXIVEL),
                        Column::initial,
                    ));
                }

                let colunas = &self.colunas;
                let selecionada = self.selecionada;
                let ordenacao = self.ordenacao;
                let mut clicada = None;
                let mut coluna_clicada = None;

                builder
                    .header(ALTURA_CABECALHO, |mut cabecalho| {
                        for (indice, coluna) in colunas.iter().enumerate() {
                            // O cabeçalho acompanha o alinhamento da própria coluna — um
                            // rótulo "Total" à esquerda não guia o olho até o número que
                            // fica à direita, embaixo dele. O clique de ordenação usa o
                            // `Response` que `cabecalho_coluna` monta na mão (via
                            // `ui.interact`), não o da célula: a tabela sente só `hover`
                            // quando a grade não é `selecionavel`, e o cabeçalho precisa
                            // clicar mesmo assim.
                            cabecalho.col(|ui| {
                                if cabecalho_coluna(ui, &cores, coluna, indice, ordenacao)
                                    .clicked()
                                {
                                    coluna_clicada = Some(indice);
                                }
                            });
                        }
                    })
                    .body(|corpo| {
                        corpo.rows(self.altura_linha.pixels(), total_linhas, |mut row| {
                            let indice = row.index();
                            if self.selecionavel {
                                row.set_selected(selecionada == Some(indice));
                            }
                            let mut linha_grade = LinhaGrade {
                                linha: row,
                                colunas,
                                indice_coluna: 0,
                            };
                            linha(indice, &mut linha_grade);
                            if self.selecionavel && linha_grade.response().clicked() {
                                clicada = Some(indice);
                            }
                        });
                    });

                RespostaGrade {
                    linha_clicada: clicada,
                    coluna_clicada,
                }
            })
            .inner
    }
}

/// Desenha um cabeçalho de coluna clicável: rótulo + seta ▲/▼ quando é a coluna ativa da
/// ordenação, realce sutil no hover (`docs/12-ui-ux.md` §9: nenhum alvo clicável fica sem
/// feedback). Devolve a `Response` do próprio cabeçalho — a célula da tabela por trás só
/// sente `hover` quando a grade não é `selecionavel`, então o clique de ordenar precisa da
/// sua própria área interativa, não da da célula.
fn cabecalho_coluna(
    ui: &mut Ui,
    cores: &crate::tokens::Cores,
    coluna: &ColunaGrade,
    indice: usize,
    ordenacao: Option<(usize, Direcao)>,
) -> Response {
    let rect = ui.max_rect();
    let id = ui.id().with("ordenar");
    let resp = ui.interact(rect, id, Sense::click());

    let th = ativar(ui, id.with("hover"), resp.hovered(), Mov::RAPIDO);
    if th > 0.001_f32 {
        ui.painter()
            .rect_filled(rect, 0.0, cores.superficie_hover.gamma_multiply(th));
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }

    let direcao_ativa = ordenacao.and_then(|(i, d)| (i == indice).then_some(d));
    let cor_rotulo = direcao_ativa.map(|_| cores.rubro);

    let montar = |ui: &mut Ui| {
        ui.add_space(Espaco::E4);
        let mut rotulo = Rotulo::campo(coluna.rotulo);
        if let Some(cor) = cor_rotulo {
            rotulo = rotulo.cor(cor);
        }
        ui.add(rotulo);
        if let Some(direcao) = direcao_ativa {
            ui.add_space(2.0_f32);
            ui.add(Rotulo::campo(direcao.seta()).cor(cores.rubro));
        }
    };
    match coluna.alinhamento {
        Alinhamento::Esquerda => montar(ui),
        Alinhamento::Direita => {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), montar);
        }
    }

    resp
}
