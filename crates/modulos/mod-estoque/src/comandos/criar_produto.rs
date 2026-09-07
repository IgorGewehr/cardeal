//! Cadastra um produto (fatia mínima: sem grade/lote/validade ainda — ver `src/lib.rs`).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::produto::Produto;
use crate::repositorio::RepositorioEstoque;

/// Cadastra um produto.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarProduto {
    /// O grupo do produto.
    pub grupo_produto: Id,
    /// O nome.
    pub nome: String,
    /// NCM (8 dígitos, com ou sem máscara).
    pub ncm: String,
    /// A unidade padrão.
    pub unidade_padrao: Id,
    /// Código de barras (GTIN), quando o produto tem um impresso.
    pub codigo_barras: Option<String>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ProdutoCriado {
    /// O produto criado.
    pub produto: Id,
}

impl Comando for CriarProduto {
    type Saida = ProdutoCriado;
    const PERMISSAO: &'static str = "estoque.produto.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1/2. Validar (domínio puro).
        let mut produto = Produto::novo(
            ctx.empresa,
            self.grupo_produto,
            self.nome,
            &self.ncm,
            self.unidade_padrao,
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        if let Some(gtin) = self.codigo_barras.as_deref() {
            produto = produto
                .com_codigo_barras(gtin)
                .map_err(|e| Erro::de_dominio(&e))?;
        }

        // 4. Persistir.
        RepositorioEstoque::novo(uow).inserir_produto(&produto)?;

        Ok(ProdutoCriado {
            produto: produto.id,
        })
    }
}
