//! Configurações › Fiscal › **Série da NFC-e por caixa**: este computador é um terminal
//! (`crate::maquina::terminal`) e pode numerar as notas na sua série; quem não tem série própria
//! usa a da empresa. A lista mostra os outros caixas com série, para tirar a de um que saiu de uso.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Id};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_fiscal::{DefinirSerieDoTerminal, SerieDoTerminal, SeriesDosTerminais};

/// O formulário deste caixa e a lista dos caixas com série.
#[derive(Default, Clone)]
pub struct EstadoCaixa {
    pub series: Vec<SerieDoTerminal>,
    pub nome: String,
    pub serie: String,
    /// Vazio = manter a numeração.
    pub proximo: String,
    pub erro: Option<(String, String)>,
}

impl EstadoCaixa {
    /// Lê os caixas com série e preenche o formulário com o deste computador.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        self.series = motor
            .consultar(
                sessao,
                "fiscal.series_dos_terminais.v1",
                &SeriesDosTerminais,
            )
            .unwrap_or_default();
        self.preencher(crate::maquina::terminal());
    }

    fn preencher(&mut self, este: Id) {
        match self.series.iter().find(|s| s.terminal == este) {
            Some(s) => {
                self.nome.clone_from(&s.nome);
                self.serie = s.serie.to_string();
                self.proximo = s.proximo_numero.to_string();
            }
            None => {
                self.serie.clear();
                self.proximo.clear();
            }
        }
        self.erro = None;
    }

    fn erro_em(&self, campo: &str) -> Option<String> {
        self.erro
            .as_ref()
            .filter(|(c, _)| c == campo)
            .map(|(_, m)| m.clone())
    }
}

fn numero(texto: &str, campo: &str, nome: &str) -> Result<u64, Erro> {
    texto.trim().replace('.', "").parse::<u64>().map_err(|_| {
        Erro::novo(CodigoErro::VALOR_INVALIDO, format!("{nome}: só números")).no_campo(campo)
    })
}

/// Dá (ou tira, com `serie: None`) a série de um terminal e guarda a lista que volta.
fn definir(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoCaixa,
    pedido: DefinirSerieDoTerminal,
) -> Result<(), Erro> {
    let series: Vec<SerieDoTerminal> =
        motor.executar(sessao, "fiscal.definir_serie_do_terminal.v1", &pedido)?;
    estado.series = series;
    Ok(())
}

/// Grava a série deste computador.
///
/// # Errors
/// Número inválido no formulário ou o erro do comando (com o campo marcado).
pub fn salvar_este(motor: &Motor, sessao: &Sessao, estado: &mut EstadoCaixa) -> Result<(), Erro> {
    let este = crate::maquina::terminal();
    let r = numero(&estado.serie, "serie", "Série")
        .and_then(|s| {
            u32::try_from(s).map_err(|_| {
                Erro::novo(CodigoErro::FORA_DA_FAIXA, "Série grande demais").no_campo("serie")
            })
        })
        .and_then(|serie| {
            let proximo = if estado.proximo.trim().is_empty() {
                None
            } else {
                Some(numero(&estado.proximo, "proximo_numero", "Próximo número")?)
            };
            Ok((serie, proximo))
        })
        .and_then(|(serie, proximo_numero)| {
            let pedido = DefinirSerieDoTerminal {
                terminal: este,
                nome: estado.nome.clone(),
                serie: Some(serie),
                proximo_numero,
            };
            definir(motor, sessao, estado, pedido)
        });
    match r {
        Ok(()) => {
            estado.preencher(este);
            Ok(())
        }
        Err(e) => {
            estado.erro = e.campo.clone().map(|c| (c, e.mensagem.clone()));
            Err(e)
        }
    }
}

/// Tira a série própria de um terminal (ele volta à da empresa).
fn tirar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoCaixa,
    terminal: Id,
) -> Result<(), Erro> {
    let pedido = DefinirSerieDoTerminal {
        terminal,
        nome: String::new(),
        serie: None,
        proximo_numero: None,
    };
    definir(motor, sessao, estado, pedido)?;
    if terminal == crate::maquina::terminal() {
        estado.preencher(terminal);
    }
    Ok(())
}

/// Desenha a seção.
pub fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoCaixa,
    pode: bool,
) {
    let este = crate::maquina::terminal();
    let tem_serie = estado.series.iter().any(|s| s.terminal == este);
    let (e_nome, e_serie, e_proximo) = (
        estado.erro_em("nome"),
        estado.erro_em("serie"),
        estado.erro_em("proximo_numero"),
    );
    let (mut salvar, mut tirar_de) = (false, None);
    super::cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Série da NFC-e por caixa"));
        ui.add_space(Espaco::E4);
        ui.add(
            Rotulo::apoio(
                "Cada caixa pode numerar as notas na sua série; o que não tem série própria usa a \
                 da empresa (acima). Este computador é um caixa: dê a ele um nome e uma série. O \
                 próximo número só precisa ser informado se a série já foi usada em outro sistema.",
            )
            .quebravel(),
        );
        ui.add_space(Espaco::E12);
        ui.columns(3, |c| {
            c[0].add(
                Campo::novo("Nome deste caixa", &mut estado.nome)
                    .marcador("Caixa 2")
                    .erro(e_nome),
            );
            c[1].add(
                Campo::novo("Série", &mut estado.serie)
                    .marcador("2")
                    .erro(e_serie),
            );
            c[2].add(
                Campo::novo("Próximo número", &mut estado.proximo)
                    .marcador("1")
                    .erro(e_proximo),
            );
        });
        ui.add_space(Espaco::E12);
        ui.horizontal(|ui| {
            salvar = pode
                && ui
                    .add(Botao::primario("Salvar a série deste caixa"))
                    .clicked();
            if pode && tem_serie && ui.add(Botao::fantasma("Usar a série da empresa")).clicked() {
                tirar_de = Some(este);
            }
        });
        if !estado.series.is_empty() {
            ui.add_space(Espaco::E12);
            for s in &estado.series {
                ui.horizontal(|ui| {
                    ui.add(Rotulo::campo(format!(
                        "{} — série {}, próximo nº {}",
                        s.nome, s.serie, s.proximo_numero
                    )));
                    if s.terminal == este {
                        ui.add(Etiqueta::neutra("este computador"));
                    } else if pode && ui.add(Botao::fantasma("Tirar a série").pequeno()).clicked()
                    {
                        tirar_de = Some(s.terminal);
                    }
                });
            }
        }
    });
    if salvar {
        match salvar_este(motor, sessao, estado) {
            Ok(()) => notificar(ui.ctx(), Notificacao::sucesso("Série deste caixa salva")),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
    if let Some(t) = tirar_de {
        match tirar(motor, sessao, estado, t) {
            Ok(()) => notificar(
                ui.ctx(),
                Notificacao::sucesso("O caixa voltou à série da empresa"),
            ),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn este_caixa_ganha_e_perde_a_sua_serie() {
        let t = motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let mut e = EstadoCaixa::default();
        e.carregar(m, s);
        assert!(e.series.is_empty());
        // Sem série: o campo é marcado.
        e.nome = "Caixa 2".into();
        assert_eq!(
            salvar_este(m, s, &mut e).unwrap_err().campo.as_deref(),
            Some("serie")
        );
        e.serie = "2".into();
        salvar_este(m, s, &mut e).unwrap();
        assert_eq!(e.series.len(), 1);
        assert_eq!(e.series[0].terminal, crate::maquina::terminal());
        assert_eq!((e.serie.as_str(), e.proximo.as_str()), ("2", "1"));
        assert!(e.erro.is_none());
        // A série da empresa (1) não serve.
        e.serie = "1".into();
        assert!(salvar_este(m, s, &mut e).is_err());
        assert_eq!(e.erro.as_ref().map(|(c, _)| c.as_str()), Some("serie"));
        // Desenha, e volta à série da empresa.
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| secao(ui, m, s, &mut e, true));
        });
        tirar(m, s, &mut e, crate::maquina::terminal()).unwrap();
        assert!(e.series.is_empty() && e.serie.is_empty());
    }
}
