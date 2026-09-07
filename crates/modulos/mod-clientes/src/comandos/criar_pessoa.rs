//! Cria uma pessoa (física ou jurídica) com um papel e, **opcionalmente**, um documento
//! principal, data de nascimento/abertura, um endereço e um contato.
//!
//! `docs/modulos/clientes.md` §5. O documento não é obrigatório — um cadastro de balcão
//! rápido pode ser só nome + papel. Quando há documento, dispara a checagem de duplicidade
//! **por documento** (certeza — `UNIQUE(empresa, tipo, numero)`); a sugestão por
//! similaridade de nome (`SugerirMesclagem`) fica para quando o submódulo `dedup` tiver um
//! consumidor real.

use cardeal_kernel::{Data, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

use crate::cadastro::{Contato, DocumentoPessoa, Endereco, TipoContato, TipoDocumento, TipoEndereco};
use crate::erros::ErroClientes;
use crate::eventos::PessoaCriada;
use crate::pessoa::{ConstrutorPessoa, Papel, TipoPessoa};
use crate::repositorio::RepositorioClientes;

/// Um endereço informado já no cadastro da pessoa (opcional). Mesmos campos de
/// [`AdicionarEndereco`](super::AdicionarEndereco) sem a `pessoa`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnderecoInicial {
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
}

/// Um contato informado já no cadastro da pessoa (opcional).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContatoInicial {
    /// O tipo do contato.
    pub tipo: TipoContato,
    /// O valor (telefone ou e-mail) — validado conforme o tipo.
    pub valor: String,
}

/// Cria uma pessoa com um papel e, opcionalmente, documento / nascimento / endereço /
/// contato.
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
    /// O tipo do documento principal — `None` = cadastro sem documento.
    pub documento_tipo: Option<TipoDocumento>,
    /// O número do documento (com ou sem máscara — CPF/CNPJ são normalizados).
    pub documento_numero: Option<String>,
    /// Nascimento (PF) ou abertura (PJ), quando informada.
    pub data_nascimento: Option<Data>,
    /// Endereço a gravar junto (opcional).
    pub endereco: Option<EnderecoInicial>,
    /// Contato a gravar junto (opcional).
    pub contato: Option<ContatoInicial>,
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
        if let Some(d) = self.data_nascimento {
            construtor = construtor.nascimento_abertura(d);
        }
        let pessoa = construtor.construir().map_err(|e| Erro::de_dominio(&e))?;

        // Documento é opcional: só quando tipo E número vierem preenchidos.
        let documento = match (self.documento_tipo, self.documento_numero) {
            (Some(tipo), Some(numero)) if !numero.trim().is_empty() => Some(
                DocumentoPessoa::novo(ctx.empresa, pessoa.id, tipo, &numero)
                    .map_err(|e| Erro::de_dominio(&e))?,
            ),
            _ => None,
        };

        let endereco = self
            .endereco
            .map(|e| {
                let mut end = Endereco::novo(
                    pessoa.id,
                    e.tipo,
                    e.logradouro,
                    e.numero,
                    e.bairro,
                    e.cidade,
                    &e.uf,
                    &e.cep,
                )
                .map_err(|err| Erro::de_dominio(&err))?;
                end.complemento = e.complemento;
                end.principal = true;
                Ok::<_, Erro>(end)
            })
            .transpose()?;

        let contato = self
            .contato
            .map(|c| {
                let mut ct = Contato::novo(pessoa.id, c.tipo, &c.valor)
                    .map_err(|err| Erro::de_dominio(&err))?;
                ct.principal = true;
                Ok::<_, Erro>(ct)
            })
            .transpose()?;

        // 2. Duplicidade por documento — certeza, só quando há documento.
        if let Some(doc) = &documento {
            if RepositorioClientes::novo(uow)
                .buscar_documento(ctx.empresa, doc.tipo, &doc.numero)?
                .is_some()
            {
                return Err(Erro::de_dominio(&ErroClientes::DocumentoDuplicado));
            }
        }

        // 3. (não se aplica — este módulo nunca lança no razão.)

        // 4. Persistir.
        let mut repo = RepositorioClientes::novo(uow);
        repo.inserir_pessoa(&pessoa)?;
        if let Some(doc) = &documento {
            repo.inserir_documento(doc)?;
        }
        if let Some(end) = &endereco {
            repo.inserir_endereco(ctx.empresa, end)?;
        }
        if let Some(ct) = &contato {
            repo.inserir_contato(ctx.empresa, ct)?;
        }

        // 5. Publicar.
        uow.publicar(PessoaCriada {
            pessoa: pessoa.id,
            tipo: if pessoa.tipo == TipoPessoa::Juridica {
                "Juridica"
            } else {
                "Fisica"
            },
            documento_principal: documento.as_ref().map(|d| d.id),
        })
        .map_err(|e| Erro::de_dominio(&e))?;

        Ok(PessoaCadastrada { pessoa: pessoa.id })
    }
}
