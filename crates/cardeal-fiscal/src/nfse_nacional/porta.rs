//! [`PortaComNfseNacional`]: a porta fiscal da empresa com a NFS-e do padrão nacional emitida
//! direto na Sefin; todo o resto (NF-e, NFC-e, DF-e, e a NFS-e dos municípios fora do padrão
//! nacional) segue para a porta de base (o `sefaz-api`). Sem a chave do `sefaz-api`, a NFS-e
//! nacional funciona mesmo assim — o resto responde o erro da base.

use cardeal_kernel::{Cnpj, Erro, Instante, Resultado, Uf};
use rand::Rng as _;

use super::{emite_pela_sefin_nacional, NfseNacional};
use crate::chave::ChaveAcesso;
use crate::dfe::{Manifestacao, PaginaDfe};
use crate::emissao::{
    Csc, EntradaEmContingencia, Nfce, Nfe, NotaEmContingencia, PedidoInutilizacao, RetornoSefaz,
};
use crate::nfce_offline::montar_nfce_em_contingencia;
use crate::porta::{ModeloFiscal, Nsu, PortaFiscal, StatusSefaz};
use crate::{CancelamentoNfse, Emitente, Nfse, RetornoNfse};

/// A porta com a NFS-e nacional direta.
pub struct PortaComNfseNacional {
    base: Result<Box<dyn PortaFiscal>, Erro>,
    nacional: NfseNacional,
}

impl std::fmt::Debug for PortaComNfseNacional {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PortaComNfseNacional")
            .field("nacional", &self.nacional)
            .finish_non_exhaustive()
    }
}

impl PortaComNfseNacional {
    /// A base (ou o erro dela, quando não há `sefaz-api`) e a emissão nacional.
    #[must_use]
    pub fn nova(base: Result<Box<dyn PortaFiscal>, Erro>, nacional: NfseNacional) -> Self {
        Self { base, nacional }
    }

    fn base(&self) -> Resultado<&dyn PortaFiscal> {
        self.base.as_ref().map(AsRef::as_ref).map_err(Clone::clone)
    }
}

impl PortaFiscal for PortaComNfseNacional {
    fn distribuicao_dfe(&self, cnpj: &Cnpj, uf: Uf, desde: Nsu) -> Resultado<PaginaDfe> {
        self.base()?.distribuicao_dfe(cnpj, uf, desde)
    }

    fn consultar_dfe_por_chave(
        &self,
        cnpj: &Cnpj,
        uf: Uf,
        chave: &ChaveAcesso,
    ) -> Resultado<PaginaDfe> {
        self.base()?.consultar_dfe_por_chave(cnpj, uf, chave)
    }

    fn manifestar(
        &self,
        cnpj: &Cnpj,
        chave: &ChaveAcesso,
        tipo: Manifestacao,
        justificativa: Option<&str>,
    ) -> Resultado<RetornoSefaz> {
        self.base()?.manifestar(cnpj, chave, tipo, justificativa)
    }

    fn status_servico(&self, uf: Uf, modelo: ModeloFiscal) -> Resultado<StatusSefaz> {
        self.base()?.status_servico(uf, modelo)
    }

    fn emitir_nfe(&self, nota: &Nfe) -> Resultado<RetornoSefaz> {
        self.base()?.emitir_nfe(nota)
    }

    fn emitir_nfce(&self, nota: &Nfce, csc: &Csc) -> Resultado<RetornoSefaz> {
        self.base()?.emitir_nfce(nota, csc)
    }

    fn consultar_documento(
        &self,
        chave: &ChaveAcesso,
        uf: Uf,
        xml_assinado: Option<&str>,
    ) -> Resultado<RetornoSefaz> {
        self.base()?.consultar_documento(chave, uf, xml_assinado)
    }

    fn cancelar(
        &self,
        chave: &ChaveAcesso,
        protocolo: &str,
        justificativa: &str,
        uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        self.base()?.cancelar(chave, protocolo, justificativa, uf)
    }

    fn inutilizar(&self, pedido: &PedidoInutilizacao) -> Resultado<RetornoSefaz> {
        self.base()?.inutilizar(pedido)
    }

    fn emitir_nfse(&self, nota: &Nfse) -> Resultado<RetornoNfse> {
        if emite_pela_sefin_nacional(&nota.prestador.endereco.codigo_municipio) {
            self.nacional.emitir(nota)
        } else {
            self.base()?.emitir_nfse(nota)
        }
    }

    fn consultar_nfse_por_dps(
        &self,
        prestador: &Emitente,
        serie: u32,
        numero: u64,
    ) -> Resultado<RetornoNfse> {
        if emite_pela_sefin_nacional(&prestador.endereco.codigo_municipio) {
            self.nacional.consultar_por_dps(prestador, serie, numero)
        } else {
            self.base()?
                .consultar_nfse_por_dps(prestador, serie, numero)
        }
    }

    fn cancelar_nfse(
        &self,
        prestador: &Emitente,
        cancelamento: &CancelamentoNfse,
    ) -> Resultado<RetornoNfse> {
        if emite_pela_sefin_nacional(&prestador.endereco.codigo_municipio) {
            self.nacional.cancelar(prestador, cancelamento)
        } else {
            self.base()?.cancelar_nfse(prestador, cancelamento)
        }
    }

    fn carta_de_correcao(
        &self,
        chave: &ChaveAcesso,
        texto: &str,
        sequencia: u32,
        uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        self.base()?.carta_de_correcao(chave, texto, sequencia, uf)
    }

    fn cancelar_por_substituicao(
        &self,
        chave: &ChaveAcesso,
        protocolo: &str,
        justificativa: &str,
        uf: Uf,
        substituta: &ChaveAcesso,
    ) -> Resultado<RetornoSefaz> {
        self.base()?
            .cancelar_por_substituicao(chave, protocolo, justificativa, uf, substituta)
    }

    /// A NFC-e em contingência é montada e assinada **aqui** (ADR-0021) — sem depender de rede:
    /// é o caso do caixa sem internet. Só sem o responsável técnico, ou com o que a montagem
    /// local não cobre, ela vai ao `sefaz-api` (o `/contingencia/prepare`, o mesmo XML).
    fn preparar_nfce_em_contingencia(
        &self,
        nota: &Nfce,
        csc: &Csc,
        entrada: &EntradaEmContingencia,
    ) -> Resultado<NotaEmContingencia> {
        let local = match &entrada.responsavel_tecnico {
            Some(responsavel) => self.nacional.chave_de_assinatura().and_then(|chave| {
                montar_nfce_em_contingencia(
                    nota,
                    entrada,
                    self.nacional.ambiente(),
                    responsavel,
                    chave,
                    Instante::agora(),
                    rand::thread_rng().gen_range(10_000_000..100_000_000),
                )
                .map_err(|e| Erro::de_dominio(&e))
            }),
            None => Err(Erro::de_dominio(&crate::ErroFiscal::ApiFiscalRecusou(
                "sem o responsável técnico (nenhuma nota autorizada ainda, nem CARDEAL_RESP_TEC_*)"
                    .into(),
            ))),
        };
        match local {
            Ok(n) => Ok(n),
            Err(motivo) => {
                tracing::warn!(motivo = %motivo.mensagem, "contingência local não saiu; pedindo ao sefaz-api");
                self.base()
                    .map_err(|_| motivo.clone())?
                    .preparar_nfce_em_contingencia(nota, csc, entrada)
                    .map_err(|e| {
                        Erro::novo(
                            e.codigo,
                            format!("{} (e a local: {})", e.mensagem, motivo.mensagem),
                        )
                    })
            }
        }
    }

    fn transmitir_nfce_em_contingencia(
        &self,
        xml_assinado: &str,
        uf: Uf,
    ) -> Resultado<RetornoSefaz> {
        self.base()?
            .transmitir_nfce_em_contingencia(xml_assinado, uf)
    }
}
