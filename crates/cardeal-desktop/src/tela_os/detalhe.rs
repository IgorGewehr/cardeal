//! O diálogo de detalhe da OS: laudo, orçamento, financeiro e a ação certa para o estado.

use super::*;

pub(super) fn dialogo_detalhe(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Some(detalhe) = estado.detalhe.clone() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let os = detalhe.ordem.clone();
    let titulo = format!("OS #{} · {}", os.numero, equipamento_label(&os));

    let fechar = Dialogo::nova(titulo).mostrar(
        ctx,
        estado,
        |ui, estado| corpo_detalhe(ui, motor, sessao, estado, &detalhe),
        |ui, estado| {
            // A ponta final do fluxo do técnico — laudo/orçamento/execução terminam aqui: o
            // comprovante em PDF pra entregar/mandar ao cliente, disponível em qualquer
            // estado (`gerar_pdf` já cobre da entrada à quitação). Ação de peso, então
            // primário — igual à ação de workflow do estado atual, mas em outra zona da tela
            // (rodapé fixo, sempre visível, em vez de escondida no fim do corpo rolável). O
            // rodapé desenha da direita para a esquerda (`Dialogo::mostrar`), então o botão
            // adicionado primeiro fica mais à direita — a mesma convenção de "ação primária
            // primeiro" já usada no resto da tela.
            // Faturar disponível desde a abertura, ao lado de Comprovante/Fechar — pedido
            // explícito do usuário (2026-09-15): laudo/orçamento/aprovação/execução são
            // trâmite opcional, não um portão pro faturamento. Some só nos estados terminais
            // onde faturar já não faz sentido (`OrdemServico::faturar` recusaria de qualquer
            // forma; a UI só evita oferecer uma ação que o backend já vai rejeitar).
            if pode_faturar(os.estado) && ui.add(Botao::primario("Faturar")).clicked() {
                estado.dlg = Dlg::faturar();
            }
            if ui.add(Botao::primario("Comprovante (PDF)")).clicked() {
                gerar_pdf(ui.ctx(), estado, &detalhe);
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

/// Se `FaturarOrdemServico` aceita a OS neste estado — qualquer um menos os três terminais
/// (`OrdemServico::faturar`, 2026-09-15).
pub(super) const fn pode_faturar(estado: EstadoOs) -> bool {
    !matches!(
        estado,
        EstadoOs::Faturada | EstadoOs::Cancelada | EstadoOs::Reprovada
    )
}

pub(super) fn corpo_detalhe(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Estado"));
        let (rotulo, tom) = estado_etiqueta(os.estado);
        ui.add(Etiqueta::nova(rotulo, tom));
        if pode_cancelar(os.estado) && ui.add(Botao::destrutivo("Cancelar OS").pequeno()).clicked()
        {
            estado.confirmar_exclusao = Some((os.id, equipamento_label(os).to_owned()));
        }
        if os.estado == EstadoOs::Faturada
            && ui.add(Botao::secundario("Desfaturar").pequeno()).clicked()
        {
            estado.confirmar_acao = Some(AcaoPendente::Desfaturar(os.id));
        }
        if os.estado == EstadoOs::Cancelada
            && ui.add(Botao::secundario("Reabrir OS").pequeno()).clicked()
        {
            estado.confirmar_acao = Some(AcaoPendente::Reabrir(os.id));
        }
    });
    ui.add_space(Espaco::E16);

    // ── Laudo ──────────────────────────────────────────────────────────────
    ui.add(Rotulo::titulo_secao("Laudo técnico"));
    ui.add_space(Espaco::E8);
    if matches!(os.estado, EstadoOs::Aberta | EstadoOs::EmDiagnostico) {
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Problema relatado", &mut estado.laudo_problema));
            c[1].add(Campo::novo(
                "Diagnóstico (opcional)",
                &mut estado.laudo_diagnostico,
            ));
        });
        ui.add_space(Espaco::E8);
        let ja_tem_laudo = detalhe.laudo.is_some();
        let rotulo_laudo = if ja_tem_laudo {
            "Atualizar laudo"
        } else {
            "Registrar laudo"
        };
        if ui.add(Botao::primario(rotulo_laudo)).clicked() {
            let diagnostico = (!estado.laudo_diagnostico.trim().is_empty())
                .then(|| estado.laudo_diagnostico.clone());
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
    } else if let Some(laudo) = &detalhe.laudo {
        ui.add(Rotulo::interface(format!("Problema: {}", laudo.descricao_problema)).quebravel());
        if let Some(dg) = &laudo.diagnostico {
            ui.add(Rotulo::interface(format!("Diagnóstico: {dg}")).quebravel());
        }
    } else {
        ui.add(Rotulo::interface("—"));
    }
    ui.add_space(Espaco::E16);

    // ── Orçamento ──────────────────────────────────────────────────────────
    ui.add(Rotulo::titulo_secao("Orçamento"));
    ui.add_space(Espaco::E8);
    // Remover/aplicar são decididos aqui e executados depois dos laços (os laços seguram
    // `estado` emprestado para ler os nomes das peças).
    let pode_ajustar = os.estado.aceita_ajuste_de_itens();
    let em_execucao = os.estado == EstadoOs::EmExecucao;
    let mut remover: Option<(Id, TipoItemOrcamento)> = None;
    let mut aplicar: Option<Id> = None;
    for item in &detalhe.itens_mao_de_obra {
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(item.descricao.clone()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if pode_ajustar && ui.add(Botao::fantasma("Remover").pequeno()).clicked() {
                    remover = Some((item.id, TipoItemOrcamento::MaoDeObra));
                }
                ui.add(ValorDinheiro::novo(item.valor));
            });
        });
    }
    for item in &detalhe.itens_peca {
        let nome = estado
            .produtos
            .iter()
            .find(|p| p.produto == item.produto)
            .map_or_else(|| "Peça do estoque".to_owned(), |p| p.nome.clone());
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(format!("{} × {nome}", item.quantidade)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Uma peça já aplicada não sai do orçamento: o estoque já foi baixado.
                if !item.aplicada {
                    if em_execucao && ui.add(Botao::secundario("Aplicar").pequeno()).clicked() {
                        aplicar = Some(item.id);
                    }
                    if pode_ajustar && ui.add(Botao::fantasma("Remover").pequeno()).clicked() {
                        remover = Some((item.id, TipoItemOrcamento::Peca));
                    }
                }
                ui.add(Rotulo::interface(format!(
                    "{} /un",
                    item.preco_unitario.formatar_com_simbolo()
                )));
                ui.add(if item.aplicada {
                    Etiqueta::positiva("aplicada")
                } else {
                    Etiqueta::atencao("pendente")
                });
            });
        });
    }
    if let Some((item, tipo)) = remover {
        aplicar_e_recarregar(
            ui.ctx(),
            motor,
            sessao,
            estado,
            os.id,
            "os.remover_item_orcamento.v1",
            &RemoverItemOrcamento {
                ordem_servico: os.id,
                item,
                tipo,
            },
            "Item removido do orçamento",
        );
    }
    if let Some(item) = aplicar {
        aplicar_pecas(ui.ctx(), motor, sessao, estado, os.id, &[item]);
    }
    ui.add_space(Espaco::E4);
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Total"));
        ui.add(ValorDinheiro::novo(os.valor_total));
    });

    if os.estado.aceita_edicao_de_orcamento() {
        ui.add_space(Espaco::E12);
        ui.add(Rotulo::campo("Mão de obra"));
        ui.add_space(Espaco::E4);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Serviço", &mut estado.mao_de_obra_descricao));
            c[1].add(Campo::novo("Valor", &mut estado.mao_de_obra_valor).marcador("80,00"));
        });
        ui.add_space(Espaco::E4);
        if ui
            .add(Botao::secundario("+ Adicionar mão de obra"))
            .clicked()
        {
            adicionar_mao_de_obra(ui.ctx(), motor, sessao, estado, os.id);
        }

        ui.add_space(Espaco::E12);
        // TODO(backend): a busca aqui compara só o nome do produto — quando o código curto de
        // rastreabilidade (post-it físico na peça) existir em `ItemProdutoComSaldo`, somar
        // `p.codigo_curto` ao `subtitulo` abaixo é o suficiente para o técnico bipar/digitar o
        // código de bancada em vez de catar o nome numa lista de centenas de peças.
        let opcoes_peca: Vec<OpcaoBusca<Id>> = estado
            .produtos
            .iter()
            .map(|p| {
                OpcaoBusca::nova(p.produto, p.nome.clone())
                    .subtitulo(format!("{} disponível(is)", p.disponivel))
            })
            .collect();
        SeletorBusca::novo(
            "Peça do estoque",
            &mut estado.peca_busca,
            &mut estado.peca_produto,
        )
        .opcoes(opcoes_peca)
        .marcador("Buscar peça por nome…")
        .mostrar(ui);
        ui.add_space(Espaco::E4);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Quantidade", &mut estado.peca_qtd).marcador("1"));
            c[1].add(Campo::novo("Preço unitário", &mut estado.peca_preco).marcador("0,00"));
        });
        ui.add_space(Espaco::E4);
        if ui.add(Botao::secundario("+ Adicionar peça")).clicked() {
            adicionar_peca(ui.ctx(), motor, sessao, estado, os.id);
        }

        ui.add_space(Espaco::E12);
        ui.horizontal(|ui| {
            if !os.valor_total.e_zero()
                && ui.add(Botao::primario("Enviar para aprovação")).clicked()
            {
                aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.enviar_para_aprovacao.v1",
                    &EnviarParaAprovacao {
                        ordem_servico: os.id,
                    },
                    "Orçamento enviado para aprovação",
                );
            }
        });
    }

    ui.add_space(Espaco::E16);
    secao_apontamento(ui, motor, sessao, estado, os.id);

    if os.estado == EstadoOs::Faturada {
        ui.add_space(Espaco::E16);
        ui.add(Rotulo::titulo_secao("Financeiro"));
        ui.add_space(Espaco::E8);
        match &estado.titulo_gerado {
            Some(titulo) => {
                ui.add(Rotulo::interface(format!(
                    "Título a receber gerado: {}",
                    titulo.id.curto()
                )));
                ui.add(ValorDinheiro::novo(titulo.valor_original));
            }
            None => {
                ui.add(Rotulo::interface(
                    "Faturada sem cobrança (garantia/cortesia) — nenhum título gerado.",
                ));
            }
        }
    }

    ui.add_space(Espaco::E16);
    acoes_por_estado(ui, motor, sessao, estado, detalhe);
}

pub(super) fn acoes_por_estado(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    match os.estado {
        EstadoOs::AguardandoAprovacao => {
            ui.add(Rotulo::titulo_secao("Aprovação do cliente"));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo(
                "Quem aprovou (nome e documento)",
                &mut estado.aprovador,
            ));
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                if ui.add(Botao::primario("Aprovar")).clicked() {
                    if estado.aprovador.trim().is_empty() {
                        notificar(
                            ui.ctx(),
                            Notificacao::aviso("Informe quem aprovou (nome e documento)."),
                        );
                    } else {
                        estado.confirmar_acao = Some(AcaoPendente::AprovarOrcamento(os.id));
                    }
                }
                if ui.add(Botao::destrutivo("Reprovar")).clicked() {
                    estado.confirmar_acao = Some(AcaoPendente::ReprovarOrcamento(os.id));
                }
            });
        }
        EstadoOs::Aprovada => {
            if ui.add(Botao::primario("Iniciar execução")).clicked() {
                aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.iniciar_execucao.v1",
                    &IniciarExecucao {
                        ordem_servico: os.id,
                    },
                    "Execução iniciada",
                );
            }
        }
        EstadoOs::EmExecucao => {
            let pendentes: Vec<Id> = detalhe
                .itens_peca
                .iter()
                .filter(|i| !i.aplicada)
                .map(|i| i.id)
                .collect();
            if !pendentes.is_empty() {
                let n = pendentes.len();
                ui.add(
                    Rotulo::interface(if n == 1 {
                        "1 peça do orçamento ainda não saiu do estoque.".to_owned()
                    } else {
                        format!("{n} peças do orçamento ainda não saíram do estoque.")
                    })
                    .cor(ui.cores().atencao),
                );
                ui.add_space(Espaco::E8);
                if estado.locais.len() > 1 {
                    let opcoes: Vec<(Id, String)> = estado
                        .locais
                        .iter()
                        .map(|l| (l.id, l.nome.clone()))
                        .collect();
                    SeletorOpcao::novo("Sai do local de estoque", &mut estado.aplicar_local)
                        .opcoes(opcoes)
                        .mostrar(ui);
                    ui.add_space(Espaco::E8);
                }
                let rotulo = if n == 1 {
                    "Aplicar peça"
                } else {
                    "Aplicar todas as peças"
                };
                if ui.add(Botao::primario(rotulo)).clicked() {
                    aplicar_pecas(ui.ctx(), motor, sessao, estado, os.id, &pendentes);
                }
            } else if ui.add(Botao::primario("Concluir execução")).clicked() {
                estado.confirmar_acao = Some(AcaoPendente::ConcluirExecucao(os.id));
            }
        }
        EstadoOs::Faturada
        | EstadoOs::Cancelada
        | EstadoOs::Reprovada
        | EstadoOs::Aberta
        | EstadoOs::EmDiagnostico
        | EstadoOs::Concluida => {}
    }
}
