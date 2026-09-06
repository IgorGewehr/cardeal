//! Compromisso — a reserva de um horário, com ou sem cliente (`docs/modulos/agenda.md` §3/§4).
//! Domínio puro.

use cardeal_kernel::{Id, Instante, Versao};
use serde::{Deserialize, Serialize};

use crate::erros::ErroAgenda;

/// Se o compromisso é interno ou envolve um cliente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TipoCompromisso {
    /// Sem cliente — reunião interna, manutenção.
    Interno,
    /// Com um cliente vinculado.
    Cliente,
}

/// O estado de um [`Compromisso`] (`docs/modulos/agenda.md` §4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoCompromisso {
    /// Criado, aguardando confirmação.
    Agendado,
    /// Confirmado — o cliente/recurso já sabe.
    Confirmado,
    /// Em andamento.
    EmAndamento,
    /// Concluído. Terminal.
    Concluido,
    /// Cancelado — nunca apagado (§11.3). Terminal.
    Cancelado,
    /// O horário passou sem que o compromisso fosse iniciado. Terminal.
    NaoCompareceu,
}

impl EstadoCompromisso {
    /// O rótulo em português.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Agendado => "Agendado",
            Self::Confirmado => "Confirmado",
            Self::EmAndamento => "EmAndamento",
            Self::Concluido => "Concluido",
            Self::Cancelado => "Cancelado",
            Self::NaoCompareceu => "NaoCompareceu",
        }
    }

    /// Verdadeiro para os três estados terminais (`docs/modulos/agenda.md` §4).
    #[must_use]
    pub const fn e_terminal(self) -> bool {
        matches!(
            self,
            Self::Concluido | Self::Cancelado | Self::NaoCompareceu
        )
    }
}

/// Uma reserva de horário — de um ou mais recursos, com ou sem cliente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Compromisso {
    /// Identidade.
    pub id: Id,
    /// A empresa.
    pub empresa: Id,
    /// Título legível: "Visita técnica", "Reunião de alinhamento".
    pub titulo: String,
    /// Interno ou com cliente.
    pub tipo: TipoCompromisso,
    /// O cliente vinculado, quando `tipo` é [`TipoCompromisso::Cliente`].
    pub cliente: Option<Id>,
    /// Quando começa.
    pub inicio: Instante,
    /// Quando termina — sempre depois de `inicio`.
    pub fim: Instante,
    /// O estado atual.
    pub estado: EstadoCompromisso,
    /// O módulo que criou o compromisso (`"os"`, `"hotelaria"`), `None` se avulso.
    pub origem_modulo: Option<String>,
    /// O agregado de origem, quando houver.
    pub origem_id: Option<Id>,
    /// Quem criou.
    pub criado_por: Id,
    /// Versão para bloqueio otimista.
    pub versao: Versao,
}

impl Compromisso {
    /// Abre um compromisso novo em [`EstadoCompromisso::Agendado`].
    ///
    /// # Errors
    /// [`ErroAgenda::TituloVazio`], [`ErroAgenda::PeriodoInvalido`].
    #[allow(clippy::too_many_arguments)]
    pub fn abrir(
        empresa: Id,
        titulo: impl Into<String>,
        tipo: TipoCompromisso,
        cliente: Option<Id>,
        inicio: Instante,
        fim: Instante,
        origem_modulo: Option<String>,
        origem_id: Option<Id>,
        criado_por: Id,
    ) -> Result<Self, ErroAgenda> {
        let titulo = titulo.into();
        if titulo.trim().is_empty() {
            return Err(ErroAgenda::TituloVazio);
        }
        if fim.em_micros() <= inicio.em_micros() {
            return Err(ErroAgenda::PeriodoInvalido);
        }
        Ok(Self {
            id: Id::novo(),
            empresa,
            titulo,
            tipo,
            cliente,
            inicio,
            fim,
            estado: EstadoCompromisso::Agendado,
            origem_modulo,
            origem_id,
            criado_por,
            versao: Versao::INICIAL,
        })
    }

    /// Verdadeiro se `[inicio, fim)` deste compromisso sobrepõe o intervalo dado.
    #[must_use]
    pub fn sobrepoe(&self, inicio: Instante, fim: Instante) -> bool {
        self.inicio.em_micros() < fim.em_micros() && self.fim.em_micros() > inicio.em_micros()
    }

    /// `Agendado → Confirmado`.
    ///
    /// # Errors
    /// [`ErroAgenda::CompromissoCancelado`], [`ErroAgenda::EstadoInvalido`].
    pub fn confirmar(&mut self) -> Result<(), ErroAgenda> {
        match self.estado {
            EstadoCompromisso::Agendado => {
                self.estado = EstadoCompromisso::Confirmado;
                self.versao = self.versao.proxima();
                Ok(())
            }
            EstadoCompromisso::Cancelado => Err(ErroAgenda::CompromissoCancelado),
            _ => Err(ErroAgenda::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Agendado",
            }),
        }
    }

    /// `Confirmado → EmAndamento`.
    ///
    /// # Errors
    /// [`ErroAgenda::EstadoInvalido`].
    pub fn iniciar(&mut self) -> Result<(), ErroAgenda> {
        self.exigir(EstadoCompromisso::Confirmado)?;
        self.estado = EstadoCompromisso::EmAndamento;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// `EmAndamento → Concluido`.
    ///
    /// # Errors
    /// [`ErroAgenda::EstadoInvalido`].
    pub fn concluir(&mut self) -> Result<(), ErroAgenda> {
        self.exigir(EstadoCompromisso::EmAndamento)?;
        self.estado = EstadoCompromisso::Concluido;
        self.versao = self.versao.proxima();
        Ok(())
    }

    /// `{Agendado,Confirmado,EmAndamento} → Cancelado` — nunca apaga (§11.3).
    ///
    /// # Errors
    /// [`ErroAgenda::CompromissoConcluido`], [`ErroAgenda::EstadoInvalido`].
    pub fn cancelar(&mut self) -> Result<(), ErroAgenda> {
        match self.estado {
            EstadoCompromisso::Agendado
            | EstadoCompromisso::Confirmado
            | EstadoCompromisso::EmAndamento => {
                self.estado = EstadoCompromisso::Cancelado;
                self.versao = self.versao.proxima();
                Ok(())
            }
            EstadoCompromisso::Concluido => Err(ErroAgenda::CompromissoConcluido),
            _ => Err(ErroAgenda::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: "Agendado, Confirmado ou EmAndamento",
            }),
        }
    }

    /// `Confirmado → NaoCompareceu` — o gatilho de `registrar_nao_comparecimento_pendentes`
    /// quando o horário passa sem que o compromisso tenha sido iniciado.
    ///
    /// # Errors
    /// [`ErroAgenda::EstadoInvalido`].
    pub fn marcar_nao_compareceu(&mut self) -> Result<(), ErroAgenda> {
        self.exigir(EstadoCompromisso::Confirmado)?;
        self.estado = EstadoCompromisso::NaoCompareceu;
        self.versao = self.versao.proxima();
        Ok(())
    }

    fn exigir(&self, esperado: EstadoCompromisso) -> Result<(), ErroAgenda> {
        if self.estado == esperado {
            Ok(())
        } else {
            Err(ErroAgenda::EstadoInvalido {
                atual: self.estado.rotulo(),
                esperado: esperado.rotulo(),
            })
        }
    }
}

#[cfg(test)]
mod testes {
    use cardeal_kernel::{Data, Fuso, Hora};

    use super::*;

    fn instante(h: u32) -> Instante {
        Instante::de_data_hora(
            Data::de_ymd(2026, 3, 10).unwrap(),
            Hora::de_hms(h, 0, 0).unwrap(),
            Fuso::BRASILIA,
        )
    }

    fn compromisso() -> Compromisso {
        Compromisso::abrir(
            Id::novo(),
            "Visita técnica",
            TipoCompromisso::Cliente,
            Some(Id::novo()),
            instante(9),
            instante(10),
            None,
            None,
            Id::novo(),
        )
        .unwrap()
    }

    #[test]
    fn titulo_vazio_e_periodo_invertido_sao_recusados() {
        assert_eq!(
            Compromisso::abrir(
                Id::novo(),
                "  ",
                TipoCompromisso::Interno,
                None,
                instante(9),
                instante(10),
                None,
                None,
                Id::novo(),
            )
            .unwrap_err(),
            ErroAgenda::TituloVazio
        );
        assert_eq!(
            Compromisso::abrir(
                Id::novo(),
                "Reunião",
                TipoCompromisso::Interno,
                None,
                instante(10),
                instante(9),
                None,
                None,
                Id::novo(),
            )
            .unwrap_err(),
            ErroAgenda::PeriodoInvalido
        );
    }

    #[test]
    fn sobreposicao_e_detectada_nas_bordas() {
        let c = compromisso(); // 09:00–10:00
        assert!(c.sobrepoe(instante(9), instante(11))); // mesmo início
        assert!(c.sobrepoe(instante(8), instante(10))); // sobrepõe a primeira metade
        assert!(!c.sobrepoe(instante(10), instante(11))); // começa exatamente quando o outro termina
        assert!(!c.sobrepoe(instante(7), instante(9))); // termina exatamente quando o outro começa
    }

    #[test]
    fn ciclo_feliz_agendado_confirmado_em_andamento_concluido() {
        let mut c = compromisso();
        c.confirmar().unwrap();
        assert_eq!(c.estado, EstadoCompromisso::Confirmado);
        c.iniciar().unwrap();
        assert_eq!(c.estado, EstadoCompromisso::EmAndamento);
        c.concluir().unwrap();
        assert_eq!(c.estado, EstadoCompromisso::Concluido);
        assert!(c.estado.e_terminal());
    }

    #[test]
    fn cancelar_e_permitido_ate_em_andamento_mas_nao_depois_de_concluido() {
        let mut c = compromisso();
        c.cancelar().unwrap();
        assert_eq!(c.estado, EstadoCompromisso::Cancelado);

        let mut c = compromisso();
        c.confirmar().unwrap();
        c.iniciar().unwrap();
        c.concluir().unwrap();
        assert_eq!(c.cancelar().unwrap_err(), ErroAgenda::CompromissoConcluido);
    }

    #[test]
    fn confirmar_um_cancelado_da_erro_especifico() {
        let mut c = compromisso();
        c.cancelar().unwrap();
        assert_eq!(c.confirmar().unwrap_err(), ErroAgenda::CompromissoCancelado);
    }

    #[test]
    fn nao_comparecimento_so_a_partir_de_confirmado() {
        let mut c = compromisso();
        assert!(matches!(
            c.marcar_nao_compareceu().unwrap_err(),
            ErroAgenda::EstadoInvalido { .. }
        ));
        c.confirmar().unwrap();
        c.marcar_nao_compareceu().unwrap();
        assert_eq!(c.estado, EstadoCompromisso::NaoCompareceu);
    }
}
