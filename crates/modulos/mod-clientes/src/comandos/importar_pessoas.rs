//! Importa clientes e fornecedores de uma planilha CSV — ver [`crate::importacao`]. Tudo ou
//! nada: qualquer erro de linha recusa o arquivo inteiro, e todas as pessoas entram na mesma
//! transação, pelo mesmo caminho do cadastro manual ([`criar_pessoa_comum`],
//! [`adicionar_contato_comum`]).

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use super::{adicionar_contato_comum, criar_pessoa_comum};
#[cfg(feature = "sqlite")]
use super::{AdicionarContato, ContatoInicial, CriarPessoa};
#[cfg(feature = "sqlite")]
use crate::credito::LimiteCredito;
#[cfg(feature = "sqlite")]
use crate::importacao::planejar;
#[cfg(feature = "sqlite")]
use crate::repositorio::RepositorioClientes;

/// Cria as pessoas que a planilha traz e o cadastro ainda não tem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportarPessoas {
    /// O conteúdo do CSV.
    pub texto: String,
}

/// O que a importação fez.
#[derive(Debug, Serialize, Deserialize)]
pub struct PessoasImportadas {
    /// Pessoas criadas.
    pub criadas: u32,
    /// Linhas que já estavam no cadastro (ficaram como estavam).
    pub ja_existiam: u32,
    /// Quantas das criadas são fornecedores.
    pub fornecedores: u32,
    /// Quantas entraram com limite de crédito.
    pub com_limite: u32,
}

impl Comando for ImportarPessoas {
    type Saida = PessoasImportadas;
    const PERMISSAO: &'static str = "clientes.pessoa.criar";
    const RISCO: Risco = Risco::Medio;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar + 2. Validar.
        let existentes = crate::consultas::indice_de_pessoas(uow.conexao(), ctx.organizacao)?;
        let plano = planejar(&self.texto, &existentes);
        if let Some(e) = plano.erros.first() {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                format!(
                    "a planilha tem {} linha(s) com erro (a primeira: linha {}: {}); nada foi importado",
                    plano.erros.len(),
                    e.linha,
                    e.mensagem
                ),
            ));
        }
        // A importação não é atalho de permissão.
        if plano.define_limite() && !ctx.concede("clientes.credito.definir_limite") {
            return Err(Erro::novo(
                CodigoErro::SEM_PERMISSAO,
                "o limite de crédito exige a permissão de definir limite",
            ));
        }
        let previa = plano.previa();

        // 4. Persistir (5. publicar fica com o cadastro de cada pessoa).
        for novo in plano.novos {
            let mut contatos = novo.contatos.into_iter();
            let pessoa = criar_pessoa_comum(
                CriarPessoa {
                    tipo: novo.tipo,
                    nome: novo.nome,
                    nome_fantasia: novo.nome_fantasia,
                    papel_inicial: novo.papel,
                    documento_tipo: novo.documento.as_ref().map(|(t, _)| *t),
                    documento_numero: novo.documento.map(|(_, n)| n),
                    data_nascimento: novo.nascimento,
                    endereco: novo.endereco,
                    contato: contatos
                        .next()
                        .map(|(tipo, valor)| ContatoInicial { tipo, valor }),
                },
                ctx,
                uow,
            )?
            .pessoa;
            for (tipo, valor) in contatos {
                adicionar_contato_comum(
                    &AdicionarContato {
                        pessoa,
                        tipo,
                        valor,
                        principal: false,
                    },
                    ctx,
                    uow,
                )?;
            }
            if let Some(limite) = novo.limite {
                let lc = LimiteCredito::definir(pessoa, limite, ctx.agora)
                    .map_err(|e| Erro::de_dominio(&e))?;
                RepositorioClientes::novo(uow).inserir_limite_credito(ctx.organizacao, &lc)?;
            }
        }

        Ok(PessoasImportadas {
            criadas: previa.novos,
            ja_existiam: previa.ja_existiam,
            fornecedores: previa.fornecedores,
            com_limite: previa.com_limite,
        })
    }
}
