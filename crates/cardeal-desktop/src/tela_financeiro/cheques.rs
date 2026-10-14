//! A aba "Cheques": a carteira de cheques recebidos (`mod_financeiro::cheque`). Receber um
//! cheque dá a baixa na parcela, mas o dinheiro só entra quando o banco compensa; se voltar, a
//! devolução reabre a parcela. Os pré-datados mostram quando podem ir ao banco.

use super::*;
use cardeal_ui::molecules::{OpcaoBusca, SeletorBusca};
use mod_financeiro::cheque::{descricao_alinea, DadosDoCheque, EstadoCheque, ALINEAS};
use mod_financeiro::{
    CarteiraDeCheques, ChequeCompensado, ChequeFoiDevolvido, ChequeRegistrado, CompensarCheque,
    DevolverCheque, ItemCheque, ItemTituloEmAberto, RegistrarCheque, TitulosAReceberEmAberto,
};

/// O formulário "Receber cheque".
#[derive(Default)]
pub(super) struct FormCheque {
    pub(super) busca: String,
    pub(super) parcela: Option<Id>,
    pub(super) valor: String,
    pub(super) recebido_em: String,
    pub(super) bom_para: String,
    pub(super) banco: String,
    pub(super) agencia: String,
    pub(super) conta: String,
    pub(super) numero: String,
    pub(super) emitente: String,
    pub(super) documento: String,
}

/// A ação aberta sobre um cheque da grade.
pub(super) enum AcaoCheque {
    Compensar {
        cheque: Id,
        conta: Option<Id>,
        data: String,
    },
    Devolver {
        cheque: Id,
        alinea: Option<u8>,
        observacao: String,
    },
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoCheques {
    pub(super) itens: Vec<ItemCheque>,
    /// 0 = em carteira, 1 = compensados, 2 = devolvidos, 3 = todos.
    pub(super) filtro: u8,
    pub(super) erro: Option<String>,
    pub(super) abertas: Vec<ItemTituloEmAberto>,
    pub(super) contas: Vec<ItemContaDisponivel>,
    pub(super) form: Option<FormCheque>,
    pub(super) acao: Option<AcaoCheque>,
}

impl EstadoCheques {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        match motor.consultar(
            sessao,
            "financeiro.carteira_de_cheques.v1",
            &CarteiraDeCheques { estado: None },
        ) {
            Ok(i) => {
                self.itens = i;
                self.erro = None;
            }
            Err(e) => {
                self.itens.clear();
                self.erro = Some(e.mensagem);
            }
        }
        self.abertas = motor
            .consultar(
                sessao,
                "financeiro.titulos_a_receber_em_aberto.v1",
                &TitulosAReceberEmAberto,
            )
            .unwrap_or_default();
        self.contas = motor
            .consultar(
                sessao,
                "financeiro.contas_disponiveis.v1",
                &ContasDisponiveis,
            )
            .unwrap_or_default();
        self.acao = None;
    }

    fn visiveis(&self) -> Vec<&ItemCheque> {
        self.itens
            .iter()
            .filter(|i| match self.filtro {
                0 => i.cheque.estado == EstadoCheque::EmCarteira,
                1 => i.cheque.estado == EstadoCheque::Compensado,
                2 => i.cheque.estado == EstadoCheque::Devolvido,
                _ => true,
            })
            .collect()
    }
}

fn data(texto: &str, rotulo: &str) -> Result<Data, String> {
    Data::de_str(texto.trim()).map_err(|_| format!("{rotulo}: digite a data como dd/mm/aaaa"))
}

/// Registra o cheque do formulário.
pub(super) fn registrar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) -> Result<ChequeRegistrado, String> {
    let f = estado
        .cheques
        .form
        .as_ref()
        .ok_or_else(|| "Abra o formulário".to_owned())?;
    let parcela = f
        .parcela
        .ok_or_else(|| "Escolha a conta a receber que o cheque paga".to_owned())?;
    let valor = Dinheiro::de_str(f.valor.trim())
        .ok()
        .filter(|v| v.e_positivo())
        .ok_or_else(|| "Digite o valor do cheque".to_owned())?;
    let recebido_em = data(&f.recebido_em, "Recebido em")?;
    let bom_para = if f.bom_para.trim().is_empty() {
        recebido_em
    } else {
        data(&f.bom_para, "Bom para")?
    };
    let comando = RegistrarCheque {
        parcela,
        valor,
        recebido_em,
        bom_para,
        dados: DadosDoCheque {
            banco: f.banco.clone(),
            agencia: f.agencia.clone(),
            conta: f.conta.clone(),
            numero: f.numero.clone(),
            emitente: f.emitente.clone(),
            documento_emitente: Some(f.documento.clone()).filter(|d| !d.trim().is_empty()),
        },
    };
    let r: ChequeRegistrado = motor
        .executar(sessao, "financeiro.registrar_cheque.v1", &comando)
        .map_err(|e| e.mensagem)?;
    estado.cheques.form = None;
    estado.cheques.carregar(motor, sessao);
    Ok(r)
}

/// Executa a ação aberta (compensar ou devolver); devolve a mensagem de sucesso.
pub(super) fn executar_acao(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) -> Result<String, String> {
    let msg = match estado.cheques.acao.as_ref() {
        None => return Err("Nada a fazer".to_owned()),
        Some(AcaoCheque::Compensar {
            cheque,
            conta,
            data: d,
        }) => {
            let conta = conta.ok_or_else(|| "Escolha a conta do depósito".to_owned())?;
            let _: ChequeCompensado = motor
                .executar(
                    sessao,
                    "financeiro.compensar_cheque.v1",
                    &CompensarCheque {
                        cheque: *cheque,
                        conta,
                        data: data(d, "Compensado em")?,
                    },
                )
                .map_err(|e| e.mensagem)?;
            "Cheque compensado: o dinheiro entrou na conta".to_owned()
        }
        Some(AcaoCheque::Devolver {
            cheque,
            alinea,
            observacao,
        }) => {
            let _: ChequeFoiDevolvido = motor
                .executar(
                    sessao,
                    "financeiro.devolver_cheque.v1",
                    &DevolverCheque {
                        cheque: *cheque,
                        alinea: *alinea,
                        observacao: observacao.clone(),
                    },
                )
                .map_err(|e| e.mensagem)?;
            "Cheque devolvido: a conta voltou a ficar em aberto".to_owned()
        }
    };
    estado.cheques.carregar(motor, sessao);
    Ok(msg)
}

fn situacao(i: &ItemCheque, hoje: Data) -> Etiqueta {
    let c = &i.cheque;
    match c.estado {
        EstadoCheque::EmCarteira if c.pronto_para_depositar(hoje) => {
            Etiqueta::atencao("pode depositar")
        }
        EstadoCheque::EmCarteira => {
            Etiqueta::info(format!("pré-datado · {}", c.bom_para.formatar_curta()))
        }
        EstadoCheque::Compensado => Etiqueta::positiva(format!(
            "compensado · {}",
            c.compensado_em
                .map(Data::formatar_curta)
                .unwrap_or_default()
        )),
        EstadoCheque::Devolvido => Etiqueta::negativa(match c.alinea {
            Some(a) => format!("devolvido · alínea {a}"),
            None => "devolvido".to_owned(),
        }),
    }
}

pub(super) fn painel_cheques(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let hoje = Data::hoje(Fuso::BRASILIA);
    if let Some(erro) = &estado.cheques.erro {
        EstadoVazio::novo(Icone::Dinheiro, erro.clone()).mostrar(ui);
        return;
    }
    let c = &estado.cheques;
    let em_carteira: Vec<&ItemCheque> = c
        .itens
        .iter()
        .filter(|i| i.cheque.estado == EstadoCheque::EmCarteira)
        .collect();
    let soma = |v: &[&ItemCheque]| v.iter().fold(Dinheiro::ZERO, |t, i| t + i.cheque.valor);
    let prontos: Vec<&ItemCheque> = em_carteira
        .iter()
        .copied()
        .filter(|i| i.cheque.pronto_para_depositar(hoje))
        .collect();
    let devolvidos = c
        .itens
        .iter()
        .filter(|i| i.cheque.estado == EstadoCheque::Devolvido)
        .count();
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Em carteira", soma(&em_carteira))
            .tom(Tom::Neutro)
            .variacao(format!("{} cheque(s)", em_carteira.len())),
        CartaoKpi::novo("Pode depositar", soma(&prontos))
            .tom(if prontos.is_empty() {
                Tom::Neutro
            } else {
                Tom::Atencao
            })
            .variacao(format!("{} já no \"bom para\"", prontos.len())),
        CartaoKpi::contagem("Devolvidos", devolvidos).tom(if devolvidos == 0 {
            Tom::Neutro
        } else {
            Tom::Negativo
        }),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    ui.horizontal(|ui| {
        if ui.add(Botao::primario("+ Receber cheque")).clicked() {
            estado.cheques.form = Some(FormCheque {
                recebido_em: hoje.formatar(),
                ..FormCheque::default()
            });
        }
        ui.add_space(Espaco::E12);
        if let Some(f) = Abas::nova(&[
            (0_u8, "Em carteira"),
            (1, "Compensados"),
            (2, "Devolvidos"),
            (3, "Todos"),
        ])
        .selecionada(estado.cheques.filtro)
        .id_salt("cheques-filtro")
        .mostrar(ui)
        {
            estado.cheques.filtro = f;
        }
    });
    formulario(ui, motor, sessao, estado);
    ui.add_space(Espaco::E8);
    grade(ui, hoje, estado);
    acao(ui, motor, sessao, estado);
}

fn formulario(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    if estado.cheques.form.is_none() {
        return;
    }
    let opcoes: Vec<OpcaoBusca<Id>> = estado
        .cheques
        .abertas
        .iter()
        .map(|p| {
            OpcaoBusca::nova(
                p.parcela,
                format!(
                    "{} — {}",
                    estado.nome_contraparte(&p.contraparte),
                    p.saldo().formatar_com_simbolo()
                ),
            )
            .subtitulo(format!(
                "vence {}{}",
                p.vencimento.formatar(),
                p.descricao
                    .as_ref()
                    .map(|d| format!(" · {d}"))
                    .unwrap_or_default()
            ))
        })
        .collect();
    let saldos: Vec<(Id, Dinheiro)> = estado
        .cheques
        .abertas
        .iter()
        .map(|p| (p.parcela, p.saldo()))
        .collect();
    let (mut salvar, mut cancelar) = (false, false);
    ui.add_space(Espaco::E8);
    let Some(f) = estado.cheques.form.as_mut() else {
        return;
    };
    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Receber cheque"));
        ui.add(Rotulo::apoio(
            "A conta a receber fica paga, mas o dinheiro só entra quando o banco compensar. \
             Pré-datado: o \"bom para\" diz quando pode depositar.",
        ));
        ui.add_space(Espaco::E8);
        let antes = f.parcela;
        SeletorBusca::novo("Conta a receber que ele paga", &mut f.busca, &mut f.parcela)
            .opcoes(opcoes)
            .marcador("Cliente ou descrição")
            .mostrar(ui);
        if f.parcela != antes && f.valor.trim().is_empty() {
            if let Some((_, saldo)) = saldos.iter().find(|(p, _)| Some(*p) == f.parcela) {
                f.valor = saldo.formatar();
            }
        }
        ui.add_space(Espaco::E8);
        ui.columns(3, |c| {
            c[0].add(Campo::novo("Valor", &mut f.valor).marcador("0,00"));
            c[1].add(Campo::novo("Recebido em", &mut f.recebido_em).mascara(Mascara::Data));
            c[2].add(
                Campo::novo("Bom para", &mut f.bom_para)
                    .mascara(Mascara::Data)
                    .marcador("à vista"),
            );
        });
        ui.columns(4, |c| {
            c[0].add(Campo::novo("Banco", &mut f.banco).marcador("001"));
            c[1].add(Campo::novo("Agência", &mut f.agencia));
            c[2].add(Campo::novo("Conta", &mut f.conta));
            c[3].add(Campo::novo("Nº do cheque", &mut f.numero));
        });
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Emitente", &mut f.emitente).marcador("Titular da conta"));
            c[1].add(Campo::novo("CPF/CNPJ do emitente", &mut f.documento).marcador("opcional"));
        });
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            salvar = ui.add(Botao::primario("Receber cheque")).clicked();
            cancelar = ui.add(Botao::fantasma("Cancelar")).clicked();
        });
    });
    if cancelar {
        estado.cheques.form = None;
    } else if salvar {
        match registrar(motor, sessao, estado) {
            Ok(r) => notificar(
                ui.ctx(),
                Notificacao::sucesso(if r.parcela_quitada {
                    "Cheque recebido: a conta ficou paga"
                } else {
                    "Cheque recebido: a conta segue com saldo"
                }),
            ),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
}

fn grade(ui: &mut egui::Ui, hoje: Data, estado: &mut EstadoTelaFinanceiro) {
    let linhas: Vec<ItemCheque> = estado.cheques.visiveis().into_iter().cloned().collect();
    let nomes: Vec<String> = linhas
        .iter()
        .map(|i| {
            i.cliente
                .and_then(|c| estado.nomes.get(&c).cloned())
                .or_else(|| i.descricao.clone())
                .unwrap_or_default()
        })
        .collect();
    let mut abrir: Option<AcaoCheque> = None;
    let contas_padrao = estado
        .cheques
        .contas
        .iter()
        .find(|c| !c.e_caixa)
        .map(|c| c.conta);
    Grade::nova(vec![
        ColunaGrade::nova("Bom para").largura(90.0),
        ColunaGrade::nova("Emitente").largura(220.0),
        ColunaGrade::nova("Cheque").largura(300.0),
        ColunaGrade::nova("Valor").largura(110.0).numero(),
        ColunaGrade::nova("Situação").largura(170.0),
        ColunaGrade::nova("").largura(200.0),
    ])
    .com_acoes()
    .id_salt("cheques")
    .vazio("Nenhum cheque neste recorte.")
    .mostrar(ui, linhas.len(), |i, row| {
        let item = &linhas[i];
        let c = &item.cheque;
        row.col(|ui| {
            ui.add(Rotulo::interface(c.bom_para.formatar_curta()));
        });
        row.col(|ui| {
            ui.vertical(|ui| {
                ui.add(Rotulo::interface(c.dados.emitente.clone()));
                if !nomes[i].is_empty() {
                    ui.add(Rotulo::apoio(nomes[i].clone()));
                }
            });
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(c.dados.resumo()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(c.valor).neutro());
        });
        row.col(|ui| {
            ui.add(situacao(item, hoje));
        });
        row.col(|ui| {
            ui.horizontal(|ui| {
                if c.estado == EstadoCheque::EmCarteira
                    && ui.add(Botao::primario("Compensou").pequeno()).clicked()
                {
                    abrir = Some(AcaoCheque::Compensar {
                        cheque: c.id,
                        conta: contas_padrao,
                        data: hoje.formatar(),
                    });
                }
                if c.estado != EstadoCheque::Devolvido
                    && ui.add(Botao::fantasma("Voltou").pequeno()).clicked()
                {
                    abrir = Some(AcaoCheque::Devolver {
                        cheque: c.id,
                        alinea: None,
                        observacao: String::new(),
                    });
                }
            });
        });
    });
    if abrir.is_some() {
        estado.cheques.acao = abrir;
    }
}

fn acao(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaFinanceiro) {
    let contas: Vec<(Id, String)> = estado
        .cheques
        .contas
        .iter()
        .map(|c| (c.conta, format!("{} ({})", c.nome, c.codigo)))
        .collect();
    let (mut confirmar, mut cancelar) = (false, false);
    let Some(a) = estado.cheques.acao.as_mut() else {
        return;
    };
    ui.add_space(Espaco::E12);
    match a {
        AcaoCheque::Compensar { conta, data, .. } => {
            Painel::novo().realce(Tom::Positivo).mostrar(ui, |ui| {
                ui.add(Rotulo::titulo_secao("O banco compensou o cheque"));
                ui.add(Rotulo::apoio(
                    "O dinheiro entra na conta escolhida. Quando o extrato chegar, o depósito \
                     casa sozinho na Conciliação.",
                ));
                ui.add_space(Espaco::E8);
                ui.columns(2, |c| {
                    SeletorOpcao::novo("Depositado em", conta)
                        .opcoes(contas.clone())
                        .mostrar(&mut c[0]);
                    c[1].add(Campo::novo("Compensado em", data).mascara(Mascara::Data));
                });
                ui.add_space(Espaco::E8);
                ui.horizontal(|ui| {
                    confirmar = ui.add(Botao::primario("Confirmar compensação")).clicked();
                    cancelar = ui.add(Botao::fantasma("Cancelar")).clicked();
                });
            });
        }
        AcaoCheque::Devolver {
            alinea, observacao, ..
        } => {
            Painel::novo().realce(Tom::Negativo).mostrar(ui, |ui| {
                ui.add(Rotulo::titulo_secao("O cheque voltou"));
                ui.add(Rotulo::apoio(
                    "Estorna a compensação (se já tinha entrado) e reabre a conta a receber do \
                     cliente.",
                ));
                ui.add_space(Espaco::E8);
                ui.columns(2, |c| {
                    SeletorOpcao::novo("Alínea", alinea)
                        .opcoes(
                            ALINEAS
                                .iter()
                                .map(|(n, d)| (*n, format!("{n} — {d}")))
                                .collect::<Vec<_>>(),
                        )
                        .placeholder("Se o banco informou")
                        .mostrar(&mut c[0]);
                    c[1].add(Campo::novo("Observação", observacao).marcador("opcional"));
                });
                if let Some(d) = alinea.and_then(descricao_alinea) {
                    ui.add(Rotulo::apoio(d));
                }
                ui.add_space(Espaco::E8);
                ui.horizontal(|ui| {
                    confirmar = ui
                        .add(Botao::primario("Registrar devolução").tom(Tom::Negativo))
                        .clicked();
                    cancelar = ui.add(Botao::fantasma("Cancelar")).clicked();
                });
            });
        }
    }
    if cancelar {
        estado.cheques.acao = None;
    } else if confirmar {
        match executar_acao(motor, sessao, estado) {
            Ok(m) => notificar(ui.ctx(), Notificacao::sucesso(m)),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
}
