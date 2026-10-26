//! O fechamento do mês para o contador: as notas emitidas no mês, cada uma com os XMLs que a
//! lei manda guardar por 5 anos — o da nota (`nfeProc`) e o do evento (`procEventoNFe` do
//! cancelamento, `procInutNFe` da inutilização) —, e o que ainda não teve desfecho: um número
//! sem desfecho é um buraco na série, e o contador pergunta por ele.
//!
//! Lição do `gestao-raiz` (`accounting-export.service.ts`): lá o mês é o da criação da nota, o
//! XML do cancelamento é guardado mas não vai no pacote, a inutilizada fica de fora e o "baixa
//! da SEFAZ se faltar" lê um campo que a consulta do `sefaz-api` nunca devolve. Aqui:
//! - o mês da nota é o da **emissão** (`dhEmi` do XML; sem XML, o da autorização), e o da
//!   inutilizada é o da inutilização;
//! - canceladas levam a nota e o evento; inutilizadas, o evento;
//! - só **produção** — nota de homologação não tem valor fiscal e não vai ao contador;
//! - o que falta guardar é dito, nota a nota, com o porquê — nada se baixa às escondidas.

use cardeal_fiscal::ModeloFiscal;
#[cfg(feature = "sqlite")]
use cardeal_kernel::Resultado;
use cardeal_kernel::{Competencia, Data, Dinheiro, Fuso, Instante};
use cardeal_modkit::Consulta;
#[cfg(feature = "sqlite")]
use cardeal_modkit::Ctx;
#[cfg(feature = "sqlite")]
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::documento::{DocumentoResumo, EstadoDocumento};

/// Como a nota fechou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SituacaoNoMes {
    /// Autorizada e valendo.
    Autorizada,
    /// Autorizada e depois cancelada.
    Cancelada,
    /// O número foi inutilizado.
    Inutilizada,
    /// Denegada pela SEFAZ (irregularidade cadastral) — o número fica usado.
    Denegada,
}

impl SituacaoNoMes {
    /// Para a planilha e o resumo.
    #[must_use]
    pub const fn nome(self) -> &'static str {
        match self {
            Self::Autorizada => "Autorizada",
            Self::Cancelada => "Cancelada",
            Self::Inutilizada => "Inutilizada",
            Self::Denegada => "Denegada",
        }
    }

    const fn pasta(self) -> &'static str {
        match self {
            Self::Autorizada => "Autorizadas",
            Self::Cancelada => "Canceladas",
            Self::Inutilizada => "Inutilizadas",
            Self::Denegada => "Denegadas",
        }
    }

    /// A situação final de um documento — `None` se ele ainda não teve desfecho.
    #[must_use]
    pub const fn de(estado: EstadoDocumento) -> Option<Self> {
        match estado {
            EstadoDocumento::Autorizada => Some(Self::Autorizada),
            EstadoDocumento::Cancelada => Some(Self::Cancelada),
            EstadoDocumento::Inutilizada => Some(Self::Inutilizada),
            EstadoDocumento::Denegada => Some(Self::Denegada),
            EstadoDocumento::Pendente | EstadoDocumento::Incerta | EstadoDocumento::Rejeitada => {
                None
            }
        }
    }
}

/// Por que um XML não está no pacote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum XmlQueFalta {
    /// A nota foi confirmada pela consulta à SEFAZ (a resposta da emissão se perdeu) e a nota
    /// assinada não ficou guardada — sem ela, a consulta não tem como devolver o XML.
    NotaConfirmadaPelaConsulta,
    /// A nota foi confirmada pela consulta e a nota assinada ficou guardada: a busca na SEFAZ
    /// devolve o XML.
    NotaABuscar,
    /// O XML do cancelamento não veio (provedor antigo, ou a resposta se perdeu): a consulta
    /// da nota cancelada o traz.
    EventoDeCancelamento,
    /// O XML da inutilização não veio — e não há consulta que o devolva.
    EventoDeInutilizacao,
}

impl XmlQueFalta {
    /// O que falta, para o resumo do pacote.
    #[must_use]
    pub const fn explicar(self) -> &'static str {
        match self {
            Self::NotaConfirmadaPelaConsulta => {
                "XML de nota confirmada pela consulta à SEFAZ — a resposta da emissão se perdeu e \
                 a nota assinada não ficou guardada"
            }
            Self::NotaABuscar => {
                "XML de nota confirmada pela consulta à SEFAZ — o Cardeal busca na SEFAZ"
            }
            Self::EventoDeCancelamento => "XML do cancelamento — não veio na resposta da SEFAZ",
            Self::EventoDeInutilizacao => "XML da inutilização — não veio na resposta da SEFAZ",
        }
    }

    /// O que fazer.
    #[must_use]
    pub const fn o_que_fazer(self) -> &'static str {
        match self {
            Self::NotaConfirmadaPelaConsulta => {
                "Baixe o XML no portal da SEFAZ, com o certificado, e mande junto."
            }
            Self::NotaABuscar => "Gere o pacote de novo depois que a fila fiscal transmitir.",
            Self::EventoDeCancelamento => {
                "O Cardeal busca na consulta da nota: gere o pacote de novo depois que a fila \
                 fiscal transmitir. Se continuar faltando, a planilha traz o protocolo, que \
                 comprova o cancelamento."
            }
            Self::EventoDeInutilizacao => {
                "A planilha traz o protocolo de cada uma, que comprova a inutilização na SEFAZ."
            }
        }
    }

    /// Para a coluna da planilha.
    #[must_use]
    pub const fn curto(self) -> &'static str {
        match self {
            Self::NotaConfirmadaPelaConsulta => "falta o XML da nota",
            Self::NotaABuscar => "XML da nota: buscando na SEFAZ",
            Self::EventoDeCancelamento => "falta o XML do cancelamento",
            Self::EventoDeInutilizacao => "falta o XML da inutilização",
        }
    }
}

/// Uma nota do mês.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotaDoMes {
    /// O modelo.
    pub modelo: ModeloFiscal,
    /// A série.
    pub serie: u32,
    /// O número.
    pub numero: u64,
    /// Como fechou.
    pub situacao: SituacaoNoMes,
    /// O dia da emissão (o da inutilização, para a inutilizada).
    pub data: Data,
    /// A chave de acesso.
    pub chave: Option<String>,
    /// O protocolo de autorização.
    pub protocolo: Option<String>,
    /// O protocolo do cancelamento ou da inutilização.
    pub protocolo_evento: Option<String>,
    /// O total da nota.
    pub total: Dinheiro,
    /// O XML da nota (`nfeProc`).
    pub xml: Option<String>,
    /// O XML do evento (`procEventoNFe`, `procInutNFe`).
    pub xml_evento: Option<String>,
    /// Sem o `nfeProc`, mas com a nota assinada guardada: a busca na SEFAZ o recupera.
    pub nota_assinada_guardada: bool,
}

impl NotaDoMes {
    fn pasta(&self) -> String {
        let modelo = match self.modelo {
            ModeloFiscal::Nfce => "NFC-e",
            ModeloFiscal::Nfe => "NF-e",
        };
        format!("Saidas/{modelo}/{}", self.situacao.pasta())
    }

    /// Os arquivos desta nota no pacote: (caminho no ZIP, conteúdo). Os nomes são os que os
    /// sistemas de contabilidade conhecem (os do ACBr): `<chave>-nfe.xml`,
    /// `110111<chave>01-procEventoNFe.xml`; a inutilização, pela série e o número.
    #[must_use]
    pub fn arquivos(&self) -> Vec<(String, &str)> {
        let pasta = self.pasta();
        let nome = self
            .chave
            .clone()
            .unwrap_or_else(|| format!("serie{:03}-numero{:09}", self.serie, self.numero));
        let mut v = Vec::new();
        if let Some(xml) = &self.xml {
            v.push((format!("{pasta}/{nome}-nfe.xml"), xml.as_str()));
        }
        if let Some(evento) = &self.xml_evento {
            let arquivo = match self.situacao {
                SituacaoNoMes::Inutilizada => format!(
                    "serie{:03}-numero{:09}-procInutNFe.xml",
                    self.serie, self.numero
                ),
                _ => format!("110111{nome}01-procEventoNFe.xml"),
            };
            v.push((format!("{pasta}/{arquivo}"), evento.as_str()));
        }
        v
    }

    /// Os XMLs desta nota que não estão guardados.
    #[must_use]
    pub fn faltas(&self) -> Vec<XmlQueFalta> {
        let mut v = Vec::new();
        let sem_nota = self.xml.is_none();
        let nota = if self.nota_assinada_guardada {
            XmlQueFalta::NotaABuscar
        } else {
            XmlQueFalta::NotaConfirmadaPelaConsulta
        };
        match self.situacao {
            SituacaoNoMes::Autorizada | SituacaoNoMes::Denegada if sem_nota => v.push(nota),
            SituacaoNoMes::Cancelada => {
                if sem_nota {
                    v.push(nota);
                }
                if self.xml_evento.is_none() {
                    v.push(XmlQueFalta::EventoDeCancelamento);
                }
            }
            SituacaoNoMes::Inutilizada if self.xml_evento.is_none() => {
                v.push(XmlQueFalta::EventoDeInutilizacao);
            }
            _ => {}
        }
        v
    }
}

/// As notas emitidas num mês, para o contador.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NotasDoMes {
    /// As notas que fecharam no mês (produção), por modelo, série e número.
    pub notas: Vec<NotaDoMes>,
    /// As notas de produção criadas até o fim do mês que ainda não tiveram desfecho
    /// (pendentes, incertas, rejeitadas): cada uma precisa ser autorizada ou inutilizada.
    pub sem_desfecho: Vec<DocumentoResumo>,
    /// Quantas notas de homologação (teste, sem valor fiscal) o mês teve — ficam de fora.
    pub de_homologacao: u32,
}

impl NotasDoMes {
    /// Quantas notas fecharam na situação.
    #[must_use]
    pub fn quantas(&self, situacao: SituacaoNoMes) -> usize {
        self.notas.iter().filter(|n| n.situacao == situacao).count()
    }

    /// A soma das autorizadas que valem (sem as canceladas).
    #[must_use]
    pub fn total_autorizado(&self) -> Dinheiro {
        self.notas
            .iter()
            .filter(|n| n.situacao == SituacaoNoMes::Autorizada)
            .map(|n| n.total)
            .sum()
    }

    /// Quantos XMLs faltam, por motivo.
    #[must_use]
    pub fn faltas(&self) -> Vec<(XmlQueFalta, usize)> {
        let mut contagem: std::collections::BTreeMap<XmlQueFalta, usize> =
            std::collections::BTreeMap::new();
        for f in self.notas.iter().flat_map(NotaDoMes::faltas) {
            *contagem.entry(f).or_default() += 1;
        }
        contagem.into_iter().collect()
    }
}

/// O dia da emissão: o `dhEmi` do XML (a data como a nota diz, no fuso de quem emitiu) e, sem
/// ele, o dia de `quando` (a autorização) no fuso da empresa.
#[must_use]
pub fn dia_da_emissao(xml: Option<&str>, quando: Instante, fuso: Fuso) -> Data {
    xml.and_then(|x| {
        let i = x.find("<dhEmi>")? + "<dhEmi>".len();
        let iso = x.get(i..i + 10)?;
        let n = |a: usize, b: usize| iso.get(a..b)?.parse::<u32>().ok();
        let ano = i32::try_from(n(0, 4)?).ok()?;
        Data::de_ymd(ano, n(5, 7)?, n(8, 10)?).ok()
    })
    .unwrap_or_else(|| quando.data(fuso))
}

/// As notas de um mês para o contador.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct NotasFiscaisDoMes {
    /// O mês.
    pub mes: Competencia,
}

impl Consulta for NotasFiscaisDoMes {
    type Saida = NotasDoMes;
    const PERMISSAO: &'static str = "fiscal.ver";

    #[cfg(feature = "sqlite")]
    fn executar(self, ctx: &Ctx, c: &Connection) -> Resultado<NotasDoMes> {
        use cardeal_kernel::Hora;
        let meia_noite = Hora::de_hms(0, 0, 0)?;
        let instante = |d: Data| Instante::de_data_hora(d, meia_noite, ctx.fuso);
        let (inicio, fim) = (
            instante(self.mes.primeiro_dia()),
            instante(self.mes.proxima().primeiro_dia()),
        );
        // Um dia de folga dos dois lados: a emissão (`dhEmi`) e a autorização caem em dias
        // diferentes na virada da meia-noite; quem decide é o dia da emissão.
        let folga = 24 * 60 * 60;
        let linhas = crate::sql_documento::fechados_entre(
            c,
            ctx.empresa,
            inicio.mais_segundos(-folga),
            fim.mais_segundos(folga),
        )?;
        let mut notas: Vec<NotaDoMes> = linhas
            .into_iter()
            .filter_map(|l| {
                let situacao = SituacaoNoMes::de(l.resumo.estado)?;
                let data = if situacao == SituacaoNoMes::Inutilizada {
                    l.evento_em.data(ctx.fuso)
                } else {
                    dia_da_emissao(
                        l.xml.as_deref(),
                        l.resumo.autorizado_em.unwrap_or(l.resumo.criado_em),
                        ctx.fuso,
                    )
                };
                (data.competencia() == self.mes).then_some(NotaDoMes {
                    modelo: l.resumo.modelo,
                    serie: l.resumo.serie,
                    numero: l.resumo.numero,
                    situacao,
                    data,
                    chave: l.resumo.chave,
                    protocolo: l.resumo.protocolo,
                    protocolo_evento: l.protocolo_evento,
                    total: l.resumo.total,
                    xml: l.xml,
                    xml_evento: l.xml_evento,
                    nota_assinada_guardada: l.xml_assinado_guardado,
                })
            })
            .collect();
        notas.sort_by_key(|n| (n.modelo.codigo(), n.serie, n.numero));
        Ok(NotasDoMes {
            notas,
            sem_desfecho: crate::sql_documento::sem_desfecho_ate(c, ctx.empresa, fim)?,
            de_homologacao: crate::sql_documento::de_homologacao_entre(
                c,
                ctx.empresa,
                inicio,
                fim,
            )?,
        })
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const CHAVE: &str = "43261011222333000181650010000000421100000001";

    fn nota(situacao: SituacaoNoMes, xml: bool, evento: bool) -> NotaDoMes {
        NotaDoMes {
            modelo: ModeloFiscal::Nfce,
            serie: 1,
            numero: 42,
            situacao,
            data: Data::de_ymd(2026, 10, 31).unwrap(),
            chave: (situacao != SituacaoNoMes::Inutilizada).then(|| CHAVE.to_owned()),
            protocolo: None,
            protocolo_evento: None,
            total: Dinheiro::reais(10),
            xml: xml.then(|| "<nfeProc/>".to_owned()),
            xml_evento: evento.then(|| "<evento/>".to_owned()),
            nota_assinada_guardada: false,
        }
    }

    #[test]
    fn o_dia_e_o_da_emissao_que_o_xml_diz() {
        let fuso = Fuso::BRASILIA;
        // Emitida às 23:59:58 do dia 31, autorizada já no dia 1º: vale o dia 31.
        let autorizada = Instante::de_data_hora(
            Data::de_ymd(2026, 11, 1).unwrap(),
            cardeal_kernel::Hora::de_hms(0, 0, 3).unwrap(),
            fuso,
        );
        let xml = "<nfeProc><ide><dhEmi>2026-10-31T23:59:58-03:00</dhEmi></ide></nfeProc>";
        assert_eq!(
            dia_da_emissao(Some(xml), autorizada, fuso),
            Data::de_ymd(2026, 10, 31).unwrap()
        );
        // Sem XML (ou sem dhEmi): o dia da autorização, no fuso da empresa.
        let esperado = Data::de_ymd(2026, 11, 1).unwrap();
        assert_eq!(dia_da_emissao(None, autorizada, fuso), esperado);
        assert_eq!(
            dia_da_emissao(Some("<nfeProc/>"), autorizada, fuso),
            esperado
        );
        assert_eq!(
            dia_da_emissao(Some("<dhEmi>lixo</dhEmi>"), autorizada, fuso),
            esperado
        );
    }

    #[test]
    fn arquivos_com_os_nomes_que_a_contabilidade_conhece() {
        let a = nota(SituacaoNoMes::Autorizada, true, false);
        assert_eq!(
            a.arquivos(),
            vec![(
                format!("Saidas/NFC-e/Autorizadas/{CHAVE}-nfe.xml"),
                "<nfeProc/>"
            )]
        );
        assert!(a.faltas().is_empty());

        let c = nota(SituacaoNoMes::Cancelada, true, true);
        let nomes: Vec<String> = c.arquivos().into_iter().map(|(n, _)| n).collect();
        assert_eq!(
            nomes,
            vec![
                format!("Saidas/NFC-e/Canceladas/{CHAVE}-nfe.xml"),
                format!("Saidas/NFC-e/Canceladas/110111{CHAVE}01-procEventoNFe.xml"),
            ]
        );

        let i = nota(SituacaoNoMes::Inutilizada, false, true);
        assert_eq!(
            i.arquivos()[0].0,
            "Saidas/NFC-e/Inutilizadas/serie001-numero000000042-procInutNFe.xml"
        );
        assert!(i.faltas().is_empty());
    }

    #[test]
    fn o_que_falta_guardar_e_dito_com_o_porque() {
        use XmlQueFalta::{
            EventoDeCancelamento, EventoDeInutilizacao, NotaABuscar, NotaConfirmadaPelaConsulta,
        };
        assert_eq!(
            nota(SituacaoNoMes::Autorizada, false, false).faltas(),
            vec![NotaConfirmadaPelaConsulta]
        );
        // Com a nota assinada guardada, a busca recupera o XML.
        let a_buscar = NotaDoMes {
            nota_assinada_guardada: true,
            ..nota(SituacaoNoMes::Cancelada, false, false)
        };
        assert_eq!(a_buscar.faltas(), vec![NotaABuscar, EventoDeCancelamento]);
        // Nenhum texto sai com o recuo do código no meio.
        for f in [
            NotaConfirmadaPelaConsulta,
            NotaABuscar,
            EventoDeCancelamento,
            EventoDeInutilizacao,
        ] {
            for t in [f.explicar(), f.o_que_fazer(), f.curto()] {
                assert!(!t.contains("  "), "{t}");
            }
        }
        assert_eq!(
            nota(SituacaoNoMes::Cancelada, true, false).faltas(),
            vec![EventoDeCancelamento]
        );
        assert_eq!(
            nota(SituacaoNoMes::Inutilizada, false, false).faltas(),
            vec![EventoDeInutilizacao]
        );
        let mes = NotasDoMes {
            notas: vec![
                nota(SituacaoNoMes::Autorizada, true, false),
                nota(SituacaoNoMes::Cancelada, true, false),
                nota(SituacaoNoMes::Cancelada, false, false),
            ],
            ..NotasDoMes::default()
        };
        assert_eq!(
            mes.faltas(),
            vec![(NotaConfirmadaPelaConsulta, 1), (EventoDeCancelamento, 2)]
        );
        assert_eq!(mes.quantas(SituacaoNoMes::Cancelada), 2);
        // A cancelada não soma.
        assert_eq!(mes.total_autorizado(), Dinheiro::reais(10));
        assert!(EventoDeCancelamento.o_que_fazer().contains("protocolo"));
        assert!(EventoDeCancelamento.o_que_fazer().contains("busca"));
    }
}
