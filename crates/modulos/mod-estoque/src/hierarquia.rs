//! A estrutura mercadológica: árvore de grupos com no máximo quatro níveis —
//! Grupo › Subgrupo › Categoria › Subcategoria. Domínio puro: recebe os nós já lidos e
//! responde "esta mudança cabe na árvore?".
//!
//! O uso reduzido (só Grupo e Subgrupo) não é um modo à parte: basta não criar os níveis
//! de baixo. Habilitar a estrutura completa depois é criar filhos — nada migra.

use std::collections::HashMap;

use cardeal_kernel::Id;

/// A profundidade máxima da árvore (raiz = nível 1).
pub const NIVEL_MAXIMO: u8 = 4;

/// O nome de cada nível, do 1 ao 4.
pub const NOMES_NIVEL: [&str; NIVEL_MAXIMO as usize] =
    ["Grupo", "Subgrupo", "Categoria", "Subcategoria"];

/// O nome do nível (`1..=4`); fora disso, "Nível N".
#[must_use]
pub fn nome_do_nivel(nivel: u8) -> String {
    NOMES_NIVEL
        .get(usize::from(nivel).wrapping_sub(1))
        .map_or_else(|| format!("Nível {nivel}"), |n| (*n).to_owned())
}

/// Um nó da árvore: o grupo e o pai.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NoGrupo {
    /// O grupo.
    pub id: Id,
    /// O pai; `None` na raiz.
    pub pai: Option<Id>,
}

/// Por que uma mudança na árvore foi recusada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroHierarquia {
    /// O grupo (ou o pai) não existe.
    GrupoInexistente,
    /// Passaria de [`NIVEL_MAXIMO`] níveis.
    ProfundidadeExcedida {
        /// O nível que o ponto mais fundo passaria a ocupar.
        nivel_resultante: u8,
    },
    /// Mover um grupo para dentro de si mesmo ou de um descendente.
    Ciclo,
}

/// A árvore em memória, indexada por id.
#[derive(Debug, Default)]
pub struct Arvore {
    pai: HashMap<Id, Option<Id>>,
    filhos: HashMap<Id, Vec<Id>>,
}

impl Arvore {
    /// Monta a árvore a partir dos nós.
    #[must_use]
    pub fn de_nos(nos: &[NoGrupo]) -> Self {
        let mut a = Self::default();
        for n in nos {
            a.pai.insert(n.id, n.pai);
            if let Some(p) = n.pai {
                a.filhos.entry(p).or_default().push(n.id);
            }
        }
        a
    }

    /// O nível do grupo (raiz = 1); `None` se não existe. Protegido contra laço em dados
    /// corrompidos: para no teto e devolve `NIVEL_MAXIMO + 1`.
    #[must_use]
    pub fn nivel(&self, grupo: Id) -> Option<u8> {
        let mut atual = grupo;
        let mut nivel = 1u8;
        loop {
            match self.pai.get(&atual)? {
                None => return Some(nivel),
                Some(p) => {
                    nivel += 1;
                    if nivel > NIVEL_MAXIMO + 1 {
                        return Some(nivel);
                    }
                    atual = *p;
                }
            }
        }
    }

    /// O grupo e seus ancestrais, do mais próximo à raiz — a ordem em que a herança de
    /// margem e de perfil fiscal procura um valor.
    #[must_use]
    pub fn ancestrais(&self, grupo: Id) -> Vec<Id> {
        let mut saida = Vec::new();
        let mut atual = Some(grupo);
        while let Some(g) = atual {
            if saida.contains(&g) || !self.pai.contains_key(&g) {
                break;
            }
            saida.push(g);
            atual = self.pai.get(&g).copied().flatten();
        }
        saida
    }

    /// A altura da subárvore com raiz em `grupo`: 1 para uma folha.
    #[must_use]
    pub fn altura(&self, grupo: Id) -> u8 {
        self.altura_limitada(grupo, 0)
    }

    fn altura_limitada(&self, grupo: Id, fundo: u8) -> u8 {
        if fundo > NIVEL_MAXIMO + 1 {
            return 1;
        }
        1 + self.filhos.get(&grupo).map_or(0, |fs| {
            fs.iter()
                .map(|f| self.altura_limitada(*f, fundo + 1))
                .max()
                .unwrap_or(0)
        })
    }

    /// Todos os descendentes (não inclui o próprio).
    #[must_use]
    pub fn descendentes(&self, grupo: Id) -> Vec<Id> {
        let mut saida = Vec::new();
        let mut pilha = vec![grupo];
        while let Some(g) = pilha.pop() {
            if let Some(fs) = self.filhos.get(&g) {
                for f in fs {
                    if !saida.contains(f) {
                        saida.push(*f);
                        pilha.push(*f);
                    }
                }
            }
        }
        saida
    }

    /// Um novo filho de `pai` (ou raiz, com `None`) cabe na árvore?
    ///
    /// # Errors
    /// [`ErroHierarquia::GrupoInexistente`] ou [`ErroHierarquia::ProfundidadeExcedida`].
    pub fn validar_novo_filho(&self, pai: Option<Id>) -> Result<u8, ErroHierarquia> {
        let Some(p) = pai else { return Ok(1) };
        let nivel_pai = self.nivel(p).ok_or(ErroHierarquia::GrupoInexistente)?;
        let nivel = nivel_pai + 1;
        if nivel > NIVEL_MAXIMO {
            return Err(ErroHierarquia::ProfundidadeExcedida {
                nivel_resultante: nivel,
            });
        }
        Ok(nivel)
    }

    /// Mover `grupo` (com toda a sua subárvore) para baixo de `novo_pai` cabe na árvore?
    ///
    /// # Errors
    /// [`ErroHierarquia`] conforme o motivo.
    pub fn validar_movimento(&self, grupo: Id, novo_pai: Option<Id>) -> Result<(), ErroHierarquia> {
        if !self.pai.contains_key(&grupo) {
            return Err(ErroHierarquia::GrupoInexistente);
        }
        let nivel_pai = match novo_pai {
            None => 0,
            Some(p) => {
                if p == grupo || self.descendentes(grupo).contains(&p) {
                    return Err(ErroHierarquia::Ciclo);
                }
                self.nivel(p).ok_or(ErroHierarquia::GrupoInexistente)?
            }
        };
        let fundo = nivel_pai + self.altura(grupo);
        if fundo > NIVEL_MAXIMO {
            return Err(ErroHierarquia::ProfundidadeExcedida {
                nivel_resultante: fundo,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn cadeia(n: usize) -> (Vec<NoGrupo>, Vec<Id>) {
        let ids: Vec<Id> = (0..n).map(|_| Id::novo()).collect();
        let nos = ids
            .iter()
            .enumerate()
            .map(|(i, id)| NoGrupo {
                id: *id,
                pai: if i == 0 { None } else { Some(ids[i - 1]) },
            })
            .collect();
        (nos, ids)
    }

    #[test]
    fn niveis_e_nomes() {
        let (nos, ids) = cadeia(4);
        let a = Arvore::de_nos(&nos);
        assert_eq!(a.nivel(ids[0]), Some(1));
        assert_eq!(a.nivel(ids[3]), Some(4));
        assert_eq!(nome_do_nivel(1), "Grupo");
        assert_eq!(nome_do_nivel(4), "Subcategoria");
        assert_eq!(a.nivel(Id::novo()), None);
    }

    #[test]
    fn quinto_nivel_e_recusado() {
        let (nos, ids) = cadeia(4);
        let a = Arvore::de_nos(&nos);
        assert_eq!(a.validar_novo_filho(Some(ids[2])), Ok(4));
        assert_eq!(
            a.validar_novo_filho(Some(ids[3])),
            Err(ErroHierarquia::ProfundidadeExcedida {
                nivel_resultante: 5
            })
        );
        assert_eq!(a.validar_novo_filho(None), Ok(1));
        assert_eq!(
            a.validar_novo_filho(Some(Id::novo())),
            Err(ErroHierarquia::GrupoInexistente)
        );
    }

    #[test]
    fn mover_considera_a_altura_da_subarvore() {
        // A › B › C   e   X › Y. Mover B (altura 2) para baixo de Y (nível 2) → fundo 4: cabe.
        // Mover A (altura 3) para baixo de Y → fundo 5: não cabe.
        let (a_b_c, i) = cadeia(3);
        let (x_y, j) = cadeia(2);
        let mut nos = a_b_c;
        nos.extend(x_y);
        let arv = Arvore::de_nos(&nos);
        assert_eq!(arv.altura(i[0]), 3);
        assert_eq!(arv.validar_movimento(i[1], Some(j[1])), Ok(()));
        assert_eq!(
            arv.validar_movimento(i[0], Some(j[1])),
            Err(ErroHierarquia::ProfundidadeExcedida {
                nivel_resultante: 5
            })
        );
    }

    #[test]
    fn mover_para_dentro_de_si_ou_de_descendente_e_ciclo() {
        let (nos, ids) = cadeia(3);
        let a = Arvore::de_nos(&nos);
        assert_eq!(
            a.validar_movimento(ids[0], Some(ids[0])),
            Err(ErroHierarquia::Ciclo)
        );
        assert_eq!(
            a.validar_movimento(ids[0], Some(ids[2])),
            Err(ErroHierarquia::Ciclo)
        );
        // Promover à raiz sempre cabe.
        assert_eq!(a.validar_movimento(ids[2], None), Ok(()));
    }

    #[test]
    fn ancestrais_vao_do_proprio_a_raiz() {
        let (nos, ids) = cadeia(3);
        let a = Arvore::de_nos(&nos);
        assert_eq!(a.ancestrais(ids[2]), vec![ids[2], ids[1], ids[0]]);
        assert_eq!(a.ancestrais(ids[0]), vec![ids[0]]);
        assert!(a.ancestrais(Id::novo()).is_empty());
    }

    #[test]
    fn descendentes_nao_incluem_o_proprio() {
        let (nos, ids) = cadeia(3);
        let a = Arvore::de_nos(&nos);
        let mut d = a.descendentes(ids[0]);
        d.sort();
        let mut esperado = vec![ids[1], ids[2]];
        esperado.sort();
        assert_eq!(d, esperado);
    }
}
