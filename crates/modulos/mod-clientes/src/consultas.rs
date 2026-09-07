//! As consultas de clientes: leitura autorizada sobre `clientes_pessoa`/`clientes_papel`/
//! `clientes_documento`/`clientes_limite_credito` — `docs/modulos/clientes.md` §10 (a ficha
//! de pessoa e a busca por papel, para o seletor de cliente/fornecedor de OS/compras/vendas).

use cardeal_kernel::{Id, Instante, Resultado};
use cardeal_modkit::{Consulta, Ctx};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

use crate::cadastro::{Contato, DocumentoPessoa, Endereco};
use crate::credito::LimiteCredito;
use crate::pessoa::{Papel, PapelPessoa, Pessoa};
use crate::repositorio::{
    blob, contato_de_linha, data_de, documento_de_linha, endereco_de_linha, estado_pessoa_de,
    id_de, limite_credito_de_linha, papel_de, papel_txt, persist, tipo_pessoa_de,
};

/// Busca uma pessoa (com seus papéis) pelo id. `Ok(None)` = não existe.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn pessoa_por_id(conexao: &Connection, id: Id) -> Resultado<Option<Pessoa>> {
    let cabecalho = conexao
        .query_row(
            "SELECT id, empresa, tipo, nome, nome_fantasia, data_nascimento_abertura,
                    estado, anonimizado_em, observacao, versao, criado_em
             FROM clientes_pessoa WHERE id = ?1",
            [blob(id)],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, Option<i64>>(5)?,
                    r.get::<_, String>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                    r.get::<_, i64>(9)?,
                    r.get::<_, i64>(10)?,
                ))
            },
        )
        .optional()
        .map_err(persist)?;

    let Some(c) = cabecalho else {
        return Ok(None);
    };
    let papeis = papeis_da_pessoa(conexao, id)?;

    Ok(Some(Pessoa {
        id: id_de(c.0),
        empresa: id_de(c.1),
        tipo: tipo_pessoa_de(&c.2),
        nome: c.3,
        nome_fantasia: c.4,
        data_nascimento_abertura: c.5.map(data_de),
        estado: estado_pessoa_de(&c.6),
        anonimizado_em: c.7.map(Instante::de_micros),
        observacao: c.8,
        papeis,
        versao: cardeal_kernel::Versao::nova(u64::try_from(c.9).unwrap_or(1)),
        criado_em: Instante::de_micros(c.10),
    }))
}

/// Os papéis de uma pessoa.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn papeis_da_pessoa(conexao: &Connection, pessoa: Id) -> Resultado<SmallVec<[PapelPessoa; 2]>> {
    let mut stmt = conexao
        .prepare(
            "SELECT papel, ativo_desde, ativo FROM clientes_papel WHERE pessoa = ?1 ORDER BY rowid",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(pessoa)], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })
        .map_err(persist)?;
    let mut papeis = SmallVec::new();
    for linha in linhas {
        let (papel, ativo_desde, ativo) = linha.map_err(persist)?;
        papeis.push(PapelPessoa {
            papel: papel_de(&papel),
            ativo_desde: data_de(ativo_desde),
            ativo: ativo != 0,
        });
    }
    Ok(papeis)
}

/// Os documentos de uma pessoa.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn documentos_da_pessoa(conexao: &Connection, pessoa: Id) -> Resultado<Vec<DocumentoPessoa>> {
    let mut stmt = conexao
        .prepare("SELECT id, empresa, pessoa, tipo, numero, orgao_emissor, validado_sefaz_em FROM clientes_documento WHERE pessoa = ?1 ORDER BY rowid")
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(pessoa)], documento_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// O limite de crédito de uma pessoa, se já definido.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn limite_credito_da_pessoa(
    conexao: &Connection,
    pessoa: Id,
) -> Resultado<Option<LimiteCredito>> {
    conexao
        .query_row(
            "SELECT id, pessoa, limite, situacao, motivo_bloqueio, bloqueado_em, liberado_por, revisado_em, versao
             FROM clientes_limite_credito WHERE pessoa = ?1",
            [blob(pessoa)],
            limite_credito_de_linha,
        )
        .optional()
        .map_err(persist)
}

/// Os contatos de uma pessoa.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn contatos_da_pessoa(conexao: &Connection, pessoa: Id) -> Resultado<Vec<Contato>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, pessoa, tipo, valor, principal FROM clientes_contato
             WHERE pessoa = ?1 ORDER BY principal DESC, rowid",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(pessoa)], contato_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// Os endereços de uma pessoa.
///
/// # Errors
/// [`cardeal_kernel::CodigoErro::FALHA_INTERNA`] em erro do SQLite.
pub fn enderecos_da_pessoa(conexao: &Connection, pessoa: Id) -> Resultado<Vec<Endereco>> {
    let mut stmt = conexao
        .prepare(
            "SELECT id, pessoa, tipo, logradouro, numero, complemento, bairro, cidade, uf, cep, principal
             FROM clientes_endereco WHERE pessoa = ?1 ORDER BY principal DESC, rowid",
        )
        .map_err(persist)?;
    let linhas = stmt
        .query_map([blob(pessoa)], endereco_de_linha)
        .map_err(persist)?;
    linhas
        .collect::<rusqlite::Result<Vec<_>>>()
        .map_err(persist)
}

/// O que a ficha de pessoa mostra de uma vez: a pessoa (com papéis), os documentos, os
/// contatos, os endereços e o limite de crédito, se houver.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PessoaDetalhada {
    /// A pessoa.
    pub pessoa: Pessoa,
    /// Os documentos cadastrados.
    pub documentos: Vec<DocumentoPessoa>,
    /// Os contatos cadastrados (telefone, celular, e-mail, `WhatsApp`).
    pub contatos: Vec<Contato>,
    /// Os endereços cadastrados.
    pub enderecos: Vec<Endereco>,
    /// O limite de crédito, quando já definido (normalmente só para o papel `Cliente`).
    pub limite_credito: Option<LimiteCredito>,
}

/// Busca a ficha completa de uma pessoa pelo id.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetalhePessoa {
    /// A pessoa.
    pub pessoa: Id,
}

impl Consulta for DetalhePessoa {
    type Saida = Option<PessoaDetalhada>;
    const PERMISSAO: &'static str = "clientes.pessoa.ver";

    fn executar(self, _ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let Some(pessoa) = pessoa_por_id(conexao, self.pessoa)? else {
            return Ok(None);
        };
        let documentos = documentos_da_pessoa(conexao, self.pessoa)?;
        let contatos = contatos_da_pessoa(conexao, self.pessoa)?;
        let enderecos = enderecos_da_pessoa(conexao, self.pessoa)?;
        let limite_credito = limite_credito_da_pessoa(conexao, self.pessoa)?;
        Ok(Some(PessoaDetalhada {
            pessoa,
            documentos,
            contatos,
            enderecos,
            limite_credito,
        }))
    }
}

/// Uma pessoa na lista/busca — o suficiente para um seletor ou uma grade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemPessoa {
    /// A pessoa.
    pub pessoa: Id,
    /// O nome (civil ou razão social).
    pub nome: String,
    /// O documento principal, se houver mais de um; senão o único cadastrado.
    pub documento: Option<String>,
}

/// Lista pessoas ativas com um papel específico, por nome — opcionalmente filtradas por um
/// termo de busca (nome ou documento, `LIKE`). É o que sustenta tanto a tela de Clientes
/// quanto qualquer seletor de cliente/fornecedor em OS, compras e vendas — uma única
/// consulta reaproveitada por todos. Sem cursor real ainda, mesma decisão do financeiro
/// (`docs/09-protocolo-api.md` §5): um teto de 200 linhas é suficiente para o cadastro de
/// uma PME quando combinado com um termo de busca.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PessoasPorPapel {
    /// O papel a filtrar (`Cliente`, `Fornecedor`, ...).
    pub papel: Papel,
    /// Um termo opcional — casa por nome ou documento.
    pub busca: Option<String>,
}

impl Consulta for PessoasPorPapel {
    type Saida = Vec<ItemPessoa>;
    const PERMISSAO: &'static str = "clientes.pessoa.ver";

    fn executar(self, ctx: &Ctx, conexao: &Connection) -> Resultado<Self::Saida> {
        let termo = self
            .busca
            .as_deref()
            .filter(|t| !t.trim().is_empty())
            .map(|t| format!("%{t}%"));
        let mut stmt = conexao
            .prepare(
                "SELECT p.id, p.nome,
                        (SELECT d.numero FROM clientes_documento d WHERE d.pessoa = p.id ORDER BY d.rowid LIMIT 1)
                 FROM clientes_pessoa p
                 JOIN clientes_papel cp ON cp.pessoa = p.id
                 WHERE p.empresa = ?1 AND cp.papel = ?2 AND cp.ativo = 1 AND p.estado = 'Ativa'
                   AND (?3 IS NULL OR p.nome LIKE ?3 OR EXISTS (
                        SELECT 1 FROM clientes_documento d WHERE d.pessoa = p.id AND d.numero LIKE ?3))
                 ORDER BY p.nome ASC
                 LIMIT 200",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map(
                params![blob(ctx.empresa), papel_txt(self.papel), termo],
                |r| {
                    Ok(ItemPessoa {
                        pessoa: id_de(r.get::<_, Vec<u8>>(0)?),
                        nome: r.get(1)?,
                        documento: r.get(2)?,
                    })
                },
            )
            .map_err(persist)?;
        linhas
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(persist)
    }
}
