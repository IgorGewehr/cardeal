//! Aplica uma peça do orçamento: consome o estoque de verdade e grava o custo unitário
//! devolvido.
//!
//! `docs/modulos/os.md` §5 e §11.1: peça só é aplicada com orçamento aprovado (`EmExecucao`
//! só se chega depois de `Aprovada`). Chama `mod_estoque::registrar_saida_comum` **direto,
//! na mesma transação** — mesma decisão de arquitetura documentada em
//! `mod_financeiro::lancar_titulo_comum` e `docs/contratos-internos.md` §7 regra 2.

use cardeal_kernel::{Erro, Id, Preco, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::{registrar_saida_comum, DadosSaida};
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::erros::ErroOs;
use crate::repositorio::RepositorioOs;

/// Aplica uma peça já orçada — consome o estoque no local informado.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AplicarPeca {
    /// A ordem de serviço (para conferir o estado e vincular a origem do consumo).
    pub ordem_servico: Id,
    /// O item de peça do orçamento.
    pub item_peca: Id,
    /// O local de estoque de onde a peça sai.
    pub local: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PecaFoiAplicada {
    /// O item de peça atualizado.
    pub item_peca: Id,
    /// O custo unitário real, devolvido pelo estoque.
    pub custo_unitario: Preco,
}

impl Comando for AplicarPeca {
    type Saida = PecaFoiAplicada;
    const PERMISSAO: &'static str = "os.peca.aplicar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let os = carregar_ordem(uow, self.ordem_servico)?;
        let mut item = RepositorioOs::novo(uow)
            .buscar_item_peca(self.item_peca)?
            .ok_or_else(|| Erro::nao_encontrado("item de peça"))?;

        // 2. Validar.
        os.exigir_em_execucao().map_err(|e| Erro::de_dominio(&e))?;
        if item.ordem_servico != os.id {
            return Err(Erro::de_dominio(&ErroOs::ItemNaoPertenceAOrdem));
        }

        // 3. Consumir o estoque (chamada direta a outro módulo, mesma transação).
        let saida = registrar_saida_comum(
            DadosSaida {
                produto: item.produto,
                local: self.local,
                quantidade: item.quantidade,
                origem_modulo: "os",
                origem_id: Some(os.id),
            },
            ctx,
            uow,
        )?;

        item.aplicar(saida.aplicada.custo_unitario)
            .map_err(|e| Erro::de_dominio(&e))?;

        // 4. Persistir.
        RepositorioOs::novo(uow).atualizar_item_peca(&item)?;

        Ok(PecaFoiAplicada {
            item_peca: item.id,
            custo_unitario: item.custo_unitario,
        })
    }
}
