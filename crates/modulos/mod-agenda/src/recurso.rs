//! Recurso e disponibilidade — sala, técnico, equipamento (`docs/modulos/agenda.md` §3).
//! Domínio puro.

use cardeal_kernel::Id;
use serde::{Deserialize, Serialize};

use crate::erros::ErroAgenda;

/// O tipo de um [`Recurso`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoRecurso {
    /// Sala física.
    Sala,
    /// Técnico ou pessoa que atende.
    Tecnico,
    /// Equipamento compartilhado.
    Equipamento,
    /// Pessoa — agenda de disponibilidade que não é um técnico de campo.
    Pessoa,
}

/// Algo que um compromisso reserva: sala, técnico, equipamento ou pessoa.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recurso {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// O nome: "Sala 1", "Carlos", "Furadeira industrial".
    pub nome: String,
    /// O tipo.
    pub tipo: TipoRecurso,
    /// Quantas reservas simultâneas o recurso aceita — `None` = uma por vez.
    pub capacidade: Option<u32>,
    /// Recursos inativos somem do seletor, mas os compromissos antigos mantêm o vínculo.
    pub ativo: bool,
}

impl Recurso {
    /// Cria um recurso novo, ativo.
    ///
    /// # Errors
    /// [`ErroAgenda::NomeVazio`] se o nome, sem espaços nas pontas, for vazio.
    pub fn novo(
        empresa: Id,
        nome: impl Into<String>,
        tipo: TipoRecurso,
        capacidade: Option<u32>,
    ) -> Result<Self, ErroAgenda> {
        let nome = nome.into();
        if nome.trim().is_empty() {
            return Err(ErroAgenda::NomeVazio);
        }
        Ok(Self {
            id: Id::novo(),
            empresa,
            nome,
            tipo,
            capacidade,
            ativo: true,
        })
    }
}

/// Uma janela recorrente em que um recurso está disponível — `docs/modulos/agenda.md` §3.
/// Ausência de qualquer regra para um recurso significa "sempre disponível".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisponibilidadeRecurso {
    /// Identidade.
    pub id: Id,
    /// O recurso.
    pub recurso: Id,
    /// Dia da semana: 0 (domingo) a 6 (sábado).
    pub dia_semana: u8,
    /// Minuto de início do dia (0..1440).
    pub hora_inicio: u16,
    /// Minuto de fim do dia (0..1440), sempre depois de `hora_inicio`.
    pub hora_fim: u16,
}

impl DisponibilidadeRecurso {
    /// Cria uma regra de disponibilidade nova.
    ///
    /// # Errors
    /// [`ErroAgenda::IntervaloInvalido`] se `dia_semana` estiver fora de 0..=6, ou o intervalo
    /// de horário for vazio/inválido.
    pub fn nova(
        recurso: Id,
        dia_semana: u8,
        hora_inicio: u16,
        hora_fim: u16,
    ) -> Result<Self, ErroAgenda> {
        if dia_semana > 6 || hora_inicio >= hora_fim || hora_fim > 1440 {
            return Err(ErroAgenda::IntervaloInvalido);
        }
        Ok(Self {
            id: Id::novo(),
            recurso,
            dia_semana,
            hora_inicio,
            hora_fim,
        })
    }

    /// Verdadeiro se `[inicio, fim)` (minutos do dia) cabe inteiramente dentro desta janela,
    /// no mesmo dia da semana.
    #[must_use]
    pub const fn cobre(&self, dia_semana: u8, inicio: u16, fim: u16) -> bool {
        self.dia_semana == dia_semana && inicio >= self.hora_inicio && fim <= self.hora_fim
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn nome_vazio_e_recusado() {
        assert_eq!(
            Recurso::novo(Id::novo(), "  ", TipoRecurso::Sala, None).unwrap_err(),
            ErroAgenda::NomeVazio
        );
    }

    #[test]
    fn disponibilidade_recusa_intervalo_invertido_ou_fora_do_dia() {
        let r = Id::novo();
        assert_eq!(
            DisponibilidadeRecurso::nova(r, 7, 480, 1020).unwrap_err(),
            ErroAgenda::IntervaloInvalido
        );
        assert_eq!(
            DisponibilidadeRecurso::nova(r, 1, 1020, 480).unwrap_err(),
            ErroAgenda::IntervaloInvalido
        );
    }

    #[test]
    fn cobre_so_no_mesmo_dia_e_dentro_da_janela() {
        let d = DisponibilidadeRecurso::nova(Id::novo(), 2, 480, 1080).unwrap(); // 08:00–18:00
        assert!(d.cobre(2, 540, 600)); // 09:00–10:00, terça
        assert!(!d.cobre(3, 540, 600)); // dia errado
        assert!(!d.cobre(2, 420, 600)); // começa antes das 08:00
        assert!(!d.cobre(2, 540, 1100)); // termina depois das 18:00
    }
}
