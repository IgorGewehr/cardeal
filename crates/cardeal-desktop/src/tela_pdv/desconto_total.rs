//! `Shift+F5`: o desconto no total da venda — "10%" ou "5,00" (fecha a conta num valor
//! redondo). Vale sobre o que os itens somam e acompanha item bipado ou cancelado; o backend
//! confere o teto do papel para a venda inteira e, ao finalizar, rateia o desconto entre os
//! itens (a NFC-e leva o desconto por item).

use super::*;
use mod_pdv::{AplicarDescontoNoTotal, DescontoNoTotal};

/// Lê o que o operador digitou: vazio tira o desconto; "10%" é percentual; "5", "5,00" ou
/// "R$ 5,00" é valor.
pub(super) fn ler_desconto(texto: &str) -> Result<Option<DescontoNoTotal>, &'static str> {
    let t = texto.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let desconto = match t.strip_suffix('%') {
        Some(p) => p
            .trim()
            .parse::<Percentual>()
            .map(DescontoNoTotal::Percentual)
            .map_err(|_| "percentual inválido (ex.: 10% ou 7,5%)")?,
        None => Dinheiro::de_str(t)
            .map(DescontoNoTotal::Valor)
            .map_err(|_| "valor inválido (ex.: 5,00 ou 10%)")?,
    };
    if desconto.valido() {
        Ok(Some(desconto))
    } else {
        Err("use um valor maior que zero ou um percentual até 100%")
    }
}

/// O texto que reabre o diálogo com o desconto atual.
pub(super) fn texto_do_desconto(d: DescontoNoTotal) -> String {
    match d {
        DescontoNoTotal::Percentual(p) => p.formatar_com_simbolo(),
        DescontoNoTotal::Valor(v) => v.formatar(),
    }
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaPdv,
) {
    let aplicar = |estado: &mut EstadoTelaPdv, desconto: Option<DescontoNoTotal>| {
        let Some(cupom) = estado.cupom else {
            estado.dlg = Dlg::Fechado;
            return;
        };
        match motor.executar(
            sessao,
            "pdv.aplicar_desconto_no_total.v1",
            &AplicarDescontoNoTotal { cupom, desconto },
        ) {
            Ok(r) => {
                estado.desconto_no_total = desconto;
                estado.total = r.total;
                estado.dlg = Dlg::Fechado;
                estado.foco_entrada = true;
                notificar(
                    ctx,
                    if desconto.is_some() {
                        Notificacao::sucesso(format!(
                            "Desconto no total: {} — a pagar {}",
                            r.desconto_no_total.formatar_com_simbolo(),
                            r.total.formatar_com_simbolo()
                        ))
                    } else {
                        Notificacao::info("Desconto no total retirado")
                    },
                );
            }
            // Ex.: passa do teto do papel — o diálogo continua aberto para corrigir.
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    };
    let confirmar = |estado: &mut EstadoTelaPdv| {
        let Dlg::DescontoTotal { texto } = &estado.dlg else {
            return;
        };
        match ler_desconto(texto) {
            Ok(d) => aplicar(estado, d),
            Err(e) => notificar(ctx, Notificacao::aviso(format!("Desconto: {e}."))),
        }
    };

    let liquido = estado.liquido_dos_itens();
    let descricao = format!(
        "Itens: {}. Digite 10% ou um valor (5,00); vazio tira o desconto.",
        liquido.formatar_com_simbolo()
    );
    let tem_desconto = estado.desconto_no_total.is_some();
    let fechar = Dialogo::nova("Desconto no total")
        .descricao(descricao)
        .pequeno()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::DescontoTotal { texto } = &mut estado.dlg else {
                    return;
                };
                let resp = ui.add(Campo::novo("Desconto", texto).marcador("10% ou 5,00"));
                focar_primeiro(ctx, &resp);
                // A conta, antes de confirmar: o que sai e quanto fica.
                if let Ok(Some(d)) = ler_desconto(texto) {
                    let tira = d.valor_sobre(liquido);
                    ui.add_space(Espaco::E8);
                    ui.add(Rotulo::campo(format!(
                        "−{} · a pagar {}",
                        tira.formatar_com_simbolo(),
                        (liquido - tira).formatar_com_simbolo()
                    )));
                }
                if resp.lost_focus() && enter_pressionado(ui) {
                    confirmar(estado);
                }
            },
            |ui, estado| {
                if ui.add(Botao::primario("Aplicar").atalho("Enter")).clicked() {
                    confirmar(estado);
                }
                if tem_desconto && ui.add(Botao::secundario("Tirar desconto")).clicked() {
                    aplicar(estado, None);
                }
                if ui
                    .add(Botao::secundario("Cancelar").atalho("Esc"))
                    .clicked()
                {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_percentual_valor_e_vazio() {
        assert_eq!(ler_desconto(""), Ok(None));
        assert_eq!(
            ler_desconto(" 10 % "),
            Ok(Some(DescontoNoTotal::Percentual(Percentual::pontos(10))))
        );
        assert_eq!(
            ler_desconto("R$ 5,50"),
            Ok(Some(DescontoNoTotal::Valor(Dinheiro::centavos(550))))
        );
        assert!(ler_desconto("0").is_err());
        assert!(ler_desconto("150%").is_err());
        assert!(ler_desconto("abc").is_err());
        let d = DescontoNoTotal::Percentual(Percentual::de_str("7,5").unwrap());
        assert_eq!(ler_desconto(&texto_do_desconto(d)), Ok(Some(d)));
    }
}
