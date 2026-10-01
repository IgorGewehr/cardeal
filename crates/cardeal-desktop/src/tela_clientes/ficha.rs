//! A ficha do cliente — o que o cadastro devolve em troca de ser preenchido: quantas vezes
//! veio, quando foi a última, quanto já gastou, o que deve, o que está em garantia, que
//! aparelhos já trouxe e as últimas OS (com clique para abrir).

use super::*;
use cardeal_kernel::{Data, Dinheiro, Fuso};
use mod_os::{EstadoOs, OrdemServico, OrdensDoCliente};

/// O relacionamento carregado ao abrir o cliente.
#[derive(Debug, Clone, Default)]
pub(super) struct Ficha {
    pub(super) ordens: Vec<OrdemServico>,
    pub(super) em_aberto: Dinheiro,
    pub(super) vencido: Dinheiro,
}

/// O que a ficha pede para a tela de OS fazer (a navegação é do `main`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PedidoParaOs {
    /// Abrir "Nova OS" com este cliente.
    Nova(Id),
    /// Abrir esta OS.
    Abrir(Id),
}

impl Ficha {
    /// Carrega as OS e o que o cliente deve (sem permissão, a parte fica vazia).
    pub(super) fn carregar(motor: &Motor, sessao: &Sessao, cliente: Id) -> Self {
        let ordens: Vec<OrdemServico> = motor
            .consultar(
                sessao,
                "os.ordens_do_cliente.v1",
                &OrdensDoCliente { cliente },
            )
            .unwrap_or_default();
        let hoje = Data::hoje(Fuso::BRASILIA);
        let titulos: Vec<mod_financeiro::ItemTituloEmAberto> = motor
            .consultar(
                sessao,
                "financeiro.titulos_a_receber_em_aberto.v1",
                &mod_financeiro::TitulosAReceberEmAberto,
            )
            .unwrap_or_default();
        let do_cliente = titulos.iter().filter(|t| {
            matches!(t.contraparte, Some(cardeal_ledger::Contraparte::Cliente(id)) if id == cliente)
        });
        let (mut em_aberto, mut vencido) = (Dinheiro::ZERO, Dinheiro::ZERO);
        for t in do_cliente {
            em_aberto += t.saldo();
            if t.vencimento < hoje {
                vencido += t.saldo();
            }
        }
        Self {
            ordens,
            em_aberto,
            vencido,
        }
    }

    fn gasto_total(&self) -> Dinheiro {
        self.ordens
            .iter()
            .filter(|o| o.estado == EstadoOs::Faturada)
            .fold(Dinheiro::ZERO, |acc, o| acc + o.valor_total)
    }

    fn em_garantia(&self, hoje: Data) -> usize {
        self.ordens
            .iter()
            .filter(|o| {
                o.estado == EstadoOs::Faturada
                    && o.garantia_dias > 0
                    && o.data_abertura.mais_dias(i32::from(o.garantia_dias)) >= hoje
            })
            .count()
    }

    /// Os aparelhos distintos que o cliente já trouxe (sem repetir por caixa/acento).
    pub(super) fn aparelhos(&self) -> Vec<String> {
        let mut vistos: Vec<String> = Vec::new();
        let mut saida = Vec::new();
        for o in &self.ordens {
            let chave = cardeal_kernel::texto::chave_busca(&o.equipamento);
            if !chave.is_empty() && !vistos.contains(&chave) {
                vistos.push(chave);
                saida.push(o.equipamento.clone());
            }
        }
        saida
    }
}

/// Desenha a ficha no topo do cliente aberto. Devolve o que pedir à tela de OS, se algo.
pub(super) fn secao_ficha(ui: &mut egui::Ui, ficha: &Ficha) -> Option<PedidoParaOs> {
    let hoje = Data::hoje(Fuso::BRASILIA);
    let ultima = ficha
        .ordens
        .iter()
        .map(|o| o.data_abertura)
        .max()
        .map_or_else(
            || "nunca veio".to_owned(),
            |d| format!("última em {}", d.formatar()),
        );
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("OS", ficha.ordens.len()).variacao(ultima),
        CartaoKpi::novo("Gasto total", ficha.gasto_total()).variacao("OS faturadas"),
        CartaoKpi::novo("Em aberto", ficha.em_aberto).variacao(if ficha.vencido.e_positivo() {
            format!("{} vencido", ficha.vencido.formatar_com_simbolo())
        } else {
            "nada vencido".to_owned()
        }),
        CartaoKpi::contagem("Em garantia", ficha.em_garantia(hoje)),
    ])
    .mostrar(ui);

    let aparelhos = ficha.aparelhos();
    if !aparelhos.is_empty() {
        ui.horizontal_wrapped(|ui| {
            ui.add(Rotulo::campo("Aparelhos"));
            for a in aparelhos.iter().take(8) {
                ui.add(Etiqueta::neutra(a.clone()));
            }
        });
        ui.add_space(Espaco::E8);
    }

    let mut pedido = None;
    if !ficha.ordens.is_empty() {
        SecaoExpansivel::nova(format!("Últimas OS ({})", ficha.ordens.len()))
            .aberta_por_padrao(true)
            .mostrar(ui, |ui| {
                for o in ficha.ordens.iter().take(8) {
                    ui.horizontal(|ui| {
                        if ui
                            .add(Botao::fantasma(format!("OS #{}", o.numero)).pequeno())
                            .clicked()
                        {
                            pedido = Some(PedidoParaOs::Abrir(o.id));
                        }
                        ui.add(Rotulo::interface(format!(
                            "{} · {}",
                            o.data_abertura.formatar(),
                            o.equipamento
                        )));
                        let (rotulo, tom) = crate::tela_os::estado_etiqueta(o.estado);
                        ui.add(Etiqueta::nova(rotulo, tom));
                        ui.add(ValorDinheiro::novo(o.valor_total).neutro());
                    });
                }
            });
    }
    ui.add_space(Espaco::E12);
    ui.add(Divisor::novo());
    ui.add_space(Espaco::E12);
    pedido
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::testes_comum::motor_de_teste;
    use mod_os::{
        AbrirOrdemServico, FaturarOrdemServico, FichaEntrada, ItemOrcamentoNovo, MontarOrcamentoOs,
        OrdemServicoAberta, OrdemServicoFaturada,
    };

    #[test]
    fn ficha_junta_os_gasto_em_aberto_e_aparelhos_do_cliente() {
        let t = motor_de_teste();
        let criado: PessoaCadastrada = t
            .motor
            .executar(
                &t.sessao,
                "clientes.criar_pessoa.v1",
                &CriarPessoa {
                    tipo: TipoPessoa::Fisica,
                    nome: "Maria Souza".to_owned(),
                    nome_fantasia: None,
                    papel_inicial: Papel::Cliente,
                    documento_tipo: None,
                    documento_numero: None,
                    data_nascimento: None,
                    endereco: None,
                    contato: None,
                },
            )
            .expect("cliente");
        let abrir = |aparelho: &str| -> Id {
            let r: OrdemServicoAberta = t
                .motor
                .executar(
                    &t.sessao,
                    "os.abrir_ordem_servico.v1",
                    &AbrirOrdemServico {
                        cliente: criado.pessoa,
                        equipamento: aparelho.to_owned(),
                        defeito_relatado: "x".to_owned(),
                        tecnico_responsavel: t.sessao.usuario(),
                        garantia_dias: 90,
                        ficha: FichaEntrada::default(),
                    },
                )
                .expect("OS");
            r.ordem_servico
        };
        let os = abrir("Samsung A52");
        let _ = abrir("samsung a52");
        let _ = abrir("Notebook Dell");
        let _: Id = t
            .motor
            .executar(
                &t.sessao,
                "os.montar_orcamento.v1",
                &MontarOrcamentoOs {
                    ordem_servico: os,
                    item: ItemOrcamentoNovo::MaoDeObra {
                        descricao: "Reparo".to_owned(),
                        valor: Dinheiro::reais(300),
                        tecnico: t.sessao.usuario(),
                        horas: None,
                    },
                },
            )
            .expect("mão de obra");
        let hoje = Data::hoje(Fuso::BRASILIA);
        let _: OrdemServicoFaturada = t
            .motor
            .executar(
                &t.sessao,
                "os.faturar_ordem_servico.v1",
                &FaturarOrdemServico {
                    ordem_servico: os,
                    parcelas: 1,
                    primeiro_vencimento: hoje.mais_dias(-2),
                    intervalo_dias: 0,
                    pago_no_ato: None,
                },
            )
            .expect("faturar a prazo");

        let ficha = Ficha::carregar(&t.motor, &t.sessao, criado.pessoa);
        assert_eq!(ficha.ordens.len(), 3);
        assert_eq!(ficha.gasto_total(), Dinheiro::reais(300));
        assert_eq!(ficha.em_aberto, Dinheiro::reais(300));
        assert_eq!(ficha.vencido, Dinheiro::reais(300));
        assert_eq!(ficha.em_garantia(hoje), 1);
        assert_eq!(ficha.aparelhos(), vec!["Notebook Dell", "samsung a52"]);
    }
}
