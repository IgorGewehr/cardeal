//! Renegociar o saldo de um título a receber (`financeiro.renegociar_titulo.v1`): o cliente
//! não vai pagar como combinado e o saldo vira parcelas novas, opcionalmente com multa por
//! atraso. O comando já existia; faltava a tela.

use super::*;

/// Os campos da renegociação, como o usuário digita.
#[derive(Debug, Clone, Default)]
pub(super) struct FormRenegociar {
    pub(super) parcelas: String,
    pub(super) primeiro_vencimento: String,
    pub(super) intervalo_dias: String,
    pub(super) multa: String,
    /// `true` = a confirmação está aberta.
    pub(super) confirmar: bool,
}

impl FormRenegociar {
    pub(super) fn validado(
        &self,
        titulo: Id,
    ) -> Result<mod_financeiro::RenegociarTitulo, &'static str> {
        let parcelas = self
            .parcelas
            .trim()
            .parse::<u16>()
            .ok()
            .filter(|n| (1..=60).contains(n))
            .ok_or("Parcelas: um número de 1 a 60.")?;
        let primeiro_vencimento = self
            .primeiro_vencimento
            .parse::<Data>()
            .map_err(|_| "Informe o 1º vencimento (dd/mm/aaaa).")?;
        let intervalo_dias = if parcelas == 1 {
            0
        } else {
            self.intervalo_dias
                .trim()
                .parse::<i32>()
                .ok()
                .filter(|d| (1..=365).contains(d))
                .ok_or("Intervalo: de 1 a 365 dias.")?
        };
        let multa = match self.multa.trim().trim_end_matches('%').trim() {
            "" => None,
            t => Some(
                cardeal_kernel::Percentual::de_str(t)
                    .map_err(|_| "Multa: um percentual, ex.: 2")?,
            ),
        };
        Ok(mod_financeiro::RenegociarTitulo {
            titulo,
            numero_parcelas: parcelas,
            primeiro_vencimento,
            intervalo_dias,
            politica_juros: mod_financeiro::PoliticaJuros::Nenhum,
            taxa_juros: None,
            multa,
        })
    }
}

/// A seção recolhida "Renegociar o saldo" dentro do diálogo da parcela.
pub(super) fn secao_renegociar(ui: &mut egui::Ui, form: &mut FormRenegociar, saldo: Dinheiro) {
    ui.add_space(Espaco::E16);
    SecaoExpansivel::nova("Renegociar o saldo").mostrar(ui, |ui| {
        ui.add(
            Rotulo::campo(format!(
                "O saldo de {} deste título vira parcelas novas; as atuais são encerradas.",
                saldo.formatar_com_simbolo()
            ))
            .quebravel(),
        );
        ui.add_space(Espaco::E8);
        if form.parcelas.is_empty() {
            form.parcelas = "1".to_owned();
            form.intervalo_dias = "30".to_owned();
            form.primeiro_vencimento = Data::hoje(Fuso::BRASILIA).mais_dias(30).to_string();
        }
        ui.columns(3, |c| {
            c[0].add(Campo::novo("Parcelas", &mut form.parcelas));
            c[1].add(
                Campo::novo("1º vencimento", &mut form.primeiro_vencimento).mascara(Mascara::Data),
            );
            c[2].add(Campo::novo("Intervalo (dias)", &mut form.intervalo_dias));
        });
        ui.add_space(Espaco::E8);
        ui.add(Campo::novo("Multa por atraso (%) — opcional", &mut form.multa).marcador("2"));
        ui.add_space(Espaco::E8);
        if ui.add(Botao::secundario("Renegociar")).clicked() {
            form.confirmar = true;
        }
    });
}

/// A confirmação e a execução — chamada depois do diálogo da parcela, para desenhar por cima.
pub(super) fn confirmar_renegociacao(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    titulo: Id,
) {
    let Dlg::Baixar { renegociar, .. } = &mut estado.dlg else {
        return;
    };
    if !renegociar.confirmar {
        return;
    }
    let comando = match renegociar.validado(titulo) {
        Ok(c) => c,
        Err(msg) => {
            notificar(ctx, Notificacao::aviso(msg));
            renegociar.confirmar = false;
            return;
        }
    };
    let resposta = cardeal_ui::organisms::dialogo_confirmacao(
        ctx,
        "Renegociar título",
        &format!(
            "O saldo em aberto vira {} parcela(s), a primeira em {}. As parcelas atuais são \
             encerradas como renegociadas.",
            comando.numero_parcelas,
            comando.primeiro_vencimento.formatar()
        ),
        "Renegociar",
    );
    if resposta.confirmado {
        match motor.executar(sessao, "financeiro.renegociar_titulo.v1", &comando) {
            Ok(r) => {
                let r: mod_financeiro::TituloFoiRenegociado = r;
                estado.dlg = Dlg::Fechado;
                estado.carregar(motor, sessao);
                notificar(
                    ctx,
                    Notificacao::sucesso(format!(
                        "Título renegociado — {} em {} parcela(s)",
                        r.saldo.formatar_com_simbolo(),
                        comando.numero_parcelas
                    )),
                );
                return;
            }
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    }
    if resposta.fechar || resposta.confirmado {
        if let Dlg::Baixar { renegociar, .. } = &mut estado.dlg {
            renegociar.confirmar = false;
        }
    }
}
