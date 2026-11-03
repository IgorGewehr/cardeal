//! A **carta de correção** (CC-e, evento 110110) da NF-e autorizada: corrige o que a lei deixa —
//! dados cadastrais, descrição, endereço, informações complementares —, nunca valores,
//! quantidades, impostos, o CNPJ/CPF de quem emite ou recebe nem as datas (Ajuste SINIEF 01/07).
//! Até 20 por nota; cada uma substitui a anterior (a última traz todas as correções). A NFC-e não
//! tem CC-e.
//!
//! O pedido vai pela mesma fila da nota: sem internet, espera. A SEFAZ que responde "já
//! registrado" (573, a resposta da primeira tentativa se perdeu) conta como registrada.

#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Id, Instante};
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
use cardeal_modkit::{Comando, Consulta, Risco};
#[cfg(feature = "sqlite")]
use cardeal_storage::UnidadeDeTrabalho;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::documento::Desfecho;
use crate::explicacao::explicar;

/// Quantas cartas de correção uma NF-e pode ter.
pub const MAXIMO_DE_CARTAS: u32 = 20;

/// Em que pé está a carta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoCarta {
    /// Esperando a fila.
    Pendente,
    /// Registrada na SEFAZ.
    Registrada,
    /// Recusada pela SEFAZ (o motivo diz por quê).
    Recusada,
}

impl EstadoCarta {
    /// O texto guardado na base.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::Pendente => "Pendente",
            Self::Registrada => "Registrada",
            Self::Recusada => "Recusada",
        }
    }

    /// O contrário de [`Self::como_str`].
    #[must_use]
    pub fn de_str(s: &str) -> Self {
        match s {
            "Registrada" => Self::Registrada,
            "Recusada" => Self::Recusada,
            _ => Self::Pendente,
        }
    }

    /// Para a tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Pendente => "Na fila",
            Self::Registrada => "Registrada",
            Self::Recusada => "Recusada",
        }
    }
}

/// Uma carta de correção.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartaDeCorrecao {
    /// Identidade.
    pub id: Id,
    /// A NF-e.
    pub documento: Id,
    /// A sequência (1 a 20).
    pub sequencia: u32,
    /// O texto da correção.
    pub texto: String,
    /// Em que pé está.
    pub estado: EstadoCarta,
    /// O motivo da recusa (ou da espera).
    pub motivo: Option<String>,
    /// O protocolo do registro.
    pub protocolo: Option<String>,
    /// Quando foi pedida.
    pub criada_em: Instante,
    /// Quando foi registrada.
    pub registrada_em: Option<Instante>,
}

/// O texto da correção, limpo — ou por que não vale (de 15 a 1000 caracteres).
///
/// # Errors
/// Fora do tamanho.
pub fn texto_valido(texto: &str) -> Result<String, String> {
    let t = texto.split_whitespace().collect::<Vec<_>>().join(" ");
    let n = t.chars().count();
    if (15..=1000).contains(&n) {
        Ok(t)
    } else {
        Err("A correção precisa ter de 15 a 1000 caracteres".into())
    }
}

/// O que fazer com a resposta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DestinoDaCarta {
    /// Registrada: o protocolo e o XML do evento (quando vieram).
    Registrada {
        /// O protocolo.
        protocolo: Option<String>,
        /// O `procEventoNFe`.
        xml: Option<String>,
    },
    /// Recusada, com o motivo.
    Recusada(String),
    /// Sem resposta que decida: tentar de novo depois.
    Esperar(String),
}

/// Decide o que fazer com o desfecho do envio de uma carta.
#[must_use]
pub fn decidir_carta(desfecho: Desfecho) -> DestinoDaCarta {
    use cardeal_fiscal::SituacaoRetorno;
    let r = match desfecho {
        Desfecho::Indisponivel(m) => return DestinoDaCarta::Esperar(m),
        Desfecho::Recusado(m) => return DestinoDaCarta::Recusada(m),
        Desfecho::Resposta(r) => r,
    };
    if r.sefaz_fora() {
        return DestinoDaCarta::Esperar(explicar(&r.codigo, &r.motivo));
    }
    match r.situacao {
        SituacaoRetorno::Autorizado => DestinoDaCarta::Registrada {
            protocolo: r.protocolo.filter(|p| !p.is_empty()),
            xml: r.xml.filter(|x| !x.is_empty()),
        },
        // Já registrada (a resposta da primeira tentativa se perdeu).
        SituacaoRetorno::Rejeitado if r.codigo == "573" => DestinoDaCarta::Registrada {
            protocolo: None,
            xml: None,
        },
        SituacaoRetorno::Rejeitado | SituacaoRetorno::Denegado | SituacaoRetorno::Cancelado => {
            DestinoDaCarta::Recusada(explicar(&r.codigo, &r.motivo))
        }
        SituacaoRetorno::Processando | SituacaoRetorno::Erro => {
            DestinoDaCarta::Esperar(explicar(&r.codigo, &r.motivo))
        }
    }
}

/// Pede uma carta de correção para uma NF-e autorizada; a fila a leva à SEFAZ.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmitirCartaDeCorrecao {
    /// A NF-e.
    pub documento: Id,
    /// O texto da correção (15 a 1000 caracteres).
    pub texto: String,
}

impl Comando for EmitirCartaDeCorrecao {
    type Saida = CartaDeCorrecao;
    const PERMISSAO: &'static str = "fiscal.emitir";
    const RISCO: Risco = Risco::Medio;
    const AUDITA: bool = true;

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, uow: &mut UnidadeDeTrabalho) -> Resultado<CartaDeCorrecao> {
        sqlite::emitir(&self, ctx, uow)
    }
}

/// As cartas de correção de uma NF-e, da mais nova para a mais antiga.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct CartasDeCorrecao {
    /// A NF-e.
    pub documento: Id,
}

impl Consulta for CartasDeCorrecao {
    type Saida = Vec<CartaDeCorrecao>;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<Vec<CartaDeCorrecao>> {
        sqlite::cartas(c, ctx.empresa, self.documento)
    }
}

#[cfg(feature = "sqlite")]
pub(crate) mod sqlite {
    use cardeal_fiscal::{AmbienteFiscal, ModeloFiscal};
    use cardeal_kernel::{CodigoErro, Erro, Id, Instante, Resultado};
    use cardeal_modkit::Ctx;
    use cardeal_storage::UnidadeDeTrabalho;
    use rusqlite::{params, Connection, Row};

    use super::{
        texto_valido, CartaDeCorrecao, DestinoDaCarta, EmitirCartaDeCorrecao, EstadoCarta,
        MAXIMO_DE_CARTAS,
    };
    use crate::documento::EstadoDocumento;
    use crate::sql_documento as sqld;

    #[allow(clippy::needless_pass_by_value)]
    fn falha(e: rusqlite::Error) -> Erro {
        Erro::novo(CodigoErro::FALHA_INTERNA, e.to_string())
    }

    fn id_de(b: &[u8]) -> Id {
        <[u8; 16]>::try_from(b).map_or(Id::NULO, Id::de_bytes)
    }

    const COLUNAS: &str = "id, documento, sequencia, texto, estado, motivo, protocolo, criada_em,
                           registrada_em";

    fn carta_de(r: &Row<'_>) -> rusqlite::Result<CartaDeCorrecao> {
        Ok(CartaDeCorrecao {
            id: id_de(&r.get::<_, Vec<u8>>(0)?),
            documento: id_de(&r.get::<_, Vec<u8>>(1)?),
            sequencia: r.get(2)?,
            texto: r.get(3)?,
            estado: EstadoCarta::de_str(&r.get::<_, String>(4)?),
            motivo: r.get(5)?,
            protocolo: r.get(6)?,
            criada_em: Instante::de_micros(r.get(7)?),
            registrada_em: r.get::<_, Option<i64>>(8)?.map(Instante::de_micros),
        })
    }

    pub(super) fn cartas(
        c: &Connection,
        empresa: Id,
        documento: Id,
    ) -> Resultado<Vec<CartaDeCorrecao>> {
        let mut stmt = c
            .prepare(&format!(
                "SELECT {COLUNAS} FROM fiscal_carta_correcao
                 WHERE empresa = ?1 AND documento = ?2 ORDER BY criada_em DESC"
            ))
            .map_err(falha)?;
        let v = stmt
            .query_map(
                params![
                    empresa.em_bytes().as_slice(),
                    documento.em_bytes().as_slice()
                ],
                carta_de,
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha);
        v
    }

    pub(super) fn emitir(
        cmd: &EmitirCartaDeCorrecao,
        ctx: &Ctx,
        uow: &mut UnidadeDeTrabalho,
    ) -> Resultado<CartaDeCorrecao> {
        let texto = texto_valido(&cmd.texto)
            .map_err(|m| Erro::novo(CodigoErro::VALOR_INVALIDO, m).no_campo("texto"))?;
        let c = uow.conexao();
        let doc = sqld::documento(c, ctx.empresa, cmd.documento)?
            .ok_or_else(|| Erro::nao_encontrado("documento fiscal"))?
            .resumo;
        if doc.modelo != ModeloFiscal::Nfe {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                "A NFC-e não tem carta de correção — só a NF-e",
            ));
        }
        if doc.estado != EstadoDocumento::Autorizada {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                format!(
                    "Só NF-e autorizada recebe carta de correção (esta está {})",
                    doc.estado.rotulo()
                ),
            ));
        }
        let (pendentes, registradas, ultima): (u32, u32, Option<u32>) = c
            .query_row(
                "SELECT COALESCE(SUM(estado = 'Pendente'), 0),
                        COALESCE(SUM(estado = 'Registrada'), 0),
                        MAX(CASE WHEN estado = 'Registrada' THEN sequencia END)
                 FROM fiscal_carta_correcao WHERE empresa = ?1 AND documento = ?2",
                params![
                    ctx.empresa.em_bytes().as_slice(),
                    cmd.documento.em_bytes().as_slice()
                ],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(falha)?;
        if pendentes > 0 {
            return Err(Erro::novo(
                CodigoErro::ESTADO_INVALIDO,
                "Já há uma carta de correção desta nota esperando a SEFAZ",
            ));
        }
        if registradas >= MAXIMO_DE_CARTAS {
            return Err(Erro::novo(
                CodigoErro::REGRA_VIOLADA,
                format!("A NF-e já tem as {MAXIMO_DE_CARTAS} cartas de correção que a lei permite"),
            ));
        }
        // A recusada não gasta a sequência: a próxima é a seguinte à última registrada.
        let sequencia = ultima.unwrap_or(0) + 1;
        let id = Id::novo();
        c.execute(
            "INSERT INTO fiscal_carta_correcao
               (id, empresa, documento, sequencia, texto, estado, criada_em)
             VALUES (?1, ?2, ?3, ?4, ?5, 'Pendente', ?6)",
            params![
                id.em_bytes().as_slice(),
                ctx.empresa.em_bytes().as_slice(),
                cmd.documento.em_bytes().as_slice(),
                sequencia,
                texto,
                ctx.agora.em_micros()
            ],
        )
        .map_err(falha)?;
        c.query_row(
            &format!("SELECT {COLUNAS} FROM fiscal_carta_correcao WHERE id = ?1"),
            [id.em_bytes().as_slice()],
            carta_de,
        )
        .map_err(falha)
    }

    /// Uma carta esperando a fila, com o que ela precisa para ir à SEFAZ.
    pub(crate) struct CartaNaFila {
        pub id: Id,
        pub ambiente: AmbienteFiscal,
        pub chave: Option<String>,
        pub sequencia: u32,
        pub texto: String,
        pub tentativas: u32,
    }

    /// As cartas pendentes cuja hora chegou.
    pub(crate) fn pendentes(
        c: &Connection,
        empresa: Id,
        agora: Instante,
        limite: u32,
    ) -> Resultado<Vec<CartaNaFila>> {
        let mut stmt = c
            .prepare(
                "SELECT k.id, d.ambiente, d.chave, k.sequencia, k.texto, k.tentativas
                 FROM fiscal_carta_correcao k JOIN fiscal_documento d ON d.id = k.documento
                 WHERE k.empresa = ?1 AND k.estado = 'Pendente' AND k.proxima_tentativa <= ?2
                 ORDER BY k.criada_em LIMIT ?3",
            )
            .map_err(falha)?;
        let v = stmt
            .query_map(
                params![empresa.em_bytes().as_slice(), agora.em_micros(), limite],
                |r| {
                    Ok(CartaNaFila {
                        id: id_de(&r.get::<_, Vec<u8>>(0)?),
                        ambiente: if r.get::<_, String>(1)? == "producao" {
                            AmbienteFiscal::Producao
                        } else {
                            AmbienteFiscal::Homologacao
                        },
                        chave: r.get(2)?,
                        sequencia: r.get(3)?,
                        texto: r.get(4)?,
                        tentativas: r.get(5)?,
                    })
                },
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha);
        v
    }

    /// Quantas cartas esperam a fila.
    pub(crate) fn quantas_pendentes(c: &Connection, empresa: Id) -> Resultado<u32> {
        c.query_row(
            "SELECT COUNT(*) FROM fiscal_carta_correcao WHERE empresa = ?1 AND estado = 'Pendente'",
            [empresa.em_bytes().as_slice()],
            |r| r.get(0),
        )
        .map_err(falha)
    }

    /// Grava o que veio da SEFAZ (só se a carta ainda está pendente).
    pub(crate) fn aplicar(
        c: &Connection,
        empresa: Id,
        id: Id,
        destino: &DestinoDaCarta,
        tentativas: u32,
        agora: Instante,
    ) -> Resultado<()> {
        let (estado, motivo, protocolo, xml, espera, registrada) = match destino {
            DestinoDaCarta::Registrada { protocolo, xml } => (
                EstadoCarta::Registrada,
                None,
                protocolo.clone(),
                xml.clone(),
                0,
                Some(agora.em_micros()),
            ),
            DestinoDaCarta::Recusada(m) => {
                (EstadoCarta::Recusada, Some(m.clone()), None, None, 0, None)
            }
            DestinoDaCarta::Esperar(m) => (
                EstadoCarta::Pendente,
                Some(m.clone()),
                None,
                None,
                crate::documento::espera(tentativas),
                None,
            ),
        };
        c.execute(
            "UPDATE fiscal_carta_correcao SET estado = ?3, motivo = ?4, protocolo = ?5, xml = ?6,
               tentativas = tentativas + 1, proxima_tentativa = ?7, registrada_em = ?8
             WHERE empresa = ?1 AND id = ?2 AND estado = 'Pendente'",
            params![
                empresa.em_bytes().as_slice(),
                id.em_bytes().as_slice(),
                estado.como_str(),
                motivo,
                protocolo,
                xml,
                agora.em_micros() + espera * 1_000_000,
                registrada
            ],
        )
        .map(|_| ())
        .map_err(falha)
    }

    /// As cartas registradas entre `de` e `ate` (pelo registro), com a chave da nota e o XML —
    /// para o pacote do contador.
    pub(crate) fn registradas_entre(
        c: &Connection,
        empresa: Id,
        de: Instante,
        ate: Instante,
    ) -> Resultado<Vec<(String, u32, String)>> {
        let mut stmt = c
            .prepare(
                "SELECT d.chave, k.sequencia, k.xml
                 FROM fiscal_carta_correcao k JOIN fiscal_documento d ON d.id = k.documento
                 WHERE k.empresa = ?1 AND k.estado = 'Registrada' AND d.ambiente = 'producao'
                   AND k.registrada_em >= ?2 AND k.registrada_em < ?3
                   AND d.chave IS NOT NULL AND k.xml IS NOT NULL
                 ORDER BY d.chave, k.sequencia",
            )
            .map_err(falha)?;
        let v = stmt
            .query_map(
                params![
                    empresa.em_bytes().as_slice(),
                    de.em_micros(),
                    ate.em_micros()
                ],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .map_err(falha)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(falha);
        v
    }
}

#[cfg(test)]
mod testes {
    use cardeal_fiscal::{RetornoSefaz, SituacaoRetorno};

    use super::*;

    fn r(situacao: SituacaoRetorno, codigo: &str) -> Desfecho {
        Desfecho::Resposta(RetornoSefaz {
            situacao,
            codigo: codigo.into(),
            motivo: format!("motivo {codigo}"),
            chave: None,
            protocolo: Some("135".into()),
            recebido_em: None,
            xml: Some("<procEventoNFe/>".into()),
            xml_assinado: None,
            xml_cancelamento: None,
            erros: vec![],
            nao_enviado: false,
        })
    }

    #[test]
    fn o_texto_vale_entre_15_e_1000_caracteres() {
        assert_eq!(
            texto_valido("  Corrige   o bairro\npara Centro ").unwrap(),
            "Corrige o bairro para Centro"
        );
        assert!(texto_valido("curto").is_err());
        assert!(texto_valido(&"x".repeat(1001)).is_err());
    }

    #[test]
    fn a_resposta_da_sefaz() {
        assert_eq!(
            decidir_carta(r(SituacaoRetorno::Autorizado, "135")),
            DestinoDaCarta::Registrada {
                protocolo: Some("135".into()),
                xml: Some("<procEventoNFe/>".into())
            }
        );
        assert!(matches!(
            decidir_carta(r(SituacaoRetorno::Rejeitado, "573")),
            DestinoDaCarta::Registrada { xml: None, .. }
        ));
        assert!(matches!(
            decidir_carta(r(SituacaoRetorno::Rejeitado, "501")),
            DestinoDaCarta::Recusada(_)
        ));
        assert!(matches!(
            decidir_carta(r(SituacaoRetorno::Erro, "108")),
            DestinoDaCarta::Esperar(_)
        ));
        assert!(matches!(
            decidir_carta(Desfecho::Indisponivel("sem rede".into())),
            DestinoDaCarta::Esperar(_)
        ));
        assert_eq!(EstadoCarta::de_str("Registrada"), EstadoCarta::Registrada);
    }
}
