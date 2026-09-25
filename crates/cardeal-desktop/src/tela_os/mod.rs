//! Tela de Ordens de Serviço — lista + dialog (criar / ver / agir). `docs/modulos/os.md`.
//!
//! Padrão de UI do projeto: a tela abre na lista inteira; "Nova OS" e o clique numa linha
//! abrem o mesmo `Dialogo`. OS é workflow (máquina de estados), então o dialog de detalhe
//! mostra as infos + a ação certa pro estado atual, em vez de um "Editar" genérico.

mod andamento;
mod apontamento;
mod confirmacoes;
mod detalhe;
mod encomenda;
mod faturar;
mod ficha;
mod lista;
mod nova;
mod orcamento;
mod pdf;
mod resumo;
#[cfg(test)]
mod testes;

use andamento::*;
use apontamento::*;
use cardeal_cliente::{IdentidadeVisual, MotorLocal, SessaoLocal, UsuarioResumo};
use cardeal_kernel::{Data, Dinheiro, Fuso, Id, Instante, Percentual, Preco, Quantidade};
use cardeal_modkit::Icone;
use cardeal_pdf::{gerar_comprovante_os, ComprovanteOsPdf, IdentidadeEmpresa, ItemPdf};
use cardeal_ui::atoms::{Botao, Divisor, Etiqueta, Rotulo, Tom, ValorDinheiro, ATALHO_NOVO};
use cardeal_ui::molecules::{
    Abas, AcaoRegistro, AcoesRegistro, BarraFiltros, Campo, CartaoKpi, EstadoVazio, Mascara,
    OpcaoBusca, SecaoExpansivel, SeletorBusca, SeletorOpcao,
};
use cardeal_ui::organisms::{
    notificar, ColunaGrade, Dialogo, Direcao, FaixaKpi, Gaveta, Grade, LayoutTela, Notificacao,
    Ordenacao,
};
use cardeal_ui::tokens::{Espaco, TemaUi};
use confirmacoes::*;
use detalhe::*;
use eframe::egui;
use encomenda::*;
use faturar::*;
use ficha::*;
use lista::*;
use mod_clientes::{
    ContatoInicial, CriarPessoa, EnderecoInicial, ItemPessoa, Papel as PapelCliente,
    PessoasPorPapel, TipoContato, TipoDocumento, TipoEndereco, TipoPessoa,
};
use mod_estoque::{ItemLocal, ItemProdutoComSaldo, Locais, ProdutosComSaldo};
use mod_financeiro::{MeioPagamento, Titulo, TituloDaOrigem};
use mod_os::{
    AbrirOrdemComClienteNovo, AbrirOrdemServico, AplicarPecas, ApontamentoDeTempo,
    ApontamentosDaOrdem, AprovarOrcamentoOs, BuscarDetalheOrdem, BuscarOrdens,
    CancelarOrdemServico, ConcluirExecucao, DesfaturarOrdemServico, DetalheOrdem,
    EditarDadosDaOrdem, EncerrarApontamento, EnviarParaAprovacao, EstadoOs, FaturarOrdemServico,
    FiltroEstadoOs, HistoricoDoEquipamento, IniciarApontamento, IniciarExecucao, ItemOrcamentoNovo,
    MontarOrcamentoOs, OrdemComClienteNovoAberta, OrdemServico, OrdemServicoAberta,
    OrdemServicoCancelada, OrdemServicoFaturada, OrdensEmAberto, PagamentoNoAto, PecaFoiAplicada,
    ReabrirOrdemServico, RegistrarLaudo, RemoverItemOrcamento, ReprovarOrcamentoOs,
    TempoTotalDaOrdem, TipoItemOrcamento,
};
use nova::*;
use orcamento::*;
use pdf::*;
use resumo::*;

/// Qual dialog está aberto.
#[derive(Default)]
// Uma única instância por tela (o diálogo aberto): o tamanho da variante "Nova" não custa nada,
// e encaixotar os campos só espalharia `Box` pelo formulário.
#[allow(clippy::large_enum_variant)]
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
        /// Previsão de entrega, nº de série e acessórios (opcionais).
        ficha: FormFicha,
    },
    Detalhe,
    /// "Editar dados da ordem" (`EditarDadosDaOrdem`) — corrigir o aparelho e/ou
    /// complementar o defeito relatado de uma OS já aberta.
    EditarDados(Id),
    /// Faturar a OS aberta em `EstadoTelaOs::detalhe` — disponível desde a abertura, não só
    /// depois do trâmite completo (pedido explícito do usuário, 2026-09-15). À vista (fatura e
    /// recebe no mesmo clique, o caso do balcão) ou a prazo (gera as parcelas em aberto no
    /// financeiro — cliente empresa, parcelamento).
    Faturar(crate::pagamento::EstadoPagamento),
    /// Pedir uma peça ao fornecedor (`os.encomendar_peca.v1`).
    Encomendar(Box<FormEncomenda>),
    /// A peça chegou: entrada, pagamento e aplicação (`os.registrar_chegada_da_peca.v1`).
    Chegou(Box<FormChegada>),
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
            ficha: FormFicha::default(),
        }
    }

    fn faturar() -> Self {
        Self::Faturar(crate::pagamento::EstadoPagamento::novo(
            MeioPagamento::Dinheiro,
        ))
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

/// Quantas OS a lista mostra por vez. A busca acontece no motor, então uma OS antiga é
/// sempre alcançável digitando o número, o aparelho ou o cliente.
const LIMITE_LISTA: u32 = 300;

/// Qual aba da tela de OS está ativa.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum AbaOs {
    #[default]
    Ordens,
    Orcamentos,
    /// A lista de compras: peças sem saldo ou encomendadas, de todas as OS.
    Compras,
}

/// Estado local da tela — sobrevive entre quadros, não entre reinícios.
#[derive(Default)]
pub struct EstadoTelaOs {
    aba: AbaOs,
    orc: crate::tela_orcamentos::EstadoOrcamentos,
    /// O resultado da busca atual (`os.buscar_ordens.v1`, filtrada no motor).
    ordens: Vec<OrdemServico>,
    /// A fila ativa inteira, para os indicadores do topo — independe da busca/filtro.
    ativas: Vec<OrdemServico>,
    /// Peças sem saldo ou encomendadas (aba "Peças a comprar").
    pecas_a_comprar: Vec<mod_os::ItemAguardandoEstoque>,
    busca_compras: String,
    clientes: Vec<ItemPessoa>,
    produtos: Vec<ItemProdutoComSaldo>,
    /// Locais de estoque de onde as peças saem ao serem aplicadas na execução.
    locais: Vec<ItemLocal>,
    /// O local escolhido para aplicar peças; começa no primeiro (o comum é haver um só).
    aplicar_local: Option<Id>,
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
    ordenacao: Ordenacao,
    /// `(ordem_servico, rótulo)` aguardando confirmação de exclusão (cancelamento).
    confirmar_exclusao: Option<(Id, String)>,
    /// Transição de estado simples (Aprovar/Reprovar/Concluir execução) aguardando
    /// confirmação — nenhuma dispara no primeiro clique.
    confirmar_acao: Option<AcaoPendente>,
    /// Campos do dialog "Editar dados da ordem" (`EditarDadosDaOrdem`).
    editar_equipamento: String,
    editar_complemento_defeito: String,
    editar_ficha: FormFicha,
    /// Outras OS do mesmo cliente com aparelho parecido (`os.historico_do_equipamento.v1`).
    historico: Vec<OrdemServico>,
    /// Uma OS do histórico que o usuário clicou para abrir — aberta no fim do quadro, fora
    /// do diálogo que está sendo desenhado.
    pedido_abrir: Option<Id>,
    /// De onde veio o preço sugerido da peça ("tabela de preço", "último cobrado…").
    peca_preco_dica: String,

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
    /// Carga completa — ao entrar na tela ou no F5: catálogos (clientes, produtos, locais,
    /// usuários, identidade), orçamentos e a lista. Depois de uma ação na OS, use as
    /// recargas parciais abaixo: recarregar tudo a cada clique travava a tela.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if self.filtro_status.is_none() {
            self.filtro_status = Some(FiltroStatusOs::Ativas);
        }
        self.carregar_clientes(motor, sessao);
        self.carregar_produtos(motor, sessao);
        if let Ok(l) = motor.consultar(sessao, "estoque.locais.v1", &Locais) {
            self.locais = l;
            let ainda_existe = self
                .aplicar_local
                .is_some_and(|id| self.locais.iter().any(|l| l.id == id));
            if !ainda_existe {
                self.aplicar_local = self.locais.first().map(|l| l.id);
            }
        }
        if let Ok(u) = motor.usuarios() {
            self.usuarios = u;
        }
        if let Ok(i) = motor.identidade_visual() {
            self.identidade = Some(i);
        }
        self.orc.carregar(motor, sessao);
        // Depois dos clientes: a busca por nome resolve o termo contra esse catálogo.
        self.recarregar_lista(motor, sessao);
    }

    /// A fila ativa (indicadores) e a lista com a busca atual — o que muda depois de
    /// qualquer ação numa OS.
    fn recarregar_lista(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        match motor.consultar(sessao, "os.ordens_em_aberto.v1", &OrdensEmAberto) {
            Ok(ativas) => {
                self.ativas = ativas;
                self.erro = None;
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
        self.buscar_ordens(motor, sessao);
        if let Ok(p) = motor.consultar(
            sessao,
            "os.pecas_aguardando_estoque.v1",
            &mod_os::PecasAguardandoEstoque,
        ) {
            self.pecas_a_comprar = p;
        }
    }

    /// O catálogo de clientes (nome na lista, busca por nome, seletor da nova OS).
    fn carregar_clientes(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
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
    }

    /// Produtos com saldo — muda quando uma peça é aplicada ou volta ao estoque.
    fn carregar_produtos(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        if let Ok(p) = motor.consultar(sessao, "estoque.produtos_com_saldo.v1", &ProdutosComSaldo) {
            self.produtos = p;
        }
    }

    /// Refaz a lista com a busca e o filtro de status atuais — no motor, sem teto que
    /// esconda OS antigas. O nome do cliente é resolvido aqui (catálogo de `mod-clientes`) e
    /// vai como lista de ids.
    fn buscar_ordens(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        let termo = self.busca.trim();
        let digitos = cardeal_kernel::texto::somente_digitos(termo);
        let clientes = if termo.is_empty() {
            Vec::new()
        } else {
            self.clientes
                .iter()
                .filter(|c| {
                    cardeal_kernel::texto::casa_por_palavras(&c.nome, termo)
                        || (digitos.len() >= 4
                            && c.documento
                                .as_deref()
                                .is_some_and(|d| d.contains(digitos.as_str())))
                })
                .map(|c| c.pessoa)
                .collect()
        };
        let filtro = match self.filtro_status.unwrap_or(FiltroStatusOs::Ativas) {
            FiltroStatusOs::Ativas => FiltroEstadoOs::Ativas,
            FiltroStatusOs::Todos => FiltroEstadoOs::Todas,
            FiltroStatusOs::Um(e) => FiltroEstadoOs::Um(e),
        };
        match motor.consultar(
            sessao,
            "os.buscar_ordens.v1",
            &BuscarOrdens {
                filtro,
                termo: termo.to_owned(),
                clientes,
                limite: LIMITE_LISTA,
            },
        ) {
            Ok(ordens) => {
                self.ordens = ordens;
                if let Some((coluna, direcao)) = self.ordenacao.atual() {
                    ordenar_ordens(&mut self.ordens, &self.clientes, coluna, direcao);
                }
            }
            Err(e) => self.erro = Some(e.mensagem),
        }
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
                self.historico.clear();
                if let Some(d) = &self.detalhe {
                    if let Ok(h) = motor.consultar(
                        sessao,
                        "os.historico_do_equipamento.v1",
                        &HistoricoDoEquipamento {
                            cliente: d.ordem.cliente,
                            equipamento: d.ordem.equipamento.clone(),
                            excluir: Some(id),
                        },
                    ) {
                        self.historico = h;
                    }
                    // O formulário do laudo abre já preenchido: com o defeito relatado se ainda
                    // não há laudo, com o laudo atual se houver (para corrigi-lo).
                    match &d.laudo {
                        None => self.laudo_problema = d.ordem.defeito_relatado.clone(),
                        Some(l) => {
                            self.laudo_problema = l.descricao_problema.clone();
                            self.laudo_diagnostico = l.diagnostico.clone().unwrap_or_default();
                        }
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
        self.peca_preco_dica.clear();
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
                    .add(Botao::primario("+ Nova OS").tecla(ATALHO_NOVO))
                    .clicked()
                {
                    estado.dlg = Dlg::nova();
                }
            }
            AbaOs::Orcamentos => {
                if ui
                    .add(Botao::primario("+ Novo orçamento").tecla(ATALHO_NOVO))
                    .clicked()
                {
                    crate::tela_orcamentos::abrir_novo(&mut estado.orc);
                }
            }
            AbaOs::Compras => {}
        },
        |ui, estado| {
            let rotulo_compras = match estado.pecas_a_comprar.len() {
                0 => "Peças a comprar".to_owned(),
                n => format!("Peças a comprar ({n})"),
            };
            if let Some(nova) = Abas::nova(&[
                (AbaOs::Ordens, "Ordens de serviço"),
                (AbaOs::Orcamentos, "Orçamentos"),
                (AbaOs::Compras, rotulo_compras.as_str()),
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
                AbaOs::Compras => aba_compras(ui, estado),
            }
        },
    );

    match estado.aba {
        AbaOs::Ordens | AbaOs::Compras => match estado.dlg {
            Dlg::Fechado => {}
            Dlg::Nova { .. } => dialogo_nova(ui.ctx(), motor, sessao, estado),
            Dlg::Detalhe => {
                dialogo_detalhe(ui.ctx(), motor, sessao, estado);
                if let Some(id) = estado.pedido_abrir.take() {
                    estado.abrir_detalhe(motor, sessao, id);
                }
            }
            Dlg::EditarDados(id) => dialogo_editar_dados(ui.ctx(), motor, sessao, estado, id),
            Dlg::Faturar(_) => dialogo_faturar(ui.ctx(), motor, sessao, estado),
            Dlg::Encomendar(_) => dialogo_encomendar(ui.ctx(), motor, sessao, estado),
            Dlg::Chegou(_) => dialogo_chegada(ui.ctx(), motor, sessao, estado),
        },
        AbaOs::Orcamentos => {
            crate::tela_orcamentos::dialogos(ui.ctx(), motor, sessao, &mut estado.orc);
        }
    }

    confirmacoes(ui.ctx(), motor, sessao, estado);
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

/// O filtro de status da listagem de OS. Pedido explícito do usuário (2026-09-14): depois de
/// faturar, a OS "sumia" da lista — precisa dar pra ver qualquer status, não só a fila ativa.
/// `Ativas` é o padrão (mesmo recorte de antes, quando a tela só sabia mostrar isso).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FiltroStatusOs {
    Ativas,
    Todos,
    Um(EstadoOs),
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
            estado.recarregar_lista(motor, sessao);
            estado.abrir_detalhe(motor, sessao, ordem);
            estado.dlg = Dlg::Detalhe;
            notificar(ctx, Notificacao::sucesso(sucesso.to_owned()));
        }
        Err(e) => notificar(ctx, Notificacao::erro(e.mensagem)),
    }
}

// ── PDF ──────────────────────────────────────────────────────────────────────

#[cfg(feature = "demo")]
impl EstadoTelaOs {
    /// Abre o diálogo da cena de demonstração (`demo_app`).
    pub fn preparar_demo(&mut self, motor: &MotorLocal, sessao: &SessaoLocal, cena: &str) {
        self.carregar(motor, sessao);
        let primeira = self
            .ordens
            .iter()
            .find(|o| o.ficha.acessorios.contains("capinha"));
        let id = primeira.map_or_else(|| self.ordens[0].id, |o| o.id);
        match cena {
            "os-detalhe" => self.abrir_detalhe(motor, sessao, id),
            "os-nova" => self.dlg = Dlg::nova(),
            "os-compras" => self.aba = AbaOs::Compras,
            "os-faturar" => {
                self.abrir_detalhe(motor, sessao, id);
                self.dlg = Dlg::faturar();
            }
            _ => {}
        }
    }
}
