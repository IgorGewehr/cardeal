//! "Enviar orçamento" e "Relatório do serviço": o dialog que monta o PDF a partir de qualquer
//! OS (aberta, concluída ou faturada). Escolhe-se mostrar o valor de cada peça e serviço ou só
//! o total, e quais formas de pagamento entram — cada uma com o seu total (Pix e dinheiro com
//! desconto, cartão em até N vezes com ou sem acréscimo). Salvo o PDF, dá para abrir o
//! WhatsApp do cliente com a mensagem pronta (o arquivo vai anexado à mão).

use super::*;
use cardeal_kernel::Arredondamento;
use cardeal_pdf::{gerar_documento_os, FormaPagamentoPdf, OpcoesDocumentoOs, TipoDocumentoOs};

/// O formulário do documento.
#[derive(Clone)]
pub(super) struct FormDocumento {
    /// `true` = relatório do serviço; `false` = orçamento.
    pub relatorio: bool,
    pub detalhado: bool,
    pub pix: bool,
    pub desconto_pix: String,
    pub dinheiro: bool,
    pub desconto_dinheiro: String,
    pub cartao: bool,
    pub parcelas: String,
    pub acrescimo_cartao: String,
    pub validade_dias: String,
    pub observacoes: String,
}

impl FormDocumento {
    /// O formulário de partida: o último usado (do mesmo tipo de documento) ou o padrão.
    pub(super) fn novo(relatorio: bool, ultimo: Option<&Self>) -> Self {
        if let Some(u) = ultimo {
            return Self {
                relatorio,
                observacoes: String::new(),
                ..u.clone()
            };
        }
        Self {
            relatorio,
            detalhado: true,
            pix: true,
            desconto_pix: "5".to_owned(),
            dinheiro: true,
            desconto_dinheiro: "5".to_owned(),
            cartao: true,
            parcelas: "10".to_owned(),
            acrescimo_cartao: "0".to_owned(),
            validade_dias: "7".to_owned(),
            observacoes: String::new(),
        }
    }
}

fn percentual(texto: &str) -> Option<Percentual> {
    if texto.trim().is_empty() {
        return Some(Percentual::ZERO);
    }
    Percentual::de_str(texto)
        .ok()
        .filter(|p| *p >= Percentual::ZERO && *p <= Percentual::CEM)
}

fn parcelas(texto: &str) -> Option<i64> {
    texto
        .trim()
        .parse::<i64>()
        .ok()
        .filter(|n| (1..=24).contains(n))
}

/// As formas de pagamento marcadas, com o total de cada uma. `Err` traz o campo inválido.
pub(super) fn formas_de_pagamento(
    f: &FormDocumento,
    total: Dinheiro,
) -> Result<Vec<FormaPagamentoPdf>, &'static str> {
    let mut formas = Vec::new();
    let a_vista = |nome: &str, texto: &str, campo: &'static str| {
        let p = percentual(texto).ok_or(campo)?;
        let valor = total - total.aplicar(p, Arredondamento::MeioAcima);
        let condicao = if p.e_zero() {
            "à vista".to_owned()
        } else {
            format!("à vista, {} de desconto", p.formatar_com_simbolo())
        };
        Ok::<_, &'static str>(FormaPagamentoPdf {
            forma: nome.to_owned(),
            condicao,
            total: valor,
        })
    };
    if f.pix {
        formas.push(a_vista("Pix", &f.desconto_pix, "Desconto no Pix")?);
    }
    if f.dinheiro {
        formas.push(a_vista(
            "Dinheiro",
            &f.desconto_dinheiro,
            "Desconto no dinheiro",
        )?);
    }
    if f.cartao {
        let n = parcelas(&f.parcelas).ok_or("Parcelas (1 a 24)")?;
        let p = percentual(&f.acrescimo_cartao).ok_or("Acréscimo no cartão")?;
        let valor = total + total.aplicar(p, Arredondamento::MeioAcima);
        let parcela = Dinheiro::centavos((valor.em_centavos() + n - 1) / n);
        let juros = if p.e_zero() {
            " sem juros".to_owned()
        } else {
            String::new()
        };
        formas.push(FormaPagamentoPdf {
            forma: "Cartão de crédito".to_owned(),
            condicao: if n == 1 {
                format!("à vista no crédito{juros}")
            } else {
                format!("em até {n}x de {}{juros}", parcela.formatar_com_simbolo())
            },
            total: valor,
        });
    }
    Ok(formas)
}

/// O dialog. Fecha voltando ao detalhe da OS.
pub(super) fn dialogo_documento(ctx: &egui::Context, estado: &mut EstadoTelaOs) {
    let Some(detalhe) = estado.detalhe.clone() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let Dlg::Documento(f) = &estado.dlg else {
        return;
    };
    let relatorio = f.relatorio;
    let os = &detalhe.ordem;
    let titulo = if relatorio {
        "Relatório do serviço"
    } else {
        "Enviar orçamento"
    };
    let descricao = format!(
        "OS #{} · {} · total {}",
        os.numero,
        nome_cliente(&estado.clientes, os.cliente),
        os.valor_total.formatar_com_simbolo()
    );
    let telefone = estado
        .clientes
        .iter()
        .find(|c| c.pessoa == os.cliente)
        .and_then(|c| c.telefone.clone());

    let mut acao: Option<bool> = None; // Some(abrir o WhatsApp?)
    let mut cancelar = false;
    let fechar = Dialogo::nova(titulo).descricao(descricao).medio().mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Documento(f) = &mut estado.dlg else {
                return;
            };
            corpo(ui, f, os.valor_total);
        },
        |ui, _| {
            if ui.add(Botao::primario("Gerar PDF")).clicked() {
                acao = Some(false);
            }
            if telefone.is_some()
                && ui
                    .add(Botao::secundario("Gerar e abrir WhatsApp"))
                    .clicked()
            {
                acao = Some(true);
            }
            if ui.add(Botao::fantasma("Cancelar")).clicked() {
                cancelar = true;
            }
        },
    );
    if fechar || cancelar {
        estado.dlg = Dlg::Detalhe;
        return;
    }
    if let Some(whatsapp) = acao {
        if gerar(
            ctx,
            estado,
            &detalhe,
            whatsapp.then_some(telefone).flatten(),
        ) {
            estado.dlg = Dlg::Detalhe;
        }
    }
}

fn corpo(ui: &mut egui::Ui, f: &mut FormDocumento, total: Dinheiro) {
    Painel::novo()
        .plano()
        .compacto()
        .titulo("Valores no documento")
        .mostrar(ui, |ui| {
            ui.add(Caixa::nova(
                &mut f.detalhado,
                "Mostrar o valor de cada peça e serviço",
            ));
            ui.add(Rotulo::apoio(if f.detalhado {
                "Sai a tabela com quantidade, valor unitário e total de cada item."
            } else {
                "Sai só a lista do que está incluído e o valor total."
            }));
        });
    ui.add_space(Espaco::E12);

    let previa = formas_de_pagamento(f, total);
    Painel::novo()
        .plano()
        .compacto()
        .titulo("Formas de pagamento")
        .mostrar(ui, |ui| {
            let valor_de = |forma: &str| {
                previa
                    .as_ref()
                    .ok()
                    .and_then(|v| v.iter().find(|x| x.forma == forma))
                    .map(|x| (x.total, x.condicao.clone()))
            };
            linha_forma(ui, &mut f.pix, "Pix", valor_de("Pix"), |ui| {
                ui.add(Campo::novo("Desconto %", &mut f.desconto_pix).marcador("0"));
            });
            linha_forma(
                ui,
                &mut f.dinheiro,
                "Dinheiro",
                valor_de("Dinheiro"),
                |ui| {
                    ui.add(Campo::novo("Desconto %", &mut f.desconto_dinheiro).marcador("0"));
                },
            );
            linha_forma(
                ui,
                &mut f.cartao,
                "Cartão de crédito",
                valor_de("Cartão de crédito"),
                |ui| {
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Parcelas", &mut f.parcelas).marcador("10"));
                        c[1].add(Campo::novo("Acréscimo %", &mut f.acrescimo_cartao).marcador("0"));
                    });
                },
            );
            if let Err(campo) = &previa {
                ui.add(Etiqueta::negativa(format!("Confira: {campo}")));
            }
        });
    ui.add_space(Espaco::E12);

    ui.columns(2, |c| {
        if f.relatorio {
            c[0].add(Rotulo::apoio(
                "O relatório leva o diagnóstico, o que foi feito, os valores e a garantia.",
            ));
        } else {
            c[0].add(Campo::novo("Validade (dias)", &mut f.validade_dias).marcador("7"));
        }
        c[1].add(
            Campo::novo("Observações", &mut f.observacoes)
                .marcador("Opcional — sai no fim do documento"),
        );
    });
}

/// Uma forma: a caixa, os campos dela (se marcada) e o total à direita.
fn linha_forma(
    ui: &mut egui::Ui,
    marcada: &mut bool,
    nome: &str,
    valor: Option<(Dinheiro, String)>,
    campos: impl FnOnce(&mut egui::Ui),
) {
    ui.columns(3, |c| {
        c[0].add_space(Espaco::E24);
        c[0].add(Caixa::nova(marcada, nome));
        if *marcada {
            campos(&mut c[1]);
            if let Some((total, condicao)) = valor {
                c[2].add_space(Espaco::E24);
                c[2].add(ValorDinheiro::novo(total));
                c[2].add(Rotulo::apoio(condicao));
            }
        }
    });
    ui.add_space(Espaco::E8);
}

/// Monta e salva o PDF; com `whatsapp`, abre a conversa com a mensagem. `true` se salvou.
fn gerar(
    ctx: &egui::Context,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
    whatsapp: Option<String>,
) -> bool {
    let Dlg::Documento(f) = &estado.dlg else {
        return false;
    };
    let f = (**f).clone();
    let os = &detalhe.ordem;
    let formas = match formas_de_pagamento(&f, os.valor_total) {
        Ok(v) => v,
        Err(campo) => {
            notificar(
                ctx,
                Notificacao::aviso(format!("Confira o campo \"{campo}\".")),
            );
            return false;
        }
    };
    let tipo = if f.relatorio {
        TipoDocumentoOs::Relatorio
    } else {
        let dias = f
            .validade_dias
            .trim()
            .parse::<i32>()
            .unwrap_or(7)
            .clamp(1, 365);
        TipoDocumentoOs::Orcamento {
            validade: Data::hoje(Fuso::BRASILIA).mais_dias(dias),
        }
    };
    let opcoes = OpcoesDocumentoOs {
        tipo,
        detalhado: f.detalhado,
        formas_pagamento: formas,
        observacoes: f.observacoes.trim().to_owned(),
    };
    let doc = montar_documento(estado, detalhe);
    let bytes = match gerar_documento_os(
        &identidade_empresa(estado),
        &doc,
        &opcoes,
        Instante::agora(),
    ) {
        Ok(b) => b,
        Err(e) => {
            notificar(ctx, Notificacao::erro(format!("PDF: {e}")));
            return false;
        }
    };
    let nome = if f.relatorio {
        format!("Relatorio-OS-{:04}.pdf", os.numero)
    } else {
        format!("Orcamento-OS-{:04}.pdf", os.numero)
    };
    if !salvar_e_abrir_pdf(ctx, &bytes, &nome) {
        return false;
    }
    estado.ultimo_documento = Some(Box::new(f.clone()));
    if let Some(tel) = whatsapp {
        let primeiro_nome = doc.cliente_nome.split_whitespace().next().unwrap_or("");
        let texto = if f.relatorio {
            format!(
                "Olá, {primeiro_nome}! Segue o relatório do serviço da OS #{} ({}). \
                 Qualquer dúvida, estamos à disposição.",
                os.numero,
                equipamento_label(os)
            )
        } else {
            format!(
                "Olá, {primeiro_nome}! Segue o orçamento da OS #{} ({}): {}. \
                 Podemos seguir com o reparo?",
                os.numero,
                equipamento_label(os),
                os.valor_total.formatar_com_simbolo()
            )
        };
        if let Some(link) = crate::telefone::link_whatsapp(&tel, &texto) {
            if let Err(e) = open::that_detached(&link) {
                notificar(
                    ctx,
                    Notificacao::erro("Não foi possível abrir o WhatsApp").detalhe(e.to_string()),
                );
            } else {
                notificar(
                    ctx,
                    Notificacao::info("WhatsApp aberto — anexe o PDF que acabou de ser salvo."),
                );
            }
        }
    }
    true
}

#[cfg(test)]
mod testes_formas {
    use super::*;

    #[test]
    fn pix_com_desconto_e_cartao_parcelado() {
        let f = FormDocumento::novo(false, None);
        let formas = formas_de_pagamento(&f, Dinheiro::reais(100)).unwrap();
        assert_eq!(formas.len(), 3);
        assert_eq!(formas[0].forma, "Pix");
        assert_eq!(formas[0].total, Dinheiro::reais(95));
        assert_eq!(formas[1].total, Dinheiro::reais(95));
        assert_eq!(formas[2].total, Dinheiro::reais(100));
        assert_eq!(formas[2].condicao, "em até 10x de R$ 10,00 sem juros");
    }

    #[test]
    fn acrescimo_no_cartao_e_campo_invalido() {
        let mut f = FormDocumento::novo(true, None);
        f.pix = false;
        f.dinheiro = false;
        f.acrescimo_cartao = "10".to_owned();
        let formas = formas_de_pagamento(&f, Dinheiro::reais(100)).unwrap();
        assert_eq!(formas[0].total, Dinheiro::reais(110));
        assert_eq!(formas[0].condicao, "em até 10x de R$ 11,00");
        f.parcelas = "0".to_owned();
        assert_eq!(
            formas_de_pagamento(&f, Dinheiro::reais(100)).unwrap_err(),
            "Parcelas (1 a 24)"
        );
    }
}
