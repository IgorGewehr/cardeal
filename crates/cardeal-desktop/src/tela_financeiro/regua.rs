//! A régua de cobrança na aba Cobrança: a fila de hoje (quem lembrar ou cobrar, com a mensagem
//! já escrita e o WhatsApp a um clique — o envio fica registrado e o cliente sai da fila até a
//! próxima etapa) e o editor da régua da empresa (`mod_financeiro::regua`).

use super::*;
use mod_financeiro::regua::{redigir, regua_padrao};
use mod_financeiro::{
    DefinirReguaDeCobranca, EtapaRegua, FilaDeCobranca, ItemDaFila, RegistrarCobranca,
    ReguaDeCobranca,
};

/// Uma etapa em edição (os dias como texto, para aceitar "-3").
#[derive(Debug, Clone, Default)]
pub(super) struct EtapaEmEdicao {
    pub(super) dias: String,
    pub(super) nome: String,
    pub(super) mensagem: String,
}

impl From<&EtapaRegua> for EtapaEmEdicao {
    fn from(e: &EtapaRegua) -> Self {
        Self {
            dias: e.dias.to_string(),
            nome: e.nome.clone(),
            mensagem: e.mensagem.clone(),
        }
    }
}

impl EstadoTelaFinanceiro {
    pub(super) fn carregar_fila_cobranca(&mut self, motor: &Motor, sessao: &Sessao) {
        self.fila_cobranca = motor
            .consultar(sessao, "financeiro.fila_de_cobranca.v1", &FilaDeCobranca)
            .ok();
    }

    fn telefone_do_cliente(&self, cliente: Id) -> Option<String> {
        self.clientes
            .iter()
            .find(|p| p.pessoa == cliente)
            .and_then(|p| p.telefone.clone())
    }

    fn nome_do_cliente(&self, cliente: Id) -> String {
        self.nomes
            .get(&cliente)
            .cloned()
            .unwrap_or_else(|| "Cliente".to_owned())
    }

    /// A mensagem pronta de um item da fila.
    pub(super) fn mensagem_da_fila(&self, item: &ItemDaFila) -> String {
        let empresa = self
            .fila_cobranca
            .as_ref()
            .map_or("", |f| f.empresa.as_str());
        redigir(
            &item.etapa.mensagem,
            &self.nome_do_cliente(item.cliente),
            item,
            empresa,
        )
    }
}

/// Registra o envio (ou o pulo) de um item e recarrega a fila.
pub(super) fn registrar(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    item: &ItemDaFila,
    pulada: bool,
) -> Result<(), String> {
    let (): () = motor
        .executar(
            sessao,
            "financeiro.registrar_cobranca.v1",
            &RegistrarCobranca {
                parcelas: item.parcelas.clone(),
                pulada,
            },
        )
        .map_err(|e| e.mensagem)?;
    estado.carregar_fila_cobranca(motor, sessao);
    Ok(())
}

/// Grava a régua do editor.
pub(super) fn salvar_regua(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) -> Result<(), String> {
    let edicao = estado
        .regua_edicao
        .as_ref()
        .ok_or_else(|| "Abra o editor da régua".to_owned())?;
    let mut etapas = Vec::new();
    for e in edicao {
        let dias: i32 = e.dias.trim().parse().map_err(|_| {
            format!(
                "\"{}\": os dias são um número (negativo = antes do vencimento)",
                e.dias
            )
        })?;
        etapas.push(EtapaRegua {
            dias,
            nome: e.nome.clone(),
            mensagem: e.mensagem.clone(),
        });
    }
    let (): () = motor
        .executar(
            sessao,
            "financeiro.definir_regua_de_cobranca.v1",
            &DefinirReguaDeCobranca { etapas },
        )
        .map_err(|e| e.mensagem)?;
    estado.regua_edicao = None;
    estado.carregar_fila_cobranca(motor, sessao);
    Ok(())
}

fn descricao_do_prazo(dias: i32) -> String {
    match dias {
        d if d < 0 => format!("vence em {} dia(s)", -d),
        0 => "vence hoje".to_owned(),
        d => format!("{d} dia(s) de atraso"),
    }
}

/// A seção "Cobranças de hoje", no topo de "Quem me deve".
pub(super) fn secao_fila(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let itens: Vec<ItemDaFila> = estado
        .fila_cobranca
        .as_ref()
        .map(|f| f.itens.clone())
        .unwrap_or_default();
    let mut abrir_editor = false;
    ui.horizontal(|ui| {
        ui.add(Rotulo::titulo_secao(if itens.is_empty() {
            "Cobranças de hoje".to_owned()
        } else {
            format!("Cobranças de hoje · {} cliente(s)", itens.len())
        }));
        ui.add_space(Espaco::E8);
        abrir_editor = ui
            .add(Botao::fantasma("Configurar régua…").pequeno())
            .clicked();
    });
    if abrir_editor {
        let etapas: Vec<EtapaRegua> = motor
            .consultar(sessao, "financeiro.regua_de_cobranca.v1", &ReguaDeCobranca)
            .unwrap_or_else(|_| regua_padrao());
        estado.regua_edicao = Some(etapas.iter().map(EtapaEmEdicao::from).collect());
    }
    ui.add(
        Rotulo::apoio(
            "A régua escolhe quem lembrar ou cobrar hoje e já escreve a mensagem. Cada etapa \
             sai uma vez por conta; depois de enviar (ou pular), o cliente só volta na próxima \
             etapa.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    editor(ui, motor, sessao, estado);
    if itens.is_empty() {
        ui.add(Etiqueta::positiva("Ninguém para cobrar hoje pela régua"));
        return;
    }
    enum Acao {
        Enviar(usize, String),
        Pular(usize),
    }
    let mut acao: Option<Acao> = None;
    Grade::nova(vec![
        ColunaGrade::nova("Cliente").largura(220.0),
        ColunaGrade::nova("Etapa").largura(170.0),
        ColunaGrade::nova("Valor").largura(120.0).numero(),
        ColunaGrade::nova("Prazo").largura(150.0),
        ColunaGrade::nova("").largura(200.0),
    ])
    .com_acoes()
    .id_salt("regua-fila")
    .mostrar(ui, itens.len(), |i, row| {
        let item = &itens[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(estado.nome_do_cliente(item.cliente)));
        });
        row.col(|ui| {
            ui.add(if item.dias > 0 {
                Etiqueta::atencao(item.etapa.nome.clone())
            } else {
                Etiqueta::info(item.etapa.nome.clone())
            });
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(item.valor).neutro());
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(descricao_do_prazo(item.dias)));
        });
        row.col(|ui| {
            ui.horizontal(|ui| {
                let link = estado.telefone_do_cliente(item.cliente).and_then(|t| {
                    crate::telefone::link_whatsapp(&t, &estado.mensagem_da_fila(item))
                });
                match link {
                    Some(l) => {
                        if ui.add(Botao::primario("WhatsApp").pequeno()).clicked() {
                            acao = Some(Acao::Enviar(i, l));
                        }
                    }
                    None => {
                        ui.add(Rotulo::apoio("sem telefone"));
                    }
                }
                if ui.add(Botao::fantasma("Pular").pequeno()).clicked() {
                    acao = Some(Acao::Pular(i));
                }
            });
        });
    });
    match acao {
        Some(Acao::Enviar(i, link)) => {
            if let Err(e) = open::that_detached(&link) {
                notificar(
                    ui.ctx(),
                    Notificacao::erro("Não foi possível abrir o WhatsApp").detalhe(e.to_string()),
                );
            } else if let Err(e) = registrar(motor, sessao, estado, &itens[i], false) {
                notificar(ui.ctx(), Notificacao::erro(e));
            }
        }
        Some(Acao::Pular(i)) => {
            if let Err(e) = registrar(motor, sessao, estado, &itens[i], true) {
                notificar(ui.ctx(), Notificacao::erro(e));
            }
        }
        None => {}
    }
}

fn editor(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaFinanceiro) {
    let Some(edicao) = estado.regua_edicao.as_mut() else {
        return;
    };
    let (mut salvar, mut cancelar, mut padrao, mut remover) = (false, false, false, None);
    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Régua de cobrança"));
        ui.add(
            Rotulo::apoio(
                "Dias contados do vencimento: negativo é antes (lembrete), 0 é no dia, positivo é \
                 atraso. Na mensagem, use {nome}, {valor}, {vencimento}, {dias} e {empresa}.",
            )
            .quebravel(),
        );
        ui.add_space(Espaco::E8);
        for (i, e) in edicao.iter_mut().enumerate() {
            ui.columns(3, |c| {
                c[0].add(Campo::novo("Dias", &mut e.dias).marcador("-3, 0, 7…"));
                c[1].add(Campo::novo("Nome da etapa", &mut e.nome));
                if c[2]
                    .add(Botao::fantasma("Remover etapa").pequeno())
                    .clicked()
                {
                    remover = Some(i);
                }
            });
            ui.add(Campo::novo("Mensagem", &mut e.mensagem).multilinha(2));
            ui.add_space(Espaco::E8);
        }
        ui.horizontal(|ui| {
            if ui.add(Botao::secundario("+ Etapa").pequeno()).clicked() {
                edicao.push(EtapaEmEdicao::default());
            }
            padrao = ui
                .add(Botao::fantasma("Restaurar a régua padrão").pequeno())
                .clicked();
        });
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            salvar = ui.add(Botao::primario("Salvar régua")).clicked();
            cancelar = ui.add(Botao::fantasma("Cancelar")).clicked();
        });
    });
    ui.add_space(Espaco::E12);
    if let Some(i) = remover {
        edicao.remove(i);
    }
    if padrao {
        *edicao = regua_padrao().iter().map(EtapaEmEdicao::from).collect();
    }
    if cancelar {
        estado.regua_edicao = None;
    } else if salvar {
        match salvar_regua(motor, sessao, estado) {
            Ok(()) => notificar(ui.ctx(), Notificacao::sucesso("Régua de cobrança salva")),
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
        }
    }
}

/// O aviso da Visão geral: quantos clientes a régua manda cobrar hoje, com o atalho para a aba.
pub(super) fn aviso_na_visao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(f) = estado
        .fila_cobranca
        .as_ref()
        .filter(|f| !f.itens.is_empty())
    else {
        return;
    };
    let total = f.itens.iter().fold(Dinheiro::ZERO, |t, i| t + i.valor);
    let n = f.itens.len();
    let mut abrir = false;
    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(format!(
                "Cobranças de hoje pela régua: {n} cliente(s), {} no total",
                total.formatar_com_simbolo()
            )));
            ui.add_space(Espaco::E12);
            abrir = ui
                .add(Botao::secundario("Abrir a cobrança").pequeno())
                .clicked();
        });
    });
    ui.add_space(Espaco::E12);
    if abrir {
        estado.aba = Aba::Cobranca;
        estado.cobranca_pagar = false;
        estado.carregar(motor, sessao);
    }
}

/// Para os testes: a fila carregada.
#[cfg(test)]
pub(super) fn fila(estado: &EstadoTelaFinanceiro) -> Option<&mod_financeiro::FilaDoDia> {
    estado.fila_cobranca.as_ref()
}
