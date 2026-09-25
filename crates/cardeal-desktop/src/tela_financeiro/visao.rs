//! A aba "Visão geral": indicadores, gráfico recebido × pago e o recorte por categoria.

use super::*;

pub(super) fn dialogo_analise(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    // Agrega o que veio (categoria × mês) em uma linha por categoria: receita, custo, saldo.
    let mut por_cat: std::collections::BTreeMap<String, (Dinheiro, Dinheiro)> =
        std::collections::BTreeMap::new();
    for it in &estado.analise {
        let nome = estado.nome_categoria(it.categoria);
        let e = por_cat
            .entry(nome)
            .or_insert((Dinheiro::ZERO, Dinheiro::ZERO));
        match it.especie {
            EspecieTitulo::Receber => e.0 += it.total_baixado,
            EspecieTitulo::Pagar => e.1 += it.total_baixado,
        }
    }
    let linhas: Vec<(String, Dinheiro, Dinheiro)> =
        por_cat.into_iter().map(|(k, (r, c))| (k, r, c)).collect();
    let meses = estado.analise_meses;

    let fechar = Dialogo::nova("Onde o dinheiro entrou e saiu")
        .largura(720.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.horizontal(|ui| {
                    for m in [3_u8, 6, 12] {
                        let b = if meses == m {
                            Botao::primario(format!("{m} meses"))
                        } else {
                            Botao::fantasma(format!("{m} meses"))
                        };
                        if ui.add(b).clicked() {
                            estado.analise_meses = m;
                            estado.carregar_analise(motor, sessao);
                        }
                    }
                });
                ui.add_space(Espaco::E12);

                if linhas.is_empty() {
                    ui.add(Rotulo::campo(
                        "Nada baixado com categoria no período — categorize os títulos ao lançar.",
                    ));
                    return;
                }

                let colunas = vec![
                    ColunaGrade::nova("Categoria"),
                    ColunaGrade::nova("Recebido").largura(140.0).numero(),
                    ColunaGrade::nova("Pago").largura(140.0).numero(),
                    ColunaGrade::nova("Saldo").largura(140.0).numero(),
                ];
                Grade::nova(colunas).mostrar(ui, linhas.len(), |i, row| {
                    let (nome, rec, pago) = &linhas[i];
                    row.col(|ui| {
                        ui.add(Rotulo::interface(nome.clone()));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(*rec));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(*pago));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(*rec - *pago).com_sinal());
                    });
                });
            },
            |ui, estado| {
                if ui.add(Botao::secundario("Fechar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

/// Um cartão de indicador da Visão Geral: o `CartaoKpi` do design system no tamanho compacto
/// (a contagem do valor e a faixa de acento vêm dele).
pub(super) fn kpi(ui: &mut egui::Ui, rotulo: &str, valor: Dinheiro, tom: Tom, apoio: Option<&str>) {
    let mut cartao = CartaoKpi::novo(rotulo, valor).compacto().tom(tom);
    if let Some(a) = apoio {
        cartao = cartao.variacao(a);
    }
    ui.add(cartao);
}

/// Largura abaixo da qual o gráfico principal e o recorte "por categoria" empilham em vez
/// de ficar lado a lado — mesma ideia de `LayoutTela::mostrar_com_detalhe`, só que local a
/// este painel (não é uma tela mestre-detalhe).
pub(super) const LIMIAR_COLUNAS_VISAO: f32 = 760.0;

pub(super) fn painel_visao(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let cores = ui.cores();

    // Faixa de KPIs — três cartões esticados pra ocupar a largura toda, em vez de flutuar
    // colados à esquerda com um vão morto do lado (pedido explícito do usuário: a Visão
    // Geral estava "horrível", essa era a maior causa numa janela larga).
    ui.columns(3, |col| {
        kpi(
            &mut col[0],
            "A receber em aberto",
            estado.dash_receber,
            Tom::Positivo,
            None,
        );
        kpi(
            &mut col[1],
            "A pagar em aberto",
            estado.dash_pagar,
            Tom::Negativo,
            None,
        );
        let saldo = estado.dash_recebido_mes - estado.dash_pago_mes;
        kpi(
            &mut col[2],
            "Saldo do mês",
            saldo,
            if saldo.e_negativo() {
                Tom::Negativo
            } else {
                Tom::Positivo
            },
            Some(&format!(
                "recebido {} · pago {}",
                estado.dash_recebido_mes.formatar_com_simbolo(),
                estado.dash_pago_mes.formatar_com_simbolo()
            )),
        );
    });
    ui.add_space(Espaco::E12);

    // Faixa "vence em 7 dias".
    let (nr, vr) = estado.dash_venc_receber;
    let (np, vp) = estado.dash_venc_pagar;
    Painel::novo()
        .realce(if np > 0 { Tom::Atencao } else { Tom::Neutro })
        .compacto()
        .mostrar(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add(Rotulo::campo("PRÓXIMOS 7 DIAS"));
                ui.add_space(Espaco::E16);
                ui.add(
                    Rotulo::interface(format!("{np} a pagar · {}", vp.formatar_com_simbolo()))
                        .cor(cores.negativo),
                );
                ui.add_space(Espaco::E16);
                ui.add(
                    Rotulo::interface(format!("{nr} a receber · {}", vr.formatar_com_simbolo()))
                        .cor(cores.positivo),
                );
            });
        });
    ui.add_space(Espaco::E24);

    let tem_dados = estado
        .serie
        .iter()
        .any(|m| m.receita.e_positivo() || m.custo.e_positivo());
    let tem_categorias = !estado.analise.is_empty();

    let cartao_grafico = |ui: &mut egui::Ui, titulo: &str, corpo: &dyn Fn(&mut egui::Ui)| {
        ui.add(Rotulo::titulo_secao(titulo));
        ui.add_space(Espaco::E8);
        Painel::novo().plano().mostrar(ui, corpo);
    };

    let largura = ui.available_width();
    let lado_a_lado = largura >= LIMIAR_COLUNAS_VISAO && tem_dados && tem_categorias;
    if lado_a_lado {
        ui.columns(2, |col| {
            cartao_grafico(&mut col[0], "Recebido × pago — últimos 6 meses", &|ui| {
                grafico_fluxo(ui, estado, &cores)
            });
            cartao_grafico(&mut col[1], "Por categoria — últimos 6 meses", &|ui| {
                GraficoBarrasHorizontais::novo(&estado.top_categorias()).mostrar(ui);
            });
        });
    } else {
        cartao_grafico(ui, "Recebido × pago — últimos 6 meses", &|ui| {
            grafico_fluxo(ui, estado, &cores)
        });
        if tem_categorias {
            ui.add_space(Espaco::E16);
            cartao_grafico(ui, "Por categoria — últimos 6 meses", &|ui| {
                GraficoBarrasHorizontais::novo(&estado.top_categorias()).mostrar(ui);
            });
        }
    }
    ui.add_space(Espaco::E16);
    if ui.add(Botao::secundario("Ver por categoria")).clicked() {
        estado.carregar_analise(motor, sessao);
        estado.dlg = Dlg::Analise;
    }
}

/// O corpo do cartão "Recebido × pago" — extraído de [`painel_visao`] porque aparece tanto
/// no layout lado a lado quanto no empilhado.
pub(super) fn grafico_fluxo(
    ui: &mut egui::Ui,
    estado: &EstadoTelaFinanceiro,
    cores: &cardeal_ui::tokens::Cores,
) {
    let tem_dados = estado
        .serie
        .iter()
        .any(|m| m.receita.e_positivo() || m.custo.e_positivo());
    if !tem_dados {
        ui.add(Rotulo::campo(
            "Sem baixas nos últimos 6 meses — o gráfico aparece conforme você recebe e paga títulos.",
        ));
        return;
    }
    let eixo: Vec<String> = estado.serie.iter().map(|m| m.rotulo.clone()).collect();
    let series = [
        SerieBarras {
            rotulo: "Recebido".to_owned(),
            cor: cores.positivo,
            valores: estado
                .serie
                .iter()
                .map(|m| reais_para_grafico(m.receita))
                .collect(),
        },
        SerieBarras {
            rotulo: "Pago".to_owned(),
            cor: cores.negativo,
            valores: estado
                .serie
                .iter()
                .map(|m| reais_para_grafico(m.custo))
                .collect(),
        },
    ];
    GraficoBarras::novo(&eixo, &series)
        .altura(260.0)
        .mostrar(ui);
}
