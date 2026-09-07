//! Materializa as recorrências pendentes de uma empresa — `docs/modulos/financeiro.md` §5,
//! `MaterializarRecorrencia`: "tarefa agendada, sem permissão de usuário". Por isso **não**
//! é um [`Comando`](cardeal_modkit::Comando) — o motor ainda não tem um agendador (nenhum
//! crate deste workspace tem cron/tarefa periódica hoje), então esta é uma função `pub`
//! comum, no mesmo padrão de `mod_compras::importar_nota_da_sefaz`: pronta para um agendador
//! futuro chamar a cada execução, ou para um botão manual "gerar recorrências" enquanto ele
//! não existe.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_ledger::{Contas, PapelConta, Razao, RepositorioRazao};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::comandos::autoria_de;
use crate::receituario::{lancar_titulo, ContasTitulo};
use crate::repositorio::RepositorioFinanceiro;
use crate::titulo::EspecieTitulo;

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
pub fn materializar_recorrencias_pendentes(
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Vec<Id>> {
    let hoje = ctx.hoje();
    let recorrencias = RepositorioFinanceiro::novo(uow).recorrencias_ativas(ctx.empresa)?;

    let mut titulos_gerados = Vec::new();
    for r in recorrencias {
        let Some(vencimento) = r
            .proxima_a_materializar(hoje)
            .map_err(|e| Erro::de_dominio(&e))?
        else {
            continue;
        };
        if RepositorioFinanceiro::novo(uow).recorrencia_ja_materializada_em(r.id, vencimento)? {
            continue;
        }

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
                let id =
                    Razao::registrar(&mut repo_razao, lanc).map_err(|e| Erro::de_dominio(&e))?;
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

        titulos_gerados.push(tcp.titulo.id);
    }
    Ok(titulos_gerados)
}
