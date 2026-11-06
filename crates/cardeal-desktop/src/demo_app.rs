//! Conferência visual do app inteiro (só com `--features demo`): banco temporário com dados
//! de exemplo, login automático, a área/diálogo pedidos e um PNG — o equivalente da galeria
//! para telas que dependem do motor.
//!
//! ```text
//! CARDEAL_DEMO=os CARDEAL_DEMO_PNG=/tmp/os.png cargo run -p cardeal-desktop --features demo
//! ```
//!
//! Cenas: `os`, `os-detalhe`, `os-nova`, `os-faturar`, `financeiro`, `financeiro-receber`,
//! `financeiro-baixa`, `financeiro-lancar`, `financeiro-custos`, `financeiro-pagar`, `financeiro-fluxo`, `financeiro-bancos`,
//! `financeiro-dre`, `estoque-estrutura`, `estoque-precos`, `estoque-lote`, `financeiro-pessoal`, `financeiro-cartoes`, `financeiro-pessoal-novo`, `login`, `login-servidor`. Sem `CARDEAL_DEMO_PNG` a janela fica aberta para mexer.

use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Preco, Quantidade};
use eframe::egui;
use mod_clientes::{ContatoInicial, CriarPessoa, Papel, PessoaCadastrada, TipoContato, TipoPessoa};
use mod_estoque::{
    CriarGrupoProduto, CriarLocal, CriarProduto, CriarUnidade, EntradaRegistrada,
    GrupoProdutoCriado, LocalCriado, ProdutoCriado, RegistrarEntrada, TipoLocal, UnidadeCriada,
};
use mod_os::{
    AbrirOrdemServico, AprovarOrcamentoOs, ConcluirExecucao, EnviarParaAprovacao, FichaEntrada,
    IniciarExecucao, ItemOrcamentoNovo, MontarOrcamentoOs, OrdemServicoAberta,
};

/// Quadros até o layout e as animações assentarem antes da captura.
const QUADROS_ANTES_DA_CAPTURA: u32 = 20;

/// O que a demo pediu, lido do ambiente.
pub struct Captura {
    pub cena: String,
    destino: Option<std::path::PathBuf>,
    _dir: tempfile::TempDir,
    pub base: std::path::PathBuf,
    quadros: u32,
    pediu: bool,
    maximizou: bool,
    /// Gestos de `CARDEAL_DEMO_GESTOS` (clique, texto, tecla), cada um no quadro em que entra —
    /// para capturar um menu aberto ou uma busca digitada sem mexer no mouse de ninguém.
    gestos: Vec<(u32, egui::Event)>,
    quadros_de_entrada: u32,
}

/// Lê `CARDEAL_DEMO_GESTOS` ("clique:753,500;texto:as;tecla:enter", posições em pontos lógicos)
/// como eventos de entrada, um gesto a cada 3 quadros a partir do 6º.
fn gestos_do_ambiente() -> Vec<(u32, egui::Event)> {
    let Ok(roteiro) = std::env::var("CARDEAL_DEMO_GESTOS") else {
        return Vec::new();
    };
    let mut eventos = Vec::new();
    let mut quadro = 6;
    for gesto in roteiro.split(';').map(str::trim).filter(|g| !g.is_empty()) {
        let (tipo, valor) = gesto.split_once(':').unwrap_or((gesto, ""));
        match tipo {
            "clique" => {
                let mut xy = valor
                    .split(',')
                    .filter_map(|n| n.trim().parse::<f32>().ok());
                let (Some(x), Some(y)) = (xy.next(), xy.next()) else {
                    continue;
                };
                let pos = egui::pos2(x, y);
                let botao = |pressionado| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: pressionado,
                    modifiers: egui::Modifiers::NONE,
                };
                eventos.push((quadro, egui::Event::PointerMoved(pos)));
                eventos.push((quadro, botao(true)));
                eventos.push((quadro + 1, botao(false)));
            }
            "texto" => eventos.push((quadro, egui::Event::Text(valor.to_owned()))),
            "tecla" => {
                let tecla = match valor {
                    "enter" => egui::Key::Enter,
                    "baixo" => egui::Key::ArrowDown,
                    "cima" => egui::Key::ArrowUp,
                    "esc" => egui::Key::Escape,
                    _ => continue,
                };
                eventos.push((
                    quadro,
                    egui::Event::Key {
                        key: tecla,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ));
            }
            _ => continue,
        }
        quadro += 3;
    }
    eventos
}

impl Captura {
    /// `Some` quando `CARDEAL_DEMO` está definido, ou com `--demo [cena]` na linha de comando
    /// (a cena padrão é `financeiro`) — o jeito de um atalho do Windows abrir a demonstração,
    /// já que atalho não define variável de ambiente.
    pub fn do_ambiente() -> Option<Self> {
        let cena = std::env::var("CARDEAL_DEMO").ok().or_else(|| {
            let mut args = std::env::args().skip_while(|a| a != "--demo").skip(1);
            std::env::args()
                .any(|a| a == "--demo")
                .then(|| args.next().unwrap_or_else(|| "financeiro".to_owned()))
        })?;
        let dir = tempfile::tempdir().expect("diretório temporário");
        let base = dir.path().join("demo.db");
        Some(Self {
            cena,
            destino: std::env::var_os("CARDEAL_DEMO_PNG").map(Into::into),
            _dir: dir,
            base,
            quadros: 0,
            pediu: false,
            maximizou: false,
            gestos: gestos_do_ambiente(),
            quadros_de_entrada: 0,
        })
    }

    /// Põe na entrada deste quadro os gestos marcados para ele (`CARDEAL_DEMO_GESTOS`).
    pub fn injetar(&mut self, entrada: &mut egui::RawInput) {
        self.quadros_de_entrada += 1;
        let agora = self.quadros_de_entrada;
        entrada.events.extend(
            self.gestos
                .iter()
                .filter(|(q, _)| *q == agora)
                .map(|(_, e)| e.clone()),
        );
    }

    /// Maximiza a janela no primeiro quadro (a demo é para mostrar: na tela cheia do notebook,
    /// e a captura sai do tamanho que a plateia vê); depois conta quadros, pede a captura e grava
    /// o PNG, e fecha a janela.
    pub fn quadro(&mut self, ctx: &egui::Context) {
        if !self.maximizou {
            self.maximizou = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            // `CARDEAL_DEMO_ZOOM=0.6`: cabe mais tela na captura (para conferir o pé das telas).
            if let Some(z) = std::env::var("CARDEAL_DEMO_ZOOM")
                .ok()
                .and_then(|z| z.parse::<f32>().ok())
            {
                ctx.set_zoom_factor(z);
            }
        }
        let Some(destino) = &self.destino else { return };
        self.quadros += 1;
        if self.quadros >= QUADROS_ANTES_DA_CAPTURA && !self.pediu {
            self.pediu = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
        }
        let img = ctx.input(|i| {
            i.events.iter().find_map(|e| match e {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(img) = img {
            let [w, h] = img.size;
            image::save_buffer(
                destino,
                img.as_raw(),
                u32::try_from(w).unwrap_or(0),
                u32::try_from(h).unwrap_or(0),
                image::ColorType::Rgba8,
            )
            .expect("gravando o PNG");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }
}

/// Configura a empresa, faz login e cria os dados de exemplo. Devolve a sessão do admin.
pub fn semear(motor: &Motor) -> Sessao {
    motor
        .configurar_inicial(
            "Assistência Demo",
            "11.222.333/0001-81",
            "demo",
            "Técnico Demo",
            "senha-forte-123",
        )
        .expect("configuração inicial");
    let s = motor.autenticar("demo", "senha-forte-123").expect("login");
    let cliente = |nome: &str, tel: &str| -> Id {
        let r: PessoaCadastrada = motor
            .executar(
                &s,
                "clientes.criar_pessoa.v1",
                &CriarPessoa {
                    tipo: TipoPessoa::Fisica,
                    nome: nome.to_owned(),
                    nome_fantasia: None,
                    papel_inicial: Papel::Cliente,
                    documento_tipo: None,
                    documento_numero: None,
                    data_nascimento: None,
                    endereco: None,
                    contato: Some(ContatoInicial {
                        tipo: TipoContato::Whatsapp,
                        valor: tel.to_owned(),
                    }),
                },
            )
            .expect("cliente");
        r.pessoa
    };
    let maria = cliente("Maria Souza", "31988887777");
    let joao = cliente("João Pereira", "31977776666");
    let oficina = cliente("Oficina do Zé", "31966665555");

    let grupo: GrupoProdutoCriado = motor
        .executar(
            &s,
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: "PECAS".to_owned(),
                nome: "Peças".to_owned(),
                pai: None,
            },
        )
        .expect("grupo");
    // Estrutura mercadológica de exemplo (3 níveis) com margem e perfil fiscal, e parâmetros
    // de preço — alimentam as cenas `estoque-estrutura` e `estoque-precos`.
    let telas: GrupoProdutoCriado = motor
        .executar(
            &s,
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: "TELAS".to_owned(),
                nome: "Telas".to_owned(),
                pai: Some(grupo.grupo_produto),
            },
        )
        .expect("subgrupo");
    let _: GrupoProdutoCriado = motor
        .executar(
            &s,
            "estoque.criar_grupo_produto.v1",
            &CriarGrupoProduto {
                codigo: "CEL".to_owned(),
                nome: "Celulares".to_owned(),
                pai: Some(telas.grupo_produto),
            },
        )
        .expect("categoria");
    motor
        .executar(
            &s,
            "estoque.definir_margem_do_grupo.v1",
            &mod_estoque::DefinirMargemDoGrupo {
                grupo: grupo.grupo_produto,
                margem: Some(cardeal_kernel::Percentual::pontos(40)),
            },
        )
        .expect("margem");
    let mut parametros = mod_estoque::ParametrosPreco::padrao_para("SimplesNacional");
    parametros.custo_operacional = cardeal_kernel::Percentual::pontos(20);
    parametros.custo_operacional_derivado = false;
    motor
        .executar(
            &s,
            "estoque.definir_parametros_preco.v1",
            &mod_estoque::DefinirParametrosPreco { parametros },
        )
        .expect("parâmetros de preço");
    let unidade: UnidadeCriada = motor
        .executar(
            &s,
            "estoque.criar_unidade.v1",
            &CriarUnidade {
                sigla: "UN".to_owned(),
                nome: "Unidade".to_owned(),
                fracionavel: false,
            },
        )
        .expect("unidade");
    let tela: ProdutoCriado = motor
        .executar(
            &s,
            "estoque.criar_produto.v1",
            &CriarProduto {
                grupo_produto: telas.grupo_produto,
                nome: "Tela Samsung A52 original".to_owned(),
                ncm: "85177099".to_owned(),
                unidade_padrao: unidade.unidade,
                codigo_barras: None,
                detalhes_tecnicos: None,
            },
        )
        .expect("produto");
    let local: LocalCriado = motor
        .executar(
            &s,
            "estoque.criar_local.v1",
            &CriarLocal {
                nome: "Bancada".to_owned(),
                tipo: TipoLocal::Deposito,
            },
        )
        .expect("local");
    let _: EntradaRegistrada = motor
        .executar(
            &s,
            "estoque.registrar_entrada.v1",
            &RegistrarEntrada {
                produto: tela.produto,
                local: local.local,
                quantidade: Quantidade::unidades(3),
                custo_unitario: Preco::reais(180),
            },
        )
        .expect("entrada");

    let hoje = Data::hoje(Fuso::BRASILIA);
    let abrir = |cliente: Id, aparelho: &str, defeito: &str, ficha: FichaEntrada| -> Id {
        let r: OrdemServicoAberta = motor
            .executar(
                &s,
                "os.abrir_ordem_servico.v1",
                &AbrirOrdemServico {
                    cliente,
                    equipamento: aparelho.to_owned(),
                    defeito_relatado: defeito.to_owned(),
                    tecnico_responsavel: s.usuario(),
                    garantia_dias: 90,
                    ficha,
                },
            )
            .expect("OS");
        r.ordem_servico
    };
    let orcar = |os: Id, reais: i64, com_peca: bool| {
        let _: Id = motor
            .executar(
                &s,
                "os.montar_orcamento.v1",
                &MontarOrcamentoOs {
                    ordem_servico: os,
                    item: ItemOrcamentoNovo::MaoDeObra {
                        descricao: "Diagnóstico e reparo".to_owned(),
                        valor: Dinheiro::reais(reais),
                        tecnico: s.usuario(),
                        horas: None,
                    },
                },
            )
            .expect("mão de obra");
        if com_peca {
            let _: Id = motor
                .executar(
                    &s,
                    "os.montar_orcamento.v1",
                    &MontarOrcamentoOs {
                        ordem_servico: os,
                        item: ItemOrcamentoNovo::Peca {
                            produto: tela.produto,
                            quantidade: Quantidade::unidades(1),
                            preco_unitario: Preco::reais(320),
                        },
                    },
                )
                .expect("peça");
        }
    };
    let passo = |nome: &str, os: Id| {
        let r: Result<(), _> = match nome {
            "enviar" => motor.executar(
                &s,
                "os.enviar_para_aprovacao.v1",
                &EnviarParaAprovacao { ordem_servico: os },
            ),
            "aprovar" => motor.executar(
                &s,
                "os.aprovar_orcamento.v1",
                &AprovarOrcamentoOs {
                    ordem_servico: os,
                    identificacao_aprovador: "Cliente, por WhatsApp".to_owned(),
                },
            ),
            "executar" => motor.executar(
                &s,
                "os.iniciar_execucao.v1",
                &IniciarExecucao { ordem_servico: os },
            ),
            _ => motor.executar(
                &s,
                "os.concluir_execucao.v1",
                &ConcluirExecucao { ordem_servico: os },
            ),
        };
        r.expect(nome);
    };

    let ficha = |dias: Option<i32>, serie: &str, acess: &str| FichaEntrada {
        previsao_entrega: dias.map(|d| hoje.mais_dias(d)),
        numero_serie: serie.to_owned(),
        acessorios: acess.to_owned(),
    };
    // Um histórico: o mesmo aparelho já passou por aqui.
    let antiga = abrir(
        maria,
        "Samsung Galaxy A52",
        "Bateria estufada",
        ficha(None, "", ""),
    );
    orcar(antiga, 150, false);
    let _ = motor
        .executar::<_>(
            &s,
            "os.faturar_ordem_servico.v1",
            &mod_os::FaturarOrdemServico {
                ordem_servico: antiga,
                parcelas: 1,
                primeiro_vencimento: hoje,
                intervalo_dias: 0,
                pago_no_ato: Some(mod_os::PagamentoNoAto {
                    meio_pagamento: mod_financeiro::MeioPagamento::Pix,
                    conta_destino: None,
                }),
            },
        )
        .map(|_: mod_os::OrdemServicoFaturada| ());

    let a52 = abrir(
        maria,
        "Samsung Galaxy A52",
        "Tela trincada, touch falhando",
        ficha(
            Some(-1),
            "IMEI 356789104455221",
            "Com capinha, sem carregador",
        ),
    );
    orcar(a52, 90, true);
    passo("enviar", a52);

    let notebook = abrir(
        joao,
        "Notebook Dell Inspiron 15",
        "Não liga depois de queda de energia",
        ficha(Some(3), "SN 7XK92L3", "Com carregador original"),
    );
    orcar(notebook, 250, false);
    passo("enviar", notebook);
    passo("aprovar", notebook);
    passo("executar", notebook);

    let furadeira = abrir(
        oficina,
        "Furadeira Bosch GSB 13 RE",
        "Faiscando no motor",
        ficha(Some(0), "", "Maleta com brocas"),
    );
    orcar(furadeira, 120, false);
    passo("enviar", furadeira);
    passo("aprovar", furadeira);
    passo("executar", furadeira);
    passo("concluir", furadeira);

    let iphone = abrir(joao, "iPhone 11", "Não carrega", ficha(Some(5), "", ""));
    let peca_nova = |os: Id, nome: &str, reais: i64| -> Id {
        motor
            .executar(
                &s,
                "os.montar_orcamento.v1",
                &MontarOrcamentoOs {
                    ordem_servico: os,
                    item: ItemOrcamentoNovo::PecaNova {
                        nome: nome.to_owned(),
                        quantidade: Quantidade::unidades(1),
                        preco_unitario: Preco::reais(reais),
                    },
                },
            )
            .expect("peça nova")
    };
    let conector = peca_nova(iphone, "Conector de carga Lightning", 90);
    let (): () = motor
        .executar(
            &s,
            "os.encomendar_peca.v1",
            &mod_os::EncomendarPeca {
                ordem_servico: iphone,
                item_peca: conector,
                fornecedor: "Distribuidora Centro".to_owned(),
                custo_previsto: Some(Preco::reais(35)),
                previsao_chegada: Some(hoje.mais_dias(2)),
            },
        )
        .expect("encomendar");
    let _ = peca_nova(notebook, "Fonte 65W Dell original", 180);

    // Contas a receber a prazo para a aba do financeiro.
    for (reais, dias) in [(480, -3), (1_200, 10), (350, 25)] {
        let _: mod_financeiro::TituloAReceberLancado = motor
            .executar(
                &s,
                "financeiro.lancar_titulo_a_receber.v1",
                &mod_financeiro::LancarTituloAReceber {
                    cliente: Some(joao),
                    valor_total: Dinheiro::reais(reais),
                    emissao: hoje.mais_dias(-20),
                    parcelas: 1,
                    primeiro_vencimento: hoje.mais_dias(dias),
                    intervalo_dias: 0,
                    observacao: None,
                    categoria: None,
                    quitado_agora: None,
                },
            )
            .expect("título");
    }
    semear_ano_financeiro(motor, &s, hoje);
    semear_pessoal(motor, &s, hoje);
    for nome in ["Nubank", "Banco do Brasil"] {
        let _: mod_financeiro::ContaBancariaCriada = motor
            .executar(
                &s,
                "financeiro.criar_conta_bancaria.v1",
                &mod_financeiro::CriarContaBancaria {
                    nome: nome.to_owned(),
                },
            )
            .expect("conta");
    }
    s
}

/// Um ano de caixa no plano da gestão financeira (o modelo com a árvore do controle por
/// categoria): de janeiro até hoje, as vendas, os serviços e as despesas do mês pagos e
/// recebidos; neste mês, o que já venceu pago (menos a energia, que fica atrasada) e o resto em
/// aberto; e os dois meses seguintes a vencer. Mais o orçamento do ano — para o DFC mês a mês, o
/// orçado × realizado, os relatórios e a visão geral terem o que mostrar.
fn semear_ano_financeiro(motor: &Motor, s: &Sessao, hoje: Data) {
    use mod_financeiro::orcamento::DefinirOrcamentoDoAno;

    let _: mod_financeiro::ModeloAplicado = motor
        .executar(
            s,
            "financeiro.aplicar_modelo_de_plano.v1",
            &mod_financeiro::AplicarModeloDePlano {
                modelo: cardeal_ledger::ModeloDePlano::GestaoFinanceira,
            },
        )
        .expect("modelo de plano");
    let plano: Vec<mod_financeiro::NoPlano> = motor
        .consultar(
            s,
            "financeiro.plano_de_contas.v1",
            &mod_financeiro::PlanoDeContas,
        )
        .expect("plano");
    let conta = |codigo: &str| plano.iter().find(|n| n.codigo == codigo).map(|n| n.id);
    // O banco começa o ano com dinheiro (o saldo de implantação em 31/12), senão o caixa do
    // DFC nasce negativo e a saúde financeira abre "em risco".
    let _: mod_financeiro::SaldoInicialDefinido = motor
        .executar(
            s,
            "financeiro.definir_saldo_inicial.v1",
            &mod_financeiro::DefinirSaldoInicial {
                conta: conta("1.1.02").expect("conta Bancos"),
                saldo: Dinheiro::reais(38_000),
                data: hoje.inicio_do_ano().mais_dias(-1),
            },
        )
        .expect("saldo inicial");
    let cats: Vec<mod_financeiro::CategoriaFinanceira> = motor
        .consultar(s, "financeiro.categorias.v1", &mod_financeiro::Categorias)
        .expect("categorias");
    let cat = |nome: &str| cats.iter().find(|c| c.nome == nome).map(|c| c.id);
    // O que passou pelo banco (Pix), para o extrato da conciliação no fim.
    let pelo_banco = std::cell::RefCell::new(Vec::<(Data, String, String)>::new());
    let lancar =
        |receber: bool, categoria: &str, descricao: &str, reais: i64, venc: Data, pago: bool| {
            let quitado = pago.then_some(mod_financeiro::QuitadoAgora {
                meio_pagamento: mod_financeiro::MeioPagamento::Pix,
                conta: None,
            });
            if pago {
                let (texto, sinal) = if receber {
                    ("PIX RECEBIDO", "")
                } else {
                    ("PIX ENVIADO", "-")
                };
                pelo_banco.borrow_mut().push((
                    venc,
                    format!("{texto} {}", descricao.to_uppercase()),
                    format!("{sinal}{reais},00"),
                ));
            }
            if receber {
                let _: mod_financeiro::TituloAReceberLancado = motor
                    .executar(
                        s,
                        "financeiro.lancar_titulo_a_receber.v1",
                        &mod_financeiro::LancarTituloAReceber {
                            cliente: None,
                            valor_total: Dinheiro::reais(reais),
                            emissao: venc,
                            parcelas: 1,
                            primeiro_vencimento: venc,
                            intervalo_dias: 0,
                            observacao: Some(descricao.to_owned()),
                            categoria: cat(categoria),
                            quitado_agora: quitado,
                        },
                    )
                    .expect("receita");
            } else {
                let _: mod_financeiro::TituloAPagarLancado = motor
                    .executar(
                        s,
                        "financeiro.lancar_titulo_a_pagar.v1",
                        &mod_financeiro::LancarTituloAPagar {
                            fornecedor: None,
                            valor_total: Dinheiro::reais(reais),
                            emissao: venc,
                            parcelas: 1,
                            primeiro_vencimento: venc,
                            intervalo_dias: 0,
                            observacao: Some(descricao.to_owned()),
                            categoria: cat(categoria),
                            quitado_agora: quitado,
                        },
                    )
                    .expect("despesa");
            }
        };
    let este_mes = hoje.inicio_do_mes();
    let mut mes = hoje.inicio_do_ano();
    let mut i: i64 = 0;
    while mes <= este_mes.mais_meses(2) {
        let dia = |d: i32| mes.mais_dias(d - 1).min(mes.fim_do_mes());
        let passado = mes < este_mes;
        let pago = |d: i32| passado || (mes == este_mes && dia(d) < hoje);
        // Margem de uns 10–20% ao mês: uma empresa saudável, com meses melhores e piores.
        let vendas = 24_000 + (i * 1_700) % 6_000;
        let servicos = 8_000 + (i * 450) % 2_000;
        // As vendas entram toda semana e os serviços duas vezes por mês — como numa loja de
        // verdade, e para o saldo do mês não abrir negativo nos primeiros dias.
        for d in [1, 8, 15, 22] {
            lancar(
                true,
                "Venda de mercadorias",
                "Vendas da semana",
                vendas / 4,
                dia(d),
                pago(d),
            );
        }
        for d in [3, 18] {
            lancar(
                true,
                "Receita de serviços",
                "Serviços da quinzena",
                servicos / 2,
                dia(d),
                pago(d),
            );
        }
        for (categoria, descricao, reais, d) in [
            ("Aluguel", "Aluguel da loja", 2_200, 10),
            ("Água", "Conta de água", 120 + i * 5, 10),
            ("Internet", "Internet fibra", 150, 10),
            ("Telefone / Celular", "Plano de celular", 130, 10),
            ("Honorários do contador", "Honorários do contador", 450, 10),
            ("Sistemas", "Sistemas e assinaturas", 480, 10),
            ("Salários", "Folha de pagamento", 6_000, 5),
            ("Pró-labore", "Pró-labore dos sócios", 4_000, 10),
            ("FGTS", "FGTS da folha", 480, 7),
            ("INSS", "INSS da folha", 900, 20),
            (
                "Simples Nacional",
                "DAS do mês",
                (vendas + servicos) * 6 / 100,
                20,
            ),
            (
                "Compra de mercadorias para revenda",
                "Reposição de estoque",
                vendas * 42 / 100,
                15,
            ),
            (
                "Combustíveis",
                "Combustível dos carros",
                600 + (i * 37) % 200,
                18,
            ),
            ("Publicidade e propaganda", "Anúncios", 800, 12),
            ("Despesas bancárias", "Tarifas do banco", 90, 2),
        ] {
            lancar(false, categoria, descricao, reais, dia(d), pago(d));
        }
        // A energia deste mês fica atrasada (para o quadro "Em atraso").
        let energia_paga = passado;
        lancar(
            false,
            "Energia elétrica",
            "Conta de luz",
            420 + (i * 23) % 120,
            dia(8),
            energia_paga,
        );
        if i == 2 {
            lancar(
                false,
                "Manutenção de equipamentos",
                "Conserto do ar-condicionado",
                1_850,
                dia(16),
                pago(16),
            );
        }
        i += 1;
        mes = mes.mais_meses(1);
    }
    // O extrato do banco dos últimos 12 dias, importado e por conciliar: o que o sistema já
    // conhece (com a sugestão de casamento) e duas linhas que ele não conhece — um Pix de
    // cliente e uma tarifa —, para a conciliação ter o que mostrar.
    let mut linhas: Vec<(Data, String, String)> = pelo_banco
        .into_inner()
        .into_iter()
        .filter(|(d, ..)| *d >= hoje.mais_dias(-12))
        .collect();
    linhas.push((
        hoje.mais_dias(-2),
        "PIX RECEBIDO MARIA SOUZA".to_owned(),
        "275,00".to_owned(),
    ));
    linhas.push((
        hoje.mais_dias(-4),
        "TARIFA PACOTE DE SERVICOS".to_owned(),
        "-39,90".to_owned(),
    ));
    linhas.sort_by_key(|(d, ..)| *d);
    let csv: String = std::iter::once("data;descrição;valor".to_owned())
        .chain(
            linhas
                .iter()
                .map(|(d, texto, valor)| format!("{};{texto};{valor}", d.formatar())),
        )
        .collect::<Vec<_>>()
        .join("\n");
    let _: mod_financeiro::ExtratoImportado = motor
        .executar(
            s,
            "financeiro.importar_extrato.v1",
            &mod_financeiro::ImportarExtrato {
                conta: conta("1.1.02").expect("conta Bancos"),
                formato: mod_financeiro::extrato::FormatoExtrato::Csv,
                arquivo: "extrato-banco.csv".to_owned(),
                conteudo: csv,
            },
        )
        .expect("extrato");
    // O orçamento do ano, pelas contas das categorias: perto do realizado, com umas linhas
    // estourando (energia, sistemas, compras) e outras sobrando (publicidade), como na vida real.
    let todo_mes = |reais: i64| [Dinheiro::reais(reais); 12];
    let contas: Vec<(cardeal_kernel::Id, [Dinheiro; 12])> = [
        ("4.1", todo_mes(26_000)),
        ("4.2", todo_mes(9_000)),
        ("5.50.01", todo_mes(2_100)),
        ("5.51.02", todo_mes(10_800)),
        ("5.52.01", todo_mes(2_200)),
        ("5.52.02", todo_mes(150)),
        ("5.52.03", todo_mes(420)),
        ("5.52.04", todo_mes(130)),
        ("5.52.05", todo_mes(150)),
        ("5.53.01", todo_mes(450)),
        ("5.54.01", todo_mes(420)),
        ("5.55.01", todo_mes(700)),
        ("5.56.01", todo_mes(1_000)),
        ("5.57.01", todo_mes(6_000)),
        ("5.57.02", todo_mes(4_000)),
        ("5.57.07", todo_mes(480)),
        ("5.57.08", todo_mes(900)),
        ("5.58.01", todo_mes(90)),
    ]
    .into_iter()
    .filter_map(|(codigo, meses)| Some((conta(codigo)?, meses)))
    .collect();
    let _: u32 = motor
        .executar(
            s,
            "financeiro.definir_orcamento_do_ano.v1",
            &DefinirOrcamentoDoAno {
                ano: hoje.ano(),
                contas,
            },
        )
        .expect("orçamento");
}

/// As finanças pessoais do usuário (pró-labore, faculdade, cartão parcelado) para as cenas do
/// pessoal. Os custos da empresa vêm do ano de caixa (`semear_ano_financeiro`).
fn semear_pessoal(motor: &Motor, s: &Sessao, hoje: Data) {
    use mod_financeiro::pessoal::comandos::LancarPessoal;
    use mod_financeiro::pessoal::{NovoPessoal, Repeticao, TipoPessoal};

    let inicio = hoje.inicio_do_mes();
    let pessoal = |tipo,
                   descricao: &str,
                   categoria: &str,
                   reais: i64,
                   dia: i32,
                   repeticao,
                   cartao: Option<&str>| {
        let _: Vec<cardeal_kernel::Id> = motor
            .executar(
                s,
                "financeiro.lancar_pessoal.v1",
                &LancarPessoal(NovoPessoal {
                    tipo,
                    descricao: descricao.to_owned(),
                    categoria: categoria.to_owned(),
                    valor: Dinheiro::reais(reais),
                    primeiro_vencimento: inicio.mais_dias(dia - 1),
                    repeticao,
                    cartao: cartao.map(str::to_owned),
                    primeira_paga: false,
                }),
            )
            .expect("pessoal");
    };
    pessoal(
        TipoPessoal::Receita,
        "Pró-labore",
        "Pró-labore",
        4_000,
        5,
        Repeticao::Mensal { meses: 12 },
        None,
    );
    pessoal(
        TipoPessoal::Receita,
        "Venda de celular usado",
        "Vendas por fora",
        900,
        18,
        Repeticao::Unica,
        None,
    );
    pessoal(
        TipoPessoal::Despesa,
        "Parcelamentos anteriores",
        "Compras",
        800,
        10,
        Repeticao::Mensal { meses: 10 },
        Some("Nubank"),
    );
    pessoal(
        TipoPessoal::Despesa,
        "Notebook",
        "Compras",
        3_600,
        10,
        Repeticao::Parcelada { vezes: 12 },
        Some("Nubank"),
    );
    pessoal(
        TipoPessoal::Despesa,
        "Mercado do mês",
        "Alimentação",
        650,
        10,
        Repeticao::Unica,
        Some("Inter"),
    );
    pessoal(
        TipoPessoal::Despesa,
        "Faculdade",
        "Educação",
        890,
        8,
        Repeticao::Mensal { meses: 18 },
        None,
    );
    pessoal(
        TipoPessoal::Despesa,
        "Aluguel do apartamento",
        "Moradia",
        1_400,
        5,
        Repeticao::Mensal { meses: 12 },
        None,
    );
    pessoal(
        TipoPessoal::Despesa,
        "Luz e internet de casa",
        "Contas da casa",
        260,
        15,
        Repeticao::Mensal { meses: 12 },
        None,
    );
}
