//! Inventário com **contagem cega** e congelamento do local contado.
//!
//! `docs/modulos/estoque.md` §4 e §11.5: `quantidade_sistema` é congelada no início e só
//! é revelada ao operador **depois** de `EncerrarInventario` — pela mesma razão do
//! fechamento de caixa cego (medir a realidade, não confirmá-la). Domínio puro.

use cardeal_kernel::{Id, Instante, Quantidade};
use serde::{Deserialize, Serialize};

use crate::erros::ErroEstoque;

/// O estado de um [`Inventario`] (`docs/modulos/estoque.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoInventario {
    /// Criado, itens ainda não listados para contagem.
    Planejado,
    /// Contagem em andamento.
    EmContagem,
    /// Local travado — nenhum movimento é aceito (`ErroEstoque::LocalCongelado`).
    Congelado,
    /// Todas as divergências foram revisadas.
    Conferido,
    /// Ajustes gerados, trava liberada. Terminal.
    Encerrado,
}

impl EstadoInventario {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Planejado => "Planejado",
            Self::EmContagem => "EmContagem",
            Self::Congelado => "Congelado",
            Self::Conferido => "Conferido",
            Self::Encerrado => "Encerrado",
        }
    }

    /// Verdadeiro se o local está travado para movimento neste estado.
    #[must_use]
    pub const fn trava_o_local(self) -> bool {
        matches!(self, Self::Congelado)
    }
}

/// Um item da contagem — a quantidade do sistema é congelada; a contada entra depois.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContagemItem {
    /// Identidade.
    pub id: Id,
    /// O produto.
    pub produto: Id,
    /// A variação.
    pub variacao: Option<Id>,
    /// O lote.
    pub lote: Option<Id>,
    /// Quantidade que o sistema registrava no início — **não revelada** até o encerramento.
    pub quantidade_sistema: Quantidade,
    /// Quantidade contada pelo operador.
    pub quantidade_contada: Option<Quantidade>,
    /// `contada − sistema` (preenchida junto da contagem).
    pub divergencia: Option<Quantidade>,
    /// Quem contou.
    pub contado_por: Option<Id>,
    /// Quando contou.
    pub contado_em: Option<Instante>,
}

impl ContagemItem {
    /// Verdadeiro se o item já foi contado.
    #[must_use]
    pub const fn contado(&self) -> bool {
        self.quantidade_contada.is_some()
    }
}

/// Um ajuste a aplicar no saldo por causa de uma divergência de contagem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AjusteInventario {
    /// O produto.
    pub produto: Id,
    /// A variação.
    pub variacao: Option<Id>,
    /// O lote.
    pub lote: Option<Id>,
    /// O delta a somar ao disponível (positivo = sobra, negativo = falta).
    pub delta: Quantidade,
}

/// Um inventário de um local.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Inventario {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O local sob contagem.
    pub local: Id,
    /// Descrição.
    pub descricao: String,
    /// O estado.
    pub estado: EstadoInventario,
    /// Quando a contagem começou.
    pub iniciado_em: Option<Instante>,
    /// Quando foi encerrado.
    pub encerrado_em: Option<Instante>,
    /// Quem criou.
    pub criado_por: Id,
    /// Os itens a contar.
    pub itens: Vec<ContagemItem>,
}

impl Inventario {
    /// Cria um inventário planejado com os itens (e seus saldos de sistema já congelados).
    #[must_use]
    pub fn criar(
        empresa: Id,
        local: Id,
        descricao: impl Into<String>,
        criado_por: Id,
        itens: Vec<ContagemItem>,
    ) -> Self {
        Self {
            id: Id::novo(),
            empresa,
            local,
            descricao: descricao.into(),
            estado: EstadoInventario::Planejado,
            iniciado_em: None,
            encerrado_em: None,
            criado_por,
            itens,
        }
    }

    /// `Planejado → EmContagem`.
    ///
    /// # Errors
    /// [`ErroEstoque::EstadoDeInventarioInvalido`].
    pub fn iniciar_contagem(&mut self, agora: Instante) -> Result<(), ErroEstoque> {
        self.exigir(EstadoInventario::Planejado)?;
        self.estado = EstadoInventario::EmContagem;
        self.iniciado_em = Some(agora);
        Ok(())
    }

    /// `EmContagem → Congelado` — a partir daqui o local recusa movimento.
    ///
    /// # Errors
    /// [`ErroEstoque::EstadoDeInventarioInvalido`].
    pub fn congelar(&mut self) -> Result<(), ErroEstoque> {
        self.exigir(EstadoInventario::EmContagem)?;
        self.estado = EstadoInventario::Congelado;
        Ok(())
    }

    /// Registra a contagem de um item, **sem revelar** `quantidade_sistema`.
    ///
    /// # Errors
    /// [`ErroEstoque::EstadoDeInventarioInvalido`] fora de `EmContagem`/`Congelado`;
    /// [`ErroEstoque::QuantidadeInvalida`] se o item não existe ou a contagem é negativa.
    pub fn registrar_contagem(
        &mut self,
        item: Id,
        contada: Quantidade,
        operador: Id,
        agora: Instante,
    ) -> Result<(), ErroEstoque> {
        if !matches!(
            self.estado,
            EstadoInventario::EmContagem | EstadoInventario::Congelado
        ) {
            return Err(ErroEstoque::EstadoDeInventarioInvalido {
                atual: self.estado.rotulo(),
                esperado: "EmContagem/Congelado",
            });
        }
        if contada.e_negativa() {
            return Err(ErroEstoque::QuantidadeInvalida);
        }
        let alvo = self
            .itens
            .iter_mut()
            .find(|i| i.id == item)
            .ok_or(ErroEstoque::QuantidadeInvalida)?;
        alvo.divergencia = Some(contada - alvo.quantidade_sistema);
        alvo.quantidade_contada = Some(contada);
        alvo.contado_por = Some(operador);
        alvo.contado_em = Some(agora);
        Ok(())
    }

    /// `Congelado → Conferido` — exige todos os itens contados.
    ///
    /// # Errors
    /// [`ErroEstoque::EstadoDeInventarioInvalido`], [`ErroEstoque::ContagensPendentes`].
    pub fn conferir(&mut self) -> Result<(), ErroEstoque> {
        self.exigir(EstadoInventario::Congelado)?;
        let pendentes = self.itens.iter().filter(|i| !i.contado()).count();
        if pendentes > 0 {
            return Err(ErroEstoque::ContagensPendentes(pendentes));
        }
        self.estado = EstadoInventario::Conferido;
        Ok(())
    }

    /// `Conferido → Encerrado` — devolve os ajustes das divergências não nulas e libera a
    /// trava do local.
    ///
    /// # Errors
    /// [`ErroEstoque::EstadoDeInventarioInvalido`].
    pub fn encerrar(&mut self, agora: Instante) -> Result<Vec<AjusteInventario>, ErroEstoque> {
        self.exigir(EstadoInventario::Conferido)?;
        let ajustes = self
            .itens
            .iter()
            .filter_map(|i| {
                let delta = i.divergencia?;
                (!delta.e_zero()).then_some(AjusteInventario {
                    produto: i.produto,
                    variacao: i.variacao,
                    lote: i.lote,
                    delta,
                })
            })
            .collect();
        self.estado = EstadoInventario::Encerrado;
        self.encerrado_em = Some(agora);
        Ok(ajustes)
    }

    fn exigir(&self, esperado: EstadoInventario) -> Result<(), ErroEstoque> {
        if self.estado == esperado {
            Ok(())
        } else {
            Err(ErroEstoque::EstadoDeInventarioInvalido {
                atual: self.estado.rotulo(),
                esperado: esperado.rotulo(),
            })
        }
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(sistema: i64) -> ContagemItem {
        ContagemItem {
            id: Id::novo(),
            produto: Id::novo(),
            variacao: None,
            lote: None,
            quantidade_sistema: Quantidade::unidades(sistema),
            quantidade_contada: None,
            divergencia: None,
            contado_por: None,
            contado_em: None,
        }
    }

    fn agora() -> Instante {
        Instante::agora()
    }

    #[test]
    fn fluxo_completo_gera_ajustes_das_divergencias() {
        let itens = vec![item(100), item(50), item(30)];
        let (id0, id1, id2) = (itens[0].id, itens[1].id, itens[2].id);
        let mut inv = Inventario::criar(Id::novo(), Id::novo(), "Depósito", Id::novo(), itens);

        inv.iniciar_contagem(agora()).unwrap();
        inv.congelar().unwrap();
        assert!(inv.estado.trava_o_local());

        inv.registrar_contagem(id0, Quantidade::unidades(98), Id::novo(), agora())
            .unwrap();
        inv.registrar_contagem(id1, Quantidade::unidades(50), Id::novo(), agora())
            .unwrap();

        // Falta contar o item 2.
        assert!(matches!(
            inv.conferir().unwrap_err(),
            ErroEstoque::ContagensPendentes(1)
        ));

        inv.registrar_contagem(id2, Quantidade::unidades(33), Id::novo(), agora())
            .unwrap();
        inv.conferir().unwrap();

        let ajustes = inv.encerrar(agora()).unwrap();
        assert_eq!(inv.estado, EstadoInventario::Encerrado);
        // item0: -2, item2: +3; item1 sem divergência não gera ajuste.
        assert_eq!(ajustes.len(), 2);
        assert!(ajustes
            .iter()
            .any(|a| a.produto == inv.itens[0].produto && a.delta == Quantidade::unidades(-2)));
        assert!(ajustes
            .iter()
            .any(|a| a.produto == inv.itens[2].produto && a.delta == Quantidade::unidades(3)));
    }

    #[test]
    fn transicoes_fora_de_ordem_falham() {
        let mut inv = Inventario::criar(Id::novo(), Id::novo(), "D", Id::novo(), vec![item(1)]);
        assert!(inv.congelar().is_err());
        assert!(inv.encerrar(agora()).is_err());
        inv.iniciar_contagem(agora()).unwrap();
        assert!(inv.iniciar_contagem(agora()).is_err());
    }

    #[test]
    fn contagem_nao_revela_o_sistema_ate_encerrar() {
        // O domínio não expõe a quantidade de sistema num getter separado; ela mora no
        // struct e é responsabilidade da camada de UI/consulta não mostrá-la antes do
        // encerramento. Aqui garantimos que a divergência é computada sem "vazar" para a
        // contagem informada.
        let itens = vec![item(100)];
        let id = itens[0].id;
        let mut inv = Inventario::criar(Id::novo(), Id::novo(), "D", Id::novo(), itens);
        inv.iniciar_contagem(agora()).unwrap();
        inv.congelar().unwrap();
        inv.registrar_contagem(id, Quantidade::unidades(90), Id::novo(), agora())
            .unwrap();
        let it = &inv.itens[0];
        assert_eq!(it.quantidade_contada, Some(Quantidade::unidades(90)));
        assert_eq!(it.divergencia, Some(Quantidade::unidades(-10)));
    }
}
