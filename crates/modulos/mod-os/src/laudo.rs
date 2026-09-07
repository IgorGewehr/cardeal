//! O laudo técnico de uma ordem de serviço.
//!
//! `docs/modulos/os.md` §3. Domínio puro — só valida que a descrição do problema não vem
//! vazia; o diagnóstico em si é texto livre do técnico, opcional no momento do registro.

use cardeal_kernel::{Id, Instante};
use serde::{Deserialize, Serialize};

use crate::erros::ErroOs;

/// O laudo técnico de uma ordem de serviço.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaudoTecnico {
    /// Identidade.
    pub id: Id,
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// O problema relatado (pelo cliente ou observado na recepção).
    pub descricao_problema: String,
    /// O diagnóstico técnico, quando já concluído.
    pub diagnostico: Option<String>,
    /// O técnico responsável pelo laudo.
    pub tecnico: Id,
    /// Quando foi registrado.
    pub criado_em: Instante,
}

impl LaudoTecnico {
    /// Cria um laudo técnico.
    ///
    /// # Errors
    /// [`ErroOs::DescricaoDoProblemaVazia`].
    pub fn novo(
        ordem_servico: Id,
        descricao_problema: impl Into<String>,
        diagnostico: Option<String>,
        tecnico: Id,
        criado_em: Instante,
    ) -> Result<Self, ErroOs> {
        let descricao_problema = descricao_problema.into().trim().to_string();
        if descricao_problema.is_empty() {
            return Err(ErroOs::DescricaoDoProblemaVazia);
        }
        Ok(Self {
            id: Id::novo(),
            ordem_servico,
            descricao_problema,
            diagnostico,
            tecnico,
            criado_em,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn descricao_vazia_e_recusada() {
        let erro =
            LaudoTecnico::novo(Id::novo(), "   ", None, Id::novo(), Instante::EPOCA).unwrap_err();
        assert_eq!(erro, ErroOs::DescricaoDoProblemaVazia);
    }

    #[test]
    fn laudo_valido_e_criado() {
        let l = LaudoTecnico::novo(
            Id::novo(),
            "Não liga",
            Some("Fonte queimada".to_string()),
            Id::novo(),
            Instante::EPOCA,
        )
        .unwrap();
        assert_eq!(l.descricao_problema, "Não liga");
    }
}
