//! A **série da NFC-e por terminal** (caixa): cada caixa numera as suas notas na sua série, e os
//! que não têm série própria usam a da empresa (Configurações › Fiscal). Com dois caixas na mesma
//! série a numeração não colide (o número é reservado na transação), mas fica intercalada — e o
//! modo autônomo (caixa sem servidor) vai exigir séries separadas.
//!
//! A série da empresa não pode ser a de um terminal (ela é a "dos caixas sem série própria"), e
//! uma série é de um terminal só. O terminal é a identidade da máquina (`maquina.json` no
//! desktop), a mesma que vai no cupom.

use cardeal_kernel::Id;
#[cfg(feature = "sqlite")]
use cardeal_kernel::{CodigoErro, Erro, Resultado};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

/// Um terminal com série própria.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerieDoTerminal {
    /// O terminal.
    pub terminal: Id,
    /// O nome ("Caixa 2").
    pub nome: String,
    /// A série.
    pub serie: u32,
    /// O próximo número da série, no ambiente atual.
    pub proximo_numero: u64,
}

/// Dá (ou tira) a série própria de um terminal. `serie: None` devolve o terminal à série da
/// empresa; `proximo_numero` acerta a numeração (quando a série já foi usada em outro sistema).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinirSerieDoTerminal {
    /// O terminal.
    pub terminal: Id,
    /// O nome, para as telas (até 40 caracteres).
    pub nome: String,
    /// A série (1 a 889), ou `None` para usar a da empresa.
    pub serie: Option<u32>,
    /// O próximo número, no ambiente atual; `None` mantém.
    pub proximo_numero: Option<u64>,
}

impl Comando for DefinirSerieDoTerminal {
    type Saida = Vec<SerieDoTerminal>;
    const PERMISSAO: &'static str = "fiscal.configurar";
    const RISCO: Risco = Risco::Alto;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<Vec<SerieDoTerminal>> {
        sqlite::definir(&self, ctx, uow)
    }
}

/// Os terminais com série própria, pela série.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct SeriesDosTerminais;

impl Consulta for SeriesDosTerminais {
    type Saida = Vec<SerieDoTerminal>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<SerieDoTerminal>> {
        sqlite::series(c, ctx.empresa)
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use rusqlite::{params, OptionalExtension};

    use super::{
        CodigoErro, Connection, Ctx, DefinirSerieDoTerminal, Erro, Id, Resultado, SerieDoTerminal,
        UnidadeDeTrabalho,
    };
    use crate::sql_documento as sqld;
    use cardeal_fiscal::ModeloFiscal;

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    fn id_de(bytes: &[u8]) -> Id {
        <[u8; 16]>::try_from(bytes).map_or(Id::NULO, Id::de_bytes)
    }

    /// A série própria do terminal, se ele tiver.
    pub(crate) fn serie_do_terminal(
        c: &Connection,
        empresa: Id,
        terminal: Id,
    ) -> Resultado<Option<u32>> {
        c.query_row(
            "SELECT serie FROM fiscal_serie_terminal WHERE empresa = ?1 AND terminal = ?2",
            params![
                empresa.em_bytes().as_slice(),
                terminal.em_bytes().as_slice()
            ],
            |r| r.get(0),
        )
        .optional()
        .map_err(falha)
    }

    pub(super) fn series(c: &Connection, empresa: Id) -> Resultado<Vec<SerieDoTerminal>> {
        let ambiente = crate::sql::ambiente(c, empresa)?;
        let mut stmt = c
            .prepare(
                "SELECT terminal, nome, serie FROM fiscal_serie_terminal WHERE empresa = ?1
                 ORDER BY serie",
            )
            .map_err(falha)?;
        let linhas = stmt
            .query_map([empresa.em_bytes().as_slice()], |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, u32>(2)?,
                ))
            })
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha)?;
        linhas
            .into_iter()
            .map(|(terminal, nome, serie)| {
                Ok(SerieDoTerminal {
                    terminal: id_de(&terminal),
                    nome,
                    serie,
                    proximo_numero: sqld::proximo_numero(
                        c,
                        empresa,
                        ModeloFiscal::Nfce,
                        serie,
                        ambiente,
                    )?,
                })
            })
            .collect()
    }

    pub(super) fn definir(
        cmd: &DefinirSerieDoTerminal,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<Vec<SerieDoTerminal>> {
        let c = uow.conexao();
        let empresa = ctx.empresa.em_bytes();
        let terminal = cmd.terminal.em_bytes();
        let Some(serie) = cmd.serie else {
            c.execute(
                "DELETE FROM fiscal_serie_terminal WHERE empresa = ?1 AND terminal = ?2",
                params![empresa.as_slice(), terminal.as_slice()],
            )
            .map_err(falha)?;
            return series(c, ctx.empresa);
        };
        let nome = cmd.nome.trim();
        if nome.is_empty() || nome.chars().count() > 40 {
            return Err(Erro::novo(
                CodigoErro::CAMPO_OBRIGATORIO,
                "Dê um nome ao caixa (até 40 caracteres), ex.: Caixa 2",
            )
            .no_campo("nome"));
        }
        if !(1..=889).contains(&serie) {
            return Err(
                Erro::novo(CodigoErro::FORA_DA_FAIXA, "A série da NFC-e vai de 1 a 889")
                    .no_campo("serie"),
            );
        }
        if sqld::config_nfce(c, ctx.empresa)?.serie == serie {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                format!(
                    "A série {serie} é a da empresa (a dos caixas sem série própria) — escolha outra"
                ),
            )
            .no_campo("serie"));
        }
        let dono: Option<String> = c
            .query_row(
                "SELECT nome FROM fiscal_serie_terminal
                 WHERE empresa = ?1 AND serie = ?2 AND terminal <> ?3",
                params![empresa.as_slice(), serie, terminal.as_slice()],
                |r| r.get(0),
            )
            .optional()
            .map_err(falha)?;
        if let Some(dono) = dono {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                format!("A série {serie} já é do {dono}"),
            )
            .no_campo("serie"));
        }
        let ambiente = crate::sql::ambiente(c, ctx.empresa)?;
        if let Some(proximo) = cmd.proximo_numero {
            if proximo == 0 || proximo > 999_999_999 {
                return Err(Erro::novo(
                    CodigoErro::FORA_DA_FAIXA,
                    "O próximo número vai de 1 a 999.999.999",
                )
                .no_campo("proximo_numero"));
            }
            if let Some(maior) =
                sqld::maior_numero(c, ctx.empresa, ModeloFiscal::Nfce, serie, ambiente)?
            {
                if proximo <= maior {
                    return Err(Erro::novo(
                        CodigoErro::REGRA_VIOLADA,
                        format!(
                            "O número {maior} já foi usado na série {serie} — o próximo precisa \
                             ser maior"
                        ),
                    )
                    .no_campo("proximo_numero"));
                }
            }
        }
        c.execute(
            "INSERT INTO fiscal_serie_terminal (empresa, terminal, nome, serie)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT (empresa, terminal) DO UPDATE SET nome = excluded.nome,
               serie = excluded.serie",
            params![empresa.as_slice(), terminal.as_slice(), nome, serie],
        )
        .map_err(falha)?;
        if let Some(proximo) = cmd.proximo_numero {
            sqld::definir_proximo(c, ctx.empresa, ModeloFiscal::Nfce, serie, ambiente, proximo)?;
        }
        series(c, ctx.empresa)
    }

    /// A série da empresa não pode virar a de um terminal.
    pub(crate) fn exigir_serie_livre(c: &Connection, empresa: Id, serie: u32) -> Resultado<()> {
        let dono: Option<String> = c
            .query_row(
                "SELECT nome FROM fiscal_serie_terminal WHERE empresa = ?1 AND serie = ?2",
                params![empresa.em_bytes().as_slice(), serie],
                |r| r.get(0),
            )
            .optional()
            .map_err(falha)?;
        match dono {
            Some(dono) => Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                format!("A série {serie} é a do {dono} — a da empresa precisa ser outra"),
            )
            .no_campo("serie")),
            None => Ok(()),
        }
    }
}
