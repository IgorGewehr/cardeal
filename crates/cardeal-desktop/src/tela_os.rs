//! Tela de Ordens de Serviço — lista + dialog (criar / ver / agir). `docs/modulos/os.md`.
//!
//! Padrão de UI do projeto: a tela abre na lista inteira; "Nova OS" e o clique numa linha
//! abrem o mesmo `Dialogo`. OS é workflow (máquina de estados), então o dialog de detalhe
//! mostra as infos + a ação certa pro estado atual, em vez de um "Editar" genérico.

use cardeal_cliente::{IdentidadeVisual, MotorLocal, SessaoLocal, UsuarioResumo};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante, Percentual, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_pdf::{gerar_comprovante_os, ComprovanteOsPdf, IdentidadeEmpresa, ItemPdf};
use cardeal_ui::atoms::{Botao, Etiqueta, Rotulo, Tom, ValorDinheiro};
use cardeal_ui::molecules::{
    Abas, Campo, CartaoKpi, EstadoVazio, Mascara, OpcaoBusca, SecaoExpansivel, SeletorBusca,
    SeletorOpcao,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, FaixaKpi, Grade, LayoutTela, Notificacao,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use eframe::egui;
use mod_clientes::{
    AdicionarContato, ContatoInicial, CriarPessoa, EnderecoInicial, ItemPessoa,
    Papel as PapelCliente, PessoaCadastrada, PessoasPorPapel, TipoContato, TipoDocumento,
    TipoEndereco, TipoPessoa,
};
use mod_estoque::{ItemProdutoComSaldo, ProdutosComSaldo};
use mod_financeiro::{
    ContaBancariaCriada, ContasDisponiveis, CriarContaBancaria, ItemContaDisponivel, MeioPagamento,
    Titulo, TituloDaOrigem,
};
use mod_os::{
    AbrirOrdemServico, ApontamentoDeTempo, ApontamentosDaOrdem, AprovarOrcamentoOs,
    BuscarDetalheOrdem, CancelarOrdemServico, ConcluirExecucao, DesfaturarOrdemServico,
    DetalheOrdem, EditarDadosDaOrdem, EncerrarApontamento, EnviarParaAprovacao, EstadoOs,
    FaturarOrdemServico, IniciarApontamento, IniciarExecucao, ItemOrcamentoNovo, MontarOrcamentoOs,
    OrdemServico, OrdemServicoAberta, OrdemServicoCancelada, OrdemServicoFaturada, PagamentoNoAto,
    ReabrirOrdemServico, RegistrarLaudo, ReprovarOrcamentoOs, TempoTotalDaOrdem, TodasAsOrdens,
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
        /// O texto digitado no lookup de cliente (`SeletorBusca`) — filtra por nome e
        /// documento.
        cliente_busca: String,
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
        /// Descrição livre do aparelho trazido para reparo — **obrigatório** (ainda
        /// corrigível depois via "Editar" na lista).
        equipamento: String,
        /// O que o cliente relatou querer resolver — **obrigatório** (junto do nome do
        /// cliente e do aparelho, os campos que a abertura exige).
        defeito_relatado: String,
    },
    Detalhe,
    /// "Editar dados da ordem" (`EditarDadosDaOrdem`) — corrigir o aparelho e/ou
    /// complementar o defeito relatado de uma OS já aberta.
    EditarDados(Id),
    /// Faturar a OS aberta em `EstadoTelaOs::detalhe` — disponível desde a abertura, não só
    /// depois do trâmite completo (pedido explícito do usuário, 2026-09-15: laudo →
    /// orçamento → aprovação → execução → conclusão são passos opcionais, não um portão pro
    /// faturamento). Pede o meio de pagamento e já baixa na hora — fatura e recebe no mesmo
    /// clique é o caso comum no balcão.
    Faturar {
        meio_pagamento: Option<MeioPagamento>,
        /// As contas "1.1.*" que não são o Caixa físico — candidatas a receber um pagamento
        /// em Pix/cartão. Carregadas uma vez na abertura do dialog.
        contas: Vec<ItemContaDisponivel>,
        contas_carregadas: bool,
        conta_escolhida: Option<Id>,
        /// `true` = mostra o miniformulário "Nova conta bancária" em vez do seletor — ligado
        /// sozinho quando `contas` vem vazia (sem conta nenhuma pra escolher).
        criando_conta: bool,
        nova_conta_nome: String,
    },
}

impl Dlg {
    fn nova() -> Self {
        Self::Nova {
            cliente_novo: false,
            cliente_sel: None,
            cliente_busca: String::new(),
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

    fn faturar() -> Self {
        Self::Faturar {
            meio_pagamento: None,
            contas: Vec::new(),
            contas_carregadas: false,
            conta_escolhida: None,
            criando_conta: false,
            nova_conta_nome: String::new(),
        }
    }
}

/// Uma transição de estado simples (sem campo próprio) aguardando confirmação num dialog
/// separado — disparada pelos botões de `acoes_por_estado`, nunca executada no primeiro
/// clique.
#[derive(Debug, Clone, Copy)]
enum AcaoPendente {
    AprovarOrcamento(Id),
    ReprovarOrcamento(Id),
    ConcluirExecucao(Id),
    Desfaturar(Id),
    Reabrir(Id),
}

/// Qual aba da tela de OS está ativa.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AbaOs {
    #[default]
    Ordens,
    Orcamentos,
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
    erro: Option<String>,
    dlg: Dlg,
    /// Filtro da lista de ordens (nº, aparelho ou nome do cliente) — client-side, mesma
    /// decisão de `tela_estoque`/`tela_clientes` (a consulta já traz tudo de uma vez).
    busca: String,
    /// Filtro por status — `None` = `FiltroStatusOs::Ativas` (o padrão).
    filtro_status: Option<FiltroStatusOs>,
    ordenacao: Option<(usize, Direcao)>,
    /// `(ordem_servico, rótulo)` aguardando confirmação de exclusão (cancelamento).
    confirmar_exclusao: Option<(Id, String)>,
    /// Transição de estado simples (Aprovar/Reprovar/Concluir execução) aguardando
    /// confirmação — nenhuma dispara no primeiro clique.
    confirmar_acao: Option<AcaoPendente>,
    /// Campos do dialog "Editar dados da ordem" (`EditarDadosDaOrdem`).
    editar_equipamento: String,
    editar_complemento_defeito: String,

    laudo_problema: String,
    laudo_diagnostico: String,
    mao_de_obra_descricao: String,
    mao_de_obra_valor: String,
    peca_produto: Option<Id>,
    /// O texto digitado no lookup de peça (`SeletorBusca`) — hoje filtra por nome; fica
    /// pronto para somar o código curto de bancada assim que o backend o expuser (ver
    /// `TODO(backend)` em `adicionar_peca`/`corpo_detalhe`).
    peca_busca: String,
    peca_qtd: String,
    peca_preco: String,
    aprovador: String,
}

impl EstadoTelaOs {
    /// Recarrega a lista de ordens (qualquer estado — `TodasAsOrdens`, o filtro de status na
    /// tela decide o que aparece) e os catálogos de cliente e produto.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if self.filtro_status.is_none() {
            self.filtro_status = Some(FiltroStatusOs::Ativas);
        }
        match motor.consultar(sessao, "os.todas_as_ordens.v1", &TodasAsOrdens) {
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
                    if d.laudo.is_none() {
                        self.laudo_problema = d.ordem.defeito_relatado.clone();
                    }
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
        self.peca_busca.clear();
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
        },
        |ui, estado| {
            if let Some(nova) = Abas::nova(&[
                (AbaOs::Ordens, "Ordens de serviço"),
                (AbaOs::Orcamentos, "Orçamentos"),
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
            }
        },
    );

    match estado.aba {
        AbaOs::Ordens => match estado.dlg {
            Dlg::Fechado => {}
            Dlg::Nova { .. } => dialogo_nova(ui.ctx(), motor, sessao, estado),
            Dlg::Detalhe => dialogo_detalhe(ui.ctx(), motor, sessao, estado),
            Dlg::EditarDados(id) => dialogo_editar_dados(ui.ctx(), motor, sessao, estado, id),
            Dlg::Faturar { .. } => dialogo_faturar(ui.ctx(), motor, sessao, estado),
        },
        AbaOs::Orcamentos => {
            crate::tela_orcamentos::dialogos(ui.ctx(), motor, sessao, &mut estado.orc);
        }
    }

    if let Some((ordem_servico, rotulo)) = estado.confirmar_exclusao.clone() {
        let resposta = cardeal_ui::organisms::dialogo_confirmacao(
            ui.ctx(),
            "Excluir OS",
            &format!(
                "Tem certeza que quer excluir a OS \"{rotulo}\"? Ela vai pra Cancelada — \
                 peças já aplicadas voltam pro estoque automaticamente, quando possível.",
            ),
            "Excluir",
        );
        if resposta.confirmado {
            match motor.executar(
                sessao,
                "os.cancelar_ordem_servico.v1",
                &CancelarOrdemServico { ordem_servico },
            ) {
                Ok(r) => {
                    let r: OrdemServicoCancelada = r;
                    estado.carregar(motor, sessao);
                    let msg = if r.pecas_pendentes_de_estorno_manual > 0 {
                        format!(
                            "OS excluída — {} peça(s) precisam de correção manual de estoque",
                            r.pecas_pendentes_de_estorno_manual
                        )
                    } else {
                        "OS excluída".to_owned()
                    };
                    notificar(ui.ctx(), Notificacao::sucesso(msg));
                }
                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
            }
        }
        if resposta.fechar {
            estado.confirmar_exclusao = None;
        }
    }

    if let Some(acao) = estado.confirmar_acao {
        let (titulo, mensagem, rotulo_acao) = match acao {
            AcaoPendente::AprovarOrcamento(_) => (
                "Aprovar orçamento",
                "Confirma a aprovação deste orçamento pelo cliente? A OS libera para \
                 execução."
                    .to_owned(),
                "Aprovar",
            ),
            AcaoPendente::ReprovarOrcamento(_) => (
                "Reprovar orçamento",
                "Confirma a reprovação? A OS encerra sem execução — esta ação não tem volta."
                    .to_owned(),
                "Reprovar",
            ),
            AcaoPendente::ConcluirExecucao(_) => (
                "Concluir execução",
                "Confirma que o reparo terminou? A OS fica pronta para faturar.".to_owned(),
                "Concluir",
            ),
            AcaoPendente::Desfaturar(_) => (
                "Desfaturar OS",
                "Confirma desfaturar esta OS? O título gerado no financeiro é cancelado \
                 (qualquer recebimento já dado é estornado) e a OS volta a aceitar edição de \
                 valores e serviços."
                    .to_owned(),
                "Desfaturar",
            ),
            AcaoPendente::Reabrir(_) => (
                "Reabrir OS",
                "Confirma reabrir esta OS cancelada? Ela volta para \"Aberta\" e aceita edição \
                 normalmente."
                    .to_owned(),
                "Reabrir",
            ),
        };
        let resposta =
            cardeal_ui::organisms::dialogo_confirmacao(ui.ctx(), titulo, &mensagem, rotulo_acao);
        if resposta.confirmado {
            match acao {
                AcaoPendente::AprovarOrcamento(ordem_servico) => aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.aprovar_orcamento.v1",
                    &AprovarOrcamentoOs {
                        ordem_servico,
                        identificacao_aprovador: estado.aprovador.clone(),
                    },
                    "Orçamento aprovado",
                ),
                AcaoPendente::ReprovarOrcamento(ordem_servico) => aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.reprovar_orcamento.v1",
                    &ReprovarOrcamentoOs { ordem_servico },
                    "Orçamento reprovado",
                ),
                AcaoPendente::ConcluirExecucao(ordem_servico) => aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.concluir_execucao.v1",
                    &ConcluirExecucao { ordem_servico },
                    "Execução concluída",
                ),
                AcaoPendente::Desfaturar(ordem_servico) => aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.desfaturar_ordem_servico.v1",
                    &DesfaturarOrdemServico { ordem_servico },
                    "OS desfaturada — título removido do financeiro",
                ),
                AcaoPendente::Reabrir(ordem_servico) => aplicar_e_recarregar(
                    ui.ctx(),
                    motor,
                    sessao,
                    estado,
                    ordem_servico,
                    "os.reabrir_ordem_servico.v1",
                    &ReabrirOrdemServico { ordem_servico },
                    "OS reaberta",
                ),
            }
        }
        if resposta.fechar {
            estado.confirmar_acao = None;
        }
    }
}

/// Se o comando `CancelarOrdemServico` aceita a OS neste estado — mesma regra de
/// `OrdemServico::cancelar` (`docs/modulos/os.md` §11 regra 5): qualquer estado anterior a
/// `Concluida`, exceto os já terminais.
const fn pode_cancelar(estado: EstadoOs) -> bool {
    matches!(
        estado,
        EstadoOs::Aberta
            | EstadoOs::EmDiagnostico
            | EstadoOs::AguardandoAprovacao
            | EstadoOs::Aprovada
            | EstadoOs::EmExecucao
    )
}

/// Verdadeiro para qualquer estado que não seja terminal — a mesma regra de
/// `mod_os::ordens_nao_finalizadas` (backend), replicada aqui porque a tela carrega
/// `TodasAsOrdens` de uma vez e filtra localmente.
const fn nao_finalizada(estado: EstadoOs) -> bool {
    !matches!(
        estado,
        EstadoOs::Faturada | EstadoOs::Cancelada | EstadoOs::Reprovada
    )
}

/// O filtro de status da listagem de OS. Pedido explícito do usuário (2026-09-14): depois de
/// faturar, a OS "sumia" da lista — precisa dar pra ver qualquer status, não só a fila ativa.
/// `Ativas` é o padrão (mesmo recorte de antes, quando a tela só sabia mostrar isso).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FiltroStatusOs {
    Ativas,
    Todos,
    Um(EstadoOs),
}

impl FiltroStatusOs {
    fn combina(self, estado: EstadoOs) -> bool {
        match self {
            Self::Ativas => nao_finalizada(estado),
            Self::Todos => true,
            Self::Um(e) => estado == e,
        }
    }
}

fn dialogo_editar_dados(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    ordem_servico: Id,
) {
    let enter =
        ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.memory(|m| m.any_popup_open());

    let fechar = Dialogo::nova("Editar dados da ordem")
        .largura(560.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                ui.add(Campo::novo("Aparelho", &mut estado.editar_equipamento));
                ui.add_space(Espaco::E8);
                ui.add(
                    Campo::novo(
                        "Complementar defeito relatado (opcional)",
                        &mut estado.editar_complemento_defeito,
                    )
                    .marcador("acrescenta ao relato original, não substitui"),
                );
            },
            |ui, estado| {
                let clicou = ui.add(Botao::primario("Salvar")).clicked();
                if clicou || enter {
                    let equipamento = (!estado.editar_equipamento.trim().is_empty())
                        .then(|| estado.editar_equipamento.clone());
                    let complemento = (!estado.editar_complemento_defeito.trim().is_empty())
                        .then(|| estado.editar_complemento_defeito.clone());
                    match motor.executar(
                        sessao,
                        "os.editar_dados_da_ordem.v1",
                        &EditarDadosDaOrdem {
                            ordem_servico,
                            equipamento,
                            complemento_defeito_relatado: complemento,
                        },
                    ) {
                        Ok(()) => {
                            estado.dlg = Dlg::Fechado;
                            estado.carregar(motor, sessao);
                            notificar(ui.ctx(), Notificacao::sucesso("Ordem atualizada"));
                        }
                        Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                    }
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

/// Faturar uma OS — disponível desde a abertura (`Dlg::faturar`, botão no rodapé de
/// `dialogo_detalhe`). Pede o meio de pagamento (Dinheiro/Pix/Cartão) quando há cobrança —
/// Dinheiro não precisa de mais nada (vai pro Caixa); Pix/cartão pedem uma conta bancária, e
/// se a empresa ainda não tem nenhuma, o miniformulário "Nova conta bancária" já abre no
/// lugar do seletor. Fatura e recebe no mesmo clique (`FaturarOrdemServico::pago_no_ato`) —
/// não existe mais um passo separado de "ir na tela de Financeiro depois pra dar baixa".
fn dialogo_faturar(
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

    if let Dlg::Faturar {
        contas_carregadas: false,
        ..
    } = &estado.dlg
    {
        let todas: Vec<ItemContaDisponivel> = motor
            .consultar(
                sessao,
                "financeiro.contas_disponiveis.v1",
                &ContasDisponiveis,
            )
            .unwrap_or_default();
        let bancos: Vec<ItemContaDisponivel> = todas.into_iter().filter(|c| !c.e_caixa).collect();
        if let Dlg::Faturar {
            contas,
            contas_carregadas,
            conta_escolhida,
            criando_conta,
            ..
        } = &mut estado.dlg
        {
            *contas_carregadas = true;
            if bancos.is_empty() {
                *criando_conta = true;
            } else if bancos.len() == 1 {
                *conta_escolhida = Some(bancos[0].conta);
            }
            *contas = bancos;
        }
    }

    let enter =
        ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.memory(|m| m.any_popup_open());
    let titulo = format!("Faturar OS #{} · {}", os.numero, equipamento_label(&os));

    let fechar = Dialogo::nova(titulo).largura(480.0).mostrar(
        ctx,
        estado,
        |ui, estado| {
            ui.horizontal(|ui| {
                ui.add(Rotulo::campo("Total a faturar"));
                ui.add(ValorDinheiro::novo(os.valor_total));
            });

            if !os.valor_total.e_positivo() {
                ui.add_space(Espaco::E12);
                ui.add(
                    Rotulo::interface(
                        "Serviço sem cobrança (garantia/cortesia) — faturar só fecha o \
                         ciclo, sem gerar título.",
                    )
                    .quebravel(),
                );
                return;
            }

            let Dlg::Faturar {
                meio_pagamento,
                contas,
                conta_escolhida,
                criando_conta,
                nova_conta_nome,
                ..
            } = &mut estado.dlg
            else {
                return;
            };

            ui.add_space(Espaco::E12);
            ui.add(Rotulo::campo("Meio de pagamento"));
            ui.add_space(Espaco::E4);
            ui.horizontal(|ui| {
                for (valor, rotulo) in [
                    (MeioPagamento::Dinheiro, "Dinheiro"),
                    (MeioPagamento::Pix, "Pix"),
                    (MeioPagamento::Cartao, "Cartão"),
                ] {
                    let botao = if *meio_pagamento == Some(valor) {
                        Botao::primario(rotulo)
                    } else {
                        Botao::fantasma(rotulo)
                    };
                    if ui.add(botao).clicked() {
                        *meio_pagamento = Some(valor);
                    }
                }
            });

            if !matches!(
                meio_pagamento,
                Some(MeioPagamento::Pix) | Some(MeioPagamento::Cartao)
            ) {
                return;
            }
            ui.add_space(Espaco::E12);
            if *criando_conta || contas.is_empty() {
                ui.add(Rotulo::campo(if contas.is_empty() {
                    "Nenhuma conta bancária cadastrada — crie uma"
                } else {
                    "Nova conta bancária"
                }));
                ui.add_space(Espaco::E4);
                ui.horizontal(|ui| {
                    ui.add(
                        Campo::novo("", nova_conta_nome).marcador("ex.: Nubank, Banco do Brasil"),
                    );
                    if ui.add(Botao::primario("Criar")).clicked() {
                        if nova_conta_nome.trim().is_empty() {
                            notificar(ui.ctx(), Notificacao::aviso("Informe o nome da conta."));
                        } else {
                            match motor.executar(
                                sessao,
                                "financeiro.criar_conta_bancaria.v1",
                                &CriarContaBancaria {
                                    nome: nova_conta_nome.clone(),
                                },
                            ) {
                                Ok(c) => {
                                    let c: ContaBancariaCriada = c;
                                    contas.push(ItemContaDisponivel {
                                        conta: c.conta,
                                        codigo: c.codigo,
                                        nome: nova_conta_nome.clone(),
                                        saldo: Dinheiro::ZERO,
                                        e_caixa: false,
                                    });
                                    *conta_escolhida = Some(c.conta);
                                    *criando_conta = false;
                                    nova_conta_nome.clear();
                                    notificar(ui.ctx(), Notificacao::sucesso("Conta criada"));
                                }
                                Err(e) => notificar(ui.ctx(), Notificacao::erro(e.mensagem)),
                            }
                        }
                    }
                });
                if !contas.is_empty() && ui.add(Botao::fantasma("Usar conta existente")).clicked() {
                    *criando_conta = false;
                }
            } else {
                SeletorOpcao::novo("Conta bancária", conta_escolhida)
                    .opcoes(
                        contas
                            .iter()
                            .map(|c| (c.conta, format!("{} ({})", c.nome, c.codigo))),
                    )
                    .mostrar(ui);
                ui.add_space(Espaco::E4);
                if ui.add(Botao::fantasma("+ Nova conta").pequeno()).clicked() {
                    *criando_conta = true;
                }
            }
        },
        |ui, estado| {
            if ui.add(Botao::primario("Faturar")).clicked() || enter {
                faturar_os(ui.ctx(), motor, sessao, estado, &os);
            }
            if ui.add(Botao::secundario("Cancelar")).clicked() {
                estado.dlg = Dlg::Detalhe;
            }
        },
    );
    if fechar {
        estado.dlg = Dlg::Detalhe;
    }
}

/// Executa `FaturarOrdemServico` com o que foi escolhido em `Dlg::Faturar` — meio de
/// pagamento obrigatório quando há cobrança (`os.valor_total > 0`), sempre 1x à vista e já
/// baixado (`pago_no_ato`): fatura e recebe são o mesmo clique.
fn faturar_os(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
    os: &OrdemServico,
) {
    let pago_no_ato = if os.valor_total.e_positivo() {
        let Dlg::Faturar {
            meio_pagamento,
            conta_escolhida,
            ..
        } = &estado.dlg
        else {
            return;
        };
        let Some(meio_pagamento) = *meio_pagamento else {
            notificar(ctx, Notificacao::aviso("Escolha o meio de pagamento."));
            return;
        };
        Some(PagamentoNoAto {
            meio_pagamento,
            conta_destino: *conta_escolhida,
        })
    } else {
        None
    };

    match motor.executar(
        sessao,
        "os.faturar_ordem_servico.v1",
        &FaturarOrdemServico {
            ordem_servico: os.id,
            parcelas: 1,
            primeiro_vencimento: Data::hoje(Fuso::BRASILIA),
            intervalo_dias: 0,
            pago_no_ato,
        },
    ) {
        Ok(f) => {
            let f: OrdemServicoFaturada = f;
            estado.carregar(motor, sessao);
            estado.abrir_detalhe(motor, sessao, os.id);
            estado.dlg = Dlg::Detalhe;
            let msg = if f.titulo_pago {
                "OS faturada e recebida"
            } else if f.titulo.is_some() {
                "OS faturada"
            } else {
                "OS faturada (sem cobrança)"
            };
            notificar(ctx, Notificacao::sucesso(msg));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
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

    // Três agregados que a lista já carrega e antes ficavam perdidos entre as linhas —
    // vira cabeçalho de indicador visual (`docs/12-ui-ux.md` §6): total de OS abertas, valor
    // parado e quantas estão esperando uma decisão do cliente (fila de cobrança/follow-up).
    // Sempre sobre a fila ativa (`nao_finalizada`), não sobre o filtro de status escolhido —
    // é saúde do pipeline, não uma contagem do que a grade está mostrando agora.
    let ativas: Vec<&OrdemServico> = estado
        .ordens
        .iter()
        .filter(|os| nao_finalizada(os.estado))
        .collect();
    let valor_parado = ativas
        .iter()
        .fold(Dinheiro::ZERO, |acc, os| acc + os.valor_total);
    let aguardando_cliente = ativas
        .iter()
        .filter(|os| os.estado == EstadoOs::AguardandoAprovacao)
        .count();
    FaixaKpi::nova(vec![
        CartaoKpi::contagem("Ordens em aberto", ativas.len()),
        CartaoKpi::novo("Valor em aberto", valor_parado).variacao("soma do total de cada OS"),
        CartaoKpi::contagem("Aguardando aprovação", aguardando_cliente)
            .variacao("orçamento com o cliente"),
    ])
    .mostrar(ui);

    ui.horizontal(|ui| {
        ui.set_max_width(360.0);
        ui.add(Campo::novo("", &mut estado.busca).marcador("Buscar por nº, aparelho ou cliente"));
        ui.add_space(Espaco::E12);
        SeletorOpcao::novo("Status", &mut estado.filtro_status)
            .opcao(FiltroStatusOs::Ativas, "Ativas (padrão)")
            .opcao(FiltroStatusOs::Todos, "Todas")
            .opcoes(
                [
                    EstadoOs::Aberta,
                    EstadoOs::EmDiagnostico,
                    EstadoOs::AguardandoAprovacao,
                    EstadoOs::Aprovada,
                    EstadoOs::EmExecucao,
                    EstadoOs::Concluida,
                    EstadoOs::Faturada,
                    EstadoOs::Cancelada,
                    EstadoOs::Reprovada,
                ]
                .map(|e| (FiltroStatusOs::Um(e), estado_etiqueta(e).0)),
            )
            .mostrar(ui);
    });
    ui.add_space(Espaco::E12);

    let filtro_status = estado.filtro_status.unwrap_or(FiltroStatusOs::Ativas);
    let termo = estado.busca.trim().to_lowercase();
    let indices: Vec<usize> = (0..estado.ordens.len())
        .filter(|&i| {
            let os = &estado.ordens[i];
            if !filtro_status.combina(os.estado) {
                return false;
            }
            if termo.is_empty() {
                return true;
            }
            os.numero.to_string().contains(&termo)
                || equipamento_label(os).to_lowercase().contains(&termo)
                || nome_cliente(&estado.clientes, os.cliente)
                    .to_lowercase()
                    .contains(&termo)
        })
        .collect();
    if indices.is_empty() {
        ui.add(
            Rotulo::interface("Nenhuma ordem para essa busca/filtro de status.")
                .cor(ui.cores().texto_medio),
        );
        return;
    }

    let colunas = vec![
        ColunaGrade::nova("Nº").largura(56.0).numero(),
        ColunaGrade::nova("Aparelho"),
        ColunaGrade::nova("Cliente").largura(180.0),
        ColunaGrade::nova("Estado").largura(170.0),
        ColunaGrade::nova("Total").largura(110.0).numero(),
        ColunaGrade::nova("Ações").largura(190.0),
    ];
    let mut editar_clicado = None;
    let mut excluir_clicado = None;
    let resposta = Grade::nova(colunas)
        .selecionavel(None)
        .ordenacao(estado.ordenacao)
        // Linha mais alta que o padrão (38px) — a coluna "Ações" carrega botões de 34px de
        // altura mínima, que ficavam praticamente colados nas bordas da linha sem isso.
        .altura_linha(cardeal_ui::tokens::AlturaLinha::Toque)
        .mostrar(ui, indices.len(), |i, row| {
            let os = &estado.ordens[indices[i]];
            row.col(|ui| {
                ui.add(Rotulo::interface(os.numero.to_string()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(equipamento_label(os).to_owned()));
            });
            row.col(|ui| {
                ui.add(Rotulo::interface(nome_cliente(
                    &estado.clientes,
                    os.cliente,
                )));
            });
            row.col(|ui| {
                let (rotulo, tom) = estado_etiqueta(os.estado);
                ui.add(Etiqueta::nova(rotulo, tom));
            });
            row.col(|ui| {
                ui.add(ValorDinheiro::novo(os.valor_total));
            });
            row.col(|ui| {
                let rubro = ui.cores().rubro;
                ui.horizontal(|ui| {
                    if ui.add(Botao::fantasma("Editar").pequeno().cor(rubro)).clicked() {
                        editar_clicado = Some((os.id, os.equipamento.clone()));
                    }
                    if pode_cancelar(os.estado)
                        && ui.add(Botao::destrutivo("Excluir").pequeno()).clicked()
                    {
                        excluir_clicado = Some((os.id, equipamento_label(os).to_owned()));
                    }
                });
            });
        });

    if let Some(coluna) = resposta.coluna_clicada {
        let direcao = match estado.ordenacao {
            Some((atual, direcao)) if atual == coluna => direcao.invertida(),
            _ => Direcao::Ascendente,
        };
        estado.ordenacao = Some((coluna, direcao));
        ordenar_ordens(&mut estado.ordens, &estado.clientes, coluna, direcao);
    }
    if let Some((id, equipamento)) = editar_clicado {
        estado.editar_equipamento = equipamento;
        estado.editar_complemento_defeito.clear();
        estado.dlg = Dlg::EditarDados(id);
    } else if let Some(alvo) = excluir_clicado {
        estado.confirmar_exclusao = Some(alvo);
    } else if let Some(i) = resposta.linha_clicada {
        let id = estado.ordens[indices[i]].id;
        estado.abrir_detalhe(motor, sessao, id);
    }
}

/// O texto que a lista mostra na coluna "Aparelho". `equipamento` é obrigatório desde
/// 2026-09-14 para OS novas, mas ordens abertas antes disso podem ter ficado sem — daí o
/// placeholder, em vez de cair no defeito relatado (que já confundiu o balcão, que lia o
/// problema do cliente como se fosse o nome do aparelho).
fn equipamento_label(os: &OrdemServico) -> &str {
    if os.equipamento.trim().is_empty() {
        "sem aparelho registrado"
    } else {
        &os.equipamento
    }
}

/// O nome do cliente dono da ordem, a partir do catálogo já carregado (`estado.clientes`) —
/// `OrdemServico` só guarda o `Id`.
fn nome_cliente(clientes: &[ItemPessoa], cliente: Id) -> &str {
    clientes
        .iter()
        .find(|c| c.pessoa == cliente)
        .map_or("Cliente", |c| c.nome.as_str())
}

/// Rótulo em português + tom da etiqueta de status para a `Grade` — mesma ideia de
/// `situacao_amigavel` (usada no PDF), só que mais curta para caber numa pílula de grade.
const fn estado_etiqueta(e: EstadoOs) -> (&'static str, Tom) {
    match e {
        EstadoOs::Aberta => ("Aberta", Tom::Info),
        EstadoOs::EmDiagnostico => ("Em diagnóstico", Tom::Info),
        EstadoOs::AguardandoAprovacao => ("Aguardando aprovação", Tom::Atencao),
        EstadoOs::Aprovada => ("Aprovada", Tom::Info),
        EstadoOs::Reprovada => ("Reprovada", Tom::Negativo),
        EstadoOs::EmExecucao => ("Em execução", Tom::Atencao),
        EstadoOs::Concluida => ("Pronta p/ faturar", Tom::Atencao),
        EstadoOs::Faturada => ("Faturada", Tom::Positivo),
        EstadoOs::Cancelada => ("Cancelada", Tom::Negativo),
    }
}

/// Ordena `ordens` pela coluna clicada no cabeçalho da [`Grade`] (Nº, Aparelho, Cliente,
/// Estado, Total).
fn ordenar_ordens(
    ordens: &mut [OrdemServico],
    clientes: &[ItemPessoa],
    coluna: usize,
    direcao: Direcao,
) {
    ordens.sort_by(|a, b| {
        let ordem = match coluna {
            0 => a.numero.cmp(&b.numero),
            1 => equipamento_label(a).cmp(equipamento_label(b)),
            2 => nome_cliente(clientes, a.cliente).cmp(nome_cliente(clientes, b.cliente)),
            3 => estado_etiqueta(a.estado).0.cmp(estado_etiqueta(b.estado).0),
            4 => a.valor_total.cmp(&b.valor_total),
            _ => std::cmp::Ordering::Equal,
        };
        match direcao {
            Direcao::Ascendente => ordem,
            Direcao::Descendente => ordem.reverse(),
        }
    });
}

fn dialogo_nova(
    ctx: &egui::Context,
    motor: &MotorLocal,
    sessao: &SessaoLocal,
    estado: &mut EstadoTelaOs,
) {
    let opcoes_cliente: Vec<OpcaoBusca<Id>> = estado
        .clientes
        .iter()
        .map(|c| {
            let opcao = OpcaoBusca::nova(c.pessoa, c.nome.clone());
            match &c.documento {
                Some(doc) => opcao.subtitulo(doc.clone()),
                None => opcao,
            }
        })
        .collect();
    // Enter confirma "Abrir OS" — o mesmo botão que já valida (`abrir_os` mostra o aviso
    // certo se faltar cliente/defeito). Não dispara dentro de um popup aberto (ex.: o combo
    // de UF do endereço).
    let enter =
        ctx.input(|i| i.key_pressed(egui::Key::Enter)) && !ctx.memory(|m| m.any_popup_open());

    let fechar = Dialogo::nova("Nova ordem de serviço")
        .largura(640.0)
        .mostrar(
            ctx,
            estado,
            |ui, estado| {
                let Dlg::Nova {
                    cliente_novo,
                    cliente_sel,
                    cliente_busca,
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
                    // Endereço é o bloco mais raramente preenchido na recepção (o cliente só
                    // quer deixar o aparelho) — fica recolhido para não competir com nome e
                    // defeito relatado, que são o que de fato importa pra abrir rápido.
                    SecaoExpansivel::nova("Endereço (opcional)").mostrar(ui, |ui| {
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
                    });
                } else {
                    SeletorBusca::novo("Cliente", cliente_busca, cliente_sel)
                        .opcoes(opcoes_cliente)
                        .marcador("Buscar por nome ou documento…")
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
                    Campo::novo("Aparelho — o que o cliente trouxe para reparo", equipamento)
                        .marcador("ex.: Furadeira Bosch GSB 13"),
                );
            },
            |ui, estado| {
                if ui.add(Botao::primario("Abrir OS")).clicked() || enter {
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
        cliente_busca: _,
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

    if equipamento.trim().is_empty() {
        notificar(
            ctx,
            Notificacao::aviso("Informe o aparelho trazido para reparo."),
        );
        return;
    }
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
    let titulo = format!("OS #{} · {}", os.numero, equipamento_label(&os));

    let fechar = Dialogo::nova(titulo).mostrar(
        ctx,
        estado,
        |ui, estado| corpo_detalhe(ui, motor, sessao, estado, &detalhe),
        |ui, estado| {
            // A ponta final do fluxo do técnico — laudo/orçamento/execução terminam aqui: o
            // comprovante em PDF pra entregar/mandar ao cliente, disponível em qualquer
            // estado (`gerar_pdf` já cobre da entrada à quitação). Ação de peso, então
            // primário — igual à ação de workflow do estado atual, mas em outra zona da tela
            // (rodapé fixo, sempre visível, em vez de escondida no fim do corpo rolável). O
            // rodapé desenha da direita para a esquerda (`Dialogo::mostrar`), então o botão
            // adicionado primeiro fica mais à direita — a mesma convenção de "ação primária
            // primeiro" já usada no resto da tela.
            // Faturar disponível desde a abertura, ao lado de Comprovante/Fechar — pedido
            // explícito do usuário (2026-09-15): laudo/orçamento/aprovação/execução são
            // trâmite opcional, não um portão pro faturamento. Some só nos estados terminais
            // onde faturar já não faz sentido (`OrdemServico::faturar` recusaria de qualquer
            // forma; a UI só evita oferecer uma ação que o backend já vai rejeitar).
            if pode_faturar(os.estado) && ui.add(Botao::primario("Faturar")).clicked() {
                estado.dlg = Dlg::faturar();
            }
            if ui.add(Botao::primario("Comprovante (PDF)")).clicked() {
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

/// Se `FaturarOrdemServico` aceita a OS neste estado — qualquer um menos os três terminais
/// (`OrdemServico::faturar`, 2026-09-15).
const fn pode_faturar(estado: EstadoOs) -> bool {
    !matches!(
        estado,
        EstadoOs::Faturada | EstadoOs::Cancelada | EstadoOs::Reprovada
    )
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
        let (rotulo, tom) = estado_etiqueta(os.estado);
        ui.add(Etiqueta::nova(rotulo, tom));
        if pode_cancelar(os.estado) && ui.add(Botao::destrutivo("Cancelar OS").pequeno()).clicked()
        {
            estado.confirmar_exclusao = Some((os.id, equipamento_label(os).to_owned()));
        }
        if os.estado == EstadoOs::Faturada
            && ui.add(Botao::secundario("Desfaturar").pequeno()).clicked()
        {
            estado.confirmar_acao = Some(AcaoPendente::Desfaturar(os.id));
        }
        if os.estado == EstadoOs::Cancelada
            && ui.add(Botao::secundario("Reabrir OS").pequeno()).clicked()
        {
            estado.confirmar_acao = Some(AcaoPendente::Reabrir(os.id));
        }
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
        // TODO(backend): a busca aqui compara só o nome do produto — quando o código curto de
        // rastreabilidade (post-it físico na peça) existir em `ItemProdutoComSaldo`, somar
        // `p.codigo_curto` ao `subtitulo` abaixo é o suficiente para o técnico bipar/digitar o
        // código de bancada em vez de catar o nome numa lista de centenas de peças.
        let opcoes_peca: Vec<OpcaoBusca<Id>> = estado
            .produtos
            .iter()
            .map(|p| {
                OpcaoBusca::nova(p.produto, p.nome.clone())
                    .subtitulo(format!("{} disponível(is)", p.disponivel))
            })
            .collect();
        SeletorBusca::novo(
            "Peça do estoque",
            &mut estado.peca_busca,
            &mut estado.peca_produto,
        )
        .opcoes(opcoes_peca)
        .marcador("Buscar peça por nome…")
        .mostrar(ui);
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
            estado.peca_busca.clear();
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
                    if estado.aprovador.trim().is_empty() {
                        notificar(
                            ui.ctx(),
                            Notificacao::aviso("Informe quem aprovou (nome e documento)."),
                        );
                    } else {
                        estado.confirmar_acao = Some(AcaoPendente::AprovarOrcamento(os.id));
                    }
                }
                if ui.add(Botao::destrutivo("Reprovar")).clicked() {
                    estado.confirmar_acao = Some(AcaoPendente::ReprovarOrcamento(os.id));
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
                estado.confirmar_acao = Some(AcaoPendente::ConcluirExecucao(os.id));
            }
        }
        EstadoOs::Faturada
        | EstadoOs::Cancelada
        | EstadoOs::Reprovada
        | EstadoOs::Aberta
        | EstadoOs::EmDiagnostico
        | EstadoOs::Concluida => {}
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
