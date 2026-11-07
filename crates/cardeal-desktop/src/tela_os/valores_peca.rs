//! "Valores" de uma peça da OS: o custo previsto e o preço ao cliente, definidos depois — a
//! peça entra sem preço quando ainda não se sabe quanto custa. Mesma regra do formulário de
//! inclusão (`orcamento::valores_da_peca`): só um preenchido vale para os dois.

use super::*;

/// O formulário.
pub(super) struct FormValoresPeca {
    item: Id,
    nome: String,
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
            nome,
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
                ui.columns(2, |c| {
                    if f.aplicada {
                        Dado::novo("Meu custo", f.custo.clone()).mostrar(&mut c[0]);
                    } else {
                        c[0].add(Campo::novo("Meu custo", &mut f.custo).marcador("opcional"));
                    }
                    c[1].add(Campo::novo("Preço ao cliente", &mut f.preco).marcador("= custo"));
                });
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
