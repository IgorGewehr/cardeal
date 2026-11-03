//! Configurações › Fiscal › **NF-e de venda**: a série e o próximo número da NF-e (modelo 55),
//! separados dos da NFC-e. O próximo número existe para continuar a numeração de outro sistema.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{CodigoErro, Erro};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_fiscal::{ConfiguracaoDaNfe, ConfiguracaoNfe, ConfigurarNfe};

/// O formulário.
#[derive(Default, Clone)]
pub struct FormNfe {
    pub serie: String,
    pub proximo: String,
    pub erro: Option<(String, String)>,
}

impl FormNfe {
    /// Lê a configuração gravada.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(c) = motor.consultar(sessao, "fiscal.configuracao_nfe.v1", &ConfiguracaoDaNfe) {
            self.de(c);
        }
    }

    fn de(&mut self, c: ConfiguracaoNfe) {
        self.serie = c.serie.to_string();
        self.proximo = c.proximo_numero.to_string();
        self.erro = None;
    }

    fn erro_em(&self, campo: &str) -> Option<String> {
        self.erro
            .as_ref()
            .filter(|(c, _)| c == campo)
            .map(|(_, m)| m.clone())
    }
}

/// Grava a série e o próximo número.
///
/// # Errors
/// Número inválido ou o erro do comando (com o campo marcado).
pub fn salvar(motor: &Motor, sessao: &Sessao, f: &mut FormNfe) -> Result<(), Erro> {
    let numero = |texto: &str, campo: &str, nome: &str| {
        texto.trim().replace('.', "").parse::<u64>().map_err(|_| {
            Erro::novo(CodigoErro::VALOR_INVALIDO, format!("{nome}: só números")).no_campo(campo)
        })
    };
    let r = numero(&f.serie, "serie", "Série")
        .and_then(|s| {
            u32::try_from(s).map_err(|_| {
                Erro::novo(CodigoErro::FORA_DA_FAIXA, "Série grande demais").no_campo("serie")
            })
        })
        .and_then(|serie| {
            Ok(ConfigurarNfe {
                serie,
                proximo_numero: numero(&f.proximo, "proximo_numero", "Próximo número")?,
            })
        })
        .and_then(|cmd| motor.executar(sessao, "fiscal.configurar_nfe.v1", &cmd));
    match r {
        Ok(c) => {
            f.de(c);
            Ok(())
        }
        Err(e) => {
            f.erro = e.campo.clone().map(|c| (c, e.mensagem.clone()));
            Err(e)
        }
    }
}

/// Desenha a seção.
pub fn secao(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, f: &mut FormNfe, pode: bool) {
    let (e_serie, e_proximo) = (f.erro_em("serie"), f.erro_em("proximo_numero"));
    let mut salvar_agora = false;
    super::cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("NF-e de venda"));
        ui.add_space(Espaco::E4);
        ui.add(
            Rotulo::apoio(
                "A NF-e sai do pedido de vendas faturado (Vendas › pedido › Emitir NF-e), com o \
                 cliente do pedido como destinatário. A série e a numeração são separadas das da \
                 NFC-e; o próximo número só precisa mudar para continuar a de outro sistema.",
            )
            .quebravel(),
        );
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
        salvar_agora = pode && ui.add(Botao::primario("Salvar NF-e")).clicked();
    });
    if salvar_agora {
        match salvar(motor, sessao, f) {
            Ok(()) => notificar(ui.ctx(), Notificacao::sucesso("Configuração da NF-e salva")),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn grava_a_serie_e_o_proximo_numero() {
        let t = motor_de_teste();
        let mut f = FormNfe::default();
        f.carregar(&t.motor, &t.sessao);
        assert_eq!((f.serie.as_str(), f.proximo.as_str()), ("1", "1"));
        f.serie = "2".into();
        f.proximo = "abc".into();
        assert_eq!(
            salvar(&t.motor, &t.sessao, &mut f)
                .unwrap_err()
                .campo
                .as_deref(),
            Some("proximo_numero")
        );
        f.proximo = "1.500".into();
        salvar(&t.motor, &t.sessao, &mut f).unwrap();
        assert_eq!((f.serie.as_str(), f.proximo.as_str()), ("2", "1500"));
    }
}
