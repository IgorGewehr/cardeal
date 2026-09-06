//! Marca `NaoCompareceu` para compromissos `Confirmado` cujo horário já passou sem início —
//! `docs/modulos/agenda.md` §5, `RegistrarNaoComparecimento`: "tarefa agendada, sem
//! permissão de usuário". Por isso não é [`Comando`](cardeal_modkit::Comando), mesmo padrão
//! de `mod_financeiro::materializar_recorrencias_pendentes`.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::repositorio::RepositorioAgenda;

/// Varre os compromissos `Confirmado` da empresa cujo `fim` já passou de `ctx.agora` e marca
/// cada um como `NaoCompareceu`. Devolve os ids atualizados.
///
/// # Errors
/// Erro de domínio (`EstadoInvalido`, que não deveria ocorrer — a varredura já filtra por
/// `Confirmado`) — desfaz o `SAVEPOINT` da tarefa junto com o resto.
pub fn registrar_nao_comparecimento_pendentes(
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<Vec<Id>> {
    let vencidos = RepositorioAgenda::novo(uow).confirmados_vencidos(ctx.empresa, ctx.agora)?;
    let mut marcados = Vec::with_capacity(vencidos.len());
    for mut c in vencidos {
        c.marcar_nao_compareceu()
            .map_err(|e| Erro::de_dominio(&e))?;
        RepositorioAgenda::novo(uow).atualizar_estado(&c)?;
        marcados.push(c.id);
    }
    Ok(marcados)
}
