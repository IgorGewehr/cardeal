//! O recorte pela **estrutura mercadológica** que qualquer tela usa: carrega a árvore e a
//! classificação dos produtos uma vez, mostra o filtro ("Toda a estrutura" ou um
//! Grupo/Subgrupo/Categoria/Subcategoria — vale a subárvore inteira) e desenha uma soma por nó.
//!
//! A tela guarda um [`EstadoRecorte`]; o domínio é `cardeal_analytics::RecorteEstrutura`. Nenhuma
//! tela remonta a árvore sozinha.

use std::collections::HashSet;

use cardeal_analytics::{NoDaEstrutura, RecorteEstrutura, SomaPorEstrutura};
use cardeal_cliente::{Motor, Sessao};
use cardeal_kernel::{Dinheiro, Id};
use cardeal_ui::atoms::{Rotulo, ValorDinheiro};
use cardeal_ui::organisms::{ArvoreHierarquia, EstadoSeletorArvore, NoArvore, SeletorEmArvore};
use mod_estoque::{ArvoreGruposProduto, ProdutoParaLote, ProdutosParaLote};

/// O estado do recorte numa tela.
#[derive(Default)]
pub struct EstadoRecorte {
    /// O nó escolhido; `None` = toda a estrutura.
    pub filtro: Option<Id>,
    seletor: EstadoSeletorArvore,
    recorte: RecorteEstrutura,
    nos_ui: Vec<NoArvore>,
    recolhidos: HashSet<Id>,
}

impl EstadoRecorte {
    /// Lê a árvore e a classificação dos produtos. Sem permissão de ver o estoque, fica vazio (o
    /// filtro não aparece e tudo passa).
    pub fn carregar(&mut self, motor: &Motor, sessao: &Sessao) {
        let arvore: Vec<mod_estoque::NoArvoreGrupo> = motor
            .consultar(
                sessao,
                "estoque.arvore_grupos_produto.v1",
                &ArvoreGruposProduto,
            )
            .unwrap_or_default();
        let produtos: Vec<ProdutoParaLote> = motor
            .consultar(
                sessao,
                "estoque.produtos_para_lote.v1",
                &ProdutosParaLote::default(),
            )
            .unwrap_or_default();
        let nos: Vec<NoDaEstrutura> = arvore
            .iter()
            .map(|n| NoDaEstrutura {
                id: n.id,
                pai: n.pai,
                nome: n.nome.clone(),
                codigo: n.codigo.clone(),
                nivel: n.nivel,
            })
            .collect();
        self.nos_ui = nos_para_ui(&nos);
        self.recorte = RecorteEstrutura::novo(nos, produtos.iter().map(|p| (p.produto, p.grupo)));
        if self
            .filtro
            .is_some_and(|f| !self.nos_ui.iter().any(|n| n.id == f))
        {
            self.filtro = None;
        }
    }

    /// O domínio do recorte.
    pub fn recorte(&self) -> &RecorteEstrutura {
        &self.recorte
    }

    /// Há árvore para filtrar?
    pub fn tem_arvore(&self) -> bool {
        !self.nos_ui.is_empty()
    }

    /// O produto passa no filtro escolhido?
    pub fn contem(&self, produto: Id) -> bool {
        self.recorte.contem(self.filtro, produto)
    }

    /// O filtro, para a barra de filtros da tela (`rotulo` identifica o painel e precisa ser único
    /// na tela). Devolve `true` no quadro em que mudou. Sem árvore, não desenha nada.
    pub fn mostrar_filtro(&mut self, ui: &mut egui::Ui, rotulo: &str) -> bool {
        if !self.tem_arvore() {
            return false;
        }
        SeletorEmArvore::novo(rotulo, &mut self.filtro, &self.nos_ui, &mut self.seletor)
            .marcador("Toda a estrutura")
            .opcao_vazia("Toda a estrutura")
            .sem_rotulo()
            .mostrar(ui)
    }

    /// A árvore com o valor de cada nó (o da subárvore); clicar num nó o põe como filtro.
    /// Devolve `true` se o filtro mudou.
    pub fn mostrar_soma(&mut self, ui: &mut egui::Ui, soma: &SomaPorEstrutura<Dinheiro>) -> bool {
        let com_valor: Vec<NoArvore> = self
            .nos_ui
            .iter()
            .filter(|n| soma.nos.iter().any(|s| s.no.id == n.id))
            .cloned()
            .collect();
        if com_valor.is_empty() {
            ui.add(Rotulo::apoio("Nada no período."));
            return false;
        }
        let com_valor = recalcular_filhos(com_valor);
        let resposta = ArvoreHierarquia::nova(&com_valor, &mut self.recolhidos)
            .selecionado(self.filtro)
            .mostrar(ui, |ui, no| {
                if let Some(s) = soma.nos.iter().find(|s| s.no.id == no.id) {
                    ui.add(ValorDinheiro::novo(s.subarvore).neutro());
                    ui.add(Rotulo::apoio(participacao(s.subarvore, soma.total)));
                }
            });
        if !soma.sem_estrutura.e_zero() {
            ui.add(Rotulo::apoio(format!(
                "Fora da estrutura (produto sem grupo): {}",
                soma.sem_estrutura.formatar_com_simbolo()
            )));
        }
        match resposta.clicado {
            Some(id) => {
                self.filtro = if self.filtro == Some(id) {
                    None
                } else {
                    Some(id)
                };
                true
            }
            None => false,
        }
    }
}

/// "12%" do total (vazio se o total é zero).
fn participacao(valor: Dinheiro, total: Dinheiro) -> String {
    if total.e_zero() {
        return String::new();
    }
    let pct = i128::from(valor.em_centavos()) * 1000 / i128::from(total.em_centavos());
    format!("{},{}%", pct / 10, (pct % 10).abs())
}

/// Os nós da árvore (pré-ordem) como o componente espera.
fn nos_para_ui(nos: &[NoDaEstrutura]) -> Vec<NoArvore> {
    recalcular_filhos(
        nos.iter()
            .map(|n| NoArvore {
                id: n.id,
                nivel: n.nivel,
                rotulo: n.nome.clone(),
                apoio: None,
                tem_filhos: false,
            })
            .collect(),
    )
}

/// Marca quem tem filho visível (o próximo da pré-ordem é mais fundo).
fn recalcular_filhos(mut v: Vec<NoArvore>) -> Vec<NoArvore> {
    for i in 0..v.len() {
        v[i].tem_filhos = v.get(i + 1).is_some_and(|p| p.nivel > v[i].nivel);
    }
    v
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn participacao_com_uma_casa() {
        assert_eq!(
            participacao(Dinheiro::reais(1), Dinheiro::reais(3)),
            "33,3%"
        );
        assert_eq!(
            participacao(Dinheiro::reais(3), Dinheiro::reais(3)),
            "100,0%"
        );
        assert_eq!(participacao(Dinheiro::reais(1), Dinheiro::ZERO), "");
    }
}
