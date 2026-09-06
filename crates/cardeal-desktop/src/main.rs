//! O aplicativo desktop do Cardeal. Ver `docs/12-ui-ux.md`.
//!
//! Shell com sidebar agrupada sobre o design system Rubro — login real
//! (`cardeal_cliente::MotorLocal`, monoposto). Navegação: Comercial (Vendas, Clientes),
//! Suprimentos (Estoque, Compras), Serviços (OS), Financeiro. Cada tela abre na lista
//! inteira; criar/ver um item acontece num `Dialogo` (regra de UI do projeto). Telas com UI:
//! Ordens de Serviço e Estoque; as demais mostram `tela_em_construcao` (backend já responde).

mod tela_agenda;
mod tela_clientes;
mod tela_compras;
mod tela_estoque;
mod tela_financeiro;
mod tela_os;
mod tela_vendas;

use std::path::PathBuf;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_modkit::{Icone, Modulo, PedidoAtivacao};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{Cartao, ItemComando, ItemSidebar, PaletaComandos, Sidebar};
use cardeal_ui::tokens::{instalar_estilo, instalar_fontes, Espaco, Rubro, Tema, TemaUi};
use eframe::egui;

fn caminho_da_base() -> PathBuf {
    let base = std::env::var_os("APPDATA").map_or_else(std::env::temp_dir, PathBuf::from);
    let dir = base.join("Cardeal");
    let _ = std::fs::create_dir_all(&dir);
    dir.join("cardeal.db")
}

fn modulos() -> Vec<&'static dyn Modulo> {
    vec![
        &mod_financeiro::ModuloFinanceiro,
        &mod_clientes::ModuloClientes,
        &mod_estoque::ModuloEstoque,
        &mod_compras::ModuloCompras,
        &mod_vendas::ModuloVendas,
        &mod_os::ModuloOs,
        &mod_agenda::ModuloAgenda,
    ]
}

fn pedido_ativacao() -> PedidoAtivacao {
    PedidoAtivacao::nova()
        .com_modulo("financeiro")
        .com_modulo("clientes")
        .com_modulo("estoque")
        .com_modulo("compras")
        .com_modulo("vendas")
        .com_modulo("os")
        .com_modulo("agenda")
}

/// Ícone da janela — quadrado `rubro-500` com cantos arredondados, gerado em código
/// (não há asset ainda). 48×48 RGBA.
fn icone_janela() -> egui::IconData {
    const L: usize = 48;
    const R: f32 = 10.0;
    let (cr, cg, cb) = (0xEF, 0x44, 0x3B); // Rubro::R500
    let mut rgba = vec![0u8; L * L * 4];
    for y in 0..L {
        for x in 0..L {
            let fx = (x as f32).min((L - 1 - x) as f32);
            let fy = (y as f32).min((L - 1 - y) as f32);
            let dentro = if fx >= R || fy >= R {
                true
            } else {
                let (dx, dy) = (R - fx, R - fy);
                dx * dx + dy * dy <= R * R
            };
            let i = (y * L + x) * 4;
            if dentro {
                rgba[i] = cr;
                rgba[i + 1] = cg;
                rgba[i + 2] = cb;
                rgba[i + 3] = 255;
            }
        }
    }
    egui::IconData { rgba, width: L as u32, height: L as u32 }
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt::init();
    let opcoes = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Cardeal")
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([920.0, 600.0])
            .with_icon(icone_janela()),
        ..eframe::NativeOptions::default()
    };
    eframe::run_native(
        "Cardeal",
        opcoes,
        Box::new(|cc| {
            instalar_fontes(&cc.egui_ctx);
            instalar_estilo(&cc.egui_ctx, Tema::Claro);
            Ok(Box::new(App::novo()))
        }),
    )
}

/// A área selecionada na sidebar, quando autenticado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Area {
    Vendas,
    Clientes,
    Estoque,
    Compras,
    Os,
    Agenda,
    Financeiro,
}

impl Area {
    const fn id(self) -> &'static str {
        match self {
            Self::Vendas => "vendas",
            Self::Clientes => "clientes",
            Self::Estoque => "estoque",
            Self::Compras => "compras",
            Self::Os => "os",
            Self::Agenda => "agenda",
            Self::Financeiro => "financeiro",
        }
    }

    fn de_id(id: &str) -> Self {
        match id {
            "vendas" => Self::Vendas,
            "clientes" => Self::Clientes,
            "compras" => Self::Compras,
            "os" => Self::Os,
            "agenda" => Self::Agenda,
            "financeiro" => Self::Financeiro,
            _ => Self::Estoque,
        }
    }
}

const ITENS_PALETA: &[ItemComando] = &[
    ItemComando {
        id: "vendas",
        rotulo: "Vendas",
        grupo: "Ir para",
    },
    ItemComando {
        id: "clientes",
        rotulo: "Clientes",
        grupo: "Ir para",
    },
    ItemComando {
        id: "estoque",
        rotulo: "Estoque",
        grupo: "Ir para",
    },
    ItemComando {
        id: "compras",
        rotulo: "Compras",
        grupo: "Ir para",
    },
    ItemComando {
        id: "os",
        rotulo: "Ordens de Serviço",
        grupo: "Ir para",
    },
    ItemComando {
        id: "agenda",
        rotulo: "Agenda",
        grupo: "Ir para",
    },
    ItemComando {
        id: "financeiro",
        rotulo: "Financeiro",
        grupo: "Ir para",
    },
    ItemComando {
        id: "tema",
        rotulo: "Alternar tema (claro/escuro)",
        grupo: "Ação",
    },
];

const GRUPOS_SIDEBAR: &[cardeal_ui::organisms::GrupoSidebar<'static>] = &[
    cardeal_ui::organisms::GrupoSidebar {
        titulo: Some("Comercial"),
        itens: &[
            ItemSidebar {
                id: "vendas",
                icone: Icone::Carrinho,
                rotulo: "Vendas",
                badge: None,
            },
            ItemSidebar {
                id: "clientes",
                icone: Icone::Pessoas,
                rotulo: "Clientes",
                badge: None,
            },
        ],
    },
    cardeal_ui::organisms::GrupoSidebar {
        titulo: Some("Suprimentos"),
        itens: &[
            ItemSidebar {
                id: "estoque",
                icone: Icone::Estoque,
                rotulo: "Estoque",
                badge: None,
            },
            ItemSidebar {
                id: "compras",
                icone: Icone::Nota,
                rotulo: "Compras",
                badge: None,
            },
        ],
    },
    cardeal_ui::organisms::GrupoSidebar {
        titulo: Some("Serviços"),
        itens: &[
            ItemSidebar {
                id: "os",
                icone: Icone::Ferramenta,
                rotulo: "Ordens de Serviço",
                badge: None,
            },
            ItemSidebar {
                id: "agenda",
                icone: Icone::Agenda,
                rotulo: "Agenda",
                badge: None,
            },
        ],
    },
    cardeal_ui::organisms::GrupoSidebar {
        titulo: Some("Financeiro"),
        itens: &[ItemSidebar {
            id: "financeiro",
            icone: Icone::Dinheiro,
            rotulo: "Financeiro",
            badge: None,
        }],
    },
];

/// Tudo que a sessão autenticada carrega.
struct EstadoAutenticado {
    sessao: SessaoLocal,
    area: Area,
    os: tela_os::EstadoTelaOs,
    estoque: tela_estoque::EstadoTelaEstoque,
    clientes: tela_clientes::EstadoTelaClientes,
    financeiro: tela_financeiro::EstadoTelaFinanceiro,
    vendas: tela_vendas::EstadoTelaVendas,
    compras: tela_compras::EstadoTelaCompras,
    agenda: tela_agenda::EstadoTelaAgenda,
}

/// O que a tela mostra agora.
enum Tela {
    Carregando,
    /// A base não pôde ser aberta — beco sem saída, mas com mensagem (nunca trava em branco).
    FalhaAoAbrir(String),
    PrimeiroAcesso {
        razao_social: String,
        cnpj: String,
        admin_nome: String,
        admin_login: String,
        admin_senha: String,
        erro: Option<String>,
    },
    Login {
        login: String,
        senha: String,
        erro: Option<String>,
    },
    Autenticado(Box<EstadoAutenticado>),
}

/// Um efeito colateral decidido durante o desenho do quadro, aplicado só depois.
enum Acao {
    Nenhuma,
    AlternarTema,
    AdminCriado(String),
    LoginOk(SessaoLocal),
    MudarArea(Area),
    AlternarSidebar,
    TentarNovamente,
}

struct App {
    motor: Option<MotorLocal>,
    tema: Tema,
    tema_aplicado: Option<Tema>,
    sidebar_expandida: bool,
    paleta_aberta: bool,
    paleta_busca: String,
    tela: Tela,
}

impl App {
    fn novo() -> Self {
        Self {
            motor: None,
            tema: Tema::Claro,
            tema_aplicado: Some(Tema::Claro),
            sidebar_expandida: true,
            paleta_aberta: false,
            paleta_busca: String::new(),
            tela: Tela::Carregando,
        }
    }

    fn carregar(&mut self) {
        match MotorLocal::abrir(&caminho_da_base(), &modulos(), &pedido_ativacao()) {
            Ok(motor) => {
                let precisa_config = motor.precisa_de_configuracao_inicial().unwrap_or(true);
                self.motor = Some(motor);
                self.tela = if precisa_config {
                    Tela::PrimeiroAcesso {
                        razao_social: String::new(),
                        cnpj: String::new(),
                        admin_nome: String::new(),
                        admin_login: String::new(),
                        admin_senha: String::new(),
                        erro: None,
                    }
                } else {
                    Tela::Login {
                        login: String::new(),
                        senha: String::new(),
                        erro: None,
                    }
                };
            }
            Err(e) => {
                tracing::error!("falha ao abrir a base: {e}");
                self.tela = Tela::FalhaAoAbrir(e.to_string());
            }
        }
    }

    fn entrar(&mut self, sessao: SessaoLocal) {
        let Some(motor) = &self.motor else { return };
        let mut os = tela_os::EstadoTelaOs::default();
        let mut estoque = tela_estoque::EstadoTelaEstoque::default();
        let mut clientes = tela_clientes::EstadoTelaClientes::default();
        let mut financeiro = tela_financeiro::EstadoTelaFinanceiro::default();
        os.carregar(motor, &sessao);
        estoque.carregar(motor, &sessao);
        clientes.carregar(motor, &sessao);
        financeiro.carregar(motor, &sessao);
        let mut vendas = tela_vendas::EstadoTelaVendas::default();
        vendas.carregar(motor, &sessao);
        let mut compras = tela_compras::EstadoTelaCompras::default();
        compras.carregar(motor, &sessao);
        let mut agenda = tela_agenda::EstadoTelaAgenda::default();
        agenda.carregar(motor, &sessao);

        self.tela = Tela::Autenticado(Box::new(EstadoAutenticado {
            sessao,
            area: Area::Os,
            os,
            estoque,
            clientes,
            financeiro,
            vendas,
            compras,
            agenda,
        }));
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.motor.is_none() && !matches!(self.tela, Tela::FalhaAoAbrir(_)) {
            self.carregar();
        }
        if self.tema_aplicado != Some(self.tema) {
            instalar_estilo(ctx, self.tema);
            self.tema_aplicado = Some(self.tema);
        }

        if ctx.input(|i| i.modifiers.command && i.modifiers.shift && i.key_pressed(egui::Key::D)) {
            self.tema = self.tema.alternado();
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::B)) {
            self.sidebar_expandida = !self.sidebar_expandida;
        }
        if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::K)) {
            self.paleta_aberta = !self.paleta_aberta;
        }

        let mut acao = Acao::Nenhuma;

        if matches!(self.tela, Tela::Autenticado(_)) {
            if let Some(id) = PaletaComandos::nova(ITENS_PALETA).mostrar(
                ctx,
                &mut self.paleta_aberta,
                &mut self.paleta_busca,
            ) {
                acao = if id == "tema" {
                    Acao::AlternarTema
                } else {
                    Acao::MudarArea(Area::de_id(id))
                };
            }
        }

        // Ctrl+1..6 salta para a n-ésima entrada da sidebar (`docs/12-ui-ux.md` §8).
        if matches!(self.tela, Tela::Autenticado(_)) {
            const TECLAS: [egui::Key; 6] = [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
                egui::Key::Num6,
            ];
            let ordem: Vec<&str> = GRUPOS_SIDEBAR
                .iter()
                .flat_map(|g| g.itens.iter().map(|i| i.id))
                .collect();
            for (i, tecla) in TECLAS.iter().enumerate() {
                if ctx.input(|inp| inp.modifiers.command && inp.key_pressed(*tecla)) {
                    if let Some(id) = ordem.get(i) {
                        acao = Acao::MudarArea(Area::de_id(id));
                    }
                }
            }
        }

        if let Tela::Autenticado(estado) = &self.tela {
            // Largura animada (`docs/12-ui-ux.md` §5: 160ms; volta a dormir ao terminar).
            let alvo = if self.sidebar_expandida { 244.0 } else { 60.0 };
            let largura = ctx.animate_value_with_time(egui::Id::new("sidebar-largura"), alvo, 0.16);
            let mostra_rotulos = largura > 150.0;
            egui::SidePanel::left("sidebar")
                .resizable(false)
                .exact_width(largura)
                .frame(
                    egui::Frame::none()
                        .fill(self.tema.cores().superficie_2)
                        .inner_margin(Espaco::E8),
                )
                .show(ctx, |ui| {
                    ui.add_space(Espaco::E8);
                    ui.horizontal(|ui| {
                        ui.add_space(Espaco::E8);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 3.0, Rubro::R500);
                        if mostra_rotulos {
                            ui.add_space(Espaco::E8);
                            ui.add(Rotulo::titulo_secao("Cardeal"));
                        }
                    });
                    ui.add_space(Espaco::E16);

                    if let Some(id) =
                        Sidebar::nova(GRUPOS_SIDEBAR, estado.area.id(), mostra_rotulos).mostrar(ui)
                    {
                        acao = Acao::MudarArea(Area::de_id(id));
                    }

                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        ui.add_space(Espaco::E8);
                        if ui
                            .add(Botao::fantasma(if self.sidebar_expandida {
                                "« Recolher"
                            } else {
                                "»"
                            }))
                            .clicked()
                        {
                            acao = Acao::AlternarSidebar;
                        }
                        if self.sidebar_expandida
                            && ui.add(Botao::fantasma("Alternar tema")).clicked()
                        {
                            acao = Acao::AlternarTema;
                        }
                    });
                });
        }

        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(self.tema.cores().fundo)
                    .inner_margin(Espaco::E24),
            )
            .show(ctx, |ui| match &mut self.tela {
                Tela::Carregando => {
                    ui.centered_and_justified(|ui| {
                        ui.add(Rotulo::interface("Abrindo a base..."));
                    });
                }
                Tela::FalhaAoAbrir(erro) => {
                    ui.add_space(96.0);
                    Cartao::novo().mostrar(ui, |ui| {
                        ui.add(Rotulo::titulo_secao("Não foi possível abrir a base"));
                        ui.add_space(Espaco::E8);
                        ui.add(
                            Rotulo::interface(erro.clone())
                                .quebravel()
                                .cor(ui.cores().negativo),
                        );
                        ui.add_space(Espaco::E16);
                        if ui.add(Botao::primario("Tentar de novo")).clicked() {
                            acao = Acao::TentarNovamente;
                        }
                    });
                }
                Tela::PrimeiroAcesso {
                    razao_social,
                    cnpj,
                    admin_nome,
                    admin_login,
                    admin_senha,
                    erro,
                } => {
                    ui.add_space(Espaco::E48);
                    Cartao::novo().largura(440.0).mostrar(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add(Rotulo::titulo_tela("Bem-vindo ao Cardeal"));
                            ui.add_space(Espaco::E4);
                            ui.add(Rotulo::campo(
                                "Primeiro acesso — cadastre sua empresa e o administrador.",
                            ));
                        });
                        ui.add_space(Espaco::E24);
                        ui.add(Campo::novo("Razão social", razao_social));
                        ui.add_space(Espaco::E12);
                        ui.add(Campo::novo("CNPJ", cnpj).marcador("00.000.000/0000-00"));
                        ui.add_space(Espaco::E12);
                        ui.add(Campo::novo("Seu nome", admin_nome));
                        ui.add_space(Espaco::E12);
                        ui.add(Campo::novo("Login", admin_login));
                        ui.add_space(Espaco::E12);
                        ui.add(
                            Campo::novo("Senha (mínimo 8 caracteres)", admin_senha)
                                .senha(true)
                                .erro(erro.clone()),
                        );
                        ui.add_space(Espaco::E16);
                        if ui
                            .add(Botao::primario("Criar administrador").preenche_largura())
                            .clicked()
                        {
                            if let Some(motor) = &self.motor {
                                match motor.configurar_inicial(
                                    razao_social,
                                    cnpj,
                                    admin_login,
                                    admin_nome,
                                    admin_senha,
                                ) {
                                    Ok(()) => acao = Acao::AdminCriado(admin_login.clone()),
                                    Err(e) => *erro = Some(e.mensagem),
                                }
                            }
                        }
                    });
                }
                Tela::Login { login, senha, erro } => {
                    ui.add_space(Espaco::E64);
                    Cartao::novo().largura(360.0).mostrar(ui, |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add(Rotulo::titulo_tela("Cardeal"));
                        });
                        ui.add_space(Espaco::E24);
                        ui.add(Campo::novo("Login", login));
                        ui.add_space(Espaco::E12);
                        ui.add(Campo::novo("Senha", senha).senha(true).erro(erro.clone()));
                        ui.add_space(Espaco::E16);

                        let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                        let clicou = ui
                            .add(Botao::primario("Entrar").atalho("Enter").preenche_largura())
                            .clicked();
                        if clicou || enter {
                            if let Some(motor) = &self.motor {
                                match motor.autenticar(login, senha) {
                                    Ok(sessao) => acao = Acao::LoginOk(sessao),
                                    Err(e) => *erro = Some(e.mensagem),
                                }
                            }
                        }
                    });
                }
                Tela::Autenticado(estado) => {
                    let Some(motor) = &self.motor else { return };
                    match estado.area {
                        Area::Os => {
                            tela_os::mostrar(ui, motor, &estado.sessao, &mut estado.os);
                        }
                        Area::Estoque => {
                            tela_estoque::mostrar(ui, motor, &estado.sessao, &mut estado.estoque);
                        }
                        Area::Clientes => {
                            tela_clientes::mostrar(ui, motor, &estado.sessao, &mut estado.clientes);
                        }
                        Area::Financeiro => {
                            tela_financeiro::mostrar(
                                ui,
                                motor,
                                &estado.sessao,
                                &mut estado.financeiro,
                            );
                        }
                        Area::Vendas => {
                            tela_vendas::mostrar(ui, motor, &estado.sessao, &mut estado.vendas);
                        }
                        Area::Compras => {
                            tela_compras::mostrar(ui, motor, &estado.sessao, &mut estado.compras);
                        }
                        Area::Agenda => {
                            tela_agenda::mostrar(ui, motor, &estado.sessao, &mut estado.agenda);
                        }
                    }
                }
            });

        match acao {
            Acao::Nenhuma => {}
            Acao::AlternarTema => self.tema = self.tema.alternado(),
            Acao::AlternarSidebar => self.sidebar_expandida = !self.sidebar_expandida,
            Acao::TentarNovamente => self.tela = Tela::Carregando,
            Acao::AdminCriado(login) => {
                self.tela = Tela::Login {
                    login,
                    senha: String::new(),
                    erro: None,
                }
            }
            Acao::LoginOk(sessao) => self.entrar(sessao),
            Acao::MudarArea(area) => {
                if let Tela::Autenticado(estado) = &mut self.tela {
                    estado.area = area;
                }
            }
        }
    }
}
