//! O diálogo de pagamento e o seu fluxo de teclado (sem motor: só empilha `Acao::Finalizar`).

use super::*;

// ── pagamento (sem motor: só empilha `Acao::Finalizar`) ──────────────────────

/// Tira do quadro o texto que uma tecla-atalho geraria — sem isso, ao escolher a forma com
/// `1` o dígito também seria digitado no campo de valor que acabou de ganhar o foco.
pub(super) fn descartar_texto(ctx: &egui::Context, texto: &str) {
    ctx.input_mut(|i| {
        i.events
            .retain(|e| !matches!(e, egui::Event::Text(t) if t == texto));
    });
}

/// Confirma o valor digitado na forma ativa. Vazio = "o que falta". `Err` = texto inválido.
pub(super) fn confirmar_valor(p: &mut EstadoPagamento, total: Dinheiro) -> Result<(), ()> {
    let texto = p.digitado.trim();
    let valor = if texto.is_empty() {
        pagamento::restante_para(total, &p.valores, p.ativa)
    } else {
        match texto.parse::<Dinheiro>() {
            Ok(v) if !v.e_negativo() => v,
            _ => return Err(()),
        }
    };
    p.valores[p.ativa] = valor;
    p.digitado.clear();
    Ok(())
}

/// As teclas do diálogo de pagamento, tratadas **antes** de desenhar.
///
/// Dois modos, decididos pelo foco do campo de valor: **escolhendo a forma** (`1..4` escolhem;
/// `Enter`/`F2` finalizam se fechou, senão passam a digitar) e **digitando o valor** (os
/// dígitos são o valor; `Enter` confirma; `Esc` volta ao modo anterior).
pub(super) fn teclas_pagamento(ctx: &egui::Context, estado: &mut EstadoTelaPdv) {
    let total = estado.total;
    let Dlg::Pagamento(p) = &mut estado.dlg else {
        return;
    };
    if p.digitando {
        if consumir(ctx, egui::Key::Escape) {
            if let Some(id) = p.id_valor {
                ctx.memory_mut(|m| m.surrender_focus(id));
            }
            p.digitado.clear();
        }
        if consumir(ctx, egui::Key::F2) {
            if confirmar_valor(p, total).is_ok() {
                estado.pendentes.push(Acao::Finalizar);
            } else {
                notificar(ctx, Notificacao::aviso("Valor inválido (ex.: 20,00)."));
            }
        }
        return;
    }

    for (i, tecla) in TECLAS_FORMA.iter().enumerate() {
        if consumir(ctx, *tecla) {
            descartar_texto(ctx, &(i + 1).to_string());
            p.ativa = i;
            p.digitado.clear();
            p.foco_valor = true;
        }
    }
    if consumir(ctx, egui::Key::ArrowUp) {
        p.ativa = p.ativa.saturating_sub(1);
    }
    if consumir(ctx, egui::Key::ArrowDown) {
        p.ativa = (p.ativa + 1).min(FORMAS.len() - 1);
    }
    let finaliza = consumir(ctx, egui::Key::F2);
    let enter = consumir(ctx, egui::Key::Enter);
    if finaliza || enter {
        let fecha = pagamento::situacao(total, &p.valores).pode_finalizar();
        if finaliza || fecha {
            estado.pendentes.push(Acao::Finalizar);
        } else {
            // `Enter` sem ter fechado: o próximo passo é digitar o valor da forma ativa.
            p.digitado.clear();
            p.foco_valor = true;
        }
    }
}

pub(super) fn dialogo_pagamento(ctx: &egui::Context, estado: &mut EstadoTelaPdv) {
    teclas_pagamento(ctx, estado);
    let numero = estado.cupom_numero.unwrap_or(0);
    let fechar = Dialogo::nova("Pagamento")
        .descricao(format!("Cupom nº {numero}"))
        .medio()
        .mostrar(ctx, estado, corpo_pagamento, rodape_pagamento);
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}

pub(super) fn corpo_pagamento(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    let total = estado.total;
    let cliente = estado.cliente.as_ref().map(|(_, nome)| nome.clone());
    let Dlg::Pagamento(p) = &mut estado.dlg else {
        return;
    };
    let situacao = pagamento::situacao(total, &p.valores);
    resumo_pagamento(ui, total, situacao, pagamento::soma(&p.valores));
    ui.add_space(Espaco::E16);

    for (i, (_, nome)) in FORMAS.iter().enumerate() {
        let valor = p.valores[i];
        let resp = ItemDeLista::novo(*nome)
            .atalho((i + 1).to_string())
            .selecionado(i == p.ativa)
            .mostrar(ui, |ui| {
                if valor > Dinheiro::ZERO {
                    ui.add(Rotulo::novo(Papel::Numero, valor.formatar_com_simbolo()));
                }
            });
        if resp.clicked() {
            p.ativa = i;
            p.digitado.clear();
            p.foco_valor = true;
        }
        ui.add_space(Espaco::E4);
    }
    ui.add_space(Espaco::E12);

    let sugestao = pagamento::restante_para(total, &p.valores, p.ativa);
    let rotulo = format!("Valor em {}", FORMAS[p.ativa].1.to_lowercase());
    let resp = ui.add(
        Campo::novo(rotulo, &mut p.digitado)
            .marcador(format!("Enter = {}", sugestao.formatar_com_simbolo())),
    );
    p.id_valor = Some(resp.id);
    if std::mem::take(&mut p.foco_valor) {
        resp.request_focus();
    }
    if resp.lost_focus() && enter_pressionado(ui) && confirmar_valor(p, total).is_err() {
        notificar(ui.ctx(), Notificacao::aviso("Valor inválido (ex.: 20,00)."));
        p.foco_valor = true;
    }
    let em_prazo = condicao_prazo(ui, p, cliente.as_deref());
    p.digitando = resp.has_focus() || p.foco_valor || em_prazo;

    ui.add_space(Espaco::E12);
    ui.horizontal_wrapped(|ui| {
        for (tecla, texto) in [
            ("1–5", "escolhe a forma"),
            ("Enter", "confirma o valor"),
            ("F2", "finaliza"),
            ("Esc", "volta"),
        ] {
            ui.add(Tecla::nova(tecla));
            ui.add(Rotulo::campo(texto));
            ui.add_space(Espaco::E8);
        }
    });
}

/// A condição da venda a prazo, quando a forma está em uso ou escolhida. Devolve se um dos
/// campos tem o foco (aí os dígitos são texto, não escolha de forma).
fn condicao_prazo(ui: &mut egui::Ui, p: &mut EstadoPagamento, cliente: Option<&str>) -> bool {
    if p.valores[pagamento::PRAZO] == Dinheiro::ZERO && p.ativa != pagamento::PRAZO {
        return false;
    }
    let mut foco = false;
    ui.add_space(Espaco::E8);
    Painel::novo()
        .realce(Tom::Atencao)
        .compacto()
        .mostrar(ui, |ui| {
            match cliente {
                Some(nome) => ui.add(Rotulo::campo(format!("A prazo para {nome}"))),
                None => ui.add(
                    Rotulo::campo("Para vender a prazo, identifique o cliente (F6).")
                        .cor(ui.cores().negativo),
                ),
            };
            ui.add_space(Espaco::E4);
            let (r1, r2) = ui.columns(2, |c| {
                (
                    c[0].add(
                        Campo::novo("1º vencimento", &mut p.prazo_vencimento)
                            .mascara(Mascara::Data),
                    ),
                    c[1].add(Campo::novo(
                        "Parcelas (a cada 30 dias)",
                        &mut p.prazo_parcelas,
                    )),
                )
            });
            foco = r1.has_focus() || r2.has_focus();
        });
    foco
}

/// A condição digitada, validada — ou o aviso do que falta.
pub(super) fn condicao_digitada(
    p: &EstadoPagamento,
) -> Result<mod_pdv::CondicaoPrazo, &'static str> {
    let primeiro_vencimento = p
        .prazo_vencimento
        .parse::<cardeal_kernel::Data>()
        .map_err(|_| "Informe o 1º vencimento (dd/mm/aaaa).")?;
    let parcelas = p
        .prazo_parcelas
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|n| (1..=24).contains(n))
        .ok_or("Parcelas: de 1 a 24.")?;
    Ok(mod_pdv::CondicaoPrazo {
        primeiro_vencimento,
        parcelas,
        intervalo_dias: 30,
    })
}

/// Total · recebido · o que falta (ou o troco), com o terceiro em destaque.
pub(super) fn resumo_pagamento(
    ui: &mut egui::Ui,
    total: Dinheiro,
    situacao: Situacao,
    recebido: Dinheiro,
) {
    let (rotulo, valor, tom) = match situacao {
        Situacao::Falta(d) => ("FALTA", d, Tom::Atencao),
        Situacao::Fecha => ("FECHOU", Dinheiro::ZERO, Tom::Positivo),
        Situacao::Troco(d) => ("TROCO", d, Tom::Positivo),
        Situacao::Excesso(d) => ("EXCEDE", d, Tom::Negativo),
    };
    ui.columns(3, |c| {
        c[0].add(Rotulo::sobrelinha("Total"));
        c[0].add(Rotulo::novo(
            Papel::TituloTela,
            total.formatar_com_simbolo(),
        ));
        c[1].add(Rotulo::sobrelinha("Recebido"));
        c[1].add(Rotulo::novo(
            Papel::TituloTela,
            recebido.formatar_com_simbolo(),
        ));
        Painel::novo()
            .realce(tom)
            .compacto()
            .mostrar(&mut c[2], |ui| {
                ui.add(Rotulo::campo(rotulo));
                ui.add(Rotulo::novo(
                    Papel::TituloTela,
                    valor.formatar_com_simbolo(),
                ));
            });
    });
}

pub(super) fn rodape_pagamento(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    let pendente = estado.finalizar_pendente;
    let pode = match &estado.dlg {
        Dlg::Pagamento(p) => pagamento::situacao(estado.total, &p.valores).pode_finalizar(),
        _ => false,
    };
    // Este layout é da direita para a esquerda: o primeiro botão fica na ponta direita.
    if ui
        .add(
            Botao::primario("Finalizar venda")
                .atalho("F2")
                .habilitado(pode)
                .carregando(pendente),
        )
        .clicked()
        && !pendente
    {
        estado.pendentes.push(Acao::Finalizar);
    }
    if ui
        .add(
            Botao::secundario("Voltar")
                .atalho("Esc")
                .habilitado(!pendente),
        )
        .clicked()
    {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}
