//! "Dicionário de nomes…": o DE/PARA que padroniza o nome dos produtos criados a partir da nota
//! (`mod_estoque::padronizacao`). Lista os termos (os aprendidos com as correções e os
//! digitados), acrescenta, remove e testa uma descrição de fornecedor contra o dicionário.

use super::*;
use mod_estoque::padronizacao::TermoDicionario;
use mod_estoque::{
    DefinirTermoDoDicionario, DicionarioDeNomes, RemoverTermoDoDicionario, SugerirNomeDoProduto,
};

/// O estado do diálogo.
#[derive(Default)]
pub(super) struct EstadoDicionario {
    pub(super) termos: Vec<TermoDicionario>,
    pub(super) de: String,
    pub(super) para: String,
    pub(super) teste: String,
    pub(super) resultado_teste: Option<String>,
}

impl EstadoDicionario {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(t) = motor.consultar(sessao, "estoque.dicionario_de_nomes.v1", &DicionarioDeNomes)
        {
            self.termos = t;
        }
    }
}

/// Grava o termo do formulário.
pub(super) fn adicionar(
    motor: &Motor,
    sessao: &Sessao,
    d: &mut EstadoDicionario,
) -> Result<(), String> {
    motor
        .executar(
            sessao,
            "estoque.definir_termo_do_dicionario.v1",
            &DefinirTermoDoDicionario {
                de: d.de.clone(),
                para: d.para.clone(),
            },
        )
        .map_err(|e| e.mensagem)?;
    d.de.clear();
    d.para.clear();
    d.carregar(motor, sessao);
    Ok(())
}

/// O nome que a descrição de teste viraria hoje.
pub(super) fn testar(motor: &Motor, sessao: &Sessao, d: &mut EstadoDicionario) {
    d.resultado_teste = motor
        .consultar(
            sessao,
            "estoque.sugerir_nome_do_produto.v1",
            &SugerirNomeDoProduto {
                descricao: d.teste.clone(),
            },
        )
        .ok();
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    let fechar = Dialogo::nova("Dicionário de nomes").largura(640.0).mostrar(
        ctx,
        estado,
        |ui, estado| corpo(ui, motor, sessao, estado),
        |ui, estado| {
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn corpo(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    let d = &mut estado.dic;
    ui.add(
        Rotulo::apoio(
            "Quando um produto é criado a partir da nota, o nome sugerido troca cada palavra da \
             descrição do fornecedor pelo que está aqui (REFRIG → Refrigerante). Deixe o \"Para\" \
             vazio para tirar a palavra do nome. Ao corrigir um nome sugerido, o sistema aprende \
             sozinho os pares que dá para explicar.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E12);
    ui.columns(2, |c| {
        c[0].add(Campo::novo("De (como vem na nota)", &mut d.de).marcador("ex.: REFRIG"));
        c[1].add(
            Campo::novo("Para (como fica no nome)", &mut d.para).marcador("ex.: Refrigerante"),
        );
    });
    ui.add_space(Espaco::E4);
    if ui
        .add_enabled(!d.de.trim().is_empty(), Botao::secundario("Adicionar"))
        .clicked()
    {
        if let Err(e) = adicionar(motor, sessao, d) {
            notificar(ui.ctx(), Notificacao::erro(e));
        }
    }
    ui.add_space(Espaco::E12);

    let mut remover: Option<String> = None;
    Grade::nova(vec![
        ColunaGrade::nova("De").largura(180.0),
        ColunaGrade::nova("Para"),
        ColunaGrade::nova("").largura(90.0),
    ])
    .id_salt("dicionario-termos")
    .vazio("Nenhum termo ainda: eles aparecem conforme você corrige nomes sugeridos.")
    .mostrar(ui, d.termos.len(), |i, row| {
        let t = &d.termos[i];
        row.col(|ui| {
            ui.add(Rotulo::campo(t.de.clone()));
        });
        row.col(|ui| {
            if t.para.is_empty() {
                ui.add(Etiqueta::neutra("some do nome"));
            } else {
                ui.add(Rotulo::interface(t.para.clone()));
            }
        });
        row.col(|ui| {
            if ui.add(Botao::fantasma("Remover").pequeno()).clicked() {
                remover = Some(t.de.clone());
            }
        });
    });
    if let Some(de) = remover {
        match motor.executar(
            sessao,
            "estoque.remover_termo_do_dicionario.v1",
            &RemoverTermoDoDicionario { de },
        ) {
            Ok(()) => d.carregar(motor, sessao),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::titulo_secao("Testar"));
    ui.horizontal(|ui| {
        ui.add(
            Campo::novo("Descrição do fornecedor", &mut d.teste)
                .marcador("ex.: REFRIG COLA PET 2000ML"),
        );
    });
    ui.add_space(Espaco::E4);
    if ui.add(Botao::secundario("Ver o nome sugerido")).clicked() {
        testar(motor, sessao, d);
    }
    if let Some(r) = &d.resultado_teste {
        ui.add(Rotulo::interface(format!("→ {r}")));
    }
}
