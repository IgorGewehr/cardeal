//! Cria um produto a partir de um item de nota que não casou com nada do catálogo e já o
//! vincula ao item (`docs/modulos/compras.md`, lunno §5.6): o nome é o que o usuário escolheu
//! (a sugestão padronizada de `estoque.sugerir_nome_do_produto.v1`, possivelmente corrigida),
//! o NCM e o código de barras vêm da nota. Aprende duas coisas: a regra de casamento
//! (fornecedor + código → produto, como o vínculo manual) e os termos que a correção do nome
//! explica no dicionário DE/PARA.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::casamento::RegraCasamentoAprendida;
#[cfg(feature = "sqlite")]
use crate::comandos::carregar_nota;
#[cfg(feature = "sqlite")]
use crate::erros::ErroCompras;
#[cfg(feature = "sqlite")]
use crate::nota::EstadoNotaEntrada;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioCompras;

/// Cria o produto do item e vincula.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarProdutoDoItem {
    /// O item da nota.
    pub item_nota: Id,
    /// O nome do produto novo.
    pub nome: String,
    /// O grupo da estrutura; `None` = "Sem grupo".
    pub grupo: Option<Id>,
}

/// O que foi criado.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProdutoDoItemCriado {
    /// O produto.
    pub produto: Id,
    /// Quantos termos o dicionário de nomes aprendeu.
    pub termos_aprendidos: u32,
}

impl Comando for CriarProdutoDoItem {
    type Saida = ProdutoDoItemCriado;
    const PERMISSAO: &'static str = "compras.entrada.conferir";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        if !ctx.concede("estoque.produto.criar") {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "criar produto a partir da nota exige a permissão de cadastrar produto",
            ));
        }
        let mut item = RepositorioCompras::novo(uow)
            .buscar_item(self.item_nota)?
            .ok_or_else(|| Erro::nao_encontrado("item de nota"))?;
        let nota = carregar_nota(uow, item.nota_entrada)?;
        if matches!(
            nota.estado,
            EstadoNotaEntrada::Confirmada | EstadoNotaEntrada::Devolvida
        ) {
            return Err(Erro::de_dominio(&ErroCompras::NotaJaConfirmada));
        }

        let criado = mod_estoque::criar_produto_de_compra_comum(
            &item.descricao_fornecedor,
            &self.nome,
            &item.ncm,
            item.codigo_barras.as_deref(),
            self.grupo,
            ctx,
            uow,
        )?;
        item.vincular(criado.produto);
        let regra = RegraCasamentoAprendida {
            empresa: ctx.empresa,
            fornecedor: nota.fornecedor,
            codigo_fornecedor: item.codigo_fornecedor.clone(),
            produto: criado.produto,
            aprendido_em: ctx.agora,
        };
        let mut repo = RepositorioCompras::novo(uow);
        repo.atualizar_item(&item)?;
        repo.upsert_regra_casamento(&regra)?;
        Ok(ProdutoDoItemCriado {
            produto: criado.produto,
            termos_aprendidos: criado.termos_aprendidos,
        })
    }
}
