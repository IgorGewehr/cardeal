//! "Quanto estou ganhando nisso": o bloco "Resultado" (total, custo, lucro, margem e o lucro
//! por hora trabalhada, do tempo apontado) e a escolha da garantia, salva num clique.

use super::*;

/// Os prazos de garantia oferecidos no seletor (dias; `0` = sem garantia).
const PRAZOS_GARANTIA: [(u16, &str); 5] = [
    (0, "Sem garantia"),
    (30, "30 dias"),
    (90, "90 dias"),
    (180, "6 meses"),
    (365, "1 ano"),
];

/// Segundos trabalhados na OS, com o cronômetro aberto do usuário somado.
pub(super) fn segundos_trabalhados(estado: &EstadoTelaOs, sessao: &Sessao) -> i64 {
    let aberto = estado
        .apontamentos
        .iter()
        .find(|a| a.tecnico == sessao.usuario() && a.esta_aberto())
        .map_or(0, |a| a.duracao_segundos(Instante::agora()));
    estado.tempo_total_ordem + aberto
}

/// `valor` por hora, dado o tempo em segundos (nada abaixo de um minuto).
fn por_hora(valor: Dinheiro, segundos: i64) -> Option<Dinheiro> {
    (segundos >= 60).then(|| {
        let centavos = i128::from(valor.em_centavos()) * 3600 / i128::from(segundos);
        Dinheiro::centavos(i64::try_from(centavos).unwrap_or(i64::MAX))
    })
}

/// O bloco "Resultado".
pub(super) fn secao_resultado(
    ui: &mut egui::Ui,
    sessao: &Sessao,
    estado: &EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    let segundos = segundos_trabalhados(estado, sessao);
    Painel::novo().plano().titulo("Resultado").mostrar_com_acoes(
        ui,
        |ui| {
            ui.add(ValorDinheiro::novo(os.valor_total));
            ui.add(Rotulo::campo("Total da OS"));
        },
        |ui| {
            let lucro = estado
                .margem_prevista
                .as_ref()
                .map(|m| m.painel.margem_liquida);
            ui.columns(3, |c| {
                Dado::novo("Tempo trabalhado", formatar_duracao(segundos)).mostrar(&mut c[0]);
                Dado::novo(
                    "Recebendo por hora",
                    por_hora(os.valor_total, segundos)
                        .map_or_else(|| "—".to_owned(), |v| v.formatar_com_simbolo()),
                )
                .mostrar(&mut c[1]);
                Dado::novo(
                    "Lucro por hora",
                    lucro
                        .and_then(|l| por_hora(l, segundos))
                        .map_or_else(|| "—".to_owned(), |v| v.formatar_com_simbolo()),
                )
                .mostrar(&mut c[2]);
            });
            if segundos < 60 {
                ui.add(
                    Rotulo::apoio(
                        "Ligue o cronômetro em \"Tempo de reparo\" para ver quanto rende cada hora.",
                    )
                    .quebravel(),
                );
            }
            if let Some(m) = &estado.margem_prevista {
                ui.add_space(Espaco::E8);
                let p = &m.painel;
                cardeal_ui::organisms::PainelMargem::novo(
                    p.receita,
                    p.custo,
                    p.margem_bruta,
                    p.margem_liquida,
                    p.margem_liquida_pct,
                )
                .minimo(m.margem_minima)
                .itens_sem_custo(p.itens_sem_custo)
                .mostrar(ui);
                if m.custo_estimado {
                    ui.add(
                        Rotulo::apoio(
                            "Peça ainda não aplicada: o custo é o combinado na encomenda ou o \
                             custo médio de hoje (com o frete rateado na compra).",
                        )
                        .quebravel(),
                    );
                }
                if p.itens_sem_custo > 0 && !detalhe.itens_mao_de_obra.is_empty() {
                    ui.add(
                        Rotulo::apoio(
                            "Mão de obra entra no custo pelas horas e o custo-hora do técnico — \
                             defina em \"Custo-hora da equipe…\".",
                        )
                        .quebravel(),
                    );
                }
            }
        },
    );
}

/// O bloco "Garantia" da coluna lateral: um seletor que salva ao escolher.
pub(super) fn secao_garantia(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    let editavel = os.aceita_edicao_de_dados();
    let mut escolha = Some(os.garantia_dias);
    Painel::novo()
        .plano()
        .compacto()
        .titulo("Garantia")
        .mostrar_com_acoes(
            ui,
            |ui| {
                if os.garantia_dias > 0 {
                    ui.add(Etiqueta::positiva("oferecida").com_ponto());
                } else {
                    ui.add(Etiqueta::neutra("sem garantia"));
                }
            },
            |ui| {
                if editavel {
                    let mut opcoes: Vec<(u16, String)> = PRAZOS_GARANTIA
                        .iter()
                        .map(|(d, t)| (*d, (*t).to_owned()))
                        .collect();
                    if !opcoes.iter().any(|(d, _)| *d == os.garantia_dias) {
                        opcoes.push((os.garantia_dias, format!("{} dias", os.garantia_dias)));
                    }
                    SeletorOpcao::novo("Prazo sobre o serviço", &mut escolha)
                        .opcoes(opcoes)
                        .mostrar(ui);
                } else {
                    Dado::novo(
                        "Prazo sobre o serviço",
                        if os.garantia_dias > 0 {
                            format!("{} dias", os.garantia_dias)
                        } else {
                            "Sem garantia".to_owned()
                        },
                    )
                    .mostrar(ui);
                }
            },
        );
    ui.add_space(Espaco::E12);
    if let Some(dias) = escolha.filter(|d| *d != os.garantia_dias) {
        aplicar_e_recarregar(
            ui.ctx(),
            motor,
            sessao,
            estado,
            os.id,
            "os.editar_dados_da_ordem.v1",
            &EditarDadosDaOrdem {
                ordem_servico: os.id,
                equipamento: None,
                complemento_defeito_relatado: None,
                ficha: None,
                garantia_dias: Some(dias),
            },
            if dias == 0 {
                "OS sem garantia"
            } else {
                "Garantia atualizada"
            },
        );
    }
}
