//! A aba **"Fornecedores"** das compras: de quem eu compro, quanto, com que frequência, o que está
//! em aberto (pedidos, atrasos, contas a pagar) — e a **ficha** de cada um: as notas, os pedidos e
//! o prazo real de entrega (do pedido até a nota).
//!
//! As contas são puras ([`resumir`], [`prazo_medio_de_entrega`]); a tela só junta o que os módulos
//! já respondem (notas de entrada, pedidos, títulos a pagar).

use super::*;
use cardeal_kernel::Dinheiro;
use cardeal_ledger::Contraparte;
use cardeal_ui::atoms::ValorDinheiro;
use cardeal_ui::molecules::CartaoKpi;
use cardeal_ui::organisms::FaixaKpi;
use mod_compras::pedido::ResumoPedido;
use mod_compras::{PedidoCompra, PedidoDeCompra, PedidosDeCompra};
use mod_financeiro::{ItemTituloEmAberto, TitulosAPagarEmAberto};

/// O que se sabe de um fornecedor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ResumoFornecedor {
    pub(super) fornecedor: Id,
    pub(super) nome: String,
    /// Notas confirmadas nos últimos 12 meses.
    pub(super) notas_12m: usize,
    /// O valor delas.
    pub(super) comprado_12m: Dinheiro,
    /// A última nota (qualquer estado).
    pub(super) ultima_compra: Option<Data>,
    /// Pedidos aguardando mercadoria.
    pub(super) pedidos_em_aberto: usize,
    /// Desses, com a entrega combinada vencida.
    pub(super) pedidos_atrasados: usize,
    /// O que falta pagar a ele.
    pub(super) a_pagar: Dinheiro,
}

/// Junta notas, pedidos e contas a pagar por fornecedor. Quem não tem movimento nenhum fica de
/// fora; ordem: quem mais vendeu para a empresa nos 12 meses primeiro.
pub(super) fn resumir(
    fornecedores: &[(Id, String)],
    notas: &[ItemNota],
    pedidos: &[ResumoPedido],
    titulos: &[ItemTituloEmAberto],
    hoje: Data,
) -> Vec<ResumoFornecedor> {
    let ha_um_ano = hoje.mais_dias(-365);
    let mut v: Vec<ResumoFornecedor> = fornecedores
        .iter()
        .map(|(id, nome)| {
            let deles: Vec<&ItemNota> = notas.iter().filter(|n| n.fornecedor == *id).collect();
            let no_ano: Vec<&&ItemNota> = deles
                .iter()
                .filter(|n| {
                    n.estado == EstadoNotaEntrada::Confirmada && n.data_emissao >= ha_um_ano
                })
                .collect();
            let abertos: Vec<&ResumoPedido> = pedidos
                .iter()
                .filter(|p| p.fornecedor == *id && p.estado.aguardando())
                .collect();
            ResumoFornecedor {
                fornecedor: *id,
                nome: nome.clone(),
                notas_12m: no_ano.len(),
                comprado_12m: no_ano.iter().map(|n| n.valor_total).sum(),
                ultima_compra: deles.iter().map(|n| n.data_emissao).max(),
                pedidos_em_aberto: abertos.len(),
                pedidos_atrasados: abertos.iter().filter(|p| p.atrasado(hoje)).count(),
                a_pagar: titulos
                    .iter()
                    .filter(|t| t.contraparte == Some(Contraparte::Fornecedor(*id)))
                    .map(|t| t.valor_original - t.valor_baixado)
                    .sum(),
            }
        })
        .filter(|r| r.ultima_compra.is_some() || r.pedidos_em_aberto > 0 || r.a_pagar.e_positivo())
        .collect();
    v.sort_by(|a, b| {
        b.comprado_12m
            .cmp(&a.comprado_12m)
            .then(a.nome.cmp(&b.nome))
    });
    v
}

/// O prazo médio real de entrega, em dias: do dia do pedido até a emissão da primeira nota dele.
/// `None` se nenhum pedido recebeu nota.
pub(super) fn prazo_medio_de_entrega(pedidos: &[PedidoCompra]) -> Option<i64> {
    let prazos: Vec<i64> = pedidos
        .iter()
        .filter_map(|p| {
            let primeira = p.notas.iter().map(|n| n.emissao).min()?;
            Some(i64::from(primeira.em_dias() - p.data.em_dias()).max(0))
        })
        .collect();
    if prazos.is_empty() {
        return None;
    }
    let n = i64::try_from(prazos.len()).unwrap_or(1);
    Some((prazos.iter().sum::<i64>() + n / 2) / n)
}

/// A ficha aberta.
pub(super) struct Ficha {
    resumo: ResumoFornecedor,
    pedidos: Vec<PedidoCompra>,
    prazo: Option<i64>,
}

/// O estado da aba.
#[derive(Default)]
pub(super) struct EstadoFornecedores {
    linhas: Vec<ResumoFornecedor>,
    busca: String,
    ficha: Option<Ficha>,
}

impl EstadoFornecedores {
    pub(super) fn carregar(
        &mut self,
        motor: &Motor,
        sessao: &Sessao,
        fornecedores: &HashMap<Id, String>,
        notas: &[ItemNota],
    ) {
        let pedidos: Vec<ResumoPedido> = motor
            .consultar(
                sessao,
                "compras.pedidos.v1",
                &PedidosDeCompra {
                    so_em_andamento: true,
                    fornecedor: None,
                },
            )
            .unwrap_or_default();
        let titulos: Vec<ItemTituloEmAberto> = motor
            .consultar(
                sessao,
                "financeiro.titulos_a_pagar_em_aberto.v1",
                &TitulosAPagarEmAberto,
            )
            .unwrap_or_default();
        let lista: Vec<(Id, String)> = fornecedores
            .iter()
            .map(|(id, nome)| (*id, nome.clone()))
            .collect();
        self.linhas = resumir(&lista, notas, &pedidos, &titulos, Data::hoje(FUSO));
    }

    #[cfg(test)]
    pub(super) fn linhas(&self) -> &[ResumoFornecedor] {
        &self.linhas
    }

    #[cfg(test)]
    pub(super) fn abrir_para_teste(&mut self, motor: &Motor, sessao: &Sessao, i: usize) {
        self.abrir(motor, sessao, i);
    }

    #[cfg(test)]
    pub(super) const fn ficha_aberta(&self) -> bool {
        self.ficha.is_some()
    }

    fn abrir(&mut self, motor: &Motor, sessao: &Sessao, i: usize) {
        let Some(resumo) = self.linhas.get(i).cloned() else {
            return;
        };
        let resumos: Vec<ResumoPedido> = motor
            .consultar(
                sessao,
                "compras.pedidos.v1",
                &PedidosDeCompra {
                    so_em_andamento: false,
                    fornecedor: Some(resumo.fornecedor),
                },
            )
            .unwrap_or_default();
        let pedidos: Vec<PedidoCompra> = resumos
            .iter()
            .take(50)
            .filter_map(|r| {
                motor
                    .consultar(
                        sessao,
                        "compras.pedido.v1",
                        &PedidoDeCompra { pedido: r.pedido },
                    )
                    .ok()
                    .flatten()
            })
            .collect();
        let prazo = prazo_medio_de_entrega(&pedidos);
        self.ficha = Some(Ficha {
            resumo,
            pedidos,
            prazo,
        });
    }
}

pub(super) fn corpo(
    ui: &mut egui::Ui,
    motor: &Motor,
    sessao: &Sessao,
    notas: &[ItemNota],
    e: &mut EstadoFornecedores,
) {
    let total: Dinheiro = e.linhas.iter().map(|l| l.comprado_12m).sum();
    let atrasados: usize = e.linhas.iter().map(|l| l.pedidos_atrasados).sum();
    let a_pagar: Dinheiro = e.linhas.iter().map(|l| l.a_pagar).sum();
    FaixaKpi::nova(vec![
        CartaoKpi::novo("Comprado em 12 meses", total),
        CartaoKpi::contagem("Fornecedores ativos", e.linhas.len()),
        CartaoKpi::contagem("Pedidos atrasados", atrasados).tom(if atrasados > 0 {
            Tom::Atencao
        } else {
            Tom::Neutro
        }),
        CartaoKpi::novo("A pagar a fornecedores", a_pagar),
    ])
    .mostrar(ui);
    ui.add_space(Espaco::E12);
    BarraFiltros::nova(&mut e.busca)
        .marcador("Buscar fornecedor")
        .mostrar(ui);
    let termo = e.busca.trim().to_owned();
    let visiveis: Vec<usize> = (0..e.linhas.len())
        .filter(|&i| {
            termo.is_empty() || cardeal_kernel::texto::casa_por_palavras(&e.linhas[i].nome, &termo)
        })
        .collect();
    let mut abrir = None;
    Grade::nova(vec![
        ColunaGrade::nova("Fornecedor"),
        ColunaGrade::nova("Comprado (12 meses)")
            .largura(150.0)
            .numero(),
        ColunaGrade::nova("Notas").largura(70.0).numero(),
        ColunaGrade::nova("Última compra").largura(120.0),
        ColunaGrade::nova("Pedidos em aberto").largura(150.0),
        ColunaGrade::nova("A pagar").largura(120.0).numero(),
        ColunaGrade::nova("").largura(80.0),
    ])
    .com_acoes()
    .id_salt("compras-fornecedores")
    .vazio("Nenhum fornecedor com compra, pedido ou conta em aberto.")
    .mostrar(ui, visiveis.len(), |k, row| {
        let i = visiveis[k];
        let l = &e.linhas[i];
        row.col(|ui| {
            ui.add(Rotulo::interface(l.nome.clone()));
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(l.comprado_12m).neutro());
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(l.notas_12m.to_string()));
        });
        row.col(|ui| {
            ui.add(Rotulo::interface(
                l.ultima_compra.map(Data::formatar).unwrap_or_default(),
            ));
        });
        row.col(|ui| {
            ui.horizontal(|ui| {
                if l.pedidos_em_aberto > 0 {
                    ui.add(Rotulo::interface(l.pedidos_em_aberto.to_string()));
                }
                if l.pedidos_atrasados > 0 {
                    ui.add(Etiqueta::atencao(format!(
                        "{} atrasado(s)",
                        l.pedidos_atrasados
                    )));
                }
            });
        });
        row.col(|ui| {
            ui.add(ValorDinheiro::novo(l.a_pagar).neutro());
        });
        row.col(|ui| {
            if ui.add(Botao::fantasma("Ficha").pequeno()).clicked() {
                abrir = Some(i);
            }
        });
    });
    if let Some(i) = abrir {
        e.abrir(motor, sessao, i);
    }
    dialogo_ficha(ui.ctx(), notas, e);
}

fn dialogo_ficha(ctx: &egui::Context, notas: &[ItemNota], e: &mut EstadoFornecedores) {
    let Some(titulo) = e
        .ficha
        .as_ref()
        .map(|f| format!("Fornecedor · {}", f.resumo.nome))
    else {
        return;
    };
    let fechar = Dialogo::nova(titulo).largura(820.0).mostrar(
        ctx,
        e,
        |ui, e| {
            let Some(f) = &e.ficha else {
                return;
            };
            let r = &f.resumo;
            let deles: Vec<&ItemNota> = notas
                .iter()
                .filter(|n| n.fornecedor == r.fornecedor)
                .collect();
            FaixaKpi::nova(vec![
                CartaoKpi::novo("Comprado em 12 meses", r.comprado_12m)
                    .variacao(format!("{} nota(s)", r.notas_12m)),
                CartaoKpi::contagem(
                    "Prazo real de entrega",
                    f.prazo
                        .map_or_else(|| "—".to_owned(), |d| format!("{d} dia(s)")),
                )
                .variacao("do pedido até a nota"),
                CartaoKpi::contagem("Pedidos em aberto", r.pedidos_em_aberto).variacao(
                    if r.pedidos_atrasados > 0 {
                        format!("{} atrasado(s)", r.pedidos_atrasados)
                    } else {
                        "nenhum atrasado".to_owned()
                    },
                ),
                CartaoKpi::novo("A pagar", r.a_pagar),
            ])
            .mostrar(ui);
            ui.add_space(Espaco::E12);
            ui.add(Rotulo::titulo_secao("Pedidos"));
            Grade::nova(vec![
                ColunaGrade::nova("Pedido").largura(90.0),
                ColunaGrade::nova("Data").largura(100.0),
                ColunaGrade::nova("Situação").largura(170.0),
                ColunaGrade::nova("Total").largura(120.0).numero(),
                ColunaGrade::nova("Notas"),
            ])
            .id_salt("ficha-pedidos")
            .vazio("Nenhum pedido.")
            .mostrar(ui, f.pedidos.len(), |i, row| {
                let p = &f.pedidos[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(format!("{:04}", p.numero)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(p.data.formatar()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(p.estado.nome()));
                });
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(p.total()).neutro());
                });
                row.col(|ui| {
                    ui.add(Rotulo::apoio(
                        p.notas
                            .iter()
                            .map(|n| format!("{} ({})", n.numero, n.emissao.formatar_curta()))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ));
                });
            });
            ui.add_space(Espaco::E12);
            ui.add(Rotulo::titulo_secao("Notas de entrada"));
            Grade::nova(vec![
                ColunaGrade::nova("Nota").largura(120.0),
                ColunaGrade::nova("Emissão").largura(110.0),
                ColunaGrade::nova("Situação").largura(160.0),
                ColunaGrade::nova("Valor").largura(120.0).numero(),
            ])
            .id_salt("ficha-notas")
            .vazio("Nenhuma nota.")
            .mostrar(ui, deles.len().min(100), |i, row| {
                let n = deles[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(format!("{}/{}", n.numero, n.serie)));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(n.data_emissao.formatar()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(situacao_da_nota(n.estado)));
                });
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(n.valor_total).neutro());
                });
            });
        },
        |ui, e| {
            if ui.add(Botao::secundario("Fechar")).clicked() {
                e.ficha = None;
            }
        },
    );
    if fechar {
        e.ficha = None;
    }
}

/// A situação da nota, para gente.
const fn situacao_da_nota(e: EstadoNotaEntrada) -> &'static str {
    match e {
        EstadoNotaEntrada::AConferir => "a conferir",
        EstadoNotaEntrada::Conferida => "conferida",
        EstadoNotaEntrada::Confirmada => "confirmada",
        EstadoNotaEntrada::Devolvida => "devolvida",
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use mod_compras::pedido::NotaDoPedido;
    use mod_compras::EstadoPedido;

    fn nota(fornecedor: Id, dia: i32, reais: i64, estado: EstadoNotaEntrada) -> ItemNota {
        ItemNota {
            nota: Id::novo(),
            fornecedor,
            numero: "1".into(),
            serie: "1".into(),
            data_emissao: Data::de_dias(dia),
            valor_total: Dinheiro::reais(reais),
            estado,
            itens: 1,
            alerta: None,
        }
    }

    fn resumo_pedido(fornecedor: Id, previsao: Option<i32>, estado: EstadoPedido) -> ResumoPedido {
        ResumoPedido {
            pedido: Id::novo(),
            numero: 1,
            fornecedor,
            fornecedor_nome: String::new(),
            data: Data::de_dias(1000),
            previsao_entrega: previsao.map(Data::de_dias),
            estado,
            total: Dinheiro::reais(10),
            recebido: Dinheiro::ZERO,
        }
    }

    #[test]
    fn resume_compras_do_ano_pedidos_atrasados_e_ordena_por_valor() {
        let (alfa, beta, parado) = (Id::novo(), Id::novo(), Id::novo());
        let hoje = Data::de_dias(1000);
        let notas = vec![
            nota(alfa, 990, 100, EstadoNotaEntrada::Confirmada),
            nota(alfa, 500, 900, EstadoNotaEntrada::Confirmada), // mais de um ano
            nota(beta, 995, 300, EstadoNotaEntrada::Confirmada),
            nota(beta, 999, 50, EstadoNotaEntrada::AConferir), // não confirmada
        ];
        let pedidos = vec![
            resumo_pedido(alfa, Some(990), EstadoPedido::Enviado), // atrasado
            resumo_pedido(alfa, Some(1010), EstadoPedido::Enviado),
            resumo_pedido(beta, None, EstadoPedido::Recebido), // terminou
        ];
        let r = resumir(
            &[
                (alfa, "Alfa".into()),
                (beta, "Beta".into()),
                (parado, "Sem nada".into()),
            ],
            &notas,
            &pedidos,
            &[],
            hoje,
        );
        assert_eq!(r.len(), 2, "quem não tem movimento fica de fora");
        assert_eq!(r[0].nome, "Beta", "Beta comprou R$ 300 no ano; Alfa R$ 100");
        assert_eq!(r[0].comprado_12m, Dinheiro::reais(300));
        assert_eq!(r[0].ultima_compra, Some(Data::de_dias(999)));
        assert_eq!(r[1].notas_12m, 1);
        assert_eq!(r[1].pedidos_em_aberto, 2);
        assert_eq!(r[1].pedidos_atrasados, 1);
    }

    #[test]
    fn prazo_medio_vai_do_pedido_a_primeira_nota() {
        let pedido = |data: i32, notas: &[i32]| PedidoCompra {
            id: Id::novo(),
            numero: 1,
            fornecedor: Id::novo(),
            fornecedor_nome: String::new(),
            cotacao: None,
            data: Data::de_dias(data),
            previsao_entrega: None,
            condicao_pagamento: String::new(),
            frete: Dinheiro::ZERO,
            observacao: String::new(),
            estado: EstadoPedido::Recebido,
            motivo: None,
            itens: Vec::new(),
            notas: notas
                .iter()
                .map(|d| NotaDoPedido {
                    nota: Id::novo(),
                    numero: "1".into(),
                    serie: "1".into(),
                    emissao: Data::de_dias(*d),
                    estado: EstadoNotaEntrada::Confirmada,
                    total: Dinheiro::ZERO,
                })
                .collect(),
        };
        assert_eq!(prazo_medio_de_entrega(&[]), None);
        assert_eq!(
            prazo_medio_de_entrega(&[
                pedido(100, &[104, 110]),
                pedido(200, &[206]),
                pedido(300, &[])
            ]),
            Some(5),
            "(4 + 6) / 2; o pedido sem nota não entra"
        );
    }
}
