//! `F11`: as vendas de hoje. Daqui se estorna uma venda já paga (com motivo e a autorização
//! de um supervisor, como o `F8`), se imprime o cupom fiscal e se acompanha a NFC-e de cada
//! venda — inclusive a da venda estornada, que a fila cancela ou inutiliza.

use std::collections::HashMap;

use cardeal_kernel::{Data, Fuso, Instante};
use mod_fiscal::{
    AnularDocumentoDaVenda, DocumentoResumo, DocumentosDasVendas, EstadoDocumento,
    PrazoDeCancelamento,
};
use mod_pdv::{
    EstadoCupom, EstornarVenda, FormaPagamentoPdv, VendaDoPeriodo, VendaFoiEstornada,
    VendasDoPeriodo,
};

use super::*;

/// A permissão que o supervisor precisa ter para autorizar o estorno.
const PERMISSAO_ESTORNAR: &str = "pdv.venda.estornar";

/// O diálogo aberto.
#[derive(Default)]
pub(super) struct EstadoVendasDoDia {
    pub(super) vendas: Vec<VendaDoPeriodo>,
    /// A NFC-e de cada venda que tem uma.
    pub(super) notas: HashMap<Id, DocumentoResumo>,
    pub(super) sel: Option<usize>,
    /// O formulário de estorno, quando aberto.
    pub(super) estorno: Option<FormEstorno>,
}

/// Motivo e a segunda identidade de quem autoriza.
#[derive(Default)]
pub(super) struct FormEstorno {
    pub(super) motivo: String,
    pub(super) supervisor: String,
    pub(super) senha: String,
}

impl EstadoVendasDoDia {
    /// Carrega as vendas de hoje e as NFC-e delas.
    pub(super) fn carregar(motor: &Motor, sessao: &Sessao) -> Result<Self, String> {
        let hoje = Data::hoje(Fuso::BRASILIA);
        let vendas = motor
            .consultar(
                sessao,
                "pdv.vendas_do_periodo.v1",
                &VendasDoPeriodo {
                    de: hoje,
                    ate: hoje,
                },
            )
            .map_err(|e| e.mensagem)?;
        let mut estado = Self {
            vendas,
            ..Self::default()
        };
        estado.carregar_notas(motor, sessao);
        estado.sel = (!estado.vendas.is_empty()).then_some(0);
        Ok(estado)
    }

    fn carregar_notas(&mut self, motor: &Motor, sessao: &Sessao) {
        if !sessao.concede("fiscal.ver") || self.vendas.is_empty() {
            return;
        }
        let cupons = self.vendas.iter().map(|v| v.cupom).collect();
        let notas: Result<Vec<DocumentoResumo>, _> = motor.consultar(
            sessao,
            "fiscal.documentos_das_vendas.v1",
            &DocumentosDasVendas { cupons },
        );
        if let Ok(notas) = notas {
            self.notas = notas.into_iter().map(|d| (d.origem, d)).collect();
        }
    }

    fn venda_sel(&self) -> Option<&VendaDoPeriodo> {
        self.sel.and_then(|i| self.vendas.get(i))
    }

    fn nota_sel(&self) -> Option<&DocumentoResumo> {
        self.venda_sel().and_then(|v| self.notas.get(&v.cupom))
    }

    /// Total vendido (sem as estornadas) e quantas foram estornadas.
    fn totais(&self) -> (Dinheiro, usize) {
        let vendido = self
            .vendas
            .iter()
            .filter(|v| v.estado == EstadoCupom::Finalizado)
            .map(|v| v.total)
            .fold(Dinheiro::ZERO, |a, b| a + b);
        let estornadas = self
            .vendas
            .iter()
            .filter(|v| v.estado == EstadoCupom::Estornado)
            .count();
        (vendido, estornadas)
    }
}

fn forma_rotulo(f: FormaPagamentoPdv) -> &'static str {
    match f {
        FormaPagamentoPdv::Dinheiro => "Dinheiro",
        FormaPagamentoPdv::Pix => "Pix",
        FormaPagamentoPdv::Debito => "Débito",
        FormaPagamentoPdv::Credito => "Crédito",
        FormaPagamentoPdv::Prazo => "A prazo",
    }
}

/// O que a NFC-e da venda está fazendo, numa etiqueta.
fn etiqueta_nota(nota: Option<&DocumentoResumo>) -> Etiqueta {
    let Some(d) = nota else {
        return Etiqueta::neutra("sem NFC-e");
    };
    if d.anulacao_pendente() {
        return Etiqueta::negativa("cancelamento recusado");
    }
    if let Some(p) = d.pedido {
        return Etiqueta::atencao(p.rotulo());
    }
    let rotulo = d.estado.rotulo();
    match d.estado {
        EstadoDocumento::Autorizada => Etiqueta::positiva(rotulo),
        EstadoDocumento::Pendente | EstadoDocumento::Incerta => Etiqueta::info(rotulo),
        EstadoDocumento::Contingencia => Etiqueta::atencao(rotulo),
        EstadoDocumento::Rejeitada | EstadoDocumento::Denegada => Etiqueta::negativa(rotulo),
        EstadoDocumento::Cancelada | EstadoDocumento::Inutilizada => Etiqueta::neutra(rotulo),
    }
}

/// O prazo de cancelamento da NFC-e da venda selecionada, se ela ainda vale e pode ser
/// estornada — a SEFAZ dá 30 minutos depois da autorização.
fn prazo_da_nota(v: &EstadoVendasDoDia, agora: Instante) -> Option<PrazoDeCancelamento> {
    let venda = v.venda_sel()?;
    if venda.estado != EstadoCupom::Finalizado {
        return None;
    }
    v.nota_sel()
        .filter(|d| !d.anulada && d.pedido.is_none())
        .and_then(|d| d.prazo_de_cancelamento(agora))
}

/// O que o prazo diz para quem vai estornar a venda: dentro dele, o cancelamento da NFC-e vai
/// junto; fora, a venda se desfaz, mas a SEFAZ pode recusar a nota (não bloqueia: algumas UFs
/// aceitam fora do prazo).
fn aviso_do_prazo(p: PrazoDeCancelamento) -> String {
    if p.dentro() {
        let mut t = format!(
            "NFC-e autorizada há {}: estornando agora, o cancelamento vai dentro do prazo \
             (faltam {}).",
            p.ha(),
            p.falta()
        );
        if p.falta_pouco() {
            t.push_str(
                " Falta pouco — sem internet, o pedido espera na fila e o prazo continua correndo.",
            );
        }
        t
    } else {
        format!(
            "NFC-e autorizada há {}: passou o prazo de cancelamento ({}). O estorno desfaz a \
             venda, mas a SEFAZ pode recusar o cancelamento da nota — aí a saída é uma NFC-e de \
             devolução.",
            p.ha(),
            p.prazo()
        )
    }
}

/// A justificativa que vai à SEFAZ (15 a 255 caracteres): o motivo tem ao menos 10, então o
/// prefixo garante o mínimo.
fn justificativa(numero: i64, motivo: &str) -> String {
    format!("Venda nº {numero} estornada no PDV: {motivo}")
        .chars()
        .take(255)
        .collect()
}

/// Pede ao fiscal que anule a NFC-e da venda estornada e acorda a fila. Usa quem tiver a
/// permissão — o supervisor ou o próprio operador; sem nenhum dos dois, a lista mostra a nota
/// e o botão para tentar depois.
fn anular_nfce(
    ctx: &egui::Context,
    motor: &Motor,
    sessoes: [&Sessao; 2],
    cupom: Id,
    justificativa: &str,
) -> Result<Option<DocumentoResumo>, String> {
    let Some(sessao) = sessoes.into_iter().find(|s| s.concede("fiscal.emitir")) else {
        return Ok(None);
    };
    let nota: Option<DocumentoResumo> = motor
        .executar(
            sessao,
            "fiscal.anular_documento_da_venda.v1",
            &AnularDocumentoDaVenda {
                cupom,
                justificativa: justificativa.to_owned(),
            },
        )
        .map_err(|e| e.mensagem)?;
    if nota.is_some() {
        crate::fiscal_fundo::disparar(ctx, motor, sessao);
    }
    Ok(nota)
}

/// O que dizer ao operador depois do estorno: quanto devolver e por onde.
fn frase_da_devolucao(numero: i64, estorno: &VendaFoiEstornada) -> String {
    let mut dinheiro = Dinheiro::ZERO;
    let mut fora = Dinheiro::ZERO;
    for p in &estorno.devolver {
        if p.forma == FormaPagamentoPdv::Dinheiro {
            dinheiro += p.valor;
        } else {
            fora += p.valor;
        }
    }
    let mut partes = Vec::new();
    if dinheiro.e_positivo() {
        partes.push(format!(
            "devolva {} em dinheiro",
            dinheiro.formatar_com_simbolo()
        ));
    }
    if fora.e_positivo() {
        partes.push(format!(
            "{} de Pix/cartão se devolve pelo banco ou pela maquininha",
            fora.formatar_com_simbolo()
        ));
    }
    format!("Venda nº {numero} estornada: {}.", partes.join("; "))
}

/// Estorna a venda selecionada em nome do supervisor e anula a NFC-e dela. `Ok` traz a frase
/// para o operador.
pub(super) fn estornar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    dlg: &mut EstadoVendasDoDia,
) -> Result<String, String> {
    let (Some(venda), Some(form)) = (dlg.venda_sel().cloned(), dlg.estorno.as_ref()) else {
        return Err("Escolha a venda a estornar.".to_owned());
    };
    let motivo = form.motivo.trim().to_owned();
    if motivo.chars().count() < mod_pdv::MOTIVO_MINIMO_DO_ESTORNO {
        return Err("Explique o motivo do estorno (ao menos 10 caracteres).".to_owned());
    }
    let login = form.supervisor.trim().to_owned();
    if login.is_empty() || form.senha.is_empty() {
        return Err("Estornar uma venda exige o login e a senha de um supervisor.".to_owned());
    }
    let autorizador = motor
        .autenticar(&login, &form.senha)
        .map_err(|e| e.mensagem)?;
    if !autorizador.concede(PERMISSAO_ESTORNAR) {
        return Err(format!("{login} não pode autorizar o estorno de venda."));
    }
    let fora_do_prazo = prazo_da_nota(dlg, Instante::agora()).is_some_and(|p| !p.dentro());
    let estorno: VendaFoiEstornada = motor
        .executar(
            &autorizador,
            "pdv.estornar_venda.v1",
            &EstornarVenda {
                cupom: venda.cupom,
                motivo: motivo.clone(),
                autorizado_por: autorizador.usuario(),
            },
        )
        .map_err(|e| e.mensagem)?;
    let mut frase = frase_da_devolucao(venda.numero, &estorno);
    if fora_do_prazo {
        frase.push_str(
            " A NFC-e passou do prazo de cancelamento: se a SEFAZ recusar, emita uma NFC-e de \
             devolução.",
        );
    }
    if let Err(e) = anular_nfce(
        ctx,
        motor,
        [&autorizador, sessao],
        venda.cupom,
        &justificativa(venda.numero, &motivo),
    ) {
        frase.push_str(&format!(" A NFC-e ficou para anular: {e}"));
    }
    let sel = dlg.sel;
    *dlg = EstadoVendasDoDia::carregar(motor, sessao).unwrap_or_default();
    dlg.sel = sel.filter(|&i| i < dlg.vendas.len());
    Ok(frase)
}

pub(super) fn dialogo_vendas_do_dia(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaPdv,
) {
    let Dlg::VendasDoDia(dlg) = &estado.dlg else {
        return;
    };
    let (vendido, estornadas) = dlg.totais();
    let descricao = format!(
        "{} venda(s) · {} vendido{}",
        dlg.vendas.len(),
        vendido.formatar_com_simbolo(),
        if estornadas > 0 {
            format!(" · {estornadas} estornada(s)")
        } else {
            String::new()
        }
    );
    let nomes: HashMap<Id, String> = estado
        .clientes
        .iter()
        .map(|c| (c.pessoa, c.nome.clone()))
        .collect();
    let mut acao_corpo: Option<AcaoVendas> = None;
    let mut acao_rodape: Option<AcaoVendas> = None;

    let fechar = Dialogo::nova("Vendas de hoje")
        .descricao(descricao)
        .largura(860.0)
        .mostrar(
            ctx,
            &mut estado.dlg,
            |ui, dlg| {
                let Dlg::VendasDoDia(v) = dlg else { return };
                corpo(ctx, ui, v, &nomes, &mut acao_corpo);
            },
            |ui, dlg| {
                let Dlg::VendasDoDia(v) = dlg else { return };
                rodape(ui, v, &mut acao_rodape);
            },
        );

    let Dlg::VendasDoDia(dlg) = &mut estado.dlg else {
        return;
    };
    let acao = acao_rodape.or(acao_corpo);
    let fechar = fechar || matches!(acao, Some(AcaoVendas::Fechar));
    match acao {
        Some(AcaoVendas::AbrirEstorno) => dlg.estorno = Some(FormEstorno::default()),
        Some(AcaoVendas::Voltar) => dlg.estorno = None,
        Some(AcaoVendas::ConfirmarEstorno) => match estornar(ctx, motor, sessao, dlg) {
            Ok(frase) => notificar(ctx, Notificacao::sucesso(frase)),
            Err(e) => notificar(ctx, Notificacao::erro(e)),
        },
        Some(AcaoVendas::Comprovante(cupom)) => {
            super::acoes::imprimir_comprovante(ctx, motor, sessao, cupom, Dinheiro::ZERO, true);
        }
        Some(AcaoVendas::Imprimir(documento)) => {
            match crate::danfce::imprimir(motor, sessao, documento) {
                Ok(saida) => notificar(ctx, Notificacao::sucesso(saida.mensagem("Cupom fiscal"))),
                Err(e) => notificar(ctx, Notificacao::erro(e)),
            }
        }
        Some(AcaoVendas::AnularNfce) => {
            if let Some(venda) = dlg.venda_sel().cloned() {
                let motivo = venda
                    .estorno
                    .as_ref()
                    .map_or("venda desfeita", |e| e.motivo.as_str())
                    .to_owned();
                match anular_nfce(
                    ctx,
                    motor,
                    [sessao, sessao],
                    venda.cupom,
                    &justificativa(venda.numero, &motivo),
                ) {
                    Ok(_) => notificar(ctx, Notificacao::info("A NFC-e vai ser anulada pela fila")),
                    Err(e) => notificar(ctx, Notificacao::erro(e)),
                }
                dlg.carregar_notas(motor, sessao);
            }
        }
        Some(AcaoVendas::Fechar) | None => {}
    }
    if fechar && dlg.estorno.is_some() {
        // Esc com o formulário aberto só volta para a lista.
        dlg.estorno = None;
    } else if fechar {
        estado.dlg = Dlg::Fechado;
        estado.foco_entrada = true;
    }
}

enum AcaoVendas {
    AbrirEstorno,
    ConfirmarEstorno,
    Voltar,
    Imprimir(Id),
    Comprovante(Id),
    AnularNfce,
    Fechar,
}

fn corpo(
    ctx: &egui::Context,
    ui: &mut egui::Ui,
    v: &mut EstadoVendasDoDia,
    nomes: &HashMap<Id, String>,
    acao: &mut Option<AcaoVendas>,
) {
    if v.estorno.is_none() && !v.vendas.is_empty() {
        let n = v.vendas.len();
        if ui.input(|i| i.key_pressed(egui::Key::ArrowDown)) {
            v.sel = Some(v.sel.map_or(0, |s| (s + 1).min(n - 1)));
        }
        if ui.input(|i| i.key_pressed(egui::Key::ArrowUp)) {
            v.sel = Some(v.sel.map_or(0, |s| s.saturating_sub(1)));
        }
    }
    let colunas = vec![
        ColunaGrade::nova("Hora").largura(64.0),
        ColunaGrade::nova("Nº").largura(56.0).numero(),
        ColunaGrade::nova("Cliente").largura(170.0),
        ColunaGrade::nova("Pagamento").largura(170.0),
        ColunaGrade::nova("Total").largura(100.0).numero(),
        ColunaGrade::nova("Venda").largura(100.0),
        ColunaGrade::nova("NFC-e").largura(170.0),
    ];
    let vendas = &v.vendas;
    let notas = &v.notas;
    let resposta = Grade::nova(colunas)
        .id_salt("pdv-vendas-do-dia")
        .vazio("Nenhuma venda hoje.")
        .selecionavel(v.sel)
        .mostrar(ui, vendas.len(), |i, linha| {
            let venda = &vendas[i];
            linha.col(|ui| {
                ui.add(Rotulo::campo(
                    venda.abertura.hora(Fuso::BRASILIA).formatar(),
                ));
            });
            linha.col(|ui| {
                ui.add(Rotulo::campo(venda.numero.to_string()));
            });
            linha.col(|ui| {
                let nome = venda
                    .cliente
                    .and_then(|c| nomes.get(&c).cloned())
                    .unwrap_or_else(|| "—".to_owned());
                ui.add(Rotulo::campo(nome));
            });
            linha.col(|ui| {
                let formas: Vec<&str> = venda
                    .pagamentos
                    .iter()
                    .map(|p| forma_rotulo(p.forma))
                    .collect();
                ui.add(Rotulo::campo(formas.join(" + ")));
            });
            linha.col(|ui| {
                ui.add(Rotulo::campo(venda.total.formatar_com_simbolo()));
            });
            linha.col(|ui| {
                ui.add(if venda.estado == EstadoCupom::Estornado {
                    Etiqueta::negativa("estornada")
                } else {
                    Etiqueta::positiva("paga")
                });
            });
            linha.col(|ui| {
                ui.add(etiqueta_nota(notas.get(&venda.cupom)));
            });
        });
    if let Some(i) = resposta.linha_clicada {
        v.sel = Some(i);
        v.estorno = None;
    }

    let Some(venda) = v.venda_sel().cloned() else {
        return;
    };
    if let Some(e) = &venda.estorno {
        ui.add_space(Espaco::E8);
        ui.add(Rotulo::apoio(format!(
            "Estornada às {} — {}",
            e.em.hora(Fuso::BRASILIA).formatar(),
            e.motivo
        )));
    }
    if let Some(p) = prazo_da_nota(v, Instante::agora()) {
        if p.dentro() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(30));
        }
        ui.add_space(Espaco::E8);
        ui.add(
            Rotulo::apoio(aviso_do_prazo(p))
                .quebravel()
                .cor(if p.dentro() {
                    ui.cores().texto_fraco
                } else {
                    ui.cores().atencao
                }),
        );
    }
    if v.nota_sel().is_some_and(DocumentoResumo::anulacao_pendente) {
        ui.add_space(Espaco::E8);
        ui.add(
            Rotulo::apoio(
                "A SEFAZ recusou o cancelamento desta NFC-e (em geral, por passar do prazo). \
                 A nota continua valendo: emita uma NFC-e de devolução ou fale com o contador.",
            )
            .quebravel(),
        );
    }
    let Some(form) = v.estorno.as_mut() else {
        return;
    };
    ui.add_space(Espaco::E12);
    ui.add(
        Rotulo::interface(format!(
            "Estornar a venda nº {} ({}): a mercadoria volta ao estoque, o lançamento é estornado \
             e a NFC-e, se houver, é cancelada ou inutilizada pela fila.",
            venda.numero,
            venda.total.formatar_com_simbolo()
        ))
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    let r1 = ui.add(Campo::novo("Motivo", &mut form.motivo).marcador("Cliente desistiu da compra"));
    focar_primeiro(ctx, &r1);
    ui.add_space(Espaco::E8);
    let (r2, r3) = ui.columns(2, |c| {
        (
            c[0].add(Campo::novo("Supervisor (login)", &mut form.supervisor)),
            c[1].add(Campo::novo("Senha", &mut form.senha).senha(true)),
        )
    });
    if (r1.lost_focus() || r2.lost_focus() || r3.lost_focus()) && enter_pressionado(ui) {
        *acao = Some(AcaoVendas::ConfirmarEstorno);
    }
}

fn rodape(ui: &mut egui::Ui, v: &EstadoVendasDoDia, acao: &mut Option<AcaoVendas>) {
    if v.estorno.is_some() {
        if ui
            .add(Botao::destrutivo("Confirmar estorno").atalho("Enter"))
            .clicked()
        {
            *acao = Some(AcaoVendas::ConfirmarEstorno);
        }
        if ui.add(Botao::secundario("Voltar").atalho("Esc")).clicked() {
            *acao = Some(AcaoVendas::Voltar);
        }
        return;
    }
    let venda = v.venda_sel();
    let nota = v.nota_sel();
    if ui
        .add(
            Botao::destrutivo("Estornar venda")
                .habilitado(venda.is_some_and(|v| v.estado == EstadoCupom::Finalizado)),
        )
        .clicked()
    {
        *acao = Some(AcaoVendas::AbrirEstorno);
    }
    if let Some(v) = venda {
        if ui.add(Botao::secundario("Imprimir comprovante")).clicked() {
            *acao = Some(AcaoVendas::Comprovante(v.cupom));
        }
    }
    if let Some(d) = nota.filter(|d| {
        matches!(
            d.estado,
            EstadoDocumento::Autorizada | EstadoDocumento::Contingencia
        ) && !d.anulada
    }) {
        if ui.add(Botao::secundario("Imprimir cupom fiscal")).clicked() {
            *acao = Some(AcaoVendas::Imprimir(d.id));
        }
    }
    let falta_anular = venda.is_some_and(|v| v.estado == EstadoCupom::Estornado)
        && nota.is_some_and(|d| !d.anulada);
    if falta_anular && ui.add(Botao::secundario("Anular NFC-e")).clicked() {
        *acao = Some(AcaoVendas::AnularNfce);
    }
    if ui.add(Botao::fantasma("Fechar").atalho("Esc")).clicked() {
        *acao = Some(AcaoVendas::Fechar);
    }
}

#[cfg(test)]
pub(super) mod testes {
    use super::*;

    #[test]
    fn o_prazo_da_nfce_avisa_quem_estorna() {
        let p = |min: i64| PrazoDeCancelamento {
            modelo: mod_fiscal::ModeloFiscal::Nfce,
            decorrido_segundos: min * 60,
        };
        assert_eq!(
            aviso_do_prazo(p(12)),
            "NFC-e autorizada há 12 min: estornando agora, o cancelamento vai dentro do prazo \
             (faltam 18 min)."
        );
        assert!(aviso_do_prazo(p(27)).contains("Falta pouco"));
        assert!(aviso_do_prazo(p(45)).contains("passou o prazo de cancelamento (30 min)"));
    }

    #[test]
    fn justificativa_cabe_na_sefaz() {
        let j = justificativa(7, "Cliente desistiu");
        assert!((15..=255).contains(&j.chars().count()));
        assert_eq!(justificativa(7, &"x".repeat(400)).chars().count(), 255);
    }

    #[test]
    fn frase_separa_o_dinheiro_do_que_volta_pelo_banco() {
        let pg = |forma, reais| mod_pdv::PagamentoCupom {
            id: Id::novo(),
            forma,
            valor: Dinheiro::reais(reais),
            prazo: None,
        };
        let estorno = VendaFoiEstornada {
            lancamentos: vec![],
            itens_devolvidos: 1,
            devolver: vec![
                pg(FormaPagamentoPdv::Dinheiro, 30),
                pg(FormaPagamentoPdv::Pix, 20),
            ],
        };
        let f = frase_da_devolucao(3, &estorno);
        assert!(f.contains("devolva R$ 30,00 em dinheiro"), "{f}");
        assert!(f.contains("R$ 20,00 de Pix/cartão"), "{f}");
    }

    /// Uma venda de verdade: produto com estoque, tabela de preço, caixa aberto, cupom pago em
    /// dinheiro (R$ 25,00).
    fn vender(m: &Motor, s: &Sessao) -> Id {
        let cupom = cupom_com_um_item(m, s);
        let _: VendaFoiFinalizada = m
            .executar(
                s,
                "pdv.finalizar_venda.v1",
                &FinalizarVenda {
                    cupom,
                    pagamentos: vec![mod_pdv::PagamentoInformado {
                        forma: FormaPagamentoPdv::Dinheiro,
                        valor: Dinheiro::reais(25),
                        prazo: None,
                    }],
                },
            )
            .unwrap();
        cupom
    }

    /// O cupom aberto, com um item de R$ 25,00: produto com estoque, tabela de preço e caixa
    /// aberto.
    pub(in crate::tela_pdv) fn cupom_com_um_item(m: &Motor, s: &Sessao) -> Id {
        use mod_estoque::{
            CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, GrupoProdutoCriado,
            LocalCriado, ProdutoCriado, RegistrarEntrada, TipoLocal, UnidadeCriada,
        };
        use mod_vendas::{CriarRegraPreco, CriarTabelaPreco, TabelaPrecoCriada, TipoTabela};
        let grupo: GrupoProdutoCriado = m
            .executar(
                s,
                "estoque.criar_grupo_produto.v1",
                &CriarGrupoProduto {
                    codigo: "G".into(),
                    nome: "Geral".into(),
                    pai: None,
                },
            )
            .unwrap();
        let unidade: UnidadeCriada = m
            .executar(
                s,
                "estoque.criar_unidade.v1",
                &CriarUnidade {
                    sigla: "UN".into(),
                    nome: "Unidade".into(),
                    fracionavel: false,
                },
            )
            .unwrap();
        let produto: ProdutoCriado = m
            .executar(
                s,
                "estoque.criar_produto.v1",
                &CriarProduto {
                    grupo_produto: grupo.grupo_produto,
                    nome: "Capinha".into(),
                    ncm: "39269090".into(),
                    unidade_padrao: unidade.unidade,
                    codigo_barras: None,
                    detalhes_tecnicos: None,
                },
            )
            .unwrap();
        let local: LocalCriado = m
            .executar(
                s,
                "estoque.criar_local.v1",
                &CriarLocal {
                    nome: "Loja".into(),
                    tipo: TipoLocal::Deposito,
                },
            )
            .unwrap();
        let _: mod_estoque::EntradaRegistrada = m
            .executar(
                s,
                "estoque.registrar_entrada.v1",
                &RegistrarEntrada {
                    produto: produto.produto,
                    local: local.local,
                    quantidade: Quantidade::unidades(10),
                    custo_unitario: Preco::reais(10),
                },
            )
            .unwrap();
        let tabela: TabelaPrecoCriada = m
            .executar(
                s,
                "vendas.criar_tabela_preco.v1",
                &CriarTabelaPreco {
                    nome: "Balcão".into(),
                    tipo: TipoTabela::Venda,
                    vigente_de: Data::hoje(Fuso::BRASILIA).mais_dias(-1),
                    vigente_ate: None,
                },
            )
            .unwrap();
        let _: mod_vendas::RegraPrecoCriada = m
            .executar(
                s,
                "vendas.criar_regra_preco.v1",
                &CriarRegraPreco {
                    tabela_preco: tabela.tabela_preco,
                    alvo_produto: Some(produto.produto),
                    alvo_grupo: None,
                    quantidade_minima: None,
                    preco: Preco::reais(25),
                    periodo_de: None,
                    periodo_ate: None,
                },
            )
            .unwrap();
        let contas: Vec<ItemContaResultado> = m
            .consultar(s, "financeiro.contas_caixa.v1", &ContasDeCaixa)
            .unwrap();
        let caixa: CaixaCadastrado = m
            .executar(
                s,
                "financeiro.cadastrar_caixa.v1",
                &CadastrarCaixa {
                    nome: "Caixa 1".into(),
                    local_operacao: None,
                    conta_razao: contas[0].conta,
                    permite_negativo: false,
                },
            )
            .unwrap();
        let sessao_caixa: CaixaFoiAberto = m
            .executar(
                s,
                "financeiro.abrir_caixa.v1",
                &AbrirCaixa {
                    caixa: caixa.caixa,
                    valor_abertura: Dinheiro::ZERO,
                },
            )
            .unwrap();
        let cupom: CupomAberto = m
            .executar(
                s,
                "pdv.abrir_cupom.v1",
                &AbrirCupom {
                    sessao_caixa: sessao_caixa.sessao,
                    terminal: Id::novo(),
                    serie_fiscal: 1,
                    cliente: None,
                    tabela_preco: tabela.tabela_preco,
                    local_expedicao: local.local,
                    vendedor: None,
                },
            )
            .unwrap();
        let _: ItemFoiAdicionado = m
            .executar(
                s,
                "pdv.adicionar_item.v1",
                &AdicionarItem {
                    cupom: cupom.cupom,
                    produto: produto.produto,
                    variacao: None,
                    quantidade: Quantidade::unidades(1),
                },
            )
            .unwrap();
        cupom.cupom
    }

    #[test]
    fn estorna_a_venda_de_hoje_com_supervisor() {
        let t = crate::testes_comum::motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let cupom = vender(m, s);
        let ctx = egui::Context::default();
        let mut dlg = EstadoVendasDoDia::carregar(m, s).unwrap();
        assert_eq!(dlg.vendas.len(), 1);
        assert_eq!((dlg.sel, dlg.vendas[0].cupom), (Some(0), cupom));

        let form = |motivo: &str, senha: &str| FormEstorno {
            motivo: motivo.into(),
            supervisor: "igor".into(),
            senha: senha.into(),
        };
        dlg.estorno = Some(form("curto", "senha-forte-123"));
        assert!(estornar(&ctx, m, s, &mut dlg)
            .unwrap_err()
            .contains("motivo"));
        dlg.estorno = Some(form("Cliente desistiu da compra", "errada"));
        assert!(estornar(&ctx, m, s, &mut dlg).is_err());
        assert_eq!(dlg.vendas[0].estado, EstadoCupom::Finalizado, "nada mudou");

        dlg.estorno = Some(form("Cliente desistiu da compra", "senha-forte-123"));
        let frase = estornar(&ctx, m, s, &mut dlg).unwrap();
        assert!(frase.contains("devolva R$ 25,00 em dinheiro"), "{frase}");
        assert_eq!(dlg.vendas[0].estado, EstadoCupom::Estornado);
        assert!(
            dlg.estorno.is_none(),
            "a lista recarregada fecha o formulário"
        );

        // O diálogo desenha a venda estornada sem tropeçar.
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let mut estado = EstadoTelaPdv {
            dlg: Dlg::VendasDoDia(dlg),
            ..EstadoTelaPdv::default()
        };
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                dialogo_vendas_do_dia(ctx, m, s, &mut estado);
            });
        }
        assert!(matches!(estado.dlg, Dlg::VendasDoDia(_)));
    }

    #[test]
    fn comprovante_da_venda_sai_na_bobina_configurada() {
        let t = crate::testes_comum::motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let cupom = vender(m, s);
        let largura = |pdf: &[u8]| -> f32 {
            let texto = String::from_utf8_lossy(pdf);
            let i = texto.find("/MediaBox [0 0 ").unwrap() + "/MediaBox [0 0 ".len();
            texto[i..]
                .split_whitespace()
                .next()
                .unwrap()
                .parse()
                .unwrap()
        };

        let pdf = std::fs::read(
            crate::comprovante::imprimir(m, s, cupom, Dinheiro::reais(5), false)
                .unwrap()
                .caminho()
                .unwrap(),
        )
        .unwrap();
        let texto = String::from_utf8_lossy(&pdf).into_owned();
        assert!(texto.contains("COMPROVANTE DE VENDA"));
        assert!(texto.contains("Troco"));
        assert!((largura(&pdf) - 226.772).abs() < 0.01);

        let r: Result<(), _> = m.executar(
            s,
            "pdv.definir_impressao.v1",
            &mod_pdv::DefinirImpressaoPdv {
                impressao: mod_pdv::ImpressaoPdv {
                    bobina: mod_pdv::LarguraBobina::Mm58,
                    imprimir_ao_finalizar: true,
                    rodape: Some("Trocas em até 7 dias".into()),
                },
            },
        );
        r.unwrap();
        let pdf = std::fs::read(
            crate::comprovante::imprimir(m, s, cupom, Dinheiro::ZERO, true)
                .unwrap()
                .caminho()
                .unwrap(),
        )
        .unwrap();
        let texto = String::from_utf8_lossy(&pdf).into_owned();
        assert!((largura(&pdf) - 164.409).abs() < 0.01, "58 mm");
        assert!(texto.contains("Trocas em"));
        assert!(!texto.contains("Troco"), "a reimpressão não sabe o troco");
        // Com a térmica configurada, o mesmo comprovante vai em ESC/POS, na bobina de 58 mm.
        let (endereco, recebido) = crate::impressora::testes::impressora_de_rede();
        let saida = crate::comprovante::imprimir_em(
            m,
            s,
            cupom,
            Dinheiro::ZERO,
            true,
            &crate::maquina::ImpressoraDaMaquina {
                destino: crate::maquina::Destino::Rede(endereco.clone()),
                acentos: false,
            },
        )
        .unwrap();
        assert_eq!(saida, crate::impressora::Saida::Impressora(endereco));
        let bytes = recebido.join().unwrap();
        assert!(bytes.starts_with(b"\x1b@"));
        let texto = String::from_utf8_lossy(&bytes);
        assert!(texto.contains("COMPROVANTE DE VENDA") && texto.contains("Trocas em"));
        assert!(texto.contains(&"-".repeat(32)), "32 colunas na de 58 mm");
        // A tela de PDV lê a preferência.
        let mut estado = EstadoTelaPdv::default();
        estado.carregar(m, s);
        assert!(estado.impressao.imprimir_ao_finalizar);
    }
}
