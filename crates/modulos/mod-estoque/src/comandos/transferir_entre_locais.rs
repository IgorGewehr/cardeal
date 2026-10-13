//! Transfere produtos de um local para outro do mesmo CNPJ (`docs/modulos/estoque.md` §11.6) —
//! ver [`crate::transferencia`]. Tudo numa transação: para cada item, saída na origem e entrada
//! no destino, ao custo médio da origem (o destino recalcula o próprio custo médio).
//!
//! Decisões desta fatia:
//! - **Sem saldo, não transfere.** Diferente da venda (que segue mesmo com saldo negativo,
//!   porque o mundo físico já aconteceu), transferir o que o sistema não tem é erro de
//!   digitação — e espalharia saldo negativo por dois locais.
//! - **Produto com lote ativo na origem é recusado:** o lote guarda o local, e mover só o saldo
//!   deixaria a peça apontando para o lugar errado. A transferência por lote fica para depois.
//! - **Sem lançamento no razão:** o estoque é uma conta só por CNPJ, então a transferência
//!   entre locais não muda saldo contábil (débito e crédito cairiam na mesma conta). Fica nos
//!   movimentos `TransferenciaSaida`/`TransferenciaEntrada`, com origem `transferencia`.
//! - Local em inventário (travado) recusa a transferência, como qualquer movimento.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Arredondamento, Dinheiro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::erros::ErroEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioEstoque;
#[cfg(feature = "sqlite")]
use crate::repositorio_inventario::{exigir_local_livre, local_da_empresa};
#[cfg(feature = "sqlite")]
use crate::repositorio_transferencia as sql;
#[cfg(feature = "sqlite")]
use crate::saldo::{Movimento, SaldoLocal, TipoMovimento};
use crate::transferencia::ItemTransferencia;
#[cfg(feature = "sqlite")]
use crate::transferencia::Transferencia;

/// Transfere produtos entre dois locais do CNPJ da sessão.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferirEntreLocais {
    /// De onde sai.
    pub origem: Id,
    /// Para onde vai.
    pub destino: Id,
    /// Os produtos e quantidades.
    pub itens: Vec<ItemTransferencia>,
    /// Uma observação livre ("reposição da loja").
    pub observacao: String,
}

/// O que a transferência fez.
#[derive(Debug, Serialize, Deserialize)]
pub struct TransferenciaFeita {
    /// O documento.
    pub transferencia: Id,
    /// Quantos produtos.
    pub itens: u32,
    /// O valor transferido, ao custo médio da origem.
    pub valor: cardeal_kernel::Dinheiro,
}

impl Comando for TransferirEntreLocais {
    type Saida = TransferenciaFeita;
    const PERMISSAO: &'static str = "estoque.transferencia.criar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let t = Transferencia::nova(
            ctx.empresa,
            self.origem,
            self.destino,
            &self.itens,
            &self.observacao,
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        for local in [t.origem, t.destino] {
            if !local_da_empresa(uow.conexao(), ctx.empresa, local)? {
                return Err(Erro::nao_encontrado("local de estoque"));
            }
            exigir_local_livre(uow.conexao(), local)?;
        }
        sql::inserir(uow.conexao(), &t, ctx.usuario, ctx.agora)?;

        let mut valor = Dinheiro::ZERO;
        for item in &t.itens {
            valor += transferir_item(&t, item, ctx, uow)?;
        }
        Ok(TransferenciaFeita {
            transferencia: t.id,
            itens: u32::try_from(t.itens.len()).unwrap_or(u32::MAX),
            valor,
        })
    }
}

/// Saída na origem e entrada no destino de um item; devolve o valor ao custo da origem.
#[cfg(feature = "sqlite")]
fn transferir_item(
    t: &Transferencia,
    item: &ItemTransferencia,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Dinheiro> {
    let produto = RepositorioEstoque::novo(uow)
        .buscar_produto(item.produto)?
        .filter(|p| p.empresa == ctx.organizacao)
        .ok_or_else(|| Erro::nao_encontrado("produto"))?;
    if sql::tem_lote_ativo(uow.conexao(), produto.id, t.origem)? {
        return Err(Erro::de_dominio(&ErroEstoque::TransferenciaComLote {
            produto: produto.nome,
        }));
    }

    // Origem: tem que ter o que sai.
    let mut repo = RepositorioEstoque::novo(uow);
    let mut origem = repo
        .buscar_saldo(produto.id, t.origem)?
        .unwrap_or_else(|| SaldoLocal::zerado(ctx.empresa, produto.id, None, t.origem, ctx.agora));
    if origem.quantidade_disponivel < item.quantidade {
        return Err(Erro::de_dominio(&ErroEstoque::SaldoInsuficienteNaOrigem {
            produto: produto.nome,
            disponivel: origem.quantidade_disponivel.to_string(),
            pedido: item.quantidade.to_string(),
        }));
    }
    let custo = origem.custo_medio;
    origem
        .saida(item.quantidade, ctx.agora)
        .map_err(|e| Erro::de_dominio(&e))?;
    repo.atualizar_saldo(&origem)?;

    // Destino: entra ao custo da origem (recalcula o custo médio de lá); sem custo conhecido,
    // só a quantidade muda.
    let existente = repo.buscar_saldo(produto.id, t.destino)?;
    let destino_e_novo = existente.is_none();
    let mut destino = existente
        .unwrap_or_else(|| SaldoLocal::zerado(ctx.empresa, produto.id, None, t.destino, ctx.agora));
    if custo.unidades_internas() > 0 {
        destino
            .entrada(item.quantidade, custo, ctx.agora)
            .map_err(|e| Erro::de_dominio(&e))?;
    } else {
        destino.ajustar(item.quantidade, ctx.agora);
    }
    if destino_e_novo {
        repo.inserir_saldo(&destino)?;
    } else {
        repo.atualizar_saldo(&destino)?;
    }

    for (tipo, local) in [
        (TipoMovimento::TransferenciaSaida, t.origem),
        (TipoMovimento::TransferenciaEntrada, t.destino),
    ] {
        repo.inserir_movimento(&Movimento {
            id: Id::novo(),
            empresa: ctx.empresa,
            produto: produto.id,
            variacao: None,
            local,
            tipo,
            quantidade: item.quantidade,
            custo_unitario: Some(custo),
            lote: None,
            origem_modulo: "transferencia".to_string(),
            origem_id: Some(t.id),
            lancamento: None,
            criado_em: ctx.agora,
            criado_por: ctx.usuario,
        })?;
    }
    sql::inserir_item(uow.conexao(), t.id, produto.id, item.quantidade, custo)?;
    Ok(Dinheiro::de_total(
        item.quantidade,
        custo,
        Arredondamento::MeioAcima,
    ))
}
