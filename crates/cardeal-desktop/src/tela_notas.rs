//! Notas fiscais — a fila e o histórico das NFC-e das vendas (`docs/modulos/fiscal.md`): o que
//! está na fila, conferindo, rejeitado (com o que corrigir) e autorizado. Daqui se reenvia a
//! nota rejeitada depois de corrigir e se manda transmitir na hora.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Fuso, Id};
use cardeal_modkit::Icone;
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom, ValorDinheiro};
use cardeal_ui::molecules::{dado, Abas, CartaoKpi, EstadoVazio};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, FaixaKpi, Grade, LayoutTela, Notificacao, Painel,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_fiscal::{
    AmbienteFiscal, DetalheDocumento, DocumentoFiscal, DocumentoResumo, DocumentosFiscais,
    EstadoDocumento, ReenviarDocumento, ResumoDaFilaFiscal, ResumoFiscal,
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
        EstadoDocumento::Rejeitada | EstadoDocumento::Denegada => Etiqueta::negativa(rotulo),
        EstadoDocumento::Cancelada => Etiqueta::neutra(rotulo),
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
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Na fila", r.pendentes).tom(tom_se(r.pendentes, Tom::Atencao)),
        CartaoKpi::contagem("Conferindo", r.incertas).tom(tom_se(r.incertas, Tom::Atencao)),
        CartaoKpi::contagem("Rejeitadas", r.rejeitadas).tom(tom_se(r.rejeitadas, Tom::Negativo)),
        CartaoKpi::contagem("Autorizadas hoje", r.autorizadas_hoje).tom(Tom::Positivo),
    ])
    .mostrar(ui);
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
                if d.ambiente == AmbienteFiscal::Homologacao {
                    ui.add(Etiqueta::neutra("teste"));
                }
            });
        });
        row.col(|ui| {
            let texto = match d.estado {
                EstadoDocumento::Autorizada => d.chave.clone().unwrap_or_default(),
                _ => d.motivo.clone().unwrap_or_default(),
            };
            ui.add(Rotulo::campo(texto).quebravel());
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

fn detalhe(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoNotas) {
    let Some(d) = &estado.detalhe else { return };
    let mut fechar = false;
    let mut imprimir = None;
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
            ui.horizontal(|ui| {
                if d.resumo.estado == EstadoDocumento::Autorizada
                    && ui.add(Botao::secundario("Imprimir cupom fiscal")).clicked()
                {
                    imprimir = Some(d.resumo.id);
                }
                fechar = ui.add(Botao::fantasma("Fechar")).clicked();
            });
        });
    if fechar {
        estado.detalhe = None;
    }
    if let Some(id) = imprimir {
        match crate::danfce::imprimir(motor, sessao, id) {
            Ok(_) => notificar(
                ui.ctx(),
                Notificacao::sucesso("Cupom fiscal aberto para imprimir"),
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
}
