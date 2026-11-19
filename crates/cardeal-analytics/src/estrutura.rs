//! O **recorte pela estrutura mercadológica**, para qualquer relatório: "este produto está
//! dentro de Bebidas › Sucos?" e "quanto deu cada Grupo/Subgrupo/Categoria/Subcategoria?".
//!
//! Domínio puro e genérico: recebe os nós da árvore e a que grupo pertence cada produto, e soma
//! qualquer medida aditiva (dinheiro, quantidade, contagem). Uma tela de compras, de OS, de PDV ou
//! a curva ABC usam o mesmo recorte — nenhuma remonta a árvore sozinha.
//!
//! Vale a classificação **de hoje**: o produto que mudou de grupo leva o histórico junto (é o que
//! os relatórios de pequeno comércio esperam, e o que a tela avisa).

use std::collections::HashMap;
use std::ops::AddAssign;

use cardeal_kernel::Id;

use crate::rentabilidade::NoDaEstrutura;

/// A árvore e a classificação dos produtos, prontas para recortar e somar.
#[derive(Debug, Clone, Default)]
pub struct RecorteEstrutura {
    nos: Vec<NoDaEstrutura>,
    indice: HashMap<Id, usize>,
    grupo_do_produto: HashMap<Id, Id>,
}

/// O total de um nó: o dele e de tudo abaixo (`subarvore`) e só dos produtos ligados direto a ele
/// (`proprio`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoSomado<M> {
    /// O nó.
    pub no: NoDaEstrutura,
    /// O nó e todos os descendentes.
    pub subarvore: M,
    /// Só os produtos classificados direto neste nó.
    pub proprio: M,
    /// Quantos produtos com valor entraram na subárvore.
    pub produtos: u32,
}

/// O resultado de somar uma medida pela árvore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SomaPorEstrutura<M> {
    /// Os nós, na ordem recebida (pré-ordem), cada um com o seu total. Nó sem valor algum na
    /// subárvore fica de fora.
    pub nos: Vec<NoSomado<M>>,
    /// O total de tudo.
    pub total: M,
    /// O que veio de produto sem grupo conhecido (fora da árvore).
    pub sem_estrutura: M,
}

impl RecorteEstrutura {
    /// Monta o recorte: os nós (pré-ordem, como a árvore os devolve) e `(produto, grupo)`.
    #[must_use]
    pub fn novo(nos: Vec<NoDaEstrutura>, produtos: impl IntoIterator<Item = (Id, Id)>) -> Self {
        let indice = nos.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        Self {
            nos,
            indice,
            grupo_do_produto: produtos.into_iter().collect(),
        }
    }

    /// Os nós da árvore.
    #[must_use]
    pub fn nos(&self) -> &[NoDaEstrutura] {
        &self.nos
    }

    /// O grupo do produto, se conhecido e na árvore.
    #[must_use]
    pub fn grupo_do_produto(&self, produto: Id) -> Option<Id> {
        self.grupo_do_produto
            .get(&produto)
            .copied()
            .filter(|g| self.indice.contains_key(g))
    }

    /// O grupo e os acima dele, do mais específico à raiz. Para no teto de profundidade (dados
    /// corrompidos com ciclo não travam).
    #[must_use]
    pub fn ancestrais(&self, grupo: Id) -> Vec<Id> {
        let mut v = Vec::new();
        let mut atual = Some(grupo);
        while let Some(g) = atual {
            if v.len() > self.nos.len() || !self.indice.contains_key(&g) {
                break;
            }
            v.push(g);
            atual = self.nos[self.indice[&g]].pai;
        }
        v
    }

    /// O produto está no `filtro` (o grupo ou qualquer descendente)? Sem filtro, todos estão;
    /// com filtro, o produto sem grupo conhecido não está.
    #[must_use]
    pub fn contem(&self, filtro: Option<Id>, produto: Id) -> bool {
        let Some(filtro) = filtro else {
            return true;
        };
        self.grupo_do_produto(produto)
            .is_some_and(|g| self.ancestrais(g).contains(&filtro))
    }

    /// O caminho do grupo do produto ("Bebidas › Sucos"); vazio sem grupo.
    #[must_use]
    pub fn caminho(&self, produto: Id) -> String {
        let Some(g) = self.grupo_do_produto(produto) else {
            return String::new();
        };
        let mut nomes: Vec<&str> = self
            .ancestrais(g)
            .iter()
            .map(|a| self.nos[self.indice[a]].nome.as_str())
            .collect();
        nomes.reverse();
        nomes.join(" › ")
    }

    /// Soma a medida de cada produto no grupo dele e em todos os acima.
    #[must_use]
    pub fn somar<M>(&self, valores: impl IntoIterator<Item = (Id, M)>) -> SomaPorEstrutura<M>
    where
        M: Copy + Default + AddAssign + PartialEq,
    {
        let mut sub = vec![M::default(); self.nos.len()];
        let mut proprio = vec![M::default(); self.nos.len()];
        let mut contagem = vec![0u32; self.nos.len()];
        let mut total = M::default();
        let mut sem_estrutura = M::default();
        let mut por_produto: HashMap<Id, M> = HashMap::new();
        for (produto, m) in valores {
            *por_produto.entry(produto).or_default() += m;
        }
        for (produto, m) in por_produto {
            total += m;
            let Some(g) = self.grupo_do_produto(produto) else {
                sem_estrutura += m;
                continue;
            };
            proprio[self.indice[&g]] += m;
            for a in self.ancestrais(g) {
                let i = self.indice[&a];
                sub[i] += m;
                contagem[i] += 1;
            }
        }
        let nos = self
            .nos
            .iter()
            .enumerate()
            .filter(|(i, _)| contagem[*i] > 0)
            .map(|(i, n)| NoSomado {
                no: n.clone(),
                subarvore: sub[i],
                proprio: proprio[i],
                produtos: contagem[i],
            })
            .collect();
        SomaPorEstrutura {
            nos,
            total,
            sem_estrutura,
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use cardeal_kernel::Dinheiro;

    fn no(id: Id, pai: Option<Id>, nome: &str, nivel: u8) -> NoDaEstrutura {
        NoDaEstrutura {
            id,
            pai,
            nome: nome.to_owned(),
            codigo: String::new(),
            nivel,
        }
    }

    /// Bebidas › (Sucos, Refrigerantes) e Limpeza; um produto em cada folha e um em Bebidas.
    struct Cenario {
        recorte: RecorteEstrutura,
        bebidas: Id,
        sucos: Id,
        limpeza: Id,
        suco: Id,
        cola: Id,
        sabao: Id,
        agua: Id,
    }

    fn cenario() -> Cenario {
        let (bebidas, sucos, refri, limpeza) = (Id::novo(), Id::novo(), Id::novo(), Id::novo());
        let (suco, cola, sabao, agua) = (Id::novo(), Id::novo(), Id::novo(), Id::novo());
        let recorte = RecorteEstrutura::novo(
            vec![
                no(bebidas, None, "Bebidas", 1),
                no(sucos, Some(bebidas), "Sucos", 2),
                no(refri, Some(bebidas), "Refrigerantes", 2),
                no(limpeza, None, "Limpeza", 1),
            ],
            [
                (suco, sucos),
                (cola, refri),
                (sabao, limpeza),
                (agua, bebidas),
            ],
        );
        Cenario {
            recorte,
            bebidas,
            sucos,
            limpeza,
            suco,
            cola,
            sabao,
            agua,
        }
    }

    #[test]
    fn o_filtro_pega_o_grupo_e_tudo_abaixo() {
        let c = cenario();
        assert!(c.recorte.contem(Some(c.bebidas), c.suco));
        assert!(c.recorte.contem(Some(c.bebidas), c.agua));
        assert!(c.recorte.contem(Some(c.sucos), c.suco));
        assert!(!c.recorte.contem(Some(c.sucos), c.cola));
        assert!(!c.recorte.contem(Some(c.bebidas), c.sabao));
        assert!(c.recorte.contem(None, c.sabao), "sem filtro, tudo");
        assert!(
            !c.recorte.contem(Some(c.bebidas), Id::novo()),
            "produto fora da árvore não entra num filtro"
        );
    }

    #[test]
    fn o_caminho_vai_da_raiz_ao_grupo_do_produto() {
        let c = cenario();
        assert_eq!(c.recorte.caminho(c.suco), "Bebidas › Sucos");
        assert_eq!(c.recorte.caminho(c.sabao), "Limpeza");
        assert_eq!(c.recorte.caminho(Id::novo()), "");
    }

    #[test]
    fn a_subarvore_soma_os_filhos_e_o_proprio() {
        let c = cenario();
        let s = c.recorte.somar([
            (c.suco, Dinheiro::reais(10)),
            (c.cola, Dinheiro::reais(20)),
            (c.agua, Dinheiro::reais(5)),
            (c.sabao, Dinheiro::reais(7)),
            (c.suco, Dinheiro::reais(1)),
        ]);
        let de = |id: Id| s.nos.iter().find(|n| n.no.id == id).unwrap();
        assert_eq!(de(c.bebidas).subarvore, Dinheiro::reais(36));
        assert_eq!(de(c.bebidas).proprio, Dinheiro::reais(5));
        assert_eq!(de(c.bebidas).produtos, 3);
        assert_eq!(
            de(c.sucos).subarvore,
            Dinheiro::reais(11),
            "o mesmo produto soma"
        );
        assert_eq!(de(c.limpeza).subarvore, Dinheiro::reais(7));
        assert_eq!(s.total, Dinheiro::reais(43));
        assert_eq!(s.sem_estrutura, Dinheiro::ZERO);
    }

    #[test]
    fn produto_sem_grupo_vai_para_fora_da_estrutura_e_no_sem_valor_some() {
        let c = cenario();
        let s = c.recorte.somar([
            (c.suco, Dinheiro::reais(3)),
            (Id::novo(), Dinheiro::reais(4)),
        ]);
        assert_eq!(s.total, Dinheiro::reais(7));
        assert_eq!(s.sem_estrutura, Dinheiro::reais(4));
        assert!(
            s.nos.iter().all(|n| n.no.id != c.limpeza),
            "Limpeza não vendeu"
        );
        assert_eq!(s.nos.len(), 2, "Bebidas e Sucos");
    }

    #[test]
    fn ciclo_nos_dados_nao_trava() {
        let (a, b, p) = (Id::novo(), Id::novo(), Id::novo());
        let r = RecorteEstrutura::novo(
            vec![no(a, Some(b), "A", 1), no(b, Some(a), "B", 2)],
            [(p, a)],
        );
        assert!(r.ancestrais(a).len() <= 3);
        let _ = r.somar([(p, 1i64)]);
        assert!(r.contem(Some(b), p));
    }
}
