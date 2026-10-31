//! **Cotação** (`docs/modulos/compras.md` §3–§6): os mesmos itens perguntados a vários
//! fornecedores, as propostas lado a lado, e a decisão — item a item — que vira pedido.
//!
//! - [`SalvarCotacao`]: os itens (produto e quantidade) e os fornecedores convidados; o texto
//!   da solicitação ([`Cotacao::texto_da_solicitacao`]) vai a cada um;
//! - [`RegistrarPropostaFornecedor`]: o preço de cada item que o fornecedor cotou (o que ele não
//!   tem fica sem preço), o prazo, a condição de pagamento e o frete. O fornecedor que não foi
//!   convidado e mandou proposta entra na cotação;
//! - [`ComparativoDaCotacao`]: a matriz item × fornecedor, o menor preço de cada item, o total de
//!   cada fornecedor (com o frete), o último preço pago e a **escolha sugerida** — o fornecedor
//!   único mais barato que cotou tudo, ou a divisão pelo menor preço de cada item quando ela sai
//!   mais barata **com os fretes somados**;
//! - [`DecidirCotacao`]: quem compra escolhe o fornecedor de cada item (pode deixar item de fora)
//!   e a cotação gera **um pedido por fornecedor**, em rascunho — ou já aprovado, se pedir e
//!   puder aprovar.

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

use crate::pedido::PedidoSalvo;

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite;

/// O estado de uma [`Cotacao`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoCotacao {
    /// Esperando e recebendo propostas.
    Aberta,
    /// Decidida: os pedidos foram gerados.
    Decidida,
    /// Desistiu-se.
    Cancelada,
}

impl EstadoCotacao {
    /// O código gravado na base.
    #[must_use]
    pub const fn codigo(self) -> &'static str {
        match self {
            Self::Aberta => "Aberta",
            Self::Decidida => "Decidida",
            Self::Cancelada => "Cancelada",
        }
    }

    /// O estado a partir do código gravado.
    #[must_use]
    pub fn de_codigo(c: &str) -> Option<Self> {
        [Self::Aberta, Self::Decidida, Self::Cancelada]
            .into_iter()
            .find(|e| e.codigo() == c)
    }
}

/// Um item da cotação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemCotacao {
    /// Identidade.
    pub id: Id,
    /// O produto.
    pub produto: Id,
    /// O nome do produto.
    pub descricao: String,
    /// A unidade.
    pub unidade: String,
    /// Quanto se quer comprar.
    pub quantidade: Quantidade,
    /// O fornecedor escolhido na decisão.
    pub escolhido: Option<Id>,
}

/// A proposta de um fornecedor (ou o convite ainda sem resposta).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropostaFornecedor {
    /// O fornecedor.
    pub fornecedor: Id,
    /// O nome.
    pub fornecedor_nome: String,
    /// Já respondeu.
    pub respondeu: bool,
    /// Em quantos dias entrega.
    pub prazo_entrega_dias: Option<u16>,
    /// A condição de pagamento.
    pub condicao_pagamento: String,
    /// O frete.
    pub frete: Dinheiro,
    /// Observação do fornecedor.
    pub observacao: String,
    /// O preço de cada item cotado: (item, preço unitário).
    pub precos: Vec<(Id, Preco)>,
}

impl PropostaFornecedor {
    /// O preço de um item, se cotou.
    #[must_use]
    pub fn preco(&self, item: Id) -> Option<Preco> {
        self.precos
            .iter()
            .find(|(i, _)| *i == item)
            .map(|(_, p)| *p)
    }
}

/// Uma cotação.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cotacao {
    /// Identidade.
    pub id: Id,
    /// O número, sequencial na empresa.
    pub numero: u32,
    /// Do que se trata ("Reposição de bebidas — novembro").
    pub descricao: String,
    /// O dia.
    pub data: Data,
    /// Até quando se esperam as propostas.
    pub prazo_resposta: Option<Data>,
    /// O estado.
    pub estado: EstadoCotacao,
    /// O motivo do cancelamento.
    pub motivo: Option<String>,
    /// Os itens.
    pub itens: Vec<ItemCotacao>,
    /// Os fornecedores e as propostas, na ordem do convite.
    pub propostas: Vec<PropostaFornecedor>,
    /// Os pedidos gerados na decisão: (pedido, número).
    pub pedidos: Vec<(Id, u32)>,
}

impl Cotacao {
    /// O texto da solicitação, para mandar a cada fornecedor (pelo `WhatsApp` ou e-mail).
    #[must_use]
    pub fn texto_da_solicitacao(&self, empresa: &str) -> String {
        use std::fmt::Write as _;
        let mut t = format!("*Solicitação de cotação nº {}* — {empresa}\n", self.numero);
        if !self.descricao.trim().is_empty() {
            let _ = writeln!(t, "{}", self.descricao.trim());
        }
        if let Some(p) = self.prazo_resposta {
            let _ = writeln!(t, "Responder até: {}", p.formatar());
        }
        t.push_str("\nPor favor, informe o preço unitário de cada item:\n");
        for (n, i) in self.itens.iter().enumerate() {
            let _ = writeln!(
                t,
                "{}. {} — {} {}",
                n + 1,
                i.descricao,
                i.quantidade.formatar(0),
                i.unidade
            );
        }
        t.push_str("\nE também o prazo de entrega, a condição de pagamento e o frete.");
        t
    }
}

/// Um preço na matriz do comparativo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CelulaComparativo {
    /// O preço unitário cotado.
    pub preco: Option<Preco>,
    /// Quantidade × preço.
    pub total: Option<Dinheiro>,
    /// O menor preço do item.
    pub melhor: bool,
}

/// Um item no comparativo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinhaComparativo {
    /// O item da cotação.
    pub item: Id,
    /// O nome do produto.
    pub descricao: String,
    /// A unidade.
    pub unidade: String,
    /// A quantidade.
    pub quantidade: Quantidade,
    /// Um preço por fornecedor, na ordem de [`Comparativo::colunas`].
    pub celulas: Vec<CelulaComparativo>,
    /// A coluna do menor preço.
    pub melhor: Option<usize>,
    /// O último preço pago pelo produto.
    pub ultimo_pago: Option<Preco>,
}

/// Um fornecedor no comparativo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColunaComparativo {
    /// O fornecedor.
    pub fornecedor: Id,
    /// O nome.
    pub nome: String,
    /// Já respondeu.
    pub respondeu: bool,
    /// Quantos itens cotou.
    pub cotados: usize,
    /// Cotou todos os itens.
    pub cotou_tudo: bool,
    /// A soma dos itens cotados.
    pub itens: Dinheiro,
    /// O frete.
    pub frete: Dinheiro,
    /// Itens + frete.
    pub total: Dinheiro,
    /// O prazo de entrega.
    pub prazo_entrega_dias: Option<u16>,
    /// A condição de pagamento.
    pub condicao_pagamento: String,
}

/// As propostas lado a lado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comparativo {
    /// Os itens.
    pub linhas: Vec<LinhaComparativo>,
    /// Os fornecedores.
    pub colunas: Vec<ColunaComparativo>,
    /// O fornecedor único mais barato entre os que cotaram tudo.
    pub melhor_unico: Option<usize>,
    /// Comprando cada item do mais barato: o total com os fretes de quem entra.
    pub total_por_item: Dinheiro,
    /// A escolha sugerida, item a item: (item, fornecedor).
    pub sugerida: Vec<(Id, Id)>,
    /// O total da escolha sugerida, com frete.
    pub total_sugerido: Dinheiro,
}

/// O comparativo da cotação. `ultimos` é o último preço pago por produto.
#[must_use]
pub fn comparar<S: std::hash::BuildHasher>(
    cotacao: &Cotacao,
    ultimos: &HashMap<Id, Preco, S>,
) -> Comparativo {
    let respondidas: Vec<&PropostaFornecedor> = cotacao.propostas.iter().collect();
    let linhas: Vec<LinhaComparativo> = cotacao
        .itens
        .iter()
        .map(|i| {
            let mut celulas: Vec<CelulaComparativo> = respondidas
                .iter()
                .map(|p| {
                    let preco = p.preco(i.id);
                    CelulaComparativo {
                        preco,
                        total: preco.map(|x| {
                            Dinheiro::de_total(i.quantidade, x, Arredondamento::MeioAcima)
                        }),
                        melhor: false,
                    }
                })
                .collect();
            let melhor = celulas
                .iter()
                .enumerate()
                .filter_map(|(n, c)| c.preco.map(|p| (n, p)))
                .min_by_key(|(_, p)| *p)
                .map(|(n, _)| n);
            if let Some(m) = melhor {
                celulas[m].melhor = true;
            }
            LinhaComparativo {
                item: i.id,
                descricao: i.descricao.clone(),
                unidade: i.unidade.clone(),
                quantidade: i.quantidade,
                celulas,
                melhor,
                ultimo_pago: ultimos.get(&i.produto).copied(),
            }
        })
        .collect();
    let colunas = colunas_do_comparativo(&respondidas, &linhas);
    let melhor_unico = colunas
        .iter()
        .enumerate()
        .filter(|(_, c)| c.cotou_tudo)
        .min_by_key(|(_, c)| c.total)
        .map(|(n, _)| n);
    let por_item: Vec<(Id, usize)> = linhas
        .iter()
        .filter_map(|l| l.melhor.map(|m| (l.item, m)))
        .collect();
    let total_de = |escolha: &[(Id, usize)]| -> Dinheiro {
        let mut usados: Vec<usize> = Vec::new();
        let mut total = Dinheiro::ZERO;
        for (item, col) in escolha {
            if let Some(l) = linhas.iter().find(|l| l.item == *item) {
                total += l.celulas[*col].total.unwrap_or_default();
            }
            if !usados.contains(col) {
                usados.push(*col);
                total += colunas[*col].frete;
            }
        }
        total
    };
    let total_por_item = total_de(&por_item);
    let (escolha, total_sugerido) = match melhor_unico {
        Some(u) if colunas[u].total <= total_por_item => {
            let e: Vec<(Id, usize)> = linhas.iter().map(|l| (l.item, u)).collect();
            (e, colunas[u].total)
        }
        _ => (por_item, total_por_item),
    };
    Comparativo {
        sugerida: escolha
            .iter()
            .map(|(i, c)| (*i, colunas[*c].fornecedor))
            .collect(),
        linhas,
        colunas,
        melhor_unico,
        total_por_item,
        total_sugerido,
    }
}

/// Um fornecedor por proposta: quantos itens cotou e o total com o frete.
fn colunas_do_comparativo(
    propostas: &[&PropostaFornecedor],
    linhas: &[LinhaComparativo],
) -> Vec<ColunaComparativo> {
    propostas
        .iter()
        .enumerate()
        .map(|(n, p)| {
            let cotados = linhas
                .iter()
                .filter(|l| l.celulas[n].preco.is_some())
                .count();
            let itens: Dinheiro = linhas.iter().filter_map(|l| l.celulas[n].total).sum();
            ColunaComparativo {
                fornecedor: p.fornecedor,
                nome: p.fornecedor_nome.clone(),
                respondeu: p.respondeu,
                cotados,
                cotou_tudo: !linhas.is_empty() && cotados == linhas.len(),
                itens,
                frete: p.frete,
                total: itens + p.frete,
                prazo_entrega_dias: p.prazo_entrega_dias,
                condicao_pagamento: p.condicao_pagamento.clone(),
            }
        })
        .collect()
}

// ── Comandos ─────────────────────────────────────────────────────────────

/// Um item a cotar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ItemDaCotacao {
    /// O produto.
    pub produto: Id,
    /// Quanto.
    pub quantidade: Quantidade,
}

/// Cria (sem `cotacao`) ou edita a cotação aberta. O item e o fornecedor que continuam guardam
/// os preços já registrados.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SalvarCotacao {
    /// A cotação a editar; `None` cria.
    pub cotacao: Option<Id>,
    /// Do que se trata.
    pub descricao: String,
    /// Até quando se esperam as propostas.
    pub prazo_resposta: Option<Data>,
    /// Os itens.
    pub itens: Vec<ItemDaCotacao>,
    /// Os fornecedores convidados.
    pub fornecedores: Vec<Id>,
}

/// A cotação gravada.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CotacaoSalva {
    /// A cotação.
    pub cotacao: Id,
    /// O número.
    pub numero: u32,
}

impl Comando for SalvarCotacao {
    type Saida = CotacaoSalva;
    const PERMISSAO: &'static str = "compras.cotacao.criar";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = false;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<CotacaoSalva> {
        sqlite::salvar(&self, ctx, uow)
    }
}

/// Registra (ou substitui) a proposta de um fornecedor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarPropostaFornecedor {
    /// A cotação.
    pub cotacao: Id,
    /// O fornecedor.
    pub fornecedor: Id,
    /// O preço unitário de cada item cotado: (item da cotação, preço). O que fica de fora
    /// o fornecedor não tem.
    pub precos: Vec<(Id, Preco)>,
    /// Em quantos dias entrega.
    pub prazo_entrega_dias: Option<u16>,
    /// A condição de pagamento.
    pub condicao_pagamento: String,
    /// O frete.
    pub frete: Dinheiro,
    /// Observação.
    pub observacao: String,
}

impl Comando for RegistrarPropostaFornecedor {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.cotacao.editar";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = false;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::registrar_proposta(&self, ctx, uow)
    }
}

/// A escolha de um item: de quem comprar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EscolhaDaCotacao {
    /// O item da cotação.
    pub item: Id,
    /// O fornecedor escolhido.
    pub fornecedor: Id,
}

/// Decide a cotação: um pedido por fornecedor escolhido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecidirCotacao {
    /// A cotação.
    pub cotacao: Id,
    /// De quem comprar cada item (o item fora da lista não se compra).
    pub escolhas: Vec<EscolhaDaCotacao>,
    /// Gerar os pedidos já aprovados (exige `compras.pedido.aprovar`).
    pub aprovar_pedidos: bool,
}

impl Comando for DecidirCotacao {
    type Saida = Vec<PedidoSalvo>;
    const PERMISSAO: &'static str = "compras.cotacao.decidir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Vec<PedidoSalvo>> {
        sqlite::decidir(&self, ctx, uow)
    }
}

/// Cancela a cotação aberta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelarCotacao {
    /// A cotação.
    pub cotacao: Id,
    /// Por quê (ao menos 10 caracteres).
    pub motivo: String,
}

impl Comando for CancelarCotacao {
    type Saida = ();
    const PERMISSAO: &'static str = "compras.cotacao.decidir";
    const RISCO: Risco = Risco::Baixo;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<()> {
        sqlite::cancelar(&self, ctx, uow)
    }
}

// ── Consultas ────────────────────────────────────────────────────────────

/// Uma cotação na lista.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoCotacao {
    /// A cotação.
    pub cotacao: Id,
    /// O número.
    pub numero: u32,
    /// Do que se trata.
    pub descricao: String,
    /// O dia.
    pub data: Data,
    /// O prazo das respostas.
    pub prazo_resposta: Option<Data>,
    /// O estado.
    pub estado: EstadoCotacao,
    /// Quantos itens.
    pub itens: usize,
    /// Quantos fornecedores convidados.
    pub fornecedores: usize,
    /// Quantos responderam.
    pub respostas: usize,
}

/// As cotações da empresa, da mais nova para a mais antiga.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Cotacoes {
    /// Só as abertas.
    pub so_abertas: bool,
}

impl Consulta for Cotacoes {
    type Saida = Vec<ResumoCotacao>;
    const PERMISSAO: &'static str = "compras.cotacao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<ResumoCotacao>> {
        sqlite::cotacoes(c, ctx.empresa, self.so_abertas)
    }
}

/// A cotação inteira.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CotacaoCompleta {
    /// A cotação.
    pub cotacao: Id,
}

impl Consulta for CotacaoCompleta {
    type Saida = Option<Cotacao>;
    const PERMISSAO: &'static str = "compras.cotacao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<Cotacao>> {
        sqlite::cotacao(c, ctx.empresa, self.cotacao)
    }
}

/// O comparativo das propostas da cotação.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ComparativoDaCotacao {
    /// A cotação.
    pub cotacao: Id,
}

impl Consulta for ComparativoDaCotacao {
    type Saida = Option<Comparativo>;
    const PERMISSAO: &'static str = "compras.cotacao.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Option<Comparativo>> {
        let Some(cot) = sqlite::cotacao(c, ctx.empresa, self.cotacao)? else {
            return Ok(None);
        };
        let produtos: Vec<Id> = cot.itens.iter().map(|i| i.produto).collect();
        let ultimos: HashMap<Id, Preco> =
            crate::pedido::sqlite::ultimos_precos(c, ctx.empresa, &produtos)?
                .into_iter()
                .map(|p| (p.produto, p.preco))
                .collect();
        Ok(Some(comparar(&cot, &ultimos)))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(q: i64) -> ItemCotacao {
        ItemCotacao {
            id: Id::novo(),
            produto: Id::novo(),
            descricao: "Item".into(),
            unidade: "UN".into(),
            quantidade: Quantidade::unidades(q),
            escolhido: None,
        }
    }

    fn proposta(nome: &str, frete: i64, precos: Vec<(Id, i64)>) -> PropostaFornecedor {
        PropostaFornecedor {
            fornecedor: Id::novo(),
            fornecedor_nome: nome.into(),
            respondeu: true,
            prazo_entrega_dias: Some(5),
            condicao_pagamento: String::new(),
            frete: Dinheiro::reais(frete),
            observacao: String::new(),
            precos: precos
                .into_iter()
                .map(|(i, p)| (i, Preco::centavos(p)))
                .collect(),
        }
    }

    fn cotacao(itens: Vec<ItemCotacao>, propostas: Vec<PropostaFornecedor>) -> Cotacao {
        Cotacao {
            id: Id::novo(),
            numero: 3,
            descricao: "Reposição".into(),
            data: Data::de_ymd(2026, 10, 1).unwrap(),
            prazo_resposta: None,
            estado: EstadoCotacao::Aberta,
            motivo: None,
            itens,
            propostas,
            pedidos: Vec::new(),
        }
    }

    #[test]
    fn dividir_compensa_quando_a_economia_paga_o_frete() {
        let (a, b) = (item(10), item(10));
        // Alfa: 10,00 e 10,00 (frete 5) = 205. Beta: 12,00 e 8,00 (frete 5) = 205.
        // Dividindo: 10×10 + 10×8 = 180 + fretes 10 = 190 < 205.
        let alfa = proposta("Alfa", 5, vec![(a.id, 1_000), (b.id, 1_000)]);
        let beta = proposta("Beta", 5, vec![(a.id, 1_200), (b.id, 800)]);
        let (fa, fb) = (alfa.fornecedor, beta.fornecedor);
        let c = comparar(
            &cotacao(vec![a.clone(), b.clone()], vec![alfa, beta]),
            &HashMap::new(),
        );
        assert_eq!(c.linhas[0].melhor, Some(0));
        assert_eq!(c.linhas[1].melhor, Some(1));
        assert_eq!(c.colunas[0].total, Dinheiro::reais(205));
        assert_eq!(c.total_por_item, Dinheiro::reais(190));
        assert_eq!(c.sugerida, vec![(a.id, fa), (b.id, fb)]);
        assert_eq!(c.total_sugerido, Dinheiro::reais(190));
    }

    #[test]
    fn um_so_fornecedor_quando_o_frete_come_a_economia() {
        let (a, b) = (item(10), item(10));
        // Alfa cota tudo (10,00 e 9,00, frete 5); Beta só cota b (8,00), com frete 25.
        let alfa = proposta("Alfa", 5, vec![(a.id, 1_000), (b.id, 900)]);
        let beta = proposta("Beta", 25, vec![(b.id, 800)]);
        let fa = alfa.fornecedor;
        let c = comparar(
            &cotacao(vec![a.clone(), b.clone()], vec![alfa, beta]),
            &HashMap::new(),
        );
        assert_eq!(c.melhor_unico, Some(0), "só a Alfa cotou tudo");
        assert!(!c.colunas[1].cotou_tudo);
        // Alfa: 100 + 90 + 5 = 195; dividir: 100 + 80 + 5 + 25 = 210.
        assert_eq!(c.colunas[0].total, Dinheiro::reais(195));
        assert_eq!(c.total_por_item, Dinheiro::reais(210));
        assert_eq!(c.sugerida, vec![(a.id, fa), (b.id, fa)]);
        assert_eq!(c.total_sugerido, Dinheiro::reais(195));
    }

    #[test]
    fn item_sem_preco_fica_sem_sugestao_e_o_ultimo_pago_aparece() {
        let a = item(2);
        let alfa = proposta("Alfa", 0, vec![]);
        let ultimos: HashMap<Id, Preco> = [(a.produto, Preco::centavos(450))].into();
        let c = comparar(&cotacao(vec![a], vec![alfa]), &ultimos);
        assert_eq!(c.linhas[0].melhor, None);
        assert!(c.sugerida.is_empty());
        assert_eq!(c.linhas[0].ultimo_pago, Some(Preco::centavos(450)));
    }

    #[test]
    fn a_solicitacao_lista_os_itens() {
        let mut a = item(12);
        a.descricao = "Refrigerante Cola 2L".into();
        let t = cotacao(vec![a], vec![]).texto_da_solicitacao("Mercado Bom");
        assert!(t.contains("cotação nº 3"));
        assert!(t.contains("1. Refrigerante Cola 2L — 12 UN"), "{t}");
    }
}
