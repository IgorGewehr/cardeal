//! Cadastro rápido de peça — o balcão digitou "Tela A52 original" no orçamento de uma OS (ou
//! comprou algo que ainda não existe no catálogo) e não quer parar para escolher grupo,
//! unidade e NCM. Usa o grupo "Peças" e a unidade "UN" (criados na primeira vez) e deixa o
//! NCM pendente: ele só importa na emissão fiscal e é preenchido pela primeira nota de compra
//! casada com o produto ([`completar_ncm_se_vazio`]). Se já existe produto ativo com o mesmo
//! nome, devolve esse em vez de duplicar.

use cardeal_kernel::{texto, Erro, Id, Resultado};
use cardeal_modkit::{Comando, Ctx, Risco};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::produto::{Produto, Unidade};
use crate::repositorio::{blob, id_de, persist, RepositorioEstoque};

/// Código do grupo padrão das peças criadas às pressas.
const GRUPO_PADRAO: (&str, &str) = ("PECAS", "Peças");
/// Sigla da unidade padrão.
const UNIDADE_PADRAO: (&str, &str) = ("UN", "Unidade");

/// Cria (ou reaproveita) uma peça só pelo nome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CriarPecaRapida {
    /// O nome, como o balcão chama a peça.
    pub nome: String,
}

/// O que o comando devolve.
#[derive(Debug, Serialize, Deserialize)]
pub struct PecaRapidaCriada {
    /// O produto (novo ou o que já existia com esse nome).
    pub produto: Id,
    /// `false` quando reaproveitou um produto existente.
    pub nova: bool,
}

impl Comando for CriarPecaRapida {
    type Saida = PecaRapidaCriada;
    const PERMISSAO: &'static str = "estoque.produto.criar";
    const RISCO: Risco = Risco::Baixo;

    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Self::Saida> {
        criar_peca_rapida_comum(&self.nome, ctx, uow)
    }
}

/// O corpo de [`CriarPecaRapida`], chamável direto por outro módulo na mesma transação
/// (`os.montar_orcamento.v1` com peça nova). A permissão fica por conta de quem chama.
///
/// # Errors
/// Nome vazio, ou erro de persistência.
pub fn criar_peca_rapida_comum(
    nome: &str,
    ctx: &Ctx,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<PecaRapidaCriada> {
    let nome = nome.trim();
    if let Some(existente) = produto_ativo_com_nome(uow, ctx.empresa, nome)? {
        return Ok(PecaRapidaCriada {
            produto: existente,
            nova: false,
        });
    }
    let grupo = grupo_padrao(uow, ctx.empresa)?;
    let unidade = unidade_padrao(uow, ctx.empresa)?;
    let produto =
        Produto::novo(ctx.empresa, grupo, nome, "", unidade).map_err(|e| Erro::de_dominio(&e))?;
    RepositorioEstoque::novo(uow).inserir_produto(&produto)?;
    Ok(PecaRapidaCriada {
        produto: produto.id,
        nova: true,
    })
}

/// Preenche o NCM de um produto que ainda está sem — chamado quando uma nota de compra casa
/// com ele. Não mexe num NCM já informado (quem cadastrou à mão sabe o que pôs).
///
/// # Errors
/// Erro de persistência.
pub fn completar_ncm_se_vazio(
    produto: Id,
    ncm: &str,
    uow: &mut UnidadeDeTrabalho,
) -> Resultado<()> {
    let ncm = texto::somente_digitos(ncm);
    if ncm.len() != 8 {
        return Ok(());
    }
    uow.conexao()
        .execute(
            "UPDATE estoque_produto SET ncm = ?2, versao = versao + 1 WHERE id = ?1 AND ncm = ''",
            params![blob(produto), ncm],
        )
        .map_err(persist)?;
    Ok(())
}

fn produto_ativo_com_nome(
    uow: &UnidadeDeTrabalho,
    empresa: Id,
    nome: &str,
) -> Resultado<Option<Id>> {
    let chave = texto::chave_busca(nome);
    let mut stmt = uow
        .conexao()
        .prepare("SELECT id, nome FROM estoque_produto WHERE empresa = ?1 AND ativo = 1")
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(empresa)], |r| {
            Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(persist)?;
    for linha in linhas {
        let (id, n) = linha.map_err(persist)?;
        if texto::chave_busca(&n) == chave {
            return Ok(Some(id_de(id)));
        }
    }
    Ok(None)
}

fn grupo_padrao(uow: &mut UnidadeDeTrabalho, empresa: Id) -> Resultado<Id> {
    let existente: Option<Vec<u8>> = uow
        .conexao()
        .query_row(
            "SELECT id FROM estoque_grupo_produto WHERE empresa = ?1 AND codigo = ?2",
            params![blob(empresa), GRUPO_PADRAO.0],
            |r| r.get(0),
        )
        .optional()
        .map_err(persist)?;
    if let Some(id) = existente {
        return Ok(id_de(id));
    }
    let id = Id::novo();
    RepositorioEstoque::novo(uow).inserir_grupo_produto(
        empresa,
        id,
        GRUPO_PADRAO.0,
        GRUPO_PADRAO.1,
        None,
    )?;
    Ok(id)
}

fn unidade_padrao(uow: &mut UnidadeDeTrabalho, empresa: Id) -> Resultado<Id> {
    let existente: Option<Vec<u8>> = uow
        .conexao()
        .query_row(
            "SELECT id FROM estoque_unidade WHERE empresa = ?1 AND sigla = ?2",
            params![blob(empresa), UNIDADE_PADRAO.0],
            |r| r.get(0),
        )
        .optional()
        .map_err(persist)?;
    if let Some(id) = existente {
        return Ok(id_de(id));
    }
    let unidade = Unidade {
        id: Id::novo(),
        empresa,
        sigla: UNIDADE_PADRAO.0.to_owned(),
        nome: UNIDADE_PADRAO.1.to_owned(),
        fracionavel: false,
    };
    RepositorioEstoque::novo(uow).inserir_unidade(&unidade)?;
    Ok(unidade.id)
}
