//! Configurações › Fiscal › **SPED (EFD ICMS/IPI)**: o que o arquivo pede e o cadastro não tem
//! (o contabilista, o perfil, a atividade, o código de receita e o vencimento do ICMS) e a
//! geração do mês — o arquivo para o contador validar e transmitir pelo PVA, e o registro da
//! apuração, cujo saldo credor abre o mês seguinte. Só para o regime normal: o Simples é
//! dispensado.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Competencia, Data, Dinheiro, Fuso};
use cardeal_ui::atoms::{Botao, Caixa, Rotulo};
use cardeal_ui::molecules::{dado, Campo, SeletorOpcao};
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_fiscal::efd::{
    ArquivoEfd, ConfiguracaoDaEfd, ConfiguracaoEfd, ConfigurarEfd, Contabilista, EfdDoMes,
    PerfilEfd, RegistrarApuracaoIcms,
};

/// O formulário e a última geração.
#[derive(Clone)]
pub struct FormSped {
    pub perfil: Option<PerfilEfd>,
    pub industrial: Option<bool>,
    pub classificacao: String,
    pub contabilista: Contabilista,
    pub codigo_receita: String,
    pub dia: String,
    /// O mês a gerar (o anterior, de início).
    pub mes: Competencia,
    pub substituta: bool,
    /// O saldo credor do mês anterior digitado (vazio: o da apuração registrada).
    pub saldo: String,
    /// A última geração (ou o erro dela).
    pub gerada: Option<Result<EfdDoMes, String>>,
}

impl Default for FormSped {
    fn default() -> Self {
        Self {
            perfil: Some(PerfilEfd::A),
            industrial: Some(false),
            classificacao: String::new(),
            contabilista: Contabilista::default(),
            codigo_receita: String::new(),
            dia: String::new(),
            mes: Data::hoje(Fuso::BRASILIA).competencia().anterior(),
            substituta: false,
            saldo: String::new(),
            gerada: None,
        }
    }
}

impl FormSped {
    /// Lê a configuração gravada.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(c) = motor.consultar(sessao, "fiscal.configuracao_efd.v1", &ConfiguracaoDaEfd) {
            self.de(c);
        }
    }

    fn de(&mut self, c: ConfiguracaoEfd) {
        self.perfil = Some(c.perfil);
        self.industrial = Some(c.industrial);
        self.classificacao = c.classificacao_industrial;
        self.contabilista = c.contabilista;
        self.codigo_receita = c.codigo_receita;
        self.dia = if c.dia_vencimento == 0 {
            String::new()
        } else {
            c.dia_vencimento.to_string()
        };
    }

    fn configuracao(&self) -> Result<ConfiguracaoEfd, String> {
        let dia = if self.dia.trim().is_empty() {
            0
        } else {
            self.dia
                .trim()
                .parse::<u32>()
                .ok()
                .filter(|d| (1..=31).contains(d))
                .ok_or("O dia do vencimento vai de 1 a 31")?
        };
        Ok(ConfiguracaoEfd {
            perfil: self.perfil.unwrap_or_default(),
            industrial: self.industrial.unwrap_or(false),
            classificacao_industrial: self.classificacao.trim().to_owned(),
            contabilista: self.contabilista.clone(),
            codigo_receita: self.codigo_receita.trim().to_owned(),
            dia_vencimento: dia,
        })
    }
}

/// Grava a configuração.
///
/// # Errors
/// O dia inválido ou o erro do comando.
pub fn salvar(motor: &Motor, sessao: &Sessao, f: &FormSped) -> Result<(), String> {
    let configuracao = f.configuracao()?;
    motor
        .executar(
            sessao,
            "fiscal.configurar_efd.v1",
            &ConfigurarEfd { configuracao },
        )
        .map_err(|e| e.mensagem)
}

/// Gera o arquivo do mês escolhido.
pub fn gerar(motor: &Motor, sessao: &Sessao, f: &mut FormSped) {
    let saldo = if f.saldo.trim().is_empty() {
        Ok(None)
    } else {
        Dinheiro::de_str(&f.saldo)
            .map(Some)
            .map_err(|_| "Saldo credor anterior: um valor, como 1.234,56".to_owned())
    };
    f.gerada = Some(saldo.and_then(|s| crate::sped::gerar(motor, sessao, f.mes, f.substituta, s)));
}

/// Grava o arquivo (ISO-8859-1, como o PVA lê) onde o usuário escolher.
fn salvar_arquivo(ctx: &egui::Context, a: &ArquivoEfd) {
    let Some(caminho) = rfd::FileDialog::new()
        .set_title("Onde gravar a EFD ICMS/IPI")
        .set_file_name(&a.nome)
        .add_filter("Arquivo do SPED", &["txt"])
        .save_file()
    else {
        return;
    };
    match std::fs::write(&caminho, a.em_latin1()) {
        Ok(()) => notificar(
            ctx,
            Notificacao::sucesso(format!("EFD gravada: {}", a.nome)),
        ),
        Err(e) => notificar(ctx, Notificacao::erro(format!("Não consegui gravar: {e}"))),
    }
}

/// Registra a apuração do mês gerado.
///
/// # Errors
/// O erro do comando.
pub fn registrar(motor: &Motor, sessao: &Sessao, f: &FormSped) -> Result<(), String> {
    let Some(Ok(EfdDoMes {
        arquivo: Some(a), ..
    })) = &f.gerada
    else {
        return Err("Gere o arquivo do mês antes".into());
    };
    motor
        .executar(
            sessao,
            "fiscal.registrar_apuracao_icms.v1",
            &RegistrarApuracaoIcms {
                competencia: f.mes,
                apuracao: a.apuracao,
            },
        )
        .map_err(|e| e.mensagem)
}

fn campos_da_configuracao(ui: &mut egui::Ui, f: &mut FormSped) {
    let c = &mut f.contabilista;
    ui.columns(2, |col| {
        SeletorOpcao::novo("Perfil (o da SEFAZ da UF)", &mut f.perfil)
            .opcoes([
                (PerfilEfd::A, "Perfil A"),
                (PerfilEfd::B, "Perfil B"),
                (PerfilEfd::C, "Perfil C"),
            ])
            .mostrar(&mut col[0]);
        SeletorOpcao::novo("Atividade", &mut f.industrial)
            .opcoes([
                (false, "Comércio, serviço e outros"),
                (true, "Indústria ou equiparado"),
            ])
            .mostrar(&mut col[1]);
    });
    if f.industrial == Some(true) {
        ui.add_space(Espaco::E8);
        ui.add(
            Campo::novo(
                "Classificação do estabelecimento industrial (00 a 09)",
                &mut f.classificacao,
            )
            .marcador("00"),
        );
    }
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::sobrelinha("Contabilista (registro 0100)"));
    ui.add_space(Espaco::E4);
    ui.columns(3, |col| {
        col[0].add(Campo::novo("Nome", &mut c.nome));
        col[1].add(Campo::novo("CPF", &mut c.cpf).marcador("000.000.000-00"));
        col[2].add(Campo::novo("CRC", &mut c.crc).marcador("RS-012345/O"));
    });
    ui.add_space(Espaco::E8);
    ui.columns(3, |col| {
        col[0].add(Campo::novo("E-mail", &mut c.email));
        col[1].add(Campo::novo("Telefone", &mut c.telefone));
        col[2].add(Campo::novo("CNPJ do escritório", &mut c.cnpj).marcador("opcional"));
    });
    ui.add_space(Espaco::E8);
    ui.columns(3, |col| {
        col[0].add(Campo::novo("Logradouro", &mut c.endereco));
        col[1].add(Campo::novo("Número", &mut c.numero));
        col[2].add(Campo::novo("Bairro", &mut c.bairro));
    });
    ui.add_space(Espaco::E8);
    ui.columns(3, |col| {
        col[0].add(Campo::novo("CEP", &mut c.cep));
        col[1].add(
            Campo::novo("Código IBGE do município", &mut c.codigo_municipio).marcador("4314902"),
        );
        col[2].add(Campo::novo("Complemento", &mut c.complemento));
    });
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::sobrelinha("ICMS a recolher (registro E116)"));
    ui.add_space(Espaco::E4);
    ui.columns(2, |col| {
        col[0].add(
            Campo::novo("Código de receita do ICMS na UF", &mut f.codigo_receita)
                .marcador("o contador sabe"),
        );
        col[1].add(Campo::novo("Dia do vencimento (mês seguinte)", &mut f.dia).marcador("12"));
    });
}

fn resultado(ui: &mut egui::Ui, f: &FormSped) -> (bool, bool) {
    let (mut salvar, mut registrar) = (false, false);
    match &f.gerada {
        None => {}
        Some(Err(e)) => {
            ui.add(
                Rotulo::interface(e.clone())
                    .quebravel()
                    .cor(ui.cores().negativo),
            );
        }
        Some(Ok(efd)) => match &efd.arquivo {
            None => {
                for i in &efd.impedimentos {
                    ui.add(
                        Rotulo::interface(i.clone())
                            .quebravel()
                            .cor(ui.cores().negativo),
                    );
                }
            }
            Some(a) => {
                let ap = &a.apuracao;
                ui.columns(3, |col| {
                    dado(&mut col[0], "Documentos", &a.documentos.to_string());
                    dado(
                        &mut col[1],
                        "Débitos de ICMS",
                        &ap.debitos.formatar_com_simbolo(),
                    );
                    dado(
                        &mut col[2],
                        "Créditos de ICMS",
                        &ap.creditos.formatar_com_simbolo(),
                    );
                });
                ui.add_space(Espaco::E8);
                ui.columns(3, |col| {
                    dado(
                        &mut col[0],
                        "Saldo credor anterior",
                        &ap.saldo_credor_anterior.formatar_com_simbolo(),
                    );
                    dado(
                        &mut col[1],
                        "ICMS a recolher",
                        &ap.a_recolher.formatar_com_simbolo(),
                    );
                    dado(
                        &mut col[2],
                        "Saldo credor para o mês seguinte",
                        &ap.saldo_credor_a_transportar.formatar_com_simbolo(),
                    );
                });
                if !a.avisos.is_empty() {
                    ui.add_space(Espaco::E8);
                    ui.add(Rotulo::sobrelinha("Para o contador olhar"));
                    for aviso in &a.avisos {
                        ui.add(Rotulo::apoio(format!("• {aviso}")).quebravel());
                    }
                }
                ui.add_space(Espaco::E12);
                ui.horizontal(|ui| {
                    salvar = ui.add(Botao::primario("Salvar o arquivo…")).clicked();
                    registrar = ui
                        .add(Botao::secundario(format!(
                            "Registrar a apuração de {}",
                            f.mes.formatar()
                        )))
                        .clicked();
                });
            }
        },
    }
    (salvar, registrar)
}

/// Desenha a seção.
pub fn secao(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, f: &mut FormSped, pode: bool) {
    let (mut salvar_config, mut gerar_agora) = (false, false);
    let (mut gravar_arquivo, mut registrar_agora) = (false, false);
    super::cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("SPED — EFD ICMS/IPI"));
        ui.add_space(Espaco::E4);
        ui.add(
            Rotulo::apoio(
                "O arquivo do mês para o contador validar e transmitir pelo PVA da Receita: as \
                 notas emitidas, as compras que entraram e a apuração do ICMS. Só o regime normal \
                 (Lucro Presumido ou Real) entrega — o Simples Nacional é dispensado. O inventário \
                 de dezembro (bloco H, na EFD de fevereiro) fica com o contador.",
            )
            .quebravel(),
        );
        ui.add_space(Espaco::E12);
        campos_da_configuracao(ui, f);
        ui.add_space(Espaco::E12);
        salvar_config = pode
            && ui
                .add(Botao::secundario("Salvar configuração do SPED"))
                .clicked();

        ui.add_space(Espaco::E16);
        ui.add(Rotulo::sobrelinha("Gerar o arquivo"));
        ui.add_space(Espaco::E4);
        ui.horizontal(|ui| {
            if ui.add(Botao::secundario("‹").pequeno()).clicked() {
                f.mes = f.mes.anterior();
                f.gerada = None;
            }
            ui.add(Rotulo::titulo_secao(f.mes.formatar_extenso()));
            if ui.add(Botao::secundario("›").pequeno()).clicked() {
                f.mes = f.mes.proxima();
                f.gerada = None;
            }
        });
        ui.add_space(Espaco::E8);
        ui.columns(2, |col| {
            col[0].add(
                Campo::novo("Saldo credor do mês anterior", &mut f.saldo)
                    .marcador("vazio: o da apuração registrada"),
            );
            col[1].add_space(Espaco::E16);
            col[1].add(Caixa::nova(
                &mut f.substituta,
                "Arquivo substituto (retificação)",
            ));
        });
        ui.add_space(Espaco::E8);
        gerar_agora = ui
            .add(Botao::primario(format!(
                "Gerar a EFD de {}",
                f.mes.formatar()
            )))
            .clicked();
        ui.add_space(Espaco::E12);
        (gravar_arquivo, registrar_agora) = resultado(ui, f);
        registrar_agora &= pode;
    });
    if salvar_config {
        match salvar(motor, sessao, f) {
            Ok(()) => notificar(ui.ctx(), Notificacao::sucesso("Configuração do SPED salva")),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
    if gerar_agora {
        gerar(motor, sessao, f);
    }
    if gravar_arquivo {
        if let Some(Ok(EfdDoMes {
            arquivo: Some(a), ..
        })) = &f.gerada
        {
            salvar_arquivo(ui.ctx(), a);
        }
    }
    if registrar_agora {
        match registrar(motor, sessao, f) {
            Ok(()) => notificar(
                ui.ctx(),
                Notificacao::sucesso(format!(
                    "Apuração de {} registrada: o saldo credor abre o mês seguinte",
                    f.mes.formatar()
                )),
            ),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn grava_a_configuracao_e_o_simples_e_dispensado() {
        let t = motor_de_teste();
        let mut f = FormSped::default();
        f.carregar(&t.motor, &t.sessao);
        assert_eq!(f.perfil, Some(PerfilEfd::A));
        f.dia = "40".into();
        assert!(salvar(&t.motor, &t.sessao, &f)
            .unwrap_err()
            .contains("1 a 31"));
        f.dia = "12".into();
        f.perfil = Some(PerfilEfd::B);
        f.contabilista.nome = "Ana Contadora".into();
        f.contabilista.crc = "RS-012345/O".into();
        f.codigo_receita = "0221".into();
        salvar(&t.motor, &t.sessao, &f).unwrap();
        let mut de_novo = FormSped::default();
        de_novo.carregar(&t.motor, &t.sessao);
        assert_eq!(de_novo.perfil, Some(PerfilEfd::B));
        assert_eq!(de_novo.dia, "12");
        assert_eq!(de_novo.contabilista.nome, "Ana Contadora");

        // A empresa de teste é do Simples: não há EFD, e o motivo aparece.
        gerar(&t.motor, &t.sessao, &mut de_novo);
        let Some(Ok(efd)) = &de_novo.gerada else {
            panic!("a consulta respondeu: {:?}", de_novo.gerada);
        };
        assert!(crate::sped::dispensada(efd), "{efd:?}");
        assert!(registrar(&t.motor, &t.sessao, &de_novo).is_err());
        // O saldo digitado errado não gera.
        de_novo.saldo = "abc".into();
        gerar(&t.motor, &t.sessao, &mut de_novo);
        assert!(matches!(de_novo.gerada, Some(Err(_))));
    }
}
