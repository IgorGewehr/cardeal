//! "Devolver ao fornecedor" no diálogo de uma nota confirmada: quanto de cada item volta e por
//! quê; a mercadoria sai do estoque, a conta a pagar da nota é abatida e o que sobrar vira
//! crédito com o fornecedor. Abaixo, as devoluções feitas, cada uma com a NF-e de devolução —
//! emitir, a situação na fila, o DANFE.

use std::collections::HashMap;

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Id, Quantidade};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo};
use cardeal_ui::molecules::Campo;
use cardeal_ui::organisms::{notificar, Notificacao};
use cardeal_ui::tokens::Espaco;
use eframe::egui;
use mod_compras::{
    DevolucaoAoFornecedor, DevolucaoFeita, DevolucoesDaNota, DevolverAoFornecedor,
    EmitirNfeDaDevolucao, ItemADevolver, ItemNotaEntrada, MOTIVO_MINIMO_DEVOLUCAO,
};
use mod_fiscal::devolucao_compra::DocumentoDaDevolucao;
use mod_fiscal::{DocumentoResumo, EstadoDocumento};

use crate::fiscal_fundo;

/// As devoluções da nota aberta e o formulário de uma nova.
#[derive(Default)]
pub(super) struct EstadoDevolucao {
    /// A nota cujas devoluções estão carregadas.
    nota: Option<Id>,
    devolucoes: Vec<DevolucaoAoFornecedor>,
    /// A NF-e de cada devolução (`None`: ainda não emitida).
    notas_fiscais: HashMap<Id, Option<DocumentoResumo>>,
    /// A geração da fila fiscal que as NF-e mostram.
    geracao_vista: u64,
    /// O formulário de uma devolução nova está aberto.
    aberta: bool,
    /// Quanto volta, por item (texto do campo).
    quantidades: HashMap<Id, String>,
    motivo: String,
}

impl EstadoDevolucao {
    fn carregar(&mut self, motor: &Motor, sessao: &Sessao, nota: Id) {
        self.nota = Some(nota);
        self.geracao_vista = fiscal_fundo::geracao();
        self.devolucoes = motor
            .consultar(
                sessao,
                "compras.devolucoes_da_nota.v1",
                &DevolucoesDaNota { nota_entrada: nota },
            )
            .unwrap_or_default();
        self.notas_fiscais = self
            .devolucoes
            .iter()
            .map(|d| {
                let doc = motor
                    .consultar(
                        sessao,
                        "fiscal.documento_da_devolucao.v1",
                        &DocumentoDaDevolucao { devolucao: d.id },
                    )
                    .unwrap_or_default();
                (d.id, doc)
            })
            .collect();
    }

    /// Quanto de cada item já voltou.
    fn ja_devolvido(&self) -> HashMap<Id, Quantidade> {
        let mut m: HashMap<Id, Quantidade> = HashMap::new();
        for d in &self.devolucoes {
            for i in &d.itens {
                *m.entry(i.item).or_insert(Quantidade::ZERO) += i.quantidade;
            }
        }
        m
    }
}

/// O texto do resultado da devolução.
fn resultado(feito: &DevolucaoFeita) -> String {
    let mut msg = format!(
        "Devolução de {} registrada",
        feito.valor_total.formatar_com_simbolo()
    );
    if feito.abatido.e_positivo() {
        msg.push_str(&format!(
            " — {} abatidos da conta a pagar",
            feito.abatido.formatar_com_simbolo()
        ));
    }
    let credito = feito.valor_total - feito.abatido;
    if feito.credito.is_some() && credito.e_positivo() {
        msg.push_str(&format!(
            "; {} de crédito com o fornecedor (Contas a receber)",
            credito.formatar_com_simbolo()
        ));
    }
    msg
}

/// O bloco das devoluções. `pode_devolver`: a nota está confirmada. Devolve `true` quando
/// devolveu (a tela recarrega a nota).
pub(super) fn secao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoDevolucao,
    nota: Id,
    itens: &[ItemNotaEntrada],
    pode_devolver: bool,
) -> bool {
    if estado.nota != Some(nota) || estado.geracao_vista != fiscal_fundo::geracao() {
        estado.carregar(motor, sessao, nota);
    }
    let mut devolveu = false;
    if !estado.devolucoes.is_empty() {
        lista(ui, motor, sessao, estado);
        ui.add_space(Espaco::E12);
    }
    if !pode_devolver || !sessao.concede("compras.devolucao.criar") {
        return false;
    }
    if !estado.aberta {
        ui.add(
            Rotulo::apoio(
                "Mercadoria com defeito, vencida ou a mais? Devolva ao fornecedor: ela sai do \
                 estoque, a conta a pagar da nota é abatida e, se já foi paga, o valor vira \
                 crédito com ele.",
            )
            .quebravel(),
        );
        ui.add_space(Espaco::E8);
        if ui
            .add(Botao::secundario("Devolver ao fornecedor…"))
            .clicked()
        {
            estado.aberta = true;
        }
        return false;
    }
    ui.add(Rotulo::titulo_secao("Devolver ao fornecedor"));
    ui.add_space(Espaco::E8);
    let ja = estado.ja_devolvido();
    for item in itens {
        let voltou = ja.get(&item.id).copied().unwrap_or(Quantidade::ZERO);
        let resta = item.quantidade - voltou;
        let texto = estado.quantidades.entry(item.id).or_default();
        ui.horizontal(|ui| {
            ui.add(
                Rotulo::interface(format!(
                    "{} — entrou {}, pode voltar {}",
                    item.descricao_fornecedor,
                    item.quantidade.formatar(0),
                    resta.max(Quantidade::ZERO).formatar(0)
                ))
                .quebravel(),
            );
        });
        if resta.e_positiva() {
            ui.add(Campo::novo("Quanto volta", texto).marcador("0"));
        }
        ui.add_space(Espaco::E4);
    }
    ui.add(
        Campo::novo("Motivo (vai na NF-e de devolução)", &mut estado.motivo)
            .marcador("Ex.: produto com a embalagem avariada"),
    );
    ui.add_space(Espaco::E8);
    let pedidos: Vec<ItemADevolver> = itens
        .iter()
        .filter_map(|i| {
            let q = Quantidade::de_str(estado.quantidades.get(&i.id)?.trim()).ok()?;
            q.e_positiva().then_some(ItemADevolver {
                item: i.id,
                quantidade: q,
            })
        })
        .collect();
    let pode =
        !pedidos.is_empty() && estado.motivo.trim().chars().count() >= MOTIVO_MINIMO_DEVOLUCAO;
    ui.horizontal(|ui| {
        if ui
            .add(Botao::destrutivo("Devolver").habilitado(pode))
            .clicked()
        {
            let r: Result<DevolucaoFeita, _> = motor.executar(
                sessao,
                "compras.devolver_ao_fornecedor.v1",
                &DevolverAoFornecedor {
                    nota_entrada: nota,
                    itens: pedidos.clone(),
                    motivo: estado.motivo.trim().to_owned(),
                },
            );
            match r {
                Ok(feito) => {
                    notificar(ui.ctx(), Notificacao::sucesso(resultado(&feito)));
                    estado.aberta = false;
                    estado.quantidades.clear();
                    estado.motivo.clear();
                    estado.nota = None;
                    devolveu = true;
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
             {MOTIVO_MINIMO_DEVOLUCAO} caracteres)."
        )));
    }
    devolveu
}

/// As devoluções feitas, com a NF-e de cada uma.
fn lista(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoDevolucao) {
    ui.add(Rotulo::titulo_secao("Devoluções ao fornecedor"));
    ui.add_space(Espaco::E8);
    let mut emitir = None;
    let mut danfe = None;
    for d in &estado.devolucoes {
        ui.add(Rotulo::interface(format!(
            "{} — {} — {}",
            d.data.formatar(),
            d.valor_total.formatar_com_simbolo(),
            d.motivo
        )));
        for i in &d.itens {
            ui.add(Rotulo::campo(format!(
                "{} × {} ({})",
                i.quantidade.formatar(0),
                i.descricao,
                i.valor.formatar_com_simbolo()
            )));
        }
        let credito = d.valor_total - d.abatido;
        if d.credito.is_some() && credito.e_positivo() {
            ui.add(Rotulo::apoio(format!(
                "Abateu {} da conta a pagar; {} ficaram de crédito com o fornecedor.",
                d.abatido.formatar_com_simbolo(),
                credito.formatar_com_simbolo()
            )));
        } else if d.abatido.e_positivo() {
            ui.add(Rotulo::apoio(format!(
                "Abateu {} da conta a pagar.",
                d.abatido.formatar_com_simbolo()
            )));
        }
        let nfe = estado.notas_fiscais.get(&d.id).cloned().flatten();
        ui.horizontal(|ui| match &nfe {
            None => {
                if sessao.concede("fiscal.emitir")
                    && ui
                        .add(Botao::secundario("Emitir NF-e de devolução"))
                        .clicked()
                {
                    emitir = Some(d.id);
                }
            }
            Some(doc) => {
                let etiqueta =
                    format!("NF-e {}-{}: {}", doc.serie, doc.numero, doc.estado.rotulo());
                ui.add(
                    match doc.estado {
                        EstadoDocumento::Autorizada => Etiqueta::positiva(etiqueta),
                        EstadoDocumento::Rejeitada | EstadoDocumento::Denegada => {
                            Etiqueta::negativa(etiqueta)
                        }
                        _ => Etiqueta::atencao(etiqueta),
                    }
                    .com_ponto(),
                );
                if doc.estado == EstadoDocumento::Autorizada
                    && ui.add(Botao::secundario("Imprimir DANFE")).clicked()
                {
                    danfe = Some(doc.id);
                }
                if doc.estado == EstadoDocumento::Rejeitada
                    && sessao.concede("fiscal.emitir")
                    && ui.add(Botao::secundario("Emitir de novo")).clicked()
                {
                    emitir = Some(d.id);
                }
            }
        });
        if let Some(m) = nfe
            .as_ref()
            .filter(|n| n.estado == EstadoDocumento::Rejeitada)
            .and_then(|n| n.motivo.clone())
        {
            ui.add(Rotulo::apoio(m).quebravel());
        }
        ui.add_space(Espaco::E8);
    }
    if let Some(devolucao) = emitir {
        let r: Result<DocumentoResumo, _> = motor.executar(
            sessao,
            "compras.emitir_nfe_da_devolucao.v1",
            &EmitirNfeDaDevolucao { devolucao },
        );
        match r {
            Ok(doc) if doc.estado == EstadoDocumento::Rejeitada => notificar(
                ui.ctx(),
                Notificacao::aviso(
                    doc.motivo
                        .unwrap_or_else(|| "A NF-e de devolução foi rejeitada".into()),
                ),
            ),
            Ok(_) => {
                fiscal_fundo::disparar(ui.ctx(), motor, sessao);
                notificar(
                    ui.ctx(),
                    Notificacao::sucesso("NF-e de devolução na fila de transmissão"),
                );
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
        estado.nota = None;
    }
    if let Some(id) = danfe {
        match crate::danfe::imprimir(motor, sessao, id) {
            Ok(_) => notificar(ui.ctx(), Notificacao::sucesso("DANFE aberto para imprimir")),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::Dinheiro;

    use super::*;

    #[test]
    fn o_resultado_diz_o_abatido_e_o_credito() {
        let feito = DevolucaoFeita {
            devolucao: Id::novo(),
            valor_total: Dinheiro::centavos(3_474),
            abatido: Dinheiro::centavos(579),
            credito: Some(Id::novo()),
            nota_devolvida: true,
        };
        assert_eq!(
            resultado(&feito),
            "Devolução de R$ 34,74 registrada — R$ 5,79 abatidos da conta a pagar; R$ 28,95 de \
             crédito com o fornecedor (Contas a receber)"
        );
        let so_abate = DevolucaoFeita {
            abatido: Dinheiro::centavos(2_316),
            valor_total: Dinheiro::centavos(2_316),
            credito: None,
            ..feito
        };
        assert_eq!(
            resultado(&so_abate),
            "Devolução de R$ 23,16 registrada — R$ 23,16 abatidos da conta a pagar"
        );
    }
}
