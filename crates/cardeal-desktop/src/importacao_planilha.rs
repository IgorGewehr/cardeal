//! O que as importações por planilha têm em comum na tela (clientes, contas em aberto): ler o
//! CSV escolhido, salvar o modelo, a linha "Escolher arquivo… · Baixar modelo… · nome" e a
//! grade de linhas com erro. Cada importação tem a sua prévia e o seu comando.

use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::organisms::{notificar, ColunaGrade as Coluna, Grade, Notificacao};
use eframe::egui;

/// Planilhas antigas vêm em ISO-8859-1, que não é UTF-8 válido.
pub fn ler_texto(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| bytes.iter().map(|b| char::from(*b)).collect())
}

/// Abre o seletor de arquivo e lê o CSV: `(nome do arquivo, texto)`, ou `None` se o usuário
/// desistiu ou o arquivo não pôde ser lido (aí já notifica).
fn escolher_csv(ctx: &egui::Context) -> Option<(String, String)> {
    let caminho = rfd::FileDialog::new()
        .add_filter("Planilha CSV", &["csv", "txt"])
        .pick_file()?;
    match std::fs::read(&caminho) {
        Ok(bytes) => Some((
            caminho.file_name().map_or_else(
                || "planilha".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            ),
            ler_texto(&bytes),
        )),
        Err(e) => {
            notificar(
                ctx,
                Notificacao::erro("Não consegui ler o arquivo").detalhe(e.to_string()),
            );
            None
        }
    }
}

fn salvar_modelo(ctx: &egui::Context, nome: &str, conteudo: &str) {
    let Some(caminho) = rfd::FileDialog::new()
        .set_file_name(nome)
        .add_filter("Planilha CSV", &["csv"])
        .save_file()
    else {
        return;
    };
    match std::fs::write(&caminho, conteudo) {
        Ok(()) => notificar(
            ctx,
            Notificacao::sucesso(format!("Modelo salvo em {}", caminho.display())),
        ),
        Err(e) => notificar(
            ctx,
            Notificacao::erro(format!("Não foi possível salvar: {e}")),
        ),
    }
}

/// "Escolher arquivo… · Baixar modelo… · nome-do-arquivo.csv". Devolve o arquivo escolhido
/// neste quadro: `(nome, texto)`.
pub fn barra_arquivo(
    ui: &mut egui::Ui,
    arquivo_atual: &str,
    nome_modelo: &str,
    modelo: &str,
) -> Option<(String, String)> {
    let mut escolhido = None;
    ui.horizontal(|ui| {
        if ui.add(Botao::secundario("Escolher arquivo…")).clicked() {
            escolhido = escolher_csv(ui.ctx());
        }
        if ui.add(Botao::fantasma("Baixar modelo…")).clicked() {
            salvar_modelo(ui.ctx(), nome_modelo, modelo);
        }
        if !arquivo_atual.is_empty() {
            ui.add(Rotulo::interface(arquivo_atual.to_owned()));
        }
    });
    escolhido
}

/// "Linhas com erro": a linha do arquivo e o problema.
pub fn grade_de_erros(ui: &mut egui::Ui, id_salt: &'static str, erros: &[(usize, String)]) {
    ui.add(Rotulo::titulo_secao("Linhas com erro"));
    Grade::nova(vec![
        Coluna::nova("Linha").largura(70.0).numero(),
        Coluna::nova("Problema"),
    ])
    .id_salt(id_salt)
    .mostrar(ui, erros.len(), |i, row| {
        let (linha, mensagem) = &erros[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(linha.to_string()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(mensagem.clone()));
        });
    });
}

/// "… e mais N." quando a amostra não mostra tudo.
pub fn e_mais(ui: &mut egui::Ui, total: u32, mostradas: usize) {
    let resto = usize::try_from(total)
        .unwrap_or(0)
        .saturating_sub(mostradas);
    if resto > 0 {
        ui.add(Rotulo::apoio(format!("… e mais {resto}.")));
    }
}
