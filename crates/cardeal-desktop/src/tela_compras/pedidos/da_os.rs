//! As peças em falta de uma OS, a encomendar — pedido do usuário (2026-11-07): na OS, um botão
//! só ("Encomendar peças em falta") traz para cá todas as que precisam de compra; aqui elas
//! aparecem no topo da lista de pedidos, cada uma com a sua caixa (desmarca a que não vai
//! agora), e vão para um pedido aberto ou um novo. Salvo o pedido, as peças ficam marcadas
//! como encomendadas na OS — com o fornecedor, o preço combinado e a entrega prevista.

use super::*;
use cardeal_ui::atoms::Caixa;

/// Uma peça da OS sem estoque, ainda não encomendada.
#[derive(Debug, Clone)]
pub struct PecaEmFalta {
    /// O item de peça na OS.
    pub item_peca: Id,
    /// O produto (a peça nova digitada na OS já nasce no catálogo).
    pub produto: Id,
    /// O nome, para a lista.
    pub nome: String,
    /// Quanto a OS precisa.
    pub quantidade: Quantidade,
    /// O custo previsto digitado na OS (zero = não sabe).
    pub custo: Preco,
    /// Vai para o pedido (a caixa).
    pub marcada: bool,
}

/// As peças em falta de uma OS.
#[derive(Debug, Clone)]
pub struct PecasDaOs {
    /// A ordem de serviço.
    pub ordem: Id,
    /// O número, para o texto.
    pub numero: u64,
    /// O aparelho, para o texto.
    pub equipamento: String,
    /// As peças.
    pub pecas: Vec<PecaEmFalta>,
}

/// As peças à espera de pedido, com o destino escolhido (`None` = novo pedido).
pub(super) struct EscolhaDaOs {
    pub(super) pecas: PecasDaOs,
    pub(super) destino: Option<Option<Id>>,
}

/// O vínculo do formulário com a OS: ao salvar, estes itens ficam encomendados na OS.
#[derive(Debug, Clone)]
pub(super) struct VinculoOs {
    pub(super) ordem: Id,
    pub(super) numero: u64,
    /// `(item de peça na OS, produto)`.
    pub(super) itens: Vec<(Id, Id)>,
}

/// O painel no topo da lista de pedidos.
pub(super) fn painel(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoPedidos,
) {
    let abertos: Vec<(Option<Id>, String)> = std::iter::once((None, "Novo pedido".to_owned()))
        .chain(
            estado
                .lista
                .iter()
                .filter(|p| {
                    matches!(p.estado, EstadoPedido::Rascunho)
                        || (p.estado == EstadoPedido::Enviado && p.recebido.e_zero())
                })
                .map(|p| {
                    (
                        Some(p.pedido),
                        format!(
                            "Pedido {:04} · {} · {}",
                            p.numero,
                            p.fornecedor_nome,
                            p.estado.nome()
                        ),
                    )
                }),
        )
        .collect();
    let Some(escolha) = estado.da_os.as_mut() else {
        return;
    };
    let mut continuar = false;
    let mut descartar = false;
    let titulo = format!(
        "Peças a encomendar · OS #{} · {}",
        escolha.pecas.numero, escolha.pecas.equipamento
    );
    Painel::novo()
        .realce(Tom::Info)
        .titulo(titulo)
        .mostrar(ui, |ui| {
            ui.add(
                Rotulo::apoio(
                    "Desmarque o que não vai neste pedido. Ao salvar o pedido, as marcadas ficam \
                     como encomendadas na OS.",
                )
                .quebravel(),
            );
            ui.add_space(Espaco::E8);
            for p in &mut escolha.pecas.pecas {
                ui.horizontal(|ui| {
                    ui.add(Caixa::nova(
                        &mut p.marcada,
                        format!("{} × {}", p.quantidade.formatar(0), p.nome),
                    ));
                    if !p.custo.e_zero() {
                        ui.add(Rotulo::apoio(format!(
                            "custo previsto {}",
                            p.custo.formatar_com_simbolo()
                        )));
                    }
                });
            }
            ui.add_space(Espaco::E8);
            let alguma = escolha.pecas.pecas.iter().any(|p| p.marcada);
            ui.columns(2, |c| {
                SeletorOpcao::novo("Colocar em", &mut escolha.destino)
                    .opcoes(abertos)
                    .mostrar(&mut c[0]);
                c[1].add_space(Espaco::E24);
                c[1].horizontal(|ui| {
                    continuar = ui
                        .add(Botao::primario("Montar o pedido").habilitado(alguma))
                        .clicked();
                    descartar = ui.add(Botao::fantasma("Agora não")).clicked();
                });
            });
        });
    ui.add_space(Espaco::E16);

    if descartar {
        estado.da_os = None;
    } else if continuar {
        montar_form(ui.ctx(), motor, sessao, estado);
    }
}

/// Abre o formulário (do pedido escolhido ou de um novo) com as peças marcadas.
fn montar_form(ctx: &egui::Context, motor: &Motor, sessao: &Sessao, estado: &mut EstadoPedidos) {
    let Some(escolha) = estado.da_os.take() else {
        return;
    };
    let mut form = match escolha.destino.flatten() {
        Some(pedido) => {
            match motor.consultar(sessao, "compras.pedido.v1", &PedidoDeCompra { pedido }) {
                Ok(Some(p)) => FormPedido::de_pedido(&p),
                Ok(None) | Err(_) => {
                    notificar(ctx, Notificacao::erro("Não foi possível abrir o pedido."));
                    estado.da_os = Some(escolha);
                    return;
                }
            }
        }
        None => {
            let mut f = FormPedido::novo();
            f.linhas.clear();
            f
        }
    };
    let marcadas: Vec<&PecaEmFalta> = escolha.pecas.pecas.iter().filter(|p| p.marcada).collect();
    for p in &marcadas {
        match form
            .linhas
            .iter_mut()
            .find(|l| l.produto == Some(p.produto))
        {
            Some(l) => {
                let atual = l
                    .qtd
                    .trim()
                    .parse::<Quantidade>()
                    .unwrap_or(Quantidade::ZERO);
                l.qtd = (atual + p.quantidade).formatar(0);
            }
            None => form.linhas.push(LinhaPedido {
                produto: Some(p.produto),
                busca: String::new(),
                qtd: p.quantidade.formatar(0),
                preco: if p.custo.e_zero() {
                    String::new()
                } else {
                    p.custo.formatar()
                },
                // Com o custo da OS, não sugere outro por cima; sem, sugere o último pago.
                sugerido_para: (!p.custo.e_zero()).then_some(p.produto),
            }),
        }
    }
    form.da_os = Some(VinculoOs {
        ordem: escolha.pecas.ordem,
        numero: escolha.pecas.numero,
        itens: marcadas.iter().map(|p| (p.item_peca, p.produto)).collect(),
    });
    estado.dlg = Dlg::Form(Box::new(form));
}

/// Depois de salvar o pedido: marca as peças como encomendadas na OS.
pub(super) fn marcar_encomendadas(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    vinculo: &VinculoOs,
    cmd: &SalvarPedidoCompra,
    fornecedor: &str,
) {
    let mut ok = 0;
    for (item, produto) in &vinculo.itens {
        let custo = cmd
            .itens
            .iter()
            .find(|i| i.produto == *produto)
            .map(|i| i.preco_unitario)
            .filter(|p| !p.e_zero());
        let r = motor.executar(
            sessao,
            "os.encomendar_peca.v1",
            &mod_os::EncomendarPeca {
                ordem_servico: vinculo.ordem,
                item_peca: *item,
                fornecedor: fornecedor.to_owned(),
                custo_previsto: custo,
                previsao_chegada: cmd.previsao_entrega,
            },
        );
        match r {
            Ok(()) => ok += 1,
            Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
        }
    }
    if ok > 0 {
        notificar(
            ctx,
            Notificacao::sucesso(format!(
                "{ok} peça(s) da OS #{} marcadas como encomendadas",
                vinculo.numero
            )),
        );
    }
}
