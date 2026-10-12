//! "Importar planilha" da aba Estrutura: escolhe um CSV (`Grupo;Subgrupo;Categoria;
//! Subcategoria`, um caminho por linha), mostra "N novos, M já existiam, K erros" e só então
//! confirma. Tudo ou nada: com um erro de linha, o botão de confirmar não existe.

use super::*;
use cardeal_ui::organisms::ColunaGrade as Coluna;
use mod_estoque::importacao_estrutura::PreviaImportacao;
use mod_estoque::{EstruturaImportada, ImportarEstrutura, PreviaDaImportacaoDaEstrutura};

/// A planilha escolhida e a prévia dela.
#[derive(Default)]
pub(super) struct EstadoImportacao {
    arquivo: String,
    texto: String,
    previa: Option<PreviaImportacao>,
}

impl EstadoImportacao {
    pub(super) fn limpar(&mut self) {
        *self = Self::default();
    }
}

/// Planilhas antigas vêm em ISO-8859-1, que não é UTF-8 válido.
fn ler_texto(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| bytes.iter().map(|b| char::from(*b)).collect())
}

/// Guarda o texto e pede a prévia ao motor (sem gravar nada).
pub(super) fn carregar_texto(
    motor: &Motor,
    sessao: &Sessao,
    imp: &mut EstadoImportacao,
    arquivo: &str,
    texto: String,
) -> Result<(), String> {
    let previa: PreviaImportacao = motor
        .consultar(
            sessao,
            "estoque.previa_importacao_estrutura.v1",
            &PreviaDaImportacaoDaEstrutura {
                texto: texto.clone(),
            },
        )
        .map_err(|e| e.mensagem)?;
    imp.arquivo = arquivo.to_owned();
    imp.texto = texto;
    imp.previa = Some(previa);
    Ok(())
}

fn escolher_arquivo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    let Some(caminho) = rfd::FileDialog::new()
        .add_filter("Planilha CSV", &["csv", "txt"])
        .pick_file()
    else {
        return;
    };
    let bytes = match std::fs::read(&caminho) {
        Ok(b) => b,
        Err(e) => {
            notificar(
                ctx,
                Notificacao::erro("Não consegui ler o arquivo").detalhe(e.to_string()),
            );
            return;
        }
    };
    let arquivo = caminho.file_name().map_or_else(
        || "planilha".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    if let Err(e) = carregar_texto(
        motor,
        sessao,
        &mut estado.est.imp,
        &arquivo,
        ler_texto(&bytes),
    ) {
        notificar(
            ctx,
            Notificacao::erro("Não consegui analisar a planilha").detalhe(e),
        );
    }
}

/// Aplica a planilha; devolve o que foi criado.
pub(super) fn confirmar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) -> Result<EstruturaImportada, String> {
    let r: EstruturaImportada = motor
        .executar(
            sessao,
            "estoque.importar_estrutura.v1",
            &ImportarEstrutura {
                texto: estado.est.imp.texto.clone(),
            },
        )
        .map_err(|e| e.mensagem)?;
    estado.est.imp.limpar();
    estado.carregar(motor, sessao);
    Ok(r)
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    let fechar = Dialogo::nova("Importar estrutura por planilha")
        .largura(640.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| corpo(ui, ctx, motor, sessao, estado),
            |ui, estado| {
                let pode = estado
                    .est
                    .imp
                    .previa
                    .as_ref()
                    .is_some_and(|p| p.erros.is_empty() && p.novos > 0);
                if ui.add_enabled(pode, Botao::primario("Importar")).clicked() {
                    match confirmar(motor, sessao, estado) {
                        Ok(r) => {
                            notificar(
                                ui.ctx(),
                                Notificacao::sucesso(format!("{} grupo(s) criado(s)", r.criados)),
                            );
                            estado.dlg = Dlg::Fechado;
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

fn corpo(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    ui.add(Rotulo::apoio(
        "Um CSV com um caminho por linha: Grupo;Subgrupo;Categoria;Subcategoria (os níveis de \
         baixo são opcionais; o separador pode ser ; ou ,; o cabeçalho é opcional). O que já \
         existe, sem diferenciar caixa nem acento, é mantido; nada é gravado se alguma linha \
         tiver erro.",
    ));
    ui.add_space(Espaco::E12);
    ui.horizontal(|ui| {
        if ui.add(Botao::secundario("Escolher arquivo…")).clicked() {
            escolher_arquivo(ctx, motor, sessao, estado);
        }
        if !estado.est.imp.arquivo.is_empty() {
            ui.add(Rotulo::interface(estado.est.imp.arquivo.clone()));
        }
    });
    let Some(p) = &estado.est.imp.previa else {
        return;
    };
    ui.add_space(Espaco::E12);
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Novos", p.novos).tom(Tom::Positivo),
        CartaoKpi::contagem("Já existiam", p.ja_existiam).tom(Tom::Neutro),
        CartaoKpi::contagem("Com erro", p.erros.len()).tom(if p.erros.is_empty() {
            Tom::Neutro
        } else {
            Tom::Negativo
        }),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    if !p.erros.is_empty() {
        ui.add(Rotulo::titulo_secao("Linhas com erro"));
        Grade::nova(vec![
            Coluna::nova("Linha").largura(70.0).numero(),
            Coluna::nova("Problema"),
        ])
        .id_salt("importacao-erros")
        .mostrar(ui, p.erros.len(), |i, row| {
            let e = &p.erros[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(e.linha.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(e.mensagem.clone()));
            });
        });
    } else if p.novos == 0 {
        ui.add(Rotulo::interface(
            "Tudo o que a planilha traz já está cadastrado: nada a importar.",
        ));
    } else {
        ui.add(Rotulo::titulo_secao("O que será criado"));
        Grade::nova(vec![Coluna::nova("Caminho")])
            .id_salt("importacao-novos")
            .mostrar(ui, p.amostra.len(), |i, row| {
                row.col(|ui| {
                    ui.add(Rotulo::interface(p.amostra[i].clone()));
                });
            });
        let resto = usize::try_from(p.novos)
            .unwrap_or(0)
            .saturating_sub(p.amostra.len());
        if resto > 0 {
            ui.add(Rotulo::apoio(format!("… e mais {resto}.")));
        }
    }
}
