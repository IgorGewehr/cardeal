//! Notas fiscais — a fila e o histórico das NFC-e das vendas (`docs/modulos/fiscal.md`): o que
//! está na fila, conferindo, rejeitado (com o que corrigir) e autorizado. Daqui se reenvia a
//! nota rejeitada depois de corrigir e se manda transmitir na hora.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Fuso, Id, Instante};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom, ValorDinheiro};
use cardeal_ui::molecules::{dado, Abas, Campo, CartaoKpi, EstadoVazio};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, FaixaKpi, Grade, LayoutTela, Notificacao, Painel,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_fiscal::{
    AmbienteFiscal, CancelarNfce, DetalheDocumento, DocumentoFiscal, DocumentoResumo,
    DocumentosFiscais, EstadoDocumento, InutilizarNumero, PedidoDocumento, ReenviarDocumento,
    ResumoDaFilaFiscal, ResumoFiscal,
};

use crate::fiscal_fundo;

const FUSO: Fuso = Fuso::BRASILIA;

/// O filtro da lista.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Filtro {
    /// Tudo o que precisa de atenção: na fila, conferindo, rejeitadas.
    #[default]
    Atencao,
    /// Só as autorizadas.
    Autorizadas,
    /// Todas.
    Todas,
}

/// O estado da tela.
#[derive(Default)]
pub struct EstadoNotas {
    pub documentos: Vec<DocumentoResumo>,
    pub resumo: Option<ResumoFiscal>,
    pub filtro: Filtro,
    pub detalhe: Option<DetalheDocumento>,
    pub erro: Option<String>,
    /// A geração da transmissão em segundo plano que esta lista já mostra.
    pub geracao_vista: u64,
    /// O cancelamento/inutilização sendo pedido no detalhe: o pedido e a justificativa.
    pub pedindo: Option<(PedidoDocumento, String)>,
}

/// Pede o cancelamento (autorizada) ou a inutilização (rejeitada) do documento e dispara a
/// transmissão.
///
/// # Errors
/// O erro do comando (justificativa curta, estado errado, pedido repetido).
pub fn pedir(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoNotas,
    documento: Id,
    pedido: PedidoDocumento,
    justificativa: &str,
) -> Result<DocumentoResumo, String> {
    let justificativa = justificativa.trim().to_owned();
    let r: Result<DocumentoResumo, _> = match pedido {
        PedidoDocumento::Cancelar => motor.executar(
            sessao,
            "fiscal.cancelar_nfce.v1",
            &CancelarNfce {
                documento,
                justificativa,
            },
        ),
        PedidoDocumento::Inutilizar => motor.executar(
            sessao,
            "fiscal.inutilizar_numero.v1",
            &InutilizarNumero {
                documento,
                justificativa,
            },
        ),
        PedidoDocumento::CancelarPorSubstituicao => {
            return Err("O cancelamento por substituição a fila pede sozinha".to_owned());
        }
    };
    let d = r.map_err(|e| e.mensagem)?;
    estado.pedindo = None;
    fiscal_fundo::disparar(ctx, motor, sessao);
    estado.carregar(motor, sessao);
    Ok(d)
}

impl EstadoNotas {
    /// Carrega a lista e o resumo.
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        self.geracao_vista = fiscal_fundo::geracao();
        match motor.consultar(
            sessao,
            "fiscal.documentos.v1",
            &DocumentosFiscais { estado: None },
        ) {
            Ok(d) => {
                self.documentos = d;
                self.erro = None;
            }
            Err(e) => {
                self.documentos.clear();
                self.erro = Some(e.mensagem);
            }
        }
        self.resumo = motor
            .consultar(sessao, "fiscal.resumo_da_fila.v1", &ResumoDaFilaFiscal)
            .ok();
        if let Some(d) = &self.detalhe {
            let id = d.resumo.id;
            self.detalhe = motor
                .consultar(
                    sessao,
                    "fiscal.documento.v1",
                    &DocumentoFiscal { documento: id },
                )
                .ok();
        }
    }

    fn visiveis(&self) -> Vec<&DocumentoResumo> {
        self.documentos
            .iter()
            .filter(|d| match self.filtro {
                Filtro::Atencao => matches!(
                    d.estado,
                    EstadoDocumento::Pendente
                        | EstadoDocumento::Incerta
                        | EstadoDocumento::Contingencia
                        | EstadoDocumento::Rejeitada
                ),
                Filtro::Autorizadas => d.estado == EstadoDocumento::Autorizada,
                Filtro::Todas => true,
            })
            .collect()
    }
}

/// Reenvia uma nota rejeitada e dispara a transmissão.
///
/// # Errors
/// O erro do comando.
pub fn reenviar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoNotas,
    documento: Id,
) -> Result<DocumentoResumo, String> {
    let d: DocumentoResumo = motor
        .executar(
            sessao,
            "fiscal.reenviar_documento.v1",
            &ReenviarDocumento { documento },
        )
        .map_err(|e| e.mensagem)?;
    if d.estado == EstadoDocumento::Pendente {
        fiscal_fundo::disparar(ctx, motor, sessao);
    }
    estado.carregar(motor, sessao);
    Ok(d)
}

fn etiqueta(d: &DocumentoResumo) -> Etiqueta {
    let rotulo = d.estado.rotulo();
    match d.estado {
        EstadoDocumento::Autorizada => Etiqueta::positiva(rotulo),
        EstadoDocumento::Pendente | EstadoDocumento::Incerta => Etiqueta::info(rotulo),
        EstadoDocumento::Contingencia => Etiqueta::atencao(rotulo),
        EstadoDocumento::Rejeitada | EstadoDocumento::Denegada => Etiqueta::negativa(rotulo),
        EstadoDocumento::Cancelada | EstadoDocumento::Inutilizada => Etiqueta::neutra(rotulo),
    }
}

/// O que a linha da nota diz: a chave da autorizada; o prazo da que está em contingência; o
/// motivo das outras.
fn texto_da_linha(d: &DocumentoResumo) -> String {
    let fechada = matches!(
        d.estado,
        EstadoDocumento::Cancelada | EstadoDocumento::Inutilizada
    );
    if d.substituida_por.is_some() && !fechada {
        return "Ficou sem resposta da SEFAZ e foi substituída por outra NFC-e, em contingência — \
                esta vai ser cancelada por substituição ou inutilizada sozinha."
            .to_owned();
    }
    match (d.estado, d.prazo_da_contingencia()) {
        (EstadoDocumento::Autorizada, _) => d.chave.clone().unwrap_or_default(),
        (EstadoDocumento::Contingencia, Some(prazo)) => format!(
            "Cupom fiscal emitido em contingência — a SEFAZ estava fora. Transmitir até {}.",
            prazo.formatar(cardeal_kernel::Fuso::BRASILIA)
        ),
        _ => d.motivo.clone().unwrap_or_default(),
    }
}

/// A tela.
pub fn mostrar(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoNotas) {
    if estado.geracao_vista != fiscal_fundo::geracao() {
        estado.carregar(motor, sessao);
    }
    LayoutTela::nova("Notas fiscais").mostrar(
        ui,
        estado,
        |ui, _| {
            if fiscal_fundo::rodando() {
                ui.add(Etiqueta::info("transmitindo…").com_ponto());
            }
        },
        |ui, estado| corpo(ui, motor, sessao, estado),
    );
}

fn corpo(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoNotas) {
    if let Some(e) = &estado.erro {
        EstadoVazio::novo(Icone::Nota, e.clone()).mostrar(ui);
        return;
    }
    let r = estado.resumo.clone().unwrap_or_default();
    let tom_se = |n: u32, tom: Tom| if n == 0 { Tom::Neutro } else { tom };
    let mut cartoes = vec![
        CartaoKpi::contagem("Na fila", r.pendentes).tom(tom_se(r.pendentes, Tom::Atencao)),
        CartaoKpi::contagem("Conferindo", r.incertas).tom(tom_se(r.incertas, Tom::Atencao)),
        CartaoKpi::contagem("Rejeitadas", r.rejeitadas).tom(tom_se(r.rejeitadas, Tom::Negativo)),
        CartaoKpi::contagem("Autorizadas hoje", r.autorizadas_hoje).tom(Tom::Positivo),
    ];
    // A contingência só aparece quando há: a SEFAZ esteve fora e o prazo de 24 h corre.
    if r.em_contingencia > 0 {
        let prazo = r.contingencia_desde.map_or_else(String::new, |d| {
            format!(
                "transmitir até {}",
                d.mais_segundos(mod_fiscal::PRAZO_DA_CONTINGENCIA_SEGUNDOS)
                    .formatar(cardeal_kernel::Fuso::BRASILIA)
            )
        });
        cartoes.insert(
            2,
            CartaoKpi::contagem("Em contingência", r.em_contingencia)
                .variacao(prazo)
                .tom(Tom::Atencao),
        );
    }
    FaixaKpi::nova(cartoes).mostrar(ui);
    ui.add_space(Espaco::E12);
    ui.horizontal(|ui| {
        if let Some(f) = Abas::nova(&[
            (Filtro::Atencao, "Precisam de atenção"),
            (Filtro::Autorizadas, "Autorizadas"),
            (Filtro::Todas, "Todas"),
        ])
        .selecionada(estado.filtro)
        .id_salt("notas-filtro")
        .mostrar(ui)
        {
            estado.filtro = f;
        }
        ui.add_space(Espaco::E12);
        let pode = sessao.concede("fiscal.emitir") && !fiscal_fundo::rodando();
        if ui
            .add_enabled(pode, Botao::secundario("Transmitir agora").pequeno())
            .clicked()
            && fiscal_fundo::disparar(ui.ctx(), motor, sessao)
        {
            notificar(ui.ctx(), Notificacao::info("Transmitindo a fila…"));
        }
        if let Some(Err(m)) = fiscal_fundo::ultimo_resultado() {
            ui.add_space(Espaco::E8);
            ui.add(Rotulo::apoio(format!("Última tentativa: {m}")).cor(ui.cores().negativo));
        }
    });
    ui.add_space(Espaco::E8);
    grade(ui, motor, sessao, estado);
    detalhe(ui, motor, sessao, estado);
}

fn grade(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoNotas) {
    let linhas: Vec<DocumentoResumo> = estado.visiveis().into_iter().cloned().collect();
    let pode_reenviar = sessao.concede("fiscal.emitir");
    let mut reenviar_id = None;
    let mut ver_id = None;
    Grade::nova(vec![
        ColunaGrade::nova("Nº").largura(80.0),
        ColunaGrade::nova("Data").largura(110.0),
        ColunaGrade::nova("Total").largura(110.0).numero(),
        ColunaGrade::nova("Situação").largura(120.0),
        ColunaGrade::nova("Detalhe"),
        ColunaGrade::nova("").largura(180.0),
    ])
    .com_acoes()
    .id_salt("notas-fiscais")
    .vazio(match estado.filtro {
        Filtro::Atencao => "Nenhuma nota precisa de atenção.",
        _ => "Nenhuma nota neste recorte.",
    })
    .mostrar(ui, linhas.len(), |i, row| {
        let d = &linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(format!("{}-{}", d.serie, d.numero)));
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(d.criado_em.data(FUSO).formatar()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(d.total).neutro());
        });
        row.col(|ui| {
            ui.horizontal(|ui| {
                ui.add(etiqueta(d));
                if let Some(p) = d.pedido {
                    ui.add(Etiqueta::atencao(p.rotulo()));
                }
                if d.anulacao_pendente() {
                    ui.add(Etiqueta::negativa("venda desfeita, cancelamento recusado"));
                } else if d.anulada {
                    ui.add(Etiqueta::neutra("venda desfeita"));
                }
                if d.ambiente == AmbienteFiscal::Homologacao {
                    ui.add(Etiqueta::neutra("teste"));
                }
            });
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(texto_da_linha(d)).quebravel());
        });
        row.col(|ui| {
            ui.horizontal(|ui| {
                if d.estado == EstadoDocumento::Rejeitada
                    && pode_reenviar
                    && ui.add(Botao::primario("Reenviar").pequeno()).clicked()
                {
                    reenviar_id = Some(d.id);
                }
                if ui.add(Botao::fantasma("Detalhes").pequeno()).clicked() {
                    ver_id = Some(d.id);
                }
            });
        });
    });
    if let Some(id) = reenviar_id {
        match reenviar(ui.ctx(), motor, sessao, estado, id) {
            Ok(d) if d.estado == EstadoDocumento::Pendente => {
                notificar(ui.ctx(), Notificacao::sucesso("Nota de volta na fila"));
            }
            Ok(d) => notificar(
                ui.ctx(),
                Notificacao::aviso(d.motivo.unwrap_or_else(|| "Ainda falta corrigir".into())),
            ),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
    if let Some(id) = ver_id {
        estado.detalhe = motor
            .consultar(
                sessao,
                "fiscal.documento.v1",
                &DocumentoFiscal { documento: id },
            )
            .ok();
    }
}

/// O que o pedido de cancelamento diz, com o prazo (30 minutos para a NFC-e).
fn texto_do_cancelamento(prazo: Option<mod_fiscal::PrazoDeCancelamento>) -> String {
    let inicio = prazo.map_or_else(
        || "Cancelar esta NFC-e na SEFAZ.".to_owned(),
        mod_fiscal::PrazoDeCancelamento::aviso,
    );
    format!("{inicio} A venda em si não é desfeita aqui.")
}

fn detalhe(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoNotas) {
    let Some(d) = estado.detalhe.clone() else {
        return;
    };
    let pode_cancelar = sessao.concede("fiscal.cancelar") && d.resumo.pedido.is_none();
    // O prazo de cancelamento corre: avisa, não bloqueia (algumas UFs aceitam fora dele).
    let prazo = d
        .resumo
        .prazo_de_cancelamento(Instante::agora())
        .filter(|_| d.resumo.pedido.is_none() && !d.resumo.anulada);
    if prazo.is_some_and(|p| p.dentro()) {
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_secs(30));
    }
    let mut fechar = false;
    let mut imprimir = None;
    let mut abrir_pedido = None;
    let mut confirmar = false;
    let mut desistir = false;
    ui.add_space(Espaco::E12);
    Painel::novo()
        .titulo(format!("NFC-e {}-{}", d.resumo.serie, d.resumo.numero))
        .mostrar(ui, |ui| {
            ui.columns(3, |c| {
                dado(&mut c[0], "Situação", d.resumo.estado.rotulo());
                dado(&mut c[1], "Total", &d.resumo.total.formatar_com_simbolo());
                dado(
                    &mut c[2],
                    "Ambiente",
                    match d.resumo.ambiente {
                        AmbienteFiscal::Homologacao => "Homologação (teste)",
                        AmbienteFiscal::Producao => "Produção",
                    },
                );
            });
            ui.add_space(Espaco::E8);
            if let Some(chave) = &d.resumo.chave {
                dado(ui, "Chave de acesso", chave);
                ui.add_space(Espaco::E8);
            }
            if let Some(p) = &d.resumo.protocolo {
                dado(ui, "Protocolo", p);
                ui.add_space(Espaco::E8);
            }
            if let Some(p) = prazo {
                ui.add(Rotulo::sobrelinha("Prazo de cancelamento"));
                ui.add(
                    Rotulo::interface(p.curto()).cor(if p.dentro() {
                        ui.cores().texto
                    } else {
                        ui.cores().atencao
                    }),
                );
                ui.add_space(Espaco::E8);
            }
            if let Some(m) = &d.resumo.motivo {
                dado(ui, "Última resposta", m);
                ui.add_space(Espaco::E8);
            }
            if let Some(nota) = &d.nota {
                ui.add(Rotulo::sobrelinha("Itens"));
                for item in &nota.itens {
                    ui.add(Rotulo::campo(format!(
                        "{} × {} — {} (NCM {}, CFOP {})",
                        item.quantidade.formatar(0),
                        item.descricao,
                        item.valor_liquido().formatar_com_simbolo(),
                        item.ncm,
                        item.cfop
                    )));
                }
                if let Some(c) = &nota.consumidor {
                    ui.add_space(Espaco::E4);
                    let doc = match c.documento {
                        cardeal_fiscal::DocumentoConsumidor::Cpf(cpf) => {
                            format!("CPF {}", cpf.mascarado())
                        }
                        cardeal_fiscal::DocumentoConsumidor::Cnpj(cnpj) => {
                            format!("CNPJ {}", cnpj.formatar())
                        }
                    };
                    ui.add(Rotulo::apoio(format!("Consumidor: {doc}")));
                }
                ui.add_space(Espaco::E8);
            }
            if let Some(p) = d.resumo.pedido {
                ui.add(Etiqueta::atencao(p.rotulo()).com_ponto());
                ui.add_space(Espaco::E8);
            }
            if let Some((pedido, justificativa)) = estado.pedindo.as_mut() {
                let fora_do_prazo = prazo.is_some_and(|p| !p.dentro());
                let tom = if *pedido == PedidoDocumento::Cancelar && fora_do_prazo {
                    Tom::Negativo
                } else {
                    Tom::Atencao
                };
                Painel::novo().realce(tom).mostrar(ui, |ui| {
                    ui.add(Rotulo::interface(match pedido {
                        PedidoDocumento::Cancelar | PedidoDocumento::CancelarPorSubstituicao => {
                            texto_do_cancelamento(prazo)
                        }
                        PedidoDocumento::Inutilizar => {
                            "Inutilizar o número desta nota, que não vai mais ser enviada — sem isso ele fica como um buraco na série.".to_owned()
                        }
                    }).quebravel());
                    ui.add_space(Espaco::E8);
                    ui.add(
                        Campo::novo("Justificativa (15 a 255 caracteres)", justificativa)
                            .multilinha(2),
                    );
                    ui.add_space(Espaco::E8);
                    ui.horizontal(|ui| {
                        confirmar = ui
                            .add(Botao::destrutivo(match pedido {
                                PedidoDocumento::Cancelar
                                | PedidoDocumento::CancelarPorSubstituicao => "Confirmar cancelamento",
                                PedidoDocumento::Inutilizar => "Confirmar inutilização",
                            }))
                            .clicked();
                        desistir = ui.add(Botao::fantasma("Desistir")).clicked();
                    });
                });
                ui.add_space(Espaco::E8);
            }
            ui.horizontal(|ui| {
                if matches!(
                    d.resumo.estado,
                    EstadoDocumento::Autorizada | EstadoDocumento::Contingencia
                ) && ui.add(Botao::secundario("Imprimir cupom fiscal")).clicked()
                {
                    imprimir = Some(d.resumo.id);
                }
                if pode_cancelar && estado.pedindo.is_none() {
                    if d.resumo.estado == EstadoDocumento::Autorizada
                        && ui.add(Botao::fantasma("Cancelar NFC-e")).clicked()
                    {
                        abrir_pedido = Some(PedidoDocumento::Cancelar);
                    }
                    if d.resumo.estado == EstadoDocumento::Rejeitada
                        && ui.add(Botao::fantasma("Inutilizar número")).clicked()
                    {
                        abrir_pedido = Some(PedidoDocumento::Inutilizar);
                    }
                }
                fechar = ui.add(Botao::fantasma("Fechar")).clicked();
            });
        });
    if let Some(p) = abrir_pedido {
        estado.pedindo = Some((p, String::new()));
    }
    if desistir {
        estado.pedindo = None;
    }
    if confirmar {
        if let Some((pedido, justificativa)) = estado.pedindo.clone() {
            match pedir(
                ui.ctx(),
                motor,
                sessao,
                estado,
                d.resumo.id,
                pedido,
                &justificativa,
            ) {
                Ok(_) => notificar(
                    ui.ctx(),
                    Notificacao::sucesso(match pedido {
                        PedidoDocumento::Cancelar | PedidoDocumento::CancelarPorSubstituicao => {
                            "Cancelamento na fila"
                        }
                        PedidoDocumento::Inutilizar => "Inutilização na fila",
                    }),
                ),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
            }
        }
    }
    if fechar {
        estado.detalhe = None;
        estado.pedindo = None;
    }
    if let Some(id) = imprimir {
        match crate::danfce::imprimir(motor, sessao, id) {
            Ok(saida) => notificar(
                ui.ctx(),
                Notificacao::sucesso(saida.mensagem("Cupom fiscal")),
            ),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;

    fn documento(estado: EstadoDocumento, numero: u64) -> DocumentoResumo {
        DocumentoResumo {
            id: Id::novo(),
            modelo: mod_fiscal::ModeloFiscal::Nfce,
            serie: 1,
            numero,
            ambiente: AmbienteFiscal::Homologacao,
            estado,
            total: cardeal_kernel::Dinheiro::reais(50),
            chave: (estado == EstadoDocumento::Autorizada).then(|| "4".repeat(44)),
            protocolo: None,
            codigo: None,
            motivo: (estado == EstadoDocumento::Rejeitada).then(|| "o produto está sem NCM".into()),
            origem: Id::novo(),
            tentativas: 0,
            criado_em: cardeal_kernel::Instante::agora(),
            autorizado_em: None,
            pedido: None,
            anulada: false,
            contingencia_em: None,
            substitui: None,
            substituida_por: None,
        }
    }

    fn desenhar(motor: &Motor, sessao: &Sessao, estado: &mut EstadoNotas) {
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    mostrar(ui, motor, sessao, estado);
                });
            });
        }
    }

    #[test]
    fn a_contingencia_pede_atencao_mostra_o_prazo_e_o_indicador() {
        let t = motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let mut estado = EstadoNotas::default();
        estado.carregar(m, s);
        let desde = cardeal_kernel::Instante::de_micros(1_793_030_400_000_000);
        let mut d = documento(EstadoDocumento::Contingencia, 4);
        d.contingencia_em = Some(desde);
        assert_eq!(
            texto_da_linha(&d),
            "Cupom fiscal emitido em contingência — a SEFAZ estava fora. Transmitir até \
             27/10/2026 13:00."
        );
        let mut substituida = documento(EstadoDocumento::Incerta, 3);
        substituida.substituida_por = Some(Id::novo());
        assert!(texto_da_linha(&substituida).contains("foi substituída por outra NFC-e"));
        estado.documentos = vec![d, documento(EstadoDocumento::Autorizada, 5)];
        assert_eq!(estado.visiveis().len(), 1, "a contingência pede atenção");
        estado.resumo = Some(ResumoFiscal {
            em_contingencia: 1,
            contingencia_desde: Some(desde),
            ..ResumoFiscal::default()
        });
        estado.geracao_vista = fiscal_fundo::geracao();
        desenhar(m, s, &mut estado);
        assert_eq!(estado.resumo.map(|r| r.em_contingencia), Some(1));
    }

    #[test]
    fn lista_filtra_e_desenha() {
        let t = motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let mut estado = EstadoNotas::default();
        estado.carregar(m, s);
        assert!(estado.documentos.is_empty());
        assert_eq!(estado.resumo.as_ref().map(|r| r.pendentes), Some(0));
        desenhar(m, s, &mut estado);

        // Com documentos (como a fila os devolve), o filtro padrão mostra o que pede atenção.
        estado.documentos = vec![
            documento(EstadoDocumento::Autorizada, 1),
            documento(EstadoDocumento::Rejeitada, 2),
            documento(EstadoDocumento::Pendente, 3),
        ];
        let numeros = |e: &EstadoNotas| e.visiveis().iter().map(|d| d.numero).collect::<Vec<_>>();
        assert_eq!(numeros(&estado), vec![2, 3]);
        estado.filtro = Filtro::Autorizadas;
        assert_eq!(numeros(&estado), vec![1]);
        estado.filtro = Filtro::Todas;
        estado.geracao_vista = fiscal_fundo::geracao();
        let (rejeitada, documentos) = (estado.documentos[1].clone(), estado.documentos.clone());
        estado.detalhe = Some(DetalheDocumento {
            resumo: rejeitada,
            nota: None,
            xml: None,
            xml_assinado: None,
        });
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                corpo(ui, m, s, &mut estado);
            });
        });
        assert_eq!(estado.documentos, documentos);

        // Reenviar um documento que não existe dá erro, sem quebrar a tela.
        assert!(reenviar(&ctx, m, s, &mut estado, Id::novo()).is_err());
    }

    #[test]
    fn detalhe_com_cancelamento_aberto_e_pedido_invalido() {
        let t = motor_de_teste();
        let (m, s) = (&t.motor, &t.sessao);
        let mut estado = EstadoNotas::default();
        estado.carregar(m, s);
        // Autorizada há 45 minutos: passou o prazo da NFC-e — o pedido avisa, não bloqueia.
        let mut resumo = documento(EstadoDocumento::Autorizada, 9);
        resumo.autorizado_em = Some(Instante::agora().mais_segundos(-45 * 60));
        let prazo = resumo.prazo_de_cancelamento(Instante::agora());
        assert!(prazo.is_some_and(|p| !p.dentro()));
        let texto = texto_do_cancelamento(prazo);
        assert!(texto.contains("passou o prazo de 30 min"), "{texto}");
        assert!(texto.ends_with("A venda em si não é desfeita aqui."));
        assert_eq!(
            texto_do_cancelamento(None),
            "Cancelar esta NFC-e na SEFAZ. A venda em si não é desfeita aqui."
        );
        estado.detalhe = Some(DetalheDocumento {
            resumo,
            nota: None,
            xml: None,
            xml_assinado: None,
        });
        estado.pedindo = Some((
            PedidoDocumento::Cancelar,
            "Cliente desistiu da compra".into(),
        ));
        let ctx = egui::Context::default();
        cardeal_ui::tokens::instalar_fontes(&ctx);
        cardeal_ui::tokens::instalar_estilo(&ctx, cardeal_ui::tokens::Tema::Claro);
        for _ in 0..2 {
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    detalhe(ui, m, s, &mut estado);
                });
            });
        }
        // O documento não existe na base: o pedido volta com erro e o formulário fica aberto.
        let id = estado.detalhe.as_ref().unwrap().resumo.id;
        assert!(pedir(
            &ctx,
            m,
            s,
            &mut estado,
            id,
            PedidoDocumento::Cancelar,
            "Cliente desistiu da compra"
        )
        .is_err());
        assert!(estado.pedindo.is_some());
    }
}
