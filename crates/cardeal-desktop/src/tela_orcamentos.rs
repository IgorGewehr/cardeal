//! A aba **Orçamentos** da tela de Ordens de Serviço — o gestor de orçamentos comerciais.
//! `docs/modulos/orcamentos.md`.
//!
//! Lista com KPIs e filtros (estado, cliente, busca, período) + um `Dialogo` para
//! criar/editar/ver-e-agir. "Gerar PDF" monta o documento com `cardeal-pdf` e a identidade
//! da empresa (`MotorLocal::identidade_visual`).

use cardeal_cliente::{IdentidadeVisual, MotorLocal, SessaoLocal};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante, Percentual, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_pdf::{gerar_orcamento, IdentidadeEmpresa, ItemPdf, OrcamentoPdf};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom, ValorDinheiro};
use cardeal_ui::molecules::{Campo, EstadoVazio, SeletorOpcao};
use cardeal_ui::organisms::{notificar, ColunaGrade, Dialogo, Grade, Notificacao};
use cardeal_ui::tokens::{Espaco, Raio, TemaUi};
use eframe::egui;
use mod_clientes::{ItemPessoa, Papel as PapelCliente, PessoasPorPapel};
use mod_orcamentos::{
    BuscarOrcamento, CancelarOrcamento, ConverterOrcamentoEmOs, CriarOrcamento,
    DefinirItensOrcamento, DetalheOrcamento, DuplicarOrcamento, EditarOrcamento, EnviarOrcamento,
    EstadoOrcamento, ItemOrcamentoLista, NovoItemOrcamento, OrcamentoConvertido, OrcamentoCriado,
    OrcamentosRecentes, RegistrarDecisaoOrcamento, ResumoOrcamentos, ResumoOrcamentosSaida,
};

// ── Formulário ───────────────────────────────────────────────────────────────

#[derive(Default)]
struct LinhaItem {
    descricao: String,
    qtd: String,
    unidade: String,
    preco: String,
    desconto: String,
}

impl LinhaItem {
    fn vazia() -> Self {
        Self {
            qtd: "1".to_owned(),
            unidade: "un".to_owned(),
            preco: String::new(),
            desconto: String::new(),
            descricao: String::new(),
        }
    }
}

struct FormOrcamento {
    editar: Option<Id>,
    cliente_avulso: bool,
    cliente_sel: Option<Id>,
    cliente_nome: String,
    cliente_doc: String,
    cliente_contato: String,
    assunto: String,
    descricao: String,
    validade_dias: String,
    cond_pagamento: String,
    prazo_entrega: String,
    observacoes: String,
    desconto: String,
    itens: Vec<LinhaItem>,
}

impl FormOrcamento {
    fn novo() -> Self {
        Self {
            editar: None,
            cliente_avulso: false,
            cliente_sel: None,
            cliente_nome: String::new(),
            cliente_doc: String::new(),
            cliente_contato: String::new(),
            assunto: String::new(),
            descricao: String::new(),
            validade_dias: "15".to_owned(),
            cond_pagamento: String::new(),
            prazo_entrega: String::new(),
            observacoes: String::new(),
            desconto: String::new(),
            itens: vec![LinhaItem::vazia()],
        }
    }

    fn de_detalhe(d: &DetalheOrcamento) -> Self {
        let o = &d.orcamento;
        Self {
            editar: Some(o.id),
            cliente_avulso: o.cliente.is_none(),
            cliente_sel: o.cliente,
            cliente_nome: o.cliente_nome.clone(),
            cliente_doc: o.cliente_documento.clone().unwrap_or_default(),
            cliente_contato: o.cliente_contato.clone().unwrap_or_default(),
            assunto: o.assunto.clone(),
            descricao: o.descricao.clone().unwrap_or_default(),
            validade_dias: o.data_emissao.dias_ate(o.validade).max(0).to_string(),
            cond_pagamento: o.condicoes_pagamento.clone().unwrap_or_default(),
            prazo_entrega: o.prazo_entrega.clone().unwrap_or_default(),
            observacoes: o.observacoes.clone().unwrap_or_default(),
            desconto: if o.desconto_percentual.e_zero() {
                String::new()
            } else {
                o.desconto_percentual.formatar(2)
            },
            itens: d
                .itens
                .iter()
                .map(|i| LinhaItem {
                    descricao: i.descricao.clone(),
                    qtd: i.quantidade.formatar(0),
                    unidade: i.unidade.clone(),
                    preco: i.preco_unitario.formatar(),
                    desconto: if i.desconto_percentual.e_zero() {
                        String::new()
                    } else {
                        i.desconto_percentual.formatar(2)
                    },
                })
                .collect(),
        }
    }
}

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Form(Box<FormOrcamento>),
    Detalhe,
    Decisao {
        aprovado: bool,
        identificacao: String,
    },
    Converter {
        garantia: String,
    },
}

// ── Estado da aba ────────────────────────────────────────────────────────────

/// Estado local da aba de orçamentos — sobrevive entre quadros.
#[derive(Default)]
pub struct EstadoOrcamentos {
    lista: Vec<ItemOrcamentoLista>,
    resumo: Option<ResumoOrcamentosSaida>,
    clientes: Vec<ItemPessoa>,
    identidade: Option<IdentidadeVisual>,
    detalhe: Option<DetalheOrcamento>,
    erro: Option<String>,
    f_estado: Option<EstadoOrcamento>,
    f_cliente: Option<Id>,
    f_texto: String,
    f_dias: i64,
    dlg: Dlg,
}

impl EstadoOrcamentos {
    /// Recarrega a lista (com os filtros atuais), os KPIs e os catálogos.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let hoje = Data::hoje(Fuso::BRASILIA);
        let consulta = OrcamentosRecentes {
            estado: self.f_estado,
            cliente: self.f_cliente,
            texto: (!self.f_texto.trim().is_empty()).then(|| self.f_texto.trim().to_owned()),
            desde: (self.f_dias > 0)
                .then(|| hoje.mais_dias(-i32::try_from(self.f_dias).unwrap_or(0))),
            ate: None,
        };
        match motor.consultar(sessao, "orcamentos.orcamentos_recentes.v1", &consulta) {
            Ok(v) => {
                self.lista = v;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(r) = motor.consultar(sessao, "orcamentos.resumo.v1", &ResumoOrcamentos) {
            self.resumo = Some(r);
        }
        if let Ok(c) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: PapelCliente::Cliente,
                busca: None,
            },
        ) {
            self.clientes = c;
        }
        if let Ok(i) = motor.identidade_visual() {
            self.identidade = Some(i);
        }
    }

    fn abrir_detalhe(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, id: Id) {
        match motor.consultar(
            sessao,
            "orcamentos.buscar_orcamento.v1",
            &BuscarOrcamento { orcamento: id },
        ) {
            Ok(Some(d)) => {
                self.detalhe = Some(d);
                self.dlg = Dlg::Detalhe;
            }
            Ok(None) => self.erro = Some("Orçamento não encontrado.".to_owned()),
            Err(e) => self.erro = Some(e.mensagem),
        }
    }
}

/// Prepara o dialog de "novo orçamento" (chamado pelo botão do cabeçalho da tela de OS).
pub fn abrir_novo(estado: &mut EstadoOrcamentos) {
    estado.dlg = Dlg::Form(Box::new(FormOrcamento::novo()));
}

// ── Corpo (dentro do LayoutTela da tela de OS) ───────────────────────────────

/// Desenha o miolo da aba: KPIs + filtros + grade.
pub fn corpo(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
) {
    if let Some(erro) = &estado.erro {
        ui.add(
            Rotulo::interface(erro.clone())
                .quebravel()
                .cor(ui.cores().negativo),
        );
        ui.add_space(Espaco::E12);
    }

    kpis(ui, estado);
    ui.add_space(Espaco::E16);
    filtros(ui, motor, sessao, estado);
    ui.add_space(Espaco::E12);

    if estado.lista.is_empty() {
        if EstadoVazio::novo(Icone::Nota, "Nenhum orçamento com esses filtros.")
            .acao("Criar o primeiro orçamento")
            .mostrar(ui)
        {
            abrir_novo(estado);
        }
        return;
    }

    let hoje = Data::hoje(Fuso::BRASILIA);
    let colunas = vec![
        ColunaGrade::nova("Nº").largura(52.0).numero(),
        ColunaGrade::nova("Cliente"),
        ColunaGrade::nova("Assunto"),
        ColunaGrade::nova("Emissão").largura(92.0),
        ColunaGrade::nova("Validade").largura(92.0),
        ColunaGrade::nova("Total").largura(120.0).numero(),
        ColunaGrade::nova("Estado").largura(140.0),
    ];
    let resposta =
        Grade::nova(colunas)
            .selecionavel(None)
            .mostrar(ui, estado.lista.len(), |i, row| {
                let o = &estado.lista[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(format!("{:04}", o.numero)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(o.cliente_nome.clone()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(o.assunto.clone()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::campo(o.data_emissao.formatar_curta()));
                });
                row.col(|ui| {
                    let r = Rotulo::campo(o.validade.formatar_curta());
                    ui.add(if o.validade < hoje && o.vencido {
                        r.cor(ui.cores().negativo)
                    } else {
                        r
                    });
                });
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(o.total));
                });
                row.col(|ui| {
                    ui.add(etiqueta_estado(o.estado, o.vencido));
                });
            });
    if let Some(i) = resposta.linha_clicada {
        let id = estado.lista[i].orcamento;
        estado.abrir_detalhe(motor, sessao, id);
    }
}

/// Desenha os dialogs da aba (chamado depois do `LayoutTela`, com `ui.ctx()`).
pub fn dialogos(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
) {
    match estado.dlg {
        Dlg::Fechado => {}
        Dlg::Form(_) => dialogo_form(ctx, motor, sessao, estado),
        Dlg::Detalhe => dialogo_detalhe(ctx, motor, sessao, estado),
        Dlg::Decisao { .. } => dialogo_decisao(ctx, motor, sessao, estado),
        Dlg::Converter { .. } => dialogo_converter(ctx, motor, sessao, estado),
    }
}

// ── KPIs ─────────────────────────────────────────────────────────────────────

fn kpis(ui: &mut egui::Ui, estado: &EstadoOrcamentos) {
    let Some(r) = &estado.resumo else { return };
    let cores = ui.cores();
    ui.horizontal_wrapped(|ui| {
        kpi(
            ui,
            "Em aberto",
            &format!("{}", r.abertos),
            cores.texto_forte,
            Some(&r.valor_aberto.formatar_com_simbolo()),
        );
        kpi(
            ui,
            "Aprovados (30 dias)",
            &format!("{}", r.aprovados_periodo),
            cores.positivo,
            Some(&r.valor_aprovado_periodo.formatar_com_simbolo()),
        );
        kpi(
            ui,
            "Taxa de conversão",
            &format!("{}%", r.taxa_conversao.formatar(0)),
            cores.info,
            Some("aprovados ÷ decididos"),
        );
    });
}

fn kpi(ui: &mut egui::Ui, rotulo: &str, valor: &str, cor: egui::Color32, apoio: Option<&str>) {
    let cores = ui.cores();
    egui::Frame::none()
        .fill(cores.superficie)
        .stroke(egui::Stroke::new(1.0_f32, cores.borda))
        .rounding(Raio::CARTAO)
        .inner_margin(Espaco::E16)
        .show(ui, |ui| {
            ui.set_width(196.0);
            ui.vertical(|ui| {
                ui.add(Rotulo::campo(rotulo.to_uppercase()));
                ui.add_space(Espaco::E4);
                ui.add(Rotulo::titulo_secao(valor.to_owned()).cor(cor));
                if let Some(a) = apoio {
                    ui.add_space(Espaco::E4);
                    ui.add(Rotulo::campo(a.to_owned()));
                }
            });
        });
}

// ── Filtros ──────────────────────────────────────────────────────────────────

const ESTADOS_FILTRO: [(Option<EstadoOrcamento>, &str); 7] = [
    (None, "Todos"),
    (Some(EstadoOrcamento::Rascunho), "Rascunho"),
    (Some(EstadoOrcamento::Enviado), "Enviado"),
    (Some(EstadoOrcamento::Aprovado), "Aprovado"),
    (Some(EstadoOrcamento::Recusado), "Recusado"),
    (Some(EstadoOrcamento::Expirado), "Expirado"),
    (Some(EstadoOrcamento::Convertido), "Convertido"),
];

fn filtros(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
) {
    let cores = ui.cores();
    egui::Frame::none()
        .fill(cores.superficie_2)
        .rounding(Raio::ITEM)
        .inner_margin(Espaco::E8)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (valor, rot) in ESTADOS_FILTRO {
                    let sel = estado.f_estado == valor;
                    let b = if sel {
                        Botao::primario(rot).pequeno()
                    } else {
                        Botao::fantasma(rot).pequeno()
                    };
                    if ui.add(b).clicked() && !sel {
                        estado.f_estado = valor;
                        estado.carregar(motor, sessao);
                    }
                }
                ui.separator();
                for (d, rot) in [(0_i64, "Sempre"), (30, "30 d"), (90, "90 d"), (365, "12 m")] {
                    let sel = estado.f_dias == d;
                    let b = if sel {
                        Botao::primario(rot).pequeno()
                    } else {
                        Botao::fantasma(rot).pequeno()
                    };
                    if ui.add(b).clicked() && !sel {
                        estado.f_dias = d;
                        estado.carregar(motor, sessao);
                    }
                }
            });
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                let ops: Vec<(Id, String)> = estado
                    .clientes
                    .iter()
                    .map(|c| (c.pessoa, c.nome.clone()))
                    .collect();
                ui.allocate_ui(egui::vec2(260.0, 40.0), |ui| {
                    let antes = estado.f_cliente;
                    SeletorOpcao::novo("Cliente", &mut estado.f_cliente)
                        .opcoes(ops)
                        .placeholder("Todos os clientes")
                        .mostrar(ui);
                    if estado.f_cliente != antes {
                        estado.carregar(motor, sessao);
                    }
                });
                ui.add_space(Espaco::E8);
                ui.allocate_ui(egui::vec2(280.0, 40.0), |ui| {
                    let resp = ui.add(Campo::novo(
                        "Busca (assunto ou cliente)",
                        &mut estado.f_texto,
                    ));
                    if resp.lost_focus() || resp.changed() {
                        estado.carregar(motor, sessao);
                    }
                });
            });
        });
}

// ── Dialog: formulário (novo / editar) ───────────────────────────────────────

fn dialogo_form(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
) {
    let ops_cli: Vec<(Id, String)> = estado
        .clientes
        .iter()
        .map(|c| (c.pessoa, c.nome.clone()))
        .collect();
    let titulo = match &estado.dlg {
        Dlg::Form(f) if f.editar.is_some() => "Editar orçamento",
        _ => "Novo orçamento",
    };

    let fechar = Dialogo::nova(titulo).largura(820.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Form(f) = &mut estado.dlg else {
                return;
            };

            ui.horizontal(|ui| {
                let existente = if f.cliente_avulso {
                    Botao::fantasma("Cliente cadastrado")
                } else {
                    Botao::primario("Cliente cadastrado")
                };
                if ui.add(existente).clicked() {
                    f.cliente_avulso = false;
                }
                let avulso = if f.cliente_avulso {
                    Botao::primario("Cliente avulso")
                } else {
                    Botao::fantasma("Cliente avulso")
                };
                if ui.add(avulso).clicked() {
                    f.cliente_avulso = true;
                }
            });
            ui.add_space(Espaco::E8);
            if f.cliente_avulso {
                ui.columns(3, |c| {
                    c[0].add(Campo::novo("Nome do cliente", &mut f.cliente_nome));
                    c[1].add(Campo::novo("Documento (opcional)", &mut f.cliente_doc));
                    c[2].add(Campo::novo("Contato (opcional)", &mut f.cliente_contato));
                });
            } else {
                SeletorOpcao::novo("Cliente", &mut f.cliente_sel)
                    .opcoes(ops_cli.clone())
                    .placeholder("Buscar cliente cadastrado…")
                    .mostrar(ui);
            }
            ui.add_space(Espaco::E12);

            ui.add(
                Campo::novo("Assunto", &mut f.assunto).marcador("ex.: Reforma do motor elétrico"),
            );
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo(
                "Descrição / escopo (opcional)",
                &mut f.descricao,
            ));
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Validade (dias)", &mut f.validade_dias));
                c[1].add(
                    Campo::novo("Desconto geral (%) (opcional)", &mut f.desconto).marcador("0"),
                );
            });
            ui.add_space(Espaco::E8);
            ui.columns(2, |c| {
                c[0].add(Campo::novo(
                    "Condições de pagamento (opcional)",
                    &mut f.cond_pagamento,
                ));
                c[1].add(Campo::novo(
                    "Prazo de entrega (opcional)",
                    &mut f.prazo_entrega,
                ));
            });
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Observações (opcional)", &mut f.observacoes));

            ui.add_space(Espaco::E16);
            ui.add(Rotulo::titulo_secao("Itens"));
            ui.add_space(Espaco::E8);
            editor_itens(ui, f);

            ui.add_space(Espaco::E12);
            let (subtotal, total) = previsao_totais(f);
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Subtotal"));
                ui.add(ValorDinheiro::novo(subtotal));
                ui.add_space(Espaco::E16);
                ui.add(Rotulo::campo("Total estimado"));
                ui.add(ValorDinheiro::novo(total).destaque());
            });
        },
        |ui, estado| {
            if ui.add(Botao::primario("Salvar")).clicked() {
                salvar_form(ui.ctx(), motor, sessao, estado);
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

fn editor_itens(ui: &mut egui::Ui, f: &mut FormOrcamento) {
    let mut remover: Option<usize> = None;
    for (i, linha) in f.itens.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.allocate_ui(egui::vec2(300.0, 40.0), |ui| {
                ui.add(Campo::novo(
                    if i == 0 { "Descrição" } else { "" },
                    &mut linha.descricao,
                ));
            });
            ui.allocate_ui(egui::vec2(58.0, 40.0), |ui| {
                ui.add(Campo::novo(if i == 0 { "Qtd" } else { "" }, &mut linha.qtd));
            });
            ui.allocate_ui(egui::vec2(52.0, 40.0), |ui| {
                ui.add(Campo::novo(
                    if i == 0 { "Un" } else { "" },
                    &mut linha.unidade,
                ));
            });
            ui.allocate_ui(egui::vec2(96.0, 40.0), |ui| {
                ui.add(Campo::novo(
                    if i == 0 { "Preço unit." } else { "" },
                    &mut linha.preco,
                ));
            });
            ui.allocate_ui(egui::vec2(64.0, 40.0), |ui| {
                ui.add(Campo::novo(
                    if i == 0 { "Desc.%" } else { "" },
                    &mut linha.desconto,
                ));
            });
            if ui.add(Botao::fantasma("✕").pequeno()).clicked() {
                remover = Some(i);
            }
        });
        ui.add_space(Espaco::E4);
    }
    if let Some(i) = remover {
        if f.itens.len() > 1 {
            f.itens.remove(i);
        } else if let Some(l) = f.itens.get_mut(i) {
            *l = LinhaItem::vazia();
        }
    }
    if ui.add(Botao::secundario("+ Adicionar item")).clicked() {
        f.itens.push(LinhaItem::vazia());
    }
}

fn previsao_totais(f: &FormOrcamento) -> (Dinheiro, Dinheiro) {
    let mut subtotal = Dinheiro::ZERO;
    let mut liquido = Dinheiro::ZERO;
    for l in &f.itens {
        let (Ok(qtd), Ok(preco)) = (
            l.qtd.trim().parse::<Quantidade>(),
            l.preco.trim().parse::<Preco>(),
        ) else {
            continue;
        };
        let bruto = Dinheiro::de_total(qtd, preco, cardeal_kernel::Arredondamento::MeioAcima);
        let desc = l
            .desconto
            .trim()
            .parse::<Percentual>()
            .unwrap_or(Percentual::ZERO);
        subtotal += bruto;
        liquido += bruto - bruto.aplicar(desc, cardeal_kernel::Arredondamento::MeioAcima);
    }
    let desc_cab = f
        .desconto
        .trim()
        .parse::<Percentual>()
        .unwrap_or(Percentual::ZERO);
    let total = liquido - liquido.aplicar(desc_cab, cardeal_kernel::Arredondamento::MeioAcima);
    (subtotal, total)
}

fn coletar_itens(f: &FormOrcamento) -> Result<Vec<NovoItemOrcamento>, String> {
    let mut itens = Vec::new();
    for (i, l) in f.itens.iter().enumerate() {
        if l.descricao.trim().is_empty() && l.preco.trim().is_empty() {
            continue;
        }
        let qtd = l
            .qtd
            .trim()
            .parse::<Quantidade>()
            .map_err(|_| format!("Item {}: quantidade inválida.", i + 1))?;
        let preco = l
            .preco
            .trim()
            .parse::<Preco>()
            .map_err(|_| format!("Item {}: preço inválido.", i + 1))?;
        let desconto = if l.desconto.trim().is_empty() {
            Percentual::ZERO
        } else {
            l.desconto
                .trim()
                .parse::<Percentual>()
                .map_err(|_| format!("Item {}: desconto inválido.", i + 1))?
        };
        itens.push(NovoItemOrcamento {
            descricao: l.descricao.trim().to_owned(),
            quantidade: qtd,
            unidade: l.unidade.trim().to_owned(),
            preco_unitario: preco,
            desconto_percentual: desconto,
        });
    }
    Ok(itens)
}

fn salvar_form(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
) {
    let Dlg::Form(f) = &estado.dlg else { return };

    let cliente = if f.cliente_avulso {
        None
    } else {
        f.cliente_sel
    };
    let cliente_nome = if f.cliente_avulso {
        f.cliente_nome.trim().to_owned()
    } else {
        estado
            .clientes
            .iter()
            .find(|c| Some(c.pessoa) == f.cliente_sel)
            .map(|c| c.nome.clone())
            .unwrap_or_default()
    };
    if cliente_nome.is_empty() {
        notificar(
            ctx,
            Notificacao::aviso("Escolha um cliente ou informe o nome."),
        );
        return;
    }
    if f.assunto.trim().is_empty() {
        notificar(ctx, Notificacao::aviso("Informe o assunto do orçamento."));
        return;
    }
    let Ok(validade_dias) = f.validade_dias.trim().parse::<u16>() else {
        notificar(ctx, Notificacao::aviso("Validade em dias inválida."));
        return;
    };
    let desconto = if f.desconto.trim().is_empty() {
        Percentual::ZERO
    } else {
        match f.desconto.trim().parse::<Percentual>() {
            Ok(p) => p,
            Err(_) => {
                notificar(ctx, Notificacao::aviso("Desconto geral inválido."));
                return;
            }
        }
    };
    let itens = match coletar_itens(f) {
        Ok(v) => v,
        Err(msg) => {
            notificar(ctx, Notificacao::aviso(msg));
            return;
        }
    };

    let doc = (!f.cliente_doc.trim().is_empty()).then(|| f.cliente_doc.trim().to_owned());
    let contato =
        (!f.cliente_contato.trim().is_empty()).then(|| f.cliente_contato.trim().to_owned());
    let descricao = (!f.descricao.trim().is_empty()).then(|| f.descricao.trim().to_owned());
    let cond = (!f.cond_pagamento.trim().is_empty()).then(|| f.cond_pagamento.trim().to_owned());
    let prazo = (!f.prazo_entrega.trim().is_empty()).then(|| f.prazo_entrega.trim().to_owned());
    let obs = (!f.observacoes.trim().is_empty()).then(|| f.observacoes.trim().to_owned());

    let assunto = f.assunto.trim().to_owned();
    let resultado: Result<Id, String> = if let Some(id) = f.editar {
        let editar = EditarOrcamento {
            orcamento: id,
            cliente,
            cliente_nome,
            cliente_documento: doc,
            cliente_contato: contato,
            assunto,
            descricao,
            validade_dias,
            condicoes_pagamento: cond,
            prazo_entrega: prazo,
            observacoes: obs,
            desconto_percentual: desconto,
        };
        let passo = motor
            .executar(sessao, "orcamentos.editar_orcamento.v1", &editar)
            .map_err(|e| e.mensagem);
        match passo {
            Ok(()) => motor
                .executar(
                    sessao,
                    "orcamentos.definir_itens.v1",
                    &DefinirItensOrcamento {
                        orcamento: id,
                        itens,
                    },
                )
                .map(|()| id)
                .map_err(|e| e.mensagem),
            Err(msg) => Err(msg),
        }
    } else {
        motor
            .executar(
                sessao,
                "orcamentos.criar_orcamento.v1",
                &CriarOrcamento {
                    cliente,
                    cliente_nome,
                    cliente_documento: doc,
                    cliente_contato: contato,
                    assunto,
                    descricao,
                    validade_dias,
                    condicoes_pagamento: cond,
                    prazo_entrega: prazo,
                    observacoes: obs,
                    desconto_percentual: desconto,
                    itens,
                },
            )
            .map(|c: OrcamentoCriado| c.orcamento)
            .map_err(|e| e.mensagem)
    };

    match resultado {
        Ok(id) => {
            estado.carregar(motor, sessao);
            estado.abrir_detalhe(motor, sessao, id);
            notificar(ctx, Notificacao::sucesso("Orçamento salvo"));
        }
        Err(msg) => notificar(ctx, Notificacao::erro(msg)),
    }
}

// ── Dialog: detalhe + ações ──────────────────────────────────────────────────

fn dialogo_detalhe(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
) {
    let Some(detalhe) = estado.detalhe.clone() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let o = detalhe.orcamento.clone();
    let titulo = format!("Orçamento {:04} · {}", o.numero, o.cliente_nome);

    let fechar = Dialogo::nova(titulo).largura(760.0).mostrar(
        ctx,
        estado,
        |ui, _estado| corpo_detalhe(ui, &detalhe),
        |ui, estado| acoes_detalhe(ui, motor, sessao, estado, &detalhe),
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
        estado.detalhe = None;
    }
}

fn corpo_detalhe(ui: &mut egui::Ui, d: &DetalheOrcamento) {
    let o = &d.orcamento;
    ui.horizontal(|ui| {
        ui.add(etiqueta_estado(o.estado, false));
        ui.add_space(Espaco::E12);
        ui.add(Rotulo::campo(format!(
            "Emissão {} · válido até {}",
            o.data_emissao.formatar(),
            o.validade.formatar()
        )));
    });
    ui.add_space(Espaco::E12);

    kv(ui, "Cliente", &o.cliente_nome);
    if let Some(doc) = &o.cliente_documento {
        kv(ui, "Documento", doc);
    }
    if let Some(c) = &o.cliente_contato {
        kv(ui, "Contato", c);
    }
    kv(ui, "Assunto", &o.assunto);
    if let Some(desc) = &o.descricao {
        kv(ui, "Descrição", desc);
    }
    ui.add_space(Espaco::E12);

    ui.add(Rotulo::titulo_secao("Itens"));
    ui.add_space(Espaco::E4);
    let colunas = vec![
        ColunaGrade::nova("Descrição"),
        ColunaGrade::nova("Qtd").largura(50.0).numero(),
        ColunaGrade::nova("Un").largura(40.0),
        ColunaGrade::nova("Vl unit.").largura(96.0).numero(),
        ColunaGrade::nova("Total").largura(110.0).numero(),
    ];
    Grade::nova(colunas).mostrar(ui, d.itens.len(), |i, row| {
        let it = &d.itens[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(it.descricao.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(it.quantidade.formatar(0)));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(it.unidade.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(it.preco_unitario.formatar_com_simbolo()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(it.total));
        });
    });
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Subtotal"));
        ui.add(ValorDinheiro::novo(d.subtotal));
        ui.add_space(Espaco::E16);
        if !d.desconto.e_zero() {
            ui.add(Rotulo::campo("Desconto"));
            ui.add(ValorDinheiro::novo(d.desconto));
            ui.add_space(Espaco::E16);
        }
        ui.add(Rotulo::titulo_secao("Total"));
        ui.add(ValorDinheiro::novo(d.total).destaque());
    });

    for (rot, txt) in [
        ("Condições de pagamento", &o.condicoes_pagamento),
        ("Prazo de entrega", &o.prazo_entrega),
        ("Observações", &o.observacoes),
    ] {
        if let Some(t) = txt {
            ui.add_space(Espaco::E8);
            kv(ui, rot, t);
        }
    }
    if let Some(os) = o.os_gerada {
        ui.add_space(Espaco::E8);
        kv(ui, "Convertido na OS", &format!("{os}"));
    }
}

fn acoes_detalhe(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
    d: &DetalheOrcamento,
) {
    let id = d.orcamento.id;
    let estado_atual = d.orcamento.estado;

    // À direita, da direita para a esquerda: Fechar sempre; depois PDF; depois as ações de estado.
    if ui.add(Botao::secundario("Fechar")).clicked() {
        estado.dlg = Dlg::Fechado;
        estado.detalhe = None;
    }
    if ui.add(Botao::secundario("Gerar PDF")).clicked() {
        gerar_pdf(ui.ctx(), estado, d);
    }
    if ui.add(Botao::fantasma("Duplicar")).clicked() {
        match motor
            .executar(
                sessao,
                "orcamentos.duplicar_orcamento.v1",
                &DuplicarOrcamento { orcamento: id },
            )
            .map(|c: OrcamentoCriado| c.orcamento)
        {
            Ok(novo) => {
                estado.carregar(motor, sessao);
                estado.abrir_detalhe(motor, sessao, novo);
                notificar(ui.ctx(), Notificacao::sucesso("Orçamento duplicado"));
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }

    match estado_atual {
        EstadoOrcamento::Rascunho => {
            if ui.add(Botao::destrutivo("Cancelar orçamento")).clicked() {
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    id,
                    "orcamentos.cancelar_orcamento.v1",
                    &CancelarOrcamento { orcamento: id },
                    "Orçamento cancelado",
                );
            }
            if ui.add(Botao::secundario("Editar")).clicked() {
                estado.dlg = Dlg::Form(Box::new(FormOrcamento::de_detalhe(d)));
            }
            if ui.add(Botao::primario("Enviar ao cliente")).clicked() {
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    id,
                    "orcamentos.enviar_orcamento.v1",
                    &EnviarOrcamento { orcamento: id },
                    "Orçamento enviado",
                );
            }
        }
        EstadoOrcamento::Enviado => {
            if ui.add(Botao::secundario("Editar")).clicked() {
                estado.dlg = Dlg::Form(Box::new(FormOrcamento::de_detalhe(d)));
            }
            if ui.add(Botao::destrutivo("Recusado")).clicked() {
                estado.dlg = Dlg::Decisao {
                    aprovado: false,
                    identificacao: String::new(),
                };
            }
            if ui.add(Botao::primario("Registrar aprovação")).clicked() {
                estado.dlg = Dlg::Decisao {
                    aprovado: true,
                    identificacao: String::new(),
                };
            }
        }
        EstadoOrcamento::Aprovado => {
            if ui.add(Botao::destrutivo("Cancelar")).clicked() {
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    id,
                    "orcamentos.cancelar_orcamento.v1",
                    &CancelarOrcamento { orcamento: id },
                    "Orçamento cancelado",
                );
            }
            if ui.add(Botao::primario("Converter em OS")).clicked() {
                estado.dlg = Dlg::Converter {
                    garantia: "90".to_owned(),
                };
            }
        }
        EstadoOrcamento::Recusado
        | EstadoOrcamento::Expirado
        | EstadoOrcamento::Cancelado
        | EstadoOrcamento::Convertido => {}
    }
}

fn dialogo_decisao(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
) {
    let (aprovado, id) = match (&estado.dlg, &estado.detalhe) {
        (Dlg::Decisao { aprovado, .. }, Some(d)) => (*aprovado, d.orcamento.id),
        _ => {
            estado.dlg = Dlg::Fechado;
            return;
        }
    };
    let titulo = if aprovado {
        "Registrar aprovação"
    } else {
        "Registrar recusa"
    };
    let fechar = Dialogo::nova(titulo).largura(520.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Decisao { identificacao, .. } = &mut estado.dlg else { return };
            if aprovado {
                ui.add(Rotulo::campo(
                    "A aprovação nunca é implícita — registre quem aprovou (nome e documento, ou \"assinatura em anexo\").",
                ));
                ui.add_space(Espaco::E8);
                ui.add(Campo::novo("Quem aprovou", identificacao));
            } else {
                ui.add(Rotulo::interface("Confirmar que o cliente recusou este orçamento?"));
            }
        },
        |ui, estado| {
            let rot = if aprovado { "Confirmar aprovação" } else { "Confirmar recusa" };
            if ui.add(Botao::primario(rot)).clicked() {
                let identificacao = match &estado.dlg {
                    Dlg::Decisao { identificacao, .. } => {
                        (!identificacao.trim().is_empty()).then(|| identificacao.trim().to_owned())
                    }
                    _ => None,
                };
                if aprovado && identificacao.is_none() {
                    notificar(ui.ctx(), Notificacao::aviso("Informe quem aprovou."));
                    return;
                }
                aplicar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    id,
                    "orcamentos.registrar_decisao.v1",
                    &RegistrarDecisaoOrcamento { orcamento: id, aprovado, identificacao },
                    if aprovado { "Orçamento aprovado" } else { "Orçamento recusado" },
                );
            }
            if ui.add(Botao::secundario("Voltar")).clicked() {
                estado.dlg = Dlg::Detalhe;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Detalhe;
    }
}

fn dialogo_converter(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
) {
    let id = match &estado.detalhe {
        Some(d) => d.orcamento.id,
        None => {
            estado.dlg = Dlg::Fechado;
            return;
        }
    };
    let fechar = Dialogo::nova("Converter orçamento em OS").largura(520.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Converter { garantia } = &mut estado.dlg else { return };
            ui.add(Rotulo::campo(
                "Abre uma ordem de serviço para este cliente, com o assunto como equipamento e os \
                 itens do orçamento lançados como mão de obra. Você segue montando o restante na OS.",
            ));
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Prazo de garantia (dias)", garantia));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Converter")).clicked() {
                let garantia = match &estado.dlg {
                    Dlg::Converter { garantia } => garantia.trim().parse::<u16>().unwrap_or(90),
                    _ => 90,
                };
                match motor
                    .executar(
                        sessao,
                        "orcamentos.converter_em_os.v1",
                        &ConverterOrcamentoEmOs {
                            orcamento: id,
                            tecnico_responsavel: sessao.usuario(),
                            garantia_dias: garantia,
                        },
                    )
                    .map(|c: OrcamentoConvertido| c.numero)
                {
                    Ok(numero) => {
                        estado.dlg = Dlg::Fechado;
                        estado.detalhe = None;
                        estado.carregar(motor, sessao);
                        notificar(
                            ui.ctx(),
                            Notificacao::sucesso(format!("OS #{numero} criada a partir do orçamento")),
                        );
                    }
                    Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                }
            }
            if ui.add(Botao::secundario("Voltar")).clicked() {
                estado.dlg = Dlg::Detalhe;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Detalhe;
    }
}

// ── PDF ──────────────────────────────────────────────────────────────────────

fn gerar_pdf(ctx: &egui::Context, estado: &EstadoOrcamentos, d: &DetalheOrcamento) {
    let ident = estado.identidade.clone().unwrap_or_default();
    let empresa = IdentidadeEmpresa {
        nome_fantasia: ident.nome_fantasia,
        razao_social: ident.razao_social,
        cnpj: ident.cnpj,
        endereco: ident.endereco,
        telefone: ident.telefone,
        email: ident.email,
        site: ident.site,
        logo_png: ident.logo_png,
    };
    let o = &d.orcamento;
    let doc = OrcamentoPdf {
        numero: o.numero,
        cliente_nome: o.cliente_nome.clone(),
        cliente_documento: o.cliente_documento.clone().unwrap_or_default(),
        cliente_contato: o.cliente_contato.clone().unwrap_or_default(),
        assunto: o.assunto.clone(),
        descricao: o.descricao.clone().unwrap_or_default(),
        data_emissao: o.data_emissao,
        validade: o.validade,
        condicoes_pagamento: o.condicoes_pagamento.clone().unwrap_or_default(),
        prazo_entrega: o.prazo_entrega.clone().unwrap_or_default(),
        observacoes: o.observacoes.clone().unwrap_or_default(),
        responsavel: String::new(),
        itens: d
            .itens
            .iter()
            .map(|i| ItemPdf {
                descricao: i.descricao.clone(),
                quantidade: i.quantidade,
                unidade: i.unidade.clone(),
                preco_unitario: i.preco_unitario,
                desconto_pct: i.desconto_percentual,
                total: i.total,
            })
            .collect(),
        subtotal: d.subtotal,
        desconto: d.desconto,
        total: d.total,
    };

    let bytes = match gerar_orcamento(&empresa, &doc, Instante::agora()) {
        Ok(b) => b,
        Err(e) => {
            notificar(ctx, Notificacao::erro(format!("PDF: {e}")));
            return;
        }
    };
    let nome = format!("Orcamento-{:04}.pdf", o.numero);
    let Some(caminho) = rfd::FileDialog::new()
        .set_file_name(&nome)
        .add_filter("PDF", &["pdf"])
        .save_file()
    else {
        return;
    };
    match std::fs::write(&caminho, &bytes) {
        Ok(()) => {
            let _ = open::that_detached(&caminho);
            notificar(
                ctx,
                Notificacao::sucesso(format!("PDF salvo em {}", caminho.display())),
            );
        }
        Err(e) => notificar(
            ctx,
            Notificacao::erro(format!("Não foi possível salvar: {e}")),
        ),
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn aplicar<C>(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoOrcamentos,
    id: Id,
    nome: &str,
    comando: &C,
    sucesso: &str,
) where
    C: cardeal_modkit::Comando + serde::Serialize,
    C::Saida: serde::de::DeserializeOwned,
{
    match motor.executar(sessao, nome, comando) {
        Ok(_) => {
            estado.carregar(motor, sessao);
            estado.abrir_detalhe(motor, sessao, id);
            notificar(ctx, Notificacao::sucesso(sucesso.to_owned()));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn etiqueta_estado(e: EstadoOrcamento, vencido: bool) -> Etiqueta {
    if vencido && matches!(e, EstadoOrcamento::Rascunho | EstadoOrcamento::Enviado) {
        return Etiqueta::nova("Vencido", Tom::Atencao);
    }
    match e {
        EstadoOrcamento::Rascunho => Etiqueta::nova("Rascunho", Tom::Neutro),
        EstadoOrcamento::Enviado => Etiqueta::nova("Enviado", Tom::Info),
        EstadoOrcamento::Aprovado => Etiqueta::nova("Aprovado", Tom::Positivo),
        EstadoOrcamento::Recusado => Etiqueta::nova("Recusado", Tom::Negativo),
        EstadoOrcamento::Expirado => Etiqueta::nova("Expirado", Tom::Atencao),
        EstadoOrcamento::Cancelado => Etiqueta::nova("Cancelado", Tom::Neutro),
        EstadoOrcamento::Convertido => Etiqueta::nova("Convertido em OS", Tom::Positivo),
    }
}

fn kv(ui: &mut egui::Ui, chave: &str, valor: &str) {
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo(format!("{chave}: ")));
        ui.add(Rotulo::interface(valor.to_owned()).quebravel());
    });
}
