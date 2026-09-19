//! O estado da tela de PDV e os tipos que ele carrega (linha do cupom, diálogo aberto,
//! ação pedida).

use super::*;

/// Uma linha do carrinho, reconstruída dos retornos dos comandos.
pub(super) struct Linha {
    pub(super) item: Id,
    pub(super) nome: String,
    /// O código de barras, para a coluna "Código" do cupom.
    pub(super) codigo: Option<String>,
    pub(super) quantidade: Quantidade,
    pub(super) preco: Preco,
    pub(super) desconto: Percentual,
    /// Já com o desconto aplicado — o que entra no total do cupom.
    pub(super) total: Dinheiro,
    pub(super) cancelado: bool,
}

impl Linha {
    /// O total antes do desconto.
    pub(super) fn bruto(&self) -> Dinheiro {
        Dinheiro::de_total(self.quantidade, self.preco, Arredondamento::MeioAcima)
    }

    /// Aplica o desconto com **a mesma conta do domínio** (`ItemCupom::aplicar_desconto`):
    /// o desconto é arredondado por item e subtraído do bruto. O backend valida o teto; aqui
    /// só espelhamos o resultado de um comando que já deu certo.
    pub(super) fn com_desconto(&mut self, pct: Percentual) {
        let bruto = self.bruto();
        self.desconto = pct;
        self.total = bruto - bruto.aplicar(pct, Arredondamento::MeioAcima);
    }
}

/// O resultado da última venda — fica na tela até o próximo item (o **troco** é a
/// informação mais importante do momento seguinte ao `F2`).
#[derive(Clone, Copy)]
pub(super) struct UltimaVenda {
    pub(super) numero: i64,
    pub(super) total: Dinheiro,
    pub(super) troco: Dinheiro,
}

/// Estado do diálogo de pagamento.
pub(super) struct EstadoPagamento {
    pub(super) valores: Valores,
    /// A forma escolhida (índice em [`FORMAS`]).
    pub(super) ativa: usize,
    /// O que o operador está digitando para a forma ativa.
    pub(super) digitado: String,
    /// Pede o foco do campo de valor no próximo quadro.
    pub(super) foco_valor: bool,
    /// O id do campo de valor, para devolver o foco.
    pub(super) id_valor: Option<egui::Id>,
    /// Se o campo de valor tinha o foco **ao fim do quadro anterior** ("digitando valor") ou
    /// não ("escolhendo forma": os dígitos escolhem forma). Tem que ser o estado do quadro
    /// anterior, não o foco atual: o egui tira o foco de um campo de linha única assim que vê
    /// `Esc`, *antes* de a tela rodar — o foco atual já seria falso no quadro do `Esc`, que
    /// então fecharia o diálogo em vez de só sair da digitação.
    pub(super) digitando: bool,
}

impl EstadoPagamento {
    pub(super) fn nova() -> Self {
        Self {
            valores: [Dinheiro::ZERO; 4],
            ativa: 0,
            digitado: String::new(),
            foco_valor: false,
            id_valor: None,
            digitando: false,
        }
    }
}

#[derive(Default)]
pub(super) enum Dlg {
    #[default]
    Fechado,
    CadastrarCaixa {
        nome: String,
        conta: Option<Id>,
    },
    AbrirCaixa {
        valor: String,
    },
    Pagamento(EstadoPagamento),
    Desconto {
        linha: usize,
        percentual: String,
    },
    CancelarCupom {
        motivo: String,
        /// Login do supervisor que autoriza (segunda identidade).
        supervisor: String,
        senha: String,
    },
    FecharCaixa {
        contado: String,
        motivo: String,
    },
    Sangria {
        valor: String,
        motivo: String,
    },
    /// `F6`: identificar o cliente da venda.
    Cliente {
        busca: String,
        /// A linha destacada na lista.
        sel: usize,
    },
    /// `F10`: consulta de preço sem abrir venda.
    ConsultaPreco {
        /// O que o operador digitou/bipou.
        termo: String,
        /// A resposta da última consulta, até a próxima.
        resultado: Option<PrecoConsultado>,
    },
}

/// O que a tela quer que aconteça. As visões só empilham isto; [`aplicar`] executa.
pub(super) enum Acao {
    /// `Enter` no campo de bipe.
    Bipar,
    /// Clique numa linha da busca por nome.
    Adicionar(Id),
    /// `F7`.
    CancelarLinha,
    /// `F5`.
    AbrirDesconto,
    /// `F2` na tela de venda.
    AbrirPagamento,
    /// `F2` no diálogo de pagamento.
    Finalizar,
    /// `F8`.
    PedirCancelarCupom,
    /// `F9`.
    AbrirSangria,
    /// `F12` com o caixa aberto.
    AbrirFechamento,
    /// `F10`.
    AbrirConsultaPreco,
    /// `F6`.
    AbrirCliente,
    /// Um cliente foi escolhido (`None` = consumidor não identificado).
    EscolherCliente(Option<(Id, String)>),
    /// `Enter` no diálogo de consulta de preço.
    ConsultarPreco,
}

/// Estado local da tela de PDV.
pub struct EstadoTelaPdv {
    pub(super) terminal: Id,
    pub(super) caixas: Vec<ItemCaixa>,
    pub(super) contas_caixa: Vec<ItemContaResultado>,
    pub(super) tabelas: Vec<TabelaPreco>,
    pub(super) locais: Vec<ItemLocal>,
    pub(super) produtos: Vec<ItemProdutoComSaldo>,
    /// Os clientes cadastrados (até 200, o teto de `PessoasPorPapel` sem busca).
    pub(super) clientes: Vec<ItemPessoa>,
    /// O cliente identificado na venda atual, com o nome para mostrar.
    pub(super) cliente: Option<(Id, String)>,
    pub(super) caixa_sel: Option<Id>,
    pub(super) tabela_sel: Option<Id>,
    pub(super) local_sel: Option<Id>,
    /// O campo de bipe/busca.
    pub(super) entrada: String,
    /// Busca por nome, memorizada: `(texto da busca, índices em `produtos`)` — refazer o
    /// filtro só quando o texto muda (Pilar I: nada de trabalho por quadro sem motivo).
    pub(super) resultados: (String, Vec<usize>),
    /// A linha destacada na lista de resultados.
    pub(super) sel_resultado: usize,
    pub(super) cupom: Option<Id>,
    pub(super) cupom_numero: Option<i64>,
    pub(super) carrinho: Vec<Linha>,
    /// A linha destacada no cupom (alvo de `F5`/`F7`).
    pub(super) linha_sel: Option<usize>,
    pub(super) total: Dinheiro,
    pub(super) ultima_venda: Option<UltimaVenda>,
    pub(super) dlg: Dlg,
    pub(super) erro: Option<String>,
    /// `true` entre o clique em "Finalizar" e a execução de fato — dá um quadro para o egui
    /// pintar o spinner + toast "Emitindo NFC-e…" antes da chamada bloqueante.
    pub(super) finalizar_pendente: bool,
    /// Devolve o foco ao campo de bipe no próximo quadro (depois de cada ação).
    pub(super) foco_entrada: bool,
    pub(super) pendentes: Vec<Acao>,
}

impl Default for EstadoTelaPdv {
    fn default() -> Self {
        Self {
            terminal: Id::novo(),
            caixas: Vec::new(),
            contas_caixa: Vec::new(),
            tabelas: Vec::new(),
            locais: Vec::new(),
            produtos: Vec::new(),
            clientes: Vec::new(),
            cliente: None,
            caixa_sel: None,
            tabela_sel: None,
            local_sel: None,
            entrada: String::new(),
            resultados: (String::new(), Vec::new()),
            sel_resultado: 0,
            cupom: None,
            cupom_numero: None,
            carrinho: Vec::new(),
            linha_sel: None,
            total: Dinheiro::ZERO,
            ultima_venda: None,
            dlg: Dlg::Fechado,
            erro: None,
            finalizar_pendente: false,
            foco_entrada: true,
            pendentes: Vec::new(),
        }
    }
}

impl EstadoTelaPdv {
    /// Carrega caixas, catálogos e produtos.
    pub fn carregar(&mut self, motor: &MotorLocal, sessao: &SessaoLocal) {
        self.erro = None;
        match motor.consultar(sessao, "financeiro.caixas.v1", &Caixas) {
            Ok(c) => self.caixas = c,
            Err(e) => self.erro = Some(e.mensagem),
        }
        if let Ok(c) = motor.consultar(sessao, "financeiro.contas_caixa.v1", &ContasDeCaixa) {
            self.contas_caixa = c;
        }
        if let Ok(t) = motor.consultar(sessao, "vendas.tabelas_de_preco.v1", &TabelasDePreco) {
            self.tabelas = t;
        }
        if let Ok(l) = motor.consultar(sessao, "estoque.locais.v1", &Locais) {
            self.locais = l;
        }
        if let Ok(p) = motor.consultar(sessao, "estoque.produtos_com_saldo.v1", &ProdutosComSaldo) {
            self.produtos = p;
            // Os índices memorizados apontavam para a lista antiga.
            self.resultados = ("\u{0}".to_owned(), Vec::new());
        }

        if let Ok(c) = motor.consultar(
            sessao,
            "clientes.pessoas_por_papel.v1",
            &PessoasPorPapel {
                papel: PapelPessoa::Cliente,
                busca: None,
            },
        ) {
            self.clientes = c;
        }

        if self.caixa_sel.is_none() {
            self.caixa_sel = self
                .caixas
                .iter()
                .find(|c| c.sessao_aberta.is_some())
                .or_else(|| self.caixas.first())
                .map(|c| c.caixa);
        }
        if self.tabela_sel.is_none() {
            self.tabela_sel = self.tabelas.first().map(|t| t.id);
        }
        if self.local_sel.is_none() {
            self.local_sel = self.locais.first().map(|l| l.id);
        }
    }

    pub(super) fn caixa_atual(&self) -> Option<&ItemCaixa> {
        let id = self.caixa_sel?;
        self.caixas.iter().find(|c| c.caixa == id)
    }

    pub(super) fn sessao_aberta(&self) -> Option<Id> {
        self.caixa_atual().and_then(|c| c.sessao_aberta)
    }

    pub(super) fn ativos(&self) -> impl Iterator<Item = &Linha> {
        self.carrinho.iter().filter(|l| !l.cancelado)
    }

    /// A linha selecionada, se ainda for um item ativo.
    pub(super) fn linha_ativa_sel(&self) -> Option<usize> {
        self.linha_sel
            .filter(|&i| self.carrinho.get(i).is_some_and(|l| !l.cancelado))
    }

    pub(super) fn recalcular_total(&mut self) {
        self.total = self
            .ativos()
            .map(|l| l.total)
            .fold(Dinheiro::ZERO, |a, b| a + b);
    }

    /// Move o destaque entre os itens ativos do cupom.
    pub(super) fn mover_linha(&mut self, subir: bool) {
        let ativos: Vec<usize> = self
            .carrinho
            .iter()
            .enumerate()
            .filter(|(_, l)| !l.cancelado)
            .map(|(i, _)| i)
            .collect();
        if ativos.is_empty() {
            self.linha_sel = None;
            return;
        }
        let pos = self
            .linha_sel
            .and_then(|s| ativos.iter().position(|&i| i == s));
        let novo = match (pos, subir) {
            (None, true) => ativos.len() - 1,
            (None, false) => 0,
            (Some(p), true) => p.saturating_sub(1),
            (Some(p), false) => (p + 1).min(ativos.len() - 1),
        };
        self.linha_sel = Some(ativos[novo]);
    }

    /// Refaz a busca por nome só se o texto mudou desde a última vez.
    pub(super) fn atualizar_resultados(&mut self) {
        if self.resultados.0 == self.entrada {
            return;
        }
        let indices = match entrada::interpretar(&self.entrada) {
            Entrada::Busca { termo, .. } if termo.chars().count() >= 2 => self
                .produtos
                .iter()
                .enumerate()
                .filter(|(_, p)| casa_por_palavras(&p.nome, &termo))
                .map(|(i, _)| i)
                .take(MAX_RESULTADOS)
                .collect(),
            _ => Vec::new(),
        };
        self.resultados = (self.entrada.clone(), indices);
        self.sel_resultado = self
            .sel_resultado
            .min(self.resultados.1.len().saturating_sub(1));
    }
}
