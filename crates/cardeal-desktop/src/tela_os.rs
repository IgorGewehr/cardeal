//! Tela de Ordens de Serviço — lista + dialog (criar / ver / agir). `docs/modulos/os.md`.
//!
//! Padrão de UI do projeto: a tela abre na lista inteira; "Nova OS" e o clique numa linha
//! abrem o mesmo `Dialogo`. OS é workflow (máquina de estados), então o dialog de detalhe
//! mostra as infos + a ação certa pro estado atual, em vez de um "Editar" genérico.

use cardeal_analytics::{calcular_margem_de_os, CustoHorario, MargemDeOs};
use cardeal_cliente::{IdentidadeVisual, MotorLocal, SessaoLocal, UsuarioResumo};
use cardeal_kernel::{Dinheiro, Id, Instante, Percentual, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_pdf::{gerar_comprovante_os, ComprovanteOsPdf, IdentidadeEmpresa, ItemPdf};
use cardeal_ui::atoms::{Botao, Rotulo, ValorDinheiro};
use cardeal_ui::molecules::{Abas, Campo, CartaoKpi, EstadoVazio, Mascara, SeletorOpcao};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, FaixaKpi, Grade, LayoutTela, Notificacao,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{
    AdicionarContato, ContatoInicial, CriarPessoa, EnderecoInicial, ItemPessoa,
    Papel as PapelCliente, PessoaCadastrada, PessoasPorPapel, TipoContato, TipoDocumento,
    TipoEndereco, TipoPessoa,
};
use mod_estoque::{ItemProdutoComSaldo, ProdutosComSaldo};
use mod_financeiro::{Titulo, TituloDaOrigem};
use mod_os::{
    AbrirOrdemServico, ApontamentoDeTempo, ApontamentosDaOrdem, AprovarOrcamentoOs,
    BuscarDetalheOrdem, ConcluirExecucao, DetalheOrdem, EncerrarApontamento, EnviarParaAprovacao,
    EstadoOs, FaturarOrdemServico, IniciarApontamento, IniciarExecucao, ItemOrcamentoNovo,
    MontarOrcamentoOs, OrdemServico, OrdemServicoAberta, OrdensEmAberto, RegistrarLaudo,
    ReprovarOrcamentoOs, TempoTotalDaOrdem,
};

/// Qual dialog está aberto.
#[derive(Default)]
enum Dlg {
    #[default]
    Fechado,
    Nova {
        /// `true` = cadastrar cliente novo; `false` = escolher da lista.
        cliente_novo: bool,
        cliente_sel: Option<Id>,
        nome: String,
        /// CPF ou CNPJ — opcional (pedido do usuário: só o nome do cliente é obrigatório).
        documento: String,
        /// Telefone/WhatsApp — opcional.
        telefone: String,
        /// E-mail — opcional.
        email: String,
        end_logradouro: String,
        end_numero: String,
        end_bairro: String,
        end_cidade: String,
        end_uf: String,
        end_cep: String,
        /// Descrição livre do equipamento — opcional (completável depois na edição da OS).
        equipamento: String,
        /// O que o cliente relatou querer resolver — **obrigatório** (junto do nome do
        /// cliente, o único campo que a abertura exige).
        defeito_relatado: String,
    },
    Detalhe,
}

impl Dlg {
    fn nova() -> Self {
        Self::Nova {
            cliente_novo: false,
            cliente_sel: None,
            nome: String::new(),
            documento: String::new(),
            telefone: String::new(),
            email: String::new(),
            end_logradouro: String::new(),
            end_numero: String::new(),
            end_bairro: String::new(),
            end_cidade: String::new(),
            end_uf: String::new(),
            end_cep: String::new(),
            equipamento: String::new(),
            defeito_relatado: String::new(),
        }
    }
}

/// Qual aba da tela de OS está ativa.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AbaOs {
    #[default]
    Ordens,
    Orcamentos,
    Lucratividade,
}

/// Estado local da tela — sobrevive entre quadros, não entre reinícios.
#[derive(Default)]
pub struct EstadoTelaOs {
    aba: AbaOs,
    orc: crate::tela_orcamentos::EstadoOrcamentos,
    ordens: Vec<OrdemServico>,
    clientes: Vec<ItemPessoa>,
    produtos: Vec<ItemProdutoComSaldo>,
    usuarios: Vec<UsuarioResumo>,
    identidade: Option<IdentidadeVisual>,
    detalhe: Option<DetalheOrdem>,
    apontamentos: Vec<ApontamentoDeTempo>,
    tempo_total_ordem: i64,
    titulo_gerado: Option<Titulo>,
    margens: Vec<MargemDeOs>,
    calculando_margens: bool,
    erro: Option<String>,
    dlg: Dlg,

    laudo_problema: String,
    laudo_diagnostico: String,
    mao_de_obra_descricao: String,
    mao_de_obra_valor: String,
    peca_produto: Option<Id>,
    peca_qtd: String,
    peca_preco: String,
    aprovador: String,
}

impl EstadoTelaOs {
    /// Recarrega a lista de ordens em aberto e os catálogos de cliente e produto.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "os.ordens_em_aberto.v1", &OrdensEmAberto) {
            Ok(ordens) => {
                self.ordens = ordens;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(c) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: PapelCliente::Cliente,
                busca: None,
            },
        ) {
            self.clientes = c;
        }
        if let Ok(p) = motor.consultar(sessao, "estoque.produtos_com_saldo.v1", &ProdutosComSaldo) {
            self.produtos = p;
        }
        if let Ok(u) = motor.usuarios() {
            self.usuarios = u;
        }
        if let Ok(i) = motor.identidade_visual() {
            self.identidade = Some(i);
        }
        self.orc.carregar(motor, sessao);
    }

    fn abrir_detalhe(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, id: Id) {
        match motor.consultar(
            sessao,
            "os.buscar_detalhe_ordem.v1",
            &BuscarDetalheOrdem { ordem_servico: id },
        ) {
            Ok(detalhe) => {
                self.detalhe = detalhe;
                self.dlg = Dlg::Detalhe;
                self.limpar_campos();
                self.carregar_apontamentos(motor, sessao, id);
                self.titulo_gerado = None;
                if let Some(d) = &self.detalhe {
                    if d.ordem.estado == EstadoOs::Faturada {
                        if let Ok(t) = motor.consultar(
                            sessao,
                            "financeiro.titulo_da_origem.v1",
                            &TituloDaOrigem {
                                origem_modulo: "os".to_string(),
                                origem_id: id,
                            },
                        ) {
                            self.titulo_gerado = t;
                        }
                    }
                }
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
    }

    /// Recarrega os apontamentos de tempo e o total acumulado da ordem.
    fn carregar_apontamentos(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, ordem: Id) {
        self.apontamentos = motor
            .consultar(
                sessao,
                "os.apontamentos_da_ordem.v1",
                &ApontamentosDaOrdem {
                    ordem_servico: ordem,
                },
            )
            .unwrap_or_default();
        self.tempo_total_ordem = motor
            .consultar(
                sessao,
                "os.tempo_total_da_ordem.v1",
                &TempoTotalDaOrdem {
                    ordem_servico: ordem,
                },
            )
            .unwrap_or(0);
    }

    fn limpar_campos(&mut self) {
        self.laudo_problema.clear();
        self.laudo_diagnostico.clear();
        self.mao_de_obra_descricao.clear();
        self.mao_de_obra_valor.clear();
        self.peca_produto = None;
        self.peca_qtd.clear();
        self.peca_preco.clear();
        self.aprovador.clear();
    }
}

/// Desenha a tela inteira.
pub fn mostrar(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    LayoutTela::nova("Ordens de Serviço").mostrar(
        ui,
        estado,
        |ui, estado| match estado.aba {
            AbaOs::Ordens => {
                if ui
                    .add(Botao::primario("+ Nova OS").atalho("Ctrl+N"))
                    .clicked()
                {
                    estado.dlg = Dlg::nova();
                }
            }
            AbaOs::Orcamentos => {
                if ui
                    .add(Botao::primario("+ Novo orçamento").atalho("Ctrl+N"))
                    .clicked()
                {
                    crate::tela_orcamentos::abrir_novo(&mut estado.orc);
                }
            }
            AbaOs::Lucratividade => {
                let rotulo = if estado.calculando_margens {
                    "Calculando…"
                } else {
                    "Calcular lucratividade"
                };
                if ui.add(Botao::primario(rotulo)).clicked() && !estado.calculando_margens {
                    calcular_margens(motor, sessao, estado);
                }
            }
        },
        |ui, estado| {
            if let Some(nova) = Abas::nova(&[
                (AbaOs::Ordens, "Ordens de serviço"),
                (AbaOs::Orcamentos, "Orçamentos"),
                (AbaOs::Lucratividade, "Lucratividade"),
            ])
            .selecionada(estado.aba)
            .mostrar(ui)
            {
                estado.aba = nova;
            }
            ui.add_space(Espaco::E16);

            match estado.aba {
                AbaOs::Ordens => {
                    if let Some(erro) = &estado.erro {
                        ui.add(
                            Rotulo::interface(erro.clone())
                                .quebravel()
                                .cor(ui.cores().negativo),
                        );
                        ui.add_space(Espaco::E12);
                    }
                    lista(ui, motor, sessao, estado);
                }
                AbaOs::Orcamentos => {
                    crate::tela_orcamentos::corpo(ui, motor, sessao, &mut estado.orc);
                }
                AbaOs::Lucratividade => lucratividade(ui, estado),
            }
        },
    );

    match estado.aba {
        AbaOs::Ordens => match estado.dlg {
            Dlg::Fechado => {}
            Dlg::Nova { .. } => dialogo_nova(ui.ctx(), motor, sessao, estado),
            Dlg::Detalhe => dialogo_detalhe(ui.ctx(), motor, sessao, estado),
        },
        AbaOs::Orcamentos => {
            crate::tela_orcamentos::dialogos(ui.ctx(), motor, sessao, &mut estado.orc);
        }
        AbaOs::Lucratividade => {}
    }
}

/// Recalcula a margem de cada ordem hoje carregada (`OrdensEmAberto` — ver a limitação
/// documentada em `lucratividade`) buscando detalhe + apontamentos de cada uma. Sem custo
/// por hora de técnico cadastrado ainda, a margem líquida vem `None` ("não calculável") em
/// vez de inventar um número — `cardeal_analytics::calcular_margem_de_os` já trata isso.
fn calcular_margens(motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaOs) {
    estado.calculando_margens = true;
    let custo_horario = CustoHorario::nova();
    let mut margens = Vec::with_capacity(estado.ordens.len());
    for os in estado.ordens.clone() {
        let detalhe: Option<DetalheOrdem> = motor
            .consultar(
                sessao,
                "os.buscar_detalhe_ordem.v1",
                &BuscarDetalheOrdem {
                    ordem_servico: os.id,
                },
            )
            .unwrap_or(None);
        let Some(detalhe) = detalhe else { continue };
        let apontamentos: Vec<ApontamentoDeTempo> = motor
            .consultar(
                sessao,
                "os.apontamentos_da_ordem.v1",
                &ApontamentosDaOrdem {
                    ordem_servico: os.id,
                },
            )
            .unwrap_or_default();
        margens.push(calcular_margem_de_os(
            &detalhe,
            &apontamentos,
            &custo_horario,
        ));
    }
    estado.margens = margens;
    estado.calculando_margens = false;
}

/// A aba "Lucratividade": receita, custo real de peça e margem de cada OS **atualmente em
/// aberto** (mesma lista da aba "Ordens" — não há hoje uma consulta de OS faturadas por
/// período; ver `docs/modulos/os.md` §6 e o gap anotado lá). Ainda assim é útil para ver, de
/// trabalhos em andamento, quais já estão com a peça consumida a um custo maior que o
/// orçado.
fn lucratividade(ui: &mut egui::Ui, estado: &mut EstadoTelaOs) {
    if estado.margens.is_empty() {
        ui.add(Rotulo::interface(
            "Clique em \"Calcular lucratividade\" para ver a margem de cada ordem em aberto.",
        ));
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Nº").largura(56.0).numero(),
        ColunaGrade::nova("Receita").largura(110.0).numero(),
        ColunaGrade::nova("Custo peça").largura(110.0).numero(),
        ColunaGrade::nova("Margem bruta").largura(120.0).numero(),
        ColunaGrade::nova("Margem líquida").largura(130.0).numero(),
        ColunaGrade::nova("Tempo apontado").largura(120.0).numero(),
    ];
    Grade::nova(colunas)
        .selecionavel(None)
        .mostrar(ui, estado.margens.len(), |i, row| {
            let m = &estado.margens[i];
            row.col(|ui| {
                ui.add(Rotulo::interface(m.numero.to_string()));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(m.receita_total()));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(m.custo_pecas));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(m.margem_bruta));
            });
            row.col(|ui| match m.margem_liquida {
                Some(v) => {
                    ui.add(ValorDinheiro::novo(v));
                }
                None => {
                    ui.add(Rotulo::interface("não calculável"));
                }
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(formatar_duracao(
                    m.tempo_apontado_segundos,
                )));
            });
        });
}

fn lista(ui: &mut egui::Ui, motor: &MotorLocal, sessao: &SessaoLocal, estado: &mut EstadoTelaOs) {
    if estado.ordens.is_empty() {
        if estado.erro.is_none()
            && EstadoVazio::novo(Icone::Ferramenta, "Nenhuma ordem em aberto.")
                .acao("Abrir a primeira OS")
                .mostrar(ui)
        {
            estado.dlg = Dlg::nova();
        }
        return;
    }

    // Dois agregados que a lista já carrega e antes ficavam perdidos entre as linhas —
    // vira cabeçalho de indicador visual (`docs/12-ui-ux.md` §6), a pedido explícito da
    // revisão de UI/UX de 2026-09-11 ("total de OS abertas, valor parado").
    let valor_parado = estado
        .ordens
        .iter()
        .fold(Dinheiro::ZERO, |acc, os| acc + os.valor_total);
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Ordens em aberto", estado.ordens.len()),
        CartaoKpi::novo("Valor em aberto", valor_parado).variacao("soma do total de cada OS"),
    ])
    .mostrar(ui);

    let colunas = vec![
        ColunaGrade::nova("Nº").largura(56.0).numero(),
        ColunaGrade::nova("Equipamento"),
        ColunaGrade::nova("Estado").largura(160.0),
        ColunaGrade::nova("Total").largura(120.0).numero(),
    ];
    let clicada =
        Grade::nova(colunas)
            .selecionavel(None)
            .mostrar(ui, estado.ordens.len(), |i, row| {
                let os = &estado.ordens[i];
                row.col(|ui| {
                    ui.add(Rotulo::interface(os.numero.to_string()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::interface(os.equipamento.clone()));
                });
                row.col(|ui| {
                    ui.add(Rotulo::campo(os.estado.rotulo()));
                });
                row.col(|ui| {
                    ui.add(ValorDinheiro::novo(os.valor_total));
                });
            });
    if let Some(i) = clicada {
        let id = estado.ordens[i].id;
        estado.abrir_detalhe(motor, sessao, id);
    }
}

fn dialogo_nova(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let ops_cli: Vec<(Id, String)> = estado
        .clientes
        .iter()
        .map(|c| (c.pessoa, c.nome.clone()))
        .collect();

    let fechar = Dialogo::nova("Nova ordem de serviço")
        .largura(640.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Nova {
                    cliente_novo,
                    cliente_sel,
                    nome,
                    documento,
                    telefone,
                    email,
                    end_logradouro,
                    end_numero,
                    end_bairro,
                    end_cidade,
                    end_uf,
                    end_cep,
                    equipamento,
                    defeito_relatado,
                } = &mut estado.dlg
                else {
                    return;
                };

                ui.horizontal(|ui| {
                    let sel_existente = if *cliente_novo {
                        Botao::fantasma("Cliente existente")
                    } else {
                        Botao::primario("Cliente existente")
                    };
                    if ui.add(sel_existente).clicked() {
                        *cliente_novo = false;
                    }
                    let sel_novo = if *cliente_novo {
                        Botao::primario("Novo cliente")
                    } else {
                        Botao::fantasma("Novo cliente")
                    };
                    if ui.add(sel_novo).clicked() {
                        *cliente_novo = true;
                    }
                });
                ui.add_space(Espaco::E12);

                if *cliente_novo {
                    // Só o nome é obrigatório — documento, telefone, e-mail e endereço são
                    // opcionais (pedido do usuário: "de obrigatório só o nome").
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Nome do cliente", nome));
                        c[1].add(
                            Campo::novo("Documento (opcional)", documento)
                                .mascara(Mascara::Documento)
                                .marcador("CPF ou CNPJ"),
                        );
                    });
                    ui.add_space(Espaco::E8);
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Telefone/WhatsApp (opcional)", telefone));
                        c[1].add(Campo::novo("E-mail (opcional)", email));
                    });
                    ui.add_space(Espaco::E8);
                    ui.add(Rotulo::campo("Endereço (opcional)"));
                    ui.add_space(Espaco::E4);
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Logradouro", end_logradouro));
                        c[1].add(Campo::novo("Número", end_numero));
                    });
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("Bairro", end_bairro));
                        c[1].add(Campo::novo("Cidade", end_cidade));
                    });
                    ui.columns(2, |c| {
                        c[0].add(Campo::novo("UF", end_uf).marcador("MG"));
                        c[1].add(Campo::novo("CEP", end_cep).marcador("00000-000"));
                    });
                } else {
                    SeletorOpcao::novo("Cliente", cliente_sel)
                        .opcoes(ops_cli.clone())
                        .placeholder("Buscar cliente cadastrado…")
                        .mostrar(ui);
                }
                ui.add_space(Espaco::E16);
                ui.separator();
                ui.add_space(Espaco::E12);
                ui.add(
                    Campo::novo(
                        "Defeito relatado — o que o cliente quer resolver",
                        defeito_relatado,
                    )
                    .marcador("ex.: não liga, tela quebrada, não carrega"),
                );
                ui.add_space(Espaco::E8);
                ui.add(
                    Campo::novo("Equipamento (opcional)", equipamento)
                        .marcador("ex.: Furadeira Bosch GSB 13 — pode completar depois"),
                );
            },
            |ui, estado| {
                if ui.add(Botao::primario("Abrir OS")).clicked() {
                    abrir_os(ui.ctx(), motor, sessao, estado);
                }
                if ui.add(Botao::secundario("Cancelar")).clicked() {
                    estado.dlg = Dlg::Fechado;
                }
            },
        );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn abrir_os(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Dlg::Nova {
        cliente_novo,
        cliente_sel,
        nome,
        documento,
        telefone,
        email,
        end_logradouro,
        end_numero,
        end_bairro,
        end_cidade,
        end_uf,
        end_cep,
        equipamento,
        defeito_relatado,
    } = &estado.dlg
    else {
        return;
    };
    let (cliente_novo, cliente_sel) = (*cliente_novo, *cliente_sel);
    let nome = nome.clone();
    let documento = documento.clone();
    let telefone = telefone.clone();
    let email = email.clone();
    let end_logradouro = end_logradouro.clone();
    let end_numero = end_numero.clone();
    let end_bairro = end_bairro.clone();
    let end_cidade = end_cidade.clone();
    let end_uf = end_uf.clone();
    let end_cep = end_cep.clone();
    let equipamento = equipamento.clone();
    let defeito_relatado = defeito_relatado.clone();

    if defeito_relatado.trim().is_empty() {
        notificar(
            ctx,
            Notificacao::aviso("Informe o defeito relatado pelo cliente."),
        );
        return;
    }

    let cliente_id = if cliente_novo {
        if nome.trim().is_empty() {
            notificar(ctx, Notificacao::aviso("Informe o nome do cliente."));
            return;
        }
        let digitos_doc: String = documento.chars().filter(char::is_ascii_digit).collect();
        let endereco = (!end_logradouro.trim().is_empty()).then(|| EnderecoInicial {
            tipo: TipoEndereco::Residencial,
            logradouro: end_logradouro,
            numero: end_numero,
            complemento: None,
            bairro: end_bairro,
            cidade: end_cidade,
            uf: end_uf,
            cep: end_cep,
        });
        // `CriarPessoa` só aceita um contato inicial — quando telefone E e-mail vêm
        // preenchidos, o telefone entra na criação e o e-mail via `AdicionarContato` logo
        // depois.
        let contato_inicial = if !telefone.trim().is_empty() {
            Some(ContatoInicial {
                tipo: TipoContato::Whatsapp,
                valor: telefone.clone(),
            })
        } else if !email.trim().is_empty() {
            Some(ContatoInicial {
                tipo: TipoContato::Email,
                valor: email.clone(),
            })
        } else {
            None
        };

        let r = motor.executar(
            sessao,
            "clientes.criar_pessoa.v1",
            &CriarPessoa {
                tipo: TipoPessoa::Fisica,
                nome,
                nome_fantasia: None,
                papel_inicial: PapelCliente::Cliente,
                documento_tipo: (!digitos_doc.is_empty()).then_some(if digitos_doc.len() == 14 {
                    TipoDocumento::Cnpj
                } else {
                    TipoDocumento::Cpf
                }),
                documento_numero: (!digitos_doc.is_empty()).then_some(documento),
                data_nascimento: None,
                endereco,
                contato: contato_inicial,
            },
        );
        let pessoa = match r {
            Ok(c) => {
                let c: PessoaCadastrada = c;
                c.pessoa
            }
            Err(e) => {
                notificar(ctx, Notificacao::erro(e.mensagem));
                return;
            }
        };

        if !telefone.trim().is_empty() && !email.trim().is_empty() {
            if let Err(e) = motor.executar(
                sessao,
                "clientes.adicionar_contato.v1",
                &AdicionarContato {
                    pessoa,
                    tipo: TipoContato::Email,
                    valor: email,
                    principal: true,
                },
            ) {
                notificar(
                    ctx,
                    Notificacao::aviso("Cliente criado, mas o e-mail não foi salvo")
                        .detalhe(e.mensagem),
                );
            }
        }

        pessoa
    } else if let Some(id) = cliente_sel {
        id
    } else {
        notificar(
            ctx,
            Notificacao::aviso("Escolha um cliente da lista ou cadastre um novo."),
        );
        return;
    };

    match motor.executar(
        sessao,
        "os.abrir_ordem_servico.v1",
        &AbrirOrdemServico {
            cliente: cliente_id,
            equipamento,
            defeito_relatado,
            tecnico_responsavel: sessao.usuario(),
            garantia_dias: 90,
        },
    ) {
        Ok(aberta) => {
            let aberta: OrdemServicoAberta = aberta;
            estado.erro = None;
            estado.carregar(motor, sessao);
            estado.abrir_detalhe(motor, sessao, aberta.ordem_servico);
            notificar(
                ctx,
                Notificacao::sucesso(format!("OS #{} aberta", aberta.numero)),
            );
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn dialogo_detalhe(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let Some(detalhe) = estado.detalhe.clone() else {
        estado.dlg = Dlg::Fechado;
        return;
    };
    let os = detalhe.ordem.clone();
    let titulo = format!("OS #{} · {}", os.numero, os.equipamento);

    let fechar = Dialogo::nova(titulo).mostrar(
        ctx,
        estado,
        |ui, estado| corpo_detalhe(ui, motor, sessao, estado, &detalhe),
        |ui, estado| {
            if ui.add(Botao::secundario("Comprovante (PDF)")).clicked() {
                gerar_pdf(ui.ctx(), estado, &detalhe);
            }
            if ui.add(Botao::secundario("Fechar")).clicked() {
                estado.dlg = Dlg::Fechado;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Fechado;
    }
}

fn corpo_detalhe(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Estado"));
        ui.add(Rotulo::interface(os.estado.rotulo()));
    });
    ui.add_space(Espaco::E16);

    // ── Laudo ──────────────────────────────────────────────────────────────
    ui.add(Rotulo::titulo_secao("Laudo técnico"));
    ui.add_space(Espaco::E8);
    if let Some(laudo) = &detalhe.laudo {
        ui.add(Rotulo::interface(format!("Problema: {}", laudo.descricao_problema)).quebravel());
        if let Some(dg) = &laudo.diagnostico {
            ui.add(Rotulo::interface(format!("Diagnóstico: {dg}")).quebravel());
        }
    } else if matches!(os.estado, EstadoOs::Aberta) {
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Problema relatado", &mut estado.laudo_problema));
            c[1].add(Campo::novo(
                "Diagnóstico (opcional)",
                &mut estado.laudo_diagnostico,
            ));
        });
        ui.add_space(Espaco::E8);
        if ui.add(Botao::primario("Registrar laudo")).clicked() {
            let diagnostico = (!estado.laudo_diagnostico.trim().is_empty())
                .then(|| estado.laudo_diagnostico.clone());
            aplicar_e_recarregar(
                ui.ctx(),
                motor,
                sessao,
                estado,
                os.id,
                "os.registrar_laudo.v1",
                &RegistrarLaudo {
                    ordem_servico: os.id,
                    descricao_problema: estado.laudo_problema.clone(),
                    diagnostico,
                    tecnico: sessao.usuario(),
                },
                "Laudo registrado",
            );
        }
    } else {
        ui.add(Rotulo::interface("—"));
    }
    ui.add_space(Espaco::E16);

    // ── Orçamento ──────────────────────────────────────────────────────────
    ui.add(Rotulo::titulo_secao("Orçamento"));
    ui.add_space(Espaco::E8);
    for item in &detalhe.itens_mao_de_obra {
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(item.descricao.clone()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(ValorDinheiro::novo(item.valor));
            });
        });
    }
    for item in &detalhe.itens_peca {
        let nome = estado
            .produtos
            .iter()
            .find(|p| p.produto == item.produto)
            .map_or_else(|| "Peça do estoque".to_owned(), |p| p.nome.clone());
        ui.horizontal(|ui| {
            ui.add(Rotulo::interface(format!("{} × {nome}", item.quantidade)));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add(Rotulo::campo(if item.aplicada {
                    "aplicada"
                } else {
                    "pendente"
                }));
                ui.add(Rotulo::interface(format!(
                    "{} /un",
                    item.preco_unitario.formatar_com_simbolo()
                )));
            });
        });
    }
    ui.add_space(Espaco::E4);
    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Total"));
        ui.add(ValorDinheiro::novo(os.valor_total));
    });

    if os.estado.aceita_edicao_de_orcamento() {
        ui.add_space(Espaco::E12);
        ui.add(Rotulo::campo("Mão de obra"));
        ui.add_space(Espaco::E4);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Serviço", &mut estado.mao_de_obra_descricao));
            c[1].add(Campo::novo("Valor", &mut estado.mao_de_obra_valor).marcador("80,00"));
        });
        ui.add_space(Espaco::E4);
        if ui
            .add(Botao::secundario("+ Adicionar mão de obra"))
            .clicked()
        {
            adicionar_mao_de_obra(ui.ctx(), motor, sessao, estado, os.id);
        }

        ui.add_space(Espaco::E12);
        ui.add(Rotulo::campo("Peça do estoque"));
        ui.add_space(Espaco::E4);
        {
            let ops: Vec<(Id, String)> = estado
                .produtos
                .iter()
                .map(|p| (p.produto, format!("{}  ({} disp.)", p.nome, p.disponivel)))
                .collect();
            SeletorOpcao::novo("Produto", &mut estado.peca_produto)
                .opcoes(ops)
                .placeholder("Buscar produto…")
                .mostrar(ui);
        }
        ui.add_space(Espaco::E4);
        ui.columns(2, |c| {
            c[0].add(Campo::novo("Quantidade", &mut estado.peca_qtd).marcador("1"));
            c[1].add(Campo::novo("Preço unitário", &mut estado.peca_preco).marcador("0,00"));
        });
        ui.add_space(Espaco::E4);
        if ui.add(Botao::secundario("+ Adicionar peça")).clicked() {
            adicionar_peca(ui.ctx(), motor, sessao, estado, os.id);
        }

        ui.add_space(Espaco::E12);
        ui.horizontal(|ui| {
            if !os.valor_total.e_zero()
                && ui.add(Botao::primario("Enviar para aprovação")).clicked()
            {
                aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.enviar_para_aprovacao.v1",
                    &EnviarParaAprovacao {
                        ordem_servico: os.id,
                    },
                    "Orçamento enviado para aprovação",
                );
            }
        });
    }

    ui.add_space(Espaco::E16);
    secao_apontamento(ui, motor, sessao, estado, os.id);

    if os.estado == EstadoOs::Faturada {
        ui.add_space(Espaco::E16);
        ui.add(Rotulo::titulo_secao("Financeiro"));
        ui.add_space(Espaco::E8);
        match &estado.titulo_gerado {
            Some(titulo) => {
                ui.add(Rotulo::interface(format!(
                    "Título a receber gerado: {}",
                    titulo.id.curto()
                )));
                ui.add(ValorDinheiro::novo(titulo.valor_original));
            }
            None => {
                ui.add(Rotulo::interface(
                    "Faturada sem cobrança (garantia/cortesia) — nenhum título gerado.",
                ));
            }
        }
    }

    ui.add_space(Espaco::E16);
    acoes_por_estado(ui, motor, sessao, estado, detalhe);
}

/// Formata segundos como `Hh MMmin` — o suficiente para a UI mostrar tempo acumulado sem
/// exigir uma dependência de formatação de duração no kernel só para isto.
fn formatar_duracao(segundos: i64) -> String {
    let segundos = segundos.max(0);
    let horas = segundos / 3600;
    let minutos = (segundos % 3600) / 60;
    if horas > 0 {
        format!("{horas}h {minutos:02}min")
    } else {
        format!("{minutos}min")
    }
}

/// A seção "Apontamento de tempo" do detalhe da OS: tempo total acumulado (encerrado) +
/// cronômetro do técnico logado, se houver um apontamento aberto dele nesta ordem — botão
/// simples "Iniciar"/"Encerrar apontamento", sem exigir uma tela própria
/// (`docs/modulos/os.md`: apontamento de tempo real, diferente da mão de obra orçada).
fn secao_apontamento(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem_servico: Id,
) {
    ui.add(Rotulo::titulo_secao("Apontamento de tempo"));
    ui.add_space(Espaco::E8);

    let aberto_do_usuario = estado
        .apontamentos
        .iter()
        .find(|a| a.tecnico == sessao.usuario() && a.esta_aberto())
        .cloned();

    ui.horizontal(|ui| {
        ui.add(Rotulo::campo("Tempo total registrado"));
        let total = match &aberto_do_usuario {
            Some(ap) => estado.tempo_total_ordem + ap.duracao_segundos(Instante::agora()),
            None => estado.tempo_total_ordem,
        };
        ui.add(Rotulo::interface(formatar_duracao(total)));
    });
    ui.add_space(Espaco::E8);

    match aberto_do_usuario {
        Some(ap) => {
            ui.add(Rotulo::interface(format!(
                "Cronômetro rodando desde {}",
                ap.inicio.formatar(cardeal_kernel::Fuso::BRASILIA)
            )));
            if ui.add(Botao::secundario("Encerrar apontamento")).clicked() {
                let r = motor.executar(
                    sessao,
                    "os.encerrar_apontamento.v1",
                    &EncerrarApontamento { apontamento: ap.id },
                );
                match r {
                    Ok(_duracao) => {
                        estado.carregar_apontamentos(motor, sessao, ordem_servico);
                        notificar(ui.ctx(), Notificacao::sucesso("Apontamento encerrado"));
                    }
                    Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                }
            }
        }
        None => {
            if ui.add(Botao::primario("Iniciar apontamento")).clicked() {
                let r = motor.executar(
                    sessao,
                    "os.iniciar_apontamento.v1",
                    &IniciarApontamento {
                        ordem_servico,
                        tecnico: sessao.usuario(),
                    },
                );
                match r {
                    Ok(_) => {
                        estado.carregar_apontamentos(motor, sessao, ordem_servico);
                        notificar(ui.ctx(), Notificacao::sucesso("Apontamento iniciado"));
                    }
                    Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                }
            }
        }
    }
}

fn adicionar_mao_de_obra(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    os: Id,
) {
    match estado.mao_de_obra_valor.parse::<Dinheiro>() {
        Ok(valor) => {
            let r = motor.executar(
                sessao,
                "os.montar_orcamento.v1",
                &MontarOrcamentoOs {
                    ordem_servico: os,
                    item: ItemOrcamentoNovo::MaoDeObra {
                        descricao: estado.mao_de_obra_descricao.clone(),
                        valor,
                        tecnico: sessao.usuario(),
                        horas: None,
                    },
                },
            );
            match r {
                Ok(id) => {
                    let _: Id = id;
                    estado.mao_de_obra_descricao.clear();
                    estado.mao_de_obra_valor.clear();
                    estado.abrir_detalhe(motor, sessao, os);
                    estado.dlg = Dlg::Detalhe;
                    notificar(ctx, Notificacao::sucesso("Mão de obra adicionada"));
                }
                Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
            }
        }
        Err(_) => notificar(
            ctx,
            Notificacao::aviso("Valor inválido — use o formato 80,00"),
        ),
    }
}

fn adicionar_peca(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    os: Id,
) {
    let Some(produto) = estado.peca_produto else {
        notificar(
            ctx,
            Notificacao::aviso("Escolha o produto da lista de estoque."),
        );
        return;
    };
    let (Ok(quantidade), Ok(preco_unitario)) = (
        estado.peca_qtd.parse::<Quantidade>(),
        estado.peca_preco.parse::<Preco>(),
    ) else {
        notificar(ctx, Notificacao::aviso("Quantidade ou preço inválidos."));
        return;
    };
    let r = motor.executar(
        sessao,
        "os.montar_orcamento.v1",
        &MontarOrcamentoOs {
            ordem_servico: os,
            item: ItemOrcamentoNovo::Peca {
                produto,
                quantidade,
                preco_unitario,
            },
        },
    );
    match r {
        Ok(id) => {
            let _: Id = id;
            estado.peca_produto = None;
            estado.peca_qtd.clear();
            estado.peca_preco.clear();
            estado.abrir_detalhe(motor, sessao, os);
            estado.dlg = Dlg::Detalhe;
            notificar(ctx, Notificacao::sucesso("Peça adicionada ao orçamento"));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

fn acoes_por_estado(
    ui: &mut egui::Ui,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    detalhe: &DetalheOrdem,
) {
    let os = &detalhe.ordem;
    match os.estado {
        EstadoOs::AguardandoAprovacao => {
            ui.add(Rotulo::titulo_secao("Aprovação do cliente"));
            ui.add_space(Espaco::E8);
            ui.add(Campo::novo(
                "Quem aprovou (nome e documento)",
                &mut estado.aprovador,
            ));
            ui.add_space(Espaco::E8);
            ui.horizontal(|ui| {
                if ui.add(Botao::primario("Aprovar")).clicked() {
                    aplicar_e_recarregar(
                        ui.ctx(),
                        motor,
                        sessao,
                        estado,
                        os.id,
                        "os.aprovar_orcamento.v1",
                        &AprovarOrcamentoOs {
                            ordem_servico: os.id,
                            identificacao_aprovador: estado.aprovador.clone(),
                        },
                        "Orçamento aprovado",
                    );
                }
                if ui.add(Botao::destrutivo("Reprovar")).clicked() {
                    aplicar_e_recarregar(
                        ui.ctx(),
                        motor,
                        sessao,
                        estado,
                        os.id,
                        "os.reprovar_orcamento.v1",
                        &ReprovarOrcamentoOs {
                            ordem_servico: os.id,
                        },
                        "Orçamento reprovado",
                    );
                }
            });
        }
        EstadoOs::Aprovada => {
            if ui.add(Botao::primario("Iniciar execução")).clicked() {
                aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.iniciar_execucao.v1",
                    &IniciarExecucao {
                        ordem_servico: os.id,
                    },
                    "Execução iniciada",
                );
            }
        }
        EstadoOs::EmExecucao => {
            if detalhe.itens_peca.iter().any(|i| !i.aplicada) {
                ui.add(
                    Rotulo::interface("Há peça do orçamento ainda não aplicada no estoque.")
                        .cor(ui.cores().atencao),
                );
            } else if ui.add(Botao::primario("Concluir execução")).clicked() {
                aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.concluir_execucao.v1",
                    &ConcluirExecucao {
                        ordem_servico: os.id,
                    },
                    "Execução concluída",
                );
            }
        }
        EstadoOs::Concluida => {
            if ui.add(Botao::primario("Faturar")).clicked() {
                aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    os.id,
                    "os.faturar_ordem_servico.v1",
                    &FaturarOrdemServico {
                        ordem_servico: os.id,
                    },
                    "OS faturada",
                );
            }
        }
        EstadoOs::Faturada
        | EstadoOs::Cancelada
        | EstadoOs::Reprovada
        | EstadoOs::Aberta
        | EstadoOs::EmDiagnostico => {}
    }
}

#[allow(clippy::too_many_arguments)]
fn aplicar_e_recarregar<C: cardeal_modkit::Comando + serde::Serialize>(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem: Id,
    nome: &str,
    comando: &C,
    sucesso: &str,
) where
    C::Saida: serde::de::DeserializeOwned,
{
    match motor.executar(sessao, nome, comando) {
        Ok(_) => {
            estado.carregar(motor, sessao);
            estado.abrir_detalhe(motor, sessao, ordem);
            estado.dlg = Dlg::Detalhe;
            notificar(ctx, Notificacao::sucesso(sucesso.to_owned()));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

// ── PDF ──────────────────────────────────────────────────────────────────────

/// Rótulo amigável do estado da OS para o comprovante — o cliente não deveria ler
/// "EmDiagnostico" no papel que leva pra casa.
fn situacao_amigavel(e: EstadoOs) -> &'static str {
    match e {
        EstadoOs::Aberta => "Aberta — aguardando diagnóstico",
        EstadoOs::EmDiagnostico => "Em diagnóstico",
        EstadoOs::AguardandoAprovacao => "Aguardando aprovação do orçamento",
        EstadoOs::Aprovada => "Orçamento aprovado — aguardando execução",
        EstadoOs::Reprovada => "Orçamento reprovado",
        EstadoOs::EmExecucao => "Em execução",
        EstadoOs::Concluida => "Execução concluída — aguardando faturamento",
        EstadoOs::Faturada => "Concluída e faturada",
        EstadoOs::Cancelada => "Cancelada",
    }
}

/// Monta e salva o comprovante de OS em PDF — mesmo padrão de
/// `tela_orcamentos::gerar_pdf` (identidade visual da empresa + diálogo nativo "Salvar
/// como"). Gerável em qualquer estado da OS: uma OS recém-aberta ainda sem laudo/orçamento
/// vira o comprovante de entrada (só cliente + equipamento); uma faturada leva o laudo, os
/// itens aplicados e a garantia — o mesmo documento cobre as duas pontas do fluxo.
fn gerar_pdf(ctx: &egui::Context, estado: &EstadoTelaOs, d: &DetalheOrdem) {
    let ident = estado.identidade.clone().unwrap_or_default();
    let empresa = IdentidadeEmpresa {
        nome_fantasia: ident.nome_fantasia,
        razao_social: ident.razao_social,
        cnpj: ident.cnpj,
        endereco: ident.endereco,
        telefone: ident.telefone,
        email: ident.email,
        site: ident.site,
        logo_png: ident.logo_png,
    };

    let os = &d.ordem;
    let cliente = estado.clientes.iter().find(|c| c.pessoa == os.cliente);
    let tecnico = estado
        .usuarios
        .iter()
        .find(|u| u.id == os.tecnico_responsavel);

    let mut itens: Vec<ItemPdf> = Vec::new();
    for peca in &d.itens_peca {
        let nome_produto = estado
            .produtos
            .iter()
            .find(|p| p.produto == peca.produto)
            .map_or_else(|| "Peça".to_owned(), |p| p.nome.clone());
        let rotulo = if peca.coberto_garantia {
            format!("Peça — {nome_produto} (garantia)")
        } else {
            format!("Peça — {nome_produto}")
        };
        itens.push(ItemPdf {
            descricao: rotulo,
            quantidade: peca.quantidade,
            unidade: "un".to_owned(),
            preco_unitario: peca.preco_unitario,
            desconto_pct: Percentual::ZERO,
            total: peca.total_cobrado(),
        });
    }
    for mdo in &d.itens_mao_de_obra {
        let quantidade = mdo.horas.unwrap_or_else(|| Quantidade::unidades(1));
        itens.push(ItemPdf {
            descricao: format!("Mão de obra — {}", mdo.descricao),
            quantidade,
            unidade: if mdo.horas.is_some() {
                "h".to_owned()
            } else {
                "srv".to_owned()
            },
            preco_unitario: Preco::centavos(mdo.valor.em_centavos()),
            desconto_pct: Percentual::ZERO,
            total: mdo.valor,
        });
    }
    let subtotal = itens.iter().fold(Dinheiro::ZERO, |acc, i| acc + i.total);

    let doc = ComprovanteOsPdf {
        numero: os.numero,
        situacao: situacao_amigavel(os.estado).to_owned(),
        cliente_nome: cliente.map_or_else(String::new, |c| c.nome.clone()),
        cliente_documento: cliente
            .and_then(|c| c.documento.clone())
            .unwrap_or_default(),
        cliente_contato: String::new(),
        equipamento: os.equipamento.clone(),
        data_abertura: os.data_abertura,
        // O defeito relatado agora é capturado na abertura (`OrdemServico::defeito_relatado`,
        // obrigatório) — o laudo só entra como respaldo para uma OS aberta antes desta versão
        // (migração aditiva, coluna nova preenchida com "" nas linhas antigas).
        defeito_relatado: if os.defeito_relatado.trim().is_empty() {
            d.laudo
                .as_ref()
                .map_or_else(String::new, |l| l.descricao_problema.clone())
        } else {
            os.defeito_relatado.clone()
        },
        diagnostico: d
            .laudo
            .as_ref()
            .and_then(|l| l.diagnostico.clone())
            .unwrap_or_default(),
        itens,
        subtotal,
        total: subtotal,
        garantia_dias: os.garantia_dias,
        aprovado_por: os.aprovado_por.clone().unwrap_or_default(),
        tecnico_responsavel: tecnico.map_or_else(String::new, |t| t.nome.clone()),
    };

    let bytes = match gerar_comprovante_os(&empresa, &doc, Instante::agora()) {
        Ok(b) => b,
        Err(e) => {
            notificar(ctx, Notificacao::erro(format!("PDF: {e}")));
            return;
        }
    };
    let nome = format!("OS-{:04}.pdf", os.numero);
    let Some(caminho) = rfd::FileDialog::new()
        .set_file_name(&nome)
        .add_filter("PDF", &["pdf"])
        .save_file()
    else {
        return;
    };
    match std::fs::write(&caminho, &bytes) {
        Ok(()) => {
            let _ = open::that_detached(&caminho);
            notificar(
                ctx,
                Notificacao::sucesso(format!("PDF salvo em {}", caminho.display())),
            );
        }
        Err(e) => notificar(
            ctx,
            Notificacao::erro(format!("Não foi possível salvar: {e}")),
        ),
    }
}
