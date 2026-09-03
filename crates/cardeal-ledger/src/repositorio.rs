//! O adaptador SQLite do Razão: [`RepositorioRazao`] implementa [`PortaRazao`] sobre a
//! [`UnidadeDeTrabalho`](cardeal_storage::UnidadeDeTrabalho) real.
//!
//! `docs/05-nucleo-financeiro.md` §3.4 e `docs/06-modelo-de-dados.md` §3. Toda escrita
//! acontece dentro do `SAVEPOINT` da tarefa do escritor — se o comando falhar depois, o
//! lançamento é desfeito junto.

use std::collections::HashMap;

use cardeal_kernel::{Data, Dinheiro, Id, Instante, Quantidade};
use cardeal_storage::UnidadeDeTrabalho;
use rusqlite::{Connection, OptionalExtension};
use smallvec::SmallVec;

use crate::conta::{GrupoFluxo, Natureza, PapelConta, TipoConta};
use crate::erros::ErroRazao;
use crate::lancamento::{Contraparte, EstadoLancamento, Lancamento, Origem, Partida};
use crate::plano::plano_padrao;
use crate::porta::{InfoConta, PortaRazao};

type Resultado<T> = Result<T, ErroRazao>;

#[allow(clippy::needless_pass_by_value)] // usado como `.map_err(persist)`
fn persist(e: rusqlite::Error) -> ErroRazao {
    ErroRazao::FalhaDePersistencia(e.to_string())
}

fn blob(id: Id) -> Vec<u8> {
    id.em_bytes().to_vec()
}

fn blob_opt(id: Option<Id>) -> Option<Vec<u8>> {
    id.map(blob)
}

fn id_de(bytes: Vec<u8>) -> Id {
    <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
}

fn data_de(dias: i64) -> Data {
    Data::de_dias(i32::try_from(dias).unwrap_or(0))
}

fn estado_txt(e: EstadoLancamento) -> &'static str {
    match e {
        EstadoLancamento::Previsto => "Previsto",
        EstadoLancamento::Confirmado => "Confirmado",
        EstadoLancamento::Realizado => "Realizado",
        EstadoLancamento::Estornado => "Estornado",
    }
}

fn estado_de(s: &str) -> EstadoLancamento {
    match s {
        "Previsto" => EstadoLancamento::Previsto,
        "Realizado" => EstadoLancamento::Realizado,
        "Estornado" => EstadoLancamento::Estornado,
        _ => EstadoLancamento::Confirmado,
    }
}

fn natureza_txt(n: Natureza) -> &'static str {
    match n {
        Natureza::Ativo => "Ativo",
        Natureza::Passivo => "Passivo",
        Natureza::PatrimonioLiquido => "PatrimonioLiquido",
        Natureza::Receita => "Receita",
        Natureza::Despesa => "Despesa",
    }
}

fn grupo_fluxo_txt(g: GrupoFluxo) -> &'static str {
    match g {
        GrupoFluxo::Operacional => "Operacional",
        GrupoFluxo::Investimento => "Investimento",
        GrupoFluxo::Financiamento => "Financiamento",
    }
}

fn contraparte_split(c: Option<Contraparte>) -> (Option<&'static str>, Option<Vec<u8>>) {
    match c {
        None => (None, None),
        Some(cp) => {
            let t = match cp {
                Contraparte::Cliente(_) => "Cliente",
                Contraparte::Fornecedor(_) => "Fornecedor",
                Contraparte::Funcionario(_) => "Funcionario",
                Contraparte::Socio(_) => "Socio",
                Contraparte::Outro(_) => "Outro",
            };
            (Some(t), Some(blob(cp.id())))
        }
    }
}

fn contraparte_join(tipo: Option<String>, id: Option<Vec<u8>>) -> Option<Contraparte> {
    let id = id_de(id?);
    Some(match tipo?.as_str() {
        "Fornecedor" => Contraparte::Fornecedor(id),
        "Funcionario" => Contraparte::Funcionario(id),
        "Socio" => Contraparte::Socio(id),
        "Outro" => Contraparte::Outro(id),
        _ => Contraparte::Cliente(id),
    })
}

/// Implementa [`PortaRazao`] gravando e lendo `razao_conta`/`razao_lancamento`/`razao_partida`.
pub struct RepositorioRazao<'a, 'b> {
    uow: &'a mut UnidadeDeTrabalho<'b>,
}

impl<'a, 'b> RepositorioRazao<'a, 'b> {
    /// Cria o repositório sobre a unidade de trabalho corrente.
    pub fn novo(uow: &'a mut UnidadeDeTrabalho<'b>) -> Self {
        Self { uow }
    }

    fn conn(&self) -> &Connection {
        self.uow.conexao()
    }

    fn ler_partidas(&self, lancamento: Id) -> Resultado<SmallVec<[Partida; 4]>> {
        let mut stmt = self
            .conn()
            .prepare(
                "SELECT conta, valor, contraparte_tipo, contraparte_id, centro_custo, projeto,
                        documento, quantidade, complemento
                 FROM razao_partida WHERE lancamento = ?1 ORDER BY ordem",
            )
            .map_err(persist)?;
        let linhas = stmt
            .query_map([blob(lancamento)], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<Vec<u8>>>(3)?,
                    r.get::<_, Option<Vec<u8>>>(4)?,
                    r.get::<_, Option<Vec<u8>>>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, Option<i64>>(7)?,
                    r.get::<_, Option<String>>(8)?,
                ))
            })
            .map_err(persist)?;

        let mut partidas = SmallVec::new();
        for linha in linhas {
            let (conta, valor, cp_t, cp_id, cc, proj, doc, qtd, compl) = linha.map_err(persist)?;
            partidas.push(Partida {
                conta: id_de(conta),
                valor: Dinheiro::centavos(valor),
                contraparte: contraparte_join(cp_t, cp_id),
                centro_custo: cc.map(id_de),
                projeto: proj.map(id_de),
                documento: doc,
                quantidade: qtd.map(Quantidade::interna),
                complemento: compl,
            });
        }
        Ok(partidas)
    }
}

impl PortaRazao for RepositorioRazao<'_, '_> {
    fn usuario(&self) -> Id {
        self.uow.usuario()
    }

    fn dispositivo(&self) -> Id {
        self.uow.dispositivo()
    }

    fn agora(&self) -> Instante {
        self.uow.agora()
    }

    fn info_conta(&self, conta: Id) -> Resultado<Option<InfoConta>> {
        self.conn()
            .query_row(
                "SELECT empresa, codigo, tipo, ativa FROM razao_conta WHERE id = ?1",
                [blob(conta)],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()
            .map_err(persist)
            .map(|opt| {
                opt.map(|(empresa, codigo, tipo, ativa)| InfoConta {
                    empresa: id_de(empresa),
                    codigo,
                    tipo: if tipo == "Sintetica" {
                        TipoConta::Sintetica
                    } else {
                        TipoConta::Analitica
                    },
                    ativa: ativa != 0,
                })
            })
    }

    fn conta_por_papel(&self, empresa: Id, papel: PapelConta) -> Resultado<Option<Id>> {
        self.conn()
            .query_row(
                "SELECT id FROM razao_conta
                 WHERE empresa = ?1 AND papel_padrao = ?2 AND ativa = 1 LIMIT 1",
                rusqlite::params![blob(empresa), papel.to_string()],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(persist)
            .map(|opt| opt.map(id_de))
    }

    fn conta_por_codigo(&self, empresa: Id, codigo: &str) -> Resultado<Option<Id>> {
        self.conn()
            .query_row(
                "SELECT id FROM razao_conta WHERE empresa = ?1 AND codigo = ?2",
                rusqlite::params![blob(empresa), codigo],
                |r| r.get::<_, Vec<u8>>(0),
            )
            .optional()
            .map_err(persist)
            .map(|opt| opt.map(id_de))
    }

    fn periodo_fechado_ate(&self, empresa: Id) -> Resultado<Option<Data>> {
        let ate: Option<i64> = self
            .conn()
            .query_row(
                "SELECT MAX(ate) FROM razao_fechamento WHERE empresa = ?1",
                [blob(empresa)],
                |r| r.get(0),
            )
            .optional()
            .map_err(persist)?
            .flatten();
        Ok(ate.map(data_de))
    }

    fn proximo_numero_lancamento(&mut self, _empresa: Id) -> Resultado<u64> {
        self.uow
            .proximo_numero("razao.lancamento")
            .map_err(|e| ErroRazao::FalhaDePersistencia(e.to_string()))
    }

    fn inserir_lancamento(&mut self, lancamento: Lancamento) -> Resultado<()> {
        let l = &lancamento;
        self.conn()
            .execute(
                "INSERT INTO razao_lancamento
                   (id, empresa, numero, competencia, vencimento, liquidacao, estado,
                    origem_modulo, origem_tipo, origem_id, historico, estorna, estornado_por,
                    criado_em, criado_por, dispositivo)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                rusqlite::params![
                    blob(l.id),
                    blob(l.empresa),
                    i64::try_from(l.numero).unwrap_or(i64::MAX),
                    l.competencia.em_dias(),
                    l.vencimento.map(Data::em_dias),
                    l.liquidacao.map(Data::em_dias),
                    estado_txt(l.estado),
                    l.origem.modulo,
                    l.origem.tipo,
                    blob_opt(l.origem.id),
                    l.historico,
                    blob_opt(l.estorna),
                    blob_opt(l.estornado_por),
                    l.criado_em.em_micros(),
                    blob(l.criado_por),
                    blob(l.dispositivo),
                ],
            )
            .map_err(persist)?;

        for (ordem, p) in l.partidas.iter().enumerate() {
            let (cp_t, cp_id) = contraparte_split(p.contraparte);
            self.conn()
                .execute(
                    "INSERT INTO razao_partida
                       (lancamento, ordem, conta, valor, contraparte_tipo, contraparte_id,
                        centro_custo, projeto, documento, quantidade, complemento)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                    rusqlite::params![
                        blob(l.id),
                        i64::try_from(ordem).unwrap_or(i64::MAX),
                        blob(p.conta),
                        p.valor.em_centavos(),
                        cp_t,
                        cp_id,
                        blob_opt(p.centro_custo),
                        blob_opt(p.projeto),
                        p.documento,
                        p.quantidade.map(Quantidade::unidades_internas),
                        p.complemento,
                    ],
                )
                .map_err(persist)?;
        }
        Ok(())
    }

    fn buscar_lancamento(&self, id: Id) -> Resultado<Option<Lancamento>> {
        let cabecalho = self
            .conn()
            .query_row(
                "SELECT empresa, numero, competencia, vencimento, liquidacao, estado,
                        origem_modulo, origem_tipo, origem_id, historico, estorna,
                        estornado_por, criado_em, criado_por, dispositivo
                 FROM razao_lancamento WHERE id = ?1",
                [blob(id)],
                |r| {
                    Ok((
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, Option<i64>>(3)?,
                        r.get::<_, Option<i64>>(4)?,
                        r.get::<_, String>(5)?,
                        r.get::<_, String>(6)?,
                        r.get::<_, String>(7)?,
                        r.get::<_, Option<Vec<u8>>>(8)?,
                        r.get::<_, String>(9)?,
                        r.get::<_, Option<Vec<u8>>>(10)?,
                        r.get::<_, Option<Vec<u8>>>(11)?,
                        r.get::<_, i64>(12)?,
                        r.get::<_, Vec<u8>>(13)?,
                        r.get::<_, Vec<u8>>(14)?,
                    ))
                },
            )
            .optional()
            .map_err(persist)?;

        let Some(c) = cabecalho else {
            return Ok(None);
        };
        let partidas = self.ler_partidas(id)?;

        Ok(Some(Lancamento {
            id,
            empresa: id_de(c.0),
            numero: u64::try_from(c.1).unwrap_or(0),
            competencia: data_de(c.2),
            vencimento: c.3.map(data_de),
            liquidacao: c.4.map(data_de),
            estado: estado_de(&c.5),
            origem: Origem {
                modulo: c.6,
                tipo: c.7,
                id: c.8.map(id_de),
            },
            historico: c.9,
            estorna: c.10.map(id_de),
            estornado_por: c.11.map(id_de),
            criado_em: Instante::de_micros(c.12),
            criado_por: id_de(c.13),
            dispositivo: id_de(c.14),
            partidas,
        }))
    }

    fn atualizar_lancamento(&mut self, lancamento: Lancamento) -> Resultado<()> {
        let l = &lancamento;
        self.conn()
            .execute(
                "UPDATE razao_lancamento
                 SET estado = ?2, liquidacao = ?3, estorna = ?4, estornado_por = ?5
                 WHERE id = ?1",
                rusqlite::params![
                    blob(l.id),
                    estado_txt(l.estado),
                    l.liquidacao.map(Data::em_dias),
                    blob_opt(l.estorna),
                    blob_opt(l.estornado_por),
                ],
            )
            .map_err(persist)?;
        Ok(())
    }
}

/// Semeia o plano de contas gerencial padrão ([`plano_padrao`]) para `empresa` em
/// `razao_conta`, resolvendo o `pai` de cada conta pelo código.
///
/// # Errors
/// [`ErroRazao::FalhaDePersistencia`] em erro do SQLite.
pub fn semear_plano_padrao(uow: &mut UnidadeDeTrabalho, empresa: Id) -> Resultado<()> {
    let plano = plano_padrao();
    let ids: HashMap<String, Id> = plano
        .iter()
        .map(|s| (s.codigo.como_str().to_string(), Id::novo()))
        .collect();

    for s in &plano {
        let id = ids[s.codigo.como_str()];
        let pai = s.codigo.pai().and_then(|p| ids.get(p.como_str()).copied());
        uow.conexao()
            .execute(
                "INSERT INTO razao_conta
                   (id, empresa, codigo, nome, natureza, tipo, pai, nivel, grupo_fluxo,
                    modulo_origem, papel_padrao, ativa, versao)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,NULL,?10,1,1)",
                rusqlite::params![
                    blob(id),
                    blob(empresa),
                    s.codigo.como_str(),
                    s.nome,
                    natureza_txt(s.natureza),
                    match s.tipo {
                        TipoConta::Sintetica => "Sintetica",
                        TipoConta::Analitica => "Analitica",
                    },
                    blob_opt(pai),
                    i64::from(s.codigo.nivel()),
                    s.grupo_fluxo.map(grupo_fluxo_txt),
                    s.papel.map(|p| p.to_string()),
                ],
            )
            .map_err(persist)?;
    }
    Ok(())
}
