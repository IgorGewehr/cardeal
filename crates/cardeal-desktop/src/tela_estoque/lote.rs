//! A aba "Alteração em lote": escolher produtos (por grupo e busca), dizer o que mudar
//! (margem, grupo, situação), **ver a prévia** do antes e depois e só então aplicar. O
//! histórico guarda cada alteração e deixa desfazer a última nas 24 horas seguintes.

use std::collections::HashSet;

use super::*;
use cardeal_kernel::Percentual;
use cardeal_ui::atoms::Caixa;
use cardeal_ui::molecules::Abas;
use mod_estoque::alteracao_em_lote::{Alteracao, Mudanca, ValoresProduto};
use mod_estoque::{
    AlteracaoAplicada, AlteracaoDesfeita, AlteracaoRegistrada, AplicarAlteracaoEmLote,
    DesfazerAlteracaoEmLote, HistoricoDeAlteracoesEmLote, LinhaDaPrevia, PreviaDaAlteracaoEmLote,
    ProdutoParaLote, ProdutosParaLote,
};

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoLote {
    pub(super) grupo: Option<Id>,
    pub(super) busca: String,
    pub(super) produtos: Vec<ProdutoParaLote>,
    pub(super) marcados: HashSet<Id>,
    /// 0 = manter, 1 = definir, 2 = voltar a herdar.
    pub(super) margem_modo: u8,
    pub(super) margem: String,
    grupo_destino: Option<Id>,
    /// 0 = manter, 1 = ativar, 2 = desativar.
    ativo_modo: u8,
    previa: Option<Vec<LinhaDaPrevia>>,
    pub(super) historico: Vec<AlteracaoRegistrada>,
}

impl EstadoLote {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(p) = motor.consultar(
            sessao,
            "estoque.produtos_para_lote.v1",
            &ProdutosParaLote {
                grupo: self.grupo,
                busca: Some(self.busca.clone()),
            },
        ) {
            self.marcados
                .retain(|id| p.iter().any(|x| x.produto == *id));
            self.produtos = p;
        }
        if let Ok(h) = motor.consultar(
            sessao,
            "estoque.historico_de_alteracoes_em_lote.v1",
            &HistoricoDeAlteracoesEmLote,
        ) {
            self.historico = h;
        }
        self.previa = None;
    }

    /// A alteração montada do formulário; erro com a mensagem para o usuário.
    pub(super) fn alteracao(&self) -> Result<Alteracao, String> {
        let margem = match self.margem_modo {
            1 => {
                let m = Percentual::de_str(self.margem.trim())
                    .map_err(|_| "Margem: digite um número, como 20 ou 12,5".to_owned())?;
                if !m.e_fracao_valida() || m == Percentual::CEM {
                    return Err("Margem: precisa estar entre 0% e 100%".to_owned());
                }
                Mudanca::Definir(Some(m))
            }
            2 => Mudanca::Definir(None),
            _ => Mudanca::Manter,
        };
        let a = Alteracao {
            margem,
            grupo: self.grupo_destino.map_or(Mudanca::Manter, Mudanca::Definir),
            ativo: match self.ativo_modo {
                1 => Mudanca::Definir(true),
                2 => Mudanca::Definir(false),
                _ => Mudanca::Manter,
            },
        };
        if a.e_vazia() {
            Err("Escolha o que mudar: margem, grupo ou situação.".to_owned())
        } else {
            Ok(a)
        }
    }

    fn ids_marcados(&self) -> Vec<Id> {
        self.produtos
            .iter()
            .filter(|p| self.marcados.contains(&p.produto))
            .map(|p| p.produto)
            .collect()
    }

    fn descricao(&self, a: &Alteracao, n: usize, nomes: &[(Id, String)]) -> String {
        let nome = |id: Id| {
            nomes
                .iter()
                .find(|(i, _)| *i == id)
                .map_or_else(|| "grupo".to_owned(), |(_, n)| n.clone())
        };
        let mut partes = Vec::new();
        match a.margem {
            Mudanca::Definir(Some(m)) => partes.push(format!("margem {}%", m.formatar(1))),
            Mudanca::Definir(None) => partes.push("margem volta a ser herdada".to_owned()),
            Mudanca::Manter => {}
        }
        if let Mudanca::Definir(g) = a.grupo {
            partes.push(format!("para o grupo {}", nome(g)));
        }
        if let Mudanca::Definir(ativo) = a.ativo {
            partes.push(if ativo { "ativar" } else { "desativar" }.to_owned());
        }
        format!("{} em {n} produto(s)", partes.join(", "))
    }
}

fn texto_margem(m: Option<Percentual>) -> String {
    m.map_or_else(|| "herda".to_owned(), |m| format!("{}%", m.formatar(1)))
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    ui.add(Rotulo::apoio(
        "Mude a margem, o grupo ou a situação de vários produtos de uma vez. Nada é gravado antes \
         de você ver a prévia e confirmar, e a última alteração pode ser desfeita por 24 horas.",
    ));
    ui.add_space(Espaco::E12);
    escolha(ui, motor, sessao, estado);
    ui.add_space(Espaco::E16);
    mudanca(ui, motor, sessao, estado);
    ui.add_space(Espaco::E16);
    historico(ui, motor, sessao, estado);
}

fn escolha(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    ui.add(Rotulo::titulo_secao("1. Escolha os produtos"));
    ui.add_space(Espaco::E8);
    let ops = estado.est.nos_para_seletor();
    let mut recarregar = false;
    let antes = estado.lote.grupo;
    let busca_antes = estado.lote.busca.clone();
    BarraFiltros::nova(&mut estado.lote.busca)
        .marcador("Buscar produto")
        .filtro(|ui| {
            SeletorOpcao::novo("Grupo", &mut estado.lote.grupo)
                .opcoes(ops.clone())
                .placeholder("Todos os grupos")
                .sem_rotulo()
                .mostrar(ui);
        })
        .mostrar(ui);
    if antes != estado.lote.grupo || busca_antes != estado.lote.busca {
        recarregar = true;
    }
    if recarregar {
        estado.lote.carregar(motor, sessao);
    }
    ui.add_space(Espaco::E8);
    ui.horizontal(|ui| {
        if ui
            .add(Botao::secundario("Marcar todos da lista").pequeno())
            .clicked()
        {
            let ids: Vec<Id> = estado.lote.produtos.iter().map(|p| p.produto).collect();
            estado.lote.marcados.extend(ids);
            estado.lote.previa = None;
        }
        if ui
            .add(Botao::fantasma("Limpar seleção").pequeno())
            .clicked()
        {
            estado.lote.marcados.clear();
            estado.lote.previa = None;
        }
        ui.add(Rotulo::apoio(format!(
            "{} marcado(s) de {}",
            estado.lote.marcados.len(),
            estado.lote.produtos.len()
        )));
    });
    ui.add_space(Espaco::E8);

    let nomes_grupo = estado.est.nos_para_seletor();
    let mut alternado: Option<usize> = None;
    Grade::nova(vec![
        ColunaGrade::nova("").largura(40.0),
        ColunaGrade::nova("Produto").largura(300.0),
        ColunaGrade::nova("Grupo").largura(220.0),
        ColunaGrade::nova("Margem").largura(110.0),
        ColunaGrade::nova("Situação").largura(110.0),
    ])
    .id_salt("lote-produtos")
    .vazio("Nenhum produto neste recorte.")
    .mostrar(ui, estado.lote.produtos.len(), |i, row| {
        let p = &estado.lote.produtos[i];
        row.col(|ui| {
            let mut marcado = estado.lote.marcados.contains(&p.produto);
            if ui.add(Caixa::nova(&mut marcado, "")).changed() {
                alternado = Some(i);
            }
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(p.nome.clone()));
        });
        row.col(|ui| {
            let g = nomes_grupo
                .iter()
                .find(|(id, _)| *id == p.grupo)
                .map_or_else(String::new, |(_, n)| n.clone());
            ui.add(Rotulo::interface(g));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(texto_margem(p.margem)));
        });
        row.col(|ui| {
            if p.ativo {
                ui.add(Etiqueta::positiva("ativo"));
            } else {
                ui.add(Etiqueta::neutra("inativo"));
            }
        });
    });
    if let Some(i) = alternado {
        let id = estado.lote.produtos[i].produto;
        if !estado.lote.marcados.remove(&id) {
            estado.lote.marcados.insert(id);
        }
        estado.lote.previa = None;
    }
}

fn mudanca(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    ui.add(Rotulo::titulo_secao("2. O que mudar"));
    ui.add_space(Espaco::E8);
    let l = &mut estado.lote;
    ui.add(Rotulo::campo("Margem líquida desejada"));
    if let Some(m) = Abas::nova(&[
        (0_u8, "Manter"),
        (1, "Definir"),
        (2, "Voltar a herdar do grupo"),
    ])
    .selecionada(l.margem_modo)
    .id_salt("lote-margem")
    .mostrar(ui)
    {
        l.margem_modo = m;
        l.previa = None;
    }
    if l.margem_modo == 1 {
        ui.add_space(Espaco::E4);
        ui.add(Campo::novo("Nova margem (%)", &mut l.margem).marcador("ex.: 25"));
    }
    ui.add_space(Espaco::E8);
    ui.columns(2, |c| {
        let ops = estado.est.nos_para_seletor();
        SeletorOpcao::novo("Mover para o grupo", &mut estado.lote.grupo_destino)
            .opcoes(ops)
            .placeholder("Manter o grupo")
            .mostrar(&mut c[0]);
        c[1].add(Rotulo::campo("Situação"));
        if let Some(a) = Abas::nova(&[(0_u8, "Manter"), (1, "Ativar"), (2, "Desativar")])
            .selecionada(estado.lote.ativo_modo)
            .id_salt("lote-ativo")
            .mostrar(&mut c[1])
        {
            estado.lote.ativo_modo = a;
            estado.lote.previa = None;
        }
    });
    ui.add_space(Espaco::E12);
    if ui
        .add_enabled(
            !estado.lote.marcados.is_empty(),
            Botao::secundario("Ver a prévia"),
        )
        .clicked()
    {
        ver_previa(ui.ctx(), motor, sessao, estado);
    }
    if estado.lote.previa.is_some() {
        previa(ui, motor, sessao, estado);
    }
}

fn ver_previa(ctx: &egui::Context, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    let alteracao = match estado.lote.alteracao() {
        Ok(a) => a,
        Err(m) => {
            notificar(ctx, Notificacao::aviso(m));
            return;
        }
    };
    match motor.consultar(
        sessao,
        "estoque.previa_da_alteracao_em_lote.v1",
        &PreviaDaAlteracaoEmLote {
            produtos: estado.lote.ids_marcados(),
            alteracao,
        },
    ) {
        Ok(p) => estado.lote.previa = Some(p),
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn celula_valores(
    ui: &mut egui::Ui,
    antes: &ValoresProduto,
    depois: &ValoresProduto,
    grupos: &[(Id, String)],
) {
    let nome = |id: Id| {
        grupos
            .iter()
            .find(|(i, _)| *i == id)
            .map_or_else(|| "—".to_owned(), |(_, n)| n.clone())
    };
    let situacao = |a: bool| if a { "ativo" } else { "inativo" };
    let mut partes: Vec<String> = Vec::new();
    if antes.margem != depois.margem {
        partes.push(format!(
            "margem {} → {}",
            texto_margem(antes.margem),
            texto_margem(depois.margem)
        ));
    }
    if antes.grupo != depois.grupo {
        partes.push(format!(
            "grupo {} → {}",
            nome(antes.grupo),
            nome(depois.grupo)
        ));
    }
    if antes.ativo != depois.ativo {
        partes.push(format!(
            "{} → {}",
            situacao(antes.ativo),
            situacao(depois.ativo)
        ));
    }
    if partes.is_empty() {
        ui.add(Rotulo::interface("já está assim").cor(ui.cores().texto_fraco));
    } else {
        ui.add(Rotulo::interface(partes.join(" · ")));
    }
}

fn previa(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    let Some(linhas) = estado.lote.previa.clone() else {
        return;
    };
    let muda = linhas.iter().filter(|l| l.muda()).count();
    ui.add_space(Espaco::E12);
    ui.add(Rotulo::titulo_secao(format!(
        "Prévia: {muda} de {} produto(s) mudam",
        linhas.len()
    )));
    ui.add_space(Espaco::E8);
    let grupos = estado.est.nos_para_seletor();
    Grade::nova(vec![
        ColunaGrade::nova("Produto").largura(300.0),
        ColunaGrade::nova("Antes → depois").largura(520.0),
    ])
    .id_salt("lote-previa")
    .mostrar(ui, linhas.len(), |i, row| {
        let l = &linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(l.nome.clone()));
        });
        row.col(|ui| celula_valores(ui, &l.antes, &l.depois, &grupos));
    });
    ui.add_space(Espaco::E12);
    if ui
        .add_enabled(
            muda > 0,
            Botao::primario(format!("Aplicar a {muda} produto(s)")),
        )
        .clicked()
    {
        aplicar(ui.ctx(), motor, sessao, estado);
    }
}

pub(super) fn aplicar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    let alteracao = match estado.lote.alteracao() {
        Ok(a) => a,
        Err(m) => {
            notificar(ctx, Notificacao::aviso(m));
            return;
        }
    };
    let produtos = estado.lote.ids_marcados();
    let descricao =
        estado
            .lote
            .descricao(&alteracao, produtos.len(), &estado.est.nos_para_seletor());
    match motor.executar(
        sessao,
        "estoque.aplicar_alteracao_em_lote.v1",
        &AplicarAlteracaoEmLote {
            descricao,
            produtos,
            alteracao,
        },
    ) {
        Ok(r) => {
            let r: AlteracaoAplicada = r;
            estado.carregar(motor, sessao);
            estado.lote.carregar(motor, sessao);
            estado.lote.marcados.clear();
            notificar(
                ctx,
                Notificacao::sucesso(format!("{} produto(s) alterado(s)", r.alterados)),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn historico(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    ui.add(Rotulo::titulo_secao("Alterações anteriores"));
    ui.add_space(Espaco::E8);
    if estado.lote.historico.is_empty() {
        ui.add(Rotulo::apoio("Nenhuma alteração em lote ainda."));
        return;
    }
    let mut desfazer: Option<Id> = None;
    let fuso = cardeal_kernel::Fuso::BRASILIA;
    Grade::nova(vec![
        ColunaGrade::nova("Quando").largura(160.0),
        ColunaGrade::nova("O que mudou").largura(420.0),
        ColunaGrade::nova("Campos").largura(160.0),
        ColunaGrade::nova("Situação").largura(130.0),
        ColunaGrade::nova("").largura(130.0),
    ])
    .com_acoes()
    .id_salt("lote-historico")
    .mostrar(ui, estado.lote.historico.len(), |i, row| {
        let h = &estado.lote.historico[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(h.criado_em.formatar(fuso)));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(h.descricao.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(h.campos.clone()));
        });
        row.col(|ui| {
            if h.desfeita_em.is_some() {
                ui.add(Etiqueta::neutra("desfeita"));
            } else {
                ui.add(Etiqueta::positiva("aplicada"));
            }
        });
        row.col(|ui| {
            if h.pode_desfazer && ui.add(Botao::secundario("Desfazer").pequeno()).clicked() {
                desfazer = Some(h.lote);
            }
        });
    });
    if let Some(lote) = desfazer {
        match motor.executar(
            sessao,
            "estoque.desfazer_alteracao_em_lote.v1",
            &DesfazerAlteracaoEmLote { lote },
        ) {
            Ok(r) => {
                let r: AlteracaoDesfeita = r;
                estado.carregar(motor, sessao);
                estado.lote.carregar(motor, sessao);
                let ignorados = if r.ignorados > 0 {
                    format!(" ({} editado(s) depois ficaram como estão)", r.ignorados)
                } else {
                    String::new()
                };
                notificar(
                    ui.ctx(),
                    Notificacao::sucesso(format!(
                        "{} produto(s) restaurado(s){ignorados}",
                        r.restaurados
                    )),
                );
            }
            Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
        }
    }
}
