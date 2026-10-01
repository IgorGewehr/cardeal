//! Lançar um título a receber/pagar e cadastrar categoria.

use super::*;

pub(super) fn dialogo_categoria(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let fechar = Dialogo::nova("Nova categoria").largura(460.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovaCategoria { nome, especie } = &mut estado.dlg else {
                return;
            };
            ui.add(Rotulo::apoio(
                "Rótulo livre para relatório — \"Aluguel\", \"Peças\", \"Assinatura SaaS\". \
                 Não afeta a contabilização, só agrupa os gráficos de \"Por categoria\".",
            ));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Nome", nome).marcador("Aluguel"));
            ui.add_space(Espaco::E12);
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Serve para"));
                ui.add_space(Espaco::E8);
                if let Some(e) = Abas::nova(&[
                    (None, "Ambas"),
                    (Some(EspecieTitulo::Receber), "Só a receber"),
                    (Some(EspecieTitulo::Pagar), "Só a pagar"),
                ])
                .selecionada(*especie)
                .id_salt("categoria-especie")
                .mostrar(ui)
                {
                    *especie = e;
                }
            });
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar")).clicked() {
                if let Dlg::NovaCategoria { nome, especie } = &estado.dlg {
                    let nome = nome.trim().to_owned();
                    if nome.is_empty() {
                        notificar(ui.ctx(), Notificacao::aviso("Informe o nome da categoria."));
                        return;
                    }
                    let especie = *especie;
                    match motor
                        .executar(
                            sessao,
                            "financeiro.criar_categoria.v1",
                            &CriarCategoria { nome, especie },
                        )
                        .map(|_: mod_financeiro::CategoriaCriada| ())
                    {
                        Ok(()) => {
                            estado.dlg = Dlg::Fechado;
                            estado.carregar_categorias(motor, sessao);
                            notificar(ui.ctx(), Notificacao::sucesso("Categoria criada"));
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                    }
                }
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

pub(super) fn dialogo_lancar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let a_receber = matches!(&estado.dlg, Dlg::Lancar(f) if f.a_receber);
    let titulo = if a_receber {
        "Lançar a receber"
    } else {
        "Lançar a pagar"
    };
    let cats: Vec<(Id, String)> = estado
        .categorias
        .iter()
        .filter(|c| {
            c.especie
                .is_none_or(|e| (e == EspecieTitulo::Receber) == a_receber)
        })
        .map(|c| (c.id, c.nome.clone()))
        .collect();
    let dica = if a_receber {
        "À vista: com \"Já recebido\" marcado, entra no caixa/banco agora; desmarcado, fica a \
         receber. A prazo: parcelas e vencimentos."
    } else {
        "À vista: com \"Já pago\" marcado, sai do caixa/banco agora; desmarcado, fica a pagar. \
         A prazo: parcelas e vencimentos."
    };
    let fechar = Dialogo::nova(titulo)
        .descricao(dica)
        .largura(720.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Lancar(f) = &mut estado.dlg else {
                    return;
                };
                let mut nova_categoria = false;
                ui.columns(2, |c| {
                    c[0].add(
                        Campo::novo(
                            if a_receber {
                                "Do que é"
                            } else {
                                "O que é esta despesa"
                            },
                            &mut f.descricao,
                        )
                        .marcador(if a_receber {
                            "ex.: conserto do notebook, sinal da OS"
                        } else {
                            "ex.: aluguel de outubro, pró-labore, conta de luz"
                        }),
                    );
                    SeletorOpcao::novo(
                        if a_receber {
                            "Categoria"
                        } else {
                            "Categoria — para onde vai o dinheiro"
                        },
                        &mut f.categoria,
                    )
                    .opcoes(cats.clone())
                    .placeholder("Escolha (aluguel, pró-labore, peças…)")
                    .mostrar(&mut c[1]);
                    nova_categoria = c[1]
                        .add(botao_cadastro_rapido("+ Nova categoria"))
                        .clicked();
                });
                if nova_categoria {
                    estado.dlg_rapido = Some(DlgRapido::Categoria {
                        alvo: AlvoRapido::Lancar,
                        nome: String::new(),
                        especie: Some(if a_receber {
                            EspecieTitulo::Receber
                        } else {
                            EspecieTitulo::Pagar
                        }),
                    });
                }
                let Dlg::Lancar(f) = &mut estado.dlg else {
                    return;
                };
                ui.columns(2, |c| {
                    c[0].add(Campo::novo("Valor", &mut f.valor).marcador("0,00"));
                    c[1].add(Campo::novo("Data", &mut f.data).mascara(Mascara::Data));
                });
                ui.add_space(Espaco::E16);
                let (catalogo, papel) = if a_receber {
                    (&estado.clientes, Papel::Cliente)
                } else {
                    (&estado.fornecedores, Papel::Fornecedor)
                };
                f.pessoa.mostrar(ui, catalogo, papel, true, false);
                ui.add_space(Espaco::E16);
                f.pagamento.mostrar_com(
                    ui,
                    motor,
                    sessao,
                    "lancar",
                    crate::pagamento::Prazo::Parcelado,
                    Some(if a_receber {
                        "Já recebido"
                    } else {
                        "Já pago"
                    }),
                );
            },
            |ui, estado| {
                let tom = if a_receber {
                    Tom::Positivo
                } else {
                    Tom::Negativo
                };
                if ui
                    .add(Botao::primario("Lançar e continuar").tom(tom))
                    .clicked()
                {
                    lancar(ui.ctx(), motor, sessao, estado, true);
                }
                if ui
                    .add(Botao::secundario("Lançar e fechar").tom(tom))
                    .clicked()
                {
                    lancar(ui.ctx(), motor, sessao, estado, false);
                }
                if ui.add(Botao::fantasma("Cancelar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

/// Lança o que está no formulário. Cliente/fornecedor novo é cadastrado antes (e fica
/// escolhido no formulário: se o lançamento falhar, a correção não cadastra de novo).
/// `continuar` = limpa o formulário para o próximo lançamento, mantendo data e meio.
pub(super) fn lancar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    continuar: bool,
) {
    let Dlg::Lancar(f) = &estado.dlg else { return };
    let a_receber = f.a_receber;
    let aviso = |msg: &str| notificar(ctx, Notificacao::aviso(msg.to_owned()));
    let valor = match f.valor.parse::<Dinheiro>() {
        Ok(v) if v.e_positivo() => v,
        _ => return aviso("Informe o valor (ex.: 150,00)."),
    };
    let Ok(data) = f.data.parse::<Data>() else {
        return aviso("Informe a data (dd/mm/aaaa).");
    };
    let (parcelas, vencimento, intervalo, quitado) = if f.pagamento.condicao.a_prazo {
        match f.pagamento.prazo_validado() {
            Ok((p, v, i)) => (p, v, i, None),
            Err(msg) => return aviso(msg),
        }
    } else if !f.pagamento.condicao.pago {
        // À vista ainda não pago: uma parcela em aberto vencendo na data do lançamento.
        (1, data, 0, None)
    } else {
        match f.pagamento.meio_validado() {
            Ok(meio) => (
                1,
                data,
                0,
                Some(mod_financeiro::QuitadoAgora {
                    meio_pagamento: meio,
                    conta: f.pagamento.conta_destino(),
                }),
            ),
            Err(msg) => return aviso(msg),
        }
    };
    let descricao = (!f.descricao.trim().is_empty()).then(|| f.descricao.trim().to_owned());
    let categoria = f.categoria;
    let papel = if a_receber {
        Papel::Cliente
    } else {
        Papel::Fornecedor
    };

    let contraparte = if f.pessoa.cadastrando() {
        let cmd = match f.pessoa.para_criar(papel, None) {
            Ok(c) => c,
            Err(msg) => return aviso(msg),
        };
        match motor.executar(sessao, "clientes.criar_pessoa.v1", &cmd) {
            Ok(p) => {
                let p: PessoaCadastrada = p;
                estado.carregar_nomes(motor, sessao);
                if let Dlg::Lancar(f) = &mut estado.dlg {
                    f.pessoa.novo = false;
                    f.pessoa.selecionada = Some(p.pessoa);
                }
                Some(p.pessoa)
            }
            Err(e) => return notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    } else {
        f.pessoa.escolhida()
    };

    let r = if a_receber {
        motor
            .executar(
                sessao,
                "financeiro.lancar_titulo_a_receber.v1",
                &LancarTituloAReceber {
                    cliente: contraparte,
                    valor_total: valor,
                    emissao: data,
                    parcelas,
                    primeiro_vencimento: vencimento,
                    intervalo_dias: intervalo,
                    observacao: descricao,
                    categoria,
                    quitado_agora: quitado,
                },
            )
            .map(|_: mod_financeiro::TituloAReceberLancado| ())
    } else {
        motor
            .executar(
                sessao,
                "financeiro.lancar_titulo_a_pagar.v1",
                &LancarTituloAPagar {
                    fornecedor: contraparte,
                    valor_total: valor,
                    emissao: data,
                    parcelas,
                    primeiro_vencimento: vencimento,
                    intervalo_dias: intervalo,
                    observacao: descricao,
                    categoria,
                    quitado_agora: quitado,
                },
            )
            .map(|_: mod_financeiro::TituloAPagarLancado| ())
    };
    match r {
        Ok(()) => {
            let msg = match (quitado.is_some(), a_receber) {
                (true, true) => format!("Recebimento de {} lançado", valor.formatar_com_simbolo()),
                (true, false) => format!("Pagamento de {} lançado", valor.formatar_com_simbolo()),
                (false, true) => format!("{} a receber lançado", valor.formatar_com_simbolo()),
                (false, false) => format!("{} a pagar lançado", valor.formatar_com_simbolo()),
            };
            if continuar {
                if let Dlg::Lancar(f) = &mut estado.dlg {
                    let anterior = std::mem::replace(f, FormLancar::novo(a_receber));
                    f.data = anterior.data;
                    f.pagamento = anterior.pagamento;
                }
            } else {
                estado.dlg = Dlg::Fechado;
            }
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso(msg));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
