//! **Pedido de compra** (`docs/modulos/compras.md` §3–§6): o que a empresa encomendou a um
//! fornecedor, a que preço, e quanto disso já chegou.
//!
//! - `Rascunho` → [`AprovarPedidoCompra`] → `Enviado` (aprovado: pode ir ao fornecedor);
//! - as **notas de entrada** do fornecedor se ligam ao pedido — sozinhas, pelo número do pedido
//!   que o fornecedor escreve no XML (`compra/xPed`, `det/prod/xPed`) ou, sem ele, quando o
//!   fornecedor tem um pedido só em aberto com algum item da nota; ou à mão
//!   ([`VincularNotaAoPedido`]);
//! - a **conferência** ([`ConferenciaComPedido`]) compara item a item o que veio com o que se
//!   pediu: quantidade a mais, preço acima, item fora do pedido. A nota que diverge nunca se
//!   confirma sozinha;
//! - confirmar a entrada **recebe** no pedido (`ParcialmenteRecebido` → `Recebido`); estornar a
//!   entrada desfaz o recebimento;
//! - o saldo que não vem mais se **encerra** ([`EncerrarPedidoCompra`], com motivo) — senão o
//!   pedido ficaria "a caminho" para sempre, e o radar de reposição contaria com mercadoria que
//!   não vem. O pedido sem recebimento se **cancela**.
//!
//! O pedido não mexe em estoque, Razão nem financeiro: quem faz isso é a nota.

use std::collections::HashMap;

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Arredondamento, Data, Dinheiro, Id, Preco, Quantidade};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::erros::ErroCompras;
use crate::nota::EstadoNotaEntrada;

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;

/// O mínimo de caracteres do motivo de cancelar ou encerrar um pedido.
pub const MOTIVO_MINIMO_PEDIDO: usize = 10;

/// O estado de um [`PedidoCompra`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoPedido {
    /// Em montagem: edita-se à vontade; ainda não vale como encomenda.
    Rascunho,
    /// Aprovado — pode (ou já foi) para o fornecedor; espera a mercadoria.
    Enviado,
    /// Parte chegou.
    ParcialmenteRecebido,
    /// Tudo chegou.
    Recebido,
    /// O saldo que faltava não vem mais (decisão de quem compra, com motivo).
    Encerrado,
    /// Desistiu-se do pedido antes de chegar qualquer coisa.
    Cancelado,
}

impl EstadoPedido {
    /// O código gravado na base.
    #[must_use]
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Rascunho => "Rascunho",
            Self::Enviado => "Enviado",
            Self::ParcialmenteRecebido => "ParcialmenteRecebido",
            Self::Recebido => "Recebido",
            Self::Encerrado => "Encerrado",
            Self::Cancelado => "Cancelado",
        }
    }

    /// O estado a partir do código gravado.
    #[must_use]
    pub fn de_codigo(codigo: &str) -> Option<Self> {
        [
            Self::Rascunho,
            Self::Enviado,
            Self::ParcialmenteRecebido,
            Self::Recebido,
            Self::Encerrado,
            Self::Cancelado,
        ]
        .into_iter()
        .find(|e| e.codigo() == codigo)
    }

    /// O nome para a tela.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Rascunho => "Rascunho",
            Self::Enviado => "Aguardando entrega",
            Self::ParcialmenteRecebido => "Recebido em parte",
            Self::Recebido => "Recebido",
            Self::Encerrado => "Encerrado",
            Self::Cancelado => "Cancelado",
        }
    }

    /// Espera mercadoria: aprovado e ainda não todo recebido.
    #[must_use]
    pub const fn aguardando(self) -> bool {
        matches!(self, Self::Enviado | Self::ParcialmenteRecebido)
    }

    /// Terminou (recebido, encerrado ou cancelado).
    #[must_use]
    pub const fn terminado(self) -> bool {
        matches!(self, Self::Recebido | Self::Encerrado | Self::Cancelado)
    }
}

/// Um item do pedido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemPedido {
    /// Identidade.
    pub id: Id,
    /// O produto do estoque.
    pub produto: Id,
    /// O nome do produto quando o pedido foi feito.
    pub descricao: String,
    /// A unidade do produto (`UN`, `CX`…).
    pub unidade: String,
    /// Quanto se pediu.
    pub quantidade: Quantidade,
    /// O preço combinado, por unidade.
    pub preco_unitario: Preco,
    /// Quanto já chegou (somado das notas confirmadas ligadas ao pedido).
    pub recebida: Quantidade,
}

impl ItemPedido {
    /// O valor do item.
    #[must_use]
    pub fn total(&self) -> Dinheiro {
        Dinheiro::de_total(
            self.quantidade,
            self.preco_unitario,
            Arredondamento::MeioAcima,
        )
    }

    /// Quanto falta chegar.
    #[must_use]
    pub fn saldo(&self) -> Quantidade {
        (self.quantidade - self.recebida).nao_negativa()
    }
}

/// Uma nota de entrada ligada ao pedido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotaDoPedido {
    /// A nota.
    pub nota: Id,
    /// Número/série.
    pub numero: String,
    /// A série.
    pub serie: String,
    /// A emissão.
    pub emissao: Data,
    /// O estado da nota.
    pub estado: EstadoNotaEntrada,
    /// O total da nota.
    pub total: Dinheiro,
}

/// Um pedido de compra.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedidoCompra {
    /// Identidade.
    pub id: Id,
    /// O número, sequencial na empresa — o que vai ao fornecedor e volta no XML (`xPed`).
    pub numero: u32,
    /// O fornecedor.
    pub fornecedor: Id,
    /// O nome do fornecedor.
    pub fornecedor_nome: String,
    /// A cotação de onde saiu, se saiu de uma.
    pub cotacao: Option<Id>,
    /// O dia do pedido.
    pub data: Data,
    /// Quando o fornecedor prometeu entregar.
    pub previsao_entrega: Option<Data>,
    /// A condição de pagamento combinada ("28/56 dias", "à vista no Pix"…).
    pub condicao_pagamento: String,
    /// O frete combinado.
    pub frete: Dinheiro,
    /// Observação para o fornecedor.
    pub observacao: String,
    /// O estado.
    pub estado: EstadoPedido,
    /// O motivo do cancelamento ou do encerramento.
    pub motivo: Option<String>,
    /// Os itens, na ordem do pedido.
    pub itens: Vec<ItemPedido>,
    /// As notas ligadas ao pedido.
    pub notas: Vec<NotaDoPedido>,
}

impl PedidoCompra {
    /// A soma dos itens.
    #[must_use]
    pub fn total_dos_itens(&self) -> Dinheiro {
        self.itens.iter().map(ItemPedido::total).sum()
    }

    /// Itens + frete.
    #[must_use]
    pub fn total(&self) -> Dinheiro {
        self.total_dos_itens() + self.frete
    }

    /// Já chegou alguma coisa?
    #[must_use]
    pub fn tem_recebimento(&self) -> bool {
        self.itens.iter().any(|i| i.recebida.e_positiva())
    }

    /// O pedido se edita: em rascunho, ou aprovado sem nada recebido (editar o aprovado o
    /// devolve ao rascunho — precisa de nova aprovação).
    #[must_use]
    pub fn editavel(&self) -> bool {
        match self.estado {
            EstadoPedido::Rascunho => true,
            EstadoPedido::Enviado => !self.tem_recebimento(),
            _ => false,
        }
    }

    /// `Rascunho` → `Enviado`.
    ///
    /// # Errors
    /// [`ErroCompras::EstadoDoPedidoInvalido`] fora do rascunho; [`ErroCompras::PedidoVazio`].
    pub fn aprovar(&mut self) -> Result<(), ErroCompras> {
        if self.estado != EstadoPedido::Rascunho {
            return Err(ErroCompras::EstadoDoPedidoInvalido {
                atual: self.estado.nome(),
                acao: "aprovar",
            });
        }
        if self.itens.is_empty() {
            return Err(ErroCompras::PedidoVazio);
        }
        self.estado = EstadoPedido::Enviado;
        // O rascunho editado com nota já ligada pode ter recebido: o estado acompanha.
        self.atualizar_pelo_recebimento();
        Ok(())
    }

    /// Cancela o pedido que nada recebeu.
    ///
    /// # Errors
    /// [`ErroCompras::PedidoComRecebimento`]; [`ErroCompras::EstadoDoPedidoInvalido`] no pedido
    /// terminado; [`ErroCompras::MotivoDoPedidoCurto`].
    pub fn cancelar(&mut self, motivo: &str) -> Result<(), ErroCompras> {
        let motivo = motivo_valido(motivo)?;
        if self.tem_recebimento() {
            return Err(ErroCompras::PedidoComRecebimento);
        }
        if !matches!(self.estado, EstadoPedido::Rascunho | EstadoPedido::Enviado) {
            return Err(ErroCompras::EstadoDoPedidoInvalido {
                atual: self.estado.nome(),
                acao: "cancelar",
            });
        }
        self.estado = EstadoPedido::Cancelado;
        self.motivo = Some(motivo);
        Ok(())
    }

    /// Encerra o saldo do pedido recebido em parte: o que falta não vem mais.
    ///
    /// # Errors
    /// [`ErroCompras::EstadoDoPedidoInvalido`] fora de `ParcialmenteRecebido` (o pedido sem
    /// recebimento se cancela); [`ErroCompras::MotivoDoPedidoCurto`].
    pub fn encerrar(&mut self, motivo: &str) -> Result<(), ErroCompras> {
        let motivo = motivo_valido(motivo)?;
        if self.estado != EstadoPedido::ParcialmenteRecebido {
            return Err(ErroCompras::EstadoDoPedidoInvalido {
                atual: self.estado.nome(),
                acao: "encerrar",
            });
        }
        self.estado = EstadoPedido::Encerrado;
        self.motivo = Some(motivo);
        Ok(())
    }

    /// O estado que o recebimento dá ao pedido aprovado. O rascunho, o encerrado e o cancelado
    /// não mudam.
    pub fn atualizar_pelo_recebimento(&mut self) {
        if !matches!(
            self.estado,
            EstadoPedido::Enviado | EstadoPedido::ParcialmenteRecebido | EstadoPedido::Recebido
        ) {
            return;
        }
        self.estado = if self.itens.iter().all(|i| i.recebida >= i.quantidade) {
            EstadoPedido::Recebido
        } else if self.tem_recebimento() {
            EstadoPedido::ParcialmenteRecebido
        } else {
            EstadoPedido::Enviado
        };
    }

    /// O pedido em texto, para mandar ao fornecedor (pelo `WhatsApp` ou e-mail).
    #[must_use]
    pub fn texto_para_o_fornecedor(&self, empresa: &str, fornecedor: &str) -> String {
        use std::fmt::Write as _;
        let mut t = format!(
            "*Pedido de compra nº {}* — {empresa}\nPara: {fornecedor}\nData: {}\n",
            self.numero,
            self.data.formatar()
        );
        if let Some(p) = self.previsao_entrega {
            let _ = writeln!(t, "Entrega combinada: {}", p.formatar());
        }
        t.push('\n');
        for (n, i) in self.itens.iter().enumerate() {
            let _ = writeln!(
                t,
                "{}. {} — {} {} × {} = {}",
                n + 1,
                i.descricao,
                i.quantidade.formatar(0),
                i.unidade,
                i.preco_unitario.formatar_com_simbolo(),
                i.total().formatar_com_simbolo()
            );
        }
        let _ = write!(
            t,
            "\nTotal dos itens: {}",
            self.total_dos_itens().formatar_com_simbolo()
        );
        if self.frete.e_positivo() {
            let _ = write!(
                t,
                "\nFrete: {}\nTotal: {}",
                self.frete.formatar_com_simbolo(),
                self.total().formatar_com_simbolo()
            );
        }
        if !self.condicao_pagamento.trim().is_empty() {
            let _ = write!(t, "\nPagamento: {}", self.condicao_pagamento.trim());
        }
        if !self.observacao.trim().is_empty() {
            let _ = write!(t, "\nObs.: {}", self.observacao.trim());
        }
        let _ = write!(
            t,
            "\n\nPor favor, informe o nº {} do pedido na nota fiscal (campo xPed).",
            self.numero
        );
        t
    }
}

/// O motivo, com os espaços normalizados, se tem o mínimo.
fn motivo_valido(motivo: &str) -> Result<String, ErroCompras> {
    let m = motivo.split_whitespace().collect::<Vec<_>>().join(" ");
    if m.chars().count() < MOTIVO_MINIMO_PEDIDO {
        return Err(ErroCompras::MotivoDoPedidoCurto);
    }
    Ok(m)
}

/// O número de pedido que o fornecedor escreveu (`xPed`: "42", "PC-0042", "Pedido 42/2026"…):
/// o primeiro bloco de dígitos.
#[must_use]
pub fn numero_do_xped(xped: &str) -> Option<u32> {
    let inicio = xped.find(|c: char| c.is_ascii_digit())?;
    let digitos: String = xped[inicio..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digitos.parse().ok().filter(|n| *n > 0)
}

/// Quanto de cada item do pedido uma nota recebe: o que a nota trouxe de cada produto enche as
/// linhas do pedido com aquele produto, na ordem, até o saldo de cada uma; o que passar do
/// pedido fica na última linha do produto (o pedido mostra que veio a mais). Produto fora do
/// pedido não recebe nada (a conferência o aponta).
#[must_use]
pub fn distribuir_recebimento(
    itens: &[ItemPedido],
    recebido: &[(Id, Quantidade)],
) -> Vec<(Id, Quantidade)> {
    let mut por_produto: HashMap<Id, Quantidade> = HashMap::new();
    for (p, q) in recebido {
        if q.e_positiva() {
            *por_produto.entry(*p).or_insert(Quantidade::ZERO) += *q;
        }
    }
    let mut saida: Vec<(Id, Quantidade)> = Vec::new();
    for item in itens {
        let Some(resta) = por_produto.get_mut(&item.produto) else {
            continue;
        };
        if !resta.e_positiva() {
            continue;
        }
        let ultima = !itens
            .iter()
            .skip_while(|i| i.id != item.id)
            .skip(1)
            .any(|i| i.produto == item.produto);
        let q = if ultima {
            *resta
        } else {
            (*resta).min(item.saldo())
        };
        if q.e_positiva() {
            saida.push((item.id, q));
            *resta -= q;
        }
    }
    saida
}

/// Um item da nota, para conferir contra o pedido.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemParaConferir {
    /// O produto casado (sem casamento, não se compara).
    pub produto: Option<Id>,
    /// A descrição do fornecedor.
    pub descricao: String,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// O preço unitário da nota (`vUnCom`).
    pub preco_unitario: Preco,
}

/// Uma linha da conferência: um produto do pedido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaConferencia {
    /// O produto.
    pub produto: Id,
    /// O nome no pedido.
    pub descricao: String,
    /// Quanto se pediu.
    pub pedida: Quantidade,
    /// Quanto chegou antes desta nota.
    pub recebida_antes: Quantidade,
    /// Quanto esta nota traz.
    pub nesta_nota: Quantidade,
    /// O preço do pedido.
    pub preco_pedido: Preco,
    /// O maior preço na nota (o do item, se veio).
    pub preco_na_nota: Option<Preco>,
}

impl LinhaConferencia {
    /// Quanto passa do pedido (zero se não passa).
    #[must_use]
    pub fn a_mais(&self) -> Quantidade {
        (self.recebida_antes + self.nesta_nota - self.pedida).nao_negativa()
    }

    /// Quanto ainda falta depois desta nota.
    #[must_use]
    pub fn faltando(&self) -> Quantidade {
        (self.pedida - self.recebida_antes - self.nesta_nota).nao_negativa()
    }

    /// A nota cobra mais que o pedido (por mais de um centavo por unidade).
    #[must_use]
    pub fn preco_acima(&self) -> bool {
        self.preco_na_nota.is_some_and(|p| {
            p.unidades_internas() - self.preco_pedido.unidades_internas()
                > Preco::centavos(1).unidades_internas()
        })
    }
}

/// A nota conferida contra o pedido.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConferenciaDoPedido {
    /// O pedido.
    pub pedido: Id,
    /// O número do pedido.
    pub numero: u32,
    /// Como o pedido foi ligado à nota.
    pub ligacao: String,
    /// Os produtos do pedido.
    pub linhas: Vec<LinhaConferencia>,
    /// Os itens da nota com produto fora do pedido.
    pub fora_do_pedido: Vec<String>,
    /// Os itens da nota ainda sem produto (não dá para comparar).
    pub sem_produto: usize,
    /// O que diverge, em texto: quantidade a mais, preço acima, item fora do pedido.
    pub divergencias: Vec<String>,
    /// Depois desta nota, o pedido fica todo recebido.
    pub completa_o_pedido: bool,
}

/// Confere os itens de uma nota contra o pedido. `recebida_antes` de cada item do pedido é a
/// recebida **sem** esta nota.
#[must_use]
pub fn conferir<S: std::hash::BuildHasher>(
    pedido: &PedidoCompra,
    recebida_antes: &HashMap<Id, Quantidade, S>,
    itens: &[ItemParaConferir],
    ligacao: &str,
) -> ConferenciaDoPedido {
    let mut linhas: Vec<LinhaConferencia> = Vec::new();
    for i in &pedido.itens {
        let antes = recebida_antes
            .get(&i.id)
            .copied()
            .unwrap_or(Quantidade::ZERO);
        if let Some(l) = linhas.iter_mut().find(|l| l.produto == i.produto) {
            l.pedida += i.quantidade;
            l.recebida_antes += antes;
            continue;
        }
        linhas.push(LinhaConferencia {
            produto: i.produto,
            descricao: i.descricao.clone(),
            pedida: i.quantidade,
            recebida_antes: antes,
            nesta_nota: Quantidade::ZERO,
            preco_pedido: i.preco_unitario,
            preco_na_nota: None,
        });
    }
    let mut fora = Vec::new();
    let mut sem_produto = 0;
    for item in itens {
        let Some(p) = item.produto else {
            sem_produto += 1;
            continue;
        };
        match linhas.iter_mut().find(|l| l.produto == p) {
            Some(l) => {
                l.nesta_nota += item.quantidade;
                l.preco_na_nota = Some(
                    l.preco_na_nota
                        .map_or(item.preco_unitario, |x| x.max(item.preco_unitario)),
                );
            }
            None => fora.push(item.descricao.clone()),
        }
    }
    let mut divergencias = Vec::new();
    for l in &linhas {
        if l.a_mais().e_positiva() {
            divergencias.push(format!(
                "{}: veio {} a mais do que o pedido",
                l.descricao,
                l.a_mais().formatar(0)
            ));
        }
        if l.preco_acima() {
            let p = l.preco_na_nota.unwrap_or_default();
            divergencias.push(format!(
                "{}: a nota cobra {} por unidade; o pedido combinou {}",
                l.descricao,
                p.formatar_com_simbolo(),
                l.preco_pedido.formatar_com_simbolo()
            ));
        }
    }
    for d in &fora {
        divergencias.push(format!("{d}: não está no pedido"));
    }
    let completa = linhas.iter().all(|l| l.faltando().e_zero());
    ConferenciaDoPedido {
        pedido: pedido.id,
        numero: pedido.numero,
        ligacao: ligacao.to_owned(),
        linhas,
        fora_do_pedido: fora,
        sem_produto,
        divergencias,
        completa_o_pedido: completa,
    }
}

// ── Comandos ─────────────────────────────────────────────────────────────

/// Um item do pedido, como quem compra digita.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDoPedido {
    /// O produto do estoque.
    pub produto: Id,
    /// Quanto.
    pub quantidade: Quantidade,
    /// A que preço (por unidade).
    pub preco_unitario: Preco,
}

/// Cria (sem `pedido`) ou edita um pedido de compra. Editar o pedido já aprovado (sem nada
/// recebido) o devolve ao rascunho.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalvarPedidoCompra {
    /// O pedido a editar; `None` cria.
    pub pedido: Option<Id>,
    /// O fornecedor.
    pub fornecedor: Id,
    /// Quando o fornecedor prometeu entregar.
    pub previsao_entrega: Option<Data>,
    /// A condição de pagamento combinada.
    pub condicao_pagamento: String,
    /// O frete combinado.
    pub frete: Dinheiro,
    /// Observação para o fornecedor.
    pub observacao: String,
    /// Os itens.
    pub itens: Vec<ItemDoPedido>,
}

/// O pedido gravado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PedidoSalvo {
    /// O pedido.
    pub pedido: Id,
    /// O número.
    pub numero: u32,
    /// O estado depois de salvar.
    pub estado: EstadoPedido,
}

impl Comando for SalvarPedidoCompra {
    type Saida = PedidoSalvo;
    const PERMISSAO: &'static str = "compras.pedido.criar";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<PedidoSalvo> {
        sqlite::salvar(&self, ctx, uow)
    }
}

/// Aprova o pedido: `Rascunho` → `Enviado`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AprovarPedidoCompra {
    /// O pedido.
    pub pedido: Id,
}

impl Comando for AprovarPedidoCompra {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.pedido.aprovar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::mudar(self.pedido, ctx, uow, PedidoCompra::aprovar)
    }
}

/// Cancela o pedido que nada recebeu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarPedidoCompra {
    /// O pedido.
    pub pedido: Id,
    /// Por quê (ao menos [`MOTIVO_MINIMO_PEDIDO`] caracteres).
    pub motivo: String,
}

impl Comando for CancelarPedidoCompra {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.pedido.aprovar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        let motivo = self.motivo;
        sqlite::mudar(self.pedido, ctx, uow, |p| p.cancelar(&motivo))
    }
}

/// Encerra o saldo do pedido recebido em parte.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncerrarPedidoCompra {
    /// O pedido.
    pub pedido: Id,
    /// Por quê (ao menos [`MOTIVO_MINIMO_PEDIDO`] caracteres).
    pub motivo: String,
}

impl Comando for EncerrarPedidoCompra {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.pedido.aprovar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        let motivo = self.motivo;
        sqlite::mudar(self.pedido, ctx, uow, |p| p.encerrar(&motivo))
    }
}

/// Liga a nota (ainda não confirmada) a um pedido aprovado do mesmo fornecedor, ou desliga
/// (`pedido: None`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct VincularNotaAoPedido {
    /// A nota de entrada.
    pub nota_entrada: Id,
    /// O pedido; `None` desliga.
    pub pedido: Option<Id>,
}

impl Comando for VincularNotaAoPedido {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.entrada.conferir";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = false;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::vincular(self.nota_entrada, self.pedido, ctx, uow)
    }
}

// ── Consultas ────────────────────────────────────────────────────────────

/// Um pedido na lista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoPedido {
    /// O pedido.
    pub pedido: Id,
    /// O número.
    pub numero: u32,
    /// O fornecedor.
    pub fornecedor: Id,
    /// O nome do fornecedor.
    pub fornecedor_nome: String,
    /// O dia do pedido.
    pub data: Data,
    /// A entrega prometida.
    pub previsao_entrega: Option<Data>,
    /// O estado.
    pub estado: EstadoPedido,
    /// Itens + frete.
    pub total: Dinheiro,
    /// Quanto do valor dos itens já chegou (pelo preço do pedido).
    pub recebido: Dinheiro,
}

impl ResumoPedido {
    /// A entrega prometida já passou e o pedido ainda espera mercadoria.
    #[must_use]
    pub fn atrasado(&self, hoje: Data) -> bool {
        self.estado.aguardando() && self.previsao_entrega.is_some_and(|p| p < hoje)
    }
}

/// Os pedidos da empresa, do mais novo para o mais antigo.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PedidosDeCompra {
    /// Só os que ainda não terminaram (rascunho, aguardando, recebido em parte).
    pub so_em_andamento: bool,
    /// Só os de um fornecedor.
    pub fornecedor: Option<Id>,
}

impl Consulta for PedidosDeCompra {
    type Saida = Vec<ResumoPedido>;
    const PERMISSAO: &'static str = "compras.pedido.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<ResumoPedido>> {
        sqlite::pedidos(c, ctx.empresa, self)
    }
}

/// Um pedido inteiro: itens, recebimento e notas.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct PedidoDeCompra {
    /// O pedido.
    pub pedido: Id,
}

impl Consulta for PedidoDeCompra {
    type Saida = Option<PedidoCompra>;
    const PERMISSAO: &'static str = "compras.pedido.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<PedidoCompra>> {
        sqlite::pedido(c, ctx.empresa, self.pedido)
    }
}

/// A nota conferida contra o pedido a que está ligada (`None`: a nota não tem pedido).
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ConferenciaComPedido {
    /// A nota de entrada.
    pub nota_entrada: Id,
}

impl Consulta for ConferenciaComPedido {
    type Saida = Option<ConferenciaDoPedido>;
    const PERMISSAO: &'static str = "compras.entrada.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<ConferenciaDoPedido>> {
        sqlite::conferencia(c, ctx.empresa, self.nota_entrada)
    }
}

/// O que está a caminho: por produto, o saldo dos pedidos aprovados que ainda esperam
/// mercadoria — o radar de reposição desconta, para não pedir de novo o que já vem.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ACaminho;

impl Consulta for ACaminho {
    type Saida = Vec<(Id, Quantidade)>;
    const PERMISSAO: &'static str = "compras.pedido.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<(Id, Quantidade)>> {
        sqlite::a_caminho(c, ctx.empresa)
    }
}

/// O último preço pago por um produto (a última nota confirmada que o trouxe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrecoPago {
    /// O produto.
    pub produto: Id,
    /// O preço unitário da nota (`vUnCom`).
    pub preco: Preco,
    /// O fornecedor.
    pub fornecedor: Id,
    /// A emissão da nota.
    pub data: Data,
}

/// O último preço pago por cada produto pedido — o preço sugerido do pedido e a referência da
/// cotação.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UltimosPrecosPagos {
    /// Os produtos.
    pub produtos: Vec<Id>,
}

impl Consulta for UltimosPrecosPagos {
    type Saida = Vec<PrecoPago>;
    const PERMISSAO: &'static str = "compras.pedido.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<PrecoPago>> {
        sqlite::ultimos_precos(c, ctx.empresa, &self.produtos)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(produto: Id, q: i64, preco: i64, recebida: i64) -> ItemPedido {
        ItemPedido {
            id: Id::novo(),
            produto,
            descricao: "Refrigerante".into(),
            unidade: "UN".into(),
            quantidade: Quantidade::unidades(q),
            preco_unitario: Preco::centavos(preco),
            recebida: Quantidade::unidades(recebida),
        }
    }

    fn pedido(itens: Vec<ItemPedido>) -> PedidoCompra {
        PedidoCompra {
            id: Id::novo(),
            numero: 7,
            fornecedor: Id::novo(),
            fornecedor_nome: "Distribuidora Alfa".into(),
            cotacao: None,
            data: Data::de_ymd(2026, 10, 1).unwrap(),
            previsao_entrega: None,
            condicao_pagamento: "28 dias".into(),
            frete: Dinheiro::reais(10),
            observacao: String::new(),
            estado: EstadoPedido::Enviado,
            motivo: None,
            itens,
            notas: Vec::new(),
        }
    }

    #[test]
    fn o_numero_vem_do_primeiro_bloco_de_digitos() {
        assert_eq!(numero_do_xped("42"), Some(42));
        assert_eq!(numero_do_xped("PC-0042"), Some(42));
        assert_eq!(numero_do_xped("Pedido 42/2026"), Some(42));
        assert_eq!(numero_do_xped("sem número"), None);
        assert_eq!(numero_do_xped("000"), None);
    }

    #[test]
    fn o_estado_segue_o_recebimento_e_o_encerrado_fica() {
        let p = Id::novo();
        let mut ped = pedido(vec![item(p, 10, 400, 0)]);
        ped.atualizar_pelo_recebimento();
        assert_eq!(ped.estado, EstadoPedido::Enviado);
        ped.itens[0].recebida = Quantidade::unidades(4);
        ped.atualizar_pelo_recebimento();
        assert_eq!(ped.estado, EstadoPedido::ParcialmenteRecebido);
        assert!(ped.cancelar("Fornecedor sumiu de vez").is_err());
        ped.encerrar("O resto não vem mais").unwrap();
        assert_eq!(ped.estado, EstadoPedido::Encerrado);
        ped.itens[0].recebida = Quantidade::unidades(10);
        ped.atualizar_pelo_recebimento();
        assert_eq!(ped.estado, EstadoPedido::Encerrado, "encerrado não volta");
    }

    #[test]
    fn aprovar_cancelar_e_editar() {
        let mut ped = pedido(vec![item(Id::novo(), 1, 100, 0)]);
        ped.estado = EstadoPedido::Rascunho;
        assert!(ped.editavel());
        assert_eq!(ped.cancelar("curto"), Err(ErroCompras::MotivoDoPedidoCurto));
        ped.aprovar().unwrap();
        assert!(ped.editavel(), "aprovado sem recebimento ainda se edita");
        assert!(ped.aprovar().is_err());
        ped.cancelar("Comprei de outro fornecedor").unwrap();
        assert_eq!(ped.estado, EstadoPedido::Cancelado);
        assert!(!ped.editavel());
        let mut vazio = pedido(Vec::new());
        vazio.estado = EstadoPedido::Rascunho;
        assert_eq!(vazio.aprovar(), Err(ErroCompras::PedidoVazio));
    }

    #[test]
    fn o_recebimento_enche_as_linhas_na_ordem_e_o_excesso_fica_na_ultima() {
        let p = Id::novo();
        let outro = Id::novo();
        let itens = vec![
            item(p, 10, 400, 6),
            item(outro, 5, 100, 0),
            item(p, 10, 400, 0),
        ];
        let d = distribuir_recebimento(
            &itens,
            &[
                (p, Quantidade::unidades(20)),
                (Id::novo(), Quantidade::unidades(3)),
            ],
        );
        assert_eq!(
            d,
            vec![
                (itens[0].id, Quantidade::unidades(4)),
                (itens[2].id, Quantidade::unidades(16)),
            ]
        );
    }

    #[test]
    fn a_conferencia_aponta_a_mais_preco_acima_e_fora_do_pedido() {
        let p = Id::novo();
        let ped = pedido(vec![item(p, 10, 400, 0)]);
        let c = conferir(
            &ped,
            &HashMap::new(),
            &[
                ItemParaConferir {
                    produto: Some(p),
                    descricao: "REFRIG".into(),
                    quantidade: Quantidade::unidades(12),
                    preco_unitario: Preco::centavos(420),
                },
                ItemParaConferir {
                    produto: Some(Id::novo()),
                    descricao: "BRINDE".into(),
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::centavos(100),
                },
                ItemParaConferir {
                    produto: None,
                    descricao: "SEM CASAR".into(),
                    quantidade: Quantidade::unidades(1),
                    preco_unitario: Preco::centavos(100),
                },
            ],
            "pelo número do pedido no XML",
        );
        assert_eq!(c.linhas[0].a_mais(), Quantidade::unidades(2));
        assert!(c.linhas[0].preco_acima());
        assert_eq!(c.fora_do_pedido, vec!["BRINDE".to_owned()]);
        assert_eq!(c.sem_produto, 1);
        assert_eq!(c.divergencias.len(), 3);
        assert!(c.completa_o_pedido);
    }

    #[test]
    fn a_nota_parcial_no_preco_nao_diverge() {
        let p = Id::novo();
        let ped = pedido(vec![item(p, 10, 400, 0)]);
        let antes: HashMap<Id, Quantidade> = [(ped.itens[0].id, Quantidade::unidades(3))].into();
        let c = conferir(
            &ped,
            &antes,
            &[ItemParaConferir {
                produto: Some(p),
                descricao: "REFRIG".into(),
                quantidade: Quantidade::unidades(4),
                preco_unitario: Preco::centavos(401),
            }],
            "",
        );
        assert!(c.divergencias.is_empty(), "{:?}", c.divergencias);
        assert_eq!(c.linhas[0].faltando(), Quantidade::unidades(3));
        assert!(!c.completa_o_pedido);
    }

    #[test]
    fn o_texto_para_o_fornecedor_tem_o_numero_e_os_totais() {
        let ped = pedido(vec![item(Id::novo(), 10, 400, 0)]);
        let t = ped.texto_para_o_fornecedor("Mercado Bom", "Distribuidora Alfa");
        assert!(t.contains("Pedido de compra nº 7"));
        assert!(t.contains("Total: R$ 50,00"), "{t}");
        assert!(t.contains("28 dias"));
        assert!(t.contains("xPed"));
    }
}
