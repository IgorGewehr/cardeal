//! Tela de PDV — a frente de caixa. `docs/modulos/pdv.md`.
//!
//! Fluxo: escolher o caixa → abrir a sessão (se fechada) → bipar produtos no cupom →
//! finalizar com as formas de pagamento. O carrinho é reconstruído no cliente a partir do
//! retorno de cada `AdicionarItem` (preço resolvido pelo backend); não há consulta de leitura
//! do cupom nesta fatia do `mod-pdv`.

use cardeal_cliente::{MotorLocal, SessaoLocal};
use cardeal_kernel::{Arredondamento, Dinheiro, Id, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Rotulo};
use cardeal_ui::molecules::{Campo, EstadoVazio, SeletorOpcao};
use cardeal_ui::organisms::{notificar, Dialogo, LayoutTela, Notificacao};
use cardeal_ui::tokens::{Espaco, Raio, TemaUi};
use eframe::egui;
use mod_estoque::{ItemLocal, ItemProdutoComSaldo, Locais, ProdutosComSaldo};
use mod_financeiro::{
    AbrirCaixa, CadastrarCaixa, CaixaCadastrado, CaixaFoiAberto, Caixas, ContasDeCaixa, ItemCaixa,
    ItemContaResultado,
};
use mod_pdv::{
    AbrirCupom, AdicionarItem, CancelarCupom, CupomAberto, FinalizarVenda, FormaPagamentoPdv,
    ItemFoiAdicionado, PagamentoInformado, VendaFoiFinalizada,
};
use mod_vendas::{TabelaPreco, TabelasDePreco};

const SERIE_FISCAL: i64 = 1;

/// Uma linha do carrinho, reconstruída dos retornos de `AdicionarItem`.
struct Linha {
    item: Id,
    nome: String,
    quantidade: Quantidade,
    preco: Preco,
    total: Dinheiro,
    cancelado: bool,
}

#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    CadastrarCaixa {
        nome: String,
        conta: Option<Id>,
    },
    AbrirCaixa {
        valor: String,
    },
    Pagamento {
        dinheiro: String,
        pix: String,
        debito: String,
        credito: String,
    },
}

/// Estado local da tela de PDV.
pub struct EstadoTelaPdv {
    terminal: Id,
    caixas: Vec<ItemCaixa>,
    contas_caixa: Vec<ItemContaResultado>,
    tabelas: Vec<TabelaPreco>,
    locais: Vec<ItemLocal>,
    produtos: Vec<ItemProdutoComSaldo>,
    caixa_sel: Option<Id>,
    tabela_sel: Option<Id>,
    local_sel: Option<Id>,
    busca: String,
    quantidade: String,
    cupom: Option<Id>,
    cupom_numero: Option<i64>,
    carrinho: Vec<Linha>,
    total: Dinheiro,
    ultima_venda: Option<(i64, Dinheiro)>,
    dlg: Dlg,
    erro: Option<String>,
    /// `true` entre o clique em "Confirmar venda" e a execução de fato — dá um quadro para
    /// o egui pintar o spinner + toast "Emitindo NFC-e…" antes da chamada bloqueante.
    finalizar_pendente: bool,
}

impl Default for EstadoTelaPdv {
    fn default() -> Self {
        Self {
            terminal: Id::novo(),
            caixas: Vec::new(),
            contas_caixa: Vec::new(),
            tabelas: Vec::new(),
            locais: Vec::new(),
            produtos: Vec::new(),
            caixa_sel: None,
            tabela_sel: None,
            local_sel: None,
            busca: String::new(),
            quantidade: "1".to_owned(),
            cupom: None,
            cupom_numero: None,
            carrinho: Vec::new(),
            total: Dinheiro::ZERO,
            ultima_venda: None,
            dlg: Dlg::Fechado,
            erro: None,
            finalizar_pendente: false,
        }
    }
}

impl EstadoTelaPdv {
    /// Carrega caixas, catálogos e produtos.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        self.erro = None;
        match motor.consultar(sessao, "financeiro.caixas.v1", &Caixas) {
            Ok(c) => self.caixas = c,
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(c) = motor.consultar(sessao, "financeiro.contas_caixa.v1", &ContasDeCaixa) {
            self.contas_caixa = c;
        }
        if let Ok(t) = motor.consultar(sessao, "vendas.tabelas_de_preco.v1", &TabelasDePreco) {
            self.tabelas = t;
        }
        if let Ok(l) = motor.consultar(sessao, "estoque.locais.v1", &Locais) {
            self.locais = l;
        }
        if let Ok(p) = motor.consultar(sessao, "estoque.produtos_com_saldo.v1", &ProdutosComSaldo) {
            self.produtos = p;
        }

        if self.caixa_sel.is_none() {
            self.caixa_sel = self
                .caixas
                .iter()
                .find(|c| c.sessao_aberta.is_some())
                .or_else(|| self.caixas.first())
                .map(|c| c.caixa);
        }
        if self.tabela_sel.is_none() {
            self.tabela_sel = self.tabelas.first().map(|t| t.id);
        }
        if self.local_sel.is_none() {
            self.local_sel = self.locais.first().map(|l| l.id);
        }
    }

    fn caixa_atual(&self) -> Option<&ItemCaixa> {
        let id = self.caixa_sel?;
        self.caixas.iter().find(|c| c.caixa == id)
    }

    fn sessao_aberta(&self) -> Option<Id> {
        self.caixa_atual().and_then(|c| c.sessao_aberta)
    }

    fn ativos(&self) -> impl Iterator<Item = &Linha> {
        self.carrinho.iter().filter(|l| !l.cancelado)
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    // Execução adiada do fechamento fiscal: o clique só marcou a intenção; agora, um quadro
    // depois, o spinner/toast já pintaram e podemos rodar a chamada bloqueante.
    if std::mem::take(&mut estado.finalizar_pendente) {
        finalizar(ui.ctx(), motor, sessao, estado);
    }

    let sem_caixa = estado.caixas.is_empty();
    let sessao_id = estado.sessao_aberta();

    LayoutTela::nova("PDV — Frente de caixa").mostrar(
        ui,
        estado,
        |_ui, _estado| {},
        |ui, estado| {
            barra_caixa(ui, estado);
            ui.add_space(Espaco::E12);

            if let Some(e) = &estado.erro {
                ui.add(Rotulo::interface(e.clone()).quebravel().cor(ui.cores().negativo));
                ui.add_space(Espaco::E12);
            }
            if let Some((numero, total)) = estado.ultima_venda {
                let cores = ui.cores();
                egui::Frame::none()
                    .fill(cores.positivo_suave)
                    .rounding(Raio::ITEM)
                    .inner_margin(egui::Margin::symmetric(Espaco::E12, Espaco::E8))
                    .show(ui, |ui| {
                        ui.add(
                            Rotulo::interface(format!(
                                "Venda nº {numero} finalizada · {}",
                                total.formatar_com_simbolo()
                            ))
                            .cor(cores.positivo),
                        );
                    });
                ui.add_space(Espaco::E12);
            }

            if sem_caixa {
                EstadoVazio::novo(
                    Icone::Caixa,
                    "Nenhum caixa cadastrado. Cadastre um para começar a vender.",
                )
                .mostrar(ui);
                if ui.add(Botao::primario("Cadastrar caixa")).clicked() {
                    estado.dlg = Dlg::CadastrarCaixa {
                        nome: "Caixa 1".to_owned(),
                        conta: estado.contas_caixa.first().map(|c| c.conta),
                    };
                }
                return;
            }
            if sessao_id.is_none() {
                caixa_fechado(ui, estado);
                return;
            }

            ui.columns(2, |c| {
                catalogo(&mut c[0], motor, sessao, &mut *estado);
                carrinho(&mut c[1], motor, sessao, &mut *estado);
            });
        },
    );

    match &estado.dlg {
        Dlg::Fechado => {}
        Dlg::CadastrarCaixa { .. } => dialogo_cadastrar(ui.ctx(), motor, sessao, estado),
        Dlg::AbrirCaixa { .. } => dialogo_abrir(ui.ctx(), motor, sessao, estado),
        Dlg::Pagamento { .. } => dialogo_pagamento(ui.ctx(), motor, sessao, estado),
    }
}

/// Seletor de caixa (largura contida) + selo de estado.
fn barra_caixa(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    if estado.caixas.is_empty() {
        return;
    }
    let cores = ui.cores();
    let aberto = estado.sessao_aberta().is_some();
    ui.horizontal(|ui| {
        let mut sel = estado.caixa_sel;
        ui.allocate_ui_with_layout(
            egui::vec2(300.0, 46.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                SeletorOpcao::novo("Caixa", &mut sel)
                    .opcoes(estado.caixas.iter().map(|c| (c.caixa, c.nome.clone())))
                    .mostrar(ui);
            },
        );
        if sel != estado.caixa_sel {
            estado.caixa_sel = sel;
            estado.cupom = None;
            estado.cupom_numero = None;
            estado.carrinho.clear();
            estado.total = Dinheiro::ZERO;
        }
        ui.add_space(Espaco::E12);
        let (rot, cor) = if aberto {
            ("● Caixa aberto", cores.positivo)
        } else {
            ("● Caixa fechado", cores.texto_fraco)
        };
        ui.add(Rotulo::interface(rot).cor(cor));
    });
}

fn caixa_fechado(ui: &mut egui::Ui, estado: &mut EstadoTelaPdv) {
    let nome = estado
        .caixa_atual()
        .map_or("—".to_owned(), |c| c.nome.clone());
    let cores = ui.cores();
    egui::Frame::none()
        .fill(cores.atencao_suave)
        .rounding(Raio::CARTAO)
        .inner_margin(Espaco::E24)
        .show(ui, |ui| {
            ui.add(Rotulo::titulo_secao(format!("{nome} está fechado")));
            ui.add_space(Espaco::E8);
            ui.add(Rotulo::campo(
                "Abra a sessão de caixa com o valor de troco inicial para começar a vender.",
            ));
            ui.add_space(Espaco::E16);
            if ui.add(Botao::primario("Abrir caixa")).clicked() {
                estado.dlg = Dlg::AbrirCaixa {
                    valor: "0,00".to_owned(),
                };
            }
        });
}

fn catalogo(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    ui.add(Rotulo::titulo_secao("Catálogo"));
    ui.add_space(Espaco::E8);
    let mut tab = estado.tabela_sel;
    let mut loc = estado.local_sel;
    ui.columns(2, |c| {
        SeletorOpcao::novo("Tabela de preço", &mut tab)
            .opcoes(estado.tabelas.iter().map(|t| (t.id, t.nome.clone())))
            .mostrar(&mut c[0]);
        SeletorOpcao::novo("Local", &mut loc)
            .opcoes(estado.locais.iter().map(|l| (l.id, l.nome.clone())))
            .mostrar(&mut c[1]);
    });
    estado.tabela_sel = tab;
    estado.local_sel = loc;
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        ui.add(Campo::novo("Buscar", &mut estado.busca).marcador("nome do produto"));
        ui.allocate_ui_with_layout(
            egui::vec2(90.0, 46.0),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.add(Campo::novo("Qtd", &mut estado.quantidade));
            },
        );
    });
    ui.add_space(Espaco::E12);

    let busca = estado.busca.trim().to_lowercase();
    let filtrados: Vec<(Id, String, String)> = estado
        .produtos
        .iter()
        .filter(|p| busca.is_empty() || p.nome.to_lowercase().contains(&busca))
        .take(60)
        .map(|p| (p.produto, p.nome.clone(), p.disponivel.formatar(0)))
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("pdv-produtos")
        .auto_shrink([false, false])
        .max_height(360.0)
        .show(ui, |ui| {
            for (produto, nome, saldo) in &filtrados {
                egui::Frame::none()
                    .fill(ui.cores().superficie)
                    .stroke(egui::Stroke::new(1.0_f32, ui.cores().borda))
                    .rounding(Raio::ITEM)
                    .inner_margin(Espaco::E12)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add(Rotulo::interface(nome.clone()));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui.add(Botao::primario("+").pequeno()).clicked() {
                                        adicionar(ui.ctx(), motor, sessao, estado, *produto);
                                    }
                                    ui.add(Rotulo::campo(format!("saldo {saldo}")));
                                },
                            );
                        });
                    });
                ui.add_space(Espaco::E4);
            }
            if filtrados.is_empty() {
                ui.add(Rotulo::campo("Nenhum produto encontrado."));
            }
        });
}

fn carrinho(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let cores = ui.cores();
    ui.add(Rotulo::titulo_secao("Cupom"));
    ui.add_space(Espaco::E8);

    if estado.carrinho.is_empty() {
        ui.add(Rotulo::campo("Sem itens. Adicione produtos pelo catálogo."));
    }

    let mut cancelar: Option<usize> = None;
    egui::ScrollArea::vertical()
        .id_salt("pdv-carrinho")
        .auto_shrink([false, false])
        .max_height(300.0)
        .show(ui, |ui| {
            for (i, linha) in estado.carrinho.iter().enumerate() {
                egui::Frame::none()
                    .fill(if linha.cancelado {
                        cores.superficie_2
                    } else {
                        cores.superficie
                    })
                    .stroke(egui::Stroke::new(1.0_f32, cores.borda))
                    .rounding(Raio::CAMPO)
                    .inner_margin(Espaco::E8)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let nome = if linha.cancelado {
                                Rotulo::interface(linha.nome.clone()).cor(cores.texto_fraco)
                            } else {
                                Rotulo::interface(linha.nome.clone())
                            };
                            ui.add(nome);
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if !linha.cancelado
                                        && ui.add(Botao::fantasma("✕").pequeno()).clicked()
                                    {
                                        cancelar = Some(i);
                                    }
                                    ui.add(Rotulo::interface(linha.total.formatar_com_simbolo()));
                                    ui.add(Rotulo::campo(format!(
                                        "{} × {}",
                                        linha.quantidade.formatar(0),
                                        linha.preco.formatar_com_simbolo()
                                    )));
                                },
                            );
                        });
                    });
                ui.add_space(Espaco::E4);
            }
        });

    if let Some(i) = cancelar {
        cancelar_item(ui.ctx(), motor, sessao, estado, i);
    }

    ui.add_space(Espaco::E12);
    ui.separator();
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        ui.add(Rotulo::titulo_secao("TOTAL"));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                Rotulo::titulo_secao(estado.total.formatar_com_simbolo()).cor(cores.texto_forte),
            );
        });
    });
    ui.add_space(Espaco::E12);
    ui.horizontal(|ui| {
        let pode = estado.ativos().count() > 0;
        if ui
            .add(Botao::primario("Finalizar (F2)").pequeno())
            .clicked()
            && pode
        {
            estado.dlg = Dlg::Pagamento {
                dinheiro: estado.total.formatar(),
                pix: String::new(),
                debito: String::new(),
                credito: String::new(),
            };
        }
        if estado.cupom.is_some()
            && ui.add(Botao::destrutivo("Cancelar cupom").pequeno()).clicked()
        {
            cancelar_cupom(ui.ctx(), motor, sessao, estado);
        }
    });
}

// ── ações ────────────────────────────────────────────────────────────────────

fn garantir_cupom(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) -> Option<Id> {
    if let Some(c) = estado.cupom {
        return Some(c);
    }
    let (Some(sessao_caixa), Some(tabela_preco), Some(local_expedicao)) =
        (estado.sessao_aberta(), estado.tabela_sel, estado.local_sel)
    else {
        notificar(
            ctx,
            Notificacao::aviso("Configure tabela de preço e local antes de vender."),
        );
        return None;
    };
    match motor.executar(
        sessao,
        "pdv.abrir_cupom.v1",
        &AbrirCupom {
            sessao_caixa,
            terminal: estado.terminal,
            serie_fiscal: SERIE_FISCAL,
            cliente: None,
            tabela_preco,
            local_expedicao,
        },
    ) {
        Ok(CupomAberto {
            cupom,
            numero_terminal,
        }) => {
            estado.cupom = Some(cupom);
            estado.cupom_numero = Some(numero_terminal);
            estado.ultima_venda = None;
            Some(cupom)
        }
        Err(e) => {
            notificar(ctx, Notificacao::erro(e.mensagem));
            None
        }
    }
}

fn adicionar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
    produto: Id,
) {
    let Ok(quantidade) = estado.quantidade.parse::<Quantidade>() else {
        notificar(ctx, Notificacao::aviso("Quantidade inválida."));
        return;
    };
    let Some(cupom) = garantir_cupom(ctx, motor, sessao, estado) else {
        return;
    };
    let nome = estado
        .produtos
        .iter()
        .find(|p| p.produto == produto)
        .map_or_else(|| "Produto".to_owned(), |p| p.nome.clone());

    match motor.executar(
        sessao,
        "pdv.adicionar_item.v1",
        &AdicionarItem {
            cupom,
            produto,
            variacao: None,
            quantidade,
        },
    ) {
        Ok(ItemFoiAdicionado {
            item,
            preco_unitario,
            total_cupom,
        }) => {
            let total = Dinheiro::de_total(quantidade, preco_unitario, Arredondamento::MeioAcima);
            estado.carrinho.push(Linha {
                item,
                nome,
                quantidade,
                preco: preco_unitario,
                total,
                cancelado: false,
            });
            estado.total = total_cupom;
            estado.erro = None;
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn cancelar_item(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
    i: usize,
) {
    let (Some(cupom), Some(linha)) = (estado.cupom, estado.carrinho.get(i)) else {
        return;
    };
    let item = linha.item;
    match motor.executar(
        sessao,
        "pdv.cancelar_item.v1",
        &mod_pdv::CancelarItem {
            cupom,
            item,
            motivo: "Removido no balcão".to_owned(),
        },
    ) {
        Ok(()) => {
            if let Some(l) = estado.carrinho.get_mut(i) {
                l.cancelado = true;
            }
            let novo_total = estado
                .carrinho
                .iter()
                .filter(|l| !l.cancelado)
                .map(|l| l.total)
                .fold(Dinheiro::ZERO, |a, b| a + b);
            estado.total = novo_total;
            estado.erro = None;
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn cancelar_cupom(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let Some(cupom) = estado.cupom else { return };
    match motor.executar(
        sessao,
        "pdv.cancelar_cupom.v1",
        &CancelarCupom {
            cupom,
            motivo: "Cancelado no balcão".to_owned(),
            autorizado_por: sessao.usuario(),
        },
    ) {
        Ok(()) => {
            estado.cupom = None;
            estado.cupom_numero = None;
            estado.carrinho.clear();
            estado.total = Dinheiro::ZERO;
            estado.erro = None;
            notificar(ctx, Notificacao::info("Cupom cancelado"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_cadastrar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let contas: Vec<(Id, String)> = estado
        .contas_caixa
        .iter()
        .map(|c| (c.conta, format!("{} · {}", c.codigo, c.nome)))
        .collect();

    let fechar = Dialogo::nova("Cadastrar caixa").largura(480.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::CadastrarCaixa { nome, conta } = &mut estado.dlg else {
                return;
            };
            ui.add(Campo::novo("Nome", nome).marcador("Caixa 1"));
            ui.add_space(Espaco::E12);
            SeletorOpcao::novo("Conta do Razão", conta)
                .opcoes(contas.clone())
                .placeholder("Conta de caixa (1.1.01)")
                .mostrar(ui);
        },
        |ui, estado| {
            if ui.add(Botao::primario("Cadastrar")).clicked() {
                if let Dlg::CadastrarCaixa { nome, conta } = &estado.dlg {
                    let (nome, conta) = (nome.clone(), *conta);
                    let Some(conta_razao) = conta else {
                        notificar(ctx, Notificacao::aviso("Escolha a conta do Razão."));
                        return;
                    };
                    match motor.executar(
                        sessao,
                        "financeiro.cadastrar_caixa.v1",
                        &CadastrarCaixa {
                            nome,
                            local_operacao: None,
                            conta_razao,
                            permite_negativo: false,
                        },
                    ) {
                        Ok(CaixaCadastrado { caixa }) => {
                            estado.caixa_sel = Some(caixa);
                            estado.dlg = Dlg::Fechado;
                            estado.carregar(motor, sessao);
                            notificar(ctx, Notificacao::sucesso("Caixa cadastrado"));
                        }
                        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
                    }
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

fn dialogo_abrir(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let Some(caixa) = estado.caixa_sel else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let fechar = Dialogo::nova("Abrir caixa").largura(420.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::AbrirCaixa { valor } = &mut estado.dlg else {
                return;
            };
            ui.add(Rotulo::campo("Valor de troco em caixa na abertura."));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo("Valor de abertura", valor).marcador("0,00"));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Abrir")).clicked() {
                if let Dlg::AbrirCaixa { valor } = &estado.dlg {
                    let Ok(valor_abertura) = valor.parse::<Dinheiro>() else {
                        notificar(ctx, Notificacao::aviso("Valor inválido (0,00)."));
                        return;
                    };
                    match motor.executar(
                        sessao,
                        "financeiro.abrir_caixa.v1",
                        &AbrirCaixa {
                            caixa,
                            valor_abertura,
                        },
                    ) {
                        Ok(CaixaFoiAberto { .. }) => {
                            estado.dlg = Dlg::Fechado;
                            estado.carregar(motor, sessao);
                            notificar(ctx, Notificacao::sucesso("Caixa aberto"));
                        }
                        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
                    }
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

fn dialogo_pagamento(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let total = estado.total;
    let fechar = Dialogo::nova("Pagamento").largura(520.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            let Dlg::Pagamento {
                dinheiro,
                pix,
                debito,
                credito,
            } = &mut estado.dlg
            else {
                return;
            };
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Dinheiro", dinheiro).marcador("0,00"));
                c[1].add(Campo::novo("Pix", pix).marcador("0,00"));
            });
            ui.add_space(Espaco::E8);
            ui.columns(2, |c| {
                c[0].add(Campo::novo("Débito", debito).marcador("0,00"));
                c[1].add(Campo::novo("Crédito", credito).marcador("0,00"));
            });
            ui.add_space(Espaco::E12);
            let soma = [
                dinheiro.as_str(),
                pix.as_str(),
                debito.as_str(),
                credito.as_str(),
            ]
            .iter()
            .filter_map(|s| s.parse::<Dinheiro>().ok())
            .fold(Dinheiro::ZERO, |a, b| a + b);
            let falta = total - soma;
            let cor = if falta == Dinheiro::ZERO {
                ui.cores().positivo
            } else {
                ui.cores().atencao
            };
            ui.add(
                Rotulo::interface(format!(
                    "Total {} · informado {} · {}",
                    total.formatar_com_simbolo(),
                    soma.formatar_com_simbolo(),
                    if falta.e_negativo() {
                        format!("troco {}", falta.abs().formatar_com_simbolo())
                    } else if falta == Dinheiro::ZERO {
                        "fecha".to_owned()
                    } else {
                        format!("falta {}", falta.formatar_com_simbolo())
                    }
                ))
                .cor(cor),
            );
        },
        |ui, estado| {
            let carregando = estado.finalizar_pendente;
            if ui
                .add(Botao::primario("Confirmar venda").carregando(carregando))
                .clicked()
                && !carregando
            {
                estado.finalizar_pendente = true;
                notificar(
                    ui.ctx(),
                    Notificacao::carregando("Emitindo NFC-e…").id(egui::Id::new("pdv-fiscal")),
                );
                ui.ctx().request_repaint();
            }
            if ui.add(Botao::secundario("Voltar").habilitado(!carregando)).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn finalizar(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaPdv,
) {
    let fiscal = egui::Id::new("pdv-fiscal");
    let Some(cupom) = estado.cupom else { return };
    let Dlg::Pagamento {
        dinheiro,
        pix,
        debito,
        credito,
    } = &estado.dlg
    else {
        return;
    };
    let campos = [
        (FormaPagamentoPdv::Dinheiro, dinheiro),
        (FormaPagamentoPdv::Pix, pix),
        (FormaPagamentoPdv::Debito, debito),
        (FormaPagamentoPdv::Credito, credito),
    ];
    let mut pagamentos: Vec<PagamentoInformado> = Vec::new();
    let mut soma = Dinheiro::ZERO;
    for (forma, texto) in campos {
        let t = texto.trim();
        if t.is_empty() {
            continue;
        }
        let Ok(valor) = t.parse::<Dinheiro>() else {
            notificar(ctx, Notificacao::erro("Valor de pagamento inválido.").id(fiscal));
            return;
        };
        if valor.e_negativo() || valor == Dinheiro::ZERO {
            continue;
        }
        soma += valor;
        pagamentos.push(PagamentoInformado { forma, valor });
    }
    // Excesso em dinheiro é troco — desconta da parcela em dinheiro para fechar exato.
    if soma > estado.total {
        let excesso = soma - estado.total;
        if let Some(p) = pagamentos
            .iter_mut()
            .find(|p| matches!(p.forma, FormaPagamentoPdv::Dinheiro) && p.valor >= excesso)
        {
            p.valor -= excesso;
        }
    }

    match motor.executar(
        sessao,
        "pdv.finalizar_venda.v1",
        &FinalizarVenda { cupom, pagamentos },
    ) {
        Ok(VendaFoiFinalizada { total, .. }) => {
            let numero = estado.cupom_numero.unwrap_or(0);
            estado.ultima_venda = Some((numero, total));
            estado.cupom = None;
            estado.cupom_numero = None;
            estado.carrinho.clear();
            estado.total = Dinheiro::ZERO;
            estado.dlg = Dlg::Fechado;
            estado.erro = None;
            estado.carregar(motor, sessao);
            notificar(
                ctx,
                Notificacao::sucesso(format!("Venda #{numero} concluída"))
                    .detalhe(format!("NFC-e autorizada · {}", total.formatar_com_simbolo()))
                    .id(fiscal),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem).id(fiscal)),
    }
}
