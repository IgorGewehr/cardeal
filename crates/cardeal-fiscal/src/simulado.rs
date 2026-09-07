//! `FiscalSimulado` — o adaptador de `PortaFiscal` para testes e homologação
//! (`docs/10-modulo-fiscal.md` §2). Guarda notas em memória; nenhuma rede, nenhum I/O.

use std::sync::Mutex;

use cardeal_kernel::{Cnpj, Erro, Resultado};

use crate::chave::ChaveAcesso;
use crate::porta::{Nsu, PortaFiscal, ResumoDfe, XmlNota};

/// Um adaptador fiscal em memória: o teste semeia notas com [`Self::semear`] e o código sob
/// teste as descobre via [`PortaFiscal::distribuicao_dfe`]/[`PortaFiscal::baixar_xml`],
/// exatamente como falaria com o adaptador HTTP real.
#[derive(Default)]
pub struct FiscalSimulado {
    notas: Mutex<Vec<(ResumoDfe, XmlNota)>>,
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
}
