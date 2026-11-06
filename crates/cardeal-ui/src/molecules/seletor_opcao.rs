//! Camada 2 (molecules) — `SeletorOpcao`: um dropdown no estilo do design system, para
//! substituir o `egui::ComboBox::from_label` cru das telas (rótulo à direita, largura de fábrica,
//! tipografia fora do sistema). `docs/12-ui-ux.md` §7.
//!
//! Genérico sobre o tipo do valor selecionado (`Id`, um enum, etc.), desde que seja
//! `PartialEq + Copy`.
//!
//! Com mais de [`LIMIAR_BUSCA`] opções, o painel abre com uma **caixa de busca já focada**:
//! digitar "a" deixa só o que começa com "a", "as" só o que começa com "as" (sem acento nem
//! caixa); depois vêm as opções em que alguma palavra começa assim. Setas escolhem, Enter
//! confirma. Pedido do usuário em 2026-11-06: o filtro de categoria do Financeiro, com as
//! dezenas de contas da gestão financeira, virava caça-palavras.
//!
//! O botão e o painel são desenhados aqui, não pelo `egui::ComboBox`: o do egui fecha em
//! qualquer clique — inclusive na própria caixa de busca. O desenho do botão segue o dele
//! (mesmos `WidgetVisuals`, padding e triângulo), então os seletores curtos não mudam.

use egui::{
    AboveOrBelow, Align2, Id, Key, PopupCloseBehavior, Rect, Response, ScrollArea, Sense, Shape,
    Stroke, TextWrapMode, Ui, Vec2,
};

use crate::atoms::{moldura_foco_campo, CampoTexto, Rotulo};
use crate::tokens::{Espaco, Papel, TemaUi};

/// A partir de quantas opções o painel ganha a caixa de busca.
pub const LIMIAR_BUSCA: usize = 8;

/// Altura de cada opção no painel — mais baixa que a de um campo: numa lista longa cabem nove
/// de uma vez, não cinco.
const ALTURA_ITEM: f32 = 32.0;
/// Espaço entre as opções no painel.
const ESPACO_ITEM: f32 = 2.0;
/// Quantas opções o painel mostra antes de rolar.
const ITENS_VISIVEIS: f32 = 9.0;

/// A altura da lista para `n` opções: a de todas, até [`ITENS_VISIVEIS`]. Fica fixa enquanto o
/// painel está aberto — filtrar não encolhe o painel, então a caixa de busca não sai do lugar.
#[allow(clippy::cast_precision_loss)]
fn altura_da_lista(n: usize) -> f32 {
    (n as f32).min(ITENS_VISIVEIS) * (ALTURA_ITEM + ESPACO_ITEM)
}

/// Um seletor de uma opção entre várias.
#[must_use]
pub struct SeletorOpcao<'a, T> {
    rotulo: String,
    selecionado: &'a mut Option<T>,
    opcoes: Vec<(T, String)>,
    placeholder: String,
    com_rotulo: bool,
}

/// O que o painel lembra enquanto está aberto (some ao fechar).
#[derive(Clone, Default)]
struct Busca {
    texto: String,
    /// O painel já abriu (o foco vai para a busca só no primeiro quadro).
    aberta: bool,
    /// A posição destacada na lista filtrada (setas e Enter).
    realce: usize,
    /// A pessoa usou as setas (o destaque aparece mesmo sem texto).
    navegou: bool,
}

impl<'a, T: PartialEq + Copy> SeletorOpcao<'a, T> {
    /// Um seletor com rótulo, ligado a `selecionado`.
    pub fn novo(rotulo: impl Into<String>, selecionado: &'a mut Option<T>) -> Self {
        Self {
            rotulo: rotulo.into(),
            selecionado,
            opcoes: Vec::new(),
            placeholder: "Selecione...".to_owned(),
            com_rotulo: true,
        }
    }

    /// Acrescenta uma opção.
    pub fn opcao(mut self, valor: T, texto: impl Into<String>) -> Self {
        self.opcoes.push((valor, texto.into()));
        self
    }

    /// Acrescenta várias opções de uma vez.
    pub fn opcoes<I, S>(mut self, it: I) -> Self
    where
        I: IntoIterator<Item = (T, S)>,
        S: Into<String>,
    {
        self.opcoes
            .extend(it.into_iter().map(|(v, s)| (v, s.into())));
        self
    }

    /// Não desenha o rótulo acima do combo — para barras de ferramentas, onde o valor
    /// selecionado já diz do que se trata ("Todos os recursos"). O rótulo continua sendo o
    /// identificador interno do widget, então deve ser único.
    pub const fn sem_rotulo(mut self) -> Self {
        self.com_rotulo = false;
        self
    }

    /// Texto mostrado quando nada está selecionado.
    pub fn placeholder(mut self, s: impl Into<String>) -> Self {
        self.placeholder = s.into();
        self
    }

    /// Desenha o seletor. Devolve a `Response` do botão (com `changed()` quando a escolha
    /// mudou).
    pub fn mostrar(self, ui: &mut Ui) -> Response {
        let Self {
            rotulo,
            selecionado,
            opcoes,
            placeholder,
            com_rotulo,
        } = self;
        let cores = ui.cores();

        ui.vertical(|ui| {
            if com_rotulo {
                ui.add(Rotulo::campo(&rotulo));
                ui.add_space(Espaco::E4);
            }

            let atual = selecionado
                .and_then(|s| opcoes.iter().find(|(v, _)| *v == s))
                .map_or_else(|| placeholder.clone(), |(_, t)| t.clone());
            let cor_texto = if selecionado.is_none() {
                cores.texto_fraco
            } else {
                cores.texto
            };

            let id = ui.make_persistent_id(("seletor", rotulo.as_str()));
            let id_popup = id.with("popup");
            let id_busca = id.with("busca");
            let id_acima = id.with("acima");
            let aberto = ui.memory(|m| m.is_popup_open(id_popup));
            let com_busca = opcoes.len() > LIMIAR_BUSCA;
            let altura_lista = altura_da_lista(opcoes.len());
            let largura = (ui.available_width() - Espaco::E8).max(60.0_f32);
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(largura, ui.spacing().interact_size.y),
                Sense::hover(),
            );
            let mut resp = ui.interact(rect, id, Sense::click());
            // Abre para cima quando não cabe embaixo (como o combo do egui), e o lado fica o
            // mesmo enquanto está aberto: decidido pela altura do painel, que não muda ao filtrar.
            let acima = if aberto {
                ui.data(|d| d.get_temp::<bool>(id_acima)).unwrap_or(false)
            } else {
                let busca = if com_busca {
                    ui.spacing().interact_size.y + Espaco::E8
                } else {
                    0.0
                };
                let painel = altura_lista + busca + Espaco::E16;
                let acima = rect.bottom() + painel >= ui.ctx().screen_rect().bottom();
                ui.data_mut(|d| d.insert_temp(id_acima, acima));
                acima
            };
            let lado = if acima {
                AboveOrBelow::Above
            } else {
                AboveOrBelow::Below
            };

            pintar_botao(ui, &resp, aberto, lado, &atual, cor_texto);
            if resp.clicked() {
                ui.memory_mut(|m| m.toggle_popup(id_popup));
            }
            // Mesmo anel de foco responsivo de um `Campo`.
            moldura_foco_campo(ui, &resp, Vec2::ZERO, None);

            let mostrou = egui::popup::popup_above_or_below_widget(
                ui,
                id_popup,
                &resp,
                lado,
                PopupCloseBehavior::CloseOnClickOutside,
                |ui| lista(ui, id_busca, com_busca, altura_lista, &opcoes, *selecionado),
            );
            match mostrou {
                Some(Some(valor)) => {
                    if *selecionado != Some(valor) {
                        *selecionado = Some(valor);
                        resp.mark_changed();
                    }
                    ui.memory_mut(egui::Memory::close_popup);
                    ui.data_mut(|d| d.remove::<Busca>(id_busca));
                }
                Some(None) => {}
                // Fechado: a próxima abertura começa sem filtro.
                None => ui.data_mut(|d| d.remove::<Busca>(id_busca)),
            }
            resp
        })
        .inner
    }
}

/// O botão do seletor, no desenho do combo do egui: fundo e borda do estado do widget, o
/// texto à esquerda (cortado antes do triângulo) e o triângulo para o lado em que o painel abre.
fn pintar_botao(
    ui: &Ui,
    resp: &Response,
    aberto: bool,
    lado: AboveOrBelow,
    texto: &str,
    cor: egui::Color32,
) {
    let rect = resp.rect;
    let margem = ui.spacing().button_padding;
    if ui.is_rect_visible(rect) {
        let visuais = if aberto {
            &ui.visuals().widgets.open
        } else {
            ui.style().interact(resp)
        };
        let pintor = ui.painter();
        pintor.rect(
            rect.expand(visuais.expansion),
            visuais.rounding,
            visuais.weak_bg_fill,
            visuais.bg_stroke,
        );
        let miolo = rect.shrink2(margem);
        let lado_icone = ui.spacing().icon_width;
        let icone = Align2::RIGHT_CENTER.align_size_within_rect(Vec2::splat(lado_icone), miolo);
        let t = Rect::from_center_size(
            icone.center(),
            Vec2::new(icone.width() * 0.7, icone.height() * 0.45),
        );
        let pontos = match lado {
            AboveOrBelow::Above => vec![t.left_bottom(), t.right_bottom(), t.center_top()],
            AboveOrBelow::Below => vec![t.left_top(), t.right_top(), t.center_bottom()],
        };
        pintor.add(Shape::convex_polygon(
            pontos,
            visuais.fg_stroke.color,
            Stroke::NONE,
        ));
        let area_texto = Rect::from_min_max(
            miolo.min,
            egui::pos2(icone.left() - ui.spacing().icon_spacing, miolo.max.y),
        );
        let galeria = pintor.layout_no_wrap(texto.to_owned(), Papel::Interface.font_id(), cor);
        let pos = Align2::LEFT_CENTER
            .align_size_within_rect(galeria.size(), area_texto)
            .min;
        pintor.with_clip_rect(area_texto).galley(pos, galeria, cor);
    }
}

/// O conteúdo do painel: a busca (listas longas) e as opções. Devolve a opção escolhida
/// neste quadro (clique, ou Enter na destacada).
fn lista<T: PartialEq + Copy>(
    ui: &mut Ui,
    id_busca: Id,
    com_busca: bool,
    altura_lista: f32,
    opcoes: &[(T, String)],
    selecionado: Option<T>,
) -> Option<T> {
    let mut busca: Busca = ui.data(|d| d.get_temp(id_busca)).unwrap_or_default();
    let mut escolhida = None;
    let mut rolar = false;

    if com_busca {
        let antes = busca.texto.clone();
        let campo = ui.add(
            CampoTexto::novo(&mut busca.texto)
                .marcador("Digite para filtrar…")
                .compacto(),
        );
        if !busca.aberta {
            campo.request_focus();
        }
        if busca.texto != antes {
            busca.realce = 0;
        }
        ui.add_space(Espaco::E4);
    }
    busca.aberta = true;

    let visiveis = filtrar(opcoes.iter().map(|(_, t)| t.as_str()), &busca.texto);
    if com_busca {
        let (baixo, cima, enter) = ui.input(|i| {
            (
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::Enter),
            )
        });
        if baixo {
            busca.realce = (busca.realce + 1).min(visiveis.len().saturating_sub(1));
        }
        if cima {
            busca.realce = busca.realce.saturating_sub(1);
        }
        if baixo || cima {
            busca.navegou = true;
            rolar = true;
            ui.ctx().request_repaint();
        }
        busca.realce = busca.realce.min(visiveis.len().saturating_sub(1));
        if enter {
            escolhida = visiveis.get(busca.realce).map(|&i| opcoes[i].0);
        }
    }
    let destacar = com_busca && (busca.navegou || !busca.texto.trim().is_empty());

    ui.spacing_mut().item_spacing.y = ESPACO_ITEM;
    ui.spacing_mut().interact_size.y = ALTURA_ITEM;
    ui.spacing_mut().button_padding.y = 6.0;
    let largura = ui.available_width();
    ui.allocate_ui(Vec2::new(largura, altura_lista), |ui| {
        ui.set_min_height(altura_lista);
        ScrollArea::vertical()
            .max_height(altura_lista)
            .show(ui, |ui| {
                // O painel é tão estreito quanto o botão: rótulo longo estica, não quebra.
                ui.style_mut().wrap_mode = Some(TextWrapMode::Extend);
                if visiveis.is_empty() {
                    ui.add(Rotulo::apoio(format!(
                        "Nenhuma opção com \"{}\"",
                        busca.texto.trim()
                    )));
                }
                for (pos, &i) in visiveis.iter().enumerate() {
                    let (valor, texto) = &opcoes[i];
                    let r = ui.selectable_label(selecionado == Some(*valor), texto.as_str());
                    if destacar && pos == busca.realce {
                        if rolar {
                            r.scroll_to_me(None);
                        }
                        let _ = r.clone().highlight();
                    }
                    if r.clicked() {
                        escolhida = Some(*valor);
                    }
                }
            });
    });

    ui.data_mut(|d| d.insert_temp(id_busca, busca));
    escolhida
}

/// Os índices das opções que casam com `termo`, na ordem em que aparecem: primeiro as que
/// começam com ele, depois — a partir de duas letras — as que têm uma palavra começando com ele
/// (com uma letra só, "a" acharia todo "e a"/"de a" do meio dos nomes). Sem acento, caixa nem
/// pontuação ("agua" acha "Água e esgoto"); termo vazio devolve todas.
fn filtrar<'a>(textos: impl Iterator<Item = &'a str>, termo: &str) -> Vec<usize> {
    let termo = cardeal_kernel::texto::chave_busca(termo);
    let textos: Vec<String> = textos.map(cardeal_kernel::texto::chave_busca).collect();
    if termo.is_empty() {
        return (0..textos.len()).collect();
    }
    let no_comeco = textos
        .iter()
        .enumerate()
        .filter(|(_, t)| t.starts_with(&termo))
        .map(|(i, _)| i);
    let numa_palavra = textos
        .iter()
        .enumerate()
        .filter(|(_, t)| {
            termo.chars().count() >= 2 && !t.starts_with(&termo) && t.contains(&format!(" {termo}"))
        })
        .map(|(i, _)| i);
    no_comeco.chain(numa_palavra).collect()
}

#[cfg(test)]
mod testes {
    use super::*;

    const CATEGORIAS: [&str; 7] = [
        "Aluguel",
        "Água e esgoto",
        "Assessoria contábil",
        "IRRF e outros impostos sobre a folha",
        "Impostos sobre vendas",
        "Lavação",
        "Salários",
    ];

    fn nomes(termo: &str) -> Vec<&'static str> {
        filtrar(CATEGORIAS.iter().copied(), termo)
            .into_iter()
            .map(|i| CATEGORIAS[i])
            .collect()
    }

    #[test]
    fn a_letra_deixa_so_o_que_comeca_com_ela_e_sem_acento() {
        assert_eq!(
            nomes("a"),
            ["Aluguel", "Água e esgoto", "Assessoria contábil"]
        );
        assert_eq!(nomes("as"), ["Assessoria contábil"]);
        assert_eq!(nomes("AGUA"), ["Água e esgoto"]);
    }

    #[test]
    fn quem_comeca_com_o_termo_vem_antes_de_quem_so_tem_a_palavra() {
        assert_eq!(
            nomes("imp"),
            [
                "Impostos sobre vendas",
                "IRRF e outros impostos sobre a folha"
            ]
        );
        assert_eq!(nomes("sobre v"), ["Impostos sobre vendas"]);
    }

    /// Clicar abre o painel com a busca focada; digitar "as" deixa só "Assessoria contábil" e
    /// Enter escolhe — sem tocar no mouse depois de abrir.
    #[test]
    fn abrir_digitar_e_enter_escolhe_a_primeira_que_casa() {
        let ctx = egui::Context::default();
        crate::tokens::instalar_fontes(&ctx);
        let opcoes: Vec<(usize, &str)> = CATEGORIAS
            .iter()
            .copied()
            .chain(["Telefone", "Vale-transporte"])
            .enumerate()
            .collect();
        assert!(opcoes.len() > LIMIAR_BUSCA);
        let mut selecionado: Option<usize> = None;
        let mut quadro = |eventos: Vec<egui::Event>| -> Rect {
            let mut botao = Rect::NOTHING;
            let entrada = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                events: eventos,
                ..Default::default()
            };
            let _ = ctx.run(entrada, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    botao = SeletorOpcao::novo("Categoria", &mut selecionado)
                        .opcoes(opcoes.iter().copied())
                        .mostrar(ui)
                        .rect;
                });
            });
            botao
        };
        let clique = |pressionado: bool, pos: egui::Pos2| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: pressionado,
            modifiers: egui::Modifiers::NONE,
        };

        let c = quadro(vec![]).center();
        quadro(vec![egui::Event::PointerMoved(c), clique(true, c)]);
        quadro(vec![clique(false, c)]);
        quadro(vec![]);
        quadro(vec![egui::Event::Text("as".into())]);
        quadro(vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert_eq!(selecionado, Some(2), "esperava Assessoria contábil");
    }

    #[test]
    fn termo_vazio_mostra_tudo_e_sem_par_nada() {
        assert_eq!(nomes("  ").len(), CATEGORIAS.len());
        assert!(nomes("xyz").is_empty());
    }
}
