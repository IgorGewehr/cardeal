//! Os comandos do estoque (`docs/modulos/estoque.md` §5). Um arquivo, um comando.
//!
//! `registrar_entrada_comum`/`registrar_saida_comum` são o corpo compartilhado entre o
//! comando de despacho (`RegistrarEntrada`/`RegistrarSaida`, chamado com
//! `origem_modulo: "estoque"`) e a chamada direta de outro módulo já dependente
//! (`os.AplicarPeca` chama `registrar_saida_comum` com `origem_modulo: "os"`, mesma
//! transação — ver a decisão de arquitetura em `docs/contratos-internos.md` §7 regra 2 e o
//! precedente em `mod_financeiro::lancar_titulo_comum`). Fatia mínima desta primeira
//! versão: cadastro (grupo/unidade/produto/local) e entrada/saída simples — sem
//! grade/lote/validade/inventário/transferência (ver `src/lib.rs`).

mod ajustar_saldo;
mod criar_grupo_produto;
mod criar_local;
mod criar_produto;
mod criar_unidade;
mod registrar_entrada;
mod registrar_saida;

pub use ajustar_saldo::{AjustarSaldo, SaldoAjustado};
pub use criar_grupo_produto::{CriarGrupoProduto, GrupoProdutoCriado};
pub use criar_local::{CriarLocal, LocalCriado, TipoLocal};
pub use criar_produto::{CriarProduto, ProdutoCriado};
pub use criar_unidade::{CriarUnidade, UnidadeCriada};
pub use registrar_entrada::{EntradaRegistrada, RegistrarEntrada};
pub use registrar_saida::{RegistrarSaida, SaidaRegistrada};

use cardeal_kernel::{Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::repositorio::RepositorioEstoque;
use crate::saldo::{Movimento, SaidaAplicada, SaldoLocal, TipoMovimento};

/// A [`crate::receituario::Autoria`] extraída de um [`Ctx`] de comando.
pub(crate) fn autoria_de(ctx: &Ctx) -> crate::receituario::Autoria {
    crate::receituario::Autoria {
        usuario: ctx.usuario,
        dispositivo: ctx.dispositivo,
        agora: ctx.agora,
    }
}

/// Os dados que `RegistrarEntrada` e chamadas diretas de outros módulos passam ao corpo
/// comum.
#[derive(Clone, Copy)]
pub struct DadosEntrada {
    /// O produto.
    pub produto: Id,
    /// O local que recebe.
    pub local: Id,
    /// A quantidade que entra.
    pub quantidade: Quantidade,
    /// O custo unitário desta entrada.
    pub custo_unitario: Preco,
    /// O módulo de origem (`"estoque"` para entrada manual, `"compras"`…).
    pub origem_modulo: &'static str,
    /// O agregado de origem, quando houver.
    pub origem_id: Option<Id>,
}

/// O que o corpo comum de entrada devolve.
pub struct EntradaGravada {
    /// O movimento criado.
    pub movimento: Id,
    /// O custo médio resultante após a entrada.
    pub custo_medio: Preco,
}

/// Carregar/validar/persistir uma entrada — igual para o comando de despacho e para
/// chamadas diretas de outros módulos.
///
/// # Errors
/// Erro de domínio (`QuantidadeInvalida`, `CustoUnitarioAusente`).
pub fn registrar_entrada_comum(
    dados: DadosEntrada,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<EntradaGravada> {
    // 1. Carregar (ou criar) o saldo.
    let existente = RepositorioEstoque::novo(uow).buscar_saldo(dados.produto, dados.local)?;
    let saldo_e_novo = existente.is_none();
    let mut saldo = existente.unwrap_or_else(|| {
        SaldoLocal::zerado(ctx.empresa, dados.produto, None, dados.local, ctx.agora)
    });

    // 2. Validar (domínio puro): recalcula o custo médio.
    saldo
        .entrada(dados.quantidade, dados.custo_unitario, ctx.agora)
        .map_err(|e| Erro::de_dominio(&e))?;

    // 4. Persistir.
    let mut repo = RepositorioEstoque::novo(uow);
    if saldo_e_novo {
        repo.inserir_saldo(&saldo)?;
    } else {
        repo.atualizar_saldo(&saldo)?;
    }
    let movimento = Movimento {
        id: Id::novo(),
        empresa: ctx.empresa,
        produto: dados.produto,
        variacao: None,
        local: dados.local,
        tipo: TipoMovimento::Entrada,
        quantidade: dados.quantidade,
        custo_unitario: Some(dados.custo_unitario),
        lote: None,
        origem_modulo: dados.origem_modulo.to_string(),
        origem_id: dados.origem_id,
        lancamento: None,
        criado_em: ctx.agora,
        criado_por: ctx.usuario,
    };
    repo.inserir_movimento(&movimento)?;

    Ok(EntradaGravada {
        movimento: movimento.id,
        custo_medio: saldo.custo_medio,
    })
}

/// Os dados que `RegistrarSaida` e chamadas diretas de outros módulos passam ao corpo
/// comum.
#[derive(Clone, Copy)]
pub struct DadosSaida {
    /// O produto.
    pub produto: Id,
    /// O local de onde sai.
    pub local: Id,
    /// A quantidade que sai.
    pub quantidade: Quantidade,
    /// O módulo de origem (`"estoque"` para saída manual, `"os"`, `"vendas"`…).
    pub origem_modulo: &'static str,
    /// O agregado de origem, quando houver.
    pub origem_id: Option<Id>,
}

/// O que o corpo comum de saída devolve — inclui o custo apurado, para o chamador montar o
/// próprio CMV (`docs/modulos/estoque.md` §7: quem consome é quem lança).
pub struct SaidaGravada {
    /// O movimento criado.
    pub movimento: Id,
    /// O efeito apurado (quantidade, custo unitário, custo total, divergência).
    pub aplicada: SaidaAplicada,
}

/// Carregar/validar/persistir uma saída — igual para o comando de despacho e para chamadas
/// diretas de outros módulos. Aceita saldo insuficiente (fica negativo e sinalizado, nunca
/// bloqueia — `docs/modulos/estoque.md` §11.2).
///
/// # Errors
/// [`cardeal_kernel::Erro`] com [`crate::ErroEstoque::QuantidadeInvalida`] se a quantidade
/// não for positiva.
pub fn registrar_saida_comum(
    dados: DadosSaida,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<SaidaGravada> {
    // 1. Carregar.
    let existente = RepositorioEstoque::novo(uow).buscar_saldo(dados.produto, dados.local)?;
    let saldo_e_novo = existente.is_none();
    let mut saldo = existente.unwrap_or_else(|| {
        SaldoLocal::zerado(ctx.empresa, dados.produto, None, dados.local, ctx.agora)
    });

    // 2. Validar (domínio puro): usa o custo médio vigente, nunca recalcula.
    let aplicada = saldo
        .saida(dados.quantidade, ctx.agora)
        .map_err(|e| Erro::de_dominio(&e))?;

    // 4. Persistir.
    let mut repo = RepositorioEstoque::novo(uow);
    if saldo_e_novo {
        repo.inserir_saldo(&saldo)?;
    } else {
        repo.atualizar_saldo(&saldo)?;
    }
    let movimento = Movimento {
        id: Id::novo(),
        empresa: ctx.empresa,
        produto: dados.produto,
        variacao: None,
        local: dados.local,
        tipo: TipoMovimento::Saida,
        quantidade: dados.quantidade,
        custo_unitario: Some(aplicada.custo_unitario),
        lote: None,
        origem_modulo: dados.origem_modulo.to_string(),
        origem_id: dados.origem_id,
        lancamento: None,
        criado_em: ctx.agora,
        criado_por: ctx.usuario,
    };
    repo.inserir_movimento(&movimento)?;

    Ok(SaidaGravada {
        movimento: movimento.id,
        aplicada,
    })
}
