//! Dar baixa numa parcela (meio e conta pelo `EstadoPagamento`), ver o histórico e estornar.

use super::*;

pub(super) fn dialogo_baixar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    if !matches!(estado.dlg, Dlg::Baixar { .. }) {
        return;
    }
    let Some(p) = parcela_do_dialogo(estado) else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let nome = estado.nome_contraparte(&p.contraparte);
    // Parcela em aberto: o diálogo dá baixa. Quitada: é uma consulta — histórico das baixas e
    // o estorno de uma lançada errada; sem formulário nem "Confirmar baixa".
    let aceita_baixa = p.estado.aceita_baixa();

    // Carrega o histórico de baixas uma vez por abertura do diálogo — não a cada frame
    // (mesmo padrão de `tela_estoque.rs::dialogo_ver` para os movimentos de rastreabilidade).
    if let Dlg::Baixar {
        baixas_carregadas: false,
        ..
    } = &estado.dlg
    {
        let historico: Vec<ItemBaixa> = match motor.consultar(
            sessao,
            "financeiro.baixas_da_parcela.v1",
            &BaixasDaParcela { parcela: p.parcela },
        ) {
            Ok(h) => h,
            Err(e) => {
                notificar(
                    ctx,
                    Notificacao::erro("Não foi possível carregar o histórico de baixas")
                        .detalhe(e.mensagem),
                );
                Vec::new()
            }
        };
        if let Dlg::Baixar {
            baixas,
            baixas_carregadas,
            ..
        } = &mut estado.dlg
        {
            *baixas = historico;
            *baixas_carregadas = true;
        }
    }

    if aceita_baixa {
        atualizar_situacao(motor, sessao, estado, p.parcela);
    }

    let fechar = Dialogo::nova(format!("Parcela {} · {}", p.numero, nome))
        .largura(560.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(2, |c| {
                    dado(&mut c[0], "Vencimento", &p.vencimento.to_string());
                    dado(
                        &mut c[1],
                        "Valor original",
                        &p.valor_original.formatar_com_simbolo(),
                    );
                });
                ui.columns(2, |c| {
                    dado(
                        &mut c[0],
                        "Já baixado",
                        &p.valor_baixado.formatar_com_simbolo(),
                    );
                    dado(&mut c[1], "Saldo", &p.saldo().formatar_com_simbolo());
                });
                ui.add_space(Espaco::E16);
                ui.add(Divisor::novo());
                ui.add_space(Espaco::E12);
                if aceita_baixa {
                    ui.add(Rotulo::titulo_secao("Dar baixa"));
                    ui.add_space(Espaco::E8);
                    let a_receber = estado.aba.a_receber();
                    let Dlg::Baixar {
                        valor,
                        data,
                        pagamento,
                        situacao,
                        ..
                    } = &mut estado.dlg
                    else {
                        return;
                    };
                    let rotulo_valor = if a_receber {
                        "Valor recebido"
                    } else {
                        "Valor pago"
                    };
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo(rotulo_valor, valor));
                        c[1].add(Campo::novo("Data", data).mascara(Mascara::Data));
                    });
                    if let Some((_, Some(s))) = situacao {
                        if let Some(texto) = composicao_do_devido(s) {
                            ui.add_space(Espaco::E4);
                            ui.add(Rotulo::campo(texto).cor(ui.cores().atencao));
                        }
                    }
                    ui.add_space(Espaco::E12);
                    pagamento.mostrar(ui, motor, sessao, "baixa-parcela", false);
                } else {
                    ui.add(Etiqueta::positiva("Quitada"));
                    ui.add_space(Espaco::E4);
                    ui.add(
                        Rotulo::interface(
                            "Esta parcela já foi quitada. Se uma baixa foi lançada por engano, \
                             estorne-a no histórico abaixo.",
                        )
                        .quebravel()
                        .cor(ui.cores().texto_medio),
                    );
                }

                let Dlg::Baixar { baixas, .. } = &estado.dlg else {
                    return;
                };
                if !baixas.is_empty() {
                    ui.add_space(Espaco::E16);
                    let titulo = format!("Baixas anteriores ({})", baixas.len());
                    let baixas = baixas.clone();
                    let mut estornar_clicada = None;
                    SecaoExpansivel::nova(titulo)
                        .aberta_por_padrao(true)
                        .mostrar(ui, |ui| {
                            for b in &baixas {
                                if let Some(id) = baixa_historico(ui, b) {
                                    estornar_clicada = Some(id);
                                }
                            }
                            ui.add_space(Espaco::E8);
                            let Dlg::Baixar { motivo_estorno, .. } = &mut estado.dlg else {
                                return;
                            };
                            ui.add(
                                Campo::novo("Motivo do estorno", motivo_estorno)
                                    .marcador("obrigatório para estornar uma baixa acima"),
                            );
                        });
                    if let Some(baixa_id) = estornar_clicada {
                        estornar(ui.ctx(), motor, sessao, estado, baixa_id);
                    }
                }
            },
            |ui, estado| {
                if aceita_baixa && ui.add(Botao::primario("Confirmar baixa")).clicked() {
                    baixar(ui.ctx(), motor, sessao, estado, &p);
                }
                if ui.add(Botao::secundario("Fechar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

/// Uma linha do histórico de baixas: data, valor, e "Estornar" quando ainda não estornada.
/// Devolve o id da baixa cujo botão "Estornar" foi clicado neste frame, se algum.
/// A parcela que o diálogo de baixa está mostrando, achada pelo `Id` na lista atual.
pub(super) fn parcela_do_dialogo(estado: &EstadoTelaFinanceiro) -> Option<ItemTituloEmAberto> {
    let Dlg::Baixar { parcela, .. } = estado.dlg else {
        return None;
    };
    estado
        .parcelas
        .iter()
        .find(|p| p.parcela == parcela)
        .cloned()
}

pub(super) fn baixa_historico(ui: &mut egui::Ui, b: &ItemBaixa) -> Option<Id> {
    let mut clicada = None;
    Painel::novo()
        .realce(Tom::Neutro)
        .compacto()
        .mostrar(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.add(Rotulo::interface(format!(
                        "{} · {}",
                        b.data.formatar_curta(),
                        b.valor_recebido.formatar_com_simbolo()
                    )));
                    if (b.juros + b.multa + b.desconto).e_positivo() {
                        ui.add(Rotulo::campo(format!(
                            "principal {} · juros {} · multa {} · desconto {}",
                            b.principal.formatar_com_simbolo(),
                            b.juros.formatar_com_simbolo(),
                            b.multa.formatar_com_simbolo(),
                            b.desconto.formatar_com_simbolo()
                        )));
                    }
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if b.estornada {
                        ui.add(Etiqueta::neutra("Estornada"));
                    } else if ui.add(Botao::fantasma("Estornar").pequeno()).clicked() {
                        clicada = Some(b.baixa);
                    }
                });
            });
        });
    clicada
}

pub(super) fn baixar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    p: &ItemTituloEmAberto,
) {
    let Dlg::Baixar {
        valor,
        data,
        pagamento,
        ..
    } = &estado.dlg
    else {
        return;
    };
    let (Ok(valor), Ok(data)) = (valor.parse::<Dinheiro>(), data.parse::<Data>()) else {
        notificar(
            ctx,
            Notificacao::aviso("Valor (0,00) ou data (dd/mm/aaaa) inválidos."),
        );
        return;
    };
    let meio_pagamento = match pagamento.meio_validado() {
        Ok(m) => m,
        Err(msg) => {
            notificar(ctx, Notificacao::aviso(msg));
            return;
        }
    };
    let conta_destino = pagamento.conta_destino();
    let r = if estado.aba.a_receber() {
        motor
            .executar(
                sessao,
                "financeiro.baixar_recebimento.v1",
                &BaixarRecebimento {
                    parcela: p.parcela,
                    valor,
                    data,
                    meio_pagamento,
                    conta_destino,
                },
            )
            .map(|_: mod_financeiro::RecebimentoBaixado| ())
    } else {
        motor
            .executar(
                sessao,
                "financeiro.baixar_pagamento.v1",
                &BaixarPagamento {
                    parcela: p.parcela,
                    valor,
                    data,
                    meio_pagamento,
                    conta_destino,
                },
            )
            .map(|_: mod_financeiro::PagamentoBaixado| ())
    };
    match r {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Baixa registrada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

/// Estorna uma baixa do histórico exibido no diálogo — reverte o lançamento no Razão e
/// reabre a parcela (`EstornarBaixa`, `docs/modulos/financeiro.md` §5). Fecha o diálogo e
/// recarrega a lista no sucesso, como `baixar` — o índice guardado em `Dlg::Baixar` não
/// sobrevive a um recarregamento (a ordem pode mudar), então manter o diálogo aberto
/// arriscaria apontar para a parcela errada.
pub(super) fn estornar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    baixa: Id,
) {
    let Dlg::Baixar { motivo_estorno, .. } = &estado.dlg else {
        return;
    };
    let motivo = motivo_estorno.trim();
    if motivo.is_empty() {
        notificar(
            ctx,
            Notificacao::aviso("Informe o motivo do estorno antes de confirmar."),
        );
        return;
    }
    let motivo = motivo.to_owned();
    match motor
        .executar(
            sessao,
            "financeiro.estornar_baixa.v1",
            &EstornarBaixa { baixa, motivo },
        )
        .map(|_: mod_financeiro::BaixaFoiEstornada| ())
    {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Baixa estornada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

/// Consulta quanto a parcela deve na data digitada (juros/multa de atraso, desconto de
/// antecipação) e, enquanto o valor não foi editado à mão, sugere esse total — antes a tela
/// sugeria só o principal, e uma parcela vencida "paga inteira" virava parcial.
pub(super) fn atualizar_situacao(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    parcela: Id,
) {
    let Dlg::Baixar {
        valor,
        data,
        situacao,
        valor_sugerido,
        ..
    } = &mut estado.dlg
    else {
        return;
    };
    if situacao.as_ref().is_some_and(|(d, _)| d == data) {
        return;
    }
    let Ok(na_data) = data.parse::<Data>() else {
        return;
    };
    let consultada: Option<mod_financeiro::SituacaoNaData> = motor
        .consultar(
            sessao,
            "financeiro.situacao_da_parcela.v1",
            &mod_financeiro::SituacaoDaParcela {
                parcela,
                data: na_data,
            },
        )
        .ok();
    if let Some(s) = &consultada {
        // Acompanha o total devido enquanto o campo ainda mostra a última sugestão.
        if valor_sugerido.is_empty() || *valor == *valor_sugerido {
            *valor = s.total_devido.formatar();
            *valor_sugerido = valor.clone();
        }
    }
    *situacao = Some((data.clone(), consultada));
}

/// "R$ 100,00 + multa R$ 2,00 = R$ 102,00 (10 dias de atraso)" — só quando há encargo ou
/// desconto; parcela em dia sem desconto não precisa de explicação.
pub(super) fn composicao_do_devido(s: &mod_financeiro::SituacaoNaData) -> Option<String> {
    if (s.juros + s.multa + s.desconto).e_zero() {
        return None;
    }
    let mut partes = vec![s.principal.formatar_com_simbolo()];
    if s.juros.e_positivo() {
        partes.push(format!("juros {}", s.juros.formatar_com_simbolo()));
    }
    if s.multa.e_positivo() {
        partes.push(format!("multa {}", s.multa.formatar_com_simbolo()));
    }
    let mut texto = partes.join(" + ");
    if s.desconto.e_positivo() {
        texto.push_str(&format!(
            " − desconto {}",
            s.desconto.formatar_com_simbolo()
        ));
    }
    texto.push_str(&format!(" = {}", s.total_devido.formatar_com_simbolo()));
    if s.dias_atraso > 0 {
        texto.push_str(&format!(" ({} dias de atraso)", s.dias_atraso));
    }
    Some(texto)
}

/// Quitar as parcelas marcadas na grade, cada uma pelo total devido na data, no mesmo meio
/// e conta — um comando só (`…_em_lote.v1`): ou todas, ou nenhuma.
pub(super) fn dialogo_baixar_lote(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let selecionadas: Vec<ItemTituloEmAberto> = estado
        .selecao
        .as_ref()
        .map(|ids| {
            estado
                .parcelas
                .iter()
                .filter(|p| ids.contains(&p.parcela))
                .cloned()
                .collect()
        })
        .unwrap_or_default();
    let a_receber = estado.aba.a_receber();
    let principal = selecionadas
        .iter()
        .fold(Dinheiro::ZERO, |acc, p| acc + p.saldo());
    let titulo = format!(
        "{} {} parcela(s)",
        if a_receber { "Receber" } else { "Pagar" },
        selecionadas.len()
    );
    let fechar = Dialogo::nova(titulo).largura(520.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Saldo das parcelas"));
                ui.add(ValorDinheiro::novo(principal));
            });
            ui.add(
                Rotulo::campo("Juros, multa e desconto de cada uma são calculados na data.")
                    .cor(ui.cores().texto_medio),
            );
            ui.add_space(Espaco::E12);
            let Dlg::BaixarLote { data, pagamento } = &mut estado.dlg else {
                return;
            };
            ui.add(Campo::novo("Data", data).mascara(Mascara::Data));
            ui.add_space(Espaco::E12);
            pagamento.mostrar(ui, motor, sessao, "baixa-lote", false);
        },
        |ui, estado| {
            if ui.add(Botao::primario("Confirmar")).clicked() {
                baixar_lote(ui.ctx(), motor, sessao, estado, &selecionadas);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn baixar_lote(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    selecionadas: &[ItemTituloEmAberto],
) {
    let Dlg::BaixarLote { data, pagamento } = &estado.dlg else {
        return;
    };
    let Ok(data) = data.parse::<Data>() else {
        notificar(ctx, Notificacao::aviso("Data inválida (dd/mm/aaaa)."));
        return;
    };
    let meio_pagamento = match pagamento.meio_validado() {
        Ok(m) => m,
        Err(msg) => {
            notificar(ctx, Notificacao::aviso(msg));
            return;
        }
    };
    let dados = mod_financeiro::DadosBaixaEmLote {
        parcelas: selecionadas.iter().map(|p| p.parcela).collect(),
        data,
        meio_pagamento,
        conta_destino: pagamento.conta_destino(),
    };
    let r: cardeal_kernel::Resultado<mod_financeiro::BaixasEmLoteFeitas> = if estado.aba.a_receber()
    {
        motor.executar(
            sessao,
            "financeiro.baixar_recebimentos_em_lote.v1",
            &mod_financeiro::BaixarRecebimentosEmLote(dados),
        )
    } else {
        motor.executar(
            sessao,
            "financeiro.baixar_pagamentos_em_lote.v1",
            &mod_financeiro::BaixarPagamentosEmLote(dados),
        )
    };
    match r {
        Ok(feitas) => {
            estado.dlg = Dlg::Fechado;
            estado.selecao = None;
            estado.carregar(motor, sessao);
            notificar(
                ctx,
                Notificacao::sucesso(format!(
                    "{} parcela(s) baixadas — {}",
                    feitas.quantidade,
                    feitas.total.formatar_com_simbolo()
                )),
            );
        }
        Err(e) => notificar(
            ctx,
            Notificacao::erro("Nenhuma parcela foi baixada").detalhe(e.mensagem),
        ),
    }
}
