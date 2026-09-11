//! Apontamento de tempo por ordem de serviço — quanto tempo cada técnico gasta em cada
//! trabalho, para alimentar produtividade e o custo real de mão de obra
//! (`cardeal-analytics`).
//!
//! **Não confundir com [`crate::ItemMaoDeObra::horas`]**: aquele é a mão de obra
//! *orçada/cobrada* do cliente (o valor fixo que consta no orçamento aprovado); isto é o
//! relógio de ponto real — "cheguei a bater o ponto nesse conserto" — que pode divergir do
//! orçado (um reparo pode levar mais ou menos tempo do que o cobrado). Domínio puro: só
//! valida invariantes de tempo do próprio apontamento. A regra "um técnico não trabalha em
//! duas ordens ao mesmo tempo" precisa olhar *todos* os apontamentos abertos do técnico, não
//! só este — por isso mora no comando (`IniciarApontamento`), não aqui.

use cardeal_kernel::{Id, Instante, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroOs;

/// Um apontamento de tempo de um técnico numa ordem de serviço — aberto (`fim = None`,
/// "cronômetro rodando") ou encerrado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApontamentoDeTempo {
    /// Identidade.
    pub id: Id,
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O técnico que está (ou esteve) trabalhando.
    pub tecnico: Id,
    /// Quando começou.
    pub inicio: Instante,
    /// Quando terminou — `None` enquanto o cronômetro está rodando.
    pub fim: Option<Instante>,
    /// Verdadeiro se `inicio`/`fim` foram corrigidos manualmente depois do fato (o técnico
    /// esqueceu de parar o cronômetro, por exemplo) — nunca fica implícito no dado bruto.
    pub ajustado: bool,
    /// O motivo do ajuste manual, quando `ajustado`.
    pub motivo_ajuste: Option<String>,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl ApontamentoDeTempo {
    /// Inicia um apontamento — o cronômetro começa a rodar agora.
    #[must_use]
    pub fn iniciar(ordem_servico: Id, tecnico: Id, inicio: Instante) -> Self {
        Self {
            id: Id::novo(),
            ordem_servico,
            tecnico,
            inicio,
            fim: None,
            ajustado: false,
            motivo_ajuste: None,
            versao: Versao::INICIAL,
        }
    }

    /// Verdadeiro enquanto o cronômetro está rodando (`fim` ainda não gravado).
    #[must_use]
    pub const fn esta_aberto(&self) -> bool {
        self.fim.is_none()
    }

    /// Encerra o apontamento agora.
    ///
    /// # Errors
    /// [`ErroOs::ApontamentoJaEncerrado`] se já estava fechado;
    /// [`ErroOs::FimAntesDoInicio`] se `fim` vier antes de `inicio`.
    pub fn encerrar(&mut self, fim: Instante) -> Result<(), ErroOs> {
        if !self.esta_aberto() {
            return Err(ErroOs::ApontamentoJaEncerrado);
        }
        if fim < self.inicio {
            return Err(ErroOs::FimAntesDoInicio);
        }
        self.fim = Some(fim);
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// Corrige manualmente início e fim — para quando o técnico esquece de parar o
    /// cronômetro (ou erra a hora de início). Funciona tanto sobre um apontamento aberto
    /// quanto já encerrado; sempre exige o motivo, e o marca como `ajustado` — nunca uma
    /// edição silenciosa de um fato já registrado (mesma disciplina de
    /// `mod_financeiro::EstornarBaixa`/`RenegociarTitulo`: correção pós-fato sempre com
    /// motivo e sempre rastreável).
    ///
    /// # Errors
    /// [`ErroOs::MotivoDeAjusteObrigatorio`] se o motivo vier vazio;
    /// [`ErroOs::FimAntesDoInicio`] se o novo fim vier antes do novo início.
    pub fn ajustar(
        &mut self,
        novo_inicio: Instante,
        novo_fim: Instante,
        motivo: impl Into<String>,
    ) -> Result<(), ErroOs> {
        let motivo = motivo.into().trim().to_string();
        if motivo.is_empty() {
            return Err(ErroOs::MotivoDeAjusteObrigatorio);
        }
        if novo_fim < novo_inicio {
            return Err(ErroOs::FimAntesDoInicio);
        }
        self.inicio = novo_inicio;
        self.fim = Some(novo_fim);
        self.ajustado = true;
        self.motivo_ajuste = Some(motivo);
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// A duração em segundos: até `fim`, se encerrado, ou até `agora`, se ainda rodando —
    /// para a UI mostrar o tempo acumulado de um apontamento em andamento.
    #[must_use]
    pub fn duracao_segundos(&self, agora: Instante) -> i64 {
        let fim = self.fim.unwrap_or(agora);
        self.inicio.micros_ate(fim).max(0) / 1_000_000
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn t(segundos: i64) -> Instante {
        Instante::EPOCA.mais_segundos(segundos)
    }

    #[test]
    fn iniciar_abre_sem_fim() {
        let ap = ApontamentoDeTempo::iniciar(Id::novo(), Id::novo(), t(0));
        assert!(ap.esta_aberto());
        assert_eq!(ap.duracao_segundos(t(3600)), 3600);
    }

    #[test]
    fn encerrar_fecha_e_fixa_duracao() {
        let mut ap = ApontamentoDeTempo::iniciar(Id::novo(), Id::novo(), t(0));
        ap.encerrar(t(1800)).unwrap();
        assert!(!ap.esta_aberto());
        assert_eq!(ap.duracao_segundos(t(999_999)), 1800);
    }

    #[test]
    fn encerrar_duas_vezes_e_recusado() {
        let mut ap = ApontamentoDeTempo::iniciar(Id::novo(), Id::novo(), t(0));
        ap.encerrar(t(100)).unwrap();
        assert_eq!(
            ap.encerrar(t(200)).unwrap_err(),
            ErroOs::ApontamentoJaEncerrado
        );
    }

    #[test]
    fn encerrar_antes_do_inicio_e_recusado() {
        let mut ap = ApontamentoDeTempo::iniciar(Id::novo(), Id::novo(), t(1000));
        assert_eq!(ap.encerrar(t(500)).unwrap_err(), ErroOs::FimAntesDoInicio);
    }

    #[test]
    fn ajustar_exige_motivo_e_marca_ajustado() {
        let mut ap = ApontamentoDeTempo::iniciar(Id::novo(), Id::novo(), t(0));
        assert_eq!(
            ap.ajustar(t(0), t(100), "   ").unwrap_err(),
            ErroOs::MotivoDeAjusteObrigatorio
        );
        ap.ajustar(t(0), t(7200), "Esqueceu de encerrar o cronômetro")
            .unwrap();
        assert!(ap.ajustado);
        assert_eq!(ap.duracao_segundos(t(999_999)), 7200);
    }

    #[test]
    fn ajustar_com_fim_antes_do_inicio_e_recusado() {
        let mut ap = ApontamentoDeTempo::iniciar(Id::novo(), Id::novo(), t(0));
        assert_eq!(
            ap.ajustar(t(500), t(100), "correção").unwrap_err(),
            ErroOs::FimAntesDoInicio
        );
    }

    #[test]
    fn ajustar_um_apontamento_ja_encerrado_tambem_funciona() {
        let mut ap = ApontamentoDeTempo::iniciar(Id::novo(), Id::novo(), t(0));
        ap.encerrar(t(100)).unwrap();
        ap.ajustar(t(0), t(3600), "esqueceu de parar a tempo")
            .unwrap();
        assert_eq!(ap.duracao_segundos(t(0)), 3600);
    }
}
