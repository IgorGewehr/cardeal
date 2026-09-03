//! Limite de crédito e score de pagamento.
//!
//! `docs/modulos/clientes.md` §4 e §11.4–11.5: **bloqueio é automático; liberação é sempre
//! manual e auditada**; **score é sempre derivado, nunca editável**. Domínio puro — o
//! "em aberto" do cliente vem do financeiro por consulta, nunca é duplicado aqui.

use cardeal_kernel::{Dinheiro, Id, Instante, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroClientes;

/// A situação de um [`LimiteCredito`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SituacaoCredito {
    /// Venda a prazo permitida.
    Liberado,
    /// Venda a prazo barrada até liberação por supervisor.
    Bloqueado,
}

/// O limite de crédito de um cliente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimiteCredito {
    /// Identidade.
    pub id: Id,
    /// A pessoa (papel `Cliente`).
    pub pessoa: Id,
    /// O teto de exposição a prazo.
    pub limite: Dinheiro,
    /// A situação atual.
    pub situacao: SituacaoCredito,
    /// Motivo do bloqueio (obrigatório quando `Bloqueado`).
    pub motivo_bloqueio: Option<String>,
    /// Quando foi bloqueado.
    pub bloqueado_em: Option<Instante>,
    /// O supervisor que liberou por último.
    pub liberado_por: Option<Id>,
    /// Última revisão (definição/edição de limite ou liberação).
    pub revisado_em: Instante,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

/// O crédito ainda disponível dado o total em aberto no financeiro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisponivelCredito {
    /// `limite − em_aberto` (pode ser negativo — é o "estouro").
    pub disponivel: Dinheiro,
    /// Verdadeiro se `em_aberto > limite`.
    pub excedido: Dinheiro,
}

impl LimiteCredito {
    /// Define ou ajusta o limite de um cliente.
    ///
    /// # Errors
    /// [`ErroClientes::LimiteNegativo`].
    pub fn definir(pessoa: Id, limite: Dinheiro, agora: Instante) -> Result<Self, ErroClientes> {
        if limite.e_negativo() {
            return Err(ErroClientes::LimiteNegativo);
        }
        Ok(Self {
            id: Id::novo(),
            pessoa,
            limite,
            situacao: SituacaoCredito::Liberado,
            motivo_bloqueio: None,
            bloqueado_em: None,
            liberado_por: None,
            revisado_em: agora,
            versao: Versao::INICIAL,
        })
    }

    /// Ajusta o valor do limite (não mexe na situação).
    ///
    /// # Errors
    /// [`ErroClientes::LimiteNegativo`].
    pub fn ajustar(&mut self, novo_limite: Dinheiro, agora: Instante) -> Result<(), ErroClientes> {
        if novo_limite.e_negativo() {
            return Err(ErroClientes::LimiteNegativo);
        }
        self.limite = novo_limite;
        self.revisado_em = agora;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// O disponível dado o total em aberto (soma de `valor − valor_baixado` das parcelas
    /// abertas do cliente, obtida do financeiro por consulta).
    #[must_use]
    pub fn disponivel(&self, em_aberto: Dinheiro) -> DisponivelCredito {
        let disponivel = self.limite - em_aberto;
        DisponivelCredito {
            disponivel,
            excedido: (em_aberto - self.limite).nao_negativo(),
        }
    }

    /// Bloqueia o crédito — chamado automaticamente quando uma venda/consulta detecta
    /// estouro. Idempotente: rebloquear só atualiza o motivo.
    pub fn bloquear(&mut self, motivo: impl Into<String>, agora: Instante) {
        self.situacao = SituacaoCredito::Bloqueado;
        self.motivo_bloqueio = Some(motivo.into());
        self.bloqueado_em = Some(agora);
        self.versao = self.versao.proxima();
    }

    /// Libera o crédito. Sempre manual, sempre com autor e motivo (`docs/modulos/clientes.md`
    /// §11.4) — o sistema nunca libera sozinho, mesmo que o cliente tenha pago tudo.
    ///
    /// # Errors
    /// [`ErroClientes::CreditoNaoBloqueado`] se já está liberado;
    /// [`ErroClientes::LiberacaoSemMotivo`] se o motivo vier vazio.
    pub fn liberar(
        &mut self,
        supervisor: Id,
        motivo: &str,
        agora: Instante,
    ) -> Result<(), ErroClientes> {
        if self.situacao == SituacaoCredito::Liberado {
            return Err(ErroClientes::CreditoNaoBloqueado);
        }
        if motivo.trim().is_empty() {
            return Err(ErroClientes::LiberacaoSemMotivo);
        }
        self.situacao = SituacaoCredito::Liberado;
        self.motivo_bloqueio = None;
        self.bloqueado_em = None;
        self.liberado_por = Some(supervisor);
        self.revisado_em = agora;
        self.versao = self.versao.proxima();
        Ok(())
    }
}

/// Um evento de pagamento que move o score, derivado de `financeiro.parcela_baixada.v1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventoCredito {
    /// Parcela paga em dia ou adiantada.
    PagamentoEmDia,
    /// Parcela paga com atraso.
    PagamentoComAtraso {
        /// Dias de atraso na baixa.
        dias: u16,
    },
    /// Parcela venceu e segue sem baixa (marcada por rotina).
    Inadimplencia,
    /// Título renegociado.
    TituloRenegociado,
    /// Reversão de uma baixa antes contabilizada (estorno).
    BaixaEstornada,
}

/// O menor score possível.
pub const SCORE_MINIMO: i32 = 0;
/// O maior score possível.
pub const SCORE_MAXIMO: i32 = 1000;
/// O score de partida de um cliente sem histórico.
pub const SCORE_INICIAL: i32 = 700;

/// Score de pagamento de um cliente: 0–1000, **sempre derivado** de [`EventoCredito`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Score(i32);

impl Default for Score {
    fn default() -> Self {
        Self(SCORE_INICIAL)
    }
}

impl Score {
    /// O score inicial (sem histórico).
    #[must_use]
    pub const fn inicial() -> Self {
        Self(SCORE_INICIAL)
    }

    /// O valor numérico (0–1000).
    #[must_use]
    pub const fn valor(self) -> i32 {
        self.0
    }

    /// Uma etiqueta legível para a UI.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        if self.0 >= 750 {
            "bom pagador"
        } else if self.0 >= 500 {
            "regular"
        } else {
            "atenção"
        }
    }

    /// Aplica um evento e devolve o novo score, sempre dentro de 0–1000. Atrasos pesam
    /// proporcionalmente aos dias; inadimplência e estorno derrubam mais.
    #[must_use]
    pub fn aplicar(self, evento: EventoCredito) -> Self {
        let delta = match evento {
            EventoCredito::PagamentoEmDia => 15,
            EventoCredito::PagamentoComAtraso { dias } => -2 - i32::from(dias.min(60)),
            EventoCredito::Inadimplencia => -120,
            EventoCredito::TituloRenegociado => -40,
            EventoCredito::BaixaEstornada => -15,
        };
        Self((self.0 + delta).clamp(SCORE_MINIMO, SCORE_MAXIMO))
    }
}

#[cfg(test)]
mod testes {
    use proptest::prelude::*;

    use super::*;

    fn agora() -> Instante {
        Instante::agora()
    }

    #[test]
    fn limite_negativo_e_recusado() {
        assert_eq!(
            LimiteCredito::definir(Id::novo(), Dinheiro::reais(-1), agora()).unwrap_err(),
            ErroClientes::LimiteNegativo
        );
    }

    #[test]
    fn disponivel_e_estouro() {
        let lc = LimiteCredito::definir(Id::novo(), Dinheiro::reais(5000), agora()).unwrap();
        let d = lc.disponivel(Dinheiro::reais(5340));
        assert_eq!(d.disponivel, Dinheiro::reais(-340));
        assert_eq!(d.excedido, Dinheiro::reais(340));

        let d = lc.disponivel(Dinheiro::reais(1000));
        assert_eq!(d.disponivel, Dinheiro::reais(4000));
        assert_eq!(d.excedido, Dinheiro::ZERO);
    }

    #[test]
    fn bloqueio_automatico_liberacao_manual_e_auditada() {
        let mut lc = LimiteCredito::definir(Id::novo(), Dinheiro::reais(5000), agora()).unwrap();
        lc.bloquear("limite excedido em R$ 340,00", agora());
        assert_eq!(lc.situacao, SituacaoCredito::Bloqueado);

        // Liberar sem motivo falha; liberar de novo depois de liberado falha.
        assert_eq!(
            lc.liberar(Id::novo(), "  ", agora()).unwrap_err(),
            ErroClientes::LiberacaoSemMotivo
        );
        let supervisor = Id::novo();
        lc.liberar(
            supervisor,
            "cliente antigo, autorizado pelo gerente",
            agora(),
        )
        .unwrap();
        assert_eq!(lc.situacao, SituacaoCredito::Liberado);
        assert_eq!(lc.liberado_por, Some(supervisor));
        assert_eq!(
            lc.liberar(Id::novo(), "de novo", agora()).unwrap_err(),
            ErroClientes::CreditoNaoBloqueado
        );
    }

    #[test]
    fn score_deriva_dos_eventos_e_fica_na_faixa() {
        let mut s = Score::inicial();
        for _ in 0..30 {
            s = s.aplicar(EventoCredito::PagamentoEmDia);
        }
        assert_eq!(s.valor(), SCORE_MAXIMO);
        assert_eq!(s.rotulo(), "bom pagador");

        for _ in 0..20 {
            s = s.aplicar(EventoCredito::Inadimplencia);
        }
        assert_eq!(s.valor(), SCORE_MINIMO);
        assert_eq!(s.rotulo(), "atenção");
    }

    #[test]
    fn atraso_pesa_conforme_os_dias() {
        let base = Score::inicial();
        let leve = base.aplicar(EventoCredito::PagamentoComAtraso { dias: 1 });
        let pesado = base.aplicar(EventoCredito::PagamentoComAtraso { dias: 40 });
        assert!(pesado.valor() < leve.valor());
        assert!(leve.valor() < base.valor());
    }

    fn evento_qualquer() -> impl Strategy<Value = EventoCredito> {
        prop_oneof![
            Just(EventoCredito::PagamentoEmDia),
            (0u16..365).prop_map(|dias| EventoCredito::PagamentoComAtraso { dias }),
            Just(EventoCredito::Inadimplencia),
            Just(EventoCredito::TituloRenegociado),
            Just(EventoCredito::BaixaEstornada),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(400))]

        /// Nenhuma sequência de eventos tira o score da faixa 0–1000
        /// (`docs/modulos/clientes.md` §11.5).
        #[test]
        fn score_nunca_sai_da_faixa(eventos in prop::collection::vec(evento_qualquer(), 0..200)) {
            let mut s = Score::inicial();
            for e in eventos {
                s = s.aplicar(e);
                prop_assert!((SCORE_MINIMO..=SCORE_MAXIMO).contains(&s.valor()));
            }
        }
    }
}
