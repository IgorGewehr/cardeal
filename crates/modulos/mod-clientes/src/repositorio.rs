//! Todo o SQL do módulo de clientes (`docs/15-convencoes-codigo.md` §5): um único lugar com
//! strings SQL, sempre colunas explícitas, sempre parâmetros posicionais.
//!
//! [`RepositorioClientes`] grava e lê `clientes_pessoa`/`clientes_papel`/
//! `clientes_documento`/`clientes_limite_credito` sobre a
//! [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) do escritor único — mesmo
//! padrão do `RepositorioFinanceiro`.

use cardeal_kernel::{CodigoErro, Data, Dinheiro, Erro, Id, Instante, Resultado, Uf, Versao};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{params, Connection, OptionalExtension};

use crate::cadastro::{
    Contato, DocumentoPessoa, Endereco, TipoContato, TipoDocumento, TipoEndereco,
};
use crate::credito::{LimiteCredito, SituacaoCredito};
use crate::pessoa::{EstadoPessoa, Papel, PapelPessoa, Pessoa, TipoPessoa};

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
pub(crate) fn persist(e: rusqlite::Error) -> Erro {
    Erro::novo(CodigoErro::FALHA_INTERNA, format!("clientes/SQL: {e}"))
}

pub(crate) fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

pub(crate) fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

fn dias(d: Data) -> i64 {
    i64::from(d.em_dias())
}

pub(crate) fn data_de(dias: i64) -> Data {
    Data::de_dias(i32::try_from(dias).unwrap_or(0))
}

fn versao_de(n: i64) -> Versao {
    Versao::nova(u64::try_from(n).unwrap_or(1))
}

fn versao_i64(v: Versao) -> i64 {
    i64::try_from(v.numero()).unwrap_or(i64::MAX)
}

fn tipo_pessoa_txt(t: TipoPessoa) -> &'static str {
    match t {
        TipoPessoa::Fisica => "Fisica",
        TipoPessoa::Juridica => "Juridica",
    }
}

pub(crate) fn tipo_pessoa_de(s: &str) -> TipoPessoa {
    if s == "Juridica" {
        TipoPessoa::Juridica
    } else {
        TipoPessoa::Fisica
    }
}

fn estado_pessoa_txt(e: EstadoPessoa) -> &'static str {
    match e {
        EstadoPessoa::Ativa => "Ativa",
        EstadoPessoa::Inativa => "Inativa",
        EstadoPessoa::Anonimizada => "Anonimizada",
    }
}

pub(crate) fn estado_pessoa_de(s: &str) -> EstadoPessoa {
    match s {
        "Inativa" => EstadoPessoa::Inativa,
        "Anonimizada" => EstadoPessoa::Anonimizada,
        _ => EstadoPessoa::Ativa,
    }
}

pub(crate) fn papel_txt(p: Papel) -> &'static str {
    match p {
        Papel::Cliente => "Cliente",
        Papel::Fornecedor => "Fornecedor",
        Papel::Transportadora => "Transportadora",
        Papel::Funcionario => "Funcionario",
        Papel::Socio => "Socio",
        Papel::Vendedor => "Vendedor",
    }
}

pub(crate) fn papel_de(s: &str) -> Papel {
    match s {
        "Fornecedor" => Papel::Fornecedor,
        "Transportadora" => Papel::Transportadora,
        "Funcionario" => Papel::Funcionario,
        "Socio" => Papel::Socio,
        "Vendedor" => Papel::Vendedor,
        _ => Papel::Cliente,
    }
}

fn tipo_documento_txt(t: TipoDocumento) -> &'static str {
    match t {
        TipoDocumento::Cpf => "Cpf",
        TipoDocumento::Cnpj => "Cnpj",
        TipoDocumento::Rg => "Rg",
        TipoDocumento::Ie => "Ie",
        TipoDocumento::Im => "Im",
        TipoDocumento::Passaporte => "Passaporte",
    }
}

fn tipo_documento_de(s: &str) -> TipoDocumento {
    match s {
        "Cnpj" => TipoDocumento::Cnpj,
        "Rg" => TipoDocumento::Rg,
        "Ie" => TipoDocumento::Ie,
        "Im" => TipoDocumento::Im,
        "Passaporte" => TipoDocumento::Passaporte,
        _ => TipoDocumento::Cpf,
    }
}

fn tipo_contato_txt(t: TipoContato) -> &'static str {
    match t {
        TipoContato::Telefone => "Telefone",
        TipoContato::Celular => "Celular",
        TipoContato::Email => "Email",
        TipoContato::Whatsapp => "Whatsapp",
    }
}

fn tipo_contato_de(s: &str) -> TipoContato {
    match s {
        "Celular" => TipoContato::Celular,
        "Email" => TipoContato::Email,
        "Whatsapp" => TipoContato::Whatsapp,
        _ => TipoContato::Telefone,
    }
}

fn tipo_endereco_txt(t: TipoEndereco) -> &'static str {
    match t {
        TipoEndereco::Cobranca => "Cobranca",
        TipoEndereco::Entrega => "Entrega",
        TipoEndereco::Comercial => "Comercial",
        TipoEndereco::Residencial => "Residencial",
    }
}

fn tipo_endereco_de(s: &str) -> TipoEndereco {
    match s {
        "Entrega" => TipoEndereco::Entrega,
        "Comercial" => TipoEndereco::Comercial,
        "Residencial" => TipoEndereco::Residencial,
        _ => TipoEndereco::Cobranca,
    }
}

fn uf_de(s: &str) -> Uf {
    Uf::de_sigla(s).unwrap_or(Uf::TODAS[0])
}

fn situacao_credito_txt(s: SituacaoCredito) -> &'static str {
    match s {
        SituacaoCredito::Liberado => "Liberado",
        SituacaoCredito::Bloqueado => "Bloqueado",
    }
}

fn situacao_credito_de(s: &str) -> SituacaoCredito {
    if s == "Bloqueado" {
        SituacaoCredito::Bloqueado
    } else {
        SituacaoCredito::Liberado
    }
}

/// Grava e lê pessoas, papéis, documentos e limite de crédito do módulo de clientes.
pub struct RepositorioClientes<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioClientes<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    /// Grava uma pessoa recém-criada e todos os seus papéis iniciais.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_pessoa(&mut self, p: &Pessoa) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO clientes_pessoa
                   (id, empresa, tipo, nome, nome_fantasia, data_nascimento_abertura, estado,
                    anonimizado_em, observacao, versao, criado_em)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    blob(p.id),
                    blob(p.empresa),
                    tipo_pessoa_txt(p.tipo),
                    p.nome,
                    p.nome_fantasia,
                    p.data_nascimento_abertura.map(dias),
                    estado_pessoa_txt(p.estado),
                    p.anonimizado_em.map(Instante::em_micros),
                    p.observacao,
                    versao_i64(p.versao),
                    p.criado_em.em_micros(),
                ],
            )
            .map_err(persist)?;

        for papel in &p.papeis {
            self.inserir_papel(p.id, p.empresa, papel)?;
        }
        Ok(())
    }

    /// Regrava os dados cadastrais editáveis de uma pessoa (não mexe em papéis).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_pessoa(&mut self, p: &Pessoa) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE clientes_pessoa
                 SET nome = ?2, nome_fantasia = ?3, data_nascimento_abertura = ?4, estado = ?5,
                     anonimizado_em = ?6, observacao = ?7, versao = ?8
                 WHERE id = ?1",
                params![
                    blob(p.id),
                    p.nome,
                    p.nome_fantasia,
                    p.data_nascimento_abertura.map(dias),
                    estado_pessoa_txt(p.estado),
                    p.anonimizado_em.map(Instante::em_micros),
                    p.observacao,
                    versao_i64(p.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Busca uma pessoa (com seus papéis) pelo id. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_pessoa(&self, id: Id) -> Resultado<Option<Pessoa>> {
        crate::consultas::pessoa_por_id(self.conn(), id)
    }

    /// Grava ou atualiza um papel de uma pessoa (upsert por `(pessoa, papel)`).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_papel(&mut self, pessoa: Id, empresa: Id, papel: &PapelPessoa) -> Resultado<()> {
        let afetadas = self
            .conn()
            .execute(
                "UPDATE clientes_papel SET ativo_desde = ?3, ativo = ?4
                 WHERE pessoa = ?1 AND papel = ?2",
                params![
                    blob(pessoa),
                    papel_txt(papel.papel),
                    dias(papel.ativo_desde),
                    i64::from(papel.ativo),
                ],
            )
            .map_err(persist)?;
        if afetadas == 0 {
            self.conn()
                .execute(
                    "INSERT INTO clientes_papel (id, empresa, pessoa, papel, ativo_desde, ativo)
                     VALUES (?1,?2,?3,?4,?5,?6)",
                    params![
                        blob(Id::novo()),
                        blob(empresa),
                        blob(pessoa),
                        papel_txt(papel.papel),
                        dias(papel.ativo_desde),
                        i64::from(papel.ativo),
                    ],
                )
                .map_err(persist)?;
        }
        Ok(())
    }

    /// Grava um documento de uma pessoa.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite (inclusive violação de
    /// `UNIQUE(empresa, tipo, numero)` — o comando confere duplicidade antes de chamar isto).
    pub fn inserir_documento(&mut self, d: &DocumentoPessoa) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO clientes_documento
                   (id, empresa, pessoa, tipo, numero, orgao_emissor, validado_sefaz_em)
                 VALUES (?1,?2,?3,?4,?5,?6,?7)",
                params![
                    blob(d.id),
                    blob(d.empresa),
                    blob(d.pessoa),
                    tipo_documento_txt(d.tipo),
                    d.numero,
                    d.orgao_emissor,
                    d.validado_sefaz_em.map(Instante::em_micros),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Os documentos de uma pessoa, na ordem em que foram cadastrados.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn documentos_da_pessoa(&self, pessoa: Id) -> Resultado<Vec<DocumentoPessoa>> {
        crate::consultas::documentos_da_pessoa(self.conn(), pessoa)
    }

    /// Busca um documento por `(empresa, tipo, numero)` — usado para detectar duplicidade
    /// antes de `CriarPessoa` gravar. `Ok(None)` = não existe.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_documento(
        &self,
        empresa: Id,
        tipo: TipoDocumento,
        numero: &str,
    ) -> Resultado<Option<Id>> {
        self.conn()
            .query_row(
                "SELECT pessoa FROM clientes_documento WHERE empresa = ?1 AND tipo = ?2 AND numero = ?3",
                params![blob(empresa), tipo_documento_txt(tipo), numero],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(persist)
            .map(|opt| opt.map(id_de))
    }

    /// Busca o limite de crédito de uma pessoa. `Ok(None)` = ainda não definido.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn buscar_limite_credito(&self, pessoa: Id) -> Resultado<Option<LimiteCredito>> {
        crate::consultas::limite_credito_da_pessoa(self.conn(), pessoa)
    }

    /// Grava um limite de crédito recém-definido.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_limite_credito(&mut self, empresa: Id, lc: &LimiteCredito) -> Resultado<()> {
        self.conn()
            .execute(
                "INSERT INTO clientes_limite_credito
                   (id, empresa, pessoa, limite, situacao, motivo_bloqueio, bloqueado_em,
                    liberado_por, revisado_em, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                params![
                    blob(lc.id),
                    blob(empresa),
                    blob(lc.pessoa),
                    lc.limite.em_centavos(),
                    situacao_credito_txt(lc.situacao),
                    lc.motivo_bloqueio,
                    lc.bloqueado_em.map(Instante::em_micros),
                    blob_opt(lc.liberado_por),
                    lc.revisado_em.em_micros(),
                    versao_i64(lc.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Grava um contato de uma pessoa. Se `principal`, desmarca qualquer outro contato do
    /// mesmo tipo já marcado como principal — só um principal por tipo (`docs/modulos/
    /// clientes.md` §3).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_contato(&mut self, empresa: Id, c: &Contato) -> Resultado<()> {
        if c.principal {
            self.conn()
                .execute(
                    "UPDATE clientes_contato SET principal = 0 WHERE pessoa = ?1 AND tipo = ?2",
                    params![blob(c.pessoa), tipo_contato_txt(c.tipo)],
                )
                .map_err(persist)?;
        }
        self.conn()
            .execute(
                "INSERT INTO clientes_contato (id, empresa, pessoa, tipo, valor, principal)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    blob(c.id),
                    blob(empresa),
                    blob(c.pessoa),
                    tipo_contato_txt(c.tipo),
                    c.valor,
                    i64::from(c.principal),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Os contatos de uma pessoa.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn contatos_da_pessoa(&self, pessoa: Id) -> Resultado<Vec<Contato>> {
        crate::consultas::contatos_da_pessoa(self.conn(), pessoa)
    }

    /// Grava um endereço de uma pessoa. Se `principal`, desmarca qualquer outro endereço do
    /// mesmo tipo já marcado como principal.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn inserir_endereco(&mut self, empresa: Id, e: &Endereco) -> Resultado<()> {
        if e.principal {
            self.conn()
                .execute(
                    "UPDATE clientes_endereco SET principal = 0 WHERE pessoa = ?1 AND tipo = ?2",
                    params![blob(e.pessoa), tipo_endereco_txt(e.tipo)],
                )
                .map_err(persist)?;
        }
        self.conn()
            .execute(
                "INSERT INTO clientes_endereco
                   (id, empresa, pessoa, tipo, logradouro, numero, complemento, bairro, cidade,
                    uf, cep, principal)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![
                    blob(e.id),
                    blob(empresa),
                    blob(e.pessoa),
                    tipo_endereco_txt(e.tipo),
                    e.logradouro,
                    e.numero,
                    e.complemento,
                    e.bairro,
                    e.cidade,
                    e.uf.sigla(),
                    e.cep,
                    i64::from(e.principal),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }

    /// Os endereços de uma pessoa.
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn enderecos_da_pessoa(&self, pessoa: Id) -> Resultado<Vec<Endereco>> {
        crate::consultas::enderecos_da_pessoa(self.conn(), pessoa)
    }

    /// Regrava um limite de crédito existente (ajuste, bloqueio ou liberação).
    ///
    /// # Errors
    /// [`CodigoErro::FALHA_INTERNA`] em erro do SQLite.
    pub fn atualizar_limite_credito(&mut self, lc: &LimiteCredito) -> Resultado<()> {
        self.conn()
            .execute(
                "UPDATE clientes_limite_credito
                 SET limite = ?2, situacao = ?3, motivo_bloqueio = ?4, bloqueado_em = ?5,
                     liberado_por = ?6, revisado_em = ?7, versao = ?8
                 WHERE id = ?1",
                params![
                    blob(lc.id),
                    lc.limite.em_centavos(),
                    situacao_credito_txt(lc.situacao),
                    lc.motivo_bloqueio,
                    lc.bloqueado_em.map(Instante::em_micros),
                    blob_opt(lc.liberado_por),
                    lc.revisado_em.em_micros(),
                    versao_i64(lc.versao),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }
}

pub(crate) fn documento_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<DocumentoPessoa> {
    Ok(DocumentoPessoa {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        empresa: id_de(r.get::<_, Vec<u8>>(1)?),
        pessoa: id_de(r.get::<_, Vec<u8>>(2)?),
        tipo: tipo_documento_de(&r.get::<_, String>(3)?),
        numero: r.get(4)?,
        orgao_emissor: r.get::<_, Option<String>>(5)?,
        validado_sefaz_em: r.get::<_, Option<i64>>(6)?.map(Instante::de_micros),
    })
}

pub(crate) fn contato_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Contato> {
    Ok(Contato {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        pessoa: id_de(r.get::<_, Vec<u8>>(1)?),
        tipo: tipo_contato_de(&r.get::<_, String>(2)?),
        valor: r.get(3)?,
        principal: r.get::<_, i64>(4)? != 0,
    })
}

pub(crate) fn endereco_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<Endereco> {
    Ok(Endereco {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        pessoa: id_de(r.get::<_, Vec<u8>>(1)?),
        tipo: tipo_endereco_de(&r.get::<_, String>(2)?),
        logradouro: r.get(3)?,
        numero: r.get(4)?,
        complemento: r.get::<_, Option<String>>(5)?,
        bairro: r.get(6)?,
        cidade: r.get(7)?,
        uf: uf_de(&r.get::<_, String>(8)?),
        cep: r.get(9)?,
        principal: r.get::<_, i64>(10)? != 0,
    })
}

pub(crate) fn limite_credito_de_linha(r: &rusqlite::Row<'_>) -> rusqlite::Result<LimiteCredito> {
    Ok(LimiteCredito {
        id: id_de(r.get::<_, Vec<u8>>(0)?),
        pessoa: id_de(r.get::<_, Vec<u8>>(1)?),
        limite: Dinheiro::centavos(r.get::<_, i64>(2)?),
        situacao: situacao_credito_de(&r.get::<_, String>(3)?),
        motivo_bloqueio: r.get::<_, Option<String>>(4)?,
        bloqueado_em: r.get::<_, Option<i64>>(5)?.map(Instante::de_micros),
        liberado_por: r.get::<_, Option<Vec<u8>>>(6)?.map(id_de),
        revisado_em: Instante::de_micros(r.get::<_, i64>(7)?),
        versao: versao_de(r.get::<_, i64>(8)?),
    })
}
