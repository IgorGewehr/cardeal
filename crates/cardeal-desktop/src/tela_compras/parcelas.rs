//! As parcelas da conta a pagar no "Confirmar entrada": as duplicatas da nota (`cobr/dup`) já
//! preenchidas — ou, sem elas, uma parcela vencendo na emissão —, e quem confirma ajusta,
//! divide em N parcelas ou acrescenta uma.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Data, Dinheiro};
use cardeal_ui::atoms::{Botao, CampoTexto, Rotulo};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_compras::{ItemNota, ParcelaCompra, ParcelasDaNota};

/// Uma parcela sendo digitada.
#[derive(Clone)]
struct LinhaParcela {
    numero: String,
    vencimento: String,
    valor: String,
}

/// As parcelas da nota aberta, como estão na tela.
pub(super) struct EditorParcelas {
    linhas: Vec<LinhaParcela>,
    dividir_em: String,
    intervalo: String,
    vieram_da_nota: bool,
}

impl Default for EditorParcelas {
    fn default() -> Self {
        Self {
            linhas: Vec::new(),
            dividir_em: "2".to_owned(),
            intervalo: "30".to_owned(),
            vieram_da_nota: false,
        }
    }
}

impl EditorParcelas {
    /// As parcelas da nota (as duplicatas, ou as usadas numa confirmação anterior); sem elas,
    /// uma parcela do total vencendo na emissão.
    pub(super) fn carregar(motor: &Motor, sessao: &Sessao, nota: &ItemNota) -> Self {
        let da_nota: Vec<ParcelaCompra> = motor
            .consultar(
                sessao,
                "compras.parcelas_da_nota.v1",
                &ParcelasDaNota { nota: nota.nota },
            )
            .unwrap_or_default();
        let vieram_da_nota = !da_nota.is_empty();
        let linhas = if vieram_da_nota {
            da_nota
                .into_iter()
                .map(|p| LinhaParcela {
                    numero: p.numero,
                    vencimento: p.vencimento.to_string(),
                    valor: p.valor.formatar(),
                })
                .collect()
        } else {
            vec![LinhaParcela {
                numero: String::new(),
                vencimento: nota.data_emissao.to_string(),
                valor: nota.valor_total.formatar(),
            }]
        };
        Self {
            linhas,
            vieram_da_nota,
            ..Self::default()
        }
    }

    /// As parcelas digitadas, ou o que está errado nelas.
    pub(super) fn lidas(&self) -> Result<Vec<ParcelaCompra>, String> {
        self.linhas
            .iter()
            .enumerate()
            .map(|(i, l)| {
                let vencimento =
                    l.vencimento.trim().parse::<Data>().map_err(|_| {
                        format!("Parcela {}: vencimento inválido (dd/mm/aaaa).", i + 1)
                    })?;
                let valor = l
                    .valor
                    .trim()
                    .parse::<Dinheiro>()
                    .map_err(|_| format!("Parcela {}: valor inválido.", i + 1))?;
                Ok(ParcelaCompra {
                    numero: l.numero.clone(),
                    vencimento,
                    valor,
                })
            })
            .collect()
    }

    /// Divide o total em `dividir_em` parcelas a cada `intervalo` dias, a partir do vencimento
    /// da 1ª (ou da emissão, se ela não vale).
    fn dividir(&mut self, total: Dinheiro, emissao: Data) -> Result<(), &'static str> {
        let n: usize = self
            .dividir_em
            .trim()
            .parse()
            .ok()
            .filter(|n| (1..=360).contains(n))
            .ok_or("Divida em 1 a 360 parcelas.")?;
        let intervalo: i32 = self
            .intervalo
            .trim()
            .parse()
            .ok()
            .filter(|d| (0..=366).contains(d))
            .ok_or("O intervalo vai de 0 a 366 dias.")?;
        let primeiro = self
            .linhas
            .first()
            .and_then(|l| l.vencimento.trim().parse::<Data>().ok())
            .unwrap_or(emissao);
        self.linhas = total
            .ratear(n)
            .into_iter()
            .enumerate()
            .map(|(k, valor)| LinhaParcela {
                numero: String::new(),
                vencimento: primeiro
                    .mais_dias(i32::try_from(k).unwrap_or(0) * intervalo)
                    .to_string(),
                valor: valor.formatar(),
            })
            .collect();
        Ok(())
    }
}

/// O bloco das parcelas, dentro de "Confirmar entrada".
pub(super) fn secao(
    ui: &mut egui::Ui,
    editor: &mut EditorParcelas,
    total: Dinheiro,
    emissao: Data,
) {
    ui.add(Rotulo::interface("Parcelas a pagar"));
    ui.add(
        Rotulo::apoio(if editor.vieram_da_nota {
            "As duplicatas da nota. Confira — e ajuste se o fornecedor combinou outra coisa."
        } else {
            "A nota não traz cobrança: uma parcela, vencendo na emissão. Se o fornecedor \
             parcelou, divida aqui."
        })
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    let mut remover = None;
    let varias = editor.linhas.len() > 1;
    for (i, l) in editor.linhas.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.scope(|ui| {
                ui.set_width(28.0);
                ui.add(Rotulo::campo(format!("{}.", i + 1)));
            });
            ui.scope(|ui| {
                ui.set_width(130.0);
                ui.add(CampoTexto::novo(&mut l.vencimento).marcador("dd/mm/aaaa"));
            });
            ui.scope(|ui| {
                ui.set_width(120.0);
                ui.add(CampoTexto::novo(&mut l.valor).marcador("0,00"));
            });
            if varias && ui.add(Botao::fantasma("Remover")).clicked() {
                remover = Some(i);
            }
        });
        ui.add_space(Espaco::E4);
    }
    if let Some(i) = remover {
        editor.linhas.remove(i);
    }
    ui.horizontal(|ui| {
        if ui.add(Botao::secundario("Adicionar parcela")).clicked() {
            let ultima = editor.linhas.last().cloned();
            let vencimento = ultima
                .and_then(|l| l.vencimento.trim().parse::<Data>().ok())
                .map_or(emissao, |d| d.mais_dias(30));
            editor.linhas.push(LinhaParcela {
                numero: String::new(),
                vencimento: vencimento.to_string(),
                valor: String::new(),
            });
        }
        ui.add_space(Espaco::E16);
        ui.add(Rotulo::campo("Dividir em"));
        ui.scope(|ui| {
            ui.set_width(48.0);
            ui.add(CampoTexto::novo(&mut editor.dividir_em));
        });
        ui.add(Rotulo::campo("a cada"));
        ui.scope(|ui| {
            ui.set_width(48.0);
            ui.add(CampoTexto::novo(&mut editor.intervalo));
        });
        ui.add(Rotulo::campo("dias"));
        if ui.add(Botao::secundario("Dividir")).clicked() {
            if let Err(e) = editor.dividir(total, emissao) {
                cardeal_ui::organisms::notificar(
                    ui.ctx(),
                    cardeal_ui::organisms::Notificacao::aviso(e),
                );
            }
        }
    });
    ui.add_space(Espaco::E8);
    let (texto, cor) = match editor.lidas() {
        Err(e) => (e, ui.cores().negativo),
        Ok(p) => {
            let soma: Dinheiro = p.iter().map(|p| p.valor).sum();
            if soma == total {
                (
                    format!("Somam {} — o total da nota.", soma.formatar_com_simbolo()),
                    ui.cores().positivo,
                )
            } else {
                (
                    format!(
                        "Somam {}, mas a nota é de {} (diferença de {}).",
                        soma.formatar_com_simbolo(),
                        total.formatar_com_simbolo(),
                        (total - soma).formatar_com_simbolo()
                    ),
                    ui.cores().negativo,
                )
            }
        }
    };
    ui.add(Rotulo::campo(texto).cor(cor));
}

#[cfg(test)]
mod testes {
    use super::*;

    fn editor(linhas: &[(&str, &str)]) -> EditorParcelas {
        EditorParcelas {
            linhas: linhas
                .iter()
                .map(|(v, r)| LinhaParcela {
                    numero: String::new(),
                    vencimento: (*v).to_owned(),
                    valor: (*r).to_owned(),
                })
                .collect(),
            ..EditorParcelas::default()
        }
    }

    #[test]
    fn le_as_parcelas_e_aponta_a_que_esta_errada() {
        let e = editor(&[("01/10/2026", "20,00"), ("01/11/2026", "30,00")]);
        let p = e.lidas().unwrap();
        assert_eq!(p[1].vencimento, Data::de_ymd(2026, 11, 1).unwrap());
        assert_eq!(p[1].valor, Dinheiro::reais(30));
        let e = editor(&[("01/10/2026", "20,00"), ("31/02/2026", "30,00")]);
        assert!(e.lidas().unwrap_err().starts_with("Parcela 2"));
    }

    #[test]
    fn dividir_rateia_o_total_a_partir_do_primeiro_vencimento() {
        let emissao = Data::de_ymd(2026, 9, 1).unwrap();
        let mut e = editor(&[("15/09/2026", "100,00")]);
        "3".clone_into(&mut e.dividir_em);
        e.dividir(Dinheiro::reais(100), emissao).unwrap();
        let p = e.lidas().unwrap();
        let soma: Dinheiro = p.iter().map(|p| p.valor).sum();
        assert_eq!(soma, Dinheiro::reais(100));
        assert_eq!(p.len(), 3);
        assert_eq!(p[0].vencimento, Data::de_ymd(2026, 9, 15).unwrap());
        assert_eq!(p[2].vencimento, Data::de_ymd(2026, 11, 14).unwrap());
        "0".clone_into(&mut e.dividir_em);
        assert!(e.dividir(Dinheiro::reais(100), emissao).is_err());
    }
}
