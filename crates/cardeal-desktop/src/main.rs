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
mod tela_orcamentos;
mod tela_os;
mod tela_pdv;
mod tela_settings;
mod tela_vendas;

use std::path::PathBuf;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_modkit::{Icone, Modulo, PedidoAtivacao};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{
    Cartao, ItemComando, ItemSidebar, Notificacoes, PaletaComandos, Sidebar,
};
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
        &mod_orcamentos::ModuloOrcamentos,
        &mod_agenda::ModuloAgenda,
        &mod_pdv::ModuloPdv,
    ]
}

/// Liga todos os módulos do desktop **e todos os seus submódulos** — as telas usam
/// funcionalidade de submódulos não-essenciais (contas a pagar, contas bancárias, fluxo,
/// recurso de agenda, laudo de OS, limite de crédito...) e sem isso as permissões
/// correspondentes nem entram no catálogo, travando o admin com "Sem permissão para ...".
fn pedido_ativacao() -> PedidoAtivacao {
    let mut pedido = PedidoAtivacao::nova();
    for m in modulos() {
        let manifesto = m.manifesto();
        let id = manifesto.id.como_str();
        pedido = pedido.com_modulo(id);
        for sub in manifesto.submodulos {
            pedido = pedido.com_submodulo(id, sub.id);
        }
    }
    pedido
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

/// Um botão-chevron (‹ / ›) desenhado à mão — recolher/expandir a sidebar. `aponta_esquerda`
/// = a sidebar está expandida (a seta convida a recolher).
fn chevron(ui: &mut egui::Ui, aponta_esquerda: bool, cor: egui::Color32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
    if ui.is_rect_visible(rect) {
        if resp.hovered() {
            ui.painter()
                .rect_filled(rect, 6.0, ui.style().visuals.widgets.hovered.bg_fill);
        }
        let c = rect.center();
        let (dx, dy) = (3.5_f32, 5.0_f32);
        let (perto, longe) = if aponta_esquerda {
            (c.x + dx / 2.0, c.x - dx / 2.0)
        } else {
            (c.x - dx / 2.0, c.x + dx / 2.0)
        };
        let traco = egui::Stroke::new(2.0_f32, cor);
        ui.painter().line_segment(
            [egui::pos2(perto, c.y - dy), egui::pos2(longe, c.y)],
            traco,
        );
        ui.painter().line_segment(
            [egui::pos2(longe, c.y), egui::pos2(perto, c.y + dy)],
            traco,
        );
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
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
    Pdv,
    Vendas,
    Clientes,
    Estoque,
    Compras,
    Os,
    Agenda,
    Financeiro,
    Configuracoes,
}

impl Area {
    const fn id(self) -> &'static str {
        match self {
            Self::Pdv => "pdv",
            Self::Vendas => "vendas",
            Self::Clientes => "clientes",
            Self::Estoque => "estoque",
            Self::Compras => "compras",
            Self::Os => "os",
            Self::Agenda => "agenda",
            Self::Financeiro => "financeiro",
            Self::Configuracoes => "configuracoes",
        }
    }

    fn de_id(id: &str) -> Self {
        match id {
            "pdv" => Self::Pdv,
            "vendas" => Self::Vendas,
            "clientes" => Self::Clientes,
            "compras" => Self::Compras,
            "os" => Self::Os,
            "agenda" => Self::Agenda,
            "financeiro" => Self::Financeiro,
            "configuracoes" => Self::Configuracoes,
            _ => Self::Estoque,
        }
    }
}

const ITENS_PALETA: &[ItemComando] = &[
    ItemComando {
        id: "pdv",
        rotulo: "PDV — Frente de caixa",
        grupo: "Ir para",
    },
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
                id: "pdv",
                icone: Icone::Caixa,
                rotulo: "PDV",
                badge: None,
            },
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
    pdv: tela_pdv::EstadoTelaPdv,
    settings: tela_settings::EstadoTelaSettings,
    os: tela_os::EstadoTelaOs,
    estoque: tela_estoque::EstadoTelaEstoque,
    clientes: tela_clientes::EstadoTelaClientes,
    financeiro: tela_financeiro::EstadoTelaFinanceiro,
    vendas: tela_vendas::EstadoTelaVendas,
    compras: tela_compras::EstadoTelaCompras,
    agenda: tela_agenda::EstadoTelaAgenda,
}

impl EstadoAutenticado {
    /// Recarrega a área ativa — F5, e o ponto único caso outra coisa precise forçar refresh.
    fn recarregar_area(&mut self, motor: &MotorLocal) {
        let s = &self.sessao;
        match self.area {
            Area::Pdv => self.pdv.carregar(motor, s),
            Area::Configuracoes => self.settings.carregar(motor, s),
            Area::Os => self.os.carregar(motor, s),
            Area::Estoque => self.estoque.carregar(motor, s),
            Area::Clientes => self.clientes.carregar(motor, s),
            Area::Financeiro => self.financeiro.carregar(motor, s),
            Area::Vendas => self.vendas.carregar(motor, s),
            Area::Compras => self.compras.carregar(motor, s),
            Area::Agenda => self.agenda.carregar(motor, s),
        }
    }
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

/// Onde a preferência de tema fica gravada (ao lado da base).
fn caminho_preferencias() -> PathBuf {
    caminho_da_base().with_file_name("preferencias.txt")
}

/// Lê o tema salvo; `Claro` se não houver arquivo.
fn tema_salvo() -> Tema {
    match std::fs::read_to_string(caminho_preferencias()) {
        Ok(s) if s.trim() == "escuro" => Tema::Escuro,
        _ => Tema::Claro,
    }
}

/// Grava a preferência de tema (best-effort).
fn salvar_tema(tema: Tema) {
    let _ = std::fs::write(
        caminho_preferencias(),
        if tema.e_escuro() { "escuro" } else { "claro" },
    );
}

impl App {
    fn novo() -> Self {
        let tema = tema_salvo();
        Self {
            motor: None,
            tema,
            tema_aplicado: None,
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
        let mut pdv = tela_pdv::EstadoTelaPdv::default();
        pdv.carregar(motor, &sessao);
        let mut settings = tela_settings::EstadoTelaSettings::default();
        settings.carregar(motor, &sessao);

        self.tela = Tela::Autenticado(Box::new(EstadoAutenticado {
            sessao,
            area: Area::Os,
            pdv,
            settings,
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
            if self.tema_aplicado.is_some() {
                salvar_tema(self.tema);
            }
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
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            if let (Tela::Autenticado(estado), Some(motor)) = (&mut self.tela, &self.motor) {
                estado.recarregar_area(motor);
            }
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
                    // Topo: logo + título + chevron de recolher, tudo na mesma linha.
                    ui.horizontal(|ui| {
                        ui.add_space(Espaco::E4);
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 4.0, Rubro::R500);
                        if mostra_rotulos {
                            ui.add_space(Espaco::E8);
                            ui.add(Rotulo::titulo_secao("Cardeal"));
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if chevron(ui, self.sidebar_expandida, self.tema.cores().texto_medio)
                                .clicked()
                            {
                                acao = Acao::AlternarSidebar;
                            }
                        });
                    });
                    ui.add_space(Espaco::E16);

                    if let Some(id) =
                        Sidebar::nova(GRUPOS_SIDEBAR, estado.area.id(), mostra_rotulos).mostrar(ui)
                    {
                        acao = Acao::MudarArea(Area::de_id(id));
                    }

                    // Rodapé fixo: Configurações.
                    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                        ui.add_space(Espaco::E8);
                        let sel = matches!(estado.area, Area::Configuracoes);
                        let cor = if sel {
                            self.tema.cores().rubro
                        } else {
                            self.tema.cores().texto_medio
                        };
                        let clic = cardeal_ui::atoms::superficie_clicavel(
                            ui,
                            sel,
                            cardeal_ui::atoms::altura_navegacao(),
                            |ui| {
                                cardeal_ui::atoms::desenhar_icone(ui, Icone::Config, 18.0, cor);
                                if mostra_rotulos {
                                    ui.add_space(Espaco::E12);
                                    ui.add(Rotulo::interface("Configurações").cor(cor));
                                }
                            },
                        );
                        if clic.clicked() {
                            acao = Acao::MudarArea(Area::Configuracoes);
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
                        Area::Pdv => {
                            tela_pdv::mostrar(ui, motor, &estado.sessao, &mut estado.pdv);
                        }
                        Area::Configuracoes => {
                            tela_settings::mostrar(
                                ui,
                                motor,
                                &estado.sessao,
                                &mut estado.settings,
                                &mut self.tema,
                            );
                        }
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
                if let (Tela::Autenticado(estado), Some(motor)) = (&mut self.tela, &self.motor) {
                    if estado.area != area {
                        estado.area = area;
                        // Entrar numa área sempre traz dados frescos — sem depender de um botão.
                        estado.recarregar_area(motor);
                    }
                }
            }
        }

        // Toasts — por último, para ficar acima de tudo (inclusive de qualquer dialog).
        Notificacoes::mostrar(ctx);
    }
}
