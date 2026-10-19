//! Importa as contas em aberto (a receber e a pagar) de uma planilha CSV — ver
//! [`crate::importacao`]. Tudo ou nada: qualquer erro de linha recusa o arquivo inteiro, e
//! pessoas e títulos entram na mesma transação.
//!
//! Receituário: é **saldo de abertura**, não venda nem despesa deste período — a receita e a
//! despesa já foram reconhecidas no sistema antigo. A receber: D Clientes a receber (com o
//! cliente) · C Lucros acumulados; a pagar: D Lucros acumulados · C Fornecedores. A DRE não
//! muda; o balanço passa a mostrar o que o cliente deve e o que a empresa deve. O título
//! importado pode ser cancelado (estorna o lançamento) enquanto não tiver baixa.

#[cfg(feature = "sqlite")]
use std::collections::HashSet;

#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Id, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_ledger::{Contraparte, PapelConta};
use cardeal_modkit::Comando;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::Risco;
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[cfg(feature = "sqlite")]
use crate::comandos::{lancar_titulo_comum, DadosLancamentoTitulo};
#[cfg(feature = "sqlite")]
use crate::importacao::{
    planejar, resolver, ContasResolvidas, PessoaDaConta, PessoaNova, ORIGEM_IMPORTACAO,
};
#[cfg(feature = "sqlite")]
use crate::titulo::EspecieTitulo;

/// Cria as contas em aberto que a planilha traz e ainda não foram importadas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportarContasEmAberto {
    /// O conteúdo do CSV.
    pub texto: String,
}

/// O que a importação fez.
#[derive(Debug, Serialize, Deserialize)]
pub struct ContasImportadas {
    /// Contas a receber criadas.
    pub a_receber: u32,
    /// Contas a pagar criadas.
    pub a_pagar: u32,
    /// Linhas que já tinham sido importadas.
    pub ja_existiam: u32,
    /// Pessoas cadastradas para acomodá-las.
    pub pessoas_criadas: u32,
}

impl Comando for ImportarContasEmAberto {
    type Saida = ContasImportadas;
    const PERMISSAO: &'static str = "financeiro.titulo.importar";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        // 1. Carregar + 2. Validar.
        let r = resolver_planilha(&self.texto, ctx, uow.conexao())?;
        if let Some(e) = r.erros.first() {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                format!(
                    "a planilha tem {} linha(s) com erro (a primeira: linha {}: {}); nada foi importado",
                    r.erros.len(),
                    e.linha,
                    e.mensagem
                ),
            ));
        }
        exigir_permissoes(&r, ctx)?;

        // Pessoas novas, pelo mesmo caminho do cadastro.
        let mut pessoas: Vec<Id> = Vec::with_capacity(r.pessoas_novas.len());
        for nova in &r.pessoas_novas {
            pessoas.push(cadastrar(nova, ctx, uow)?);
        }

        // 3. Lançar + 4. Persistir + 5. Publicar: o mesmo corpo do título avulso.
        let (mut a_receber, mut a_pagar) = (0u32, 0u32);
        for (conta, pessoa) in r.novas {
            let id = match pessoa {
                PessoaDaConta::Nenhuma => None,
                PessoaDaConta::Existente(id) => Some(id),
                PessoaDaConta::Nova(i) => Some(pessoas[i]),
            };
            let (contraparte, papel_contraparte) = match conta.especie {
                EspecieTitulo::Receber => {
                    a_receber += 1;
                    (id.map(Contraparte::Cliente), PapelConta::ClientesAReceber)
                }
                EspecieTitulo::Pagar => {
                    a_pagar += 1;
                    (id.map(Contraparte::Fornecedor), PapelConta::Fornecedores)
                }
            };
            lancar_titulo_comum(
                DadosLancamentoTitulo {
                    especie: conta.especie,
                    contraparte,
                    valor_total: conta.valor,
                    emissao: conta.emissao,
                    parcelas: 1,
                    primeiro_vencimento: conta.vencimento,
                    intervalo_dias: 30,
                    observacao: conta.observacao,
                    categoria: None,
                    origem_modulo: ORIGEM_IMPORTACAO,
                    origem_id: Some(conta.chave),
                    papel_contraparte,
                    papel_resultado: PapelConta::LucrosAcumulados,
                },
                ctx,
                uow,
            )?;
        }

        Ok(ContasImportadas {
            a_receber,
            a_pagar,
            ja_existiam: r.ja_existiam,
            pessoas_criadas: u32::try_from(pessoas.len()).unwrap_or(u32::MAX),
        })
    }
}

/// Lê a planilha e resolve as pessoas e o que já foi importado (a prévia usa o mesmo).
#[cfg(feature = "sqlite")]
pub(crate) fn resolver_planilha(
    texto: &str,
    ctx: &Ctx,
    conexao: &Connection,
) -> Resultado<ContasResolvidas> {
    let planilha = planejar(texto, ctx.hoje());
    let pessoas = mod_clientes::indice_de_pessoas(conexao, ctx.organizacao)?;
    let importadas = origens_importadas(conexao, ctx.empresa)?;
    Ok(resolver(planilha, &pessoas, |chave| {
        importadas.contains(&chave)
    }))
}

/// As chaves das linhas já importadas (canceladas inclusive: a conta cancelada não volta só
/// porque a mesma planilha foi importada de novo).
#[cfg(feature = "sqlite")]
fn origens_importadas(conexao: &Connection, empresa: Id) -> Resultado<HashSet<Id>> {
    use crate::repositorio::{blob, id_de, persist};
    let mut stmt = conexao
        .prepare(
            "SELECT origem_id FROM financeiro_titulo
             WHERE empresa = ?1 AND origem_modulo = ?2 AND origem_id IS NOT NULL",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map(rusqlite::params![blob(empresa), ORIGEM_IMPORTACAO], |r| {
            r.get::<_, Vec<u8>>(0)
        })
        .map_err(persist)?;
    linhas.map(|l| l.map(id_de).map_err(persist)).collect()
}

/// A importação não é atalho de permissão: criar conta a receber, a pagar e cadastrar
/// pessoa exigem, aqui também, as permissões de sempre.
#[cfg(feature = "sqlite")]
fn exigir_permissoes(r: &ContasResolvidas, ctx: &Ctx) -> Resultado<()> {
    let tem = |e: EspecieTitulo| r.novas.iter().any(|(c, _)| c.especie == e);
    let sem = |mensagem: String| Err(Erro::novo(CodigoErro::SEM_PERMISSAO, mensagem));
    if tem(EspecieTitulo::Receber) && !ctx.concede("financeiro.receber.criar") {
        return sem("as contas a receber exigem a permissão de lançar conta a receber".into());
    }
    if tem(EspecieTitulo::Pagar) && !ctx.concede("financeiro.pagar.criar") {
        return sem("as contas a pagar exigem a permissão de lançar conta a pagar".into());
    }
    if !r.pessoas_novas.is_empty() && !ctx.concede("clientes.pessoa.criar") {
        return sem(format!(
            "{} pessoa(s) da planilha não estão no cadastro: importe os clientes antes ou peça a permissão de cadastrar pessoas",
            r.pessoas_novas.len()
        ));
    }
    Ok(())
}

/// Cadastra uma pessoa da planilha: o nome, o documento e o papel da primeira conta dela.
#[cfg(feature = "sqlite")]
fn cadastrar(nova: &PessoaNova, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Id> {
    use mod_clientes::{CriarPessoa, Papel, TipoDocumento, TipoPessoa};
    let cnpj = nova.documento.as_ref().is_some_and(|d| d.len() == 14);
    let feita = mod_clientes::criar_pessoa_comum(
        CriarPessoa {
            tipo: if cnpj {
                TipoPessoa::Juridica
            } else {
                TipoPessoa::Fisica
            },
            nome: nova.nome.clone(),
            nome_fantasia: None,
            papel_inicial: match nova.especie {
                EspecieTitulo::Receber => Papel::Cliente,
                EspecieTitulo::Pagar => Papel::Fornecedor,
            },
            documento_tipo: nova.documento.as_ref().map(|_| {
                if cnpj {
                    TipoDocumento::Cnpj
                } else {
                    TipoDocumento::Cpf
                }
            }),
            documento_numero: nova.documento.clone(),
            data_nascimento: None,
            endereco: None,
            contato: None,
        },
        ctx,
        uow,
    )?;
    Ok(feita.pessoa)
}

/// A prévia da importação de contas em aberto: quantas a receber e a pagar, os totais, as
/// pessoas que seriam cadastradas e os erros por linha — sem gravar nada.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreviaDaImportacaoDeContas {
    /// O conteúdo do CSV.
    pub texto: String,
}

impl cardeal_modkit::Consulta for PreviaDaImportacaoDeContas {
    type Saida = crate::importacao::PreviaImportacaoContas;
    const PERMISSAO: &'static str = "financeiro.titulo.importar";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let r = resolver_planilha(&self.texto, ctx, conexao)?;
        let nomes = mod_clientes::nomes_das_pessoas(conexao, &r.pessoas_existentes())?;
        Ok(r.previa(ctx.hoje(), &nomes))
    }
}
