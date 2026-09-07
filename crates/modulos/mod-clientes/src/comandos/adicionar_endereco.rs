//! Adiciona um endereço a uma pessoa já existente.
//!
//! `docs/modulos/clientes.md` §3 — não estava no §5 original, mesmo motivo de
//! `AdicionarContato`.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::cadastro::{Endereco, TipoEndereco};
use crate::erros::ErroClientes;
use crate::repositorio::RepositorioClientes;

/// Adiciona um endereço a uma pessoa.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdicionarEndereco {
    /// A pessoa.
    pub pessoa: Id,
    /// O tipo do endereço.
    pub tipo: TipoEndereco,
    /// Logradouro.
    pub logradouro: String,
    /// Número.
    pub numero: String,
    /// Complemento, se houver.
    pub complemento: Option<String>,
    /// Bairro.
    pub bairro: String,
    /// Cidade.
    pub cidade: String,
    /// UF, por sigla (ex.: `"MG"`).
    pub uf: String,
    /// CEP, com ou sem máscara.
    pub cep: String,
    /// Se este passa a ser o endereço principal deste tipo.
    pub principal: bool,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct EnderecoFoiAdicionado {
    /// O endereço criado.
    pub endereco: Id,
}

impl Comando for AdicionarEndereco {
    type Saida = EnderecoFoiAdicionado;
    const PERMISSAO: &'static str = "clientes.pessoa.editar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar.
        let pessoa = RepositorioClientes::novo(uow)
            .buscar_pessoa(self.pessoa)?
            .ok_or_else(|| Erro::nao_encontrado("pessoa"))?;

        // 2. Validar (domínio puro): UF, CEP e estado editável da pessoa.
        if !pessoa.editavel() {
            return Err(Erro::de_dominio(&ErroClientes::PessoaAnonimizada));
        }
        let mut endereco = Endereco::novo(
            pessoa.id,
            self.tipo,
            self.logradouro,
            self.numero,
            self.bairro,
            self.cidade,
            &self.uf,
            &self.cep,
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        endereco.complemento = self.complemento;
        endereco.principal = self.principal;

        // 4. Persistir.
        RepositorioClientes::novo(uow).inserir_endereco(ctx.empresa, &endereco)?;

        Ok(EnderecoFoiAdicionado {
            endereco: endereco.id,
        })
    }
}
