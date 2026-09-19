//! As visões da tela de venda: só desenham e empilham [`Acao`]s — não falam com o motor.

use super::*;

// ── visões (sem motor) ───────────────────────────────────────────────────────

/// A tela de venda, ou o aviso de que não dá para vender ainda (sem caixa / caixa fechado).
pub(super) fn desenhar(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    let pronto = !estado.caixas.is_empty() && estado.sessao_aberta().is_some();
    if pronto {
        LayoutTela::nova("PDV — Frente de caixa")
            .fracao_lista(0.64)
            .mostrar_com_detalhe(ui, estado, cabecalho, lista, detalhe);
    } else {
        LayoutTela::nova("PDV — Frente de caixa").mostrar(ui, estado, cabecalho, corpo_sem_caixa);
    }
}

/// Estado do caixa e número do cupom, à direita do cabeçalho.
pub(super) fn cabecalho(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    if estado.caixas.is_empty() {
        return;
    }
    let (texto, tom) = if estado.sessao_aberta().is_some() {
        ("Caixa aberto", Tom::Positivo)
    } else {
        ("Caixa fechado", Tom::Neutro)
    };
    ui.add(Etiqueta::nova(texto, tom).com_ponto());
    if let Some(n) = estado.cupom_numero {
        ui.add_space(Espaco::E8);
        ui.add(Etiqueta::info(format!("Cupom nº {n}")));
    }
}

pub(super) fn corpo_sem_caixa(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    if estado.caixas.is_empty() {
        if EstadoVazio::novo(
            Icone::Caixa,
            "Nenhum caixa cadastrado. Cadastre um para começar a vender.",
        )
        .acao("Cadastrar caixa")
        .mostrar(ui)
        {
            let conta = estado.contas_caixa.first().map(|c| c.conta);
            abrir(
                ui.ctx(),
                estado,
                Dlg::CadastrarCaixa {
                    nome: "Caixa 1".to_owned(),
                    conta,
                },
            );
        }
        return;
    }
    barra_terminal(ui, estado);
    ui.add_space(Espaco::E12);
    aviso_erro(ui, estado);
    resultado_venda(ui, estado);

    let nome = estado
        .caixa_atual()
        .map_or("—".to_owned(), |c| c.nome.clone());
    Painel::novo()
        .realce(Tom::Atencao)
        .titulo(format!("{nome} está fechado"))
        .mostrar(ui, |ui| {
            ui.add(Rotulo::campo(
                "Abra a sessão de caixa com o valor de troco inicial para começar a vender.",
            ));
            ui.add_space(Espaco::E16);
            if ui
                .add(Botao::primario("Abrir caixa").atalho("F12"))
                .clicked()
            {
                abrir(
                    ui.ctx(),
                    estado,
                    Dlg::AbrirCaixa {
                        valor: "0,00".to_owned(),
                    },
                );
            }
        });
}

/// Caixa, tabela de preço e local: definem o cupom ao abri-lo, então ficam travados durante
/// uma venda (trocá-los no meio não teria efeito nenhum — só confundiria).
pub(super) fn barra_terminal(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    let travado = estado.cupom.is_some();
    let mut caixa = estado.caixa_sel;
    let mut tabela = estado.tabela_sel;
    let mut local = estado.local_sel;
    ui.add_enabled_ui(!travado, |ui| {
        ui.columns(3, |c| {
            SeletorOpcao::novo("Caixa", &mut caixa)
                .opcoes(estado.caixas.iter().map(|x| (x.caixa, x.nome.clone())))
                .mostrar(&mut c[0]);
            SeletorOpcao::novo("Tabela de preço", &mut tabela)
                .opcoes(estado.tabelas.iter().map(|t| (t.id, t.nome.clone())))
                .mostrar(&mut c[1]);
            SeletorOpcao::novo("Local", &mut local)
                .opcoes(estado.locais.iter().map(|l| (l.id, l.nome.clone())))
                .mostrar(&mut c[2]);
        });
    });
    estado.caixa_sel = caixa;
    estado.tabela_sel = tabela;
    estado.local_sel = local;
}

pub(super) fn aviso_erro(ui: &mut egui::Ui, estado: &EstadoTelaPdv) {
    let Some(e) = &estado.erro else { return };
    Painel::novo()
        .realce(Tom::Negativo)
        .compacto()
        .mostrar(ui, |ui| {
            ui.add(
                Rotulo::interface(e.clone())
                    .quebravel()
                    .cor(ui.cores().negativo),
            );
        });
    ui.add_space(Espaco::E12);
}

/// O resultado da última venda: o troco é o que o operador precisa ver logo depois do `F2`.
pub(super) fn resultado_venda(ui: &mut egui::Ui, estado: &EstadoTelaPdv) {
    let Some(v) = estado.ultima_venda else { return };
    let positivo = ui.cores().positivo;
    Painel::novo().realce(Tom::Positivo).mostrar(ui, |ui| {
        ui.add(Rotulo::interface(format!("Venda nº {} finalizada", v.numero)).cor(positivo));
        ui.add_space(Espaco::E8);
        if v.troco > Dinheiro::ZERO {
            ui.add(Rotulo::campo("TROCO"));
            ui.add(Rotulo::novo(
                Papel::ValorDestaque,
                v.troco.formatar_com_simbolo(),
            ));
            ui.add_space(Espaco::E4);
            ui.add(Rotulo::campo(format!(
                "Total da venda {}",
                v.total.formatar_com_simbolo()
            )));
        } else {
            ui.add(Rotulo::campo("TOTAL"));
            ui.add(Rotulo::novo(
                Papel::ValorDestaque,
                v.total.formatar_com_simbolo(),
            ));
        }
    });
    ui.add_space(Espaco::E12);
}

/// Coluna esquerda: terminal, campo de bipe, resultados da busca e o cupom.
pub(super) fn lista(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    barra_terminal(ui, estado);
    ui.add_space(Espaco::E12);
    aviso_erro(ui, estado);
    campo_bipe(ui, estado);
    ui.add_space(Espaco::E8);
    dicas(ui);
    ui.add_space(Espaco::E12);
    estado.atualizar_resultados();
    if !estado.resultados.1.is_empty() {
        painel_resultados(ui, estado);
        ui.add_space(Espaco::E12);
    }
    itens(ui, estado);
}

/// O campo de bipe: sempre focado quando nada mais está (e nenhum diálogo aberto), para o
/// leitor de código de barras nunca "digitar no vazio".
pub(super) fn campo_bipe(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    let livre = matches!(estado.dlg, Dlg::Fechado);
    let ninguem_tem_foco = ui
        .ctx()
        .memory(|m| m.focused().is_none() && !m.any_popup_open());
    let pedir = livre && (std::mem::take(&mut estado.foco_entrada) || ninguem_tem_foco);

    let resp = ui.add(
        CampoBusca::novo(&mut estado.entrada)
            .marcador("Bipe o código de barras ou digite o nome do produto")
            .foco(pedir),
    );
    if resp.changed() {
        estado.sel_resultado = 0;
    }
    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        estado.pendentes.push(Acao::Bipar);
        estado.foco_entrada = true;
    }
}

/// As teclas que a tela de venda realmente trata, ensinadas no ponto de uso.
pub(super) fn dicas(ui: &mut egui::Ui) {
    ui.horizontal_wrapped(|ui| {
        for (tecla, texto) in [
            ("Enter", "adiciona"),
            ("↑↓", "escolhe"),
            ("3*", "quantidade (3*código)"),
            ("F3", "volta ao bipe"),
            ("Esc", "limpa"),
        ] {
            ui.add(Tecla::nova(tecla));
            ui.add(Rotulo::campo(texto));
            ui.add_space(Espaco::E8);
        }
    });
}

pub(super) fn painel_resultados(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    let linhas: Vec<(Id, String, String, bool)> = estado
        .resultados
        .1
        .iter()
        .filter_map(|&i| estado.produtos.get(i))
        .map(|p| {
            (
                p.produto,
                p.nome.clone(),
                format!("saldo {}", p.disponivel.formatar(0)),
                p.disponivel <= Quantidade::ZERO,
            )
        })
        .collect();
    let sel = estado.sel_resultado;
    Painel::novo()
        .plano()
        .compacto()
        .titulo("Resultados")
        .mostrar(ui, |ui| {
            for (k, (produto, nome, saldo, sem_saldo)) in linhas.iter().enumerate() {
                let resp = ItemDeLista::novo(nome.clone())
                    .subtitulo(saldo.clone())
                    .selecionado(k == sel)
                    .mostrar(ui, |ui| {
                        if *sem_saldo {
                            ui.add(Etiqueta::atencao("sem saldo"));
                        }
                    });
                if resp.clicked() {
                    estado.pendentes.push(Acao::Adicionar(*produto));
                }
            }
        });
}

/// O cupom em andamento, como tabela (`docs/modulos/pdv.md` §10).
pub(super) fn itens(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    if estado.carrinho.is_empty() {
        EstadoVazio::novo(
            Icone::Carrinho,
            "Cupom vazio. Bipe um produto ou digite o nome.",
        )
        .mostrar(ui);
        return;
    }
    let fraco = ui.cores().texto_fraco;
    let linhas = &estado.carrinho;
    let resp = Grade::nova(vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("Qtd").largura(80.0).numero(),
        ColunaGrade::nova("Unit.").largura(110.0).numero(),
        ColunaGrade::nova("Desc.").largura(90.0).numero(),
        ColunaGrade::nova("Total").largura(120.0).numero(),
    ])
    .id_salt("pdv-cupom")
    .selecionavel(estado.linha_sel)
    .mostrar(ui, linhas.len(), |i, linha| {
        let l = &linhas[i];
        let rotulo = |papel, texto: String| {
            let r = Rotulo::novo(papel, texto);
            if l.cancelado {
                r.cor(fraco)
            } else {
                r
            }
        };
        linha.col(|ui| {
            ui.add(rotulo(Papel::Interface, l.nome.clone()));
        });
        linha.col(|ui| {
            ui.add(rotulo(Papel::Numero, l.quantidade.formatar(0)));
        });
        linha.col(|ui| {
            ui.add(rotulo(Papel::Numero, l.preco.formatar_com_simbolo()));
        });
        linha.col(|ui| {
            if l.cancelado {
                ui.add(Etiqueta::negativa("cancelado"));
            } else if l.desconto == Percentual::ZERO {
                ui.add(Rotulo::campo("—"));
            } else {
                ui.add(Etiqueta::positiva(l.desconto.formatar_com_simbolo()));
            }
        });
        linha.col(|ui| {
            ui.add(rotulo(Papel::Numero, l.total.formatar_com_simbolo()));
        });
    });
    if let Some(i) = resp.linha_clicada {
        estado.linha_sel = Some(i);
    }
}

/// Coluna direita: o total, sempre visível, e as ações.
pub(super) fn detalhe(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    resultado_venda(ui, estado);

    let itens = estado.ativos().count();
    let descontos = estado
        .ativos()
        .map(|l| l.bruto() - l.total)
        .fold(Dinheiro::ZERO, |a, b| a + b);
    let total = estado.total;
    Painel::novo().mostrar(ui, |ui| {
        ui.add(Rotulo::campo("TOTAL"));
        ui.add_space(Espaco::E4);
        ui.add(Rotulo::novo(
            Papel::ValorDestaque,
            total.formatar_com_simbolo(),
        ));
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            let rotulo = if itens == 1 { "item" } else { "itens" };
            ui.add(Rotulo::campo(format!("{itens} {rotulo}")));
            if descontos > Dinheiro::ZERO {
                ui.add_space(Espaco::E8);
                ui.add(Etiqueta::positiva(format!(
                    "descontos {}",
                    descontos.formatar_com_simbolo()
                )));
            }
        });
    });
    ui.add_space(Espaco::E12);
    acoes(ui, estado);
}

pub(super) fn acoes(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    let tem_itens = estado.ativos().count() > 0;
    let tem_selecao = estado.linha_ativa_sel().is_some();
    let tem_cupom = estado.cupom.is_some();

    if ui
        .add(
            Botao::primario("Finalizar venda")
                .atalho("F2")
                .preenche_largura()
                .habilitado(tem_itens),
        )
        .clicked()
    {
        estado.pendentes.push(Acao::AbrirPagamento);
    }
    ui.add_space(Espaco::E8);
    ui.columns(2, |c| {
        if c[0]
            .add(
                Botao::secundario("Desconto")
                    .atalho("F5")
                    .preenche_largura()
                    .habilitado(tem_selecao),
            )
            .clicked()
        {
            estado.pendentes.push(Acao::AbrirDesconto);
        }
        if c[1]
            .add(
                Botao::secundario("Cancelar item")
                    .atalho("F7")
                    .preenche_largura()
                    .habilitado(tem_selecao),
            )
            .clicked()
        {
            estado.pendentes.push(Acao::CancelarLinha);
        }
    });
    ui.add_space(Espaco::E8);
    if ui
        .add(
            Botao::destrutivo("Cancelar cupom")
                .atalho("F8")
                .preenche_largura()
                .habilitado(tem_cupom),
        )
        .clicked()
    {
        estado.pendentes.push(Acao::PedirCancelarCupom);
    }
}
