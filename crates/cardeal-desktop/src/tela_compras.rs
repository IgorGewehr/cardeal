//! Tela de Compras — notas de entrada + dialog (nova / ver / confirmar / vincular).
//! `docs/modulos/compras.md`.
//!
//! Fluxo: lançar nota manual (fornecedor + itens digitados) → a cascata de casamento roda no
//! comando → itens não casados ganham um seletor de produto (`VincularProdutoManual`) →
//! quando `Conferida`, confirmar a entrada (baixa no estoque + título a pagar).

use std::collections::HashMap;

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Dinheiro, Id};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{Campo, EstadoVazio, Mascara, SeletorOpcao};
use cardeal_ui::organisms::{notificar, ColunaGrade, Dialogo, Grade, LayoutTela, Notificacao};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{Papel, PessoasPorPapel};
use mod_compras::{
    ConfirmarEntrada, EstadoCasamento, EstadoNotaEntrada, ItemNota, ItemNotaEntrada,
    ItemNotaManual, ItensDaNota, LancarNotaManual, NotasRecentes, VincularProdutoManual,
};
use mod_estoque::{ItemLocal, ItemProdutoComSaldo, Locais, ProdutosComSaldo};

#[derive(Default, PartialEq)]
enum Dlg {
    #[default]
    Fechado,
    Ver(usize),
    Nova,
}

#[derive(Default, Clone)]
struct LinhaItem {
    codigo: String,
    descricao: String,
    ncm: String,
    quantidade: String,
    valor_unitario: String,
}

#[derive(Default)]
struct FormNova {
    cnpj: String,
    nome: String,
    numero: String,
    serie: String,
    data: String,
    frete: String,
    itens: Vec<LinhaItem>,
}

/// Estado local da tela.
#[derive(Default)]
pub struct EstadoTelaCompras {
    notas: Vec<ItemNota>,
    fornecedores: HashMap<Id, String>,
    produtos: Vec<ItemProdutoComSaldo>,
    locais: Vec<ItemLocal>,
    itens_nota: Vec<ItemNotaEntrada>,
    local_sel: Option<Id>,
    vinc_produto: HashMap<Id, Option<Id>>, // item_nota -> produto escolhido
    nova: FormNova,
    erro: Option<String>,
    dlg: Dlg,
}

impl EstadoTelaCompras {
    /// Recarrega notas, fornecedores, produtos e locais.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "compras.notas_recentes.v1", &NotasRecentes) {
            Ok(n) => {
                self.notas = n;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(f) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: Papel::Fornecedor,
                busca: None,
            },
        ) {
            self.fornecedores = f.into_iter().map(|p| (p.pessoa, p.nome)).collect();
        }
        if let Ok(p) = motor.consultar(sessao, "estoque.produtos_com_saldo.v1", &ProdutosComSaldo) {
            self.produtos = p;
        }
        if let Ok(l) = motor.consultar(sessao, "estoque.locais.v1", &Locais) {
            self.locais = l;
        }
    }

    fn abrir_nota(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, i: usize) {
        let Some(n) = self.notas.get(i) else { return };
        let id = n.nota;
        self.itens_nota = motor
            .consultar(
                sessao,
                "compras.itens_da_nota.v1",
                &ItensDaNota { nota: id },
            )
            .unwrap_or_default();
        self.vinc_produto.clear();
        self.dlg = Dlg::Ver(i);
    }

    fn recarregar_itens(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, nota: Id) {
        self.itens_nota = motor
            .consultar(sessao, "compras.itens_da_nota.v1", &ItensDaNota { nota })
            .unwrap_or_default();
    }

    fn nome(&self, fornecedor: Id) -> String {
        self.fornecedores
            .get(&fornecedor)
            .cloned()
            .unwrap_or_else(|| "—".to_owned())
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    LayoutTela::nova("Compras").mostrar(
        ui,
        estado,
        |ui, estado| {
            if ui
                .add(Botao::primario("+ Nova nota").atalho("Ctrl+N"))
                .clicked()
            {
                estado.nova = FormNova {
                    data: cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA).to_string(),
                    frete: "0,00".to_owned(),
                    itens: vec![LinhaItem::default()],
                    ..FormNova::default()
                };
                estado.dlg = Dlg::Nova;
            }
        },
        |ui, estado| {
            if let Some(erro) = &estado.erro {
                ui.add(
                    Rotulo::interface(erro.clone())
                        .quebravel()
                        .cor(ui.cores().negativo),
                );
                ui.add_space(Espaco::E12);
            }
            lista(ui, motor, sessao, estado);
        },
    );

    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Ver(i) => dialogo_ver(ui.ctx(), motor, sessao, estado, i),
        Dlg::Nova => dialogo_nova(ui.ctx(), motor, sessao, estado),
    }
}

fn lista(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    if estado.notas.is_empty() {
        if estado.erro.is_none()
            && EstadoVazio::novo(Icone::Nota, "Nenhuma nota de entrada.")
                .acao("Lançar a primeira")
                .mostrar(ui)
        {
            estado.nova = FormNova {
                data: cardeal_kernel::Data::hoje(cardeal_kernel::Fuso::BRASILIA).to_string(),
                frete: "0,00".to_owned(),
                itens: vec![LinhaItem::default()],
                ..FormNova::default()
            };
            estado.dlg = Dlg::Nova;
        }
        return;
    }
    let colunas = vec![
        ColunaGrade::nova("Fornecedor"),
        ColunaGrade::nova("Nº / série").largura(120.0),
        ColunaGrade::nova("Emissão").largura(120.0),
        ColunaGrade::nova("Itens").largura(70.0).numero(),
        ColunaGrade::nova("Total").largura(130.0).numero(),
        ColunaGrade::nova("Estado").largura(110.0),
    ];
    let clicada =
        Grade::nova(colunas)
            .selecionavel(None)
            .mostrar(ui, estado.notas.len(), |i, row| {
                let n = &estado.notas[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(estado.nome(n.fornecedor)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(format!("{} / {}", n.numero, n.serie)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(n.data_emissao.to_string()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(n.itens.to_string()));
                });
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(n.valor_total));
                });
                row.col(|ui| {
                    ui.add(Rotulo::campo(n.estado.rotulo()));
                });
            });
    if let Some(i) = clicada {
        estado.abrir_nota(motor, sessao, i);
    }
}

fn dialogo_nova(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    let fechar = Dialogo::nova("Nova nota de entrada")
        .largura(760.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let f = &mut estado.nova;
                ui.columns(2, |c| {
                    c[0].add(
                        Campo::novo("CNPJ do fornecedor", &mut f.cnpj)
                            .mascara(Mascara::Documento)
                            .marcador("00.000.000/0000-00"),
                    );
                    c[1].add(Campo::novo("Razão social", &mut f.nome));
                });
                ui.add_space(Espaco::E8);
                // CNPJ/razão social são obrigatórios de verdade (`lancar`, abaixo, valida o
                // CNPJ e `LancarNotaManual::executar` resolve o fornecedor por ele — não dá
                // pra lançar nota sem saber de quem). Número/série/NCM/código do fornecedor
                // são só texto livre no domínio (`ItemNotaManual`/`LancarNotaManual`) — a
                // cascata de casamento usa o NCM quando ele existe, mas não exige.
                ui.columns(3, |c| {
                    c[0].add(Campo::novo("Número (opcional)", &mut f.numero));
                    c[1].add(Campo::novo("Série (opcional)", &mut f.serie));
                    c[2].add(Campo::novo("Emissão", &mut f.data).mascara(Mascara::Data));
                });
                ui.add_space(Espaco::E8);
                ui.add(Campo::novo("Frete (opcional)", &mut f.frete).marcador("0,00"));

                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Itens"));
                ui.add_space(Espaco::E4);
                let mut remover = None;
                let n_itens = f.itens.len();
                for (idx, it) in f.itens.iter_mut().enumerate() {
                    ui.columns(5, |c| {
                        c[0].add(Campo::novo("Código (opcional)", &mut it.codigo));
                        c[1].add(Campo::novo("Descrição", &mut it.descricao));
                        c[2].add(Campo::novo("NCM (opcional)", &mut it.ncm));
                        c[3].add(Campo::novo("Qtd", &mut it.quantidade));
                        c[4].add(Campo::novo("Vlr unit.", &mut it.valor_unitario));
                    });
                    if n_itens > 1 && ui.add(Botao::fantasma("remover linha")).clicked() {
                        remover = Some(idx);
                    }
                    ui.add_space(Espaco::E8);
                }
                if let Some(idx) = remover {
                    f.itens.remove(idx);
                }
                if ui.add(Botao::secundario("+ Linha")).clicked() {
                    f.itens.push(LinhaItem::default());
                }
            },
            |ui, estado| {
                if ui.add(Botao::primario("Lançar nota")).clicked() {
                    lancar(ui.ctx(), motor, sessao, estado);
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

fn lancar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
) {
    let f = &estado.nova;
    let Ok(data_emissao) = f.data.parse() else {
        notificar(
            ctx,
            Notificacao::aviso("Data de emissão inválida (dd/mm/aaaa)."),
        );
        return;
    };
    let frete = f.frete.parse().unwrap_or(Dinheiro::ZERO);
    let mut itens = Vec::new();
    for it in &f.itens {
        if it.descricao.trim().is_empty() {
            continue;
        }
        let (Ok(quantidade), Ok(valor_unitario)) =
            (it.quantidade.parse(), it.valor_unitario.parse())
        else {
            notificar(
                ctx,
                Notificacao::aviso(format!("Item \"{}\": qtd/valor inválidos.", it.descricao)),
            );
            return;
        };
        itens.push(ItemNotaManual {
            codigo_fornecedor: it.codigo.clone(),
            descricao: it.descricao.clone(),
            ncm: it.ncm.clone(),
            quantidade,
            valor_unitario,
        });
    }
    if itens.is_empty() {
        notificar(ctx, Notificacao::aviso("Adicione ao menos um item."));
        return;
    }

    match motor.executar(
        sessao,
        "compras.lancar_nota_manual.v1",
        &LancarNotaManual {
            fornecedor_cnpj: f.cnpj.clone(),
            fornecedor_nome: f.nome.clone(),
            numero: f.numero.clone(),
            serie: f.serie.clone(),
            data_emissao,
            itens,
            valor_frete: frete,
            valor_seguro: Dinheiro::ZERO,
            valor_outras_despesas: Dinheiro::ZERO,
        },
    ) {
        Ok(_) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Nota lançada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_ver(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
    i: usize,
) {
    let Some(n) = estado.notas.get(i).cloned() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let nome = estado.nome(n.fornecedor);
    let ops_prod: Vec<(Id, String)> = estado
        .produtos
        .iter()
        .map(|p| (p.produto, p.nome.clone()))
        .collect();
    let ops_local: Vec<(Id, String)> = estado
        .locais
        .iter()
        .map(|l| (l.id, l.nome.clone()))
        .collect();

    let fechar = Dialogo::nova(format!("Nota {} · {nome}", n.numero))
        .largura(720.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.columns(2, |c| {
                    kv(&mut c[0], "Série", &n.serie);
                    kv(&mut c[1], "Emissão", &n.data_emissao.to_string());
                });
                ui.columns(2, |c| {
                    kv(&mut c[0], "Total", &n.valor_total.formatar_com_simbolo());
                    kv(&mut c[1], "Estado", n.estado.rotulo());
                });

                ui.add_space(Espaco::E12);
                ui.add(Rotulo::titulo_secao("Itens e casamento"));
                ui.add_space(Espaco::E4);
                let mut vincular: Option<Id> = None;
                for it in estado.itens_nota.clone() {
                    ui.horizontal(|ui| {
                        ui.add(Rotulo::interface(format!(
                            "{} × {}",
                            it.quantidade, it.descricao_fornecedor
                        )));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(Rotulo::campo(match it.estado_casamento {
                                EstadoCasamento::Casado => "casado",
                                EstadoCasamento::SugestaoForte => "sugestão",
                                EstadoCasamento::NaoCasado => "não casado",
                            }));
                        });
                    });
                    if !matches!(it.estado_casamento, EstadoCasamento::Casado) {
                        ui.horizontal(|ui| {
                            let sel = estado.vinc_produto.entry(it.id).or_default();
                            SeletorOpcao::novo("", sel)
                                .opcoes(ops_prod.clone())
                                .placeholder("casar com produto…")
                                .mostrar(ui);
                            if ui.add(Botao::fantasma("vincular")).clicked() {
                                vincular = Some(it.id);
                            }
                        });
                    }
                    ui.add_space(Espaco::E4);
                }
                if let Some(item_nota) = vincular {
                    vincular_item(ui.ctx(), motor, sessao, estado, n.nota, item_nota);
                }

                if matches!(n.estado, EstadoNotaEntrada::Conferida) {
                    ui.add_space(Espaco::E12);
                    ui.separator();
                    ui.add_space(Espaco::E12);
                    ui.add(Rotulo::titulo_secao("Confirmar entrada"));
                    ui.add_space(Espaco::E8);
                    SeletorOpcao::novo("Local que recebe", &mut estado.local_sel)
                        .opcoes(ops_local.clone())
                        .mostrar(ui);
                    ui.add_space(Espaco::E8);
                    if ui.add(Botao::primario("Confirmar entrada")).clicked() {
                        confirmar(ui.ctx(), motor, sessao, estado, n.nota);
                    }
                }
            },
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

fn vincular_item(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
    nota: Id,
    item_nota: Id,
) {
    let Some(Some(produto)) = estado.vinc_produto.get(&item_nota).copied() else {
        notificar(ctx, Notificacao::aviso("Escolha o produto para vincular."));
        return;
    };
    match motor.executar(
        sessao,
        "compras.vincular_produto_manual.v1",
        &VincularProdutoManual { item_nota, produto },
    ) {
        Ok(_) => {
            estado.recarregar_itens(motor, sessao, nota);
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Produto vinculado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn confirmar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaCompras,
    nota: Id,
) {
    let Some(local) = estado.local_sel else {
        notificar(
            ctx,
            Notificacao::aviso("Escolha o local de estoque que recebe."),
        );
        return;
    };
    match motor.executar(
        sessao,
        "compras.confirmar_entrada.v1",
        &ConfirmarEntrada {
            nota_entrada: nota,
            local,
            gerar_titulo_a_pagar: None,
        },
    ) {
        Ok(_) => {
            estado.dlg = Dlg::Fechado;
            estado.carregar(motor, sessao);
            notificar(ctx, Notificacao::sucesso("Entrada confirmada"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn kv(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.add(Rotulo::campo(chave));
    ui.add(Rotulo::interface(if valor.trim().is_empty() {
        "—"
    } else {
        valor
    }));
    ui.add_space(Espaco::E8);
}
