//! Os comandos de OS (`docs/modulos/os.md` §5). Um arquivo, um comando, nas cinco etapas de
//! `docs/15-convencoes-codigo.md` §3. `AcionarGarantia` fica para depois (ver `src/lib.rs`).

mod abrir_ordem_servico;
mod ajustar_apontamento;
mod aplicar_peca;
mod aprovar_orcamento_os;
mod cancelar_ordem_servico;
mod concluir_execucao;
mod editar_dados_da_ordem;
mod encerrar_apontamento;
mod enviar_para_aprovacao;
mod faturar_ordem_servico;
mod iniciar_apontamento;
mod iniciar_execucao;
mod montar_orcamento_os;
mod registrar_laudo;
mod registrar_mao_de_obra;
mod remover_item_orcamento;
mod reprovar_orcamento_os;

pub use abrir_ordem_servico::{AbrirOrdemServico, OrdemServicoAberta};
pub use ajustar_apontamento::AjustarApontamento;
pub use aplicar_peca::{AplicarPeca, PecaFoiAplicada};
pub use aprovar_orcamento_os::AprovarOrcamentoOs;
pub use cancelar_ordem_servico::CancelarOrdemServico;
pub use concluir_execucao::ConcluirExecucao;
pub use editar_dados_da_ordem::EditarDadosDaOrdem;
pub use encerrar_apontamento::EncerrarApontamento;
pub use enviar_para_aprovacao::EnviarParaAprovacao;
pub use faturar_ordem_servico::{FaturarOrdemServico, OrdemServicoFaturada};
pub use iniciar_apontamento::{ApontamentoIniciado, IniciarApontamento};
pub use iniciar_execucao::IniciarExecucao;
pub use montar_orcamento_os::{ItemOrcamentoNovo, MontarOrcamentoOs};
pub use registrar_laudo::RegistrarLaudo;
pub use registrar_mao_de_obra::RegistrarMaoDeObra;
pub use remover_item_orcamento::{RemoverItemOrcamento, TipoItemOrcamento};
pub use reprovar_orcamento_os::ReprovarOrcamentoOs;

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::Ctx;
use cardeal_storage::UnidadeDeTrabalho;

use crate::ordem::OrdemServico;
use crate::repositorio::RepositorioOs;

/// Carrega uma ordem de serviço ou devolve `NAO_ENCONTRADO`. Usado por todo comando que
/// opera sobre uma OS já existente.
pub(crate) fn carregar_ordem(uow: &mut UnidadeDeTrabalho, id: Id) -> Resultado<OrdemServico> {
    RepositorioOs::novo(uow)
        .buscar_ordem(id)?
        .ok_or_else(|| Erro::nao_encontrado("ordem de serviço"))
}

/// A [`crate::receituario::Autoria`] extraída de um [`Ctx`] de comando.
pub(crate) fn autoria_de(ctx: &Ctx) -> crate::receituario::Autoria {
    crate::receituario::Autoria {
        usuario: ctx.usuario,
        dispositivo: ctx.dispositivo,
        agora: ctx.agora,
        fuso: ctx.fuso,
    }
}
