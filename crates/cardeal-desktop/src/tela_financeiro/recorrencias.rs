//! Recorrências: a lista e o cadastro de uma nova.

use super::*;

pub(super) fn dialogo_recorrencias(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let linhas: Vec<(String, String, String, String, bool)> = estado
        .recorrencias
        .iter()
        .map(|r| {
            let especie = if matches!(r.especie, EspecieTitulo::Receber) {
                "A receber"
            } else {
                "A pagar"
            };
            let periodo = match r.periodicidade {
                Periodicidade::Mensal => "Mensal",
                Periodicidade::Semanal => "Semanal",
                Periodicidade::Anual => "Anual",
                Periodicidade::Personalizada => "Personalizada",
            };
            let valor = r
                .valor_fixo
                .map_or_else(|| "variável".to_owned(), |v| v.formatar_com_simbolo());
            (
                r.descricao.clone(),
                especie.to_owned(),
                periodo.to_owned(),
                valor,
                r.ativa,
            )
        })
        .collect();

    let fechar = Dialogo::nova("Recorrências").largura(760.0).mostrar(
        ctx,
        estado,
        |ui, _estado| {
            ui.add(Rotulo::campo(
                "Regras que geram títulos automaticamente — aluguel, internet, assinaturas. \
                     Nenhum título é criado agora; cada ocorrência vira título real na data.",
            ));
            ui.add_space(Espaco::E12);
            if linhas.is_empty() {
                ui.add(Rotulo::campo("Nenhuma recorrência cadastrada."));
            } else {
                let colunas = vec![
                    ColunaGrade::nova("Descrição"),
                    ColunaGrade::nova("Espécie").largura(100.0),
                    ColunaGrade::nova("Periodicidade").largura(120.0),
                    ColunaGrade::nova("Valor").largura(130.0).numero(),
                    ColunaGrade::nova("Ativa").largura(70.0),
                ];
                Grade::nova(colunas).mostrar(ui, linhas.len(), |i, row| {
                    let (desc, esp, per, val, ativa) = &linhas[i];
                    row.col(|ui| {
                        ui.add(Rotulo::interface(desc.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(esp.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(per.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::interface(val.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::campo(if *ativa { "sim" } else { "não" }));
                    });
                });
            }
        },
        |ui, estado| {
            if ui.add(Botao::primario("+ Nova recorrência")).clicked() {
                estado.carregar_contas(motor, sessao, false);
                estado.dlg = Dlg::NovaRecorrencia(FormRecorrencia::default());
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

pub(super) fn dialogo_nova_recorrencia(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let a_receber = match &estado.dlg {
        Dlg::NovaRecorrencia(f) => f.a_receber,
        _ => return,
    };
    if estado.contas_receber != a_receber {
        estado.carregar_contas(motor, sessao, a_receber);
    }
    let pessoas: Vec<(Id, String)> = if a_receber {
        estado.clientes.iter()
    } else {
        estado.fornecedores.iter()
    }
    .map(|p| (p.pessoa, p.nome.clone()))
    .collect();
    let contas: Vec<(Id, String)> = estado
        .contas
        .iter()
        .map(|c| (c.conta, format!("{} · {}", c.codigo, c.nome)))
        .collect();
    let cats: Vec<(Id, String)> = estado
        .categorias
        .iter()
        .map(|c| (c.id, c.nome.clone()))
        .collect();

    let fechar = Dialogo::nova("Nova recorrência").largura(720.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovaRecorrencia(f) = &mut estado.dlg else {
                return;
            };
            ui.horizontal(|ui| {
                for (rot, receber) in [("A pagar", false), ("A receber", true)] {
                    let sel = f.a_receber == receber;
                    let b = if sel {
                        Botao::primario(rot)
                    } else {
                        Botao::fantasma(rot)
                    };
                    if ui.add(b).clicked() && !sel {
                        f.a_receber = receber;
                        f.contraparte = None;
                        f.conta = None;
                    }
                }
            });
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Descrição", &mut f.descricao).marcador("Aluguel da loja"));
            ui.add_space(Espaco::E12);
            SeletorOpcao::novo(
                if a_receber {
                    "Cliente (opcional)"
                } else {
                    "Favorecido (opcional)"
                },
                &mut f.contraparte,
            )
            .opcoes(pessoas.clone())
            .placeholder(if a_receber {
                "Sem cliente informado"
            } else {
                "Sem favorecido informado"
            })
            .mostrar(ui);
            if ui
                .add(botao_cadastro_rapido(if a_receber {
                    "+ Cadastrar cliente"
                } else {
                    "+ Cadastrar favorecido"
                }))
                .clicked()
            {
                estado.dlg_rapido = Some(DlgRapido::Pessoa {
                    alvo: AlvoRapido::Recorrencia,
                    papel: if a_receber {
                        Papel::Cliente
                    } else {
                        Papel::Fornecedor
                    },
                    pessoa: crate::pessoa::EstadoPessoa::cadastro(),
                });
            }
            ui.add_space(Espaco::E12);
            SeletorOpcao::novo(
                if a_receber {
                    "Conta de receita"
                } else {
                    "Conta de despesa"
                },
                &mut f.conta,
            )
            .opcoes(contas.clone())
            .placeholder("Escolha a conta do plano")
            .mostrar(ui);
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Valor mensal", &mut f.valor).marcador("0,00"));
                SeletorOpcao::novo("Categoria (opcional)", &mut f.categoria)
                    .opcoes(cats.clone())
                    .placeholder("Sem categoria")
                    .mostrar(&mut c[1]);
                if c[1]
                    .add(botao_cadastro_rapido("+ Nova categoria"))
                    .clicked()
                {
                    estado.dlg_rapido = Some(DlgRapido::Categoria {
                        alvo: AlvoRapido::Recorrencia,
                        nome: String::new(),
                        especie: None,
                    });
                }
            });
            ui.add_space(Espaco::E12);
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Periodicidade"));
                for (rot, p) in [
                    ("Mensal", PeriodoRec::Mensal),
                    ("Semanal", PeriodoRec::Semanal),
                    ("Anual", PeriodoRec::Anual),
                ] {
                    let sel = f.periodicidade == p;
                    let b = if sel {
                        Botao::primario(rot)
                    } else {
                        Botao::fantasma(rot)
                    };
                    if ui.add(b).clicked() {
                        f.periodicidade = p;
                    }
                }
            });
            ui.add_space(Espaco::E12);
            ui.columns(3, |c| {
                let rot_dia = match f.periodicidade {
                    PeriodoRec::Semanal => "Dia da semana (0=dom)",
                    _ => "Dia do mês",
                };
                c[0].add(Campo::novo(rot_dia, &mut f.dia_referencia));
                c[1].add(Campo::novo("Início", &mut f.inicio).mascara(Mascara::Data));
                c[2].add(Campo::novo("Gerar com (dias)", &mut f.antecedencia));
            });
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Fim (opcional)", &mut f.fim).mascara(Mascara::Data));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar recorrência")).clicked() {
                criar_recorrencia(ui.ctx(), motor, sessao, estado);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.carregar_recorrencias(motor, sessao);
                estado.dlg = Dlg::Recorrencias;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

pub(super) fn criar_recorrencia(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Dlg::NovaRecorrencia(f) = &estado.dlg else {
        return;
    };
    if f.descricao.trim().is_empty() {
        notificar(ctx, Notificacao::aviso("Informe a descrição."));
        return;
    }
    // Cliente/fornecedor é opcional (pedido explícito do usuário) — uma recorrência avulsa
    // não precisa de uma pessoa cadastrada.
    let contraparte_id = f.contraparte;
    let Some(conta) = f.conta else {
        notificar(ctx, Notificacao::aviso("Selecione a conta do plano."));
        return;
    };
    let (Ok(valor), Ok(inicio), Ok(dia), Ok(antecedencia)) = (
        f.valor.parse::<Dinheiro>(),
        f.inicio.parse::<Data>(),
        f.dia_referencia.parse::<u8>(),
        f.antecedencia.parse::<u16>(),
    ) else {
        notificar(
            ctx,
            Notificacao::aviso("Confira valor (0,00), início (dd/mm/aaaa), dia e antecedência."),
        );
        return;
    };
    let fim = if f.fim.trim().is_empty() {
        None
    } else {
        match f.fim.parse::<Data>() {
            Ok(d) => Some(d),
            Err(_) => {
                notificar(ctx, Notificacao::aviso("Fim inválido (dd/mm/aaaa)."));
                return;
            }
        }
    };
    let especie = if f.a_receber {
        EspecieTitulo::Receber
    } else {
        EspecieTitulo::Pagar
    };
    let contraparte = contraparte_id.map(|id| {
        if f.a_receber {
            Contraparte::Cliente(id)
        } else {
            Contraparte::Fornecedor(id)
        }
    });
    let cmd = CriarRecorrencia {
        descricao: f.descricao.trim().to_owned(),
        especie,
        contraparte,
        tipo_valor: TipoValor::Fixo,
        valor_fixo: Some(valor),
        indice: None,
        media_ultimos_n: None,
        periodicidade: f.periodicidade.dominio(),
        dia_referencia: Some(dia),
        expressao_cron: None,
        inicio,
        fim,
        conta_contrapartida: conta,
        centro_custo: None,
        categoria: f.categoria,
        antecedencia_geracao_dias: antecedencia,
    };
    match motor
        .executar(sessao, "financeiro.criar_recorrencia.v1", &cmd)
        .map(|_: mod_financeiro::RecorrenciaCriada| ())
    {
        Ok(()) => {
            estado.erro = None;
            estado.carregar_recorrencias(motor, sessao);
            estado.dlg = Dlg::Recorrencias;
            notificar(ctx, Notificacao::sucesso("Recorrência criada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
