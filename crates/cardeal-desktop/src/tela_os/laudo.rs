//! O bloco "Laudo técnico" do detalhe da OS: problema relatado e diagnóstico, com o aviso de
//! alteração não salva e o botão só ativo quando há o que salvar.

use super::*;

/// O laudo como está gravado — `(problema, diagnóstico)`. Sem laudo, o defeito da abertura.
fn laudo_gravado(detalhe: &DetalheOrdem) -> (String, String) {
    detalhe.laudo.as_ref().map_or_else(
        || (detalhe.ordem.defeito_relatado.clone(), String::new()),
        |l| {
            (
                l.descricao_problema.clone(),
                l.diagnostico.clone().unwrap_or_default(),
            )
        },
    )
}

pub(super) fn secao_laudo(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    let editavel = matches!(os.estado, EstadoOs::Aberta | EstadoOs::EmDiagnostico);
    let ja_tem_laudo = detalhe.laudo.is_some();
    let (problema_gravado, diagnostico_gravado) = laudo_gravado(detalhe);
    let alterado = estado.laudo_problema.trim() != problema_gravado.trim()
        || estado.laudo_diagnostico.trim() != diagnostico_gravado.trim();
    let problema_vazio = estado.laudo_problema.trim().is_empty();

    let mut salvar = false;
    Painel::novo()
        .plano()
        .titulo("Laudo técnico")
        .mostrar_com_acoes(
            ui,
            |ui| {
                if editavel && alterado {
                    ui.add(Etiqueta::atencao("alterações não salvas").com_ponto());
                } else if ja_tem_laudo {
                    ui.add(Etiqueta::positiva("registrado").com_ponto());
                } else {
                    ui.add(Etiqueta::neutra("sem laudo"));
                }
            },
            |ui| {
                if editavel {
                    ui.columns(2, |c| {
                        c[0].add(
                            Campo::novo("Problema relatado", &mut estado.laudo_problema)
                                .multilinha(3)
                                .marcador("O que o cliente disse que acontece")
                                .erro(problema_vazio.then_some("Descreva o problema relatado")),
                        );
                        c[1].add(
                            Campo::novo("Meu diagnóstico", &mut estado.laudo_diagnostico)
                                .multilinha(3)
                                .marcador("O que encontrei — e as peças que vou precisar"),
                        );
                    });
                    ui.add_space(Espaco::E8);
                    // `with_layout` direto numa coluna ocupa toda a altura que sobra (o vão
                    // enorme abaixo dos campos); dentro de um `horizontal` fica com uma linha.
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let rotulo = if ja_tem_laudo {
                                "Salvar laudo"
                            } else {
                                "Registrar laudo"
                            };
                            let pode = !problema_vazio && (alterado || !ja_tem_laudo);
                            if ui.add(Botao::primario(rotulo).habilitado(pode)).clicked() {
                                salvar = true;
                            }
                            if !pode && !problema_vazio {
                                ui.add(Rotulo::apoio("Nada novo para salvar."));
                            }
                        });
                    });
                } else if let Some(laudo) = &detalhe.laudo {
                    ui.columns(2, |c| {
                        Dado::novo("Problema relatado", laudo.descricao_problema.clone())
                            .mostrar(&mut c[0]);
                        Dado::novo("Diagnóstico", laudo.diagnostico.clone().unwrap_or_default())
                            .mostrar(&mut c[1]);
                    });
                } else {
                    ui.add(Rotulo::apoio("Nenhum laudo foi registrado nesta OS."));
                }
            },
        );

    if salvar {
        let diagnostico =
            (!estado.laudo_diagnostico.trim().is_empty()).then(|| estado.laudo_diagnostico.clone());
        aplicar_e_recarregar(
            ui.ctx(),
            motor,
            sessao,
            estado,
            os.id,
            "os.registrar_laudo.v1",
            &RegistrarLaudo {
                ordem_servico: os.id,
                descricao_problema: estado.laudo_problema.clone(),
                diagnostico,
                tecnico: sessao.usuario(),
            },
            if ja_tem_laudo {
                "Laudo atualizado"
            } else {
                "Laudo registrado"
            },
        );
    }
}
