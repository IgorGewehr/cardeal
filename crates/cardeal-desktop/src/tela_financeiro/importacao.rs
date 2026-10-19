//! "Importar contas em aberto…": a implantação do primeiro dia — o que os clientes devem e o
//! que a empresa deve, vindo do sistema antigo numa planilha. Escolhe o CSV, mostra quantas
//! contas a receber e a pagar (e os totais), quem será cadastrado e os erros por linha, e só
//! então confirma. Tudo ou nada; importar de novo não duplica.

use cardeal_cliente::{Motor, Sessao};
use cardeal_ui::atoms::{Botao, Rotulo, Tom};
use cardeal_ui::molecules::CartaoKpi;
use cardeal_ui::organisms::{
    notificar, ColunaGrade as Coluna, Dialogo, FaixaKpi, Grade, Notificacao,
};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_financeiro::importacao::PreviaImportacaoContas;
use mod_financeiro::{ContasImportadas, ImportarContasEmAberto, PreviaDaImportacaoDeContas};

use super::{Dlg, EstadoTelaFinanceiro};
use crate::importacao_planilha::{barra_arquivo, e_mais, grade_de_erros};

/// O modelo que "Baixar modelo…" salva: as colunas que a importação entende e três linhas.
pub(super) const MODELO: &str = "\u{feff}Tipo;Nome;CPF/CNPJ;Vencimento;Valor;Emissão;Número;\
Categoria;Descrição\n\
Receber;Ana Lima;529.982.247-25;10/11/2026;150,00;10/10/2026;NF 1234;Vendas;Parcela 2 de 3\n\
Receber;Bruno Souza;;05/10/2026;80,00;;;;Fiado do caderno\n\
Pagar;Distribuidora Sul Ltda;11.222.333/0001-81;20/11/2026;1.250,00;;Boleto 5566;Mercadorias;\n";

/// A planilha escolhida e a prévia dela.
#[derive(Default)]
pub(super) struct EstadoImportacaoContas {
    arquivo: String,
    texto: String,
    pub(super) previa: Option<PreviaImportacaoContas>,
}

/// Guarda o texto e pede a prévia ao motor (sem gravar nada).
pub(super) fn carregar_texto(
    motor: &Motor,
    sessao: &Sessao,
    imp: &mut EstadoImportacaoContas,
    arquivo: &str,
    texto: String,
) -> Result<(), String> {
    let previa: PreviaImportacaoContas = motor
        .consultar(
            sessao,
            "financeiro.previa_importacao_contas.v1",
            &PreviaDaImportacaoDeContas {
                texto: texto.clone(),
            },
        )
        .map_err(|e| e.mensagem)?;
    imp.arquivo = arquivo.to_owned();
    imp.texto = texto;
    imp.previa = Some(previa);
    Ok(())
}

/// Aplica a planilha; devolve o que foi criado.
pub(super) fn confirmar(
    motor: &Motor,
    sessao: &Sessao,
    imp: &EstadoImportacaoContas,
) -> Result<ContasImportadas, String> {
    motor
        .executar(
            sessao,
            "financeiro.importar_contas.v1",
            &ImportarContasEmAberto {
                texto: imp.texto.clone(),
            },
        )
        .map_err(|e| e.mensagem)
}

/// "5 conta(s) a receber, 2 a pagar, 3 cadastro(s) novo(s)".
fn resumo(r: &ContasImportadas) -> String {
    let mut partes = Vec::new();
    if r.a_receber > 0 {
        partes.push(format!("{} conta(s) a receber", r.a_receber));
    }
    if r.a_pagar > 0 {
        partes.push(format!("{} a pagar", r.a_pagar));
    }
    if r.pessoas_criadas > 0 {
        partes.push(format!("{} cadastro(s) novo(s)", r.pessoas_criadas));
    }
    format!("Importado: {}", partes.join(", "))
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let fechar = Dialogo::nova("Importar contas em aberto por planilha")
        .largura(860.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                if let Dlg::ImportarContas(imp) = &mut estado.dlg {
                    corpo(ui, motor, sessao, imp);
                }
            },
            |ui, estado| {
                let Dlg::ImportarContas(imp) = &estado.dlg else {
                    return;
                };
                let pode = imp
                    .previa
                    .as_ref()
                    .is_some_and(|p| p.erros.is_empty() && p.a_receber + p.a_pagar > 0);
                if ui.add_enabled(pode, Botao::primario("Importar")).clicked() {
                    match confirmar(motor, sessao, imp) {
                        Ok(r) => {
                            notificar(ui.ctx(), Notificacao::sucesso(resumo(&r)));
                            estado.dlg = Dlg::Fechado;
                            estado.carregar(motor, sessao);
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
                    }
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn corpo(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, imp: &mut EstadoImportacaoContas) {
    ui.add(
        Rotulo::apoio(
            "Um CSV com cabeçalho, uma conta por linha: Tipo (Receber ou Pagar), Vencimento e \
             Valor em aberto são obrigatórios; Nome e CPF/CNPJ dizem de quem é (quem não está \
             no cadastro é cadastrado); Emissão, Número, Categoria e Descrição são opcionais. \
             Entra como saldo de abertura: aparece em Contas a receber e a pagar, mas não \
             conta como venda nem despesa deste mês. Importar o mesmo arquivo de novo não \
             duplica, e nada é gravado se alguma linha tiver erro.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E12);
    if let Some((arquivo, texto)) =
        barra_arquivo(ui, &imp.arquivo, "modelo-contas-em-aberto.csv", MODELO)
    {
        if let Err(e) = carregar_texto(motor, sessao, imp, &arquivo, texto) {
            notificar(
                ui.ctx(),
                Notificacao::erro("Não consegui analisar a planilha").detalhe(e),
            );
        }
    }
    let Some(p) = &imp.previa else {
        return;
    };
    ui.add_space(Espaco::E12);
    FaixaKpi::nova(vec![
        CartaoKpi::novo(format!("A receber ({})", p.a_receber), p.total_a_receber)
            .tom(Tom::Positivo),
        CartaoKpi::novo(format!("A pagar ({})", p.a_pagar), p.total_a_pagar).tom(Tom::Negativo),
        CartaoKpi::contagem("Já importadas", p.ja_existiam).tom(Tom::Neutro),
        CartaoKpi::contagem("Com erro", p.erros.len()).tom(if p.erros.is_empty() {
            Tom::Neutro
        } else {
            Tom::Negativo
        }),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E8);
    let mut avisos = Vec::new();
    if p.vencidas > 0 {
        avisos.push(format!(
            "{} conta(s) já vencida(s): entram em aberto, como atrasadas.",
            p.vencidas
        ));
    }
    if !p.pessoas_novas.is_empty() {
        let mostrar: Vec<&str> = p.pessoas_novas.iter().take(8).map(String::as_str).collect();
        let resto = p.pessoas_novas.len().saturating_sub(mostrar.len());
        avisos.push(format!(
            "{} pessoa(s) serão cadastradas (só com nome e documento): {}{}.",
            p.pessoas_novas.len(),
            mostrar.join(", "),
            if resto > 0 {
                format!(" e mais {resto}")
            } else {
                String::new()
            }
        ));
    }
    if !p.colunas_ignoradas.is_empty() {
        avisos.push(format!(
            "Colunas ignoradas: {}.",
            p.colunas_ignoradas.join(", ")
        ));
    }
    for aviso in avisos {
        ui.add(Rotulo::apoio(aviso).quebravel());
    }
    ui.add_space(Espaco::E12);

    if !p.erros.is_empty() {
        let erros: Vec<(usize, String)> = p
            .erros
            .iter()
            .map(|e| (e.linha, e.mensagem.clone()))
            .collect();
        grade_de_erros(ui, "importacao-contas-erros", &erros);
    } else if p.a_receber + p.a_pagar == 0 {
        ui.add(Rotulo::interface(
            "Todas as contas da planilha já foram importadas: nada a fazer.",
        ));
    } else {
        ui.add(Rotulo::titulo_secao("O que será lançado"));
        Grade::nova(vec![
            Coluna::nova("Tipo").largura(80.0),
            Coluna::nova("De quem"),
            Coluna::nova("Vencimento").largura(100.0),
            Coluna::nova("Valor").largura(110.0).numero(),
            Coluna::nova("Observação"),
        ])
        .id_salt("importacao-contas-novas")
        .mostrar(ui, p.amostra.len(), |i, row| {
            let l = &p.amostra[i];
            row.col(|ui| {
                ui.add(Rotulo::campo(l.tipo.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(l.pessoa.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(l.vencimento.formatar()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(l.valor.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(l.observacao.clone()));
            });
        });
        let total = p.a_receber + p.a_pagar;
        e_mais(ui, total, p.amostra.len());
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn o_modelo_entra_inteiro_cadastra_as_pessoas_e_nao_duplica() {
        let t = motor_de_teste();
        let mut estado = EstadoTelaFinanceiro::default();
        estado.carregar(&t.motor, &t.sessao);

        let mut imp = EstadoImportacaoContas::default();
        carregar_texto(&t.motor, &t.sessao, &mut imp, "modelo.csv", MODELO.into()).expect("prévia");
        let previa = imp.previa.as_ref().unwrap();
        assert!(previa.erros.is_empty(), "{:?}", previa.erros);
        assert_eq!((previa.a_receber, previa.a_pagar), (2, 1));
        assert_eq!(previa.total_a_receber.to_string(), "230,00");
        assert_eq!(previa.pessoas_novas.len(), 3);

        estado.dlg = Dlg::ImportarContas(imp);
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                dialogo(ctx, &t.motor, &t.sessao, &mut estado);
            });
        }
        let Dlg::ImportarContas(imp) = &estado.dlg else {
            panic!("o diálogo continua aberto até confirmar");
        };
        let r = confirmar(&t.motor, &t.sessao, imp).expect("importa");
        assert_eq!((r.a_receber, r.a_pagar, r.pessoas_criadas), (2, 1, 3));

        // De novo: nada a importar.
        let mut de_novo = EstadoImportacaoContas::default();
        carregar_texto(
            &t.motor,
            &t.sessao,
            &mut de_novo,
            "modelo.csv",
            MODELO.into(),
        )
        .expect("prévia");
        let previa = de_novo.previa.as_ref().unwrap();
        assert_eq!(
            (previa.a_receber + previa.a_pagar, previa.ja_existiam),
            (0, 3)
        );
        assert!(previa.pessoas_novas.is_empty());
    }
}
