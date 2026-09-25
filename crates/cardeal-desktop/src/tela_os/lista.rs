//! A listagem de OS: indicadores da fila ativa, busca/filtro (no motor) e a grade.

use super::*;

pub(super) fn lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let sem_busca = estado.busca.trim().is_empty()
        && matches!(estado.filtro_status, None | Some(FiltroStatusOs::Ativas));
    if estado.ativas.is_empty() && estado.ordens.is_empty() && sem_busca {
        if estado.erro.is_none()
            && EstadoVazio::novo(Icone::Ferramenta, "Nenhuma ordem em aberto.")
                .acao("Abrir a primeira OS")
                .mostrar(ui)
        {
            estado.dlg = Dlg::nova();
        }
        return;
    }

    // Saúde do pipeline (`docs/12-ui-ux.md` §6): sempre sobre a fila ativa inteira, não sobre
    // o que a busca/filtro está mostrando agora.
    let valor_parado = estado
        .ativas
        .iter()
        .fold(Dinheiro::ZERO, |acc, os| acc + os.valor_total);
    let aguardando_cliente = estado
        .ativas
        .iter()
        .filter(|os| os.estado == EstadoOs::AguardandoAprovacao)
        .count();
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Ordens em aberto", estado.ativas.len()),
        CartaoKpi::novo("Valor em aberto", valor_parado).variacao("soma do total de cada OS"),
        CartaoKpi::contagem("Aguardando aprovação", aguardando_cliente)
            .variacao("orçamento com o cliente"),
    ])
    .mostrar(ui);

    let filtro_antes = estado.filtro_status;
    let busca_mudou = BarraFiltros::nova(&mut estado.busca)
        .marcador("Buscar por nº, aparelho, defeito ou cliente")
        .filtro(|ui| {
            SeletorOpcao::novo("Status", &mut estado.filtro_status)
                .sem_rotulo()
                .opcao(FiltroStatusOs::Ativas, "Ativas (padrão)")
                .opcao(FiltroStatusOs::Todos, "Todas")
                .opcoes(
                    [
                        EstadoOs::Aberta,
                        EstadoOs::EmDiagnostico,
                        EstadoOs::AguardandoAprovacao,
                        EstadoOs::Aprovada,
                        EstadoOs::EmExecucao,
                        EstadoOs::Concluida,
                        EstadoOs::Faturada,
                        EstadoOs::Cancelada,
                        EstadoOs::Reprovada,
                    ]
                    .map(|e| (FiltroStatusOs::Um(e), estado_etiqueta(e).0)),
                )
                .mostrar(ui);
        })
        .mostrar(ui);
    if busca_mudou || estado.filtro_status != filtro_antes {
        estado.buscar_ordens(motor, sessao);
    }

    let indices: Vec<usize> = (0..estado.ordens.len()).collect();
    let colunas = vec![
        ColunaGrade::nova("Nº").largura(56.0).numero(),
        ColunaGrade::nova("Aparelho"),
        ColunaGrade::nova("Cliente").largura(180.0),
        ColunaGrade::nova("Estado").largura(170.0),
        ColunaGrade::nova("Total").largura(110.0).numero(),
        ColunaGrade::nova("Ações").largura(190.0),
    ];
    let mut acao_clicada = None;
    let resposta = Grade::nova(colunas)
        .com_acoes()
        .ordenacao(estado.ordenacao.atual())
        .vazio("Nenhuma ordem para essa busca/filtro de status.")
        .mostrar(ui, indices.len(), |i, row| {
            let os = &estado.ordens[indices[i]];
            row.col(|ui| {
                ui.add(Rotulo::interface(os.numero.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(equipamento_label(os).to_owned()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(nome_cliente(
                    &estado.clientes,
                    os.cliente,
                )));
            });
            row.col(|ui| {
                let (rotulo, tom) = estado_etiqueta(os.estado);
                ui.add(Etiqueta::nova(rotulo, tom));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(os.valor_total));
            });
            row.col(|ui| {
                acao_clicada = AcoesRegistro::novo()
                    .excluir(pode_cancelar(os.estado))
                    .mostrar(ui)
                    .map(|a| {
                        (
                            a,
                            os.id,
                            os.equipamento.clone(),
                            equipamento_label(os).to_owned(),
                        )
                    });
            });
        });

    if let Some((coluna, direcao)) = estado.ordenacao.clicar(&resposta) {
        ordenar_ordens(&mut estado.ordens, &estado.clientes, coluna, direcao);
    }
    if let Some((acao, id, equipamento, rotulo)) = acao_clicada {
        match acao {
            AcaoRegistro::Editar => {
                estado.editar_equipamento = equipamento;
                estado.editar_complemento_defeito.clear();
                estado.dlg = Dlg::EditarDados(id);
            }
            AcaoRegistro::Excluir => estado.confirmar_exclusao = Some((id, rotulo)),
        }
    } else if let Some(i) = resposta.linha_clicada {
        let id = estado.ordens[indices[i]].id;
        estado.abrir_detalhe(motor, sessao, id);
    }
}

/// Ordena `ordens` pela coluna clicada no cabeçalho da [`Grade`] (Nº, Aparelho, Cliente,
/// Estado, Total).
pub(super) fn ordenar_ordens(
    ordens: &mut [OrdemServico],
    clientes: &[ItemPessoa],
    coluna: usize,
    direcao: Direcao,
) {
    ordens.sort_by(|a, b| {
        let ordem = match coluna {
            0 => a.numero.cmp(&b.numero),
            1 => equipamento_label(a).cmp(equipamento_label(b)),
            2 => nome_cliente(clientes, a.cliente).cmp(nome_cliente(clientes, b.cliente)),
            3 => estado_etiqueta(a.estado).0.cmp(estado_etiqueta(b.estado).0),
            4 => a.valor_total.cmp(&b.valor_total),
            _ => std::cmp::Ordering::Equal,
        };
        match direcao {
            Direcao::Ascendente => ordem,
            Direcao::Descendente => ordem.reverse(),
        }
    });
}
