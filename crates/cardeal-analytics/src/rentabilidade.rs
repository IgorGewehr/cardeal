//! Rentabilidade por nível da estrutura mercadológica (Grupo › Subgrupo › Categoria ›
//! Subcategoria): quanto cada nó vendeu, quanto custou e quanto deixou de margem.
//!
//! A função [`calcular`] é pura: recebe a árvore, o mapa produto → grupo, a receita por produto
//! (vinda das consultas de vendas, PDV e OS) e o custo por produto (vindo do estoque) e devolve
//! os números por nó — **na subárvore** (o nó soma todos os descendentes) e só dos produtos
//! diretos. As regras que importam:
//!
//! - **Sem custo, sem lucro.** Produto vendido cujo custo não se conhece (nenhuma saída com custo
//!   positivo) entra na receita, mas fica **fora da margem** e é contado em `itens_sem_custo`.
//!   Nunca vira "lucro de 100%".
//! - **Custo sem receita é ignorado**: só entra quem vendeu (ou foi entregue em garantia, com
//!   receita zero) no período; uma peça aplicada em OS ainda não faturada não pesa no resultado.
//! - **Margem líquida estimada** = receita × (1 − encargos) − custo, com os encargos atuais da
//!   empresa (operação, imposto e taxas) e a **comissão de cada produto** (a própria, a do grupo
//!   mais próximo ou a padrão — comissão por estrutura). É uma estimativa: usa os percentuais de
//!   hoje. Num nó, a receita é somada por percentual de encargos e cada soma é descontada uma
//!   vez: com a mesma comissão em tudo, o resultado é idêntico ao de um percentual único.
//! - Divisão por zero não existe: sem receita (ou sem receita com custo) o percentual é `None`.

use std::collections::HashMap;

use cardeal_kernel::{Arredondamento, Dinheiro, Id, Percentual, Quantidade};
use serde::{Deserialize, Serialize};

/// A receita de um produto numa origem (venda, PDV ou OS) — o formato único das três consultas
/// públicas `*.receita_por_produto.v1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceitaDoProduto {
    /// O produto.
    pub produto: Id,
    /// A quantidade vendida.
    pub quantidade: Quantidade,
    /// A receita (já com o desconto; garantia = zero).
    pub receita: Dinheiro,
}

/// O custo do que saiu do estoque de um produto (de qualquer origem).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustoDoProduto {
    /// O produto.
    pub produto: Id,
    /// A quantidade que saiu.
    pub quantidade: Quantidade,
    /// O custo dessa quantidade.
    pub custo: Dinheiro,
}

/// Um nó da estrutura mercadológica.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoDaEstrutura {
    /// O grupo.
    pub id: Id,
    /// O pai; `None` na raiz.
    pub pai: Option<Id>,
    /// O nome.
    pub nome: String,
    /// O código curto.
    pub codigo: String,
    /// O nível (1 = Grupo … 4 = Subcategoria).
    pub nivel: u8,
}

/// A que grupo um produto pertence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProdutoDaEstrutura {
    /// O produto.
    pub produto: Id,
    /// O nome.
    pub nome: String,
    /// O grupo (o nó mais específico).
    pub grupo: Id,
    /// A comissão de vendedor que incide sobre a venda deste produto; `None` = a padrão.
    pub comissao: Option<Percentual>,
}

/// Os encargos sobre a venda: o total de hoje (operação, imposto, taxas e a comissão padrão) e
/// qual é a comissão padrão, para trocá-la pela de cada produto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct EncargosDaVenda {
    /// Tudo que incide sobre a receita, com a comissão padrão.
    pub total: Percentual,
    /// A comissão padrão contida em `total`.
    pub comissao_padrao: Percentual,
}

impl EncargosDaVenda {
    /// Os encargos de um produto: o total com a comissão dele no lugar da padrão.
    #[must_use]
    pub fn do_produto(&self, comissao: Option<Percentual>) -> Percentual {
        comissao.map_or(self.total, |c| self.total - self.comissao_padrao + c)
    }
}

/// Um percentual único: todos os produtos com os mesmos encargos.
impl From<Percentual> for EncargosDaVenda {
    fn from(total: Percentual) -> Self {
        Self {
            total,
            comissao_padrao: Percentual::ZERO,
        }
    }
}

/// Os números de um produto no período.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProdutoRentavel {
    /// O produto.
    pub produto: Id,
    /// O nome (vazio se o produto não está no mapa).
    pub nome: String,
    /// O grupo; `None` se o produto não está no mapa ou o grupo não está na árvore.
    pub grupo: Option<Id>,
    /// A quantidade vendida.
    pub quantidade: Quantidade,
    /// A receita.
    pub receita: Dinheiro,
    /// O custo; `None` quando não se conhece (fica fora da margem).
    pub custo: Option<Dinheiro>,
    /// Receita menos custo; `None` sem custo.
    pub margem_bruta: Option<Dinheiro>,
    /// A margem bruta sobre a receita; `None` sem custo ou sem receita.
    pub margem_bruta_pct: Option<Percentual>,
    /// A margem líquida estimada (receita × (1 − encargos) − custo); `None` sem custo.
    pub margem_liquida_estimada: Option<Dinheiro>,
    /// A margem líquida estimada sobre a receita; `None` sem custo ou sem receita.
    pub margem_liquida_pct: Option<Percentual>,
}

/// Os números de um nó, somando a subárvore.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoDeRentabilidade {
    /// O grupo.
    pub id: Id,
    /// O pai.
    pub pai: Option<Id>,
    /// O nome.
    pub nome: String,
    /// O código.
    pub codigo: String,
    /// O nível.
    pub nivel: u8,
    /// Receita da subárvore (todos os produtos vendidos, com ou sem custo).
    pub receita: Dinheiro,
    /// Receita só dos produtos com custo conhecido — a base da margem.
    pub receita_com_custo: Dinheiro,
    /// Custo da subárvore (só dos produtos com custo conhecido).
    pub custo: Dinheiro,
    /// `receita_com_custo − custo`.
    pub margem_bruta: Dinheiro,
    /// A margem bruta sobre `receita_com_custo`; `None` sem base.
    pub margem_bruta_pct: Option<Percentual>,
    /// A margem líquida estimada da subárvore.
    pub margem_liquida_estimada: Dinheiro,
    /// A margem líquida estimada sobre `receita_com_custo`; `None` sem base.
    pub margem_liquida_pct: Option<Percentual>,
    /// A fatia da receita total da empresa (0 se não houve receita).
    pub participacao: Percentual,
    /// Produtos com venda na subárvore.
    pub produtos: u32,
    /// Produtos vendidos sem custo conhecido na subárvore.
    pub itens_sem_custo: u32,
    /// Receita só dos produtos diretos deste nó.
    pub receita_propria: Dinheiro,
    /// Custo só dos produtos diretos deste nó (os com custo conhecido).
    pub custo_proprio: Dinheiro,
}

/// O total geral — inclui produtos sem grupo na árvore.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Totais {
    /// Receita de tudo que foi vendido.
    pub receita: Dinheiro,
    /// Receita dos produtos com custo conhecido.
    pub receita_com_custo: Dinheiro,
    /// Custo dos produtos com custo conhecido.
    pub custo: Dinheiro,
    /// `receita_com_custo − custo`.
    pub margem_bruta: Dinheiro,
    /// A margem bruta sobre `receita_com_custo`.
    pub margem_bruta_pct: Option<Percentual>,
    /// A margem líquida estimada.
    pub margem_liquida_estimada: Dinheiro,
    /// A margem líquida estimada sobre `receita_com_custo`.
    pub margem_liquida_pct: Option<Percentual>,
    /// Produtos com venda.
    pub produtos: u32,
    /// Produtos vendidos sem custo conhecido.
    pub itens_sem_custo: u32,
    /// Receita de produtos sem grupo na árvore (só no total, em nenhum nó).
    pub receita_sem_estrutura: Dinheiro,
}

/// O resultado completo.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Rentabilidade {
    /// Os nós, na ordem em que vieram.
    pub nos: Vec<NoDeRentabilidade>,
    /// Os produtos vendidos, do que mais rendeu receita para o que menos.
    pub produtos: Vec<ProdutoRentavel>,
    /// O total geral.
    pub total: Totais,
}

#[derive(Clone, Default)]
struct Acumulado {
    receita: Dinheiro,
    receita_com_custo: Dinheiro,
    /// A receita com custo, somada por percentual de encargos.
    por_encargos: Vec<(Percentual, Dinheiro)>,
    custo: Dinheiro,
    receita_propria: Dinheiro,
    custo_proprio: Dinheiro,
    produtos: u32,
    sem_custo: u32,
}

struct Medidas {
    margem_bruta: Dinheiro,
    margem_bruta_pct: Option<Percentual>,
    margem_liquida: Dinheiro,
    margem_liquida_pct: Option<Percentual>,
}

impl Acumulado {
    fn somar_encargos(&mut self, encargos: Percentual, receita: Dinheiro) {
        match self.por_encargos.iter_mut().find(|(p, _)| *p == encargos) {
            Some((_, r)) => *r += receita,
            None => self.por_encargos.push((encargos, receita)),
        }
    }

    fn medidas(&self) -> Medidas {
        medidas(self.receita_com_custo, self.custo, &self.por_encargos)
    }
}

/// `por_encargos` reparte `receita_com_custo` pelos percentuais de encargos que incidem nela.
fn medidas(
    receita_com_custo: Dinheiro,
    custo: Dinheiro,
    por_encargos: &[(Percentual, Dinheiro)],
) -> Medidas {
    let bruta = receita_com_custo - custo;
    let liquida = por_encargos
        .iter()
        .map(|(p, r)| r.aplicar(p.complemento(), Arredondamento::MeioAcima))
        .fold(Dinheiro::ZERO, |a, b| a + b)
        - custo;
    let base = receita_com_custo.em_centavos();
    let pct = |parte: Dinheiro| (base > 0).then(|| Percentual::da_razao(parte.em_centavos(), base));
    Medidas {
        margem_bruta: bruta,
        margem_bruta_pct: pct(bruta),
        margem_liquida: liquida,
        margem_liquida_pct: pct(liquida),
    }
}

/// Junta árvore, mapa produto → grupo, receita e custo e devolve a rentabilidade.
///
/// `encargos` são os percentuais que incidem sobre a venda (operação, imposto, comissão, taxas)
/// — a margem líquida é **estimada** com eles, trocando a comissão padrão pela de cada produto.
/// Um `Percentual` serve quando todos os produtos têm os mesmos encargos.
#[must_use]
pub fn calcular(
    nos: &[NoDaEstrutura],
    produtos: &[ProdutoDaEstrutura],
    receitas: &[ReceitaDoProduto],
    custos: &[CustoDoProduto],
    encargos: impl Into<EncargosDaVenda>,
) -> Rentabilidade {
    let encargos: EncargosDaVenda = encargos.into();
    let mut vendido: HashMap<Id, (Quantidade, Dinheiro)> = HashMap::new();
    for r in receitas {
        let e = vendido
            .entry(r.produto)
            .or_insert((Quantidade::interna(0), Dinheiro::ZERO));
        e.0 += r.quantidade;
        e.1 += r.receita;
    }
    let mut custo_por_produto: HashMap<Id, Dinheiro> = HashMap::new();
    for c in custos {
        *custo_por_produto.entry(c.produto).or_insert(Dinheiro::ZERO) += c.custo;
    }
    let indice: HashMap<Id, usize> = nos.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let mapa: HashMap<Id, &ProdutoDaEstrutura> = produtos.iter().map(|p| (p.produto, p)).collect();

    let mut acum = vec![Acumulado::default(); nos.len()];
    let mut total = Acumulado::default();
    let mut sem_estrutura = Dinheiro::ZERO;
    let mut linhas: Vec<ProdutoRentavel> = Vec::with_capacity(vendido.len());

    for (produto, (quantidade, receita)) in vendido {
        let custo = custo_por_produto
            .get(&produto)
            .copied()
            .filter(|c| c.e_positivo());
        let info = mapa.get(&produto);
        let grupo = info.map(|p| p.grupo).filter(|g| indice.contains_key(g));
        let encargos_do_produto = encargos.do_produto(info.and_then(|p| p.comissao));
        let medida = custo.map(|c| medidas(receita, c, &[(encargos_do_produto, receita)]));
        linhas.push(ProdutoRentavel {
            produto,
            nome: info.map(|p| p.nome.clone()).unwrap_or_default(),
            grupo,
            quantidade,
            receita,
            custo,
            margem_bruta: medida.as_ref().map(|m| m.margem_bruta),
            margem_bruta_pct: medida.as_ref().and_then(|m| m.margem_bruta_pct),
            margem_liquida_estimada: medida.as_ref().map(|m| m.margem_liquida),
            margem_liquida_pct: medida.as_ref().and_then(|m| m.margem_liquida_pct),
        });

        let soma = |a: &mut Acumulado, proprio: bool| {
            a.receita += receita;
            a.produtos += 1;
            match custo {
                Some(c) => {
                    a.receita_com_custo += receita;
                    a.somar_encargos(encargos_do_produto, receita);
                    a.custo += c;
                }
                None => a.sem_custo += 1,
            }
            if proprio {
                a.receita_propria += receita;
                a.custo_proprio += custo.unwrap_or(Dinheiro::ZERO);
            }
        };
        soma(&mut total, false);
        match grupo {
            None => sem_estrutura += receita,
            Some(g) => {
                // Sobe pelos pais; o limite protege de um ciclo que não deveria existir.
                let mut atual = Some(g);
                let mut passos = 0;
                while let Some(id) = atual {
                    let Some(&i) = indice.get(&id) else { break };
                    soma(&mut acum[i], id == g);
                    atual = nos[i].pai;
                    passos += 1;
                    if passos > nos.len() {
                        break;
                    }
                }
            }
        }
    }

    linhas.sort_by(|a, b| {
        b.receita
            .cmp(&a.receita)
            .then_with(|| a.nome.cmp(&b.nome))
            .then_with(|| a.produto.cmp(&b.produto))
    });

    let receita_total = total.receita.em_centavos();
    let nos_saida = nos
        .iter()
        .zip(&acum)
        .map(|(n, a)| {
            let m = a.medidas();
            NoDeRentabilidade {
                id: n.id,
                pai: n.pai,
                nome: n.nome.clone(),
                codigo: n.codigo.clone(),
                nivel: n.nivel,
                receita: a.receita,
                receita_com_custo: a.receita_com_custo,
                custo: a.custo,
                margem_bruta: m.margem_bruta,
                margem_bruta_pct: m.margem_bruta_pct,
                margem_liquida_estimada: m.margem_liquida,
                margem_liquida_pct: m.margem_liquida_pct,
                participacao: if receita_total > 0 {
                    Percentual::da_razao(a.receita.em_centavos(), receita_total)
                } else {
                    Percentual::ZERO
                },
                produtos: a.produtos,
                itens_sem_custo: a.sem_custo,
                receita_propria: a.receita_propria,
                custo_proprio: a.custo_proprio,
            }
        })
        .collect();

    let m = total.medidas();
    Rentabilidade {
        nos: nos_saida,
        produtos: linhas,
        total: Totais {
            receita: total.receita,
            receita_com_custo: total.receita_com_custo,
            custo: total.custo,
            margem_bruta: m.margem_bruta,
            margem_bruta_pct: m.margem_bruta_pct,
            margem_liquida_estimada: m.margem_liquida,
            margem_liquida_pct: m.margem_liquida_pct,
            produtos: total.produtos,
            itens_sem_custo: total.sem_custo,
            receita_sem_estrutura: sem_estrutura,
        },
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn no(id: Id, pai: Option<Id>, nome: &str, nivel: u8) -> NoDaEstrutura {
        NoDaEstrutura {
            id,
            pai,
            nome: nome.to_owned(),
            codigo: nome.to_owned(),
            nivel,
        }
    }

    fn prod(produto: Id, grupo: Id, nome: &str) -> ProdutoDaEstrutura {
        ProdutoDaEstrutura {
            produto,
            nome: nome.to_owned(),
            grupo,
            comissao: None,
        }
    }

    fn rec(produto: Id, qtd: i64, reais: i64) -> ReceitaDoProduto {
        ReceitaDoProduto {
            produto,
            quantidade: Quantidade::unidades(qtd),
            receita: Dinheiro::reais(reais),
        }
    }

    fn cus(produto: Id, qtd: i64, reais: i64) -> CustoDoProduto {
        CustoDoProduto {
            produto,
            quantidade: Quantidade::unidades(qtd),
            custo: Dinheiro::reais(reais),
        }
    }

    struct Cenario {
        raiz: Id,
        a: Id,
        b: Id,
        p_raiz: Id,
        p_a: Id,
        p_b: Id,
        nos: Vec<NoDaEstrutura>,
        produtos: Vec<ProdutoDaEstrutura>,
    }

    /// raiz (com produto próprio) › a (produto) e b (produto).
    fn cenario() -> Cenario {
        let (raiz, a, b) = (Id::novo(), Id::novo(), Id::novo());
        let (p_raiz, p_a, p_b) = (Id::novo(), Id::novo(), Id::novo());
        Cenario {
            raiz,
            a,
            b,
            p_raiz,
            p_a,
            p_b,
            nos: vec![
                no(raiz, None, "Raiz", 1),
                no(a, Some(raiz), "A", 2),
                no(b, Some(raiz), "B", 2),
            ],
            produtos: vec![
                prod(p_raiz, raiz, "Direto"),
                prod(p_a, a, "Em A"),
                prod(p_b, b, "Em B"),
            ],
        }
    }

    fn do_no(r: &Rentabilidade, id: Id) -> &NoDeRentabilidade {
        r.nos.iter().find(|n| n.id == id).unwrap()
    }

    #[test]
    fn subarvore_e_a_soma_dos_filhos_mais_o_proprio() {
        let c = cenario();
        let r = calcular(
            &c.nos,
            &c.produtos,
            &[
                rec(c.p_raiz, 1, 100),
                rec(c.p_a, 2, 300),
                rec(c.p_b, 3, 600),
            ],
            &[cus(c.p_raiz, 1, 60), cus(c.p_a, 2, 200), cus(c.p_b, 3, 400)],
            Percentual::ZERO,
        );
        let raiz = do_no(&r, c.raiz);
        let (a, b) = (do_no(&r, c.a), do_no(&r, c.b));
        assert_eq!(raiz.receita, a.receita + b.receita + raiz.receita_propria);
        assert_eq!(raiz.custo, a.custo + b.custo + raiz.custo_proprio);
        assert_eq!(raiz.receita, Dinheiro::reais(1000));
        assert_eq!(raiz.receita_propria, Dinheiro::reais(100));
        assert_eq!(raiz.custo_proprio, Dinheiro::reais(60));
        assert_eq!(raiz.margem_bruta, Dinheiro::reais(340));
        assert_eq!(raiz.margem_bruta_pct, Some(Percentual::pontos(34)));
        assert_eq!(raiz.produtos, 3);
        assert_eq!(a.produtos, 1);
        assert_eq!(a.participacao, Percentual::pontos(30));
        assert_eq!(raiz.participacao, Percentual::CEM);
    }

    #[test]
    fn receita_total_bate_com_a_soma_dos_produtos() {
        let c = cenario();
        let orfao = Id::novo();
        let r = calcular(
            &c.nos,
            &c.produtos,
            &[
                rec(c.p_a, 1, 10),
                rec(c.p_a, 1, 15), // outra origem do mesmo produto
                rec(c.p_b, 1, 20),
                rec(orfao, 1, 7), // produto fora do mapa
            ],
            &[],
            Percentual::ZERO,
        );
        let soma: Dinheiro = r.produtos.iter().map(|p| p.receita).sum();
        assert_eq!(r.total.receita, soma);
        assert_eq!(r.total.receita, Dinheiro::reais(52));
        assert_eq!(r.total.receita_sem_estrutura, Dinheiro::reais(7));
        assert_eq!(r.total.produtos, 3);
        assert_eq!(do_no(&r, c.raiz).receita, Dinheiro::reais(45));
        let pa = r.produtos.iter().find(|p| p.produto == c.p_a).unwrap();
        assert_eq!(pa.quantidade, Quantidade::unidades(2));
        assert_eq!(pa.receita, Dinheiro::reais(25));
    }

    #[test]
    fn produto_sem_custo_entra_na_receita_mas_fica_fora_da_margem() {
        let c = cenario();
        let r = calcular(
            &c.nos,
            &c.produtos,
            &[rec(c.p_a, 1, 100), rec(c.p_b, 1, 100)],
            &[cus(c.p_a, 1, 50)],
            Percentual::ZERO,
        );
        let raiz = do_no(&r, c.raiz);
        assert_eq!(raiz.receita, Dinheiro::reais(200));
        assert_eq!(raiz.receita_com_custo, Dinheiro::reais(100));
        assert_eq!(raiz.margem_bruta, Dinheiro::reais(50));
        assert_eq!(raiz.margem_bruta_pct, Some(Percentual::pontos(50)));
        assert_eq!(raiz.itens_sem_custo, 1);
        let b = do_no(&r, c.b);
        assert_eq!(b.margem_bruta, Dinheiro::ZERO);
        assert_eq!(b.margem_bruta_pct, None, "nunca lucro de 100%");
        let pb = r.produtos.iter().find(|p| p.produto == c.p_b).unwrap();
        assert_eq!(pb.custo, None);
        assert_eq!(pb.margem_bruta, None);
        assert_eq!(r.total.itens_sem_custo, 1);
    }

    #[test]
    fn custo_zero_conta_como_desconhecido_e_custo_sem_receita_e_ignorado() {
        let c = cenario();
        let r = calcular(
            &c.nos,
            &c.produtos,
            &[rec(c.p_a, 1, 100)],
            &[cus(c.p_a, 1, 0), cus(c.p_b, 5, 999)],
            Percentual::ZERO,
        );
        assert_eq!(r.total.itens_sem_custo, 1);
        assert_eq!(r.total.custo, Dinheiro::ZERO);
        assert_eq!(do_no(&r, c.b).produtos, 0);
    }

    #[test]
    fn garantia_tem_receita_zero_e_margem_negativa() {
        let c = cenario();
        let r = calcular(
            &c.nos,
            &c.produtos,
            &[rec(c.p_a, 1, 0)],
            &[cus(c.p_a, 1, 30)],
            Percentual::ZERO,
        );
        let a = do_no(&r, c.a);
        assert_eq!(a.margem_bruta, Dinheiro::reais(-30));
        assert_eq!(a.margem_bruta_pct, None, "sem receita não há percentual");
    }

    #[test]
    fn margem_liquida_estimada_desconta_os_encargos() {
        let c = cenario();
        let r = calcular(
            &c.nos,
            &c.produtos,
            &[rec(c.p_a, 1, 100)],
            &[cus(c.p_a, 1, 60)],
            Percentual::pontos(15),
        );
        let a = do_no(&r, c.a);
        assert_eq!(a.margem_bruta, Dinheiro::reais(40));
        assert_eq!(a.margem_liquida_estimada, Dinheiro::reais(25));
        assert_eq!(a.margem_liquida_pct, Some(Percentual::pontos(25)));
        assert_eq!(r.total.margem_liquida_estimada, Dinheiro::reais(25));
    }

    #[test]
    fn comissao_por_estrutura_troca_a_padrao_pela_do_produto() {
        let mut c = cenario();
        // Encargos de 15% com 5% de comissão padrão; o produto de B tem 10% de comissão.
        c.produtos[2].comissao = Some(Percentual::pontos(10));
        let encargos = EncargosDaVenda {
            total: Percentual::pontos(15),
            comissao_padrao: Percentual::pontos(5),
        };
        let r = calcular(
            &c.nos,
            &c.produtos,
            &[rec(c.p_a, 1, 100), rec(c.p_b, 1, 100)],
            &[cus(c.p_a, 1, 60), cus(c.p_b, 1, 60)],
            encargos,
        );
        // A: 100 × 85% − 60 = 25. B: 100 × 80% − 60 = 20. Raiz: as duas somas.
        assert_eq!(do_no(&r, c.a).margem_liquida_estimada, Dinheiro::reais(25));
        assert_eq!(do_no(&r, c.b).margem_liquida_estimada, Dinheiro::reais(20));
        assert_eq!(
            do_no(&r, c.raiz).margem_liquida_estimada,
            Dinheiro::reais(45)
        );
        assert_eq!(r.total.margem_liquida_estimada, Dinheiro::reais(45));
        let em_b = r.produtos.iter().find(|p| p.produto == c.p_b).unwrap();
        assert_eq!(em_b.margem_liquida_pct, Some(Percentual::pontos(20)));

        // Sem comissão própria em ninguém, o resultado é o de um percentual único.
        let c = cenario();
        let entrada = (
            [rec(c.p_a, 3, 333), rec(c.p_b, 7, 777)],
            [cus(c.p_a, 3, 100), cus(c.p_b, 7, 300)],
        );
        let por_estrutura = calcular(&c.nos, &c.produtos, &entrada.0, &entrada.1, encargos);
        let unico = calcular(
            &c.nos,
            &c.produtos,
            &entrada.0,
            &entrada.1,
            Percentual::pontos(15),
        );
        assert_eq!(por_estrutura, unico);
    }

    #[test]
    fn arvore_vazia_e_sem_vendas_nao_dividem_por_zero() {
        let vazio = calcular(&[], &[], &[], &[], Percentual::pontos(10));
        assert_eq!(vazio, Rentabilidade::default());
        assert_eq!(vazio.total.margem_bruta_pct, None);

        let c = cenario();
        let r = calcular(&c.nos, &c.produtos, &[], &[], Percentual::pontos(10));
        assert_eq!(r.nos.len(), 3);
        assert!(r.nos.iter().all(|n| n.participacao == Percentual::ZERO
            && n.margem_bruta_pct.is_none()
            && n.margem_liquida_pct.is_none()
            && n.produtos == 0));
        assert_eq!(r.total.receita, Dinheiro::ZERO);

        // Árvore vazia, mas com venda: tudo vai para "sem estrutura".
        let id = Id::novo();
        let r = calcular(&[], &[], &[rec(id, 1, 40)], &[], Percentual::ZERO);
        assert_eq!(r.total.receita_sem_estrutura, Dinheiro::reais(40));
        assert_eq!(r.produtos.len(), 1);
    }

    #[test]
    fn grupo_fora_da_arvore_e_ciclo_nao_travam() {
        let c = cenario();
        let fantasma = Id::novo();
        let p = Id::novo();
        let mut produtos = c.produtos.clone();
        produtos.push(prod(p, fantasma, "Perdido"));
        let r = calcular(&c.nos, &produtos, &[rec(p, 1, 9)], &[], Percentual::ZERO);
        assert_eq!(r.total.receita_sem_estrutura, Dinheiro::reais(9));

        let (x, y) = (Id::novo(), Id::novo());
        let ciclo = [no(x, Some(y), "X", 1), no(y, Some(x), "Y", 2)];
        let p2 = Id::novo();
        let r = calcular(
            &ciclo,
            &[prod(p2, x, "P")],
            &[rec(p2, 1, 5)],
            &[],
            Percentual::ZERO,
        );
        assert_eq!(r.total.receita, Dinheiro::reais(5));
    }
}
