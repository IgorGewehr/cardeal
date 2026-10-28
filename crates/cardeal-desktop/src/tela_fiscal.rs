//! Configurações › Fiscal — o que precisa existir antes da primeira nota: os dados fiscais do
//! emitente (IE, IM, endereço com código IBGE), o certificado digital A1 (guardado cifrado no
//! cofre da instalação), o ambiente (homologação/produção) e o teste de conexão com a SEFAZ
//! pelo `sefaz-api` (`docs/modulos/fiscal.md`).

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Erro, Fuso, Uf};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom};
use cardeal_ui::molecules::{dado, Campo, SeletorOpcao};
use cardeal_ui::organisms::{notificar, Notificacao, Painel};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_empresa::{
    DadosFiscais, DadosFiscaisDaEmpresa, DefinirDadosFiscaisEmpresa, EnderecoFiscal,
};
use mod_fiscal::{
    AmbienteFiscal, CertificadoResumo, ConfiguracaoDaNfce, ConfiguracaoNfce, ConfigurarNfce,
    DefinirAmbienteFiscal, InstalarCertificado, ModeloFiscal, RemoverCertificado, SituacaoFiscal,
    SituacaoFiscalDaEmpresa, StatusDaSefaz, StatusSefaz, ALERTA_VENCIMENTO_DIAS,
};

mod caixa;
mod nfe;
mod nfse;

const FUSO: Fuso = Fuso::BRASILIA;

/// O formulário dos dados fiscais (texto livre; o comando normaliza e confere).
#[derive(Default, Clone)]
pub struct FormDadosFiscais {
    pub ie: String,
    pub im: String,
    pub cep: String,
    pub uf: Option<Uf>,
    pub logradouro: String,
    pub numero: String,
    pub complemento: String,
    pub bairro: String,
    pub municipio: String,
    pub codigo_municipio: String,
}

impl FormDadosFiscais {
    fn de(d: &DadosFiscais) -> Self {
        let e = &d.endereco;
        Self {
            ie: d.inscricao_estadual.clone(),
            im: d.inscricao_municipal.clone(),
            cep: e.cep.clone(),
            uf: Uf::de_sigla(&e.uf).ok(),
            logradouro: e.logradouro.clone(),
            numero: e.numero.clone(),
            complemento: e.complemento.clone(),
            bairro: e.bairro.clone(),
            municipio: e.municipio.clone(),
            codigo_municipio: e.codigo_municipio.clone(),
        }
    }

    fn em_dados(&self) -> DadosFiscais {
        DadosFiscais {
            inscricao_estadual: self.ie.clone(),
            inscricao_municipal: self.im.clone(),
            endereco: EnderecoFiscal {
                logradouro: self.logradouro.clone(),
                numero: self.numero.clone(),
                complemento: self.complemento.clone(),
                bairro: self.bairro.clone(),
                municipio: self.municipio.clone(),
                codigo_municipio: self.codigo_municipio.clone(),
                uf: self.uf.map(|u| u.sigla().to_string()).unwrap_or_default(),
                cep: self.cep.clone(),
            },
        }
    }
}

/// O estado da aba Fiscal.
#[derive(Default)]
pub struct EstadoFiscal {
    pub situacao: Option<SituacaoFiscalDaEmpresa>,
    pub form: FormDadosFiscais,
    /// O campo com erro na última gravação, e a mensagem.
    pub erro_dados: Option<(String, String)>,
    /// O arquivo escolhido para instalar: nome e bytes.
    pub arquivo: Option<(String, Vec<u8>)>,
    pub senha: String,
    /// Mostrar o formulário de instalação mesmo com um certificado já instalado (trocar).
    pub trocando: bool,
    /// O último teste de conexão, por modelo.
    pub teste: Vec<(ModeloFiscal, Result<StatusSefaz, String>)>,
    pub erro: Option<String>,
    /// O formulário da NFC-e.
    pub nfce: FormNfce,
    /// A série deste caixa e a lista dos caixas com série.
    pub caixa: caixa::EstadoCaixa,
    /// A série e a numeração da NF-e.
    pub nfe: nfe::FormNfe,
    pub nfse: nfse::FormNfse,
}

/// O formulário da NFC-e. O token do CSC nunca volta da base: vazio = manter o guardado.
#[derive(Default, Clone)]
pub struct FormNfce {
    pub emitir: bool,
    pub serie: String,
    pub proximo: String,
    pub csc_id: String,
    pub csc_token: String,
    pub csc_cadastrado: bool,
    pub erro: Option<(String, String)>,
}

impl FormNfce {
    fn de(c: &ConfiguracaoNfce) -> Self {
        Self {
            emitir: c.emitir_nas_vendas,
            serie: c.serie.to_string(),
            proximo: c.proximo_numero.to_string(),
            csc_id: c.csc_id.clone(),
            csc_token: String::new(),
            csc_cadastrado: c.csc_cadastrado,
            erro: None,
        }
    }

    fn erro_em(&self, campo: &str) -> Option<String> {
        self.erro
            .as_ref()
            .filter(|(c, _)| c == campo)
            .map(|(_, m)| m.clone())
    }
}

/// Grava a configuração da NFC-e do formulário.
///
/// # Errors
/// Número inválido no formulário ou o erro do comando (com o campo marcado).
pub fn salvar_nfce(motor: &Motor, sessao: &Sessao, estado: &mut EstadoFiscal) -> Result<(), Erro> {
    let f = &mut estado.nfce;
    let numero = |texto: &str, campo: &str, nome: &str| {
        texto.trim().replace('.', "").parse::<u64>().map_err(|_| {
            Erro::novo(
                cardeal_kernel::CodigoErro::VALOR_INVALIDO,
                format!("{nome}: só números"),
            )
            .no_campo(campo)
        })
    };
    let resultado = numero(&f.serie, "serie", "Série")
        .and_then(|s| {
            u32::try_from(s).map_err(|_| {
                Erro::novo(
                    cardeal_kernel::CodigoErro::FORA_DA_FAIXA,
                    "Série grande demais",
                )
                .no_campo("serie")
            })
        })
        .and_then(|serie| {
            Ok((
                serie,
                numero(&f.proximo, "proximo_numero", "Próximo número")?,
            ))
        })
        .and_then(|(serie, proximo_numero)| {
            let comando = ConfigurarNfce {
                emitir_nas_vendas: f.emitir,
                serie,
                proximo_numero,
                csc_id: f.csc_id.trim().to_string(),
                csc_token: Some(f.csc_token.trim().to_string()).filter(|t| !t.is_empty()),
            };
            motor.executar(sessao, "fiscal.configurar_nfce.v1", &comando)
        });
    match resultado {
        Ok(c) => {
            estado.nfce = FormNfce::de(&c);
            estado.carregar(motor, sessao);
            Ok(())
        }
        Err(e) => {
            estado.nfce.erro = e.campo.clone().map(|c| (c, e.mensagem.clone()));
            Err(e)
        }
    }
}

impl EstadoFiscal {
    /// Carrega os dados fiscais e a situação.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        self.erro = None;
        match motor.consultar(sessao, "empresa.dados_fiscais.v1", &DadosFiscaisDaEmpresa) {
            Ok(d) => self.form = FormDadosFiscais::de(&d),
            Err(e) => self.erro = Some(e.mensagem),
        }
        match motor.consultar(sessao, "fiscal.situacao.v1", &SituacaoFiscal) {
            Ok(s) => self.situacao = Some(s),
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(c) = motor.consultar(sessao, "fiscal.configuracao_nfce.v1", &ConfiguracaoDaNfce) {
            self.nfce = FormNfce::de(&c);
        }
        self.caixa.carregar(motor, sessao);
        self.nfe.carregar(motor, sessao);
        self.nfse.carregar(motor, sessao);
    }

    fn erro_em(&self, campo: &str) -> Option<String> {
        self.erro_dados
            .as_ref()
            .filter(|(c, _)| c == campo)
            .map(|(_, m)| m.clone())
    }
}

/// Grava os dados fiscais do formulário.
///
/// # Errors
/// O erro do comando (com o campo marcado em `estado.erro_dados`).
pub fn salvar_dados(motor: &Motor, sessao: &Sessao, estado: &mut EstadoFiscal) -> Result<(), Erro> {
    let comando = DefinirDadosFiscaisEmpresa {
        dados: estado.form.em_dados(),
    };
    match motor.executar(sessao, "empresa.definir_dados_fiscais.v1", &comando) {
        Ok(d) => {
            estado.erro_dados = None;
            estado.form = FormDadosFiscais::de(&d);
            estado.carregar(motor, sessao);
            Ok(())
        }
        Err(e) => {
            estado.erro_dados = e.campo.clone().map(|c| (c, e.mensagem.clone()));
            Err(e)
        }
    }
}

/// Instala o arquivo escolhido com a senha digitada.
///
/// # Errors
/// Sem arquivo escolhido, ou o erro do comando (senha, validade, CNPJ).
pub fn instalar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoFiscal,
) -> Result<CertificadoResumo, Erro> {
    let Some((_, bytes)) = estado.arquivo.as_ref() else {
        return Err(Erro::obrigatorio("arquivo"));
    };
    let comando = InstalarCertificado {
        arquivo: bytes.clone(),
        senha: estado.senha.clone(),
    };
    let resumo = motor.executar(sessao, "fiscal.instalar_certificado.v1", &comando)?;
    estado.arquivo = None;
    estado.senha.clear();
    estado.trocando = false;
    estado.teste.clear();
    estado.carregar(motor, sessao);
    Ok(resumo)
}

/// Testa a conexão com a SEFAZ para NF-e e NFC-e. Bloqueia até a resposta (alguns
/// segundos no pior caso — o cliente HTTP tem tempo limite).
pub fn testar_conexao(motor: &Motor, sessao: &Sessao, estado: &mut EstadoFiscal) {
    estado.teste = [ModeloFiscal::Nfe, ModeloFiscal::Nfce]
        .into_iter()
        .map(|modelo| {
            let r = motor
                .consultar(sessao, "fiscal.status_sefaz.v1", &StatusDaSefaz { modelo })
                .map_err(|e| e.mensagem);
            (modelo, r)
        })
        .collect();
}

/// Troca o ambiente.
///
/// # Errors
/// O erro do comando (produção sem certificado).
pub fn definir_ambiente(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoFiscal,
    ambiente: AmbienteFiscal,
) -> Result<(), Erro> {
    motor.executar(
        sessao,
        "fiscal.definir_ambiente.v1",
        &DefinirAmbienteFiscal { ambiente },
    )?;
    estado.teste.clear();
    estado.carregar(motor, sessao);
    Ok(())
}

/// A aba Fiscal.
pub fn mostrar(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoFiscal) {
    let pode_configurar = sessao.concede("fiscal.configurar");
    if let Some(e) = &estado.erro {
        ui.add(
            Rotulo::interface(e.clone())
                .quebravel()
                .cor(ui.cores().negativo),
        );
        ui.add_space(Espaco::E12);
    }
    secao_situacao(ui, estado);
    ui.add_space(Espaco::E16);
    secao_dados(ui, motor, sessao, estado, pode_configurar);
    ui.add_space(Espaco::E16);
    secao_certificado(ui, motor, sessao, estado, pode_configurar);
    ui.add_space(Espaco::E16);
    secao_nfce(ui, motor, sessao, estado, pode_configurar);
    ui.add_space(Espaco::E16);
    caixa::secao(ui, motor, sessao, &mut estado.caixa, pode_configurar);
    ui.add_space(Espaco::E16);
    nfe::secao(ui, motor, sessao, &mut estado.nfe, pode_configurar);
    ui.add_space(Espaco::E16);
    nfse::secao(ui, motor, sessao, &mut estado.nfse, pode_configurar);
    ui.add_space(Espaco::E16);
    secao_conexao(ui, motor, sessao, estado, pode_configurar);
}

fn cartao(ui: &mut egui::Ui, conteudo: impl FnOnce(&mut egui::Ui)) {
    Painel::novo().amplo().largura(680.0).mostrar(ui, conteudo);
}

fn secao_situacao(ui: &mut egui::Ui, estado: &EstadoFiscal) {
    let Some(s) = &estado.situacao else { return };
    if s.pendencias.is_empty() {
        cartao(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(Etiqueta::positiva("Pronto para emitir").com_ponto());
                ui.add_space(Espaco::E8);
                ui.add(Rotulo::campo(rotulo_ambiente(s.ambiente)));
            });
        });
        return;
    }
    let tom = if s.pronta_para_emitir() {
        Tom::Atencao
    } else {
        Tom::Negativo
    };
    Painel::novo().realce(tom).largura(680.0).mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Antes de emitir notas"));
        ui.add_space(Espaco::E8);
        for p in &s.pendencias {
            ui.add(Rotulo::interface(format!("• {}", p.mensagem())).quebravel());
            ui.add_space(Espaco::E4);
        }
    });
}

const fn rotulo_ambiente(a: AmbienteFiscal) -> &'static str {
    match a {
        AmbienteFiscal::Homologacao => "Homologação — notas de teste, sem valor fiscal",
        AmbienteFiscal::Producao => "Produção — notas com valor fiscal",
    }
}

fn secao_dados(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoFiscal,
    pode_configurar: bool,
) {
    let e_ie = estado.erro_em("inscricao_estadual");
    let e_cep = estado.erro_em("cep");
    let e_uf = estado.erro_em("uf");
    let e_log = estado.erro_em("logradouro");
    let e_num = estado.erro_em("numero");
    let e_bairro = estado.erro_em("bairro");
    let e_mun = estado.erro_em("municipio");
    let e_ibge = estado.erro_em("codigo_municipio");
    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Dados fiscais do emitente"));
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::apoio(
            "Saem em toda nota. O código IBGE do município (7 dígitos) está no site do IBGE ou numa nota de fornecedor da mesma cidade.",
        ));
        ui.add_space(Espaco::E12);
        let f = &mut estado.form;
        ui.columns(2, |c| {
            c[0].add(
                Campo::novo("Inscrição estadual", &mut f.ie)
                    .marcador("ou ISENTO")
                    .erro(e_ie),
            );
            c[1].add(Campo::novo("Inscrição municipal (NFS-e)", &mut f.im));
        });
        ui.add_space(Espaco::E12);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("CEP", &mut f.cep).erro(e_cep));
            SeletorOpcao::novo("UF", &mut f.uf)
                .opcoes(
                    Uf::TODAS
                        .iter()
                        .map(|u| (*u, format!("{} — {}", u.sigla(), u.nome()))),
                )
                .mostrar(&mut c[1]);
            if let Some(m) = e_uf {
                c[1].add(Rotulo::apoio(m).cor(c[1].cores().negativo));
            }
        });
        ui.add_space(Espaco::E12);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Logradouro", &mut f.logradouro).erro(e_log));
            c[1].add(
                Campo::novo("Número", &mut f.numero)
                    .marcador("ou S/N")
                    .erro(e_num),
            );
        });
        ui.add_space(Espaco::E12);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Complemento", &mut f.complemento));
            c[1].add(Campo::novo("Bairro", &mut f.bairro).erro(e_bairro));
        });
        ui.add_space(Espaco::E12);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Município", &mut f.municipio).erro(e_mun));
            c[1].add(
                Campo::novo("Código IBGE do município", &mut f.codigo_municipio)
                    .marcador("7 dígitos")
                    .erro(e_ibge),
            );
        });
        ui.add_space(Espaco::E16);
        if pode_configurar && ui.add(Botao::primario("Salvar dados fiscais")).clicked() {
            match salvar_dados(motor, sessao, estado) {
                Ok(()) => notificar(ui.ctx(), Notificacao::sucesso("Dados fiscais salvos")),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
    });
}

fn secao_certificado(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoFiscal,
    pode_configurar: bool,
) {
    let certificado = estado.situacao.as_ref().and_then(|s| s.certificado.clone());
    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Certificado digital A1"));
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::apoio(
            "O arquivo e a senha ficam cifrados nesta instalação: a base de dados e os backups guardam só o texto cifrado.",
        ));
        ui.add_space(Espaco::E12);
        if let Some(c) = &certificado {
            mostrar_certificado(ui, c);
            ui.add_space(Espaco::E12);
            if pode_configurar && !estado.trocando {
                ui.horizontal(|ui| {
                    if ui.add(Botao::secundario("Trocar certificado")).clicked() {
                        estado.trocando = true;
                    }
                    ui.add_space(Espaco::E8);
                    if ui.add(Botao::fantasma("Remover")).clicked() {
                        match motor.executar(
                            sessao,
                            "fiscal.remover_certificado.v1",
                            &RemoverCertificado,
                        ) {
                            Ok(_) => {
                                estado.teste.clear();
                                estado.carregar(motor, sessao);
                                notificar(ui.ctx(), Notificacao::sucesso("Certificado removido"));
                            }
                            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                        }
                    }
                });
            }
        } else {
            ui.add(Rotulo::campo("Nenhum certificado instalado."));
            ui.add_space(Espaco::E12);
        }
        if pode_configurar && (certificado.is_none() || estado.trocando) {
            formulario_instalacao(ui, motor, sessao, estado);
        }
    });
}

fn mostrar_certificado(ui: &mut egui::Ui, c: &CertificadoResumo) {
    let hoje = cardeal_kernel::Data::hoje(FUSO);
    let dias = c.dias_para_vencer(hoje, FUSO);
    ui.horizontal(|ui| {
        ui.add(Rotulo::interface(c.titular.clone()));
        ui.add_space(Espaco::E8);
        let etiqueta = if dias < 0 {
            Etiqueta::negativa("vencido")
        } else if dias <= ALERTA_VENCIMENTO_DIAS {
            Etiqueta::atencao(format!("vence em {dias} dia(s)"))
        } else {
            Etiqueta::positiva("válido")
        };
        ui.add(etiqueta.com_ponto());
    });
    ui.add_space(Espaco::E8);
    ui.columns(2, |col| {
        dado(&mut col[0], "CNPJ", &c.cnpj);
        dado(
            &mut col[1],
            "Válido até",
            &c.valido_ate.data(FUSO).formatar(),
        );
    });
    ui.add_space(Espaco::E8);
    ui.columns(2, |col| {
        dado(&mut col[0], "Certificadora", &c.emissor);
        dado(
            &mut col[1],
            "Instalado em",
            &c.instalado_em.data(FUSO).formatar(),
        );
    });
    if c.convertido {
        ui.add_space(Espaco::E8);
        ui.add(Rotulo::apoio(
            "O arquivo usava um formato antigo (RC2) e foi regravado na instalação para o emissor aceitar. A senha continua a mesma.",
        ));
    }
}

fn formulario_instalacao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoFiscal,
) {
    ui.horizontal(|ui| {
        if ui
            .add(Botao::secundario("Escolher arquivo (.pfx / .p12)…"))
            .clicked()
        {
            escolher_arquivo(ui.ctx(), estado);
        }
        ui.add_space(Espaco::E8);
        let nome = estado.arquivo.as_ref().map_or_else(
            || "nenhum arquivo escolhido".to_string(),
            |(n, _)| n.clone(),
        );
        ui.add(Rotulo::campo(nome));
    });
    ui.add_space(Espaco::E12);
    ui.add(Campo::novo("Senha do certificado", &mut estado.senha).senha(true));
    ui.add_space(Espaco::E12);
    ui.horizontal(|ui| {
        if ui.add(Botao::primario("Instalar certificado")).clicked() {
            match instalar(motor, sessao, estado) {
                Ok(c) => notificar(
                    ui.ctx(),
                    Notificacao::sucesso(format!("Certificado de {} instalado", c.titular)),
                ),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
        if estado.trocando {
            ui.add_space(Espaco::E8);
            if ui.add(Botao::fantasma("Cancelar")).clicked() {
                estado.trocando = false;
                estado.arquivo = None;
                estado.senha.clear();
            }
        }
    });
}

fn escolher_arquivo(ctx: &egui::Context, estado: &mut EstadoFiscal) {
    let Some(caminho) = rfd::FileDialog::new()
        .add_filter("Certificado A1", &["pfx", "p12"])
        .pick_file()
    else {
        return;
    };
    match std::fs::read(&caminho) {
        Ok(bytes) => {
            let nome = caminho
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            estado.arquivo = Some((nome, bytes));
        }
        Err(e) => notificar(ctx, Notificacao::erro(format!("Não foi possível ler: {e}"))),
    }
}

fn secao_nfce(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoFiscal,
    pode_configurar: bool,
) {
    let f = &mut estado.nfce;
    let (e_serie, e_proximo, e_csc) = (
        f.erro_em("serie"),
        f.erro_em("proximo_numero"),
        f.erro_em("csc_id"),
    );
    let mut salvar = false;
    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("NFC-e nas vendas do PDV"));
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::apoio(
            "Ligada, cada venda finalizada vira NFC-e na hora e vai para a SEFAZ em segundo plano — a venda não espera. Sem internet, a nota espera na fila.",
        ));
        ui.add_space(Espaco::E12);
        ui.horizontal(|ui| {
            for (valor, rotulo) in [(true, "Emitir a cada venda"), (false, "Não emitir")] {
                let b = if f.emitir == valor {
                    Botao::primario(rotulo).pequeno()
                } else {
                    Botao::fantasma(rotulo).pequeno()
                };
                if ui.add_enabled(pode_configurar, b).clicked() {
                    f.emitir = valor;
                }
            }
        });
        ui.add_space(Espaco::E12);
        ui.columns(2, |c| {
            c[0].add(
                Campo::novo("Série", &mut f.serie)
                    .marcador("1")
                    .erro(e_serie),
            );
            c[1].add(
                Campo::novo("Próximo número", &mut f.proximo)
                    .marcador("1")
                    .erro(e_proximo),
            );
        });
        ui.add_space(Espaco::E12);
        ui.columns(2, |c| {
            c[0].add(
                Campo::novo("Identificador do CSC", &mut f.csc_id)
                    .marcador("000001")
                    .erro(e_csc),
            );
            c[1].add(
                Campo::novo("Token do CSC", &mut f.csc_token)
                    .senha(true)
                    .marcador(if f.csc_cadastrado {
                        "guardado — deixe em branco para manter"
                    } else {
                        "o código da SEFAZ"
                    }),
            );
        });
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::apoio(
            "O CSC é gerado no portal da SEFAZ do seu estado (um para homologação, outro para produção). O token fica cifrado nesta instalação.",
        ));
        ui.add_space(Espaco::E16);
        salvar = pode_configurar && ui.add(Botao::primario("Salvar NFC-e")).clicked();
    });
    if salvar {
        match salvar_nfce(motor, sessao, estado) {
            Ok(()) => notificar(
                ui.ctx(),
                Notificacao::sucesso("Configuração da NFC-e salva"),
            ),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
}

fn secao_conexao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoFiscal,
    pode_configurar: bool,
) {
    let atual = estado
        .situacao
        .as_ref()
        .map_or(AmbienteFiscal::Homologacao, |s| s.ambiente);
    cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Conexão com a SEFAZ"));
        ui.add_space(Espaco::E12);
        ui.add(Rotulo::sobrelinha("Ambiente"));
        ui.add_space(Espaco::E4);
        ui.horizontal(|ui| {
            for (ambiente, rotulo) in [
                (AmbienteFiscal::Homologacao, "Homologação"),
                (AmbienteFiscal::Producao, "Produção"),
            ] {
                let b = if ambiente == atual {
                    Botao::primario(rotulo).pequeno()
                } else {
                    Botao::fantasma(rotulo).pequeno()
                };
                if ui.add_enabled(pode_configurar, b).clicked() && ambiente != atual {
                    match definir_ambiente(motor, sessao, estado, ambiente) {
                        Ok(()) => {
                            notificar(ui.ctx(), Notificacao::sucesso("Ambiente fiscal trocado"))
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                    }
                }
            }
        });
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::apoio(
            "Em homologação as notas não têm valor fiscal. Passe para produção depois de emitir notas de teste.",
        ));
        ui.add_space(Espaco::E16);
        if ui.add(Botao::secundario("Testar conexão")).clicked() {
            testar_conexao(motor, sessao, estado);
        }
        for (modelo, resultado) in &estado.teste {
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                let nome = match modelo {
                    ModeloFiscal::Nfe => "NF-e",
                    ModeloFiscal::Nfce => "NFC-e",
                };
                ui.add(Rotulo::interface(nome));
                ui.add_space(Espaco::E8);
                match resultado {
                    Ok(s) if s.em_operacao => {
                        ui.add(Etiqueta::positiva("em operação").com_ponto());
                        ui.add(Rotulo::campo(format!("{} {}", s.codigo, s.motivo)));
                    }
                    Ok(s) => {
                        ui.add(Etiqueta::atencao("instável").com_ponto());
                        ui.add(Rotulo::campo(format!("{} {}", s.codigo, s.motivo)).quebravel());
                    }
                    Err(m) => {
                        ui.add(Etiqueta::negativa("falhou").com_ponto());
                        ui.add(Rotulo::campo(m.clone()).quebravel());
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    fn pfx(nome: &str) -> Vec<u8> {
        let caminho = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../cardeal-fiscal/tests/dados")
            .join(nome);
        std::fs::read(caminho).expect("certificado de teste")
    }

    fn desenhar(motor: &Motor, sessao: &Sessao, estado: &mut EstadoFiscal) {
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    mostrar(ui, motor, sessao, estado);
                });
            });
        }
    }

    #[test]
    fn aba_fiscal_dados_certificado_ambiente_e_conexao() {
        let t = motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let mut estado = EstadoFiscal::default();
        estado.carregar(m, s);
        assert!(!estado.situacao.as_ref().unwrap().pronta_para_emitir());
        desenhar(m, s, &mut estado);

        // Dados fiscais: o erro volta marcado no campo; corrigido, grava normalizado.
        estado.form = FormDadosFiscais {
            ie: "255.123.456".into(),
            cep: "123".into(),
            uf: Some(Uf::Sc),
            logradouro: "Rua das Flores".into(),
            numero: "100".into(),
            bairro: "Centro".into(),
            municipio: "Piratuba".into(),
            codigo_municipio: "4213104".into(),
            ..FormDadosFiscais::default()
        };
        assert!(salvar_dados(m, s, &mut estado).is_err());
        assert!(estado.erro_em("cep").is_some());
        estado.form.cep = "89667-000".into();
        salvar_dados(m, s, &mut estado).unwrap();
        assert_eq!(estado.form.cep, "89667000");
        assert_eq!(estado.form.ie, "255123456");
        assert!(estado.erro_dados.is_none());

        // Certificado: sem arquivo não instala; o de RC2 instala regravado e a senha some
        // do formulário.
        assert!(instalar(m, s, &mut estado).is_err());
        estado.arquivo = Some(("legado.pfx".into(), pfx("legado.pfx")));
        estado.senha = "errada".into();
        assert!(instalar(m, s, &mut estado)
            .unwrap_err()
            .mensagem
            .contains("senha"));
        estado.senha = "cardeal-teste".into();
        let c = instalar(m, s, &mut estado).unwrap();
        assert!(c.convertido);
        assert!(estado.senha.is_empty() && estado.arquivo.is_none());
        assert!(estado.situacao.as_ref().unwrap().pronta_para_emitir());

        // Conexão (fiscal simulado nos testes) e ambiente.
        testar_conexao(m, s, &mut estado);
        assert_eq!(estado.teste.len(), 2);
        assert!(estado
            .teste
            .iter()
            .all(|(_, r)| r.as_ref().is_ok_and(|s| s.em_operacao)));
        definir_ambiente(m, s, &mut estado, AmbienteFiscal::Producao).unwrap();
        assert_eq!(
            estado.situacao.as_ref().unwrap().ambiente,
            AmbienteFiscal::Producao
        );

        // NFC-e: sem token não liga; com token, liga e guarda (o token não volta para a tela).
        estado.nfce.emitir = true;
        estado.nfce.csc_id = "000001".into();
        assert!(salvar_nfce(m, s, &mut estado).is_err());
        assert!(estado.nfce.erro_em("csc_id").is_some());
        estado.nfce.csc_token = "TOKEN".into();
        estado.nfce.proximo = "abc".into();
        assert!(salvar_nfce(m, s, &mut estado).is_err());
        assert!(estado.nfce.erro_em("proximo_numero").is_some());
        estado.nfce.proximo = "10".into();
        salvar_nfce(m, s, &mut estado).unwrap();
        assert!(estado.nfce.emitir && estado.nfce.csc_cadastrado);
        assert!(estado.nfce.csc_token.is_empty());
        assert_eq!(estado.nfce.proximo, "10");

        // A aba desenha com certificado, no modo de troca e com o resultado do teste.
        testar_conexao(m, s, &mut estado);
        estado.trocando = true;
        desenhar(m, s, &mut estado);
    }
}
