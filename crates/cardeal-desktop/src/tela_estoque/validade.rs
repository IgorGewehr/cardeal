//! A aba "Validade": os lotes vencidos e a vencer, com o dinheiro em risco, a baixa de perda e
//! a entrada de mercadoria com validade (`docs/modulos/estoque.md` §4 lote `Ativo → Vencido →
//! Esgotado`, §6 `LotesProximosDoVencimento`, `RegistrarPerda`).
//!
//! À esquerda, a lista (o mais urgente primeiro) com o filtro de prazo e "Dar baixa" na linha;
//! a baixa abre logo abaixo, com quantidade, motivo e observação. À direita, "Entrada com
//! validade": produto, local, quantidade, custo, validade e o código do lote (em branco, o
//! sistema gera).

use super::*;
use cardeal_kernel::{Data, Dinheiro, Quantidade};
use cardeal_ui::molecules::{Mascara, OpcaoBusca, SeletorBusca};
use cardeal_ui::organisms::Painel;
use mod_estoque::{
    EntradaComLoteRegistrada, LoteComValidade, LotesComValidade, MotivoPerda, OrigemLote,
    PerdaRegistrada, RegistrarEntradaComLote, RegistrarPerda,
};

/// O prazo do filtro.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Prazo {
    #[default]
    Trinta,
    Sessenta,
    Todos,
}

impl Prazo {
    const fn dias(self) -> Option<i32> {
        match self {
            Self::Trinta => Some(30),
            Self::Sessenta => Some(60),
            Self::Todos => None,
        }
    }
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoValidade {
    pub(super) prazo: Prazo,
    pub(super) lotes: Vec<LoteComValidade>,
    /// A baixa aberta: o lote (índice em `lotes`) e o formulário.
    pub(super) baixa: Option<usize>,
    pub(super) baixa_qtd: String,
    pub(super) baixa_motivo: Option<MotivoPerda>,
    pub(super) baixa_obs: String,
    // Entrada com validade.
    pub(super) ent_busca: String,
    pub(super) ent_produto: Option<Id>,
    pub(super) ent_local: Option<Id>,
    pub(super) ent_qtd: String,
    pub(super) ent_custo: String,
    pub(super) ent_validade: String,
    pub(super) ent_codigo: String,
}

impl EstadoValidade {
    pub(super) fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        if let Ok(l) = motor.consultar(
            sessao,
            "estoque.lotes_com_validade.v1",
            &LotesComValidade {
                dias: self.prazo.dias(),
            },
        ) {
            self.lotes = l;
        }
        self.baixa = None;
    }

    /// Abre a baixa de um lote, sugerindo o saldo inteiro e o motivo "vencimento" se venceu.
    pub(super) fn abrir_baixa(&mut self, i: usize) {
        let Some(l) = self.lotes.get(i) else {
            return;
        };
        self.baixa_qtd = l.saldo.to_string();
        self.baixa_motivo = Some(if l.vencido() {
            MotivoPerda::Vencimento
        } else {
            MotivoPerda::Quebra
        });
        self.baixa_obs.clear();
        self.baixa = Some(i);
    }
}

/// Dá baixa do lote aberto.
pub(super) fn dar_baixa(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) -> Result<PerdaRegistrada, String> {
    let v = &estado.val;
    let Some(l) = v.baixa.and_then(|i| v.lotes.get(i)) else {
        return Err("Escolha o lote".to_owned());
    };
    let quantidade = Quantidade::de_str(v.baixa_qtd.trim())
        .ok()
        .filter(|q| q.e_positiva())
        .ok_or_else(|| "Digite a quantidade, maior que zero".to_owned())?;
    let comando = RegistrarPerda {
        produto: l.produto,
        local: l.local,
        lote: Some(l.lote),
        quantidade,
        motivo: v.baixa_motivo.unwrap_or(MotivoPerda::Outro),
        observacao: v.baixa_obs.clone(),
    };
    let r: PerdaRegistrada = motor
        .executar(sessao, "estoque.registrar_perda.v1", &comando)
        .map_err(|e| e.mensagem)?;
    estado.carregar(motor, sessao);
    estado.val.carregar(motor, sessao);
    Ok(r)
}

/// Registra a entrada com validade do formulário.
pub(super) fn registrar_entrada(
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) -> Result<EntradaComLoteRegistrada, String> {
    let v = &estado.val;
    let produto = v
        .ent_produto
        .ok_or_else(|| "Escolha o produto".to_owned())?;
    let local = v.ent_local.ok_or_else(|| "Escolha o local".to_owned())?;
    let quantidade = Quantidade::de_str(v.ent_qtd.trim())
        .ok()
        .filter(|q| q.e_positiva())
        .ok_or_else(|| "Digite a quantidade, maior que zero".to_owned())?;
    let custo = cardeal_kernel::Preco::de_str(v.ent_custo.trim())
        .ok()
        .filter(|c| c.unidades_internas() > 0)
        .ok_or_else(|| "Digite o custo unitário, maior que zero".to_owned())?;
    let validade = Data::de_str(v.ent_validade.trim())
        .map_err(|_| "Digite a validade como dd/mm/aaaa".to_owned())?;
    let comando = RegistrarEntradaComLote {
        produto,
        local,
        quantidade,
        custo_unitario: custo,
        codigo_lote: v.ent_codigo.trim().to_owned(),
        origem: OrigemLote::Compra,
        fornecedor: None,
        aparelho_origem: None,
        fabricacao: None,
        validade: Some(validade),
    };
    let r: EntradaComLoteRegistrada = motor
        .executar(sessao, "estoque.registrar_entrada_com_lote.v1", &comando)
        .map_err(|e| e.mensagem)?;
    let v = &mut estado.val;
    v.ent_busca.clear();
    v.ent_produto = None;
    v.ent_qtd.clear();
    v.ent_custo.clear();
    v.ent_validade.clear();
    v.ent_codigo.clear();
    estado.carregar(motor, sessao);
    estado.val.carregar(motor, sessao);
    Ok(r)
}

fn prazo_etiqueta(l: &LoteComValidade) -> Etiqueta {
    match l.dias {
        d if d < 0 => Etiqueta::negativa(format!("venceu há {} dia(s)", -d)),
        0 => Etiqueta::negativa("vence hoje"),
        d if d <= 7 => Etiqueta::atencao(format!("vence em {d} dia(s)")),
        d if d <= 30 => Etiqueta::info(format!("vence em {d} dias")),
        d => Etiqueta::neutra(format!("vence em {d} dias")),
    }
}

pub(super) fn aba(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaEstoque,
) {
    let v = &estado.val;
    let vencidos: Vec<&LoteComValidade> = v.lotes.iter().filter(|l| l.vencido()).collect();
    let semana = v.lotes.iter().filter(|l| (0..=7).contains(&l.dias)).count();
    let em_risco = vencidos
        .iter()
        .fold(Dinheiro::ZERO, |total, l| total + l.valor);
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Lotes vencidos", vencidos.len()).tom(if vencidos.is_empty() {
            Tom::Neutro
        } else {
            Tom::Negativo
        }),
        CartaoKpi::novo("Parado em vencidos", em_risco).tom(Tom::Negativo),
        CartaoKpi::contagem("Vencem em 7 dias", semana).tom(if semana == 0 {
            Tom::Neutro
        } else {
            Tom::Atencao
        }),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    ui.columns(2, |c| {
        lista(&mut c[0], motor, sessao, estado);
        entrada(&mut c[1], motor, sessao, estado);
    });
}

fn lista(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    if let Some(p) = Abas::nova(&[
        (Prazo::Trinta, "Vencidos e até 30 dias"),
        (Prazo::Sessenta, "Até 60 dias"),
        (Prazo::Todos, "Todos"),
    ])
    .selecionada(estado.val.prazo)
    .id_salt("validade-prazo")
    .mostrar(ui)
    {
        estado.val.prazo = p;
        estado.val.carregar(motor, sessao);
    }
    ui.add_space(Espaco::E8);
    let mut abrir = None;
    Grade::nova(vec![
        ColunaGrade::nova("Produto"),
        ColunaGrade::nova("Lote").largura(110.0),
        ColunaGrade::nova("Validade").largura(150.0),
        ColunaGrade::nova("Saldo").largura(90.0).numero(),
        ColunaGrade::nova("Valor").largura(100.0).numero(),
        ColunaGrade::nova("").largura(100.0),
    ])
    .id_salt("validade-lotes")
    .vazio("Nenhum lote com validade neste prazo.")
    .mostrar(ui, estado.val.lotes.len(), |i, row| {
        let l = &estado.val.lotes[i];
        row.col(|ui| {
            ui.vertical(|ui| {
                ui.add(Rotulo::interface(l.nome.clone()));
                ui.add(Rotulo::apoio(l.nome_local.clone()));
            });
        });
        row.col(|ui| {
            ui.add(Rotulo::campo(l.codigo.clone()));
        });
        row.col(|ui| {
            ui.vertical(|ui| {
                ui.add(Rotulo::interface(l.validade.formatar()));
                ui.add(prazo_etiqueta(l));
            });
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(format!("{} {}", l.saldo, l.unidade)));
        });
        row.col(|ui| {
            ui.add(cardeal_ui::atoms::ValorDinheiro::novo(l.valor).neutro());
        });
        row.col(|ui| {
            if ui.add(Botao::secundario("Dar baixa").pequeno()).clicked() {
                abrir = Some(i);
            }
        });
    });
    if let Some(i) = abrir {
        estado.val.abrir_baixa(i);
    }
    baixa(ui, motor, sessao, estado);
}

fn baixa(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    let Some(l) = estado
        .val
        .baixa
        .and_then(|i| estado.val.lotes.get(i))
        .cloned()
    else {
        return;
    };
    ui.add_space(Espaco::E12);
    Painel::novo().realce(Tom::Atencao).mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao(format!(
            "Dar baixa — {} · lote {}",
            l.nome, l.codigo
        )));
        ui.add(Rotulo::apoio(
            "Sai do estoque e vai para perdas no razão, ao custo médio do produto.",
        ));
        ui.add_space(Espaco::E8);
        let v = &mut estado.val;
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Quantidade", &mut v.baixa_qtd));
            SeletorOpcao::novo("Motivo", &mut v.baixa_motivo)
                .opcoes([
                    (MotivoPerda::Vencimento, "Vencimento"),
                    (MotivoPerda::Quebra, "Quebra/avaria"),
                    (MotivoPerda::Furto, "Furto/extravio"),
                    (MotivoPerda::Outro, "Outro"),
                ])
                .mostrar(&mut c[1]);
        });
        ui.add(Campo::novo("Observação", &mut v.baixa_obs).marcador("opcional"));
        ui.add_space(Espaco::E8);
        ui.horizontal(|ui| {
            if ui.add(Botao::primario("Dar baixa")).clicked() {
                match dar_baixa(motor, sessao, estado) {
                    Ok(r) => notificar(
                        ui.ctx(),
                        Notificacao::sucesso(format!(
                            "Baixa registrada: {} em perdas{}",
                            r.valor,
                            if r.lote_esgotado {
                                " — o lote acabou"
                            } else {
                                ""
                            }
                        )),
                    ),
                    Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
                }
            }
            if ui.add(Botao::fantasma("Cancelar")).clicked() {
                estado.val.baixa = None;
            }
        });
    });
}

fn entrada(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaEstoque) {
    Painel::novo().plano().mostrar(ui, |ui| {
        ui.add(Rotulo::titulo_secao("Entrada com validade"));
        ui.add(Rotulo::apoio(
            "Cada entrada vira um lote com a sua validade. O código do lote é opcional: em \
             branco, o sistema gera um a partir da validade.",
        ));
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
        let locais: Vec<(Id, String)> = estado
            .locais
            .iter()
            .map(|l| (l.id, l.nome.clone()))
            .collect();
        if estado.val.ent_local.is_none() && locais.len() == 1 {
            estado.val.ent_local = Some(locais[0].0);
        }
        let v = &mut estado.val;
        SeletorBusca::novo("Produto", &mut v.ent_busca, &mut v.ent_produto)
            .opcoes(opcoes)
            .marcador("Código de barras ou nome")
            .mostrar(ui);
        ui.add_space(Espaco::E8);
        SeletorOpcao::novo("Local", &mut v.ent_local)
            .opcoes(locais)
            .placeholder("Onde a mercadoria fica")
            .mostrar(ui);
        ui.add_space(Espaco::E8);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Quantidade", &mut v.ent_qtd).marcador("0"));
            c[1].add(Campo::novo("Custo unitário", &mut v.ent_custo).marcador("0,00"));
        });
        ui.columns(2, |c| {
            c[0].add(
                Campo::novo("Validade", &mut v.ent_validade)
                    .mascara(Mascara::Data)
                    .marcador("dd/mm/aaaa"),
            );
            c[1].add(Campo::novo("Código do lote", &mut v.ent_codigo).marcador("gerado"));
        });
        ui.add_space(Espaco::E8);
        if ui.add(Botao::primario("Registrar entrada")).clicked() {
            match registrar_entrada(motor, sessao, estado) {
                Ok(_) => notificar(ui.ctx(), Notificacao::sucesso("Entrada registrada")),
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e)),
            }
        }
    });
}
