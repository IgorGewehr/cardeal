//! Materializa as recorrências pendentes de uma empresa — `docs/modulos/financeiro.md` §5,
//! `MaterializarRecorrencia`. O spec a previa como "tarefa agendada", mas o motor ainda não
//! tem agendador; até ter, o desktop dispara [`MaterializarRecorrencias`] ao entrar no sistema
//! e ao abrir a tela de Financeiro. É idempotente (a checagem é contra o título já gravado),
//! então chamar a mais não duplica nada. [`materializar_recorrencias_pendentes`] continua
//! `pub` para o agendador futuro chamar direto, fora do despacho.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_ledger::RepositorioRazao;
#[cfg(feature = "sqlite")]
use cardeal_ledger::{Contas, PapelConta, Razao};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::autoria_de;
#[cfg(feature = "sqlite")]
use crate::receituario::{lancar_titulo, ContasTitulo};
#[cfg(feature = "sqlite")]
use crate::recorrencia::Recorrencia;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioFinanceiro;
#[cfg(feature = "sqlite")]
use crate::titulo::EspecieTitulo;

/// Gera os títulos das recorrências que já venceram ou entraram na janela de geração.
/// Devolve os títulos criados (vazio quando não havia nada pendente).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaterializarRecorrencias;

impl Comando for MaterializarRecorrencias {
    type Saida = Vec<Id>;
    // Quem pode criar a regra pode fazê-la render títulos; um perfil sem acesso ao
    // financeiro simplesmente não dispara a geração (outro usuário o fará).
    const PERMISSAO: &'static str = "financeiro.recorrencia.criar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        materializar_recorrencias_pendentes(ctx, uow)
    }
}

/// Varre as recorrências ativas da empresa e materializa em `Titulo` real cada ocorrência
/// que já entrou na janela de `antecedencia_geracao_dias`. Idempotente: uma ocorrência já
/// materializada para o mesmo vencimento não gera um segundo título (a regra em si não
/// guarda um cursor — a checagem é contra `financeiro_titulo` a cada chamada).
///
/// A conta de contraparte (Clientes a receber/Fornecedores) é resolvida por papel, como em
/// qualquer outro lançamento do módulo; a conta de resultado é a que a própria recorrência
/// escolheu (`conta_contrapartida`), porque ela varia por regra (aluguel vai para uma
/// despesa, uma assinatura de `SaaS` vendida vai para uma receita) e não tem um papel único.
///
/// # Errors
/// Erro de domínio (`RegraDeRecorrenciaInvalida`, papel de conta não mapeado) — desfaz o
/// `SAVEPOINT` da tarefa junto com o resto.
#[cfg(feature = "sqlite")]
pub fn materializar_recorrencias_pendentes(
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Vec<Id>> {
    let hoje = ctx.hoje();
    let recorrencias = RepositorioFinanceiro::novo(uow).recorrencias_ativas(ctx.empresa)?;

    let mut titulos_gerados = Vec::new();
    for r in recorrencias {
        let pendentes = r.pendentes_ate(hoje).map_err(|e| Erro::de_dominio(&e))?;
        for vencimento in pendentes {
            if RepositorioFinanceiro::novo(uow).recorrencia_ja_materializada_em(r.id, vencimento)? {
                continue;
            }
            titulos_gerados.push(materializar_uma(&r, vencimento, ctx, uow)?);
        }
    }
    Ok(titulos_gerados)
}

/// Grava o título de uma ocorrência (`vencimento`) de `r` — lançamento no razão, título e
/// evento, tudo na mesma transação.
#[cfg(feature = "sqlite")]
fn materializar_uma(
    r: &Recorrencia,
    vencimento: cardeal_kernel::Data,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Id> {
    let valor = r.valor_de().map_err(|e| Erro::de_dominio(&e))?;
    let mut tcp = r
        .materializar(vencimento, valor)
        .map_err(|e| Erro::de_dominio(&e))?;

    let conta_contraparte = {
        let repo = RepositorioRazao::novo(uow);
        let papel = if r.especie == EspecieTitulo::Receber {
            PapelConta::ClientesAReceber
        } else {
            PapelConta::Fornecedores
        };
        Contas::nova(&repo, ctx.empresa)
            .papel(papel)
            .map_err(|e| Erro::de_dominio(&e))?
    };

    let lancs = lancar_titulo(
        &tcp,
        ContasTitulo {
            contraparte: conta_contraparte,
            resultado: r.conta_contrapartida,
        },
        autoria_de(ctx),
    )
    .map_err(|e| Erro::de_dominio(&e))?;

    {
        let mut repo_razao = RepositorioRazao::novo(uow);
        for (parcela, lanc) in tcp.parcelas.iter_mut().zip(lancs) {
            let id = Razao::registrar(&mut repo_razao, lanc).map_err(|e| Erro::de_dominio(&e))?;
            parcela.lancamento = Some(id);
        }
    }

    RepositorioFinanceiro::novo(uow).inserir_titulo(&tcp)?;

    uow.publicar(crate::eventos::RecorrenciaMaterializada {
        recorrencia: r.id,
        titulo: tcp.titulo.id,
        vencimento,
        valor,
    })
    .map_err(|e| Erro::de_dominio(&e))?;

    Ok(tcp.titulo.id)
}
