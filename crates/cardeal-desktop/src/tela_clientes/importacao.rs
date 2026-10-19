//! "Importar clientes…": escolhe um CSV com cabeçalho (só `Nome` é obrigatória), mostra
//! "N novos, M já existiam, K erros" com quem será cadastrado e só então confirma. Tudo ou
//! nada: com uma linha com erro, o botão de importar não existe. "Baixar modelo…" salva uma
//! planilha de exemplo com todas as colunas que a importação entende.

use cardeal_cliente::{Motor, Sessao};
use cardeal_ui::atoms::{Botao, Rotulo, Tom};
use cardeal_ui::molecules::CartaoKpi;
use cardeal_ui::organisms::{
    notificar, ColunaGrade as Coluna, Dialogo, FaixaKpi, Grade, Notificacao,
};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_clientes::importacao::PreviaImportacaoClientes;
use mod_clientes::{ImportarPessoas, PessoasImportadas, PreviaDaImportacaoDePessoas};

use super::EstadoTelaClientes;
use crate::importacao_planilha::{barra_arquivo, e_mais, grade_de_erros};

/// O modelo que "Baixar modelo…" salva: todas as colunas conhecidas e duas linhas de exemplo.
pub(super) const MODELO: &str = "\u{feff}Nome;CPF/CNPJ;Nome fantasia;Celular;Telefone;E-mail;\
Endereço;Número;Complemento;Bairro;Cidade;UF;CEP;Nascimento;Limite de crédito;Papel\n\
Ana Lima;529.982.247-25;;(51) 99999-0000;;ana@exemplo.com;Rua dos Andradas;100;Ap 2;Centro;\
Porto Alegre;RS;90020-000;15/03/1985;500,00;Cliente\n\
Distribuidora Sul Ltda;11.222.333/0001-81;Distribuidora Sul;;(51) 3333-4444;;Av. Brasil;\
2000;;Centro;Canoas;RS;92010-000;;;Fornecedor\n";

/// A planilha escolhida e a prévia dela.
#[derive(Default)]
pub(super) struct EstadoImportacao {
    arquivo: String,
    texto: String,
    pub(super) previa: Option<PreviaImportacaoClientes>,
}

/// Guarda o texto e pede a prévia ao motor (sem gravar nada).
pub(super) fn carregar_texto(
    motor: &Motor,
    sessao: &Sessao,
    imp: &mut EstadoImportacao,
    arquivo: &str,
    texto: String,
) -> Result<(), String> {
    let previa: PreviaImportacaoClientes = motor
        .consultar(
            sessao,
            "clientes.previa_importacao_pessoas.v1",
            &PreviaDaImportacaoDePessoas {
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
    imp: &EstadoImportacao,
) -> Result<PessoasImportadas, String> {
    motor
        .executar(
            sessao,
            "clientes.importar_pessoas.v1",
            &ImportarPessoas {
                texto: imp.texto.clone(),
            },
        )
        .map_err(|e| e.mensagem)
}

/// "12 cadastro(s) criado(s), 3 fornecedor(es), 5 com limite de crédito".
fn resumo(r: &PessoasImportadas) -> String {
    let mut partes = vec![format!("{} cadastro(s) criado(s)", r.criadas)];
    if r.fornecedores > 0 {
        partes.push(format!("{} fornecedor(es)", r.fornecedores));
    }
    if r.com_limite > 0 {
        partes.push(format!("{} com limite de crédito", r.com_limite));
    }
    if r.ja_existiam > 0 {
        partes.push(format!("{} já existiam", r.ja_existiam));
    }
    partes.join(", ")
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaClientes,
) {
    let fechar = Dialogo::nova("Importar clientes por planilha")
        .largura(820.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| corpo(ui, motor, sessao, estado),
            |ui, estado| {
                let Some(imp) = estado.importacao.as_ref() else {
                    return;
                };
                let pode = imp
                    .previa
                    .as_ref()
                    .is_some_and(|p| p.erros.is_empty() && p.novos > 0);
                if ui.add_enabled(pode, Botao::primario("Importar")).clicked() {
                    match confirmar(motor, sessao, imp) {
                        Ok(r) => {
                            notificar(ui.ctx(), Notificacao::sucesso(resumo(&r)));
                            estado.importacao = None;
                            estado.carregar(motor, sessao);
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
                    }
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    estado.importacao = None;
                }
            },
        );
    if fechar {
        estado.importacao = None;
    }
}

fn corpo(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaClientes) {
    let Some(imp) = estado.importacao.as_mut() else {
        return;
    };
    ui.add(
        Rotulo::apoio(
            "Um CSV com cabeçalho, uma pessoa por linha. Só a coluna Nome é obrigatória; as \
             outras (CPF/CNPJ, Nome fantasia, Celular, Telefone, E-mail, Endereço, Número, \
             Complemento, Bairro, Cidade, UF, CEP, Nascimento, Limite de crédito e Papel — \
             Cliente ou Fornecedor) valem pelo nome, em qualquer ordem. O CPF/CNPJ e os \
             contatos são conferidos. Quem já está no cadastro (mesmo CPF/CNPJ ou, sem ele, o \
             mesmo nome) fica como está, e nada é gravado se alguma linha tiver erro.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E12);
    if let Some((arquivo, texto)) = barra_arquivo(ui, &imp.arquivo, "modelo-clientes.csv", MODELO) {
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
        CartaoKpi::contagem("Novos", p.novos).tom(Tom::Positivo),
        CartaoKpi::contagem("Já existiam", p.ja_existiam).tom(Tom::Neutro),
        CartaoKpi::contagem("Com limite de crédito", p.com_limite).tom(Tom::Neutro),
        CartaoKpi::contagem("Com erro", p.erros.len()).tom(if p.erros.is_empty() {
            Tom::Neutro
        } else {
            Tom::Negativo
        }),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E8);
    if p.fornecedores > 0 {
        ui.add(Rotulo::apoio(format!(
            "{} dos novos entram como fornecedor.",
            p.fornecedores
        )));
    }
    if !p.colunas_ignoradas.is_empty() {
        ui.add(
            Rotulo::apoio(format!(
                "Colunas ignoradas: {}.",
                p.colunas_ignoradas.join(", ")
            ))
            .quebravel(),
        );
    }
    ui.add_space(Espaco::E12);

    if !p.erros.is_empty() {
        let erros: Vec<(usize, String)> = p
            .erros
            .iter()
            .map(|e| (e.linha, e.mensagem.clone()))
            .collect();
        grade_de_erros(ui, "importacao-clientes-erros", &erros);
    } else if p.novos == 0 {
        ui.add(Rotulo::interface(
            "Todos da planilha já estão no cadastro: nada a importar.",
        ));
    } else {
        ui.add(Rotulo::titulo_secao("Quem será cadastrado"));
        Grade::nova(vec![
            Coluna::nova("Nome"),
            Coluna::nova("CPF/CNPJ").largura(150.0),
            Coluna::nova("Contato").largura(150.0),
            Coluna::nova("Cidade").largura(140.0),
            Coluna::nova("Limite").largura(100.0).numero(),
        ])
        .id_salt("importacao-clientes-novos")
        .mostrar(ui, p.amostra.len(), |i, row| {
            let l = &p.amostra[i];
            row.col(|ui| {
                let nome = if l.papel == "Fornecedor" {
                    format!("{} (fornecedor)", l.nome)
                } else {
                    l.nome.clone()
                };
                ui.add(Rotulo::interface(nome));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(l.documento.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(l.contato.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::campo(l.cidade.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(
                    l.limite.map_or_else(String::new, |v| v.to_string()),
                ));
            });
        });
        e_mais(ui, p.novos, p.amostra.len());
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn o_modelo_entra_inteiro_e_so_ao_confirmar() {
        let t = motor_de_teste();
        let mut estado = EstadoTelaClientes::default();
        estado.carregar(&t.motor, &t.sessao);
        let antes = estado.pessoas.len();

        // Planilha com erro: a prévia lista a linha e confirmar não grava nada.
        let mut imp = EstadoImportacao::default();
        carregar_texto(
            &t.motor,
            &t.sessao,
            &mut imp,
            "ruim.csv",
            "Nome;CPF\nCarla;123\n".into(),
        )
        .expect("prévia");
        assert_eq!(imp.previa.as_ref().unwrap().erros.len(), 1);
        assert!(confirmar(&t.motor, &t.sessao, &imp).is_err());

        // O modelo que "Baixar modelo…" salva é uma planilha válida.
        carregar_texto(&t.motor, &t.sessao, &mut imp, "modelo.csv", MODELO.into()).expect("prévia");
        let previa = imp.previa.as_ref().unwrap();
        assert!(previa.erros.is_empty(), "{:?}", previa.erros);
        assert_eq!(
            (previa.novos, previa.fornecedores, previa.com_limite),
            (2, 1, 1)
        );

        // O diálogo desenha a prévia num quadro de verdade do egui.
        estado.importacao = Some(imp);
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                dialogo(ctx, &t.motor, &t.sessao, &mut estado);
            });
        }
        let r =
            confirmar(&t.motor, &t.sessao, estado.importacao.as_ref().unwrap()).expect("importa");
        assert_eq!((r.criadas, r.fornecedores), (2, 1));
        estado.carregar(&t.motor, &t.sessao);
        assert_eq!(estado.pessoas.len(), antes + 1, "a lista é de clientes");
        assert!(estado.pessoas.iter().any(|p| p.nome == "Ana Lima"));
    }
}
