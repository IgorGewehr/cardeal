//! Cria uma pessoa (física ou jurídica) com um papel e um documento principal.
//!
//! `docs/modulos/clientes.md` §5. Dispara a checagem de duplicidade **por documento**
//! (certeza — `UNIQUE(empresa, tipo, numero)`); a sugestão por similaridade de nome
//! (`SugerirMesclagem`) fica para quando o submódulo `dedup` tiver um consumidor real.

use cardeal_kernel::{Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::cadastro::{DocumentoPessoa, TipoDocumento};
use crate::erros::ErroClientes;
use crate::eventos::PessoaCriada;
use crate::pessoa::{ConstrutorPessoa, Papel, TipoPessoa};
use crate::repositorio::RepositorioClientes;

/// Cria uma pessoa com um papel e um documento principal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarPessoa {
    /// Física ou jurídica.
    pub tipo: TipoPessoa,
    /// Nome civil ou razão social.
    pub nome: String,
    /// Nome fantasia (só `Juridica`).
    pub nome_fantasia: Option<String>,
    /// O primeiro papel (ex.: `Cliente`).
    pub papel_inicial: Papel,
    /// O tipo do documento principal.
    pub documento_tipo: TipoDocumento,
    /// O número do documento (com ou sem máscara — CPF/CNPJ são normalizados).
    pub documento_numero: String,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PessoaCadastrada {
    /// A pessoa criada.
    pub pessoa: Id,
}

impl Comando for CriarPessoa {
    type Saida = PessoaCadastrada;
    const PERMISSAO: &'static str = "clientes.pessoa.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Validar (domínio puro).
        let mut construtor = ConstrutorPessoa::nova(
            ctx.empresa,
            self.tipo,
            self.nome,
            self.papel_inicial,
            ctx.agora,
            ctx.hoje(),
        );
        if let Some(fantasia) = self.nome_fantasia {
            construtor = construtor.nome_fantasia(fantasia);
        }
        let pessoa = construtor.construir().map_err(|e| Erro::de_dominio(&e))?;
        let documento = DocumentoPessoa::novo(
            ctx.empresa,
            pessoa.id,
            self.documento_tipo,
            &self.documento_numero,
        )
        .map_err(|e| Erro::de_dominio(&e))?;

        // 2. Duplicidade por documento — certeza, sempre checada.
        if RepositorioClientes::novo(uow)
            .buscar_documento(ctx.empresa, documento.tipo, &documento.numero)?
            .is_some()
        {
            return Err(Erro::de_dominio(&ErroClientes::DocumentoDuplicado));
        }

        // 3. (não se aplica — este módulo nunca lança no razão.)

        // 4. Persistir.
        let mut repo = RepositorioClientes::novo(uow);
        repo.inserir_pessoa(&pessoa)?;
        repo.inserir_documento(&documento)?;

        // 5. Publicar.
        uow.publicar(PessoaCriada {
            pessoa: pessoa.id,
            tipo: if pessoa.tipo == TipoPessoa::Juridica {
                "Juridica"
            } else {
                "Fisica"
            },
            documento_principal: documento.id,
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(PessoaCadastrada { pessoa: pessoa.id })
    }
}
