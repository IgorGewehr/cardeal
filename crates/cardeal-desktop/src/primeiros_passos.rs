//! **Primeiros passos** (onda 8, o assistente guiado da seção 4 da especificação): o que falta
//! para a empresa operar, na ordem em que se faz — cada passo conferido pelos próprios dados
//! (não por um "marquei como feito"), com um botão que leva à tela certa.
//!
//! Abre sozinho depois do login para quem administra, enquanto falta algum passo; "Não mostrar
//! mais" vale para este computador. A paleta de comandos (`Ctrl+K`) o reabre.

use std::path::PathBuf;

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Data, Fuso, Periodo};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo};
use cardeal_ui::organisms::Dialogo;
use cardeal_ui::tokens::Espaco;
use eframe::egui;

/// Para onde um passo leva.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Destino {
    /// Configurações › Empresa.
    Empresa,
    /// Configurações › Fiscal.
    Fiscal,
    /// Configurações › Acesso.
    Acesso,
    /// Estoque (produtos).
    Estoque,
    /// Financeiro › Contas bancárias.
    Bancos,
    /// Clientes.
    Clientes,
    /// PDV.
    Pdv,
}

/// Um passo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Passo {
    /// O que fazer.
    pub titulo: &'static str,
    /// Por quê, numa linha.
    pub porque: &'static str,
    /// Já está feito (pelos dados).
    pub feito: bool,
    /// Para onde o botão leva.
    pub destino: Destino,
}

fn caminho_oculto() -> PathBuf {
    crate::caminho_da_base().with_file_name("primeiros_passos_ocultos.txt")
}

/// A pessoa pediu para não mostrar mais neste computador?
pub fn ocultos() -> bool {
    caminho_oculto().exists()
}

fn ocultar() {
    let _ = std::fs::write(caminho_oculto(), "1");
}

/// Confere cada passo nos dados da empresa da sessão.
pub fn avaliar(motor: &Motor, sessao: &Sessao) -> Vec<Passo> {
    let empresa = motor.empresa_resumo().ok();
    let identidade = motor.identidade_visual().ok();
    let empresa_feita = empresa.is_some_and(|e| !e.regime.is_empty())
        && identidade.is_some_and(|i| !i.telefone.trim().is_empty() || !i.email.trim().is_empty());
    let fiscal_feito = motor
        .consultar(sessao, "fiscal.situacao.v1", &mod_fiscal::SituacaoFiscal)
        .is_ok_and(|s| s.certificado.is_some() && s.pendencias.is_empty());
    let produtos = motor
        .consultar(
            sessao,
            "estoque.produtos_com_saldo.v1",
            &mod_estoque::ProdutosComSaldo,
        )
        .map(|v| !v.is_empty())
        .unwrap_or(false);
    let contas = motor
        .consultar(
            sessao,
            "financeiro.contas_disponiveis.v1",
            &mod_financeiro::ContasDisponiveis,
        )
        .is_ok_and(|v| v.iter().any(|c| !c.e_caixa || !c.saldo.e_zero()));
    let clientes = motor
        .consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &mod_clientes::PessoasPorPapel {
                papel: mod_clientes::Papel::Cliente,
                busca: None,
            },
        )
        .is_ok_and(|v| !v.is_empty());
    let equipe = motor.usuarios().is_ok_and(|v| v.len() > 1);
    let hoje = Data::hoje(Fuso::BRASILIA);
    let vendeu = motor
        .consultar(
            sessao,
            "financeiro.dre_do_periodo.v1",
            &mod_financeiro::DreDoPeriodo {
                periodo: Periodo::novo(hoje.inicio_do_ano(), hoje),
            },
        )
        .is_ok_and(|d| d.receita_bruta.e_positivo());
    vec![
        Passo {
            titulo: "Dados da empresa",
            porque: "O regime tributário entra no preço sugerido; o telefone e o e-mail saem nos orçamentos e pedidos.",
            feito: empresa_feita,
            destino: Destino::Empresa,
        },
        Passo {
            titulo: "Certificado digital e dados fiscais",
            porque: "Sem o certificado A1, a inscrição estadual e o CSC, não sai NFC-e nem NF-e.",
            feito: fiscal_feito,
            destino: Destino::Fiscal,
        },
        Passo {
            titulo: "Produtos",
            porque: "Cadastre um a um ou importe a planilha — também dá para criar pela nota de compra.",
            feito: produtos,
            destino: Destino::Estoque,
        },
        Passo {
            titulo: "Contas bancárias e saldo inicial",
            porque: "O caixa e o banco com o saldo de hoje deixam o fluxo de caixa e a conciliação certos.",
            feito: contas,
            destino: Destino::Bancos,
        },
        Passo {
            titulo: "Clientes",
            porque: "Importe a planilha de clientes (com o que eles devem) ou cadastre no balcão.",
            feito: clientes,
            destino: Destino::Clientes,
        },
        Passo {
            titulo: "Equipe",
            porque: "Cada pessoa com o seu login e o seu papel: o caixa não vê custo, o vendedor tem teto de desconto.",
            feito: equipe,
            destino: Destino::Acesso,
        },
        Passo {
            titulo: "Primeira venda",
            porque: "Abra o caixa no PDV e venda — a nota sai sozinha quando o fiscal estiver pronto.",
            feito: vendeu,
            destino: Destino::Pdv,
        },
    ]
}

/// O diálogo. Devolve para onde ir, se a pessoa pediu; `aberto` fecha com "Mais tarde" ou
/// "Não mostrar mais".
pub fn dialogo(ctx: &egui::Context, passos: &[Passo], aberto: &mut bool) -> Option<Destino> {
    let feitos = passos.iter().filter(|p| p.feito).count();
    let mut destino = None;
    let mut fechar_pedido = false;
    let fechar = Dialogo::nova(format!("Primeiros passos — {feitos} de {}", passos.len()))
        .largura(620.0)
        .mostrar(
            ctx,
            &mut destino,
            |ui, destino| {
                ui.add(
                    Rotulo::apoio(
                        "O que falta para a empresa operar, na ordem em que se faz. Cada passo se \
                         marca sozinho quando os dados estão lá.",
                    )
                    .quebravel(),
                );
                ui.add_space(Espaco::E12);
                for (n, p) in passos.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if p.feito {
                            ui.add(Etiqueta::positiva("feito"));
                        } else {
                            ui.add(Etiqueta::atencao("falta"));
                        }
                        ui.add(Rotulo::interface(format!("{}. {}", n + 1, p.titulo)));
                        if !p.feito && ui.add(Botao::secundario("Abrir").pequeno()).clicked() {
                            *destino = Some(p.destino);
                        }
                    });
                    ui.add(Rotulo::apoio(p.porque).quebravel());
                    ui.add_space(Espaco::E8);
                }
            },
            |ui, _| {
                if ui.add(Botao::fantasma("Não mostrar mais")).clicked() {
                    ocultar();
                    fechar_pedido = true;
                }
                if ui.add(Botao::secundario("Mais tarde")).clicked() {
                    fechar_pedido = true;
                }
            },
        );
    if fechar || fechar_pedido || destino.is_some() {
        *aberto = false;
    }
    destino
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    #[test]
    fn a_empresa_nova_tem_quase_tudo_por_fazer_e_os_passos_se_marcam_pelos_dados() {
        let t = motor_de_teste();
        let antes = avaliar(&t.motor, &t.sessao);
        assert_eq!(antes.len(), 7);
        assert!(antes.iter().filter(|p| p.feito).count() <= 1);
        let equipe = |p: &[Passo]| {
            p.iter()
                .find(|x| x.destino == Destino::Acesso)
                .unwrap()
                .feito
        };
        assert!(!equipe(&antes));
        t.motor
            .cadastrar_usuario(
                &t.sessao,
                "caixa",
                "Caixa da loja",
                mod_empresa::PapelDeFabrica::OperadorCaixa,
                "senha-do-caixa-1",
            )
            .unwrap();
        assert!(equipe(&avaliar(&t.motor, &t.sessao)));
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let mut aberto = true;
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let _ = dialogo(ctx, &antes, &mut aberto);
        });
        assert!(aberto);
    }
}
