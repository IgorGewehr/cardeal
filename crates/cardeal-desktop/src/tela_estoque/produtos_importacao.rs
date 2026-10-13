//! "Importar produtos…": escolhe um CSV com cabeçalho (só `Nome` é obrigatória), mostra
//! "N novos, M já existiam, K erros" com o que será criado e só então confirma. Tudo ou nada:
//! com um erro de linha, o botão de importar não existe. "Baixar modelo…" salva uma planilha
//! de exemplo com todas as colunas que a importação entende.

use super::*;
use cardeal_ui::organisms::ColunaGrade as Coluna;
use mod_estoque::importacao_produtos::PreviaImportacaoProdutos;
use mod_estoque::{ImportarProdutos, PreviaDaImportacaoDeProdutos, ProdutosImportados};

/// O modelo que "Baixar modelo…" salva: todas as colunas conhecidas e duas linhas de exemplo.
pub(super) const MODELO: &str = "\u{feff}Nome;Grupo;Subgrupo;Categoria;Subcategoria;Unidade;\
Código de barras;NCM;Margem;Comissão;Ponto de pedido;Estoque mínimo;Estoque máximo;Fabricante;\
Código do fabricante;Localização;Estoque;Custo;Local\n\
Refrigerante Cola 2L;Bebidas;Refrigerantes;Cola;Pet;UN;7894900011517;2202.10.00;30;;12;6;48;\
Coca-Cola;;Corredor 2;24;7,49;\n\
Arroz Parboilizado 5kg;Mercearia;Grãos;Arroz;;PCT;;1006.30.21;25;2;10;5;40;Kiarroz;;Corredor 4;\
15;21,90;\n";

/// A planilha escolhida e a prévia dela.
#[derive(Default)]
pub(super) struct EstadoImportacaoProdutos {
    arquivo: String,
    texto: String,
    pub(super) previa: Option<PreviaImportacaoProdutos>,
}

impl EstadoImportacaoProdutos {
    pub(super) fn limpar(&mut self) {
        *self = Self::default();
    }
}

/// Guarda o texto e pede a prévia ao motor (sem gravar nada).
pub(super) fn carregar_texto(
    motor: &Motor,
    sessao: &Sessao,
    imp: &mut EstadoImportacaoProdutos,
    arquivo: &str,
    texto: String,
) -> Result<(), String> {
    let previa: PreviaImportacaoProdutos = motor
        .consultar(
            sessao,
            "estoque.previa_importacao_produtos.v1",
            &PreviaDaImportacaoDeProdutos {
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
        &mut estado.imp_produtos,
        &arquivo,
        super::estrutura_importacao::ler_texto(&bytes),
    ) {
        notificar(
            ctx,
            Notificacao::erro("Não consegui analisar a planilha").detalhe(e),
        );
    }
}

fn salvar_modelo(ctx: &egui::Context) {
    let Some(caminho) = rfd::FileDialog::new()
        .set_file_name("modelo-produtos.csv")
        .add_filter("Planilha CSV", &["csv"])
        .save_file()
    else {
        return;
    };
    match std::fs::write(&caminho, MODELO) {
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

/// Aplica a planilha; devolve o que foi criado.
pub(super) fn confirmar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) -> Result<ProdutosImportados, String> {
    let r: ProdutosImportados = motor
        .executar(
            sessao,
            "estoque.importar_produtos.v1",
            &ImportarProdutos {
                texto: estado.imp_produtos.texto.clone(),
            },
        )
        .map_err(|e| e.mensagem)?;
    estado.imp_produtos.limpar();
    estado.carregar(motor, sessao);
    Ok(r)
}

/// "3 produtos criados, 2 com estoque inicial, 4 grupos novos".
fn resumo(r: &ProdutosImportados) -> String {
    let mut partes = vec![format!("{} produto(s) criado(s)", r.criados)];
    if r.com_estoque > 0 {
        partes.push(format!("{} com estoque inicial", r.com_estoque));
    }
    if r.grupos_criados > 0 {
        partes.push(format!("{} grupo(s) novo(s)", r.grupos_criados));
    }
    if r.unidades_criadas > 0 {
        partes.push(format!("{} unidade(s) nova(s)", r.unidades_criadas));
    }
    partes.join(", ")
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    let fechar = Dialogo::nova("Importar produtos por planilha")
        .largura(760.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| corpo(ui, ctx, motor, sessao, estado),
            |ui, estado| {
                let pode = estado
                    .imp_produtos
                    .previa
                    .as_ref()
                    .is_some_and(|p| p.erros.is_empty() && p.novos > 0);
                if ui.add_enabled(pode, Botao::primario("Importar")).clicked() {
                    match confirmar(motor, sessao, estado) {
                        Ok(r) => {
                            notificar(ui.ctx(), Notificacao::sucesso(resumo(&r)));
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
    ui.add(
        Rotulo::apoio(
            "Um CSV com cabeçalho, um produto por linha. Só a coluna Nome é obrigatória; as \
             outras (Grupo a Subcategoria, Unidade, Código de barras, NCM, Margem, Comissão, \
             Ponto de pedido, Estoque mínimo e máximo, Fabricante, Localização, Estoque, Custo e Local) \
             valem pelo nome, em qualquer ordem. Grupos e unidades que faltam são criados; o \
             estoque entra com o custo informado. Produto que já existe (mesmo código de \
             barras ou nome) fica como está, e nada é gravado se alguma linha tiver erro.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E12);
    ui.horizontal(|ui| {
        if ui.add(Botao::secundario("Escolher arquivo…")).clicked() {
            escolher_arquivo(ctx, motor, sessao, estado);
        }
        if ui.add(Botao::fantasma("Baixar modelo…")).clicked() {
            salvar_modelo(ctx);
        }
        if !estado.imp_produtos.arquivo.is_empty() {
            ui.add(Rotulo::interface(estado.imp_produtos.arquivo.clone()));
        }
    });
    let Some(p) = &estado.imp_produtos.previa else {
        return;
    };
    ui.add_space(Espaco::E12);
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Novos", p.novos).tom(Tom::Positivo),
        CartaoKpi::contagem("Já existiam", p.ja_existiam).tom(Tom::Neutro),
        CartaoKpi::contagem("Com estoque inicial", p.com_estoque).tom(Tom::Neutro),
        CartaoKpi::contagem("Com erro", p.erros.len()).tom(if p.erros.is_empty() {
            Tom::Neutro
        } else {
            Tom::Negativo
        }),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E8);

    let mut avisos = Vec::new();
    if p.grupos_novos > 0 {
        avisos.push(format!(
            "{} grupo(s) novo(s) na estrutura mercadológica.",
            p.grupos_novos
        ));
    }
    if !p.unidades_novas.is_empty() {
        avisos.push(format!(
            "Unidade(s) nova(s): {}.",
            p.unidades_novas.join(", ")
        ));
    }
    if p.custos_sem_estoque > 0 {
        avisos.push(format!(
            "{} linha(s) têm custo mas estoque zero: o custo só é gravado junto com o estoque, \
             então esses produtos ficam sem custo.",
            p.custos_sem_estoque
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
        ui.add(Rotulo::titulo_secao("Linhas com erro"));
        Grade::nova(vec![
            Coluna::nova("Linha").largura(70.0).numero(),
            Coluna::nova("Problema"),
        ])
        .id_salt("importacao-produtos-erros")
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
            "Todos os produtos da planilha já estão cadastrados: nada a importar.",
        ));
    } else {
        ui.add(Rotulo::titulo_secao("O que será criado"));
        Grade::nova(vec![
            Coluna::nova("Produto"),
            Coluna::nova("Grupo"),
            Coluna::nova("Un.").largura(60.0),
            Coluna::nova("Estoque").largura(100.0).numero(),
            Coluna::nova("Custo").largura(110.0).numero(),
        ])
        .id_salt("importacao-produtos-novos")
        .mostrar(ui, p.amostra.len(), |i, row| {
            let l = &p.amostra[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(l.nome.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(l.grupo.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(l.unidade.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(
                    l.estoque.map_or_else(String::new, |q| q.to_string()),
                ));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(
                    l.custo.map_or_else(String::new, |c| c.to_string()),
                ));
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
