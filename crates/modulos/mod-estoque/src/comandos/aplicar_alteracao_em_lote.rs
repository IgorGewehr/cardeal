//! Aplica uma alteração em lote (margem, grupo, situação) a vários produtos de uma vez,
//! guardando o antes e o depois de cada um para poder desfazer. Tudo-ou-nada.

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

use crate::alteracao_em_lote::Alteracao;
#[cfg(feature = "sqlite")]
use crate::alteracao_em_lote::{Mudanca, MAX_PRODUTOS};
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_lote as sql;

/// Aplica `alteracao` aos `produtos`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AplicarAlteracaoEmLote {
    /// Uma frase para o histórico ("Margem de Bebidas para 25%").
    pub descricao: String,
    /// Os produtos.
    pub produtos: Vec<Id>,
    /// O que mudar.
    pub alteracao: Alteracao,
}

/// O que foi feito.
#[derive(Debug, Serialize, Deserialize)]
pub struct AlteracaoAplicada {
    /// O registro, para desfazer depois.
    pub lote: Id,
    /// Produtos que mudaram.
    pub alterados: u32,
    /// Produtos que já estavam com os valores pedidos.
    pub sem_mudanca: u32,
}

impl Comando for AplicarAlteracaoEmLote {
    type Saida = AlteracaoAplicada;
    const PERMISSAO: &'static str = "estoque.produto.editar";
    const RISCO: Risco = Risco::Alto;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let invalido = |m: &str| Erro::novo(CodigoErro::ENTRADA_INVALIDA, m.to_owned());
        if self.alteracao.e_vazia() {
            return Err(invalido("escolha ao menos um campo para alterar"));
        }
        if self.produtos.is_empty() {
            return Err(invalido("escolha ao menos um produto"));
        }
        if self.produtos.len() > MAX_PRODUTOS {
            return Err(invalido(&format!(
                "no máximo {MAX_PRODUTOS} produtos por alteração; divida por grupo"
            )));
        }
        self.alteracao
            .validar()
            .map_err(|m| Erro::novo(CodigoErro::ENTRADA_INVALIDA, m))?;
        if self.alteracao.mexe_em_fiscal() && !ctx.concede("estoque.tributario.editar") {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "alterar NCM e campos fiscais exige a permissão de perfil tributário",
            ));
        }
        if let Mudanca::Definir(Some(m)) = self.alteracao.margem {
            if !m.e_fracao_valida() || m == cardeal_kernel::Percentual::CEM {
                return Err(Erro::novo(
                    CodigoErro::FORA_DA_FAIXA,
                    "a margem desejada precisa estar entre 0% e 100%",
                ));
            }
        }
        let repo = RepositorioEstoque::novo(uow);
        if let Mudanca::Definir(g) = self.alteracao.grupo {
            if !repo.grupo_existe(ctx.organizacao, g)? {
                return Err(Erro::nao_encontrado("grupo de produto"));
            }
        }

        let lote = Id::novo();
        let conn = repo.conn();
        sql::inserir_lote(
            conn,
            ctx.organizacao,
            lote,
            ctx.usuario,
            ctx.agora,
            self.descricao.trim(),
            &self.alteracao.campos(),
            self.produtos.len(),
        )?;
        let (mut alterados, mut sem_mudanca) = (0_u32, 0_u32);
        for produto in &self.produtos {
            let p = sql::produto_do_lote(conn, ctx.organizacao, *produto)?
                .ok_or_else(|| Erro::nao_encontrado("produto"))?;
            let depois = self.alteracao.aplicar(&p.valores);
            if depois == p.valores {
                sem_mudanca += 1;
                continue;
            }
            sql::gravar_valores(conn, ctx.organizacao, *produto, &depois)?;
            sql::inserir_item(conn, lote, *produto, &p.valores, &depois)?;
            alterados += 1;
        }
        if alterados == 0 {
            // Nada mudaria: o `?` abaixo desfaz o cabeçalho junto (SAVEPOINT da tarefa).
            return Err(invalido(
                "nenhum dos produtos escolhidos mudaria com essa alteração",
            ));
        }
        conn.execute(
            "UPDATE estoque_alteracao_lote SET quantidade = ?2 WHERE id = ?1",
            rusqlite::params![lote.em_bytes().to_vec(), i64::from(alterados)],
        )
        .map_err(crate::repositorio::persist)?;
        Ok(AlteracaoAplicada {
            lote,
            alterados,
            sem_mudanca,
        })
    }
}
