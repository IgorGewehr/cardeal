//! As abas "Fluxo" (extrato do disponível) e "Bancos" (contas e saldos).

use super::*;

pub(super) fn painel_fluxo(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    ui.horizontal(|ui| {
        ui.add(Rotulo::sobrelinha("Período"));
        ui.add_space(Espaco::E8);
        if let Some(d) = Abas::nova(&[(30_i64, "30 dias"), (90, "90 dias"), (365, "12 meses")])
            .selecionada(estado.fluxo_dias)
            .id_salt("financeiro-fluxo-periodo")
            .mostrar(ui)
        {
            estado.fluxo_dias = d;
            estado.carregar_fluxo(motor, sessao);
        }
    });
    ui.add_space(Espaco::E12);

    // Fluxo de caixa é o local de "bater o caixa": o lojista confere o físico contra o
    // sistema, então os cards aqui são só saldo em caixa e saldo em bancos — nada de
    // "entrou/saiu no período" (isso mora nos cards de Contas a Receber/Pagar agora,
    // pedido explícito do usuário).
    let saldo_caixa: Dinheiro = estado
        .contas_disp
        .iter()
        .filter(|c| c.e_caixa)
        .map(|c| c.saldo)
        .fold(Dinheiro::ZERO, |a, b| a + b);
    let saldo_bancos: Dinheiro = estado
        .contas_disp
        .iter()
        .filter(|c| !c.e_caixa)
        .map(|c| c.saldo)
        .fold(Dinheiro::ZERO, |a, b| a + b);

    ui.columns(2, |col| {
        kpi(
            &mut col[0],
            "Saldo em caixa",
            saldo_caixa,
            if saldo_caixa.e_negativo() {
                Tom::Negativo
            } else {
                Tom::Neutro
            },
            Some("confira contra o caixa físico"),
        );
        kpi(
            &mut col[1],
            "Saldo em conta bancária",
            saldo_bancos,
            if saldo_bancos.e_negativo() {
                Tom::Negativo
            } else {
                Tom::Neutro
            },
            None,
        );
    });
    ui.add_space(Espaco::E16);

    if estado.extrato.is_empty() {
        EstadoVazio::novo(
            Icone::Dinheiro,
            "Nenhum movimento no período. Aqui só entra o que de fato entrou ou saiu do caixa e \
             dos bancos — título ainda em aberto não aparece.",
        )
        .mostrar(ui);
        return;
    }

    ui.add(Rotulo::titulo_secao("Extrato"));
    ui.add_space(Espaco::E8);
    let colunas = vec![
        ColunaGrade::nova("Data").largura(96.0),
        ColunaGrade::nova("Histórico"),
        ColunaGrade::nova("Conta").largura(140.0),
        ColunaGrade::nova("Valor").largura(130.0).numero(),
        ColunaGrade::nova("Acumulado").largura(130.0).numero(),
    ];
    let mut acc = Dinheiro::ZERO;
    let linhas: Vec<(String, String, String, Dinheiro, Dinheiro)> = estado
        .extrato
        .iter()
        .map(|m| {
            acc += m.valor;
            let hist = match estado.rotulo_origem(m) {
                Some(origem) => format!("{} — {origem}", m.historico),
                None => m.historico.clone(),
            };
            (
                m.data.formatar_curta(),
                hist,
                m.conta_nome.clone(),
                m.valor,
                acc,
            )
        })
        .collect();
    Grade::nova(colunas).mostrar(ui, linhas.len(), |i, row| {
        let (data, hist, conta, valor, acumulado) = &linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::campo(data.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(hist.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(conta.clone()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(*valor).com_sinal());
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(*acumulado));
        });
    });
}

pub(super) fn painel_bancos(ui: &mut egui::Ui, estado: &mut EstadoTelaFinanceiro) {
    if estado.contas_disp.is_empty() {
        if EstadoVazio::novo(Icone::Dinheiro, "Nenhuma conta de caixa ou banco ainda.")
            .acao("+ Nova conta bancária")
            .mostrar(ui)
        {
            estado.dlg = Dlg::NovaContaBancaria {
                nome: String::new(),
            };
        }
        return;
    }
    let total: Dinheiro = estado
        .contas_disp
        .iter()
        .map(|c| c.saldo)
        .fold(Dinheiro::ZERO, |a, b| a + b);

    let colunas = vec![
        ColunaGrade::nova("Conta"),
        ColunaGrade::nova("Código").largura(90.0),
        ColunaGrade::nova("Tipo").largura(90.0),
        ColunaGrade::nova("Saldo").largura(140.0).numero(),
    ];
    let mut nova = false;
    Painel::novo()
        .titulo(format!(
            "Total disponível · {}",
            total.formatar_com_simbolo()
        ))
        .compacto()
        .mostrar_com_acoes(
            ui,
            |ui| {
                nova = ui
                    .add(Botao::secundario("+ Nova conta bancária").pequeno())
                    .clicked()
            },
            |ui| {
                Grade::nova(colunas).mostrar(ui, estado.contas_disp.len(), |i, row| {
                    let c = &estado.contas_disp[i];
                    row.col(|ui| {
                        ui.add(Rotulo::interface(c.nome.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::campo(c.codigo.clone()));
                    });
                    row.col(|ui| {
                        ui.add(Rotulo::campo(if c.e_caixa { "Caixa" } else { "Banco" }));
                    });
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(c.saldo));
                    });
                });
            },
        );
    if nova {
        estado.dlg = Dlg::NovaContaBancaria {
            nome: String::new(),
        };
    }
}

pub(super) fn dialogo_conta_bancaria(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let fechar = Dialogo::nova("Nova conta bancária").largura(460.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::NovaContaBancaria { nome } = &mut estado.dlg else {
                return;
            };
            ui.add(Rotulo::apoio(
                "Entra no plano como conta analítica do grupo 1.1 (Disponível).",
            ));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Nome", nome).marcador("Nubank — cc 12345-6"));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Criar")).clicked() {
                if let Dlg::NovaContaBancaria { nome } = &estado.dlg {
                    let nome = nome.trim().to_owned();
                    if nome.is_empty() {
                        notificar(ui.ctx(), Notificacao::aviso("Informe o nome da conta."));
                        return;
                    }
                    match motor
                        .executar(
                            sessao,
                            "financeiro.criar_conta_bancaria.v1",
                            &CriarContaBancaria { nome },
                        )
                        .map(|_: ContaBancariaCriada| ())
                    {
                        Ok(()) => {
                            estado.dlg = Dlg::Fechado;
                            estado.carregar_fluxo(motor, sessao);
                            notificar(ui.ctx(), Notificacao::sucesso("Conta bancária criada"));
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
