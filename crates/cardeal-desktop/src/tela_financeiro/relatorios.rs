//! Financeiro › "Relatórios": contas a receber e a pagar por faixa de vencimento, recebimentos e
//! pagamentos por dia e o fluxo de caixa previsto — cada um com a busca por pessoa ou descrição e
//! exportação em PDF e Excel (`exportar`). O agrupamento é de `mod_financeiro::relatorios`.

use super::exportar::{self, Coluna, Linha as LinhaExport, Relatorio, Tabela, Valor};
use super::*;
use cardeal_ui::tokens::Papel;
use mod_financeiro::relatorios::{
    fluxo_previsto, por_dia, por_faixa, FaixaDeVencimento, ItemBaixaNoPeriodo, PagamentosNoPeriodo,
    RecebimentosNoPeriodo,
};

/// Qual relatório está aberto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum QualRelatorio {
    #[default]
    ContasAPagar,
    ContasAReceber,
    PagamentosPorDia,
    RecebimentosPorDia,
    FluxoPrevisto,
}

impl QualRelatorio {
    const fn titulo(self) -> &'static str {
        match self {
            Self::ContasAPagar => "Contas a pagar",
            Self::ContasAReceber => "Contas a receber",
            Self::PagamentosPorDia => "Pagamentos por dia",
            Self::RecebimentosPorDia => "Recebimentos por dia",
            Self::FluxoPrevisto => "Fluxo de caixa previsto",
        }
    }
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoRelatorios {
    pub(super) qual: QualRelatorio,
    pub(super) periodo: FiltroPeriodo,
    pub(super) busca: String,
    /// Horizonte do previsto, em dias (30, 60, 90).
    pub(super) horizonte: i32,
    pub(super) recebimentos: Option<Result<Vec<ItemBaixaNoPeriodo>, String>>,
    pub(super) pagamentos: Option<Result<Vec<ItemBaixaNoPeriodo>, String>>,
    /// O saldo de hoje das contas do Disponível (o ponto de partida do previsto).
    pub(super) saldo_de_hoje: Option<Result<Dinheiro, String>>,
}

impl EstadoTelaFinanceiro {
    pub(super) fn carregar_relatorios(&mut self, motor: &Motor, sessao: &Sessao) {
        let periodo = self.relatorios.periodo.resolver(Data::hoje(Fuso::BRASILIA));
        let r = &mut self.relatorios;
        if r.horizonte == 0 {
            r.horizonte = 30;
        }
        r.recebimentos = sessao.concede("financeiro.receber.ver").then(|| {
            motor
                .consultar(
                    sessao,
                    "financeiro.recebimentos_no_periodo.v1",
                    &RecebimentosNoPeriodo { periodo },
                )
                .map_err(|e| e.mensagem)
        });
        r.pagamentos = sessao.concede("financeiro.pagar.ver").then(|| {
            motor
                .consultar(
                    sessao,
                    "financeiro.pagamentos_no_periodo.v1",
                    &PagamentosNoPeriodo { periodo },
                )
                .map_err(|e| e.mensagem)
        });
        let contas: Result<Vec<ItemContaDisponivel>, _> = motor.consultar(
            sessao,
            "financeiro.contas_disponiveis.v1",
            &ContasDisponiveis,
        );
        r.saldo_de_hoje = Some(
            contas
                .map(|v| v.iter().map(|c| c.saldo).sum())
                .map_err(|e| e.mensagem),
        );
    }
}

/// "10 de maio de 2026".
pub(super) fn por_extenso(d: Data) -> String {
    format!("{} de {}", d.dia(), d.competencia().formatar_extenso())
}

/// A busca casa com a pessoa, a descrição ou a categoria (sem diferenciar maiúsculas).
fn casa(busca: &str, campos: &[&str]) -> bool {
    let b = busca.trim().to_lowercase();
    b.is_empty() || campos.iter().any(|c| c.to_lowercase().contains(&b))
}

/// "Vencida há 12 dias", "Vence hoje", "Em 5 dias".
fn situacao(vencimento: Data, hoje: Data) -> String {
    let d = hoje.dias_ate(vencimento);
    match d {
        0 => "Vence hoje".to_owned(),
        1 => "Amanhã".to_owned(),
        d if d < 0 => format!("Vencida há {} dia{}", -d, if d == -1 { "" } else { "s" }),
        d => format!("Em {d} dias"),
    }
}

/// Uma linha das grades da aba.
enum Linha {
    /// O cabeçalho de um grupo (faixa, dia) com o total.
    Grupo(String, Dinheiro),
    /// Um item: as colunas de texto e os valores.
    Item {
        textos: Vec<String>,
        valores: Vec<Dinheiro>,
        alerta: bool,
    },
    /// "Nada nesta faixa".
    Vazio(String),
    /// O total geral.
    Total(String, Vec<Dinheiro>),
}

fn grade(ui: &mut egui::Ui, id: &'static str, colunas: Vec<ColunaGrade>, linhas: &[Linha]) {
    let total_colunas = colunas.len();
    Grade::nova(colunas)
        .id_salt(id)
        .vazio("Nada no período.")
        .mostrar(ui, linhas.len(), |i, row| match &linhas[i] {
            Linha::Grupo(rotulo, total) => {
                row.col(|ui| {
                    ui.add(Rotulo::campo(rotulo.clone()));
                });
                for _ in 1..total_colunas - 1 {
                    row.col(|_| {});
                }
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(*total).neutro().destaque());
                });
            }
            Linha::Item {
                textos,
                valores,
                alerta,
            } => {
                for (j, t) in textos.iter().enumerate() {
                    row.col(|ui| {
                        if j == 0 {
                            ui.add_space(Espaco::E16);
                        }
                        let r = Rotulo::interface(t.clone());
                        ui.add(if *alerta && j == textos.len() - 1 {
                            r.cor(ui.cores().negativo)
                        } else {
                            r
                        });
                    });
                }
                for v in valores {
                    row.col(|ui| super::dre::celula_valor(ui, *v));
                }
            }
            Linha::Vazio(texto) => {
                row.col(|ui| {
                    ui.add_space(Espaco::E16);
                    ui.add(Rotulo::apoio(texto.clone()));
                });
                for _ in 1..total_colunas {
                    row.col(|_| {});
                }
            }
            Linha::Total(rotulo, valores) => {
                row.col(|ui| {
                    ui.add(Rotulo::novo(Papel::TituloSecao, rotulo.clone()));
                });
                for _ in 1..total_colunas - valores.len() {
                    row.col(|_| {});
                }
                for v in valores {
                    row.col(|ui| {
                        ui.add(ValorDinheiro::novo(*v).neutro().destaque());
                    });
                }
            }
        });
}

pub(super) fn painel(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let mut opcoes = Vec::new();
    if sessao.concede("financeiro.pagar.ver") {
        opcoes.push((QualRelatorio::ContasAPagar, "Contas a pagar"));
    }
    if sessao.concede("financeiro.receber.ver") {
        opcoes.push((QualRelatorio::ContasAReceber, "Contas a receber"));
    }
    if sessao.concede("financeiro.pagar.ver") {
        opcoes.push((QualRelatorio::PagamentosPorDia, "Pagamentos por dia"));
    }
    if sessao.concede("financeiro.receber.ver") {
        opcoes.push((QualRelatorio::RecebimentosPorDia, "Recebimentos por dia"));
    }
    if sessao.concede("financeiro.banco.ver") {
        opcoes.push((QualRelatorio::FluxoPrevisto, "Fluxo de caixa previsto"));
    }
    if opcoes.is_empty() {
        EstadoVazio::novo(Icone::Dinheiro, "Sem permissão para os relatórios.").mostrar(ui);
        return;
    }
    if !opcoes.iter().any(|(q, _)| *q == estado.relatorios.qual) {
        estado.relatorios.qual = opcoes[0].0;
    }
    if let Some(q) = Abas::nova(&opcoes)
        .selecionada(estado.relatorios.qual)
        .id_salt("financeiro-relatorio")
        .mostrar(ui)
    {
        estado.relatorios.qual = q;
    }
    ui.add_space(Espaco::E12);
    match estado.relatorios.qual {
        QualRelatorio::ContasAPagar | QualRelatorio::ContasAReceber => {
            contas_por_faixa(ui, motor, estado);
        }
        QualRelatorio::PagamentosPorDia | QualRelatorio::RecebimentosPorDia => {
            baixas_por_dia(ui, motor, sessao, estado);
        }
        QualRelatorio::FluxoPrevisto => previsto(ui, motor, sessao, estado),
    }
}

/// A linha de filtros comum: a busca e os botões de exportar.
fn filtros(
    ui: &mut egui::Ui,
    motor: &Motor,
    busca: &mut String,
    montar: impl FnOnce() -> Relatorio,
) {
    ui.horizontal(|ui| {
        ui.add(
            Campo::novo("Buscar pessoa, descrição ou categoria", busca).marcador("Ex.: aluguel"),
        );
        ui.add_space(Espaco::E16);
        exportar::botoes(ui, motor, montar);
    });
    ui.add_space(Espaco::E12);
}

// ---------------------------------------------------------------- contas por faixa

fn contas_por_faixa(ui: &mut egui::Ui, motor: &Motor, estado: &mut EstadoTelaFinanceiro) {
    let qual = estado.relatorios.qual;
    let receber = qual == QualRelatorio::ContasAReceber;
    let hoje = Data::hoje(Fuso::BRASILIA);
    let busca = estado.relatorios.busca.clone();
    let base = if receber {
        &estado.abertos_receber
    } else {
        &estado.abertos_pagar
    };
    let itens: Vec<ItemTituloEmAberto> = base
        .iter()
        .filter(|p| {
            casa(
                &busca,
                &[
                    &estado.nome_contraparte(&p.contraparte),
                    p.descricao.as_deref().unwrap_or(""),
                    &estado.nome_categoria(p.categoria),
                ],
            )
        })
        .cloned()
        .collect();
    let faixas = por_faixa(&itens, hoje);
    let total: Dinheiro = faixas.iter().map(|f| f.total).sum();
    let vencido = faixas
        .iter()
        .find(|f| f.faixa == FaixaDeVencimento::Vencidos)
        .map_or(Dinheiro::ZERO, |f| f.total);
    let proximos_30: Dinheiro = faixas
        .iter()
        .filter(|f| {
            matches!(
                f.faixa,
                FaixaDeVencimento::Hoje
                    | FaixaDeVencimento::Proximos7
                    | FaixaDeVencimento::De8a15
                    | FaixaDeVencimento::De16a30
            )
        })
        .map(|f| f.total)
        .sum();
    let pessoa = if receber { "Cliente" } else { "Fornecedor" };
    // As linhas da tela e do arquivo, montadas uma vez.
    let mut linhas = Vec::new();
    let mut export = Vec::new();
    for f in &faixas {
        linhas.push(Linha::Grupo(
            format!("{} ({})", f.faixa.rotulo(), f.itens.len()),
            f.total,
        ));
        export.push(
            LinhaExport::nova(vec![
                Valor::texto(f.faixa.rotulo()),
                Valor::Vazio,
                Valor::Vazio,
                Valor::Vazio,
                Valor::Vazio,
                Valor::Dinheiro(f.total),
            ])
            .destaque(),
        );
        if f.itens.is_empty() {
            linhas.push(Linha::Vazio(format!(
                "Nada {}.",
                f.faixa.rotulo().to_lowercase()
            )));
        }
        for p in &f.itens {
            let nome = estado.nome_contraparte(&p.contraparte);
            let descricao = p.descricao.clone().unwrap_or_default();
            let categoria = estado.nome_categoria(p.categoria);
            let sit = situacao(p.vencimento, hoje);
            linhas.push(Linha::Item {
                textos: vec![
                    nome.clone(),
                    descricao.clone(),
                    categoria.clone(),
                    p.vencimento.formatar(),
                    sit.clone(),
                ],
                valores: vec![p.saldo()],
                alerta: p.vencimento < hoje,
            });
            export.push(
                LinhaExport::nova(vec![
                    Valor::texto(nome),
                    Valor::texto(descricao),
                    Valor::texto(categoria),
                    Valor::Data(p.vencimento),
                    Valor::texto(sit),
                    Valor::Dinheiro(p.saldo()),
                ])
                .recuo(1),
            );
        }
    }
    let rotulo_total = if receber {
        "Total a receber"
    } else {
        "Total a pagar"
    };
    linhas.push(Linha::Total(rotulo_total.to_owned(), vec![total]));
    export.push(
        LinhaExport::nova(vec![
            Valor::texto(rotulo_total),
            Valor::Vazio,
            Valor::Vazio,
            Valor::Vazio,
            Valor::Vazio,
            Valor::Dinheiro(total),
        ])
        .destaque(),
    );

    FaixaKpi::nova(vec![
        CartaoKpi::novo("Vencido", vencido).tom(if vencido.e_positivo() {
            Tom::Negativo
        } else {
            Tom::Neutro
        }),
        CartaoKpi::novo("Vence nos próximos 30 dias", proximos_30).tom(Tom::Neutro),
        CartaoKpi::novo(rotulo_total, total).tom(if receber {
            Tom::Positivo
        } else {
            Tom::Negativo
        }),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    ui.add(
        Rotulo::apoio(
            "Todo o em aberto, pelo vencimento — o que venceu antes aparece sempre em \"Vencidos\", \
             qualquer que seja a data.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);
    let titulo = qual.titulo();
    let subtitulo = format!("Em aberto em {}", hoje.formatar());
    let mut busca_editada = estado.relatorios.busca.clone();
    filtros(ui, motor, &mut busca_editada, || Relatorio {
        titulo: titulo.to_owned(),
        subtitulo,
        tabelas: vec![Tabela {
            titulo: "Por vencimento".to_owned(),
            nota: (!busca.trim().is_empty()).then(|| format!("Filtrado por \"{}\".", busca.trim())),
            colunas: vec![
                Coluna::texto(pessoa, 3.0),
                Coluna::texto("Descrição", 3.0),
                Coluna::texto("Categoria", 2.0),
                Coluna::texto("Vencimento", 1.4),
                Coluna::texto("Situação", 1.8),
                Coluna::numero("Valor", 1.6),
            ],
            linhas: export,
        }],
    });
    estado.relatorios.busca = busca_editada;
    grade(
        ui,
        "relatorio-faixas",
        vec![
            ColunaGrade::nova(if receber { "Cliente" } else { "Fornecedor" }).largura(240.0),
            ColunaGrade::nova("Descrição"),
            ColunaGrade::nova("Categoria").largura(170.0),
            ColunaGrade::nova("Vencimento").largura(110.0),
            ColunaGrade::nova("Situação").largura(150.0),
            ColunaGrade::nova("Valor").largura(140.0).numero(),
        ],
        &linhas,
    );
}

// ---------------------------------------------------------------- por dia

fn baixas_por_dia(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    estado: &mut EstadoTelaFinanceiro,
) {
    let qual = estado.relatorios.qual;
    let recebimentos = qual == QualRelatorio::RecebimentosPorDia;
    if super::contas::seletor_periodo(ui, &mut estado.relatorios.periodo) {
        estado.carregar_relatorios(motor, sessao);
    }
    ui.add_space(Espaco::E12);
    let periodo = estado
        .relatorios
        .periodo
        .resolver(Data::hoje(Fuso::BRASILIA));
    let dados = if recebimentos {
        &estado.relatorios.recebimentos
    } else {
        &estado.relatorios.pagamentos
    };
    let itens: Vec<ItemBaixaNoPeriodo> = match dados {
        Some(Ok(v)) => v.clone(),
        Some(Err(e)) => {
            ui.add(Rotulo::interface(e.clone()).cor(ui.cores().negativo));
            return;
        }
        None => Vec::new(),
    };
    let busca = estado.relatorios.busca.clone();
    let itens: Vec<ItemBaixaNoPeriodo> = itens
        .into_iter()
        .filter(|b| {
            casa(
                &busca,
                &[
                    &estado.nome_contraparte(&b.contraparte),
                    b.descricao.as_deref().unwrap_or(""),
                    &estado.nome_categoria(b.categoria),
                ],
            )
        })
        .collect();
    let dias = por_dia(&itens);
    let total: Dinheiro = dias.iter().map(|d| d.total).sum();
    let mut linhas = Vec::new();
    let mut export = Vec::new();
    for d in &dias {
        let rotulo = por_extenso(d.data);
        linhas.push(Linha::Grupo(rotulo.clone(), d.total));
        export.push(
            LinhaExport::nova(vec![
                Valor::texto(rotulo),
                Valor::Vazio,
                Valor::Vazio,
                Valor::Dinheiro(d.total),
            ])
            .destaque(),
        );
        for b in &d.itens {
            let nome = estado.nome_contraparte(&b.contraparte);
            let categoria = estado.nome_categoria(b.categoria);
            let descricao = b.descricao.clone().unwrap_or_default();
            linhas.push(Linha::Item {
                textos: vec![nome.clone(), categoria.clone(), descricao.clone()],
                valores: vec![b.valor],
                alerta: false,
            });
            export.push(
                LinhaExport::nova(vec![
                    Valor::texto(nome),
                    Valor::texto(categoria),
                    Valor::texto(descricao),
                    Valor::Dinheiro(b.valor),
                ])
                .recuo(1),
            );
        }
    }
    if !dias.is_empty() {
        linhas.push(Linha::Total("Total do período".to_owned(), vec![total]));
        export.push(
            LinhaExport::nova(vec![
                Valor::texto("Total do período"),
                Valor::Vazio,
                Valor::Vazio,
                Valor::Dinheiro(total),
            ])
            .destaque(),
        );
    }
    let maior = dias.iter().max_by_key(|d| d.total);
    FaixaKpi::nova(vec![
        CartaoKpi::novo(
            if recebimentos {
                "Recebido no período"
            } else {
                "Pago no período"
            },
            total,
        )
        .tom(if recebimentos {
            Tom::Positivo
        } else {
            Tom::Negativo
        }),
        CartaoKpi::novo("Maior dia", maior.map_or(Dinheiro::ZERO, |d| d.total))
            .tom(Tom::Neutro)
            .variacao(maior.map_or_else(String::new, |d| d.data.formatar())),
        CartaoKpi::novo(
            "Média por dia com movimento",
            if dias.is_empty() {
                Dinheiro::ZERO
            } else {
                Dinheiro::centavos(total.em_centavos() / i64::try_from(dias.len()).unwrap_or(1))
            },
        )
        .tom(Tom::Neutro),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    let titulo = qual.titulo();
    let subtitulo = format!("{} a {}", periodo.de.formatar(), periodo.ate.formatar());
    let mut busca_editada = estado.relatorios.busca.clone();
    filtros(ui, motor, &mut busca_editada, || Relatorio {
        titulo: titulo.to_owned(),
        subtitulo,
        tabelas: vec![Tabela {
            titulo: "Por dia".to_owned(),
            nota: Some(
                "Pela data em que o dinheiro entrou ou saiu; baixa estornada não entra.".to_owned(),
            ),
            colunas: vec![
                Coluna::texto("Pessoa", 3.0),
                Coluna::texto("Categoria", 2.0),
                Coluna::texto("Descrição", 3.0),
                Coluna::numero("Valor", 1.6),
            ],
            linhas: export,
        }],
    });
    estado.relatorios.busca = busca_editada;
    grade(
        ui,
        "relatorio-por-dia",
        vec![
            ColunaGrade::nova("Pessoa").largura(260.0),
            ColunaGrade::nova("Categoria").largura(200.0),
            ColunaGrade::nova("Descrição"),
            ColunaGrade::nova("Valor").largura(140.0).numero(),
        ],
        &linhas,
    );
}

// ---------------------------------------------------------------- previsto

fn previsto(ui: &mut egui::Ui, motor: &Motor, sessao: &Sessao, estado: &mut EstadoTelaFinanceiro) {
    let hoje = Data::hoje(Fuso::BRASILIA);
    ui.horizontal(|ui| {
        ui.add(Rotulo::sobrelinha("Até"));
        ui.add_space(Espaco::E8);
        if let Some(h) = Abas::nova(&[(30, "30 dias"), (60, "60 dias"), (90, "90 dias")])
            .selecionada(estado.relatorios.horizonte)
            .id_salt("relatorio-horizonte")
            .mostrar(ui)
        {
            estado.relatorios.horizonte = h;
        }
    });
    ui.add_space(Espaco::E12);
    let saldo_de_hoje = match &estado.relatorios.saldo_de_hoje {
        Some(Ok(s)) => *s,
        Some(Err(e)) => {
            ui.add(Rotulo::interface(e.clone()).cor(ui.cores().negativo));
            return;
        }
        None => {
            estado.carregar_relatorios(motor, sessao);
            return;
        }
    };
    let ate = hoje.mais_dias(estado.relatorios.horizonte.max(1));
    let busca = estado.relatorios.busca.clone();
    let mut em_aberto: Vec<ItemTituloEmAberto> = Vec::new();
    if sessao.concede("financeiro.receber.ver") {
        em_aberto.extend(estado.abertos_receber.iter().cloned());
    }
    if sessao.concede("financeiro.pagar.ver") {
        em_aberto.extend(estado.abertos_pagar.iter().cloned());
    }
    em_aberto.retain(|p| {
        casa(
            &busca,
            &[
                &estado.nome_contraparte(&p.contraparte),
                p.descricao.as_deref().unwrap_or(""),
                &estado.nome_categoria(p.categoria),
            ],
        )
    });
    let linhas_previstas = fluxo_previsto(saldo_de_hoje, &em_aberto, hoje, ate);
    let entradas: Dinheiro = linhas_previstas.iter().map(|l| l.entrada).sum();
    let saidas: Dinheiro = linhas_previstas.iter().map(|l| l.saida).sum();
    let fim = saldo_de_hoje + entradas - saidas;
    let menor = linhas_previstas
        .iter()
        .min_by_key(|l| l.saldo)
        .filter(|l| l.saldo < saldo_de_hoje);

    FaixaKpi::nova(vec![
        CartaoKpi::novo("Saldo de hoje", saldo_de_hoje).tom(Tom::Neutro),
        CartaoKpi::novo("Entradas previstas", entradas).tom(Tom::Positivo),
        CartaoKpi::novo("Saídas previstas", saidas).tom(Tom::Negativo),
        CartaoKpi::novo(format!("Saldo em {}", ate.formatar()), fim).tom(if fim.e_negativo() {
            Tom::Negativo
        } else {
            Tom::Positivo
        }),
    ])
    .mostrar(ui);
    if let Some(m) = menor {
        ui.add_space(Espaco::E8);
        let texto = format!(
            "O menor saldo do período é {} em {}",
            m.saldo.formatar_com_simbolo(),
            m.parcela.vencimento.formatar()
        );
        ui.add(if m.saldo.e_negativo() {
            Etiqueta::negativa(format!("{texto} — falta dinheiro nesse dia"))
        } else {
            Etiqueta::info(texto)
        });
    }
    ui.add_space(Espaco::E12);
    ui.add(
        Rotulo::apoio(
            "Do saldo de hoje nas contas e no caixa, cada conta a receber e a pagar em aberto pelo \
             vencimento. As vencidas e ainda em aberto vêm primeiro: é dinheiro que o saldo de hoje \
             ainda não viu.",
        )
        .quebravel(),
    );
    ui.add_space(Espaco::E8);

    let mut linhas = vec![Linha::Item {
        textos: vec![
            hoje.formatar(),
            "Saldo de hoje".to_owned(),
            String::new(),
            String::new(),
        ],
        valores: vec![Dinheiro::ZERO, Dinheiro::ZERO, saldo_de_hoje],
        alerta: false,
    }];
    let mut export = vec![LinhaExport::nova(vec![
        Valor::Data(hoje),
        Valor::texto("Saldo de hoje"),
        Valor::Vazio,
        Valor::Vazio,
        Valor::Vazio,
        Valor::Vazio,
        Valor::Dinheiro(saldo_de_hoje),
    ])
    .destaque()];
    for l in &linhas_previstas {
        let p = l.parcela;
        let nome = estado.nome_contraparte(&p.contraparte);
        let categoria = estado.nome_categoria(p.categoria);
        let descricao = p.descricao.clone().unwrap_or_default();
        let data = if l.vencida {
            format!("{} (vencida)", p.vencimento.formatar())
        } else {
            p.vencimento.formatar()
        };
        linhas.push(Linha::Item {
            textos: vec![
                data.clone(),
                nome.clone(),
                categoria.clone(),
                descricao.clone(),
            ],
            valores: vec![l.entrada, l.saida, l.saldo],
            alerta: false,
        });
        export.push(LinhaExport::nova(vec![
            Valor::texto(data),
            Valor::texto(nome),
            Valor::texto(categoria),
            Valor::texto(descricao),
            Valor::Dinheiro(l.entrada),
            Valor::Dinheiro(l.saida),
            Valor::Dinheiro(l.saldo),
        ]));
    }
    linhas.push(Linha::Total(
        format!("Saldo previsto em {}", ate.formatar()),
        vec![entradas, saidas, fim],
    ));
    export.push(
        LinhaExport::nova(vec![
            Valor::texto(format!("Saldo previsto em {}", ate.formatar())),
            Valor::Vazio,
            Valor::Vazio,
            Valor::Vazio,
            Valor::Dinheiro(entradas),
            Valor::Dinheiro(saidas),
            Valor::Dinheiro(fim),
        ])
        .destaque(),
    );
    let subtitulo = format!("{} a {}", hoje.formatar(), ate.formatar());
    let mut busca_editada = estado.relatorios.busca.clone();
    filtros(ui, motor, &mut busca_editada, || Relatorio {
        titulo: "Fluxo de caixa previsto".to_owned(),
        subtitulo,
        tabelas: vec![Tabela {
            titulo: "Projeção".to_owned(),
            nota: Some(
                "Do saldo de hoje das contas, as contas a receber e a pagar em aberto pelo \
                 vencimento; as vencidas primeiro."
                    .to_owned(),
            ),
            colunas: vec![
                Coluna::texto("Vencimento", 1.6),
                Coluna::texto("Pessoa", 2.4),
                Coluna::texto("Categoria", 1.8),
                Coluna::texto("Descrição", 2.4),
                Coluna::numero("Entrada", 1.4),
                Coluna::numero("Saída", 1.4),
                Coluna::numero("Saldo", 1.6),
            ],
            linhas: export,
        }],
    });
    estado.relatorios.busca = busca_editada;
    grade(
        ui,
        "relatorio-previsto",
        vec![
            ColunaGrade::nova("Vencimento").largura(170.0),
            ColunaGrade::nova("Pessoa").largura(220.0),
            ColunaGrade::nova("Categoria").largura(170.0),
            ColunaGrade::nova("Descrição"),
            ColunaGrade::nova("Entrada").largura(130.0).numero(),
            ColunaGrade::nova("Saída").largura(130.0).numero(),
            ColunaGrade::nova("Saldo").largura(140.0).numero(),
        ],
        &linhas,
    );
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn situacao_e_data_por_extenso() {
        let hoje = Data::de_ymd(2026, 11, 5).unwrap();
        assert_eq!(situacao(hoje, hoje), "Vence hoje");
        assert_eq!(situacao(hoje.mais_dias(1), hoje), "Amanhã");
        assert_eq!(situacao(hoje.mais_dias(5), hoje), "Em 5 dias");
        assert_eq!(situacao(hoje.mais_dias(-1), hoje), "Vencida há 1 dia");
        assert_eq!(situacao(hoje.mais_dias(-12), hoje), "Vencida há 12 dias");
        assert_eq!(
            por_extenso(Data::de_ymd(2026, 5, 10).unwrap()),
            "10 de maio de 2026"
        );
        assert!(casa("", &["x"]));
        assert!(casa("ALUG", &["Imobiliária", "Aluguel da loja"]));
        assert!(!casa("energia", &["Imobiliária", "Aluguel"]));
    }
}
