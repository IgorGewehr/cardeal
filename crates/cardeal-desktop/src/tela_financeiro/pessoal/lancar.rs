//! Lançar uma receita ou despesa pessoal: uma vez, parcelada (o total dividido, como a
//! compra no cartão) ou todo mês (o valor cheio, como a faculdade ou o pró-labore).

use super::*;
use cardeal_ui::atoms::Caixa;
use cardeal_ui::molecules::{Campo, Mascara};
use cardeal_ui::organisms::Dialogo;
use mod_financeiro::pessoal::comandos::LancarPessoal;
use mod_financeiro::pessoal::{NovoPessoal, Repeticao};

const CATEGORIAS_DESPESA: &[&str] = &[
    "Moradia",
    "Contas da casa",
    "Alimentação",
    "Educação",
    "Saúde",
    "Transporte",
    "Lazer",
    "Compras",
    "Assinaturas",
];
const CATEGORIAS_RECEITA: &[&str] = &[
    "Pró-labore",
    "Salário",
    "Vendas por fora",
    "Outras entradas",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum ModoRepeticao {
    Unica,
    Parcelada,
    Mensal,
}

/// O formulário.
pub(crate) struct FormPessoal {
    tipo: TipoPessoal,
    descricao: String,
    valor: String,
    vencimento: String,
    categoria: String,
    repeticao: ModoRepeticao,
    vezes: String,
    cartao: String,
    primeira_paga: bool,
}

impl FormPessoal {
    pub(crate) fn novo(tipo: TipoPessoal) -> Self {
        Self {
            tipo,
            descricao: String::new(),
            valor: String::new(),
            vencimento: Data::hoje(Fuso::BRASILIA).to_string(),
            categoria: String::new(),
            repeticao: ModoRepeticao::Unica,
            vezes: "12".to_owned(),
            cartao: String::new(),
            primeira_paga: false,
        }
    }

    #[cfg(test)]
    pub(super) fn preencher(
        &mut self,
        descricao: &str,
        valor: &str,
        categoria: &str,
        cartao: &str,
    ) {
        descricao.clone_into(&mut self.descricao);
        valor.clone_into(&mut self.valor);
        categoria.clone_into(&mut self.categoria);
        cartao.clone_into(&mut self.cartao);
    }

    #[cfg(test)]
    pub(super) fn mensal(&mut self, meses: u16) {
        self.repeticao = ModoRepeticao::Mensal;
        self.vezes = meses.to_string();
    }

    #[cfg(test)]
    pub(super) fn vazio(&self) -> bool {
        self.descricao.is_empty() && self.valor.is_empty()
    }

    #[cfg(feature = "demo")]
    pub(super) fn demo(&mut self) {
        "Tênis".clone_into(&mut self.descricao);
        "600,00".clone_into(&mut self.valor);
        "Compras".clone_into(&mut self.categoria);
        "Nubank".clone_into(&mut self.cartao);
        self.repeticao = ModoRepeticao::Parcelada;
        "6".clone_into(&mut self.vezes);
    }

    fn validado(&self) -> Result<NovoPessoal, &'static str> {
        let valor = self
            .valor
            .parse::<Dinheiro>()
            .ok()
            .filter(|v| v.e_positivo())
            .ok_or("Informe o valor (ex.: 800,00).")?;
        let vencimento = self
            .vencimento
            .parse::<Data>()
            .map_err(|_| "Informe a data (dd/mm/aaaa).")?;
        let vezes = || {
            self.vezes
                .trim()
                .parse::<u16>()
                .ok()
                .filter(|n| (1..=120).contains(n))
                .ok_or("Quantidade: de 1 a 120.")
        };
        let repeticao = match self.repeticao {
            ModoRepeticao::Unica => Repeticao::Unica,
            ModoRepeticao::Parcelada => Repeticao::Parcelada {
                vezes: vezes().and_then(|n| {
                    if n >= 2 {
                        Ok(n)
                    } else {
                        Err("Parcelado: pelo menos 2 vezes.")
                    }
                })?,
            },
            ModoRepeticao::Mensal => Repeticao::Mensal { meses: vezes()? },
        };
        if self.descricao.trim().is_empty() {
            return Err("Informe a descrição.");
        }
        Ok(NovoPessoal {
            tipo: self.tipo,
            descricao: self.descricao.trim().to_owned(),
            categoria: self.categoria.trim().to_owned(),
            valor,
            primeiro_vencimento: vencimento,
            repeticao,
            cartao: (self.tipo == TipoPessoal::Despesa && !self.cartao.trim().is_empty())
                .then(|| self.cartao.trim().to_owned()),
            primeira_paga: self.primeira_paga,
        })
    }
}

/// Uma fileira de sugestões clicáveis que preenchem `alvo`.
fn sugestoes(ui: &mut egui::Ui, itens: &[String], alvo: &mut String) {
    ui.horizontal_wrapped(|ui| {
        for s in itens {
            let b = if alvo.trim() == s {
                Botao::primario(s.clone())
            } else {
                Botao::fantasma(s.clone())
            };
            if ui.add(b.pequeno()).clicked() {
                alvo.clone_from(s);
            }
        }
    });
}

pub(super) fn dialogo(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoPessoal,
) {
    let DlgPessoal::Novo(f) = &estado.dlg else {
        return;
    };
    let despesa = f.tipo == TipoPessoal::Despesa;
    let (padrao, do_outro_tipo) = if despesa {
        (CATEGORIAS_DESPESA, CATEGORIAS_RECEITA)
    } else {
        (CATEGORIAS_RECEITA, CATEGORIAS_DESPESA)
    };
    let mut categorias: Vec<String> = padrao.iter().map(|s| (*s).to_owned()).collect();
    for c in &estado.sugestoes.categorias {
        if !categorias.contains(c) && !do_outro_tipo.contains(&c.as_str()) {
            categorias.push(c.clone());
        }
    }
    let cartoes = estado.sugestoes.cartoes.clone();

    let fechar = Dialogo::nova(if despesa {
        "Nova despesa pessoal"
    } else {
        "Nova receita pessoal"
    })
    .descricao("Só você vê seus lançamentos pessoais; eles não entram nas contas da empresa.")
    .largura(700.0)
    .mostrar(
        ctx,
        estado,
        |ui, estado| {
            let DlgPessoal::Novo(f) = &mut estado.dlg else {
                return;
            };
            ui.horizontal(|ui| {
                for (t, rot) in [
                    (TipoPessoal::Despesa, "Despesa"),
                    (TipoPessoal::Receita, "Receita"),
                ] {
                    let b = if f.tipo == t {
                        Botao::primario(rot)
                    } else {
                        Botao::fantasma(rot)
                    };
                    if ui.add(b.pequeno()).clicked() {
                        f.tipo = t;
                    }
                }
            });
            ui.add_space(Espaco::E12);
            ui.columns(2, |c| {
                c[0].add(
                    Campo::novo("Descrição", &mut f.descricao).marcador(if despesa {
                        "ex.: faculdade, notebook, conta de luz"
                    } else {
                        "ex.: pró-labore, venda do celular"
                    }),
                );
                c[1].columns(2, |c| {
                    c[0].add(Campo::novo("Valor", &mut f.valor).marcador("0,00"));
                    c[1].add(
                        Campo::novo(
                            if f.cartao.trim().is_empty() {
                                "1º vencimento"
                            } else {
                                "Vencimento da 1ª fatura"
                            },
                            &mut f.vencimento,
                        )
                        .mascara(Mascara::Data),
                    );
                });
            });
            ui.add_space(Espaco::E12);
            ui.add(Campo::novo("Categoria", &mut f.categoria).marcador("escolha abaixo ou digite"));
            sugestoes(ui, &categorias, &mut f.categoria);
            ui.add_space(Espaco::E12);
            let despesa = f.tipo == TipoPessoal::Despesa;
            ui.columns(2, |c| {
                c[0].horizontal(|ui| {
                    ui.add(Rotulo::sobrelinha("Repete"));
                    for (m, rot) in [
                        (ModoRepeticao::Unica, "Uma vez"),
                        (ModoRepeticao::Parcelada, "Parcelado"),
                        (ModoRepeticao::Mensal, "Todo mês"),
                    ] {
                        let b = if f.repeticao == m {
                            Botao::primario(rot)
                        } else {
                            Botao::fantasma(rot)
                        };
                        if ui.add(b.pequeno()).clicked() {
                            f.repeticao = m;
                        }
                    }
                });
                let (rotulo, dica) = match f.repeticao {
                    ModoRepeticao::Unica => ("", ""),
                    ModoRepeticao::Parcelada => (
                        "Em quantas vezes",
                        "O valor é o total da compra; cada mês recebe uma parcela.",
                    ),
                    ModoRepeticao::Mensal => (
                        "Por quantos meses",
                        "O valor se repete cheio todo mês (faculdade, salário, parcelas \
                         que já vinham de antes).",
                    ),
                };
                if !rotulo.is_empty() {
                    c[0].add_space(Espaco::E8);
                    c[0].add(Campo::novo(rotulo, &mut f.vezes));
                    c[0].add(Rotulo::campo(dica).quebravel());
                }
                if despesa {
                    c[1].add(
                        Campo::novo("Cartão de crédito (opcional)", &mut f.cartao)
                            .marcador("ex.: Nubank — vazio se não for no cartão"),
                    );
                    if !cartoes.is_empty() {
                        sugestoes(&mut c[1], &cartoes, &mut f.cartao);
                    }
                }
            });
            ui.add_space(Espaco::E12);
            ui.add(Caixa::nova(
                &mut f.primeira_paga,
                if despesa {
                    "A primeira já está paga"
                } else {
                    "A primeira já foi recebida"
                },
            ));
        },
        |ui, estado| {
            if ui.add(Botao::primario("Lançar e continuar")).clicked() {
                lancar(ui.ctx(), motor, sessao, estado, true);
            }
            if ui.add(Botao::secundario("Lançar e fechar")).clicked() {
                lancar(ui.ctx(), motor, sessao, estado, false);
            }
            if ui.add(Botao::fantasma("Cancelar")).clicked() {
                estado.dlg = DlgPessoal::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = DlgPessoal::Fechado;
    }
}

pub(super) fn lancar(
    ctx: &egui::Context,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoPessoal,
    continuar: bool,
) {
    let DlgPessoal::Novo(f) = &estado.dlg else {
        return;
    };
    let novo = match f.validado() {
        Ok(n) => n,
        Err(msg) => return notificar(ctx, Notificacao::aviso(msg)),
    };
    let (tipo, vencimento) = (f.tipo, f.vencimento.clone());
    match motor.executar(sessao, "financeiro.lancar_pessoal.v1", &LancarPessoal(novo)) {
        Ok(ids) => {
            let ids: Vec<Id> = ids;
            estado.dlg = if continuar {
                let mut f = FormPessoal::novo(tipo);
                f.vencimento = vencimento;
                DlgPessoal::Novo(Box::new(f))
            } else {
                DlgPessoal::Fechado
            };
            estado.carregar(motor, sessao);
            notificar(
                ctx,
                Notificacao::sucesso(if ids.len() == 1 {
                    "Lançado".to_owned()
                } else {
                    format!("{} lançamentos gerados, mês a mês", ids.len())
                }),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
