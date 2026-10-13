//! A aba "Transferências": mover produtos entre locais do mesmo CNPJ (loja ↔ depósito) e o
//! histórico (`docs/modulos/estoque.md` §10 "Transferência entre locais", §11.6).
//!
//! À esquerda, o documento: de onde, para onde, os produtos (com o disponível na origem e no
//! destino de cada um à vista) e uma observação; "Transferir" grava tudo de uma vez. À direita,
//! as transferências feitas, com os itens da selecionada.

use super::*;
use cardeal_kernel::{Fuso, Quantidade};
use cardeal_ui::molecules::{OpcaoBusca, SeletorBusca};
use cardeal_ui::organisms::Painel;
use mod_estoque::transferencia::ItemTransferencia;
use mod_estoque::{
    ResumoTransferencia, SaldoNoLocal, SaldosDoProdutoPorLocal, TransferenciaFeita, Transferencias,
    TransferirEntreLocais,
};

/// Um produto já na lista da transferência.
#[derive(Debug, Clone)]
pub(super) struct ItemNaLista {
    pub(super) produto: Id,
    pub(super) nome: String,
    pub(super) quantidade: Quantidade,
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoTransferenciaUi {
    pub(super) origem: Option<Id>,
    pub(super) destino: Option<Id>,
    pub(super) busca: String,
    pub(super) produto: Option<Id>,
    pub(super) quantidade: String,
    pub(super) itens: Vec<ItemNaLista>,
    pub(super) observacao: String,
    pub(super) historico: Vec<ResumoTransferencia>,
    selecionada: Option<usize>,
    /// O disponível do produto escolhido em cada local (e de qual produto é).
    saldos: Option<(Id, Vec<SaldoNoLocal>)>,
}

impl EstadoTransferenciaUi {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(h) = motor.consultar(sessao, "estoque.transferencias.v1", &Transferencias) {
            self.historico = h;
        }
        self.selecionada = None;
        self.saldos = None;
    }

    fn disponivel(&self, local: Option<Id>) -> Option<Quantidade> {
        let (_, saldos) = self.saldos.as_ref()?;
        let local = local?;
        saldos
            .iter()
            .find(|s| s.local == local)
            .map(|s| s.disponivel)
    }

    /// Põe o produto escolhido na lista (somando se já estiver).
    pub(super) fn adicionar(&mut self, nome: &str) -> Result<(), String> {
        let Some(produto) = self.produto else {
            return Err("Escolha o produto".to_owned());
        };
        let quantidade = Quantidade::de_str(self.quantidade.trim())
            .ok()
            .filter(|q| q.e_positiva())
            .ok_or_else(|| "Digite a quantidade, maior que zero".to_owned())?;
        match self.itens.iter_mut().find(|i| i.produto == produto) {
            Some(i) => i.quantidade += quantidade,
            None => self.itens.push(ItemNaLista {
                produto,
                nome: nome.to_owned(),
                quantidade,
            }),
        }
        self.produto = None;
        self.busca.clear();
        self.quantidade.clear();
        self.saldos = None;
        Ok(())
    }
}

/// Grava a transferência montada; devolve o que foi feito.
pub(super) fn transferir(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) -> Result<TransferenciaFeita, String> {
    let t = &estado.transf;
    let (Some(origem), Some(destino)) = (t.origem, t.destino) else {
        return Err("Escolha de onde e para onde".to_owned());
    };
    let comando = TransferirEntreLocais {
        origem,
        destino,
        itens: t
            .itens
            .iter()
            .map(|i| ItemTransferencia {
                produto: i.produto,
                quantidade: i.quantidade,
            })
            .collect(),
        observacao: t.observacao.clone(),
    };
    let r: TransferenciaFeita = motor
        .executar(sessao, "estoque.transferir_entre_locais.v1", &comando)
        .map_err(|e| e.mensagem)?;
    estado.transf.itens.clear();
    estado.transf.observacao.clear();
    estado.carregar(motor, sessao);
    estado.transf.carregar(motor, sessao);
    Ok(r)
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    if estado.locais.len() < 2 {
        EstadoVazio::novo(
            Icone::Estoque,
            "Para transferir é preciso ter ao menos dois locais (por exemplo, Loja e Depósito). \
             Cadastre o segundo em \"+ Novo produto\".",
        )
        .mostrar(ui);
        return;
    }
    ui.add(Rotulo::apoio(
        "A saída na origem e a entrada no destino acontecem juntas, ao custo médio da origem. \
         Sem saldo suficiente na origem, nada é transferido.",
    ));
    ui.add_space(Espaco::E12);
    ui.columns(2, |c| {
        formulario(&mut c[0], motor, sessao, estado);
        historico(&mut c[1], estado);
    });
}

fn formulario(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    Painel::novo().plano().mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Nova transferência"));
        ui.add_space(Espaco::E8);
        let locais: Vec<(Id, String)> = estado
            .locais
            .iter()
            .map(|l| (l.id, l.nome.clone()))
            .collect();
        ui.columns(2, |c| {
            SeletorOpcao::novo("De", &mut estado.transf.origem)
                .opcoes(locais.clone())
                .placeholder("Origem")
                .mostrar(&mut c[0]);
            SeletorOpcao::novo("Para", &mut estado.transf.destino)
                .opcoes(locais)
                .placeholder("Destino")
                .mostrar(&mut c[1]);
        });
        ui.add_space(Espaco::E8);

        let opcoes: Vec<OpcaoBusca<Id>> = estado
            .produtos
            .iter()
            .map(|p| {
                let o = OpcaoBusca::nova(p.produto, p.nome.clone());
                match &p.codigo_barras {
                    Some(c) => o.subtitulo(c.clone()),
                    None => o,
                }
            })
            .collect();
        SeletorBusca::novo(
            "Produto",
            &mut estado.transf.busca,
            &mut estado.transf.produto,
        )
        .opcoes(opcoes)
        .marcador("Código de barras ou nome")
        .mostrar(ui);

        // O disponível do produto escolhido, na origem e no destino.
        if let Some(produto) = estado.transf.produto {
            if estado
                .transf
                .saldos
                .as_ref()
                .is_none_or(|(p, _)| *p != produto)
            {
                let saldos = motor
                    .consultar(
                        sessao,
                        "estoque.saldos_do_produto_por_local.v1",
                        &SaldosDoProdutoPorLocal { produto },
                    )
                    .unwrap_or_default();
                estado.transf.saldos = Some((produto, saldos));
            }
            let texto = |q: Option<Quantidade>| q.map_or_else(|| "—".to_owned(), |q| q.to_string());
            ui.add(Rotulo::apoio(format!(
                "Disponível na origem: {} · no destino: {}",
                texto(estado.transf.disponivel(estado.transf.origem)),
                texto(estado.transf.disponivel(estado.transf.destino)),
            )));
        }
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            ui.add(Campo::novo("Quantidade", &mut estado.transf.quantidade).marcador("0"));
        });
        ui.add_space(Espaco::E4);
        if ui.add(Botao::secundario("Adicionar à lista")).clicked() {
            let nome = estado
                .transf
                .produto
                .and_then(|p| estado.produtos.iter().find(|x| x.produto == p))
                .map(|p| p.nome.clone())
                .unwrap_or_default();
            if let Err(e) = estado.transf.adicionar(&nome) {
                notificar(ui.ctx(), Notificacao::erro(e));
            }
        }
        ui.add_space(Espaco::E12);

        let mut remover = None;
        Grade::nova(vec![
            ColunaGrade::nova("Produto"),
            ColunaGrade::nova("Quantidade").largura(110.0).numero(),
            ColunaGrade::nova("").largura(90.0),
        ])
        .id_salt("transferencia-itens")
        .vazio("Nenhum produto na lista ainda.")
        .mostrar(ui, estado.transf.itens.len(), |i, row| {
            let it = &estado.transf.itens[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(it.nome.clone()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(it.quantidade.to_string()));
            });
            row.col(|ui| {
                if ui.add(Botao::fantasma("Remover").pequeno()).clicked() {
                    remover = Some(i);
                }
            });
        });
        if let Some(i) = remover {
            estado.transf.itens.remove(i);
        }
        ui.add_space(Espaco::E8);
        ui.add(
            Campo::novo("Observação", &mut estado.transf.observacao)
                .marcador("ex.: reposição da loja"),
        );
        ui.add_space(Espaco::E8);
        let t = &estado.transf;
        let pode = t.origem.is_some() && t.destino.is_some() && !t.itens.is_empty();
        if ui
            .add_enabled(pode, Botao::primario("Transferir"))
            .clicked()
        {
            match transferir(motor, sessao, estado) {
                Ok(r) => notificar(
                    ui.ctx(),
                    Notificacao::sucesso(format!(
                        "Transferência feita: {} produto(s), {} ao custo médio",
                        r.itens, r.valor
                    )),
                ),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
            }
        }
    });
}

fn historico(ui: &mut egui::Ui, estado: &mut EstadoTelaEstoque) {
    ui.add(Rotulo::titulo_secao("Transferências feitas"));
    ui.add_space(Espaco::E8);
    let fuso = Fuso::BRASILIA;
    let t = &mut estado.transf;
    let resposta = Grade::nova(vec![
        ColunaGrade::nova("Quando").largura(140.0),
        ColunaGrade::nova("De → Para"),
        ColunaGrade::nova("Itens").largura(60.0).numero(),
        ColunaGrade::nova("Valor").largura(110.0).numero(),
    ])
    .id_salt("transferencias")
    .selecionavel(t.selecionada)
    .vazio("Nenhuma transferência ainda.")
    .mostrar(ui, t.historico.len(), |i, row| {
        let h = &t.historico[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(h.criado_em.formatar(fuso)));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(format!("{} → {}", h.origem, h.destino)));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(h.itens.len().to_string()));
        });
        row.col(|ui| {
            ui.add(cardeal_ui::atoms::ValorDinheiro::novo(h.valor()).neutro());
        });
    });
    if let Some(i) = resposta.linha_clicada {
        t.selecionada = Some(i);
    }
    let Some(h) = t.selecionada.and_then(|i| t.historico.get(i)) else {
        return;
    };
    ui.add_space(Espaco::E12);
    if let Some(obs) = &h.observacao {
        ui.add(Rotulo::apoio(obs.clone()));
    }
    Grade::nova(vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("Quantidade").largura(110.0).numero(),
        ColunaGrade::nova("Valor").largura(110.0).numero(),
    ])
    .id_salt("transferencia-detalhe")
    .mostrar(ui, h.itens.len(), |i, row| {
        let it = &h.itens[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(it.nome.clone()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(format!(
                "{} {}",
                it.quantidade, it.unidade
            )));
        });
        row.col(|ui| {
            ui.add(cardeal_ui::atoms::ValorDinheiro::novo(it.valor).neutro());
        });
    });
}
