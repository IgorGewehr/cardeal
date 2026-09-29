//! Conferência visual do app inteiro (só com `--features demo`): banco temporário com dados
//! de exemplo, login automático, a área/diálogo pedidos e um PNG — o equivalente da galeria
//! para telas que dependem do motor.
//!
//! ```text
//! CARDEAL_DEMO=os CARDEAL_DEMO_PNG=/tmp/os.png cargo run -p cardeal-desktop --features demo
//! ```
//!
//! Cenas: `os`, `os-detalhe`, `os-nova`, `os-faturar`, `financeiro`, `financeiro-receber`,
//! `financeiro-baixa`, `financeiro-lancar`. Sem `CARDEAL_DEMO_PNG` a janela fica aberta para mexer.

use cardeal_cliente::{MotorLocal, SessaoLocal};
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
}

impl Captura {
    /// `Some` quando `CARDEAL_DEMO` está definido.
    pub fn do_ambiente() -> Option<Self> {
        let cena = std::env::var("CARDEAL_DEMO").ok()?;
        let dir = tempfile::tempdir().expect("diretório temporário");
        let base = dir.path().join("demo.db");
        Some(Self {
            cena,
            destino: std::env::var_os("CARDEAL_DEMO_PNG").map(Into::into),
            _dir: dir,
            base,
            quadros: 0,
            pediu: false,
        })
    }

    /// Conta quadros, pede a captura e grava o PNG; fecha a janela depois.
    pub fn quadro(&mut self, ctx: &egui::Context) {
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
pub fn semear(motor: &MotorLocal) -> SessaoLocal {
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
                grupo_produto: grupo.grupo_produto,
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
