//! Vendas › pedido faturado › **Devoluções**: o cliente devolve parte ou todo o pedido. Pedir
//! (quanto de cada item volta e por quê), aprovar e concluir — concluir repõe o estoque, devolve
//! o dinheiro pelo caixa (à vista) ou abate a conta a receber (a prazo) e estorna a comissão. Da
//! devolução concluída sai a NF-e de entrada, quando a venda saiu em NF-e.

use std::collections::HashMap;

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Id, Quantidade};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_fiscal::devolucao_venda::{DocumentoDaDevolucaoDeVenda, EmitirNfeDaDevolucaoDeVenda};
use mod_fiscal::{DocumentoResumo, EstadoDocumento};
use mod_vendas::{
    AprovarDevolucao, CancelarDevolucao, ConcluirDevolucao, Devolucao, DevolucaoFoiConcluida,
    DevolucoesDoPedido, EstadoDevolucao, ItemVenda, SolicitarDevolucao,
    MOTIVO_MINIMO_DEVOLUCAO_VENDA,
};

use crate::fiscal_fundo;

/// As devoluções do pedido aberto e o formulário de uma nova.
#[derive(Default)]
pub(super) struct DevolucoesDoPedidoAberto {
    pedido: Option<Id>,
    geracao: u64,
    devolucoes: Vec<Devolucao>,
    /// A NF-e de entrada de cada devolução concluída.
    notas: HashMap<Id, Option<DocumentoResumo>>,
    aberta: bool,
    quantidades: HashMap<Id, String>,
    motivo: String,
}

impl DevolucoesDoPedidoAberto {
    fn carregar(&mut self, motor: &Motor, sessao: &Sessao, pedido: Id) {
        self.pedido = Some(pedido);
        self.geracao = fiscal_fundo::geracao();
        self.devolucoes = motor
            .consultar(
                sessao,
                "vendas.devolucoes_do_pedido.v1",
                &DevolucoesDoPedido { pedido },
            )
            .unwrap_or_default();
        self.notas = self
            .devolucoes
            .iter()
            .filter(|d| d.estado == EstadoDevolucao::Concluida)
            .map(|d| {
                let doc = motor
                    .consultar(
                        sessao,
                        "fiscal.documento_da_devolucao_de_venda.v1",
                        &DocumentoDaDevolucaoDeVenda { devolucao: d.id },
                    )
                    .unwrap_or_default();
                (d.id, doc)
            })
            .collect();
    }

    /// Quanto de cada item já voltou (ou está pedido para voltar).
    fn ja_devolvido(&self) -> HashMap<Id, Quantidade> {
        let mut m: HashMap<Id, Quantidade> = HashMap::new();
        for d in self
            .devolucoes
            .iter()
            .filter(|d| d.estado != EstadoDevolucao::Cancelada)
        {
            for i in &d.itens {
                *m.entry(i.item_pedido).or_insert(Quantidade::ZERO) += i.quantidade;
            }
        }
        m
    }
}

/// O que a conclusão fez, numa frase.
pub(super) fn resultado(f: &DevolucaoFoiConcluida) -> String {
    let mut s = format!(
        "Devolução de {} concluída",
        f.valor_total.formatar_com_simbolo()
    );
    if f.reembolsado.e_positivo() {
        s.push_str(&format!(
            " — {} devolvidos ao cliente pelo caixa",
            f.reembolsado.formatar_com_simbolo()
        ));
    }
    if f.abatido.e_positivo() {
        s.push_str(&format!(
            " — {} abatidos da conta a receber",
            f.abatido.formatar_com_simbolo()
        ));
    }
    let credito = f.valor_total - f.reembolsado - f.abatido;
    if f.credito.is_some() && credito.e_positivo() {
        s.push_str(&format!(
            "; {} de crédito do cliente (Contas a pagar)",
            credito.formatar_com_simbolo()
        ));
    }
    if f.comissao_estornada.e_positivo() {
        s.push_str(&format!(
            "; comissão estornada: {}",
            f.comissao_estornada.formatar_com_simbolo()
        ));
    }
    s
}

/// A seção das devoluções do pedido faturado. `nome` diz o nome do produto. Devolve `true`
/// quando algo mudou (a tela recarrega o pedido).
pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut DevolucoesDoPedidoAberto,
    pedido: Id,
    itens: &[ItemVenda],
    nome: &dyn Fn(Id) -> String,
) -> bool {
    if !sessao.concede("vendas.devolucao.ver") {
        return false;
    }
    if estado.pedido != Some(pedido) || estado.geracao != fiscal_fundo::geracao() {
        estado.carregar(motor, sessao, pedido);
    }
    let mut mudou = false;
    if !estado.devolucoes.is_empty() {
        mudou |= lista(ui, motor, sessao, estado);
        ui.add_space(Espaco::E8);
    }
    if !sessao.concede("vendas.devolucao.solicitar") {
        return mudou;
    }
    if !estado.aberta {
        if ui.add(Botao::secundario("Devolução do cliente…")).clicked() {
            estado.aberta = true;
        }
        return mudou;
    }
    ui.add(Rotulo::titulo_secao("Devolução do cliente"));
    ui.add_space(Espaco::E8);
    let ja = estado.ja_devolvido();
    for item in itens {
        let resta = item.quantidade - ja.get(&item.id).copied().unwrap_or(Quantidade::ZERO);
        ui.add(Rotulo::interface(format!(
            "{} — vendido {}, pode voltar {}",
            nome(item.produto),
            item.quantidade.formatar(0),
            resta.max(Quantidade::ZERO).formatar(0)
        )));
        if resta.e_positiva() {
            let texto = estado.quantidades.entry(item.id).or_default();
            ui.add(Campo::novo("Quanto volta", texto).marcador("0"));
        }
        ui.add_space(Espaco::E4);
    }
    ui.add(
        Campo::novo("Motivo", &mut estado.motivo).marcador("Ex.: produto com defeito de fábrica"),
    );
    ui.add_space(Espaco::E8);
    let linhas: Vec<(Id, Quantidade)> = itens
        .iter()
        .filter_map(|i| {
            let q = Quantidade::de_str(estado.quantidades.get(&i.id)?.trim()).ok()?;
            q.e_positiva().then_some((i.id, q))
        })
        .collect();
    let pode =
        !linhas.is_empty() && estado.motivo.trim().chars().count() >= MOTIVO_MINIMO_DEVOLUCAO_VENDA;
    ui.horizontal(|ui| {
        if ui
            .add(Botao::primario("Pedir devolução").habilitado(pode))
            .clicked()
        {
            let r: Result<Devolucao, _> = motor.executar(
                sessao,
                "vendas.solicitar_devolucao.v1",
                &SolicitarDevolucao {
                    pedido,
                    itens: linhas.clone(),
                    motivo: estado.motivo.trim().to_owned(),
                },
            );
            match r {
                Ok(d) => {
                    notificar(
                        ui.ctx(),
                        Notificacao::sucesso(format!(
                            "Devolução de {} pedida — falta aprovar e concluir",
                            d.valor_total.formatar_com_simbolo()
                        )),
                    );
                    estado.aberta = false;
                    estado.quantidades.clear();
                    estado.motivo.clear();
                    estado.pedido = None;
                    mudou = true;
                }
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
        if ui.add(Botao::fantasma("Cancelar")).clicked() {
            estado.aberta = false;
            estado.quantidades.clear();
        }
    });
    if !pode {
        ui.add(Rotulo::apoio(format!(
            "Informe quanto volta de ao menos um item e o motivo (ao menos \
             {MOTIVO_MINIMO_DEVOLUCAO_VENDA} caracteres)."
        )));
    }
    mudou
}

/// O que fazer com uma devolução da lista.
enum Acao {
    Aprovar(Id),
    Cancelar(Id),
    Concluir(Id),
    EmitirNota(Id),
    Danfe(Id),
}

/// As devoluções do pedido, com as ações de cada uma. Devolve `true` quando algo mudou.
fn lista(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut DevolucoesDoPedidoAberto,
) -> bool {
    ui.add(Rotulo::titulo_secao("Devoluções"));
    ui.add_space(Espaco::E8);
    let mut acao = None;
    for d in &estado.devolucoes {
        ui.horizontal(|ui| {
            let rotulo = d.estado.rotulo();
            ui.add(
                match d.estado {
                    EstadoDevolucao::Concluida => Etiqueta::positiva(rotulo),
                    EstadoDevolucao::Cancelada => Etiqueta::neutra(rotulo),
                    _ => Etiqueta::atencao(rotulo),
                }
                .com_ponto(),
            );
            ui.add(Rotulo::interface(format!(
                "{} — {} — {}",
                d.data.formatar(),
                d.valor_total.formatar_com_simbolo(),
                d.motivo
            )));
        });
        ui.horizontal(|ui| match d.estado {
            EstadoDevolucao::Solicitada | EstadoDevolucao::Aprovada => {
                if d.estado == EstadoDevolucao::Solicitada
                    && sessao.concede("vendas.devolucao.aprovar")
                    && ui.add(Botao::secundario("Aprovar")).clicked()
                {
                    acao = Some(Acao::Aprovar(d.id));
                }
                if d.estado == EstadoDevolucao::Aprovada
                    && sessao.concede("vendas.devolucao.concluir")
                    && ui.add(Botao::primario("Concluir devolução")).clicked()
                {
                    acao = Some(Acao::Concluir(d.id));
                }
                if sessao.concede("vendas.devolucao.aprovar")
                    && ui.add(Botao::fantasma("Cancelar devolução")).clicked()
                {
                    acao = Some(Acao::Cancelar(d.id));
                }
            }
            EstadoDevolucao::Concluida => match estado.notas.get(&d.id).cloned().flatten() {
                None => {
                    if sessao.concede("fiscal.emitir")
                        && ui
                            .add(Botao::secundario("Emitir NF-e de entrada"))
                            .clicked()
                    {
                        acao = Some(Acao::EmitirNota(d.id));
                    }
                }
                Some(doc) => {
                    let texto =
                        format!("NF-e {}-{}: {}", doc.serie, doc.numero, doc.estado.rotulo());
                    ui.add(
                        match doc.estado {
                            EstadoDocumento::Autorizada => Etiqueta::positiva(texto),
                            EstadoDocumento::Rejeitada | EstadoDocumento::Denegada => {
                                Etiqueta::negativa(texto)
                            }
                            _ => Etiqueta::atencao(texto),
                        }
                        .com_ponto(),
                    );
                    if doc.estado == EstadoDocumento::Autorizada
                        && ui.add(Botao::secundario("Imprimir DANFE")).clicked()
                    {
                        acao = Some(Acao::Danfe(doc.id));
                    }
                    if doc.estado == EstadoDocumento::Rejeitada
                        && sessao.concede("fiscal.emitir")
                        && ui.add(Botao::secundario("Emitir de novo")).clicked()
                    {
                        acao = Some(Acao::EmitirNota(d.id));
                    }
                }
            },
            EstadoDevolucao::Cancelada => {}
        });
        if let Some(m) = estado
            .notas
            .get(&d.id)
            .cloned()
            .flatten()
            .filter(|n| n.estado == EstadoDocumento::Rejeitada)
            .and_then(|n| n.motivo)
        {
            ui.add(Rotulo::apoio(m).quebravel());
        }
        ui.add_space(Espaco::E4);
    }
    let Some(acao) = acao else {
        return false;
    };
    let ctx = ui.ctx().clone();
    let erro = |e: cardeal_kernel::Erro| notificar(&ctx, Notificacao::erro(e.mensagem));
    match acao {
        Acao::Aprovar(devolucao) => match motor.executar(
            sessao,
            "vendas.aprovar_devolucao.v1",
            &AprovarDevolucao { devolucao },
        ) {
            Ok(_) => notificar(&ctx, Notificacao::sucesso("Devolução aprovada")),
            Err(e) => erro(e),
        },
        Acao::Cancelar(devolucao) => match motor.executar(
            sessao,
            "vendas.cancelar_devolucao.v1",
            &CancelarDevolucao { devolucao },
        ) {
            Ok(_) => notificar(&ctx, Notificacao::sucesso("Devolução cancelada")),
            Err(e) => erro(e),
        },
        Acao::Concluir(devolucao) => match motor.executar(
            sessao,
            "vendas.concluir_devolucao.v1",
            &ConcluirDevolucao { devolucao },
        ) {
            Ok(f) => notificar(&ctx, Notificacao::sucesso(resultado(&f))),
            Err(e) => erro(e),
        },
        Acao::EmitirNota(devolucao) => match motor.executar(
            sessao,
            "fiscal.emitir_nfe_da_devolucao_de_venda.v1",
            &EmitirNfeDaDevolucaoDeVenda { devolucao },
        ) {
            Ok(doc) if doc.estado == EstadoDocumento::Rejeitada => notificar(
                &ctx,
                Notificacao::aviso(
                    doc.motivo
                        .unwrap_or_else(|| "A NF-e de entrada foi rejeitada".into()),
                ),
            ),
            Ok(_) => {
                fiscal_fundo::disparar(&ctx, motor, sessao);
                notificar(
                    &ctx,
                    Notificacao::sucesso("NF-e de entrada na fila de transmissão"),
                );
            }
            Err(e) => erro(e),
        },
        Acao::Danfe(documento) => match crate::danfe::imprimir(motor, sessao, documento) {
            Ok(_) => notificar(&ctx, Notificacao::sucesso("DANFE aberto para imprimir")),
            Err(e) => notificar(&ctx, Notificacao::erro(e)),
        },
    }
    estado.pedido = None;
    true
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Dinheiro;

    use super::*;

    #[test]
    fn a_frase_da_conclusao() {
        let a_vista = DevolucaoFoiConcluida {
            valor_total: Dinheiro::reais(100),
            reembolsado: Dinheiro::reais(100),
            abatido: Dinheiro::ZERO,
            credito: None,
            comissao_estornada: Dinheiro::reais(10),
        };
        assert_eq!(
            resultado(&a_vista),
            "Devolução de R$ 100,00 concluída — R$ 100,00 devolvidos ao cliente pelo caixa; \
             comissão estornada: R$ 10,00"
        );
        let a_prazo = DevolucaoFoiConcluida {
            reembolsado: Dinheiro::ZERO,
            abatido: Dinheiro::reais(60),
            credito: Some(Id::novo()),
            comissao_estornada: Dinheiro::ZERO,
            ..a_vista
        };
        assert_eq!(
            resultado(&a_prazo),
            "Devolução de R$ 100,00 concluída — R$ 60,00 abatidos da conta a receber; R$ 40,00 \
             de crédito do cliente (Contas a pagar)"
        );
    }
}
