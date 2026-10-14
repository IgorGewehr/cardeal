//! A aba "Conciliação": importar o extrato do banco (OFX/CSV), ver o que o sistema sugere para
//! cada linha e confirmar — conciliar com uma baixa ou parcela, lançar (tarifa, rendimento) ou
//! ignorar. O sistema só sugere; quem confirma é o usuário.

use super::*;
use cardeal_ui::molecules::Abas;
use mod_financeiro::extrato::FormatoExtrato;
use mod_financeiro::sugestao::{AlvoDaConciliacao, Sugestao};
use mod_financeiro::{
    ConciliarItem, EstadoItem, ExtratoImportado, IgnorarItemDoExtrato, ImportarExtrato,
    ItemDoExtrato, ItensDoExtrato, LancarItemDoExtrato,
};

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoConciliacao {
    pub(super) contas: Vec<ItemContaDisponivel>,
    pub(super) conta: Option<Id>,
    /// 0 = pendentes, 1 = conciliadas, 2 = ignoradas, 3 = todas.
    filtro: u8,
    pub(super) itens: Vec<ItemDoExtrato>,
    /// A linha que está sendo lançada (diálogo aberto).
    lancando: Option<ItemDoExtrato>,
    categoria: Option<Id>,
    descricao: String,
    /// A conferência do saldo de cada conta bancária (`super::conferencia`).
    pub(super) conferencias: Vec<mod_financeiro::ConferenciaDaConta>,
    /// O formulário do saldo digitado aberto: (data, valor).
    pub(super) saldo_form: Option<(String, String)>,
}

/// Lê o arquivo; extratos antigos vêm em ISO-8859-1, que não é UTF-8 válido.
pub(super) fn ler_texto(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec())
        .unwrap_or_else(|_| bytes.iter().map(|b| char::from(*b)).collect())
}

impl EstadoConciliacao {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(v) = motor.consultar(
            sessao,
            "financeiro.contas_disponiveis.v1",
            &ContasDisponiveis,
        ) {
            self.contas = v.into_iter().filter(|c| !c.e_caixa).collect();
        }
        if self
            .conta
            .is_none_or(|c| !self.contas.iter().any(|x| x.conta == c))
        {
            self.conta = self.contas.first().map(|c| c.conta);
        }
        self.recarregar_itens(motor, sessao);
    }

    pub(super) fn recarregar_itens(&mut self, motor: &Motor, sessao: &Sessao) {
        self.carregar_conferencias(motor, sessao);
        let Some(conta) = self.conta else {
            self.itens.clear();
            return;
        };
        if let Ok(i) = motor.consultar(
            sessao,
            "financeiro.itens_do_extrato.v1",
            &ItensDoExtrato {
                conta,
                estado: None,
            },
        ) {
            self.itens = i;
        }
    }

    fn contar(&self, e: EstadoItem) -> usize {
        self.itens.iter().filter(|i| i.estado == e).count()
    }

    fn visiveis(&self) -> Vec<&ItemDoExtrato> {
        self.itens
            .iter()
            .filter(|i| match self.filtro {
                0 => i.estado == EstadoItem::Pendente,
                1 => i.estado == EstadoItem::Conciliado,
                2 => i.estado == EstadoItem::Ignorado,
                _ => true,
            })
            .collect()
    }
}

fn texto_sugestao(s: &Sugestao) -> String {
    match s.alvo {
        AlvoDaConciliacao::Baixa(_) => {
            format!("Baixa de {} ({}%)", s.data.formatar_curta(), s.confianca)
        }
        AlvoDaConciliacao::Parcela(_) => format!(
            "Parcela que vence {}{} ({}%)",
            s.data.formatar_curta(),
            if s.texto.is_empty() {
                String::new()
            } else {
                format!(" · {}", s.texto)
            },
            s.confianca
        ),
    }
}

pub(super) fn painel_conciliacao(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    cabecalho(ui, motor, sessao, estado);
    ui.add_space(Espaco::E12);
    if estado.conciliacao.contas.is_empty() {
        EstadoVazio::novo(
            Icone::Dinheiro,
            "Cadastre uma conta bancária (aba Contas bancárias) para importar o extrato.",
        )
        .mostrar(ui);
        return;
    }
    super::conferencia::painel(ui, motor, sessao, estado);
    ui.add_space(Espaco::E12);
    let c = &estado.conciliacao;
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Pendentes", c.contar(EstadoItem::Pendente)).tom(
            if c.contar(EstadoItem::Pendente) > 0 {
                Tom::Atencao
            } else {
                Tom::Positivo
            },
        ),
        CartaoKpi::contagem("Conciliadas", c.contar(EstadoItem::Conciliado)).tom(Tom::Positivo),
        CartaoKpi::contagem("Ignoradas", c.contar(EstadoItem::Ignorado)).tom(Tom::Neutro),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    if let Some(f) = Abas::nova(&[
        (0_u8, "Pendentes"),
        (1, "Conciliadas"),
        (2, "Ignoradas"),
        (3, "Todas"),
    ])
    .selecionada(estado.conciliacao.filtro)
    .id_salt("conc-filtro")
    .mostrar(ui)
    {
        estado.conciliacao.filtro = f;
    }
    ui.add_space(Espaco::E8);
    tabela(ui, motor, sessao, estado);
    dialogo_lancar(ui.ctx(), motor, sessao, estado);
}

fn cabecalho(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaFinanceiro) {
    ui.add(Rotulo::apoio(
        "Importe o extrato do banco (OFX ou CSV). O sistema sugere o que cada linha é — uma baixa que \
         você já deu ou uma conta em aberto de mesmo valor — e você confirma. Linhas já importadas \
         não entram duas vezes.",
    ));
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        let ops: Vec<(Id, String)> = estado
            .conciliacao
            .contas
            .iter()
            .map(|c| (c.conta, c.nome.clone()))
            .collect();
        let antes = estado.conciliacao.conta;
        SeletorOpcao::novo("Conta bancária", &mut estado.conciliacao.conta)
            .opcoes(ops)
            .mostrar(ui);
        if antes != estado.conciliacao.conta {
            estado.conciliacao.recarregar_itens(motor, sessao);
        }
        ui.add_space(Espaco::E12);
        if ui
            .add_enabled(
                estado.conciliacao.conta.is_some(),
                Botao::primario("Importar extrato…"),
            )
            .clicked()
        {
            importar(ui.ctx(), motor, sessao, estado);
        }
    });
}

fn importar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(conta) = estado.conciliacao.conta else {
        return;
    };
    let Some(caminho) = rfd::FileDialog::new()
        .add_filter("Extrato bancário", &["ofx", "csv"])
        .pick_file()
    else {
        return;
    };
    let bytes = match std::fs::read(&caminho) {
        Ok(b) => b,
        Err(e) => {
            notificar(
                ctx,
                Notificacao::erro("Não consegui ler o arquivo").detalhe(e.to_string()),
            );
            return;
        }
    };
    let arquivo = caminho.file_name().map_or_else(
        || "extrato".to_owned(),
        |n| n.to_string_lossy().into_owned(),
    );
    importar_texto(
        ctx,
        motor,
        sessao,
        estado,
        conta,
        &arquivo,
        &ler_texto(&bytes),
    );
}

pub(super) fn importar_texto(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    conta: Id,
    arquivo: &str,
    conteudo: &str,
) {
    match motor.executar(
        sessao,
        "financeiro.importar_extrato.v1",
        &ImportarExtrato {
            conta,
            formato: FormatoExtrato::do_arquivo(arquivo),
            arquivo: arquivo.to_owned(),
            conteudo: conteudo.to_owned(),
        },
    ) {
        Ok(r) => {
            let r: ExtratoImportado = r;
            estado.conciliacao.filtro = 0;
            estado.conciliacao.recarregar_itens(motor, sessao);
            notificar(
                ctx,
                Notificacao::sucesso(format!(
                    "{} linha(s) importada(s); {} já existia(m)",
                    r.importados, r.repetidos
                )),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn tabela(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaFinanceiro) {
    enum Acao {
        Conciliar(Id, AlvoDaConciliacao),
        Lancar(Id),
        Ignorar(Id, bool),
    }
    let mut acao: Option<Acao> = None;
    let linhas = estado.conciliacao.visiveis();
    Grade::nova(vec![
        ColunaGrade::nova("Data").largura(90.0),
        ColunaGrade::nova("Descrição no banco").largura(300.0),
        ColunaGrade::nova("Valor").largura(120.0).numero(),
        ColunaGrade::nova("Sugestão do sistema").largura(330.0),
        ColunaGrade::nova("").largura(260.0),
    ])
    .com_acoes()
    .id_salt("conciliacao-itens")
    .vazio("Nenhuma linha neste recorte. Importe um extrato.")
    .mostrar(ui, linhas.len(), |i, row| {
        let l = linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(l.data.formatar_curta()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.descricao.clone()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(l.valor));
        });
        row.col(|ui| match (l.estado, l.sugestoes.first()) {
            (EstadoItem::Conciliado, _) => {
                ui.add(Etiqueta::positiva("conciliada"));
            }
            (EstadoItem::Ignorado, _) => {
                ui.add(Etiqueta::neutra("ignorada"));
            }
            (EstadoItem::Pendente, Some(s)) => {
                ui.add(Rotulo::interface(texto_sugestao(s)));
            }
            (EstadoItem::Pendente, None) => {
                ui.add(Rotulo::interface("sem correspondência").cor(ui.cores().texto_fraco));
            }
        });
        row.col(|ui| match l.estado {
            EstadoItem::Pendente => {
                ui.horizontal(|ui| {
                    if let Some(s) = l.sugestoes.first() {
                        if ui.add(Botao::primario("Conciliar").pequeno()).clicked() {
                            acao = Some(Acao::Conciliar(l.id, s.alvo));
                        }
                    }
                    if ui.add(Botao::secundario("Lançar").pequeno()).clicked() {
                        acao = Some(Acao::Lancar(l.id));
                    }
                    if ui.add(Botao::fantasma("Ignorar").pequeno()).clicked() {
                        acao = Some(Acao::Ignorar(l.id, true));
                    }
                });
            }
            EstadoItem::Ignorado => {
                if ui
                    .add(Botao::fantasma("Voltar para pendente").pequeno())
                    .clicked()
                {
                    acao = Some(Acao::Ignorar(l.id, false));
                }
            }
            EstadoItem::Conciliado => {}
        });
    });
    match acao {
        Some(Acao::Conciliar(item, alvo)) => {
            let r = motor.executar(
                sessao,
                "financeiro.conciliar_item.v1",
                &ConciliarItem { item, alvo },
            );
            depois(ui.ctx(), motor, sessao, estado, r, "Linha conciliada");
        }
        Some(Acao::Ignorar(item, ignorar)) => {
            let r = motor.executar(
                sessao,
                "financeiro.ignorar_item_do_extrato.v1",
                &IgnorarItemDoExtrato { item, ignorar },
            );
            depois(
                ui.ctx(),
                motor,
                sessao,
                estado,
                r,
                if ignorar {
                    "Linha ignorada"
                } else {
                    "Linha de volta aos pendentes"
                },
            );
        }
        Some(Acao::Lancar(item)) => {
            estado.conciliacao.lancando = estado
                .conciliacao
                .itens
                .iter()
                .find(|i| i.id == item)
                .cloned();
            estado.conciliacao.categoria = None;
            estado.conciliacao.descricao.clear();
        }
        None => {}
    }
}

fn depois(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
    r: cardeal_kernel::Resultado<()>,
    sucesso: &str,
) {
    match r {
        Ok(()) => {
            estado.conciliacao.recarregar_itens(motor, sessao);
            estado.carregar_dashboard(motor, sessao);
            notificar(ctx, Notificacao::sucesso(sucesso));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_lancar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let Some(item) = estado.conciliacao.lancando.clone() else {
        return;
    };
    let entrada = item.valor.e_positivo();
    let titulo = if entrada {
        "Lançar como receita"
    } else {
        "Lançar como despesa"
    };
    let categorias: Vec<(Id, String)> = estado
        .categorias
        .iter()
        .filter(|c| match c.especie {
            Some(EspecieTitulo::Receber) => entrada,
            Some(EspecieTitulo::Pagar) => !entrada,
            None => true,
        })
        .map(|c| (c.id, c.nome.clone()))
        .collect();
    let mut confirmar = false;
    let fechar = Dialogo::nova(titulo).largura(520.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            ui.add(Rotulo::interface(format!(
                "{} · {} · {}",
                item.data.formatar_curta(),
                item.descricao,
                item.valor.formatar_com_simbolo()
            )));
            ui.add(Rotulo::apoio(
                "Cria o lançamento já quitado na conta do extrato e concilia esta linha.",
            ));
            ui.add_space(Espaco::E12);
            SeletorOpcao::novo("Categoria", &mut estado.conciliacao.categoria)
                .opcoes(categorias.clone())
                .placeholder("Sem categoria")
                .mostrar(ui);
            ui.add_space(Espaco::E8);
            ui.add(
                Campo::novo("Descrição (opcional)", &mut estado.conciliacao.descricao)
                    .marcador("usa o texto do banco"),
            );
        },
        |ui, estado| {
            if ui.add(Botao::primario("Lançar e conciliar")).clicked() {
                confirmar = true;
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.conciliacao.lancando = None;
            }
        },
    );
    if confirmar {
        let r = motor.executar(
            sessao,
            "financeiro.lancar_item_do_extrato.v1",
            &LancarItemDoExtrato {
                item: item.id,
                categoria: estado.conciliacao.categoria,
                descricao: Some(estado.conciliacao.descricao.clone()),
            },
        );
        if r.is_ok() {
            estado.conciliacao.lancando = None;
        }
        depois(ctx, motor, sessao, estado, r, "Lançado e conciliado");
    }
    if fechar {
        estado.conciliacao.lancando = None;
    }
}
