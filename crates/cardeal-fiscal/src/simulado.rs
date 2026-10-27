//! `FiscalSimulado` — o adaptador de `PortaFiscal` para testes e homologação
//! (`docs/10-modulo-fiscal.md` §2). Guarda notas em memória; nenhuma rede, nenhum I/O.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use cardeal_kernel::{Cnpj, Erro, Resultado, Uf};

use crate::chave::ChaveAcesso;
use crate::dfe::{DocumentoDfe, Manifestacao, PaginaDfe, TipoDfe};
use crate::emissao::{
    Csc, EntradaEmContingencia, Nfce, Nfe, NotaEmContingencia, PedidoInutilizacao, RetornoSefaz,
    SituacaoRetorno,
};
use crate::erros::ErroFiscal;
use crate::porta::{ModeloFiscal, Nsu, PortaFiscal, StatusSefaz};

/// A distribuição `DFe` simulada, como a SEFAZ entrega: o resumo primeiro, a nota completa
/// depois da ciência (por um NSU novo, ou pela consulta por chave).
#[derive(Default)]
struct Distribuicao {
    documentos: Vec<DocumentoDfe>,
    /// A nota completa de cada resumo, liberada pela ciência.
    completas: HashMap<String, String>,
    /// As chaves com ciência.
    ciencias: HashSet<String>,
    /// As manifestações registradas (chave, evento).
    manifestadas: HashSet<(String, &'static str)>,
    consumo_indevido: bool,
}

/// Um adaptador fiscal em memória: o teste semeia notas com [`Self::semear`] (e resumos, com
/// [`Self::semear_resumo`]) e o código sob teste as descobre pela distribuição, exatamente como
/// falaria com o adaptador HTTP real.
#[derive(Default)]
pub struct FiscalSimulado {
    dfe: Mutex<Distribuicao>,
    /// Quantas vezes a distribuição foi consultada.
    pub consultas_dfe: std::sync::atomic::AtomicUsize,
    fora_do_ar: std::sync::atomic::AtomicBool,
    /// Retornos programados para as próximas emissões (o primeiro sai primeiro).
    programados: Mutex<std::collections::VecDeque<RetornoSefaz>>,
    /// O que cada chave "emitida" retorna na consulta.
    emitidas: Mutex<std::collections::HashMap<String, RetornoSefaz>>,
    /// Quantas emissões chegaram.
    pub emissoes: std::sync::atomic::AtomicUsize,
    /// A SEFAZ fora (a API no ar): emissão, consulta e transmissão voltam "não enviado".
    sefaz_fora: std::sync::atomic::AtomicBool,
    /// Quantas notas foram assinadas em contingência.
    pub contingencias: std::sync::atomic::AtomicUsize,
}

impl FiscalSimulado {
    /// Cria um simulador vazio.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Semeia uma nota completa na distribuição, no NSU informado.
    pub fn semear(&self, nsu: u64, chave: &ChaveAcesso, xml: impl Into<String>) {
        self.semear_documento(nsu, TipoDfe::Nota, chave, xml);
    }

    /// Semeia o resumo de uma nota (`resNFe`); a completa só sai depois da ciência.
    pub fn semear_resumo(
        &self,
        nsu: u64,
        chave: &ChaveAcesso,
        resumo: impl Into<String>,
        completa: impl Into<String>,
    ) {
        travar(&self.dfe)
            .completas
            .insert(chave.como_str().to_owned(), completa.into());
        self.semear_documento(nsu, TipoDfe::ResumoDaNota, chave, resumo);
    }

    /// Semeia um documento qualquer (um evento, por exemplo).
    pub fn semear_documento(
        &self,
        nsu: u64,
        tipo: TipoDfe,
        chave: &ChaveAcesso,
        xml: impl Into<String>,
    ) {
        travar(&self.dfe).documentos.push(DocumentoDfe {
            nsu: Nsu(nsu),
            tipo,
            chave: Some(chave.clone()),
            xml: xml.into(),
        });
    }

    /// Faz a distribuição responder 656 (consumo indevido), ou voltar ao normal.
    pub fn simular_consumo_indevido(&self, sim: bool) {
        travar(&self.dfe).consumo_indevido = sim;
    }

    /// Programa o retorno da próxima emissão (fila). Um retorno `Processando`/`Erro` com
    /// chave é lembrado como **autorizado** na consulta — o caso "a SEFAZ autorizou, mas a
    /// resposta se perdeu".
    pub fn programar(&self, retorno: RetornoSefaz) {
        travar(&self.programados).push_back(retorno);
    }

    /// O que a consulta desta chave vai responder.
    pub fn definir_consulta(&self, chave: &str, retorno: RetornoSefaz) {
        travar(&self.emitidas).insert(chave.to_string(), retorno);
    }

    /// A emissão simulada de uma NFC-e (`"65"`) ou NF-e (`"55"`).
    fn emitir_modelo(&self, nota: &Nfce, modelo: &str) -> Resultado<RetornoSefaz> {
        self.api_fora()?;
        if let Some(r) = self.sem_sefaz() {
            return Ok(r);
        }
        self.emissoes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let n = travar(&self.emitidas).len();
        let chave = ChaveAcesso::montar(
            nota.emitente.endereco.uf.codigo_ibge(),
            "2610",
            &nota.emitente.cnpj.sem_mascara(),
            modelo,
            nota.serie,
            nota.numero,
            1,
            u32::try_from(n).unwrap_or(0) + 10_000_000,
        );
        let chave = chave.como_str().to_string();
        let autorizado = RetornoSefaz {
            situacao: SituacaoRetorno::Autorizado,
            codigo: "100".into(),
            motivo: "Autorizado o uso da NF-e".into(),
            chave: Some(chave.clone()),
            protocolo: Some(format!("143{:012}", nota.numero)),
            recebido_em: Some("2026-10-14T12:00:00-03:00".into()),
            xml: Some(xml_simulado(&chave, nota)),
            xml_assinado: None,
            xml_cancelamento: None,
            erros: Vec::new(),
            nao_enviado: false,
        };
        let retorno = match travar(&self.programados).pop_front() {
            Some(mut r) => {
                if r.chave.is_none() && r.situacao != SituacaoRetorno::Rejeitado {
                    r.chave = Some(chave.clone());
                }
                // Como o `sefaz-api`: o desfecho incerto devolve a nota assinada.
                if matches!(
                    r.situacao,
                    SituacaoRetorno::Processando | SituacaoRetorno::Erro
                ) {
                    r.xml_assinado = Some(nfe_assinada(&chave, nota));
                }
                r
            }
            None => autorizado.clone(),
        };
        // Na consulta: o que foi autorizado (ou se perdeu no caminho) consta autorizado; o
        // rejeitado não consta.
        if matches!(
            retorno.situacao,
            SituacaoRetorno::Autorizado | SituacaoRetorno::Processando | SituacaoRetorno::Erro
        ) {
            travar(&self.emitidas).insert(chave, autorizado);
        } else if retorno.situacao == SituacaoRetorno::Denegado {
            travar(&self.emitidas).insert(chave, retorno.clone());
        }
        Ok(retorno)
    }

    /// O cancelamento (110111) ou, com a substituta, o por substituição (110112): a substituta
    /// precisa estar autorizada, como a SEFAZ exige.
    fn cancelar_com(
        &self,
        chave: &ChaveAcesso,
        justificativa: &str,
        substituta: Option<&ChaveAcesso>,
    ) -> Resultado<RetornoSefaz> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        if let Some(r) = self.sem_sefaz() {
            return Ok(r);
        }
        if let Some(r) = travar(&self.programados).pop_front() {
            return Ok(r);
        }
        let mut emitidas = travar(&self.emitidas);
        if let Some(sub) = substituta {
            let autorizada = emitidas
                .get(sub.como_str())
                .is_some_and(|r| r.situacao == SituacaoRetorno::Autorizado);
            if !autorizada {
                return Ok(rejeicao("SIM", "Rejeição: NFC-e substituta não autorizada"));
            }
        }
        let tipo = if substituta.is_some() {
            "110112"
        } else {
            "110111"
        };

        let Some(nota) = emitidas.get_mut(chave.como_str()) else {
            return Ok(rejeicao(
                "217",
                "Rejeição: NF-e não consta na base de dados da SEFAZ",
            ));
        };
        if nota.codigo == "101" {
            return Ok(rejeicao("573", "Rejeição: Duplicidade de evento"));
        }
        if justificativa.chars().count() < 15 {
            return Ok(rejeicao("400", "justificativa curta"));
        }
        let protocolo = "135260000000001";
        let evento = format!(
            "<procEventoNFe versao=\"1.00\"><evento><infEvento Id=\"ID{tipo}{c}01\">\
             <chNFe>{c}</chNFe><tpEvento>{tipo}</tpEvento><detEvento><xJust>{justificativa}\
             </xJust></detEvento></infEvento></evento><retEvento><infEvento><cStat>135</cStat>\
             <nProt>{protocolo}</nProt></infEvento></retEvento></procEventoNFe>",
            c = chave.como_str()
        );
        // Daqui em diante a consulta diz "cancelada" e traz o evento.
        nota.situacao = SituacaoRetorno::Cancelado;
        nota.codigo = "101".into();
        nota.motivo = "Cancelamento de NF-e homologado".into();
        nota.xml_cancelamento = Some(evento.clone());
        Ok(RetornoSefaz {
            situacao: SituacaoRetorno::Autorizado,
            codigo: "135".into(),
            motivo: "Evento registrado e vinculado a NF-e".into(),
            chave: Some(chave.como_str().to_string()),
            protocolo: Some(protocolo.into()),
            recebido_em: None,
            xml: Some(evento),
            xml_assinado: None,
            xml_cancelamento: None,
            erros: Vec::new(),
            nao_enviado: false,
        })
    }

    fn inutilizar_numeros(&self, p: &PedidoInutilizacao) -> Resultado<RetornoSefaz> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        if let Some(r) = travar(&self.programados).pop_front() {
            return Ok(r);
        }
        if p.inicio > p.fim || p.justificativa.chars().count() < 15 {
            return Ok(rejeicao("400", "faixa ou justificativa inválida"));
        }
        let protocolo = "102260000000001";
        Ok(RetornoSefaz {
            situacao: SituacaoRetorno::Autorizado,
            codigo: "102".into(),
            motivo: "Inutilização de número homologado".into(),
            chave: None,
            protocolo: Some(protocolo.into()),
            recebido_em: None,
            xml: Some(format!(
                "<procInutNFe versao=\"4.00\"><inutNFe><infInut><mod>{}</mod><serie>{}</serie>\
                 <nNFIni>{}</nNFIni><nNFFin>{}</nNFFin></infInut></inutNFe><retInutNFe><infInut>\
                 <cStat>102</cStat><nProt>{protocolo}</nProt></infInut></retInutNFe></procInutNFe>",
                p.modelo.codigo(),
                p.serie,
                p.inicio,
                p.fim
            )),
            xml_assinado: None,
            xml_cancelamento: None,
            erros: Vec::new(),
            nao_enviado: false,
        })
    }

    /// Faz a API fiscal simulada parar de responder (ou voltar).
    pub fn simular_fora_do_ar(&self, fora: bool) {
        self.fora_do_ar
            .store(fora, std::sync::atomic::Ordering::SeqCst);
    }

    /// Faz a SEFAZ ficar fora (a API continua respondendo, com "não enviado"), ou voltar.
    pub fn simular_sefaz_fora(&self, fora: bool) {
        self.sefaz_fora
            .store(fora, std::sync::atomic::Ordering::SeqCst);
    }

    fn api_fora(&self) -> Resultado<()> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        Ok(())
    }

    /// Com a SEFAZ fora, o retorno "não enviado" do `sefaz-api` (sem chave).
    fn sem_sefaz(&self) -> Option<RetornoSefaz> {
        self.sefaz_fora
            .load(std::sync::atomic::Ordering::SeqCst)
            .then(|| RetornoSefaz {
                situacao: SituacaoRetorno::Erro,
                codigo: "ERR".into(),
                motivo: "Erro de conexão com SEFAZ: connect ETIMEDOUT".into(),
                chave: None,
                protocolo: None,
                recebido_em: None,
                xml: None,
                xml_assinado: None,
                xml_cancelamento: None,
                erros: Vec::new(),
                nao_enviado: true,
            })
    }
}

impl PortaFiscal for FiscalSimulado {
    fn distribuicao_dfe(&self, _cnpj: &Cnpj, _uf: Uf, desde: Nsu) -> Resultado<PaginaDfe> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        self.consultas_dfe
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let d = travar(&self.dfe);
        if d.consumo_indevido {
            return Ok(PaginaDfe {
                consumo_indevido: true,
                ..PaginaDfe::default()
            });
        }
        let maior = d
            .documentos
            .iter()
            .map(|x| x.nsu)
            .max()
            .unwrap_or_default()
            .max(desde);
        let mut documentos: Vec<DocumentoDfe> = d
            .documentos
            .iter()
            .filter(|x| x.nsu > desde)
            .cloned()
            .collect();
        documentos.sort_by_key(|x| x.nsu);
        documentos.truncate(50);
        Ok(PaginaDfe {
            consumo_indevido: false,
            ultimo_nsu: documentos.last().map_or(maior, |x| x.nsu),
            maior_nsu: maior,
            documentos,
        })
    }

    fn consultar_dfe_por_chave(
        &self,
        _cnpj: &Cnpj,
        _uf: Uf,
        chave: &ChaveAcesso,
    ) -> Resultado<PaginaDfe> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        let d = travar(&self.dfe);
        let completa = d
            .ciencias
            .contains(chave.como_str())
            .then(|| d.completas.get(chave.como_str()))
            .flatten()
            .map(|xml| DocumentoDfe {
                nsu: Nsu(0),
                tipo: TipoDfe::Nota,
                chave: Some(chave.clone()),
                xml: xml.clone(),
            });
        let documento = completa.or_else(|| {
            d.documentos
                .iter()
                .find(|x| x.chave.as_ref() == Some(chave))
                .cloned()
        });
        Ok(PaginaDfe {
            documentos: documento.into_iter().collect(),
            ..PaginaDfe::default()
        })
    }

    fn manifestar(
        &self,
        _cnpj: &Cnpj,
        chave: &ChaveAcesso,
        tipo: Manifestacao,
        _justificativa: Option<&str>,
    ) -> Resultado<RetornoSefaz> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        if let Some(r) = travar(&self.programados).pop_front() {
            return Ok(r);
        }
        let mut d = travar(&self.dfe);
        let c = chave.como_str().to_owned();
        if !d.manifestadas.insert((c.clone(), tipo.evento())) {
            return Ok(rejeicao("573", "Rejeição: Duplicidade de evento"));
        }
        // A ciência libera a nota completa, que entra na distribuição por um NSU novo.
        if tipo == Manifestacao::Ciencia && d.ciencias.insert(c.clone()) {
            if let Some(xml) = d.completas.get(&c).cloned() {
                let nsu = d.documentos.iter().map(|x| x.nsu.0).max().unwrap_or(0) + 1;
                d.documentos.push(DocumentoDfe {
                    nsu: Nsu(nsu),
                    tipo: TipoDfe::Nota,
                    chave: Some(chave.clone()),
                    xml,
                });
            }
        }
        let protocolo = "891260000000001";
        Ok(RetornoSefaz {
            situacao: SituacaoRetorno::Autorizado,
            codigo: "135".into(),
            motivo: "Evento registrado e vinculado a NF-e".into(),
            chave: Some(c.clone()),
            protocolo: Some(protocolo.into()),
            recebido_em: None,
            xml: Some(format!(
                "<procEventoNFe versao=\"1.00\"><evento><infEvento><chNFe>{c}</chNFe>\
                 <tpEvento>{}</tpEvento></infEvento></evento><retEvento><infEvento>\
                 <cStat>135</cStat><nProt>{protocolo}</nProt></infEvento></retEvento>\
                 </procEventoNFe>",
                tipo.evento()
            )),
            xml_assinado: None,
            xml_cancelamento: None,
            erros: Vec::new(),
            nao_enviado: false,
        })
    }

    fn emitir_nfe(&self, nota: &Nfe) -> Resultado<RetornoSefaz> {
        // A NF-e é emitida como a NFC-e, com o modelo 55 na chave (o simulador não confere o
        // destinatário).
        let como_nfce = Nfce {
            emitente: nota.emitente.clone(),
            itens: nota.itens.clone(),
            pagamentos: nota.pagamentos.clone(),
            troco: cardeal_kernel::Dinheiro::ZERO,
            consumidor: None,
            serie: nota.serie,
            numero: nota.numero,
            informacoes: nota.informacoes.clone(),
        };
        self.emitir_modelo(&como_nfce, "55")
    }

    fn emitir_nfce(&self, nota: &Nfce, _csc: &Csc) -> Resultado<RetornoSefaz> {
        self.emitir_modelo(nota, "65")
    }

    fn consultar_documento(
        &self,
        chave: &ChaveAcesso,
        _uf: Uf,
        xml_assinado: Option<&str>,
    ) -> Resultado<RetornoSefaz> {
        self.api_fora()?;
        if let Some(r) = self.sem_sefaz() {
            return Ok(r);
        }
        Ok(travar(&self.emitidas)
            .get(chave.como_str())
            .cloned()
            .map_or_else(
                || RetornoSefaz {
                    situacao: SituacaoRetorno::Rejeitado,
                    codigo: "217".into(),
                    motivo: "Rejeição: NF-e não consta na base de dados da SEFAZ".into(),
                    chave: Some(chave.como_str().to_string()),
                    protocolo: None,
                    recebido_em: None,
                    xml: None,
                    xml_assinado: None,
                    xml_cancelamento: None,
                    erros: Vec::new(),
                    nao_enviado: false,
                },
                |mut r| {
                    // A consulta devolve o protocolo, não o XML da nota — salvo se recebeu a
                    // nota assinada daquela chave: aí devolve o `nfeProc` (como o sefaz-api).
                    r.xml = xml_assinado
                        .filter(|x| x.contains(&format!("NFe{}", chave.como_str())))
                        .map(|x| {
                            format!(
                                "<nfeProc versao=\"4.00\">{x}<protNFe><infProt><chNFe>{}</chNFe>\
                                 <cStat>100</cStat></infProt></protNFe></nfeProc>",
                                chave.como_str()
                            )
                        });
                    r
                },
            ))
    }

    fn cancelar(
        &self,
        chave: &ChaveAcesso,
        _protocolo: &str,
        justificativa: &str,
        _uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        self.cancelar_com(chave, justificativa, None)
    }

    fn cancelar_por_substituicao(
        &self,
        chave: &ChaveAcesso,
        _protocolo: &str,
        justificativa: &str,
        _uf: Uf,
        substituta: &ChaveAcesso,
    ) -> Resultado<RetornoSefaz> {
        self.cancelar_com(chave, justificativa, Some(substituta))
    }

    fn inutilizar(&self, p: &PedidoInutilizacao) -> Resultado<RetornoSefaz> {
        self.inutilizar_numeros(p)
    }

    fn carta_de_correcao(
        &self,
        chave: &ChaveAcesso,
        texto: &str,
        sequencia: u32,
        _uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        self.api_fora()?;
        if let Some(r) = self.sem_sefaz() {
            return Ok(r);
        }
        if let Some(r) = travar(&self.programados).pop_front() {
            return Ok(r);
        }
        let autorizada = travar(&self.emitidas)
            .get(chave.como_str())
            .is_some_and(|r| r.situacao == SituacaoRetorno::Autorizado);
        if !autorizada {
            return Ok(rejeicao(
                "217",
                "Rejeição: NF-e não consta na base de dados da SEFAZ",
            ));
        }
        let protocolo = format!("135{sequencia:012}");
        Ok(RetornoSefaz {
            situacao: SituacaoRetorno::Autorizado,
            codigo: "135".into(),
            motivo: "Evento registrado e vinculado a NF-e".into(),
            chave: Some(chave.como_str().to_string()),
            protocolo: Some(protocolo.clone()),
            recebido_em: None,
            xml: Some(format!(
                "<procEventoNFe versao=\"1.00\"><evento><infEvento Id=\"ID110110{c}{sequencia:02}\">\
                 <chNFe>{c}</chNFe><tpEvento>110110</tpEvento><nSeqEvento>{sequencia}</nSeqEvento>\
                 <detEvento><xCorrecao>{texto}</xCorrecao></detEvento></infEvento></evento>\
                 <retEvento><infEvento><cStat>135</cStat><nProt>{protocolo}</nProt></infEvento>\
                 </retEvento></procEventoNFe>",
                c = chave.como_str()
            )),
            xml_assinado: None,
            xml_cancelamento: None,
            erros: Vec::new(),
            nao_enviado: false,
        })
    }

    fn preparar_nfce_em_contingencia(
        &self,
        nota: &Nfce,
        _csc: &Csc,
        entrada: &EntradaEmContingencia,
    ) -> Resultado<NotaEmContingencia> {
        self.api_fora()?;
        let n = self
            .contingencias
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let chave = ChaveAcesso::montar(
            nota.emitente.endereco.uf.codigo_ibge(),
            "2610",
            &nota.emitente.cnpj.sem_mascara(),
            "65",
            nota.serie,
            nota.numero,
            9,
            u32::try_from(n).unwrap_or(0) + 90_000_000,
        );
        let chave = chave.como_str().to_string();
        // A nota assinada, com o `dhCont`/`xJust` no `ide` (como o sefaz-api monta).
        let xml = nfe_assinada(&chave, nota).replacen(
            "</ide>",
            &format!(
                "<tpEmis>9</tpEmis><dhCont>{}</dhCont><xJust>{}</xJust></ide>",
                entrada.inicio.formatar_iso(),
                entrada.justificativa
            ),
            1,
        );
        Ok(NotaEmContingencia { chave, xml })
    }

    fn transmitir_nfce_em_contingencia(
        &self,
        xml_assinado: &str,
        _uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        self.api_fora()?;
        let chave = xml_assinado
            .find("Id=\"NFe")
            .map(|i| xml_assinado[i + 7..].chars().take(44).collect::<String>())
            .unwrap_or_default();
        if let Some(mut r) = self.sem_sefaz() {
            r.chave = Some(chave);
            return Ok(r);
        }
        if let Some(mut r) = travar(&self.programados).pop_front() {
            if r.chave.is_none() {
                r.chave = Some(chave);
            }
            return Ok(r);
        }
        let mut emitidas = travar(&self.emitidas);
        // A mesma chave outra vez (a resposta da primeira se perdeu): duplicidade.
        if emitidas.contains_key(&chave) {
            let mut r = rejeicao("204", "Rejeição: Duplicidade de NF-e");
            r.chave = Some(chave);
            return Ok(r);
        }
        let numero: u64 = chave[25..34].parse().unwrap_or(0);
        let autorizado = RetornoSefaz {
            situacao: SituacaoRetorno::Autorizado,
            codigo: "100".into(),
            motivo: "Autorizado o uso da NF-e".into(),
            chave: Some(chave.clone()),
            protocolo: Some(format!("143{numero:012}")),
            recebido_em: Some("2026-10-14T12:00:00-03:00".into()),
            xml: Some(format!(
                "<nfeProc versao=\"4.00\">{xml_assinado}<protNFe><infProt><chNFe>{chave}</chNFe>\
                 <cStat>100</cStat></infProt></protNFe></nfeProc>"
            )),
            xml_assinado: None,
            xml_cancelamento: None,
            erros: Vec::new(),
            nao_enviado: false,
        };
        emitidas.insert(chave, autorizado.clone());
        Ok(autorizado)
    }

    fn status_servico(&self, uf: Uf, modelo: ModeloFiscal) -> Resultado<StatusSefaz> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        Ok(StatusSefaz {
            em_operacao: true,
            codigo: "107".into(),
            motivo: format!(
                "Serviço em Operação (simulado, {uf}, modelo {})",
                modelo.codigo()
            ),
        })
    }
}

fn rejeicao(codigo: &str, motivo: &str) -> RetornoSefaz {
    RetornoSefaz {
        situacao: SituacaoRetorno::Rejeitado,
        codigo: codigo.into(),
        motivo: motivo.into(),
        chave: None,
        protocolo: None,
        recebido_em: None,
        xml: None,
        xml_assinado: None,
        xml_cancelamento: None,
        erros: Vec::new(),
        nao_enviado: false,
    }
}

fn travar<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Um `nfeProc` mínimo: a nota "assinada" com o protocolo.
fn xml_simulado(chave: &str, nota: &Nfce) -> String {
    format!(
        "<nfeProc versao=\"4.00\">{}<protNFe><infProt><chNFe>{chave}</chNFe><cStat>100</cStat>\
         </infProt></protNFe></nfeProc>",
        nfe_assinada(chave, nota)
    )
}

/// A `NFe` mínima, com a emissão (agora, em Brasília) e o grupo do QR Code — o bastante para
/// guardar, imprimir e mandar ao contador em teste.
fn nfe_assinada(chave: &str, nota: &Nfce) -> String {
    let agora = cardeal_kernel::Instante::agora();
    let fuso = cardeal_kernel::Fuso::BRASILIA;
    format!(
        "<NFe><infNFe Id=\"NFe{chave}\"><ide><nNF>{}</nNF><serie>{}</serie>\
         <dhEmi>{}T{}-03:00</dhEmi></ide><total><ICMSTot><vNF>{}</vNF></ICMSTot></total></infNFe><infNFeSupl><qrCode>\
         https://homologacao.simulado/nfce?p={chave}|3|2</qrCode><urlChave>https://homologacao.simulado/nfce</urlChave>\
         </infNFeSupl></NFe>",
        nota.numero,
        nota.serie,
        agora.data(fuso).formatar_iso(),
        agora.hora(fuso).formatar_com_segundos(),
        {
            let c = nota.total().em_centavos();
            format!("{}.{:02}", c / 100, c % 100)
        }
    )
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn descobre_so_notas_depois_do_nsu_informado() {
        let sim = FiscalSimulado::novo();
        let cnpj = Cnpj::novo("11.222.333/0001-81").unwrap();
        let chave1 = ChaveAcesso::nova(&"1".repeat(44)).unwrap();
        let chave2 = ChaveAcesso::nova(&"2".repeat(44)).unwrap();
        sim.semear(10, &chave1, "<xml1/>");
        sim.semear(20, &chave2, "<xml2/>");

        let p = sim.distribuicao_dfe(&cnpj, Uf::Rs, Nsu(10)).unwrap();
        assert_eq!(p.documentos.len(), 1);
        assert_eq!(p.documentos[0].chave.as_ref(), Some(&chave2));
        assert_eq!((p.ultimo_nsu, p.maior_nsu), (Nsu(20), Nsu(20)));
        // Em dia: nada novo, o NSU fica onde está.
        let vazia = sim.distribuicao_dfe(&cnpj, Uf::Rs, Nsu(20)).unwrap();
        assert!(vazia.documentos.is_empty());
        assert_eq!(vazia.ultimo_nsu, Nsu(20));
        sim.simular_consumo_indevido(true);
        assert!(
            sim.distribuicao_dfe(&cnpj, Uf::Rs, Nsu(20))
                .unwrap()
                .consumo_indevido
        );
    }

    #[test]
    fn o_resumo_vira_nota_depois_da_ciencia() {
        let sim = FiscalSimulado::novo();
        let cnpj = Cnpj::novo("11.222.333/0001-81").unwrap();
        let chave = ChaveAcesso::nova(&"3".repeat(44)).unwrap();
        sim.semear_resumo(5, &chave, "<resNFe/>", "<nfeProc/>");
        let p = sim.distribuicao_dfe(&cnpj, Uf::Rs, Nsu(0)).unwrap();
        assert_eq!(p.documentos[0].tipo, TipoDfe::ResumoDaNota);
        // Sem ciência, a consulta por chave só tem o resumo.
        let antes = sim.consultar_dfe_por_chave(&cnpj, Uf::Rs, &chave).unwrap();
        assert_eq!(antes.documentos[0].tipo, TipoDfe::ResumoDaNota);
        let r = sim
            .manifestar(&cnpj, &chave, Manifestacao::Ciencia, None)
            .unwrap();
        assert_eq!(r.codigo, "135");
        assert!(r.xml.unwrap().contains("210210"));
        assert_eq!(
            sim.manifestar(&cnpj, &chave, Manifestacao::Ciencia, None)
                .unwrap()
                .codigo,
            "573"
        );
        let depois = sim.consultar_dfe_por_chave(&cnpj, Uf::Rs, &chave).unwrap();
        assert_eq!(depois.documentos[0].tipo, TipoDfe::Nota);
        // E a completa entra na distribuição por um NSU novo.
        let p = sim.distribuicao_dfe(&cnpj, Uf::Rs, Nsu(5)).unwrap();
        assert_eq!(p.documentos.len(), 1);
        assert_eq!(
            (p.documentos[0].nsu, p.documentos[0].tipo),
            (Nsu(6), TipoDfe::Nota)
        );
    }

    fn retorno(situacao: SituacaoRetorno, codigo: &str) -> RetornoSefaz {
        RetornoSefaz {
            situacao,
            codigo: codigo.into(),
            motivo: format!("simulado {codigo}"),
            chave: None,
            protocolo: None,
            recebido_em: None,
            xml: None,
            xml_assinado: None,
            xml_cancelamento: None,
            erros: vec![],
            nao_enviado: false,
        }
    }

    #[test]
    fn emite_programa_rejeicao_e_lembra_o_que_se_perdeu() {
        let sim = FiscalSimulado::novo();
        let nota = crate::emissao::exemplo();
        let csc = Csc {
            id: "1".into(),
            token: cardeal_storage::Segredo::novo("t".into()),
        };
        let ok = sim.emitir_nfce(&nota, &csc).unwrap();
        assert_eq!(ok.situacao, SituacaoRetorno::Autorizado);
        let chave = ChaveAcesso::nova(ok.chave.as_deref().unwrap()).unwrap();
        assert!(chave.dv_confere());
        assert_eq!(chave.numero(), 42);
        assert!(ok.xml.as_deref().unwrap().contains("<qrCode>"));
        assert_eq!(
            sim.consultar_documento(&chave, Uf::Rs, None)
                .unwrap()
                .situacao,
            SituacaoRetorno::Autorizado
        );

        // Rejeição programada.
        sim.programar(retorno(SituacaoRetorno::Rejeitado, "778"));
        assert_eq!(sim.emitir_nfce(&nota, &csc).unwrap().codigo, "778");

        // "Processando": a resposta se perdeu (com a nota assinada), mas a consulta acha
        // autorizada — sem o XML, ou com ele, se recebe a nota assinada.
        sim.programar(retorno(SituacaoRetorno::Processando, "TIMEOUT"));
        let perdida = sim.emitir_nfce(&nota, &csc).unwrap();
        let assinada = perdida.xml_assinado.clone().unwrap();
        let chave = ChaveAcesso::nova(perdida.chave.as_deref().unwrap()).unwrap();
        let consulta = sim.consultar_documento(&chave, Uf::Rs, None).unwrap();
        assert_eq!(consulta.situacao, SituacaoRetorno::Autorizado);
        assert!(consulta.xml.is_none());
        let com_xml = sim
            .consultar_documento(&chave, Uf::Rs, Some(&assinada))
            .unwrap();
        let nfe_proc = com_xml.xml.unwrap();
        assert!(nfe_proc.starts_with("<nfeProc") && nfe_proc.contains("<dhEmi>"));
        let desconhecida = ChaveAcesso::nova(&"4".repeat(44)).unwrap();
        assert_eq!(
            sim.consultar_documento(&desconhecida, Uf::Rs, Some(&assinada))
                .unwrap()
                .codigo,
            "217"
        );
        assert_eq!(sim.emissoes.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn cancelamento_e_inutilizacao_devolvem_o_xml_do_evento() {
        let sim = FiscalSimulado::novo();
        let nota = crate::emissao::exemplo();
        let csc = Csc {
            id: "1".into(),
            token: cardeal_storage::Segredo::novo("t".into()),
        };
        let ok = sim.emitir_nfce(&nota, &csc).unwrap();
        assert!(ok.xml.as_deref().unwrap().contains("<dhEmi>"));
        let chave = ChaveAcesso::nova(ok.chave.as_deref().unwrap()).unwrap();
        let c = sim
            .cancelar(&chave, "1", "Venda registrada em duplicidade", Uf::Rs)
            .unwrap();
        let evento = c.xml.unwrap();
        assert!(evento.starts_with("<procEventoNFe") && evento.contains(chave.como_str()));
        // A consulta da cancelada diz "cancelada" e traz o mesmo evento.
        let consulta = sim.consultar_documento(&chave, Uf::Rs, None).unwrap();
        assert_eq!(consulta.situacao, SituacaoRetorno::Cancelado);
        assert_eq!(consulta.codigo, "101");
        assert_eq!(consulta.xml_cancelamento.as_deref(), Some(evento.as_str()));
        let i = sim
            .inutilizar(&PedidoInutilizacao {
                cnpj: Cnpj::novo("11.222.333/0001-81").unwrap(),
                modelo: ModeloFiscal::Nfce,
                serie: 1,
                inicio: 7,
                fim: 7,
                ano: 26,
                justificativa: "Número pulado por falha do sistema".into(),
                uf: Uf::Rs,
            })
            .unwrap();
        assert!(i.xml.unwrap().contains("<nNFIni>7</nNFIni>"));
    }

    #[test]
    fn status_em_operacao_e_fora_do_ar_sob_comando() {
        let sim = FiscalSimulado::novo();
        let status = sim.status_servico(Uf::Sc, ModeloFiscal::Nfce).unwrap();
        assert!(status.em_operacao);
        assert_eq!(status.codigo, "107");
        sim.simular_fora_do_ar(true);
        assert!(sim.status_servico(Uf::Sc, ModeloFiscal::Nfce).is_err());
    }
}
