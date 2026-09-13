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
mod definir_ponto_pedido;
mod editar_detalhes_tecnicos_produto;
mod registrar_aparelho_origem;
mod registrar_entrada;
mod registrar_entrada_com_lote;
mod registrar_saida;

pub use ajustar_saldo::{AjustarSaldo, SaldoAjustado};
pub use criar_grupo_produto::{CriarGrupoProduto, GrupoProdutoCriado};
pub use criar_local::{CriarLocal, LocalCriado, TipoLocal};
pub use criar_produto::{CriarProduto, ProdutoCriado};
pub use criar_unidade::{CriarUnidade, UnidadeCriada};
pub use definir_ponto_pedido::DefinirPontoPedido;
pub use editar_detalhes_tecnicos_produto::EditarDetalhesTecnicosProduto;
pub use registrar_aparelho_origem::{AparelhoOrigemRegistrado, RegistrarAparelhoOrigem};
pub use registrar_entrada::{EntradaRegistrada, RegistrarEntrada};
pub use registrar_entrada_com_lote::{EntradaComLoteRegistrada, RegistrarEntradaComLote};
pub use registrar_saida::{RegistrarSaida, SaidaRegistrada};

use cardeal_kernel::{Data, Erro, Id, Preco, Quantidade, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::eventos::AbaixoPontoPedido;
use crate::produto::{Lote, OrigemLote};
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

/// Os dados para criar um [`Lote`] (peça com código de post-it) junto de uma entrada —
/// rastreabilidade opcional, pedido do dono da assistência técnica (`docs/modulos/estoque.md`
/// §3): o código curto que vai na peça, de onde ela veio (compra ou aparelho usado
/// desmontado) e, quando aplicável, o aparelho de origem.
#[derive(Debug, Clone)]
pub struct DadosNovoLote {
    /// O código curto e digitável (o texto do post-it), único por empresa.
    pub codigo: String,
    /// De onde veio.
    pub origem: OrigemLote,
    /// O fornecedor, quando `origem = Compra`.
    pub fornecedor: Option<Id>,
    /// O aparelho usado de origem, obrigatório quando `origem = AparelhoUsado`.
    pub aparelho_origem: Option<Id>,
    /// Data de fabricação, quando conhecida.
    pub fabricacao: Option<Data>,
    /// Data de validade, quando o produto controla validade.
    pub validade: Option<Data>,
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
    /// O lote criado, quando a entrada veio de [`registrar_entrada_com_lote_comum`].
    pub lote: Option<Id>,
}

/// Carregar/validar/persistir uma entrada — igual para o comando de despacho e para
/// chamadas diretas de outros módulos. Corpo compartilhado com
/// [`registrar_entrada_com_lote_comum`] — `dados_lote` é `None` aqui, sempre.
fn registrar_entrada_interna(
    dados: DadosEntrada,
    dados_lote: Option<DadosNovoLote>,
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

    // 3. Validar (domínio puro) o lote, quando esta entrada cria uma peça rastreável com
    // código de post-it.
    let lote = dados_lote
        .map(|l| {
            Lote::novo(
                ctx.empresa,
                dados.produto,
                dados.local,
                l.codigo,
                l.origem,
                l.fornecedor,
                l.aparelho_origem,
                l.fabricacao,
                l.validade,
                dados.quantidade,
                dados.custo_unitario,
                ctx.agora,
            )
        })
        .transpose()
        .map_err(|e| Erro::de_dominio(&e))?;

    // 4. Persistir.
    let mut repo = RepositorioEstoque::novo(uow);
    if saldo_e_novo {
        repo.inserir_saldo(&saldo)?;
    } else {
        repo.atualizar_saldo(&saldo)?;
    }
    if let Some(lote) = &lote {
        repo.inserir_lote(lote)?;
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
        lote: lote.as_ref().map(|l| l.id),
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
        lote: lote.map(|l| l.id),
    })
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
    registrar_entrada_interna(dados, None, ctx, uow)
}

/// Como [`registrar_entrada_comum`], mas cria também um [`Lote`] rastreável (o código do
/// post-it) para esta entrada — o que `RegistrarEntrada` usa quando a peça precisa de
/// rastreabilidade individual (`docs/modulos/estoque.md` §3). Função própria, em vez de mais
/// um campo em [`DadosEntrada`], para não quebrar quem já constrói `DadosEntrada` sem saber
/// de lote (`compras`, por exemplo).
///
/// # Errors
/// Erro de domínio (`QuantidadeInvalida`, `CustoUnitarioAusente`, `CodigoLoteVazio`,
/// `AparelhoOrigemAusente`), ou de persistência se o código já existir na empresa.
pub fn registrar_entrada_com_lote_comum(
    dados: DadosEntrada,
    lote: DadosNovoLote,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<EntradaGravada> {
    registrar_entrada_interna(dados, Some(lote), ctx, uow)
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
/// bloqueia — `docs/modulos/estoque.md` §11.2). Corpo compartilhado com
/// [`registrar_saida_de_lote_comum`] — `lote` é `None` aqui, sempre.
///
/// # Errors
/// [`cardeal_kernel::Erro`] com [`crate::ErroEstoque::QuantidadeInvalida`] se a quantidade
/// não for positiva; [`crate::ErroEstoque::LoteInexistente`]/`LoteDeOutroProduto`/
/// `LoteSaldoInsuficiente` quando `lote` é informado e não confere.
fn registrar_saida_interna(
    dados: DadosSaida,
    lote: Option<Id>,
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

    // 3. Validar o lote específico, quando informado: precisa ser do mesmo produto e ter
    // saldo (derivado de `estoque_movimento`) suficiente para a quantidade pedida. A peça
    // física é o que realmente saiu do balcão — travar aqui é o que evita aplicar/vender a
    // mesma peça (ou mais do que ela tem) duas vezes.
    let mut lote_carregado = None;
    if let Some(lote_id) = lote {
        let repo = RepositorioEstoque::novo(uow);
        let lote = repo
            .buscar_lote(lote_id)?
            .ok_or_else(|| Erro::de_dominio(&crate::erros::ErroEstoque::LoteInexistente))?;
        if lote.produto != dados.produto {
            return Err(Erro::de_dominio(&crate::erros::ErroEstoque::LoteDeOutroProduto));
        }
        let disponivel = repo.saldo_do_lote(lote_id)?;
        if disponivel < dados.quantidade {
            return Err(Erro::de_dominio(
                &crate::erros::ErroEstoque::LoteSaldoInsuficiente {
                    disponivel: disponivel.formatar(0),
                    pedido: dados.quantidade.formatar(0),
                },
            ));
        }
        lote_carregado = Some((lote, disponivel));
    }

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
        lote,
        origem_modulo: dados.origem_modulo.to_string(),
        origem_id: dados.origem_id,
        lancamento: None,
        criado_em: ctx.agora,
        criado_por: ctx.usuario,
    };
    repo.inserir_movimento(&movimento)?;

    // Se este consumo esgotou o lote, marca o estado — best-effort de UX (evita que a
    // consulta pelo código mostre uma peça "ativa" que já foi toda usada).
    if let Some((lote, disponivel_antes)) = lote_carregado {
        if disponivel_antes - dados.quantidade <= Quantidade::ZERO
            && lote.estado != crate::produto::EstadoLote::Esgotado
        {
            repo.atualizar_estado_lote(lote.id, crate::produto::EstadoLote::Esgotado)?;
        }
    }

    // 5. Avisar se a saída deixou o disponível do local abaixo do ponto de pedido —
    // best-effort, sem consumidor ainda (`docs/modulos/estoque.md` §8). Usa o `saldo` já
    // carregado (barato: nenhuma consulta extra além do produto).
    if let Some(produto) = RepositorioEstoque::novo(uow).buscar_produto(dados.produto)? {
        if produto.abaixo_do_ponto(saldo.quantidade_disponivel) {
            uow.publicar(AbaixoPontoPedido {
                produto: dados.produto,
                local: dados.local,
                disponivel: saldo.quantidade_disponivel,
                ponto_pedido: produto.ponto_pedido.unwrap_or(Quantidade::ZERO),
            })
            .map_err(|e| Erro::de_dominio(&e))?;
        }
    }

    Ok(SaidaGravada {
        movimento: movimento.id,
        aplicada,
    })
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
    registrar_saida_interna(dados, None, ctx, uow)
}

/// Como [`registrar_saida_comum`], mas consome de um lote/peça rastreável específico — o
/// código do post-it que o técnico digitou — em vez de só abater o saldo agregado do
/// produto. Valida que o lote é do mesmo produto e que ainda tem saldo suficiente (a peça
/// física não pode ser aplicada/vendida duas vezes), e grava o movimento com o lote para o
/// histórico. Função própria, em vez de mais um campo em [`DadosSaida`], para não quebrar
/// quem já constrói `DadosSaida` sem saber de lote (`vendas`/`pdv`, por exemplo).
///
/// # Errors
/// [`crate::ErroEstoque::LoteInexistente`], [`crate::ErroEstoque::LoteDeOutroProduto`],
/// [`crate::ErroEstoque::LoteSaldoInsuficiente`], além dos erros de [`registrar_saida_comum`].
pub fn registrar_saida_de_lote_comum(
    dados: DadosSaida,
    lote: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<SaidaGravada> {
    registrar_saida_interna(dados, Some(lote), ctx, uow)
}
