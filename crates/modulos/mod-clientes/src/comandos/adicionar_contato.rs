//! Adiciona um contato (telefone, celular, e-mail ou `WhatsApp`) a uma pessoa já existente.
//!
//! `docs/modulos/clientes.md` §3 — não estava no §5 original (a fatia mínima cobria só
//! cadastro/papel/documento/crédito); sem isto não há como o negócio ligar para o cliente
//! quando o equipamento fica pronto.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::cadastro::Contato;
use crate::cadastro::TipoContato;
#[cfg(feature = "sqlite")]
use crate::erros::ErroClientes;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioClientes;

/// Adiciona um contato a uma pessoa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdicionarContato {
    /// A pessoa.
    pub pessoa: Id,
    /// O tipo do contato.
    pub tipo: TipoContato,
    /// O valor (telefone ou e-mail) — validado conforme o tipo.
    pub valor: String,
    /// Se este passa a ser o contato principal deste tipo.
    pub principal: bool,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContatoFoiAdicionado {
    /// O contato criado.
    pub contato: Id,
}

impl Comando for AdicionarContato {
    type Saida = ContatoFoiAdicionado;
    const PERMISSAO: &'static str = "clientes.pessoa.editar";
    const RISCO: Risco = Risco::Baixo;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        adicionar_contato_comum(&self, ctx, uow)
    }
}

/// O corpo de [`AdicionarContato`], chamável direto por outro módulo dentro da mesma
/// transação (ver [`super::criar_pessoa_comum`]). A permissão fica por conta de quem chama.
///
/// # Errors
/// Os mesmos de [`AdicionarContato`].
#[cfg(feature = "sqlite")]
pub fn adicionar_contato_comum(
    dados: &AdicionarContato,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<ContatoFoiAdicionado> {
    // 1. Carregar.
    let pessoa = RepositorioClientes::novo(uow)
        .buscar_pessoa(dados.pessoa)?
        .ok_or_else(|| Erro::nao_encontrado("pessoa"))?;

    // 2. Validar (domínio puro): formato do contato e estado editável da pessoa.
    if !pessoa.editavel() {
        return Err(Erro::de_dominio(&ErroClientes::PessoaAnonimizada));
    }
    let mut contato =
        Contato::novo(pessoa.id, dados.tipo, &dados.valor).map_err(|e| Erro::de_dominio(&e))?;
    contato.principal = dados.principal;

    // 4. Persistir.
    RepositorioClientes::novo(uow).inserir_contato(ctx.organizacao, &contato)?;

    Ok(ContatoFoiAdicionado {
        contato: contato.id,
    })
}
