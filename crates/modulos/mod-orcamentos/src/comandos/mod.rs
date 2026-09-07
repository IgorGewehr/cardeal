//! Os comandos de orçamentos — um arquivo, um comando, nas cinco etapas de
//! `docs/15-convencoes-codigo.md` §3.

mod cancelar_orcamento;
mod converter_orcamento_em_os;
mod criar_orcamento;
mod definir_itens_orcamento;
mod duplicar_orcamento;
mod editar_orcamento;
mod enviar_orcamento;
mod registrar_decisao_orcamento;

pub use cancelar_orcamento::CancelarOrcamento;
pub use converter_orcamento_em_os::{ConverterOrcamentoEmOs, OrcamentoConvertido};
pub use criar_orcamento::{CriarOrcamento, NovoItemOrcamento, OrcamentoCriado};
pub use definir_itens_orcamento::DefinirItensOrcamento;
pub use duplicar_orcamento::DuplicarOrcamento;
pub use editar_orcamento::EditarOrcamento;
pub use enviar_orcamento::EnviarOrcamento;
pub use registrar_decisao_orcamento::RegistrarDecisaoOrcamento;

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_storage::UnidadeDeTrabalho;

use crate::orcamento::Orcamento;
use crate::repositorio::RepositorioOrcamentos;

/// Carrega um orçamento ou devolve `NAO_ENCONTRADO`.
pub(crate) fn carregar_orcamento(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<Orcamento> {
    RepositorioOrcamentos::novo(uow)
        .buscar_orcamento(id)?
        .ok_or_else(|| Erro::nao_encontrado("orçamento"))
}

/// Converte um item da carga do comando num [`crate::item::ItemOrcamento`] validado.
pub(crate) fn montar_itens(
    orcamento: Id,
    itens: Vec<NovoItemOrcamento>,
) -> Result<Vec<crate::item::ItemOrcamento>, crate::erros::ErroOrcamentos> {
    itens
        .into_iter()
        .enumerate()
        .map(|(i, it)| {
            crate::item::ItemOrcamento::novo(
                orcamento,
                u32::try_from(i).unwrap_or(u32::MAX),
                it.descricao,
                it.quantidade,
                it.unidade,
                it.preco_unitario,
                it.desconto_percentual,
            )
        })
        .collect()
}
