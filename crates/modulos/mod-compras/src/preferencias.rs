//! Preferências de importação de nota — a decisão de arquitetura desta sessão para a parte
//! que o `docs/modulos/compras.md` original não detalhava: "vários ajustes de preferências"
//! (inspirado no funcionamento do `gestao-raiz`, mas sem copiar código — ele não tem esse
//! conceito de preferência explícito, só um `createLots: boolean` por chamada). Uma linha por
//! empresa; ausência de linha = os padrões conservadores abaixo.

use cardeal_kernel::Id;
use serde::{Deserialize, Serialize};

/// Base do rateio de despesas acessórias (frete/seguro/outras) entre os itens da nota.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RateioPor {
    /// Proporcional ao valor de cada item (`docs/modulos/compras.md` §11.3, padrão).
    Valor,
    /// Proporcional à quantidade/peso de cada item.
    Peso,
}

/// As preferências de importação de nota de uma empresa.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreferenciasCompras {
    /// A empresa.
    pub empresa: Id,
    /// Se, quando **todos** os itens de uma nota casarem por regra aprendida (nunca por
    /// sugestão — essa sempre exige confirmação humana), a entrada é confirmada sozinha
    /// (estoque atualizado, e título a pagar se a preferência abaixo mandar). Padrão: `false`
    /// — o conservador é sempre deixar `AConferir` para um humano revisar.
    pub confirma_automaticamente_quando_tudo_casa: bool,
    /// Se a confirmação de entrada (manual ou automática) também gera o título a pagar no
    /// financeiro. Padrão: `true` — é o caso comum; desligar serve para quem prefere lançar
    /// o título separadamente (ex.: conferência do financeiro em outro momento).
    pub gera_titulo_a_pagar: bool,
    /// Como ratear frete/seguro/outras despesas entre os itens.
    pub rateio_por: RateioPor,
    /// O local de estoque que recebe a entrada quando a confirmação é automática (não há
    /// humano escolhendo na hora). Obrigatório se
    /// `confirma_automaticamente_quando_tudo_casa` for `true` — `DefinirPreferenciasCompras`
    /// recusa a combinação sem local.
    pub local_padrao: Option<Id>,
}

impl PreferenciasCompras {
    /// Os padrões conservadores, para quando a empresa ainda não configurou nada.
    #[must_use]
    pub const fn padrao(empresa: Id) -> Self {
        Self {
            empresa,
            confirma_automaticamente_quando_tudo_casa: false,
            gera_titulo_a_pagar: true,
            rateio_por: RateioPor::Valor,
            local_padrao: None,
        }
    }
}
