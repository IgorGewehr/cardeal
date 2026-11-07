//! O dialog de detalhe da OS (amplo, centrado): à esquerda o contexto — cliente, aparelho,
//! andamento, tempo — e à direita o trabalho — laudo, orçamento, financeiro — com a ação certa
//! para o estado no rodapé.

use super::*;

pub(super) fn dialogo_detalhe(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
) {
    let Some(detalhe) = estado.detalhe.clone() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let os = detalhe.ordem.clone();
    let titulo = format!("OS #{} · {}", os.numero, equipamento_label(&os));
    let descricao = format!(
        "{} · {}",
        nome_cliente(&estado.clientes, os.cliente),
        situacao_amigavel(os.estado)
    );
    let (selo, tom) = estado_etiqueta(os.estado);
    let proximo = proximo_passo(&detalhe);

    // Dialog centrado, deitado e de altura fixa (pedido do usuário, 2026-11-07: a gaveta
    // lateral apertava tudo numa coluna estreita). À esquerda o contexto, à direita o trabalho.
    let fechar = Dialogo::nova(titulo)
        .descricao(descricao)
        .selo(selo, tom)
        .amplo()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ColunasDialogo::nova("os-detalhe").mostrar(
                    ui,
                    estado,
                    |ui, estado| coluna_contexto(ui, motor, sessao, estado, &detalhe),
                    |ui, estado| corpo_detalhe(ui, motor, sessao, estado, &detalhe),
                );
            },
            |ui, estado| {
                // Um botão principal só: o próximo passo do fluxo (o rodapé desenha da
                // direita para a esquerda, então ele fica no canto). Faturar continua
                // disponível desde a abertura (pedido do usuário, 2026-09-15), mas como
                // secundário quando não é o passo natural.
                if let Some(passo) = &proximo {
                    if ui.add(Botao::primario(passo.rotulo())).clicked() {
                        executar_proximo(ui.ctx(), motor, sessao, estado, os.id, passo.clone());
                    }
                }
                if pode_faturar(os.estado)
                    && proximo != Some(ProximoPasso::Faturar)
                    && ui.add(Botao::secundario("Faturar")).clicked()
                {
                    estado.dlg = Dlg::faturar();
                }
                if ui.add(Botao::secundario("Relatório (PDF)")).clicked() {
                    let f = FormDocumento::novo(true, estado.ultimo_documento.as_deref());
                    estado.dlg = Dlg::Documento(Box::new(f));
                }
                if ui.add(Botao::secundario("Orçamento (PDF)")).clicked() {
                    let f = FormDocumento::novo(false, estado.ultimo_documento.as_deref());
                    estado.dlg = Dlg::Documento(Box::new(f));
                }
                if ui.add(Botao::fantasma("Fechar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

/// A coluna da esquerda: quem é o cliente, o aparelho, em que pé a OS está, o tempo de
/// bancada e as ações que mudam o estado fora do fluxo (cancelar, reabrir, desfaturar).
fn coluna_contexto(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    secao_resumo(ui, estado, detalhe);
    secao_andamento(ui, motor, sessao, estado, detalhe);
    secao_apontamento(ui, motor, sessao, estado, os.id);
    secao_garantia(ui, motor, sessao, estado, detalhe);

    let cancelar = pode_cancelar(os.estado);
    let desfaturar = os.estado == EstadoOs::Faturada;
    let reabrir = os.estado == EstadoOs::Cancelada;
    if cancelar || desfaturar || reabrir {
        Painel::novo()
            .plano()
            .compacto()
            .titulo("Outras ações")
            .mostrar(ui, |ui| {
                if cancelar
                    && ui
                        .add(Botao::destrutivo("Cancelar OS").preenche_largura())
                        .clicked()
                {
                    estado.confirmar_exclusao = Some((os.id, equipamento_label(os).to_owned()));
                }
                if desfaturar
                    && ui
                        .add(Botao::secundario("Desfaturar").preenche_largura())
                        .clicked()
                {
                    estado.confirmar_acao = Some(AcaoPendente::Desfaturar(os.id));
                }
                if reabrir
                    && ui
                        .add(Botao::secundario("Reabrir OS").preenche_largura())
                        .clicked()
                {
                    estado.confirmar_acao = Some(AcaoPendente::Reabrir(os.id));
                }
            });
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

/// A coluna principal: o trabalho na OS, cada assunto no seu bloco.
pub(super) fn corpo_detalhe(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    // O que o próximo passo pede antes (quem aprovou, de onde saem as peças) vem primeiro:
    // é o que o balcão precisa resolver agora.
    acoes_por_estado(ui, estado, detalhe);
    // A ordem da bancada: o que o cliente disse e o que eu achei → as peças (já encomendei?)
    // → os serviços → quanto estou ganhando nisso.
    secao_laudo(ui, motor, sessao, estado, detalhe);
    ui.add_space(Espaco::E16);
    secao_pecas(ui, motor, sessao, estado, detalhe);
    ui.add_space(Espaco::E16);
    secao_servicos(ui, motor, sessao, estado, detalhe);
    ui.add_space(Espaco::E16);
    secao_resultado(ui, sessao, estado, detalhe);

    if os.estado == EstadoOs::Faturada {
        ui.add_space(Espaco::E16);
        Painel::novo()
            .plano()
            .titulo("Financeiro")
            .mostrar(ui, |ui| {
                match &estado.titulo_gerado {
                    Some(titulo) => {
                        ui.horizontal(|ui| {
                            ui.add(Etiqueta::positiva("título gerado"));
                            ui.add(Rotulo::interface(format!(
                                "A receber nº {}",
                                titulo.id.curto()
                            )));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| ui.add(ValorDinheiro::novo(titulo.valor_original)),
                            );
                        });
                    }
                    None => {
                        ui.add(
                            Rotulo::interface(
                                "Faturada sem cobrança (garantia/cortesia) — nenhum título gerado.",
                            )
                            .quebravel(),
                        );
                    }
                }
                ui.add_space(Espaco::E16);
                super::nfse::secao(ui, motor, sessao, &mut estado.nfse, os.id);
            });
    }
}

pub(super) fn acoes_por_estado(
    ui: &mut egui::Ui,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    // Só o que o próximo passo do rodapé precisa saber antes (quem aprovou, de que local as
    // peças saem) e as saídas alternativas (reprovar). A ação principal fica no rodapé.
    let os = &detalhe.ordem;
    match os.estado {
        EstadoOs::AguardandoAprovacao => {
            Painel::novo()
                .realce(Tom::Atencao)
                .titulo("Aprovação do cliente")
                .mostrar(ui, |ui| {
                    ui.add(
                        Rotulo::apoio(
                            "Anote quem aprovou e use \"Aprovar orçamento\" no rodapé. Se o \
                             cliente recusou, registre a reprovação.",
                        )
                        .quebravel(),
                    );
                    ui.add_space(Espaco::E8);
                    ui.columns(2, |c| {
                        c[0].add(
                            Campo::novo("Quem aprovou (nome e documento)", &mut estado.aprovador)
                                .marcador("Ex.: Maria Souza, CPF 123…"),
                        );
                        c[1].add_space(Espaco::E24);
                        if c[1].add(Botao::destrutivo("Cliente reprovou")).clicked() {
                            estado.confirmar_acao = Some(AcaoPendente::ReprovarOrcamento(os.id));
                        }
                    });
                });
            ui.add_space(Espaco::E16);
        }
        EstadoOs::EmExecucao => {
            let n = detalhe.itens_peca.iter().filter(|i| !i.aplicada).count();
            if n > 0 {
                Painel::novo()
                    .realce(Tom::Info)
                    .titulo("Peças a baixar")
                    .mostrar(ui, |ui| {
                        ui.add(
                    Rotulo::interface(if n == 1 {
                        "1 peça ainda não saiu do estoque — sai sozinha ao concluir (ou use \
                         \"Aplicar\" na linha para baixar agora)."
                            .to_owned()
                    } else {
                        format!(
                            "{n} peças ainda não saíram do estoque — saem sozinhas ao concluir \
                             (ou use \"Aplicar\" na linha para baixar agora)."
                        )
                    })
                    .quebravel()
                    .cor(ui.cores().texto_medio),
                );
                        if estado.locais.len() > 1 {
                            ui.add_space(Espaco::E8);
                            let opcoes: Vec<(Id, String)> = estado
                                .locais
                                .iter()
                                .map(|l| (l.id, l.nome.clone()))
                                .collect();
                            SeletorOpcao::novo(
                                "Sai do local de estoque",
                                &mut estado.aplicar_local,
                            )
                            .opcoes(opcoes)
                            .mostrar(ui);
                        }
                    });
                ui.add_space(Espaco::E16);
            }
        }
        EstadoOs::Faturada
        | EstadoOs::Cancelada
        | EstadoOs::Reprovada
        | EstadoOs::Aberta
        | EstadoOs::EmDiagnostico
        | EstadoOs::Aprovada
        | EstadoOs::Concluida => {}
    }
}
