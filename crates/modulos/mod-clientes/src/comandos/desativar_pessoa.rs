//! Desativa uma pessoa (sem uso recente): some da busca padrão (`PessoasPorPapel` já filtra
//! `estado = 'Ativa'`), mas nada é apagado — documentos, contatos e endereços continuam
//! intactos, e é reversível por [`super::ReativarPessoa`]. É o "excluir" que a tela de
//! Clientes expõe: um hard delete não cabe aqui, pessoa pode estar referenciada por OS/
//! títulos (`docs/modulos/clientes.md` §4, §11.6 — mesmo raciocínio da anonimização LGPD,
//! só que reversível).

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::erros::ErroClientes;
use crate::repositorio::RepositorioClientes;

/// Desativa uma pessoa.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DesativarPessoa {
    /// A pessoa.
    pub pessoa: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PessoaDesativada {
    /// A pessoa desativada.
    pub pessoa: Id,
}

impl Comando for DesativarPessoa {
    type Saida = PessoaDesativada;
    const PERMISSAO: &'static str = "clientes.pessoa.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut pessoa = RepositorioClientes::novo(uow)
            .buscar_pessoa(self.pessoa)?
            .ok_or_else(|| Erro::nao_encontrado("pessoa"))?;
        if !pessoa.editavel() {
            return Err(Erro::de_dominio(&ErroClientes::PessoaAnonimizada));
        }
        pessoa.desativar();
        RepositorioClientes::novo(uow).atualizar_pessoa(&pessoa)?;
        Ok(PessoaDesativada { pessoa: pessoa.id })
    }
}

/// Reativa uma pessoa antes desativada — devolve à busca padrão.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct ReativarPessoa {
    /// A pessoa.
    pub pessoa: Id,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PessoaReativada {
    /// A pessoa reativada.
    pub pessoa: Id,
}

impl Comando for ReativarPessoa {
    type Saida = PessoaReativada;
    const PERMISSAO: &'static str = "clientes.pessoa.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        let mut pessoa = RepositorioClientes::novo(uow)
            .buscar_pessoa(self.pessoa)?
            .ok_or_else(|| Erro::nao_encontrado("pessoa"))?;
        pessoa.reativar();
        RepositorioClientes::novo(uow).atualizar_pessoa(&pessoa)?;
        Ok(PessoaReativada { pessoa: pessoa.id })
    }
}
