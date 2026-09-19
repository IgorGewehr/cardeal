//! Galeria viva do design system Rubro — `docs/12-ui-ux.md` §7.
//!
//! `cargo run -p cardeal-ui --example galeria`. Mostra cada camada já construída para
//! conferência visual — o mais próximo de um "teste" de pixels antes de screenshot-diff.

use cardeal_kernel::Dinheiro;
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{
    desenhar_icone, superficie_clicavel, Botao, BotaoChevron, BotaoJanela, Caixa, CampoTexto,
    Divisor, Etiqueta, Rotulo, Spinner, Tecla, TipoBotaoJanela, Tom, ValorDinheiro,
};
use cardeal_ui::molecules::{
    Abas, CabecalhoTela, Campo, CampoBusca, CartaoKpi, EstadoVazio, ItemDeLista, LinhaDeAcao,
    SeletorOpcao, Severidade,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Grade, Janela, Notificacao, Notificacoes, Painel,
};
use cardeal_ui::tokens::{instalar_estilo, instalar_fontes, Espaco, Rubro, Tema, TemaUi};
use eframe::egui;

/// `GALERIA_CAPTURA=/tmp/x.png` salva um PNG e sai (conferência visual sem abrir a mão);
/// `GALERIA_ROLAR=<px>` rola a galeria antes; `GALERIA_DIALOGO=pequeno|medio|grande` abre um
/// diálogo; `GALERIA_TEMA=escuro` usa o tema escuro.
fn main() -> eframe::Result<()> {
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1100.0, 1000.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Galeria Rubro",
        opcoes,
        Box::new(|cc| {
            let tema = if std::env::var("GALERIA_TEMA").is_ok_and(|t| t == "escuro") {
                Tema::Escuro
            } else {
                Tema::Claro
            };
            egui_extras::install_image_loaders(&cc.egui_ctx);
            instalar_fontes(&cc.egui_ctx);
            instalar_estilo(&cc.egui_ctx, tema);
            Ok(Box::new(Galeria {
                tema,
                dialogo: std::env::var("GALERIA_DIALOGO").ok(),
                janela: std::env::var("GALERIA_JANELA").is_ok(),
                captura: std::env::var("GALERIA_CAPTURA").ok().map(Into::into),
                rolar: std::env::var("GALERIA_ROLAR")
                    .ok()
                    .and_then(|v| v.parse().ok()),
                ..Galeria::default()
            }))
        }),
    )
}

#[derive(Default)]
struct Galeria {
    tema: Tema,
    campo: String,
    campo_erro: String,
    busca: String,
    motivo: String,
    marcada: bool,
    item_ativo: usize,
    opcao: Option<u8>,
    aba: u8,
    dialogo: Option<String>,
    captura: Option<std::path::PathBuf>,
    janela: bool,
    rolar: Option<f32>,
    quadros: u32,
    pediu_captura: bool,
}

impl Galeria {
    /// Salva a captura pedida por `GALERIA_CAPTURA` quando ela chega, e fecha.
    fn tratar_captura(&mut self, ctx: &egui::Context) {
        let Some(destino) = &self.captura else { return };
        self.quadros += 1;
        if self.quadros >= 12 && !self.pediu_captura {
            self.pediu_captura = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        let imagem = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = imagem {
            let [w, h] = img.size;
            image::save_buffer(
                destino,
                img.as_raw(),
                u32::try_from(w).unwrap_or(0),
                u32::try_from(h).unwrap_or(0),
                image::ColorType::Rgba8,
            )
            .expect("gravando o PNG");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }

    /// O diálogo aberto por um dos botões da seção "Diálogo".
    fn dialogo_aberto(&mut self, ctx: &egui::Context) {
        let Some(tamanho) = self.dialogo.clone() else {
            return;
        };
        let base = Dialogo::nova("Cancelar cupom nº 4087 — R$ 31,33")
            .descricao("A venda em andamento é encerrada e o cancelamento fica registrado.");
        let base = match tamanho.as_str() {
            "pequeno" => base.pequeno(),
            "medio" => base.medio(),
            _ => base,
        };
        let fechar = base.mostrar(
            ctx,
            &mut self.motivo,
            |ui, motivo| {
                ui.add(Campo::novo("Motivo", motivo).marcador("Cliente desistiu da compra"));
            },
            |ui, _| {
                ui.add(Botao::destrutivo("Cancelar cupom").atalho("Enter"));
                ui.add(Botao::secundario("Voltar").atalho("Esc"));
            },
        );
        if fechar {
            self.dialogo = None;
        }
    }
}

impl eframe::App for Galeria {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.janela {
            Janela::nova(10.0, 40.0).barra_titulo(
                ctx,
                self.tema,
                (
                    "bytes://cardeal-logo.png",
                    include_bytes!("../../../assets/marca/cardeal-icone-256.png"),
                ),
                "Cardeal",
            );
        }
        egui::CentralPanel::default().show(ctx, |ui| {
            CabecalhoTela::novo("Galeria Rubro").mostrar(ui, |ui| {
                if ui
                    .add(Botao::secundario("Alternar tema").atalho("Ctrl+Shift+D"))
                    .clicked()
                {
                    self.tema = self.tema.alternado();
                    instalar_estilo(ui.ctx(), self.tema);
                }
            });

            let mut area = egui::ScrollArea::vertical().auto_shrink([false, false]);
            if let Some(px) = self.rolar {
                area = area.vertical_scroll_offset(px);
            }
            area.show(ui, |ui| {
                ui.add(Rotulo::titulo_secao("Botões"));
                ui.horizontal_wrapped(|ui| {
                    ui.add(Botao::primario("Finalizar venda").atalho("F2"));
                    ui.add(Botao::secundario("Cancelar"));
                    ui.add(Botao::fantasma("Ver detalhes"));
                    ui.add(Botao::destrutivo("Excluir"));
                    ui.add(Botao::primario("Desabilitado").habilitado(false));
                });
                ui.add_space(Espaco::E8);
                ui.add(Botao::primario("Botão de largura cheia").preenche_largura());

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Carregamento"));
                ui.horizontal(|ui| {
                    ui.add(Spinner::novo());
                    ui.add_space(Espaco::E16);
                    ui.add(Spinner::novo().pequeno());
                    ui.add_space(Espaco::E16);
                    ui.add(Botao::primario("Emitindo…").carregando(true));
                    ui.add(Botao::secundario("Salvando…").carregando(true));
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Notificações (toast)"));
                ui.horizontal_wrapped(|ui| {
                    if ui.add(Botao::primario("Sucesso")).clicked() {
                        notificar(ui.ctx(), Notificacao::sucesso("Cliente cadastrado"));
                    }
                    if ui.add(Botao::destrutivo("Erro")).clicked() {
                        notificar(
                            ui.ctx(),
                            Notificacao::erro("Documento inválido: dígito verificador não confere")
                                .detalhe("Confira o CPF informado e tente de novo."),
                        );
                    }
                    if ui.add(Botao::secundario("Aviso")).clicked() {
                        notificar(ui.ctx(), Notificacao::aviso("3 títulos vencem hoje"));
                    }
                    if ui.add(Botao::secundario("Info")).clicked() {
                        notificar(ui.ctx(), Notificacao::info("Backup concluído às 03:00"));
                    }
                    if ui.add(Botao::fantasma("NFC-e (loading → ok)")).clicked() {
                        let id = egui::Id::new("galeria-fiscal");
                        notificar(ui.ctx(), Notificacao::carregando("Emitindo NFC-e…").id(id));
                        let ctx = ui.ctx().clone();
                        // Simula a resposta chegando ~1,5s depois.
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(1500));
                            notificar(
                                &ctx,
                                Notificacao::sucesso("NFC-e autorizada — venda #4821").id(id),
                            );
                        });
                    }
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Tipografia"));
                ui.add(Rotulo::titulo_tela("Título de tela"));
                ui.add(Rotulo::titulo_secao("Título de seção"));
                ui.add(Rotulo::campo("Rótulo de campo"));
                ui.add(Rotulo::interface("Texto de interface padrão"));
                ui.add(ValorDinheiro::novo(Dinheiro::reais(1234)).com_sinal());
                ui.add(ValorDinheiro::novo(Dinheiro::reais(-560)).com_sinal());
                ui.add(Rotulo::codigo(
                    "35260912345678901234550010000123451987654321",
                ));

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Campos"));
                ui.add(
                    CampoTexto::novo(&mut self.campo)
                        .rotulo("Campo simples")
                        .marcador("digite algo"),
                );
                ui.add_space(Espaco::E8);
                ui.add(
                    Campo::novo("Campo com erro", &mut self.campo_erro)
                        .erro(Some("CNPJ inválido — 14 dígitos")),
                );
                ui.add_space(Espaco::E8);
                SeletorOpcao::novo("Seletor de opção", &mut self.opcao)
                    .opcoes([(1_u8, "Dinheiro"), (2, "Pix"), (3, "Cartão de crédito")])
                    .mostrar(ui);

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Abas (pílula deslizante)"));
                if let Some(n) = Abas::nova(&[
                    (0_u8, "Visão geral"),
                    (1, "A receber"),
                    (2, "A pagar"),
                    (3, "Fluxo"),
                ])
                .selecionada(self.aba)
                .mostrar(ui)
                {
                    self.aba = n;
                }

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Etiquetas de estado"));
                ui.horizontal_wrapped(|ui| {
                    ui.add(Etiqueta::nova("Rascunho", Tom::Neutro));
                    ui.add(Etiqueta::nova("Enviado", Tom::Info));
                    ui.add(Etiqueta::nova("Aprovado", Tom::Positivo));
                    ui.add(Etiqueta::nova("Vence hoje", Tom::Atencao));
                    ui.add(Etiqueta::nova("Recusado", Tom::Negativo));
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Cartões de KPI"));
                ui.columns(3, |cols| {
                    cols[0].add(
                        CartaoKpi::novo("A receber", Dinheiro::reais(12480)).variacao("7 títulos"),
                    );
                    cols[1].add(
                        CartaoKpi::novo("A pagar", Dinheiro::reais(-8420)).variacao("3 títulos"),
                    );
                    cols[2].add(
                        CartaoKpi::novo("Saldo", Dinheiro::reais(48230))
                            .variacao("▲ 4,2% vs ontem"),
                    );
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Linha de ação"));
                LinhaDeAcao::novo(Severidade::Critica, "3 títulos vencem hoje", "Cobrar")
                    .valor(Dinheiro::reais(8420))
                    .subtitulo("Fornecedor Alfa, Beta, Gama")
                    .mostrar(ui);

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Navegação (linha clicável)"));
                for (i, nome) in ["Pulso", "Ordens de Serviço", "Estoque"].iter().enumerate() {
                    if superficie_clicavel(ui, self.item_ativo == i, 40.0, |ui| {
                        desenhar_icone(ui, Icone::Estoque, 18.0, ui.cores().texto);
                        ui.add_space(Espaco::E12);
                        ui.add(Rotulo::interface(*nome));
                    })
                    .clicked()
                    {
                        self.item_ativo = i;
                    }
                }

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Painel (elevado · plano · realce)"));
                ui.columns(3, |cols| {
                    Painel::novo()
                        .titulo("Elevado")
                        .mostrar(&mut cols[0], |ui| {
                            ui.add(Rotulo::interface("O cartão padrão, com sombra mínima."));
                        });
                    Painel::novo().plano().titulo("Plano").mostrar_com_acoes(
                        &mut cols[1],
                        |ui| {
                            ui.add(Etiqueta::info("3"));
                        },
                        |ui| {
                            ui.add(Rotulo::interface("Sem sombra, com ação no cabeçalho."));
                        },
                    );
                    Painel::novo()
                        .realce(Tom::Positivo)
                        .titulo("Realce")
                        .mostrar(&mut cols[2], |ui| {
                            ui.add(Rotulo::interface("Venda finalizada · troco R$ 19,00"));
                        });
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Divisor · Tecla · Etiqueta com ponto"));
                ui.add(Divisor::novo());
                ui.add_space(Espaco::E8);
                ui.horizontal(|ui| {
                    ui.add(Tecla::nova("F2"));
                    ui.add(Rotulo::campo("finaliza"));
                    ui.add_space(Espaco::E12);
                    ui.add(Tecla::nova("Esc"));
                    ui.add(Rotulo::campo("volta"));
                    ui.add_space(Espaco::E12);
                    ui.add(Tecla::nova("1–4"));
                    ui.add(Rotulo::campo("escolhe a forma"));
                    ui.add_space(Espaco::E24);
                    ui.add(Etiqueta::nova("Caixa aberto", Tom::Positivo).com_ponto());
                    ui.add(Etiqueta::nova("Caixa fechado", Tom::Neutro).com_ponto());
                    ui.add(Etiqueta::nova("Contingência", Tom::Atencao).com_ponto());
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("KPI com tom e ícone"));
                ui.columns(3, |cols| {
                    cols[0].add(
                        CartaoKpi::novo("Vendas hoje", Dinheiro::reais(4820))
                            .variacao("38 cupons")
                            .tom(Tom::Positivo)
                            .icone(Icone::Caixa),
                    );
                    cols[1].add(
                        CartaoKpi::contagem("Vencem hoje", 3)
                            .variacao("R$ 8.420,00")
                            .tom(Tom::Atencao)
                            .icone(Icone::Alerta),
                    );
                    cols[2].add(
                        CartaoKpi::novo("Em atraso", Dinheiro::reais(1200))
                            .variacao("2 clientes")
                            .tom(Tom::Negativo)
                            .icone(Icone::Pessoas),
                    );
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao(
                    "Campo de bipe · Item de lista com atalho",
                ));
                ui.add(
                    CampoBusca::novo(&mut self.busca)
                        .marcador("Bipe o código de barras ou digite o nome do produto"),
                );
                ui.add_space(Espaco::E8);
                for (i, (nome, esmaecido)) in [
                    ("Dinheiro", false),
                    ("Pix", false),
                    ("Cartão (indisponível)", true),
                ]
                .iter()
                .enumerate()
                {
                    ItemDeLista::novo(*nome)
                        .atalho((i + 1).to_string())
                        .selecionado(i == 1)
                        .esmaecido(*esmaecido)
                        .mostrar(ui, |ui| {
                            ui.add(Rotulo::novo(cardeal_ui::tokens::Papel::Numero, "R$ 50,00"));
                        });
                }
                ItemDeLista::novo("Refrigerante Cola 2L")
                    .subtitulo("saldo 48")
                    .mostrar(ui, |ui| {
                        ui.add(Etiqueta::atencao("sem saldo"));
                    });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao(
                    "Janela: botões e chevron (GALERIA_JANELA=1 mostra a barra)",
                ));
                ui.horizontal(|ui| {
                    for tipo in [
                        TipoBotaoJanela::Minimizar,
                        TipoBotaoJanela::Maximizar,
                        TipoBotaoJanela::Restaurar,
                        TipoBotaoJanela::Fechar,
                    ] {
                        ui.add(BotaoJanela::novo(tipo));
                    }
                    ui.add_space(Espaco::E24);
                    ui.add(BotaoChevron::novo(true));
                    ui.add(BotaoChevron::novo(false));
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Caixa de seleção"));
                ui.add(Caixa::nova(&mut self.marcada, "Imprimir o cupom fiscal"));
                let mut sempre = true;
                ui.add(Caixa::nova(&mut sempre, "Marcada (só para ver)"));

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Grade: vazia · carregando"));
                ui.columns(2, |cols| {
                    // Duas grades com a mesma estrutura no mesmo quadro: cada uma no seu `push_id`.
                    cols[0].push_id("grade-vazia", |ui| {
                        Grade::nova(vec![
                            ColunaGrade::nova("Cliente"),
                            ColunaGrade::nova("Total").largura(90.0).numero(),
                        ])
                        .vazio("Nenhuma conta a receber.")
                        .mostrar(ui, 0, |_, _| {});
                    });
                    cols[1].push_id("grade-carregando", |ui| {
                        Grade::nova(vec![
                            ColunaGrade::nova("Cliente"),
                            ColunaGrade::nova("Total").largura(90.0).numero(),
                        ])
                        .carregando(true)
                        .mostrar(ui, 0, |_, _| {});
                    });
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Diálogo (pequeno · médio · grande)"));
                ui.horizontal(|ui| {
                    for (rotulo, chave) in [
                        ("Pequeno", "pequeno"),
                        ("Médio", "medio"),
                        ("Grande", "grande"),
                    ] {
                        if ui.add(Botao::secundario(rotulo)).clicked() {
                            self.dialogo = Some(chave.to_owned());
                        }
                    }
                });

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Estado vazio"));
                EstadoVazio::novo(Icone::Estoque, "Nenhum produto cadastrado ainda.")
                    .acao("Cadastrar produto")
                    .mostrar(ui);

                ui.add_space(Espaco::E24);
                ui.add(Rotulo::titulo_secao("Paleta Rubro"));
                ui.horizontal(|ui| {
                    for cor in [
                        Rubro::R50,
                        Rubro::R100,
                        Rubro::R200,
                        Rubro::R300,
                        Rubro::R400,
                        Rubro::R500,
                        Rubro::R600,
                        Rubro::R700,
                        Rubro::R800,
                        Rubro::R900,
                    ] {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::Vec2::splat(32.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 4.0, cor);
                    }
                });
            });
        });

        self.dialogo_aberto(ctx);
        Notificacoes::mostrar(ctx);
        self.tratar_captura(ctx);
    }
}
