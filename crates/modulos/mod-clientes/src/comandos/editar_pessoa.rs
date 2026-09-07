//! Edita os dados cadastrais de uma pessoa já existente.
//!
//! `docs/modulos/clientes.md` §5. Recusa sobre pessoa anonimizada (LGPD) — estado terminal,
//! `docs/modulos/clientes.md` §11.6.

use cardeal_kernel::{Data, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::erros::ErroClientes;
use crate::repositorio::RepositorioClientes;

/// Edita nome, nome fantasia e observação de uma pessoa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditarPessoa {
    /// A pessoa a editar.
    pub pessoa: Id,
    /// O novo nome, se for para trocar.
    pub nome: Option<String>,
    /// O novo nome fantasia, se for para trocar.
    pub nome_fantasia: Option<String>,
    /// A nova observação, se for para trocar.
    pub observacao: Option<String>,
    /// A nova data de nascimento/abertura, se for para trocar.
    pub data_nascimento: Option<Data>,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PessoaEditada {
    /// A pessoa editada.
    pub pessoa: Id,
}

impl Comando for EditarPessoa {
    type Saida = PessoaEditada;
    const PERMISSAO: &'static str = "clientes.pessoa.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, _ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let mut pessoa = RepositorioClientes::novo(uow)
            .buscar_pessoa(self.pessoa)?
            .ok_or_else(|| Erro::nao_encontrado("pessoa"))?;

        // 2. Validar.
        if !pessoa.editavel() {
            return Err(Erro::de_dominio(&ErroClientes::PessoaAnonimizada));
        }
        if let Some(nome) = self.nome {
            let nome = nome.trim();
            if nome.is_empty() {
                return Err(Erro::de_dominio(&ErroClientes::NomeVazio));
            }
            pessoa.nome = nome.to_string();
        }
        if self.nome_fantasia.is_some() {
            pessoa.nome_fantasia = self.nome_fantasia;
        }
        if self.observacao.is_some() {
            pessoa.observacao = self.observacao;
        }
        if self.data_nascimento.is_some() {
            pessoa.data_nascimento_abertura = self.data_nascimento;
        }
        pessoa.versao = pessoa.versao.proxima();

        // 4. Persistir.
        RepositorioClientes::novo(uow).atualizar_pessoa(&pessoa)?;

        Ok(PessoaEditada { pessoa: pessoa.id })
    }
}
