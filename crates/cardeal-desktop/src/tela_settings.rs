//! Tela de Configurações — empresa, aparência e acesso.
//!
//! `nucleo`/`auth` não são módulos de despacho; esta tela fala com o motor direto
//! (`MotorLocal::empresa_resumo`/`atualizar_empresa`/`usuarios`/`papeis`), como já fazem o
//! primeiro acesso e o login.

use cardeal_cliente::{
    EmpresaResumo, IdentidadeVisual, MotorLocal, PapelResumo, SessaoLocal, UsuarioResumo,
};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, ColunaGrade, Grade, LayoutTela, Notificacao};
use cardeal_ui::tokens::{Espaco, Raio, Tema, TemaUi};
use eframe::egui;

const REGIMES: [&str; 4] = ["MEI", "SimplesNacional", "LucroPresumido", "LucroReal"];
const REGIME_ROTULO: [&str; 4] = ["MEI", "Simples Nacional", "Lucro Presumido", "Lucro Real"];

#[derive(Default, Clone, Copy, PartialEq, Eq)]
enum Aba {
    #[default]
    Empresa,
    Aparencia,
    Acesso,
}

/// Estado local da tela de Configurações.
#[derive(Default)]
pub struct EstadoTelaSettings {
    aba: Aba,
    empresa: Option<EmpresaResumo>,
    usuarios: Vec<UsuarioResumo>,
    papeis: Vec<PapelResumo>,
    razao: String,
    fantasia: String,
    regime: usize,
    erro: Option<String>,
    // Identidade para documentos (PDF de orçamento etc.).
    identidade: Option<IdentidadeVisual>,
    id_telefone: String,
    id_email: String,
    id_site: String,
    id_endereco: String,
    logo_tex: Option<egui::TextureHandle>,
}

impl EstadoTelaSettings {
    /// Carrega empresa, usuários e papéis.
    pub fn carregar(&mut self, motor: &MotorLocal, _sessao: &SessaoLocal) {
        self.erro = None;
        match motor.empresa_resumo() {
            Ok(e) => {
                self.razao = e.razao_social.clone();
                self.fantasia = e.nome_fantasia.clone();
                self.regime = REGIMES
                    .iter()
                    .position(|r| *r == e.regime)
                    .unwrap_or(1);
                self.empresa = Some(e);
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        self.usuarios = motor.usuarios().unwrap_or_default();
        self.papeis = motor.papeis().unwrap_or_default();

        match motor.identidade_visual() {
            Ok(id) => {
                self.id_telefone = id.telefone.clone();
                self.id_email = id.email.clone();
                self.id_site = id.site.clone();
                self.id_endereco = id.endereco.clone();
                self.identidade = Some(id);
                self.logo_tex = None; // recarrega a textura no próximo desenho
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }
}

/// Desenha a tela.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaSettings,
    tema: &mut Tema,
) {
    LayoutTela::nova("Configurações").mostrar(
        ui,
        estado,
        |_ui, _estado| {},
        |ui, estado| {
            abas(ui, estado);
            ui.add_space(Espaco::E16);

            if let Some(e) = &estado.erro {
                ui.add(Rotulo::interface(e.clone()).quebravel().cor(ui.cores().negativo));
                ui.add_space(Espaco::E12);
            }
            match estado.aba {
                Aba::Empresa => secao_empresa(ui, motor, sessao, estado),
                Aba::Aparencia => secao_aparencia(ui, tema),
                Aba::Acesso => secao_acesso(ui, estado),
            }
        },
    );
}

fn abas(ui: &mut egui::Ui, estado: &mut EstadoTelaSettings) {
    let cores = ui.cores();
    egui::Frame::none()
        .fill(cores.superficie_2)
        .rounding(Raio::ITEM)
        .inner_margin(3.0)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                for (aba, rot) in [
                    (Aba::Empresa, "Empresa"),
                    (Aba::Aparencia, "Aparência"),
                    (Aba::Acesso, "Usuários e acesso"),
                ] {
                    let ativa = estado.aba == aba;
                    let b = if ativa {
                        Botao::primario(rot).pequeno()
                    } else {
                        Botao::fantasma(rot).pequeno()
                    };
                    if ui.add(b).clicked() {
                        estado.aba = aba;
                    }
                }
            });
        });
}

fn secao_empresa(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaSettings,
) {
    let cnpj = estado
        .empresa
        .as_ref()
        .map_or_else(String::new, |e| e.cnpj.clone());

    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Dados da empresa"));
        ui.add_space(Espaco::E12);
        ui.add(Campo::novo("Razão social", &mut estado.razao));
        ui.add_space(Espaco::E12);
        ui.add(Campo::novo("Nome fantasia", &mut estado.fantasia));
        ui.add_space(Espaco::E12);
        kv(ui, "CNPJ", &cnpj);
        ui.add_space(Espaco::E12);

        ui.add(Rotulo::campo("REGIME TRIBUTÁRIO"));
        ui.add_space(Espaco::E4);
        ui.horizontal_wrapped(|ui| {
            for (i, rot) in REGIME_ROTULO.iter().enumerate() {
                let sel = estado.regime == i;
                let b = if sel {
                    Botao::primario(*rot).pequeno()
                } else {
                    Botao::fantasma(*rot).pequeno()
                };
                if ui.add(b).clicked() {
                    estado.regime = i;
                }
            }
        });
        ui.add_space(Espaco::E16);
        if ui.add(Botao::primario("Salvar")).clicked() {
            match motor.atualizar_empresa(
                estado.razao.trim(),
                estado.fantasia.trim(),
                REGIMES[estado.regime],
            ) {
                Ok(()) => {
                    estado.carregar(motor, sessao);
                    notificar(ui.ctx(), Notificacao::sucesso("Dados da empresa atualizados"));
                }
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
    });
    ui.add_space(Espaco::E16);
    secao_identidade(ui, motor, sessao, estado);
}

fn secao_identidade(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaSettings,
) {
    garantir_textura_logo(ui, estado);

    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Identidade para documentos"));
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::campo(
            "Aparece no cabeçalho do PDF de orçamento — e de qualquer documento futuro.",
        ));
        ui.add_space(Espaco::E12);

        ui.horizontal(|ui| {
            let cores = ui.cores();
            egui::Frame::none()
                .fill(cores.superficie_2)
                .stroke(egui::Stroke::new(1.0_f32, cores.borda))
                .rounding(Raio::ITEM)
                .inner_margin(Espaco::E8)
                .show(ui, |ui| {
                    ui.set_min_size(egui::vec2(180.0, 72.0));
                    ui.centered_and_justified(|ui| {
                        if let Some(tex) = &estado.logo_tex {
                            let size = tex.size_vec2();
                            let escala = (160.0 / size.x).min(64.0 / size.y).min(1.0);
                            ui.image((tex.id(), size * escala));
                        } else {
                            ui.add(Rotulo::campo("sem logo"));
                        }
                    });
                });
            ui.add_space(Espaco::E12);
            ui.vertical(|ui| {
                if ui.add(Botao::secundario("Escolher logo (PNG)…")).clicked() {
                    escolher_logo(ui.ctx(), motor, sessao, estado);
                }
                ui.add_space(Espaco::E4);
                if estado
                    .identidade
                    .as_ref()
                    .is_some_and(|i| i.logo_png.is_some())
                    && ui.add(Botao::fantasma("Remover logo")).clicked()
                {
                    match motor.definir_logo_empresa(None) {
                        Ok(()) => {
                            estado.carregar(motor, sessao);
                            notificar(ui.ctx(), Notificacao::sucesso("Logo removida"));
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                    }
                }
            });
        });

        ui.add_space(Espaco::E16);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Telefone", &mut estado.id_telefone).marcador("(31) 3333-4444"));
            c[1].add(Campo::novo("E-mail", &mut estado.id_email).marcador("contato@empresa.com"));
        });
        ui.add_space(Espaco::E12);
        ui.add(Campo::novo("Site", &mut estado.id_site).marcador("empresa.com.br"));
        ui.add_space(Espaco::E12);
        ui.add(
            Campo::novo("Endereço (uma linha, para o documento)", &mut estado.id_endereco)
                .marcador("Rua Exemplo, 100 — Centro, Cidade/UF"),
        );
        ui.add_space(Espaco::E16);
        if ui.add(Botao::primario("Salvar identidade")).clicked() {
            match motor.definir_contato_empresa(
                estado.id_telefone.trim(),
                estado.id_email.trim(),
                estado.id_site.trim(),
                estado.id_endereco.trim(),
            ) {
                Ok(()) => {
                    estado.carregar(motor, sessao);
                    notificar(ui.ctx(), Notificacao::sucesso("Identidade atualizada"));
                }
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
    });
}

fn escolher_logo(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaSettings,
) {
    let Some(caminho) = rfd::FileDialog::new()
        .add_filter("Imagem PNG", &["png"])
        .pick_file()
    else {
        return;
    };
    let bytes = match std::fs::read(&caminho) {
        Ok(b) => b,
        Err(e) => {
            notificar(ctx, Notificacao::erro(format!("Não foi possível ler: {e}")));
            return;
        }
    };
    match motor.definir_logo_empresa(Some(&bytes)) {
        Ok(()) => {
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Logo atualizada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn garantir_textura_logo(ui: &egui::Ui, estado: &mut EstadoTelaSettings) {
    if estado.logo_tex.is_some() {
        return;
    }
    let Some(bytes) = estado.identidade.as_ref().and_then(|i| i.logo_png.as_ref()) else {
        return;
    };
    let Ok(img) = image::load_from_memory(bytes) else {
        return;
    };
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
    let cor = egui::ColorImage::from_rgba_unmultiplied([w, h], rgba.as_raw());
    estado.logo_tex = Some(ui.ctx().load_texture("logo-empresa", cor, egui::TextureOptions::LINEAR));
}

fn secao_aparencia(ui: &mut egui::Ui, tema: &mut Tema) {
    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Tema"));
        ui.add_space(Espaco::E12);
        for (opcao, rot, desc) in [
            (Tema::Claro, "Claro", "Padrão — ambientes bem iluminados."),
            (
                Tema::Escuro,
                "Escuro",
                "Recomendado para terminais de PDV em baixa luz.",
            ),
        ] {
            let sel = *tema == opcao;
            ui.horizontal(|ui| {
                let b = if sel {
                    Botao::primario(rot)
                } else {
                    Botao::secundario(rot)
                };
                if ui.add(b).clicked() {
                    *tema = opcao;
                }
                ui.add_space(Espaco::E12);
                ui.add(Rotulo::campo(desc));
            });
            ui.add_space(Espaco::E8);
        }
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::campo(
            "A escolha vale para este computador e é lembrada entre sessões (Ctrl+Shift+D alterna rápido).",
        ));
    });
}

fn secao_acesso(ui: &mut egui::Ui, estado: &mut EstadoTelaSettings) {
    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Usuários"));
        ui.add_space(Espaco::E8);
        if estado.usuarios.is_empty() {
            ui.add(Rotulo::campo("Nenhum usuário."));
        } else {
            let colunas = vec![
                ColunaGrade::nova("Nome"),
                ColunaGrade::nova("Login").largura(180.0),
                ColunaGrade::nova("Situação").largura(110.0),
            ];
            Grade::nova(colunas).mostrar(ui, estado.usuarios.len(), |i, row| {
                let u = &estado.usuarios[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(u.nome.clone()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::campo(u.login.clone()));
                });
                row.col(|ui| {
                    let (t, c) = if u.ativo {
                        ("ativo", ui.cores().positivo)
                    } else {
                        ("inativo", ui.cores().texto_fraco)
                    };
                    ui.add(Rotulo::interface(t).cor(c));
                });
            });
        }
    });
    ui.add_space(Espaco::E16);
    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Papéis"));
        ui.add_space(Espaco::E8);
        let colunas = vec![
            ColunaGrade::nova("Papel").largura(160.0),
            ColunaGrade::nova("Descrição"),
            ColunaGrade::nova("Permissões").largura(110.0),
        ];
        Grade::nova(colunas).mostrar(ui, estado.papeis.len(), |i, row| {
            let p = &estado.papeis[i];
            row.col(|ui| {
                let nome = if p.sistema {
                    format!("{} (sistema)", p.nome)
                } else {
                    p.nome.clone()
                };
                ui.add(Rotulo::interface(nome));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(p.descricao.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(p.permissoes.to_string()));
            });
        });
    });
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::campo(
        "Criar e editar usuários e papéis chega numa próxima versão — hoje o assistente de primeiro acesso cria o administrador.",
    ));
}

fn cartao(ui: &mut egui::Ui, conteudo: impl FnOnce(&mut egui::Ui)) {
    let cores = ui.cores();
    egui::Frame::none()
        .fill(cores.superficie)
        .stroke(egui::Stroke::new(1.0_f32, cores.borda))
        .rounding(Raio::CARTAO)
        .inner_margin(Espaco::E24)
        .show(ui, |ui| {
            ui.set_width(ui.available_width().clamp(1.0, 680.0));
            conteudo(ui);
        });
}

fn kv(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.add(Rotulo::campo(chave.to_uppercase()));
    ui.add(Rotulo::interface(if valor.trim().is_empty() {
        "—"
    } else {
        valor
    }));
}
