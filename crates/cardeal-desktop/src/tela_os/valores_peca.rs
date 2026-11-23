//! "Valores" de uma peça da OS: o custo previsto e o preço ao cliente, definidos depois — a
//! peça entra sem preço quando ainda não se sabe quanto custa. Mesma regra do formulário de
//! inclusão (`orcamento::valores_da_peca`): só um preenchido vale para os dois.

use super::*;

/// O formulário.
pub(super) struct FormValoresPeca {
    item: Id,
    produto: Id,
    nome: String,
    /// O preço já veio definido ou o usuário o digitou: mudar o custo não o recalcula.
    preco_manual: bool,
    dica: String,
    aplicada: bool,
    custo: String,
    preco: String,
}

impl FormValoresPeca {
    pub(super) fn de_item(item: &mod_os::ItemPeca, nome: String) -> Self {
        let texto = |p: Preco| {
            if p.e_zero() {
                String::new()
            } else {
                p.formatar()
            }
        };
        Self {
            item: item.id,
            produto: item.produto,
            nome,
            preco_manual: !item.preco_unitario.e_zero(),
            dica: String::new(),
            aplicada: item.aplicada,
            custo: texto(item.custo_unitario),
            preco: texto(item.preco_unitario),
        }
    }
}

/// O dialog. Fecha voltando ao detalhe.
pub(super) fn dialogo_valores_peca(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaOs,
) {
    let Some(os) = estado.detalhe.as_ref().map(|d| d.ordem.id) else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let Dlg::ValoresPeca(f) = &estado.dlg else {
        return;
    };
    let titulo = format!("Valores · {}", f.nome);
    let mut salvar = false;
    let mut cancelar = false;
    let fechar = Dialogo::nova(titulo)
        .descricao("Por unidade. Só um preenchido vale para os dois (repasse a preço de custo).")
        .pequeno()
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::ValoresPeca(f) = &mut estado.dlg else {
                    return;
                };
                let (mut custo_mudou, mut preco_mudou) = (false, false);
                ui.columns(2, |c| {
                    if f.aplicada {
                        Dado::novo("Meu custo", f.custo.clone()).mostrar(&mut c[0]);
                    } else {
                        custo_mudou = c[0]
                            .add(Campo::novo("Meu custo", &mut f.custo).marcador("opcional"))
                            .changed();
                    }
                    preco_mudou = c[1]
                        .add(Campo::novo("Preço ao cliente", &mut f.preco).marcador("= custo"))
                        .changed();
                });
                if preco_mudou {
                    f.preco_manual = !f.preco.trim().is_empty();
                }
                if custo_mudou {
                    sugerir_pelo_custo(motor, sessao, f);
                }
                if !f.dica.is_empty() {
                    ui.add(Rotulo::apoio(f.dica.clone()).quebravel());
                }
            },
            |ui, _| {
                salvar = ui.add(Botao::primario("Salvar")).clicked();
                cancelar = ui.add(Botao::fantasma("Cancelar")).clicked();
            },
        );
    if fechar || cancelar {
        estado.dlg = Dlg::Detalhe;
        return;
    }
    if !salvar {
        return;
    }
    let Dlg::ValoresPeca(f) = &estado.dlg else {
        return;
    };
    let custo_txt = if f.aplicada { "" } else { f.custo.as_str() };
    let Some((custo, preco)) = valores_da_peca(custo_txt, &f.preco) else {
        notificar(
            ctx,
            Notificacao::aviso("Custo ou preço inválidos (ex.: 120,00)."),
        );
        return;
    };
    // Na peça aplicada, só o preço: o custo é o real, do estoque.
    let preco = if f.aplicada && f.preco.trim().is_empty() {
        Preco::ZERO
    } else {
        preco
    };
    let cmd = mod_os::DefinirPrecoDaPeca {
        ordem_servico: os,
        item_peca: f.item,
        preco_unitario: preco,
        custo_previsto: (!f.aplicada).then_some(custo),
    };
    aplicar_e_recarregar(
        ctx,
        motor,
        sessao,
        estado,
        os,
        "os.definir_preco_da_peca.v1",
        &cmd,
        "Valores da peça atualizados",
    );
}

/// O custo previsto mudou: o preço acompanha pelo markup do produto, a não ser que já tenha
/// sido definido ou digitado.
pub(super) fn sugerir_pelo_custo(motor: &Motor, sessao: &Sessao, f: &mut FormValoresPeca) {
    if f.preco_manual {
        return;
    }
    let Ok(custo) = f.custo.trim().parse::<Dinheiro>() else {
        return;
    };
    if !custo.e_positivo() {
        return;
    }
    let sugestao: Option<mod_estoque::SugestaoDePreco> = motor
        .consultar(
            sessao,
            "estoque.preco_sugerido_do_produto.v1",
            &mod_estoque::PrecoSugeridoDoProduto {
                produto: f.produto,
                custo: Some(custo),
            },
        )
        .ok();
    match sugestao.as_ref().and_then(dica_do_markup) {
        Some((preco, dica)) => {
            f.preco = preco.formatar();
            f.dica = dica;
        }
        None => {
            f.dica = sugestao
                .and_then(|s| s.problema)
                .map(|p| format!("Sem preço sugerido: {p}"))
                .unwrap_or_default();
        }
    }
}
