//! A aba "Custos": para onde vai o dinheiro da empresa, por categoria e mês — o que já foi
//! pago, o que está em aberto e o que as recorrências ainda vão gerar (aluguel, pró-labore,
//! energia…). Um clique na categoria abre "A pagar" filtrado por ela: o porquê de cada custo.

use super::*;
use mod_financeiro::{ItemProjecaoCategoria, ProjecaoPorCategoria, CATEGORIAS_SUGERIDAS};

/// Rótulo de coluna por mês (a janela tem no máximo 12 meses, então nunca repete).
const MES_CURTO: [&str; 12] = [
    "Jan", "Fev", "Mar", "Abr", "Mai", "Jun", "Jul", "Ago", "Set", "Out", "Nov", "Dez",
];

/// Uma linha da tabela: a categoria e o total de cada mês da janela.
pub(super) struct LinhaCustos {
    pub(super) categoria: Option<Id>,
    pub(super) nome: String,
    pub(super) por_mes: Vec<Dinheiro>,
    pub(super) total: Dinheiro,
}

impl EstadoTelaFinanceiro {
    pub(super) fn carregar_custos(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if self.custos_meses == 0 {
            self.custos_meses = 6;
        }
        let r = motor.consultar(
            sessao,
            "financeiro.projecao_por_categoria.v1",
            &ProjecaoPorCategoria {
                especie: self.custos_especie(),
                de: Data::hoje(Fuso::BRASILIA).competencia(),
                meses: self.custos_meses,
            },
        );
        self.projecao = self.anotar(r).unwrap_or_default();
    }

    pub(super) const fn custos_especie(&self) -> EspecieTitulo {
        if self.custos_receitas {
            EspecieTitulo::Receber
        } else {
            EspecieTitulo::Pagar
        }
    }

    /// Os meses da janela, a partir do atual.
    fn meses_custos(&self) -> Vec<Competencia> {
        let mut c = Data::hoje(Fuso::BRASILIA).competencia();
        (0..self.custos_meses)
            .map(|_| {
                let m = c;
                c = c.proxima();
                m
            })
            .collect()
    }

    /// A tabela categoria × mês, da categoria que mais pesa para a que menos pesa.
    pub(super) fn linhas_custos(&self) -> Vec<LinhaCustos> {
        let meses = self.meses_custos();
        let mut por_cat: HashMap<Option<Id>, Vec<Dinheiro>> = HashMap::new();
        for it in &self.projecao {
            let Some(i) = meses.iter().position(|m| *m == it.competencia) else {
                continue;
            };
            por_cat
                .entry(it.categoria)
                .or_insert_with(|| vec![Dinheiro::ZERO; meses.len()])[i] += it.total();
        }
        let mut linhas: Vec<LinhaCustos> = por_cat
            .into_iter()
            .map(|(categoria, por_mes)| LinhaCustos {
                categoria,
                nome: self.nome_categoria(categoria),
                total: por_mes.iter().fold(Dinheiro::ZERO, |a, v| a + *v),
                por_mes,
            })
            .collect();
        linhas.sort_by_key(|l| std::cmp::Reverse(l.total));
        linhas
    }

    fn soma_projecao(&self, filtro: impl Fn(&ItemProjecaoCategoria) -> bool) -> Dinheiro {
        self.projecao
            .iter()
            .filter(|i| filtro(i))
            .fold(Dinheiro::ZERO, |a, i| a + i.total())
    }

    /// Faltam categorias sugeridas (o botão para criá-las aparece).
    fn faltam_sugeridas(&self) -> bool {
        CATEGORIAS_SUGERIDAS.iter().any(|(nome, _)| {
            !self
                .categorias
                .iter()
                .any(|c| c.nome.trim().eq_ignore_ascii_case(nome))
        })
    }
}

pub(super) fn painel_custos(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    let mut recarregar = false;
    ui.horizontal_wrapped(|ui| {
        for (receitas, rot) in [(false, "Custos"), (true, "Receitas")] {
            let b = if estado.custos_receitas == receitas {
                Botao::primario(rot)
            } else {
                Botao::fantasma(rot)
            };
            if ui.add(b.pequeno()).clicked() && estado.custos_receitas != receitas {
                estado.custos_receitas = receitas;
                recarregar = true;
            }
        }
        ui.add_space(Espaco::E16);
        ui.add(Rotulo::campo("PRÓXIMOS"));
        for meses in [3_u8, 6, 12] {
            let b = if estado.custos_meses == meses {
                Botao::primario(format!("{meses} meses"))
            } else {
                Botao::fantasma(format!("{meses} meses"))
            };
            if ui.add(b.pequeno()).clicked() && estado.custos_meses != meses {
                estado.custos_meses = meses;
                recarregar = true;
            }
        }
        if estado.faltam_sugeridas() {
            ui.add_space(Espaco::E16);
            if ui
                .add(Botao::secundario("+ Categorias sugeridas").pequeno())
                .clicked()
            {
                criar_sugeridas(ui.ctx(), motor, sessao, estado);
                recarregar = true;
            }
        }
    });
    if recarregar {
        estado.carregar_custos(motor, sessao);
    }
    ui.add_space(Espaco::E12);

    let receitas = estado.custos_receitas;
    let atual = Data::hoje(Fuso::BRASILIA).competencia();
    let no_mes = estado.soma_projecao(|i| i.competencia == atual);
    let pago_mes = estado
        .projecao
        .iter()
        .filter(|i| i.competencia == atual)
        .fold(Dinheiro::ZERO, |a, i| a + i.realizado);
    let seguintes = estado.soma_projecao(|i| i.competencia > atual);
    let sem_categoria = estado.soma_projecao(|i| i.categoria.is_none());
    let tom = if receitas {
        Tom::Positivo
    } else {
        Tom::Negativo
    };
    let tom_sem = if sem_categoria.e_positivo() {
        Tom::Atencao
    } else {
        Tom::Neutro
    };
    FaixaKpi::nova(vec![
        CartaoKpi::novo(
            if receitas {
                "Receita prevista no mês"
            } else {
                "Custo previsto no mês"
            },
            no_mes,
        )
        .tom(tom),
        CartaoKpi::novo(
            if receitas {
                "Já recebido no mês"
            } else {
                "Já pago no mês"
            },
            pago_mes,
        )
        .tom(tom),
        CartaoKpi::novo(
            format!(
                "Meses seguintes ({})",
                estado.custos_meses.saturating_sub(1)
            ),
            seguintes,
        )
        .tom(tom),
        CartaoKpi::novo("Sem categoria", sem_categoria).tom(tom_sem),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E16);

    let linhas = estado.linhas_custos();
    if linhas.is_empty() {
        EstadoVazio::novo(
            Icone::Dinheiro,
            if receitas {
                "Nenhuma receita prevista nesses meses."
            } else {
                "Nenhum custo previsto nesses meses — lance as contas fixas (aluguel, \
                 pró-labore, energia) como recorrência para enxergar o futuro aqui."
            },
        )
        .mostrar(ui);
        return;
    }

    graficos(ui, estado, &linhas);
    ui.add_space(Espaco::E16);
    ui.add(Rotulo::titulo_secao(if receitas {
        "Receitas por categoria, mês a mês"
    } else {
        "Custos por categoria, mês a mês"
    }));
    ui.add(Rotulo::campo(
        "Cada valor soma o que já foi pago, o que está em aberto (o vencido conta no mês \
         atual) e o que as recorrências ainda vão gerar. Clique numa categoria para ver as contas.",
    ));
    ui.add_space(Espaco::E8);
    if let Some(i) = tabela(ui, estado, &linhas) {
        abrir_categoria(motor, sessao, estado, &linhas[i]);
    }
    ui.add_space(Espaco::E16);
    detalhe_do_mes(ui, estado, atual);
}

fn graficos(ui: &mut egui::Ui, estado: &EstadoTelaFinanceiro, linhas: &[LinhaCustos]) {
    let sinal = if estado.custos_receitas { 1.0 } else { -1.0 };
    let barras: Vec<cardeal_ui::organisms::ItemBarraHorizontal> = linhas
        .iter()
        .take(8)
        .map(|l| cardeal_ui::organisms::ItemBarraHorizontal {
            rotulo: l.nome.clone(),
            valor: reais_para_grafico(l.total) * sinal,
        })
        .collect();
    let meses = estado.meses_custos();
    let eixo: Vec<String> = meses.iter().map(|m| nome_mes(*m)).collect();
    let soma = |f: fn(&ItemProjecaoCategoria) -> Dinheiro| -> Vec<f64> {
        meses
            .iter()
            .map(|m| {
                reais_para_grafico(
                    estado
                        .projecao
                        .iter()
                        .filter(|i| i.competencia == *m)
                        .fold(Dinheiro::ZERO, |a, i| a + f(i)),
                )
            })
            .collect()
    };
    let cores = ui.cores();
    let series = [
        SerieBarras {
            rotulo: if estado.custos_receitas {
                "Recebido"
            } else {
                "Pago"
            }
            .to_owned(),
            cor: cores.positivo,
            valores: soma(|i| i.realizado),
        },
        SerieBarras {
            rotulo: "Em aberto".to_owned(),
            cor: cores.atencao,
            valores: soma(|i| i.em_aberto),
        },
        SerieBarras {
            rotulo: "Recorrências previstas".to_owned(),
            cor: cores.info,
            valores: soma(|i| i.recorrente),
        },
    ];
    let titulo_cat = if estado.custos_receitas {
        "De onde vem o dinheiro"
    } else {
        "Para onde vai o dinheiro"
    };
    ui.columns(2, |c| {
        c[0].add(Rotulo::titulo_secao(titulo_cat));
        c[0].add_space(Espaco::E8);
        Painel::novo().plano().mostrar(&mut c[0], |ui| {
            GraficoBarrasHorizontais::novo(&barras).mostrar(ui);
        });
        c[1].add(Rotulo::titulo_secao("Mês a mês"));
        c[1].add_space(Espaco::E8);
        Painel::novo().plano().mostrar(&mut c[1], |ui| {
            GraficoBarras::novo(&eixo, &series)
                .altura(220.0)
                .mostrar(ui);
        });
    });
}

/// A tabela categoria × mês; devolve a linha clicada.
fn tabela(
    ui: &mut egui::Ui,
    estado: &EstadoTelaFinanceiro,
    linhas: &[LinhaCustos],
) -> Option<usize> {
    let meses = estado.meses_custos();
    let mut colunas = vec![ColunaGrade::nova("Categoria").largura(200.0)];
    for m in &meses {
        let i = (m.mes().clamp(1, 12) - 1) as usize;
        colunas.push(ColunaGrade::nova(MES_CURTO[i]).largura(104.0).numero());
    }
    colunas.push(ColunaGrade::nova("Total").largura(120.0).numero());
    let resposta = Grade::nova(colunas)
        .id_salt("custos-categorias")
        .selecionavel(None)
        .mostrar(ui, linhas.len(), |i, row| {
            let l = &linhas[i];
            row.col(|ui| {
                if l.categoria.is_none() {
                    ui.add(Etiqueta::atencao("Sem categoria"));
                } else {
                    ui.add(Rotulo::interface(l.nome.clone()));
                }
            });
            for v in &l.por_mes {
                row.col(|ui| {
                    if v.e_zero() {
                        ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                    } else {
                        ui.add(ValorDinheiro::novo(*v).neutro());
                    }
                });
            }
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(l.total).neutro());
            });
        });
    resposta.linha_clicada
}

/// O mês atual aberto por tipo: pago, em aberto e recorrente, por categoria.
fn detalhe_do_mes(ui: &mut egui::Ui, estado: &EstadoTelaFinanceiro, atual: Competencia) {
    let mut itens: Vec<&ItemProjecaoCategoria> = estado
        .projecao
        .iter()
        .filter(|i| i.competencia == atual)
        .collect();
    if itens.is_empty() {
        return;
    }
    itens.sort_by_key(|i| std::cmp::Reverse(i.total()));
    ui.add(Rotulo::titulo_secao(format!(
        "Detalhe de {}",
        nome_mes(atual)
    )));
    ui.add_space(Espaco::E8);
    let pago = if estado.custos_receitas {
        "Recebido"
    } else {
        "Pago"
    };
    Grade::nova(vec![
        ColunaGrade::nova("Categoria").largura(200.0),
        ColunaGrade::nova(pago).largura(130.0).numero(),
        ColunaGrade::nova("Em aberto").largura(130.0).numero(),
        ColunaGrade::nova("Recorrências").largura(130.0).numero(),
        ColunaGrade::nova("Total").largura(130.0).numero(),
    ])
    .id_salt("custos-detalhe-mes")
    .mostrar(ui, itens.len(), |i, row| {
        let it = itens[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(estado.nome_categoria(it.categoria)));
        });
        for v in [it.realizado, it.em_aberto, it.recorrente, it.total()] {
            row.col(|ui| {
                if v.e_zero() {
                    ui.add(Rotulo::interface("—").cor(ui.cores().texto_fraco));
                } else {
                    ui.add(ValorDinheiro::novo(v).neutro());
                }
            });
        }
    });
}

/// Abre "A pagar"/"A receber" do ano filtrado pela categoria clicada.
fn abrir_categoria(
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
    linha: &LinhaCustos,
) {
    estado.aba = if estado.custos_receitas {
        Aba::Receber
    } else {
        Aba::Pagar
    };
    estado.periodo.preset = PresetPeriodo::Ano;
    estado.filtro_categoria = Some(
        linha
            .categoria
            .map_or(FiltroCategoria::Sem, FiltroCategoria::Uma),
    );
    estado.carregar(motor, sessao);
}

fn criar_sugeridas(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaFinanceiro,
) {
    match motor.executar(
        sessao,
        "financeiro.criar_categorias_sugeridas.v1",
        &mod_financeiro::CriarCategoriasSugeridas,
    ) {
        Ok(n) => {
            let n: u32 = n;
            estado.carregar_categorias(motor, sessao);
            notificar(
                ctx,
                Notificacao::sucesso(format!("{n} categorias criadas (pró-labore, aluguel…)")),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}
