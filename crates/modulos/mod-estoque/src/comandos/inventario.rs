//! O ciclo do inventário com contagem cega (`docs/modulos/estoque.md` §4, §11.4–5):
//!
//! - [`CriarInventario`]: planeja a contagem de um local (o local inteiro ou um grupo da
//!   estrutura, com a subárvore). Um local só tem um inventário aberto por vez.
//! - [`IniciarContagem`]: tira a foto do saldo (os produtos com saldo registrado no local) e
//!   **trava o local** na mesma transação — entradas, saídas e ajustes ali passam a ser
//!   recusados (`RECURSO_TRAVADO`) até o inventário terminar.
//! - [`RegistrarContagem`]: a quantidade contada de um produto, **sem revelar** a do sistema.
//!   Um produto achado que não estava na lista entra na hora; contar de novo corrige.
//! - [`EncerrarInventario`]: exige tudo contado, gera os ajustes das diferenças (saldo,
//!   movimento e lançamento ao custo médio, a mesma receita do `AjustarSaldo`) e libera o local.
//! - [`CancelarInventario`]: desiste antes de encerrar — libera o local sem ajustar nada.

#[cfg(feature = "sqlite")]
use cardeal_kernel::{Dinheiro, Erro, Resultado};
use cardeal_kernel::{Id, Quantidade};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::ajustar_saldo::{aplicar_delta_de_saldo, OrigemDoAjuste};
#[cfg(feature = "sqlite")]
use crate::erros::ErroEstoque;
#[cfg(feature = "sqlite")]
use crate::eventos::InventarioEncerrado;
#[cfg(feature = "sqlite")]
use crate::inventario::{ContagemItem, Inventario};
#[cfg(feature = "sqlite")]
use crate::repositorio_inventario as sql;

#[cfg(feature = "sqlite")]
fn dominio(e: &ErroEstoque) -> Erro {
    Erro::de_dominio(e)
}

#[cfg(feature = "sqlite")]
fn item_novo(produto: Id, sistema: Quantidade) -> ContagemItem {
    ContagemItem {
        id: Id::novo(),
        produto,
        variacao: None,
        lote: None,
        quantidade_sistema: sistema,
        quantidade_contada: None,
        divergencia: None,
        contado_por: None,
        contado_em: None,
    }
}

/// Planeja o inventário de um local.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarInventario {
    /// O local a contar.
    pub local: Id,
    /// Só este grupo da estrutura (com a subárvore); `None` = o local inteiro.
    pub grupo: Option<Id>,
    /// Uma descrição ("Balanço de outubro", "Corredor das bebidas").
    pub descricao: String,
}

/// O que [`CriarInventario`] devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct InventarioCriado {
    /// O inventário.
    pub inventario: Id,
}

impl Comando for CriarInventario {
    type Saida = InventarioCriado;
    const PERMISSAO: &'static str = "estoque.inventario.criar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let conn = uow.conexao();
        if !sql::local_da_empresa(conn, ctx.empresa, self.local)? {
            return Err(Erro::nao_encontrado("local de estoque"));
        }
        if sql::aberto_no_local(conn, self.local)?.is_some() {
            return Err(dominio(&ErroEstoque::LocalJaEmInventario));
        }
        if let Some(g) = self.grupo {
            if !crate::repositorio::RepositorioEstoque::novo(uow)
                .grupo_existe(ctx.organizacao, g)?
            {
                return Err(Erro::nao_encontrado("grupo de produto"));
            }
        }
        let descricao = match self.descricao.trim() {
            "" => "Inventário".to_owned(),
            d => d.to_owned(),
        };
        let inv = Inventario::criar(ctx.empresa, self.local, descricao, ctx.usuario, vec![]);
        sql::inserir(
            uow.conexao(),
            &inv,
            sql::Recorte {
                grupo: self.grupo,
                criado_em: ctx.agora,
            },
        )?;
        Ok(InventarioCriado { inventario: inv.id })
    }
}

/// Tira a foto do saldo e trava o local para a contagem.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct IniciarContagem {
    /// O inventário.
    pub inventario: Id,
}

/// O que [`IniciarContagem`] devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContagemIniciada {
    /// Quantos produtos entraram na lista.
    pub itens: u32,
}

impl Comando for IniciarContagem {
    type Saida = ContagemIniciada;
    const PERMISSAO: &'static str = "estoque.inventario.contar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (mut inv, recorte) = sql::carregar(uow.conexao(), ctx.empresa, self.inventario)?;
        let grupos = match recorte.grupo {
            Some(g) => {
                let nos = crate::repositorio::RepositorioEstoque::novo(uow)
                    .nos_dos_grupos(ctx.organizacao)?;
                let mut todos = crate::hierarquia::Arvore::de_nos(&nos).descendentes(g);
                todos.push(g);
                Some(todos)
            }
            None => None,
        };
        let foto =
            sql::foto_do_local(uow.conexao(), ctx.organizacao, inv.local, grupos.as_deref())?;
        let itens: Vec<ContagemItem> = foto
            .into_iter()
            .map(|(produto, sistema)| item_novo(produto, sistema))
            .collect();
        inv.abrir_contagem(itens, ctx.agora)
            .map_err(|e| dominio(&e))?;
        let conn = uow.conexao();
        sql::atualizar_cabecalho(conn, &inv)?;
        for i in &inv.itens {
            sql::inserir_item(conn, inv.id, i)?;
        }
        Ok(ContagemIniciada {
            itens: u32::try_from(inv.itens.len()).unwrap_or(u32::MAX),
        })
    }
}

/// A contagem de um produto — cega: a quantidade do sistema não volta.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct RegistrarContagem {
    /// O inventário.
    pub inventario: Id,
    /// O produto contado (se não estava na lista, entra agora).
    pub produto: Id,
    /// A quantidade contada.
    pub quantidade: Quantidade,
}

/// O que [`RegistrarContagem`] devolve — sem a quantidade do sistema, de propósito.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContagemRegistrada {
    /// Quantos itens já foram contados.
    pub contados: u32,
    /// Quantos itens a lista tem.
    pub itens: u32,
    /// `true` quando o produto não estava na lista e entrou agora.
    pub incluido: bool,
}

impl Comando for RegistrarContagem {
    type Saida = ContagemRegistrada;
    const PERMISSAO: &'static str = "estoque.inventario.contar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (mut inv, _) = sql::carregar(uow.conexao(), ctx.empresa, self.inventario)?;
        let na_lista = inv.item_do_produto(self.produto).map(|i| i.id);
        let incluido = na_lista.is_none();
        let item = if let Some(id) = na_lista {
            id
        } else {
            let produto = crate::repositorio::RepositorioEstoque::novo(uow)
                .buscar_produto(self.produto)?
                .filter(|p| p.empresa == ctx.organizacao)
                .ok_or_else(|| Erro::nao_encontrado("produto"))?;
            // O local está travado desde a foto: o disponível de agora é o da foto.
            let sistema = sql::disponivel(uow.conexao(), produto.id, inv.local)?;
            let novo = item_novo(produto.id, sistema);
            let id = inv.incluir_item(novo.clone()).map_err(|e| dominio(&e))?;
            sql::inserir_item(uow.conexao(), inv.id, &novo)?;
            id
        };
        inv.registrar_contagem(item, self.quantidade, ctx.usuario, ctx.agora)
            .map_err(|e| dominio(&e))?;
        if let Some(contado) = inv.itens.iter().find(|i| i.id == item) {
            sql::atualizar_item(uow.conexao(), contado)?;
        }
        Ok(ContagemRegistrada {
            contados: u32::try_from(inv.contados()).unwrap_or(u32::MAX),
            itens: u32::try_from(inv.itens.len()).unwrap_or(u32::MAX),
            incluido,
        })
    }
}

/// Encerra o inventário: ajusta as diferenças e libera o local.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EncerrarInventario {
    /// O inventário.
    pub inventario: Id,
}

/// O que [`EncerrarInventario`] devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct InventarioEncerradoResumo {
    /// Quantos produtos tiveram diferença (e ajuste).
    pub ajustes: u32,
    /// Valor das sobras (ao custo médio).
    pub sobras: cardeal_kernel::Dinheiro,
    /// Valor das faltas (ao custo médio).
    pub faltas: cardeal_kernel::Dinheiro,
}

impl Comando for EncerrarInventario {
    type Saida = InventarioEncerradoResumo;
    const PERMISSAO: &'static str = "estoque.inventario.encerrar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (mut inv, _) = sql::carregar(uow.conexao(), ctx.empresa, self.inventario)?;
        inv.conferir().map_err(|e| dominio(&e))?;
        let ajustes = inv.encerrar(ctx.agora).map_err(|e| dominio(&e))?;
        // O cabeçalho sai de Congelado antes dos ajustes: são eles que destravam o local.
        sql::atualizar_cabecalho(uow.conexao(), &inv)?;
        let (mut sobras, mut faltas) = (Dinheiro::ZERO, Dinheiro::ZERO);
        for a in &ajustes {
            let aplicado = aplicar_delta_de_saldo(
                a.produto,
                inv.local,
                a.delta,
                OrigemDoAjuste::Inventario(inv.id),
                ctx,
                uow,
            )?;
            sql::gravar_valor_ajuste(uow.conexao(), inv.id, a.produto, aplicado.valor)?;
            if a.delta.e_positiva() {
                sobras += aplicado.valor;
            } else {
                faltas += aplicado.valor;
            }
        }
        let total = u32::try_from(ajustes.len()).unwrap_or(u32::MAX);
        uow.publicar(InventarioEncerrado {
            inventario: inv.id,
            local: inv.local,
            total_ajustes: total,
        })
        .map_err(|e| Erro::de_dominio(&e))?;
        Ok(InventarioEncerradoResumo {
            ajustes: total,
            sobras,
            faltas,
        })
    }
}

/// Desiste do inventário antes de encerrar: libera o local, não ajusta nada.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CancelarInventario {
    /// O inventário.
    pub inventario: Id,
}

impl Comando for CancelarInventario {
    type Saida = ();
    const PERMISSAO: &'static str = "estoque.inventario.encerrar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let (mut inv, _) = sql::carregar(uow.conexao(), ctx.empresa, self.inventario)?;
        inv.cancelar(ctx.agora).map_err(|e| dominio(&e))?;
        sql::atualizar_cabecalho(uow.conexao(), &inv)
    }
}
