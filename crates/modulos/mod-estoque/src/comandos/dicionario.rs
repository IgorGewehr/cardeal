//! O dicionário DE/PARA de nomes de produto (ver [`crate::padronizacao`]): editar à mão e o
//! corpo comum que cria um produto vindo de uma compra — com o nome escolhido pelo usuário e o
//! que a correção dele ensina ao dicionário.

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
use crate::padronizacao::{aprender, chave};
#[cfg(feature = "sqlite")]
use crate::produto::Produto;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_dicionario as sql;

/// Grava (ou troca) um termo do dicionário. `para` vazio = a palavra some do nome sugerido.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirTermoDoDicionario {
    /// A palavra como vem na nota (`REFRIG`).
    pub de: String,
    /// Como ela deve ficar no nome (`Refrigerante`).
    pub para: String,
}

impl Comando for DefinirTermoDoDicionario {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let de = chave(&self.de);
        if de.is_empty() {
            return Err(Erro::novo(
                CodigoErro::ENTRADA_INVALIDA,
                "informe a palavra como ela vem na nota",
            ));
        }
        sql::gravar_termo(
            uow.conexao(),
            ctx.organizacao,
            &de,
            self.para.trim(),
            ctx.agora,
        )
    }
}

/// Tira um termo do dicionário.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoverTermoDoDicionario {
    /// A palavra (como está no dicionário).
    pub de: String,
}

impl Comando for RemoverTermoDoDicionario {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        sql::remover_termo(uow.conexao(), ctx.organizacao, &chave(&self.de))
    }
}

/// O que o corpo comum devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProdutoDeCompraCriado {
    /// O produto criado.
    pub produto: Id,
    /// Quantos termos o dicionário aprendeu com o nome escolhido.
    pub termos_aprendidos: u32,
}

/// Cria um produto a partir do item de uma compra: o `nome` escolhido (a sugestão,
/// possivelmente corrigida), o NCM e o código de barras da nota, no `grupo` (sem grupo: "Sem
/// grupo") e na unidade `UN` — e ensina ao dicionário o que a correção do nome explica. A
/// permissão fica por conta de quem chama (`compras.CriarProdutoDoItem` exige criar produto).
///
/// # Errors
/// Nome vazio, NCM/GTIN inválido, grupo inexistente, ou erro de persistência.
#[cfg(feature = "sqlite")]
pub fn criar_produto_de_compra_comum(
    descricao_origem: &str,
    nome: &str,
    ncm: &str,
    codigo_barras: Option<&str>,
    grupo: Option<Id>,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ProdutoDeCompraCriado> {
    let grupo = match grupo {
        Some(g) => {
            if !RepositorioEstoque::novo(uow).grupo_existe(ctx.organizacao, g)? {
                return Err(Erro::nao_encontrado("grupo de produto"));
            }
            g
        }
        None => sql::grupo_sem_grupo(uow, ctx.organizacao)?,
    };
    let unidade = sql::unidade_un(uow, ctx.organizacao)?;
    let mut produto = Produto::novo(ctx.organizacao, grupo, nome, ncm, unidade)
        .map_err(|e| Erro::de_dominio(&e))?;
    // Um GTIN da nota que já pertence a outro produto seria o casamento automático; se chegou
    // aqui sem casar, ou é inválido ou é de um produto inativo — o produto novo fica sem ele.
    if let Some(gtin) = codigo_barras.filter(|g| !g.is_empty()) {
        if RepositorioEstoque::novo(uow)
            .buscar_produto_por_codigo_barras(ctx.organizacao, gtin)?
            .is_none()
        {
            if let Ok(com) = produto.clone().com_codigo_barras(gtin) {
                produto = com;
            }
        }
    }
    RepositorioEstoque::novo(uow).inserir_produto(&produto)?;

    let dicionario = sql::dicionario(uow.conexao(), ctx.organizacao)?;
    let aprendidos = aprender(descricao_origem, &produto.nome, &dicionario);
    for t in &aprendidos {
        sql::gravar_termo(uow.conexao(), ctx.organizacao, &t.de, &t.para, ctx.agora)?;
    }
    Ok(ProdutoDeCompraCriado {
        produto: produto.id,
        termos_aprendidos: u32::try_from(aprendidos.len()).unwrap_or(u32::MAX),
    })
}
