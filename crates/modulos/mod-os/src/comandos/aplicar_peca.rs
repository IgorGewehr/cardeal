//! Aplica uma peça do orçamento: consome o estoque de verdade e grava o custo unitário
//! devolvido.
//!
//! `docs/modulos/os.md` §5 e §11.1: peça só é aplicada com orçamento aprovado (`EmExecucao`
//! só se chega depois de `Aprovada`). Chama `mod_estoque::registrar_saida_comum`/
//! `registrar_saida_de_lote_comum` **direto, na mesma transação** — mesma decisão de
//! arquitetura documentada em `mod_financeiro::lancar_titulo_comum` e
//! `docs/contratos-internos.md` §7 regra 2.
//!
//! Grava também o `local` (para `CancelarOrdemServico` saber para onde devolver, se a OS for
//! cancelada depois de aplicada) e, quando o técnico identificou a peça física pelo código do
//! post-it, o `lote` — a rastreabilidade completa pedida pelo dono da assistência técnica
//! (`docs/modulos/estoque.md` §3): "em que OS/aparelho foi aplicada, quando".

use cardeal_kernel::{Erro, Id, Preco, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use mod_estoque::{registrar_saida_comum, registrar_saida_de_lote_comum, DadosSaida};
use serde::{Deserialize, Serialize};

use crate::comandos::carregar_ordem;
use crate::erros::ErroOs;
use crate::repositorio::RepositorioOs;

/// Aplica uma peça já orçada — consome o estoque no local informado.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AplicarPeca {
    /// A ordem de serviço (para conferir o estado e vincular a origem do consumo).
    pub ordem_servico: Id,
    /// O item de peça do orçamento.
    pub item_peca: Id,
    /// O local de estoque de onde a peça sai.
    pub local: Id,
    /// O lote/peça rastreável específico a consumir (o código do post-it que o técnico
    /// digitou ou escolheu na lista), quando a rastreabilidade individual estiver em uso.
    /// `None` = consome só do saldo agregado do produto, sem vincular a uma peça física
    /// específica — continua funcionando exatamente como antes para quem não usa lote.
    pub lote: Option<Id>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PecaFoiAplicada {
    /// O item de peça atualizado.
    pub item_peca: Id,
    /// O custo unitário real, devolvido pelo estoque.
    pub custo_unitario: Preco,
    /// Verdadeiro se o consumo deixou (ou manteve) o disponível do produto negativo no
    /// local — divergência a conferir, devolvida por `mod_estoque::registrar_saida_comum`
    /// (`docs/modulos/estoque.md` §11.2). Quem chama (a tela) avisa na hora, sem precisar
    /// consultar o estoque de novo.
    pub gerou_divergencia: bool,
}

impl Comando for AplicarPeca {
    type Saida = PecaFoiAplicada;
    const PERMISSAO: &'static str = "os.peca.aplicar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        aplicar_peca_comum(self, ctx, uow)
    }
}

/// Aplica várias peças do orçamento de uma vez — "Aplicar todas as peças" da tela. **Tudo
/// ou nada**: se uma peça falhar (item de outra OS, estado errado), nenhuma sai do estoque.
/// Antes a tela disparava um `AplicarPeca` por item e uma falha no meio deixava parte
/// aplicada e parte não.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AplicarPecas {
    /// A ordem de serviço.
    pub ordem_servico: Id,
    /// Os itens de peça a aplicar.
    pub itens: Vec<Id>,
    /// O local de estoque de onde todas saem.
    pub local: Id,
}

impl Comando for AplicarPecas {
    type Saida = Vec<PecaFoiAplicada>;
    const PERMISSAO: &'static str = "os.peca.aplicar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        self.itens
            .iter()
            .map(|&item_peca| {
                aplicar_peca_comum(
                    AplicarPeca {
                        ordem_servico: self.ordem_servico,
                        item_peca,
                        local: self.local,
                        lote: None,
                    },
                    ctx,
                    uow,
                )
            })
            .collect()
    }
}

/// O corpo de [`AplicarPeca`] — reaproveitado por [`AplicarPecas`] na mesma transação.
///
/// # Errors
/// OS fora de execução, item de outra ordem, item já aplicado, ou erro do estoque.
fn aplicar_peca_comum(
    dados: AplicarPeca,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<PecaFoiAplicada> {
    // 1. Carregar.
    let os = carregar_ordem(uow, dados.ordem_servico)?;
    let mut item = RepositorioOs::novo(uow)
        .buscar_item_peca(dados.item_peca)?
        .ok_or_else(|| Erro::nao_encontrado("item de peça"))?;

    // 2. Validar.
    os.exigir_em_execucao().map_err(|e| Erro::de_dominio(&e))?;
    if item.ordem_servico != os.id {
        return Err(Erro::de_dominio(&ErroOs::ItemNaoPertenceAOrdem));
    }

    consumir_e_aplicar(&mut item, os.id, dados.local, dados.lote, ctx, uow)
}

/// Consome o estoque do item (chamada direta a `mod-estoque`, mesma transação) — do lote
/// específico quando o técnico identificou a peça física, senão do saldo agregado — e grava o
/// item como aplicado com o custo real. Sem checagem de estado: quem chama decide quando pode.
fn consumir_e_aplicar(
    item: &mut crate::execucao::ItemPeca,
    ordem: Id,
    local: Id,
    lote: Option<Id>,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<PecaFoiAplicada> {
    let dados_saida = DadosSaida {
        produto: item.produto,
        local,
        quantidade: item.quantidade,
        origem_modulo: "os",
        origem_id: Some(ordem),
    };
    let saida = match lote {
        Some(lote) => registrar_saida_de_lote_comum(dados_saida, lote, ctx, uow)?,
        None => registrar_saida_comum(dados_saida, ctx, uow)?,
    };

    item.aplicar(saida.aplicada.custo_unitario, local, lote)
        .map_err(|e| Erro::de_dominio(&e))?;
    RepositorioOs::novo(uow).atualizar_item_peca(item)?;

    Ok(PecaFoiAplicada {
        item_peca: item.id,
        custo_unitario: item.custo_unitario,
        gerou_divergencia: saida.aplicada.gerou_divergencia,
    })
}

/// Aplica, sozinho, toda peça do orçamento que ainda não saiu do estoque — chamado por
/// `ConcluirExecucao` e `FaturarOrdemServico`. Antes, concluir recusava com peça pendente e
/// faturar direto (permitido desde a abertura) cobrava a peça sem baixar o estoque e com
/// custo zero, inflando a margem. Cada peça sai do local com mais saldo dela
/// (`mod_estoque::melhor_local_de_saida`); sem saldo, sai assim mesmo e fica sinalizada
/// como divergência, igual ao "Aplicar" manual. Devolve o que foi aplicado.
///
/// # Errors
/// Nenhum local de estoque cadastrado (com peça a aplicar), ou erro do estoque.
pub(crate) fn aplicar_pendentes_automaticamente(
    ordem: Id,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Vec<PecaFoiAplicada>> {
    let pendentes: Vec<crate::execucao::ItemPeca> = RepositorioOs::novo(uow)
        .itens_peca_da_ordem(ordem)?
        .into_iter()
        .filter(|i| !i.aplicada && !i.estornada)
        .collect();
    let mut feitas = Vec::with_capacity(pendentes.len());
    for mut item in pendentes {
        let local = mod_estoque::melhor_local_de_saida(uow.conexao(), ctx.empresa, item.produto)?
            .ok_or_else(|| Erro::de_dominio(&ErroOs::SemLocalDeEstoque))?;
        feitas.push(consumir_e_aplicar(&mut item, ordem, local, None, ctx, uow)?);
    }
    Ok(feitas)
}
