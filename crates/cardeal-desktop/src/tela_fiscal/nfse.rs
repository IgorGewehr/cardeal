//! Configurações › Fiscal › **NFS-e da OS**: o código do serviço (o de tributação nacional e o
//! item da LC 116, mais o municipal e o NBS quando o município pede), o percentual e a série e
//! o próximo número do DPS. A NFS-e sai da OS faturada (a mão de obra).

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{CodigoErro, Erro, Percentual};
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_fiscal::nfse::{ConfiguracaoDaNfse, ConfiguracaoNfse, ConfigurarNfse};

/// O formulário.
#[derive(Default, Clone)]
pub struct FormNfse {
    pub codigo: String,
    pub item: String,
    pub municipal: String,
    pub nbs: String,
    pub aliquota: String,
    pub serie: String,
    pub proximo: String,
    pub erro: Option<(String, String)>,
}

impl FormNfse {
    /// Lê a configuração gravada (vazia: os padrões).
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        match motor.consultar(sessao, "fiscal.configuracao_nfse.v1", &ConfiguracaoDaNfse) {
            Ok(Some(c)) => self.de(c),
            _ => {
                if self.serie.is_empty() {
                    self.serie = "900".into();
                    self.proximo = "1".into();
                }
            }
        }
    }

    fn de(&mut self, c: ConfiguracaoNfse) {
        self.codigo = c.codigo_tributacao_nacional;
        self.item = c.item_lc116;
        self.municipal = c.codigo_tributacao_municipal.unwrap_or_default();
        self.nbs = c.nbs.unwrap_or_default();
        self.aliquota = c.aliquota.formatar(2);
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

fn opcional(t: &str) -> Option<String> {
    Some(t.trim().to_owned()).filter(|t| !t.is_empty())
}

/// Grava a configuração.
///
/// # Errors
/// Número inválido ou o erro do comando (com o campo marcado).
pub fn salvar(motor: &Motor, sessao: &Sessao, f: &mut FormNfse) -> Result<(), Erro> {
    let numero = |texto: &str, campo: &str, nome: &str| {
        texto.trim().replace('.', "").parse::<u64>().map_err(|_| {
            Erro::novo(CodigoErro::VALOR_INVALIDO, format!("{nome}: só números")).no_campo(campo)
        })
    };
    let r = Percentual::de_str(&f.aliquota)
        .map_err(|_| {
            Erro::novo(
                CodigoErro::VALOR_INVALIDO,
                "Percentual: um número, como 6 ou 6,5",
            )
            .no_campo("aliquota")
        })
        .and_then(|aliquota| {
            let serie = u32::try_from(numero(&f.serie, "serie", "Série")?).map_err(|_| {
                Erro::novo(CodigoErro::FORA_DA_FAIXA, "Série grande demais").no_campo("serie")
            })?;
            Ok(ConfigurarNfse {
                codigo_tributacao_nacional: f.codigo.clone(),
                item_lc116: f.item.clone(),
                codigo_tributacao_municipal: opcional(&f.municipal),
                nbs: opcional(&f.nbs),
                aliquota,
                serie,
                proximo_numero: Some(numero(&f.proximo, "proximo_numero", "Próximo número")?),
            })
        })
        .and_then(|cmd| motor.executar(sessao, "fiscal.configurar_nfse.v1", &cmd));
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
pub fn secao(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, f: &mut FormNfse, pode: bool) {
    let erros = [
        "codigo_tributacao_nacional",
        "item_lc116",
        "nbs",
        "aliquota",
        "serie",
        "proximo_numero",
    ]
    .map(|c| f.erro_em(c));
    let mut salvar_agora = false;
    super::cartao(ui, |ui| {
        ui.add(Rotulo::titulo_secao("NFS-e da ordem de serviço"));
        ui.add_space(Espaco::E4);
        ui.add(
            Rotulo::apoio(
                "A NFS-e sai da OS faturada (Ordens de serviço › OS › Emitir NFS-e), com a mão de \
                 obra — as peças são mercadoria e ficam de fora. Precisa da inscrição municipal \
                 nos dados fiscais. O código de tributação nacional está no anexo do Padrão \
                 Nacional (ex.: 14.01.01 para manutenção de máquinas e aparelhos). No Simples, o \
                 percentual é o aproximado dos tributos do Simples (a alíquota efetiva do DAS); \
                 no regime normal, a alíquota do ISS do município.",
            )
            .quebravel(),
        );
        ui.add_space(Espaco::E12);
        let [e_codigo, e_item, e_nbs, e_aliquota, e_serie, e_proximo] = erros;
        ui.columns(2, |c| {
            c[0].add(
                Campo::novo("Código de tributação nacional", &mut f.codigo)
                    .marcador("14.01.01")
                    .erro(e_codigo),
            );
            c[1].add(
                Campo::novo("Item da LC 116", &mut f.item)
                    .marcador("14.01")
                    .erro(e_item),
            );
        });
        ui.add_space(Espaco::E8);
        ui.columns(2, |c| {
            c[0].add(
                Campo::novo("Código municipal (se o município pede)", &mut f.municipal)
                    .marcador("opcional"),
            );
            c[1].add(
                Campo::novo("NBS (opcional)", &mut f.nbs)
                    .marcador("1.0101.00.00")
                    .erro(e_nbs),
            );
        });
        ui.add_space(Espaco::E8);
        ui.columns(3, |c| {
            c[0].add(
                Campo::novo("Percentual / alíquota (%)", &mut f.aliquota)
                    .marcador("6")
                    .erro(e_aliquota),
            );
            c[1].add(
                Campo::novo("Série do DPS", &mut f.serie)
                    .marcador("900")
                    .erro(e_serie),
            );
            c[2].add(
                Campo::novo("Próximo número", &mut f.proximo)
                    .marcador("1")
                    .erro(e_proximo),
            );
        });
        ui.add_space(Espaco::E12);
        salvar_agora = pode && ui.add(Botao::primario("Salvar NFS-e")).clicked();
    });
    if salvar_agora {
        match salvar(motor, sessao, f) {
            Ok(()) => notificar(
                ui.ctx(),
                Notificacao::sucesso("Configuração da NFS-e salva"),
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
    fn grava_o_codigo_o_percentual_e_a_serie() {
        let t = motor_de_teste();
        let mut f = FormNfse::default();
        f.carregar(&t.motor, &t.sessao);
        assert_eq!((f.serie.as_str(), f.proximo.as_str()), ("900", "1"));
        f.codigo = "14.01".into();
        f.item = "14.01".into();
        f.aliquota = "6".into();
        assert_eq!(
            salvar(&t.motor, &t.sessao, &mut f)
                .unwrap_err()
                .campo
                .as_deref(),
            Some("codigo_tributacao_nacional")
        );
        f.codigo = "14.01.01".into();
        f.proximo = "35".into();
        salvar(&t.motor, &t.sessao, &mut f).unwrap();
        assert_eq!(f.codigo, "140101");
        assert_eq!(f.proximo, "35");
        let mut de_novo = FormNfse::default();
        de_novo.carregar(&t.motor, &t.sessao);
        assert_eq!(de_novo.item, "14.01");
    }
}
