//! `FiscalSimulado` — o adaptador de `PortaFiscal` para testes e homologação
//! (`docs/10-modulo-fiscal.md` §2). Guarda notas em memória; nenhuma rede, nenhum I/O.

use std::sync::Mutex;

use cardeal_kernel::{Cnpj, Erro, Resultado, Uf};

use crate::chave::ChaveAcesso;
use crate::emissao::{Csc, Nfce, PedidoInutilizacao, RetornoSefaz, SituacaoRetorno};
use crate::erros::ErroFiscal;
use crate::porta::{ModeloFiscal, Nsu, PortaFiscal, ResumoDfe, StatusSefaz, XmlNota};

/// Um adaptador fiscal em memória: o teste semeia notas com [`Self::semear`] e o código sob
/// teste as descobre via [`PortaFiscal::distribuicao_dfe`]/[`PortaFiscal::baixar_xml`],
/// exatamente como falaria com o adaptador HTTP real.
#[derive(Default)]
pub struct FiscalSimulado {
    notas: Mutex<Vec<(ResumoDfe, XmlNota)>>,
    fora_do_ar: std::sync::atomic::AtomicBool,
    /// Retornos programados para as próximas emissões (o primeiro sai primeiro).
    programados: Mutex<std::collections::VecDeque<RetornoSefaz>>,
    /// O que cada chave "emitida" retorna na consulta.
    emitidas: Mutex<std::collections::HashMap<String, RetornoSefaz>>,
    /// Quantas emissões chegaram.
    pub emissoes: std::sync::atomic::AtomicUsize,
}

impl FiscalSimulado {
    /// Cria um simulador vazio.
    #[must_use]
    pub fn novo() -> Self {
        Self::default()
    }

    /// Semeia uma nota disponível para o CNPJ do resumo, com o NSU informado.
    pub fn semear(
        &self,
        cnpj_emitente: Cnpj,
        nsu: u64,
        chave_acesso: &ChaveAcesso,
        xml: impl Into<String>,
    ) {
        let resumo = ResumoDfe {
            chave_acesso: chave_acesso.clone(),
            cnpj_emitente,
            nsu: Nsu(nsu),
        };
        self.notas
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((resumo, XmlNota(xml.into())));
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

    /// Faz a API fiscal simulada parar de responder (ou voltar).
    pub fn simular_fora_do_ar(&self, fora: bool) {
        self.fora_do_ar
            .store(fora, std::sync::atomic::Ordering::SeqCst);
    }
}

impl PortaFiscal for FiscalSimulado {
    fn distribuicao_dfe(&self, _cnpj: &Cnpj, desde: Nsu) -> Resultado<Vec<ResumoDfe>> {
        let notas = self
            .notas
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(notas
            .iter()
            .filter(|(r, _)| r.nsu > desde)
            .map(|(r, _)| r.clone())
            .collect())
    }

    fn baixar_xml(&self, chave: &ChaveAcesso) -> Resultado<XmlNota> {
        let notas = self
            .notas
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        notas
            .iter()
            .find(|(r, _)| &r.chave_acesso == chave)
            .map(|(_, xml)| xml.clone())
            .ok_or_else(|| Erro::nao_encontrado("XML da nota"))
    }

    fn emitir_nfce(&self, nota: &Nfce, _csc: &Csc) -> Resultado<RetornoSefaz> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        self.emissoes
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let n = travar(&self.emitidas).len();
        let chave = ChaveAcesso::montar(
            nota.emitente.endereco.uf.codigo_ibge(),
            "2610",
            &nota.emitente.cnpj.sem_mascara(),
            "65",
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

    fn consultar_documento(
        &self,
        chave: &ChaveAcesso,
        _uf: Uf,
        xml_assinado: Option<&str>,
    ) -> Resultado<RetornoSefaz> {
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
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
        if self.fora_do_ar.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(Erro::de_dominio(&ErroFiscal::ServicoIndisponivel));
        }
        if let Some(r) = travar(&self.programados).pop_front() {
            return Ok(r);
        }
        let mut emitidas = travar(&self.emitidas);
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
            "<procEventoNFe versao=\"1.00\"><evento><infEvento Id=\"ID110111{c}01\">\
             <chNFe>{c}</chNFe><tpEvento>110111</tpEvento><detEvento><xJust>{justificativa}\
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
        })
    }

    fn inutilizar(&self, p: &PedidoInutilizacao) -> Resultado<RetornoSefaz> {
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
        })
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
        sim.semear(cnpj, 10, &chave1, "<xml1/>");
        sim.semear(cnpj, 20, &chave2, "<xml2/>");

        let resumos = sim.distribuicao_dfe(&cnpj, Nsu(10)).unwrap();
        assert_eq!(resumos.len(), 1);
        assert_eq!(resumos[0].chave_acesso, chave2);

        let xml = sim.baixar_xml(&chave1).unwrap();
        assert_eq!(xml.0, "<xml1/>");
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
