//! "Tributação a conferir" (aba Preços): o que o sistema aprendeu sozinho das notas de compra
//! ou percebeu de inconsistente no perfil fiscal — cada linha com "Está certo" (fica) e
//! "Desfazer" (volta o perfil de antes).

use super::*;
use mod_estoque::{
    AlvoTributacao, ConfirmarTributacaoAConferir, DesfazerTributacaoAConferir,
    ListarTributacaoAConferir, TributacaoAConferir,
};

/// Carrega a lista.
pub(super) fn carregar(motor: &Motor, sessao: &Sessao) -> Vec<TributacaoAConferir> {
    motor
        .consultar(
            sessao,
            "estoque.tributacao_a_conferir.v1",
            &ListarTributacaoAConferir,
        )
        .unwrap_or_default()
}

/// Uma ação pedida numa linha.
enum Acao {
    Confirmar(Id),
    Desfazer(Id),
}

/// A seção (some quando a lista está vazia).
pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    if estado.a_conferir.is_empty() {
        return;
    }
    let titulo = format!("Tributação a conferir ({})", estado.a_conferir.len());
    let mut acao = None;
    SecaoExpansivel::nova(titulo).mostrar(ui, |ui| {
        ui.add(
            Rotulo::apoio(
                "O que o sistema aprendeu das notas de compra (ou achou inconsistente). Nada \
                 aqui muda a nota emitida: confira e confirme, ou desfaça.",
            )
            .quebravel(),
        );
        ui.add_space(Espaco::E8);
        let linhas = &estado.a_conferir;
        Grade::nova(vec![
            ColunaGrade::nova("Produto ou categoria").largura(220.0),
            ColunaGrade::nova("O que aconteceu").largura(420.0),
            ColunaGrade::nova("De onde veio").largura(160.0),
            ColunaGrade::nova("").largura(220.0),
        ])
        .id_salt("tributacao-a-conferir")
        .mostrar(ui, linhas.len(), |i, row| {
            let l = &linhas[i];
            row.col(|ui| {
                let tipo = match l.alvo {
                    AlvoTributacao::Produto(_) => "",
                    AlvoTributacao::Grupo(_) => " (categoria)",
                };
                ui.add(Rotulo::interface(format!("{}{tipo}", l.nome)));
            });
            row.col(|ui| {
                ui.add(Rotulo::apoio(l.motivo.clone()).quebravel());
            });
            row.col(|ui| {
                ui.add(Rotulo::apoio(l.origem.clone()));
            });
            row.col(|ui| {
                // Os conflitos calculados (sem registro) somem quando o perfil é corrigido.
                if let Some(id) = l.id {
                    ui.horizontal(|ui| {
                        if ui.add(Botao::secundario("Está certo").pequeno()).clicked() {
                            acao = Some(Acao::Confirmar(id));
                        }
                        if ui.add(Botao::fantasma("Desfazer").pequeno()).clicked() {
                            acao = Some(Acao::Desfazer(id));
                        }
                    });
                }
            });
        });
    });
    let r = match acao {
        Some(Acao::Confirmar(id)) => motor.executar(
            sessao,
            "estoque.confirmar_tributacao_a_conferir.v1",
            &ConfirmarTributacaoAConferir { id },
        ),
        Some(Acao::Desfazer(id)) => motor.executar(
            sessao,
            "estoque.desfazer_tributacao_a_conferir.v1",
            &DesfazerTributacaoAConferir { id },
        ),
        None => return,
    };
    match r {
        Ok(()) => {
            estado.a_conferir = carregar(motor, sessao);
            estado.prc.carregar(motor, sessao);
            notificar(ui.ctx(), Notificacao::sucesso("Tributação conferida"));
        }
        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
    }
}
