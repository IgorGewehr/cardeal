//! Lançar um título a receber/pagar e cadastrar categoria.

use super::*;

pub(super) fn dialogo_categoria(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let fechar = Dialogo::nova("Nova categoria").largura(460.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovaCategoria { nome, especie } = &mut estado.dlg else {
                return;
            };
            ui.add(Rotulo::campo(
                "Rótulo livre para relatório — \"Aluguel\", \"Peças\", \"Assinatura SaaS\". \
                 Não afeta a contabilização, só agrupa os gráficos de \"Por categoria\".",
            ));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Nome", nome).marcador("Aluguel"));
            ui.add_space(Espaco::E12);
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Serve para"));
                for (rot, valor) in [
                    ("Ambas", None),
                    ("Só a receber", Some(EspecieTitulo::Receber)),
                    ("Só a pagar", Some(EspecieTitulo::Pagar)),
                ] {
                    let sel = *especie == valor;
                    let b = if sel {
                        Botao::primario(rot)
                    } else {
                        Botao::fantasma(rot)
                    };
                    if ui.add(b).clicked() {
                        *especie = valor;
                    }
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
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let a_receber = estado.aba.a_receber();
    let titulo = if a_receber {
        "Lançar título a receber"
    } else {
        "Lançar título a pagar"
    };
    let ops: Vec<(Id, String)> = if a_receber {
        estado
            .clientes
            .iter()
            .map(|p| (p.pessoa, p.nome.clone()))
            .collect()
    } else {
        estado
            .fornecedores
            .iter()
            .map(|p| (p.pessoa, p.nome.clone()))
            .collect()
    };
    let cats: Vec<(Id, String)> = estado
        .categorias
        .iter()
        .map(|c| (c.id, c.nome.clone()))
        .collect();

    let fechar = Dialogo::nova(titulo).largura(680.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Lancar(f) = &mut estado.dlg else {
                return;
            };
            SeletorOpcao::novo(
                if a_receber {
                    "Cliente (opcional)"
                } else {
                    "Fornecedor (opcional)"
                },
                &mut f.contraparte,
            )
            .opcoes(ops.clone())
            .placeholder(if a_receber {
                "Sem cliente informado"
            } else {
                "Sem fornecedor informado"
            })
            .mostrar(ui);
            if ui
                .add(botao_cadastro_rapido(if a_receber {
                    "+ Cadastrar cliente"
                } else {
                    "+ Cadastrar fornecedor"
                }))
                .clicked()
            {
                estado.dlg_rapido = Some(DlgRapido::Pessoa {
                    alvo: AlvoRapido::Lancar,
                    papel: if a_receber {
                        Papel::Cliente
                    } else {
                        Papel::Fornecedor
                    },
                    tipo: TipoPessoa::Fisica,
                    nome: String::new(),
                });
            }
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Valor total", &mut f.valor).marcador("0,00"));
                c[1].add(Campo::novo("Emissão", &mut f.emissao).mascara(Mascara::Data));
            });
            ui.add_space(Espaco::E12);
            ui.columns(3, |c| {
                c[0].add(Campo::novo("Parcelas", &mut f.parcelas));
                c[1].add(
                    Campo::novo("1º vencimento", &mut f.primeiro_vencimento).mascara(Mascara::Data),
                );
                c[2].add(Campo::novo("Intervalo (dias)", &mut f.intervalo));
            });
            ui.add_space(Espaco::E12);
            cardeal_ui::molecules::SeletorOpcao::novo("Categoria (opcional)", &mut f.categoria)
                .opcoes(cats.clone())
                .placeholder("Sem categoria")
                .mostrar(ui);
            if ui.add(botao_cadastro_rapido("+ Nova categoria")).clicked() {
                estado.dlg_rapido = Some(DlgRapido::Categoria {
                    alvo: AlvoRapido::Lancar,
                    nome: String::new(),
                    especie: None,
                });
            }
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Observação", &mut f.observacao));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Lançar")).clicked() {
                lancar(ui.ctx(), motor, sessao, estado);
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

pub(super) fn lancar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let a_receber = estado.aba.a_receber();
    let Dlg::Lancar(f) = &estado.dlg else { return };
    // Cliente/fornecedor é opcional (pedido explícito do usuário) — um título avulso não
    // precisa de uma pessoa cadastrada.
    let contraparte = f.contraparte;
    let (Ok(valor), Ok(emissao), Ok(parcelas), Ok(prim), Ok(intervalo)) = (
        f.valor.parse::<Dinheiro>(),
        f.emissao.parse::<Data>(),
        f.parcelas.parse::<u16>(),
        f.primeiro_vencimento.parse::<Data>(),
        f.intervalo.parse::<i32>(),
    ) else {
        notificar(
            ctx,
            Notificacao::aviso("Confira os campos — valor 0,00, datas dd/mm/aaaa."),
        );
        return;
    };
    let obs = (!f.observacao.trim().is_empty()).then(|| f.observacao.clone());
    let cat = f.categoria;

    let r = if a_receber {
        motor
            .executar(
                sessao,
                "financeiro.lancar_titulo_a_receber.v1",
                &LancarTituloAReceber {
                    cliente: contraparte,
                    valor_total: valor,
                    emissao,
                    parcelas,
                    primeiro_vencimento: prim,
                    intervalo_dias: intervalo,
                    observacao: obs,
                    categoria: cat,
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
                    emissao,
                    parcelas,
                    primeiro_vencimento: prim,
                    intervalo_dias: intervalo,
                    observacao: obs,
                    categoria: cat,
                },
            )
            .map(|_: mod_financeiro::TituloAPagarLancado| ())
    };
    match r {
        Ok(()) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Título lançado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
