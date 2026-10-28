//! Estorna a entrada de uma nota de compra — o oposto de [`super::ConfirmarEntrada`], para
//! corrigir uma entrada dada errado (produto vinculado errado, quantidade, parcelas, nota que
//! não devia ter entrado) sem mexer em estoque e financeiro à mão. Lição do `gestao-raiz`
//! (`revertNoteImport`): lá a entrada se reverte e a nota volta para ser reimportada; e se os
//! lotes já foram liberados ou consumidos, a reversão é recusada.
//!
//! Aqui, numa transação só:
//! - a mercadoria sai do estoque **ao custo da nota** (`mod_estoque::retirar_entradas_da_origem`)
//!   — recusado se ela já não está toda no local (vendida, movida);
//! - a conta a pagar é desfeita (`mod_financeiro::desfazer_titulo_da_origem_comum`): o
//!   lançamento D Estoque / C Fornecedores é estornado e o título cancelado. Se já tem
//!   pagamento, só com `estornar_pagamento` — estornar um pagamento de verdade é decisão de
//!   quem estorna, não efeito colateral (a mesma regra do `CancelarTitulo`);
//! - a nota volta para a conferência (`AConferir`), com os itens casados como estavam.
//!
//! Nunca apaga: movimentos, título e lançamentos ficam, estornados.

use cardeal_kernel::{Dinheiro, Id};
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::carregar_nota;
#[cfg(feature = "sqlite")]
use crate::erros::ErroCompras;
#[cfg(feature = "sqlite")]
use crate::eventos::EntradaEstornada;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioCompras;

/// O mínimo de caracteres do motivo — vai ao histórico do estorno no Razão.
pub const MOTIVO_MINIMO_ESTORNO: usize = 10;

/// Estorna a entrada de uma nota confirmada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EstornarEntrada {
    /// A nota de entrada.
    pub nota_entrada: Id,
    /// Por quê (ao menos [`MOTIVO_MINIMO_ESTORNO`] caracteres).
    pub motivo: String,
    /// Estornar também o pagamento da conta da nota, se houver (o dinheiro volta para a
    /// conta de onde saiu). Sem isso, nota com pagamento é recusada.
    #[serde(default)]
    pub estornar_pagamento: bool,
}

/// O que o estorno devolve.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntradaFoiEstornada {
    /// Quantas linhas de estoque saíram (produto × local).
    pub itens: usize,
    /// O título a pagar cancelado, se havia.
    pub titulo: Option<Id>,
    /// O pagamento estornado junto (zero se não havia).
    pub pagamento_estornado: Dinheiro,
}

impl Comando for EstornarEntrada {
    type Saida = EntradaFoiEstornada;
    const PERMISSAO: &'static str = "compras.entrada.estornar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut nota = carregar_nota(uow, self.nota_entrada)?;

        // 2. Validar.
        let motivo = self.motivo.trim().to_owned();
        if motivo.chars().count() < MOTIVO_MINIMO_ESTORNO {
            return Err(Erro::de_dominio(&ErroCompras::MotivoDoEstornoCurto));
        }
        nota.estornar().map_err(|e| Erro::de_dominio(&e))?;
        if crate::devolucao::sqlite::tem_devolucao(uow.conexao(), nota.id)? {
            return Err(Erro::de_dominio(&ErroCompras::NotaComDevolucao));
        }
        for origem in ["compras", "compras_st"] {
            if let Some((_, pago)) = mod_financeiro::situacao_do_titulo_da_origem(
                uow.conexao(),
                ctx.empresa,
                origem,
                nota.id,
            )? {
                if pago.e_positivo() && !self.estornar_pagamento {
                    return Err(Erro::de_dominio(&ErroCompras::EntradaComPagamento(pago)));
                }
            }
        }

        // 3. Estoque: a mercadoria sai ao custo da nota (recusa se já foi vendida/movida).
        let saidas = mod_estoque::retirar_entradas_da_origem("compras", nota.id, ctx, uow)?;

        // 4. Financeiro: pagamento (se pedido), lançamento e título.
        let historico = format!(
            "Estorno da entrada da nota {}/{} — {motivo}",
            nota.numero, nota.serie
        );
        let desfeito = mod_financeiro::desfazer_titulo_da_origem_comum(
            "compras", nota.id, &historico, ctx, uow,
        )?;
        // A guia do ST estimado, e o ST que tinha entrado no custo dos itens.
        mod_financeiro::desfazer_titulo_da_origem_comum(
            "compras_st",
            nota.id,
            &historico,
            ctx,
            uow,
        )?;
        uow.conexao()
            .execute(
                "UPDATE compras_item_nota_entrada
                 SET valor_impostos = valor_impostos - st_estimado, st_estimado = 0
                 WHERE nota_entrada = ?1",
                [nota.id.em_bytes().as_slice()],
            )
            .map_err(|e| Erro::novo(cardeal_kernel::CodigoErro::FALHA_INTERNA, e.to_string()))?;

        // 5. Persistir.
        RepositorioCompras::novo(uow).atualizar_nota(&nota)?;

        // 6. Publicar.
        let titulo = desfeito.map(|d| d.titulo);
        let pagamento_estornado = desfeito.map_or(Dinheiro::ZERO, |d| d.valor_estornado);
        uow.publicar(EntradaEstornada {
            nota_entrada: nota.id,
            fornecedor: nota.fornecedor,
            valor_total: nota.valor_total,
            titulo,
            pagamento_estornado,
            motivo,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(EntradaFoiEstornada {
            itens: saidas.len(),
            titulo,
            pagamento_estornado,
        })
    }
}
