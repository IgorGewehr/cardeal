//! Registra um aparelho usado adquirido para desmontar e reaproveitar peças (trade-in).
//!
//! `docs/modulos/estoque.md` §3 — o outro lado do caso de uso do post-it: quando uma peça não
//! veio de compra nova, veio de um aparelho usado, e este comando é o que registra esse
//! aparelho (com o custo de aquisição dele) para depois vincular a ele o(s)
//! [`crate::produto::Lote`] retirado(s), via [`super::RegistrarEntradaComLote`].

use cardeal_kernel::{Data, Dinheiro, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::aparelho_origem::AparelhoOrigem;
use crate::repositorio::RepositorioEstoque;

/// Registra um aparelho usado de origem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistrarAparelhoOrigem {
    /// Descrição livre ("iPhone 11 Pro — tela trincada").
    pub descricao: String,
    /// Identificador livre (IMEI/serial), quando anotado.
    pub identificador: Option<String>,
    /// Quanto custou adquirir o aparelho inteiro.
    pub custo_aquisicao: Dinheiro,
    /// Quando foi adquirido.
    pub adquirido_em: Data,
    /// Fornecedor/origem da aquisição, quando houver.
    pub fornecedor: Option<Id>,
    /// Observações livres.
    pub observacoes: Option<String>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct AparelhoOrigemRegistrado {
    /// O aparelho de origem criado.
    pub aparelho_origem: Id,
}

impl Comando for RegistrarAparelhoOrigem {
    type Saida = AparelhoOrigemRegistrado;
    const PERMISSAO: &'static str = "estoque.aparelho_origem.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar (domínio puro).
        let aparelho = AparelhoOrigem::novo(
            ctx.empresa,
            self.descricao,
            self.identificador,
            self.custo_aquisicao,
            self.adquirido_em,
            self.fornecedor,
            self.observacoes,
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        // 2. Persistir.
        RepositorioEstoque::novo(uow).inserir_aparelho_origem(&aparelho)?;

        Ok(AparelhoOrigemRegistrado {
            aparelho_origem: aparelho.id,
        })
    }
}
