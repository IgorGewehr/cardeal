//! A **NFS-e do padrão nacional emitida pelo próprio Cardeal** (ADR-0020): a DPS montada no
//! leiaute 1.01, assinada com o A1 da empresa (`XMLDSig`) e enviada à Sefin Nacional por TLS
//! mútuo — sem intermediário. O `sefaz-api` continua com a NF-e, a NFC-e e as NFS-e dos
//! municípios fora do padrão nacional (São Paulo e os do Betha).
//!
//! Garantias:
//! - **uma nota por DPS**: o número do DPS é reservado antes; se a resposta se perde, a
//!   consulta `GET /dps/{id}` recupera a nota, e um reenvio recusado por duplicidade também
//!   (a recusa sempre confere se a DPS já virou nota);
//! - **troca de leiaute sem parar**: se a Sefin recusar pelo esquema (E1235), a DPS é montada no
//!   outro leiaute 1.01 (vigente ↔ NT 009) e reenviada uma vez — e o processo passa a usar o que
//!   passou;
//! - o certificado é conferido no padrão ICP-Brasil antes de abrir a conexão.

pub(crate) mod assinatura;
mod cliente;
mod dps;
mod porta;
pub(crate) mod xml;

pub use porta::PortaComNfseNacional;

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

use cardeal_kernel::{Erro, Instante, Resultado};

pub use cliente::url_sefin;
pub use dps::{id_dps, montar_cancelamento, montar_dps, DpsMontada, LeiauteDps, NS_NFSE, VERSAO};

use crate::certificado::{abrir_para_assinar, ChaveDeAssinatura};
use crate::emissao::Emitente;
use crate::erros::ErroFiscal;
use crate::nfse::{CancelamentoNfse, Nfse, RetornoNfse, SituacaoNfse};
use crate::porta::AmbienteFiscal;
use crate::sefaz_api::CertificadoA1;

/// Os municípios que **não** emitem pela Sefin Nacional (sistema próprio ou Betha Cloud) — a
/// NFS-e deles segue pelo `sefaz-api`. Espelha a lista do `sefaz-api`.
const FORA_DO_PADRAO_NACIONAL: &[&str] = &[
    "3550308", // São Paulo/SP (Nota Fiscal Paulistana)
    // Betha Cloud (DPS própria)
    "4216402", "4205407", "4209102", "4204608", "4202404", "4204202", "4208203", "4211900",
    "4202008", "4208906", "4205902", "4201406", "4202305", "4202909", "4203006", "4204301",
    "4207502", "4209300", "4212502", "4216907", "4217002", "4217703", "4207601", "4304606",
    "4307005",
];

/// A NFS-e deste município sai pela Sefin Nacional?
#[must_use]
pub fn emite_pela_sefin_nacional(codigo_municipio: &str) -> bool {
    let c = xml::digitos(codigo_municipio);
    c.len() == 7 && !FORA_DO_PADRAO_NACIONAL.contains(&c.as_str())
}

/// A variável que força o leiaute (`vigente` ou `nt009`).
pub const VAR_LEIAUTE: &str = "CARDEAL_NFSE_LEIAUTE";

static LEIAUTE: [AtomicU8; 2] = [AtomicU8::new(0), AtomicU8::new(0)];

const fn indice(a: AmbienteFiscal) -> usize {
    match a {
        AmbienteFiscal::Producao => 0,
        AmbienteFiscal::Homologacao => 1,
    }
}

/// O leiaute em uso no ambiente: o que passou por último; senão o da variável; senão o NT 009
/// na produção restrita (que já valida nele) e o vigente na produção.
#[must_use]
pub fn leiaute_em_uso(ambiente: AmbienteFiscal) -> LeiauteDps {
    match LEIAUTE[indice(ambiente)].load(Ordering::Relaxed) {
        1 => LeiauteDps::Vigente,
        2 => LeiauteDps::Nt009,
        _ => match std::env::var(VAR_LEIAUTE)
            .map(|v| v.trim().to_lowercase())
            .as_deref()
        {
            Ok("nt009") => LeiauteDps::Nt009,
            Ok("vigente") => LeiauteDps::Vigente,
            _ if ambiente == AmbienteFiscal::Homologacao => LeiauteDps::Nt009,
            _ => LeiauteDps::Vigente,
        },
    }
}

fn lembrar_leiaute(ambiente: AmbienteFiscal, l: LeiauteDps) {
    let v = match l {
        LeiauteDps::Vigente => 1,
        LeiauteDps::Nt009 => 2,
    };
    LEIAUTE[indice(ambiente)].store(v, Ordering::Relaxed);
}

/// A DPS assinada, pronta para enviar (a mesma que a emissão manda — exposta para conferência e
/// para os testes contra o XSD).
///
/// # Errors
/// Os problemas de [`montar_dps`] (em frases) ou o erro da assinatura.
pub fn dps_assinada(
    nota: &Nfse,
    ambiente: AmbienteFiscal,
    leiaute: LeiauteDps,
    emitida_em: Instante,
    chave: &ChaveDeAssinatura,
) -> Result<(String, String), Vec<String>> {
    let dps = montar_dps(nota, ambiente, leiaute, emitida_em)?;
    let xml = assinatura::assinar_enveloped(
        "DPS",
        NS_NFSE,
        &[("versao", VERSAO)],
        &dps.inf_dps,
        &dps.id,
        chave,
    )
    .map_err(|e| vec![e.to_string()])?;
    Ok((dps.id, xml))
}

/// O pedido de cancelamento assinado.
///
/// # Errors
/// O erro da assinatura.
pub fn cancelamento_assinado(
    c: &CancelamentoNfse,
    cnpj_autor: &str,
    ambiente: AmbienteFiscal,
    pedido_em: Instante,
    chave: &ChaveDeAssinatura,
) -> Result<String, ErroFiscal> {
    let (id, inf) = montar_cancelamento(
        &c.chave,
        cnpj_autor,
        c.motivo,
        &c.justificativa,
        ambiente,
        pedido_em,
    );
    assinatura::assinar_enveloped(
        "pedRegEvento",
        NS_NFSE,
        &[("versao", VERSAO)],
        &inf,
        &id,
        chave,
    )
}

/// A emissão direta na Sefin Nacional para um certificado e um ambiente. O certificado só é
/// aberto (e a conexão montada) na primeira chamada.
pub struct NfseNacional {
    certificado: CertificadoA1,
    ambiente: AmbienteFiscal,
    aberto: OnceLock<Result<(ChaveDeAssinatura, cliente::ClienteSefin), ErroFiscal>>,
}

impl std::fmt::Debug for NfseNacional {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NfseNacional")
            .field("ambiente", &self.ambiente)
            .finish_non_exhaustive()
    }
}

fn rejeitada(mensagens: Vec<String>) -> RetornoNfse {
    RetornoNfse {
        situacao: SituacaoNfse::Rejeitada,
        numero: None,
        chave: None,
        codigo_verificacao: None,
        xml: None,
        mensagens,
    }
}

impl NfseNacional {
    /// Para o certificado e o ambiente.
    #[must_use]
    pub fn novo(certificado: CertificadoA1, ambiente: AmbienteFiscal) -> Self {
        Self {
            certificado,
            ambiente,
            aberto: OnceLock::new(),
        }
    }

    fn abrir(&self) -> Resultado<&(ChaveDeAssinatura, cliente::ClienteSefin)> {
        self.aberto
            .get_or_init(|| {
                let chave = abrir_para_assinar(self.certificado.pfx.expor(), self.certificado.senha.expor())?;
                let agora = Instante::agora();
                if chave.info.vencido(agora) {
                    return Err(ErroFiscal::CertificadoInvalido(
                        "o certificado digital está vencido — instale o novo em Configurações › Fiscal".into(),
                    ));
                }
                let cliente = cliente::ClienteSefin::novo(&chave, self.ambiente)?;
                Ok((chave, cliente))
            })
            .as_ref()
            .map_err(Erro::de_dominio)
    }

    /// A chave do A1 (aberta uma vez), para o que o Cardeal assina sozinho — a NFC-e em
    /// contingência local.
    pub(crate) fn chave_de_assinatura(&self) -> Resultado<&ChaveDeAssinatura> {
        self.abrir().map(|(chave, _)| chave)
    }

    /// O ambiente desta porta.
    pub(crate) const fn ambiente(&self) -> AmbienteFiscal {
        self.ambiente
    }

    /// Emite a NFS-e.
    ///
    /// # Errors
    /// O certificado não abre, ou a Sefin não respondeu (tentar de novo depois: a DPS reenviada
    /// é reconhecida, não duplica).
    pub fn emitir(&self, nota: &Nfse) -> Resultado<RetornoNfse> {
        let (chave, cliente) = self.abrir()?;
        let mut leiaute = leiaute_em_uso(self.ambiente);
        let mut trocou = false;
        loop {
            let (id, xml) = match dps_assinada(nota, self.ambiente, leiaute, emissao_agora(), chave)
            {
                Ok(x) => x,
                Err(problemas) => return Ok(rejeitada(problemas)),
            };
            let resposta = cliente.emitir(&xml).map_err(|e| Erro::de_dominio(&e))?;
            let r = cliente::interpretar_nfse(&resposta);
            if r.situacao != SituacaoNfse::Rejeitada {
                if r.situacao == SituacaoNfse::Autorizada {
                    lembrar_leiaute(self.ambiente, leiaute);
                }
                return Ok(r);
            }
            // Recusada: a DPS já tinha virado nota (resposta perdida, reenvio)? Então é ela.
            if let Ok(Some(achada)) = self.nota_da_dps(&id) {
                return Ok(achada);
            }
            let schema = cliente::codigos(&r.mensagens)
                .iter()
                .any(|c| matches!(*c, "E1235" | "RNG6110"));
            if schema && !trocou {
                tracing::warn!(
                    ?leiaute,
                    "Sefin recusou a DPS pelo esquema; tentando o outro leiaute 1.01"
                );
                leiaute = leiaute.outro();
                trocou = true;
                continue;
            }
            return Ok(r);
        }
    }

    /// A nota de uma DPS: `None` se ela não virou nota.
    fn nota_da_dps(&self, id: &str) -> Result<Option<RetornoNfse>, String> {
        let (_, cliente) = self.abrir().map_err(|e| e.mensagem)?;
        let resposta = cliente.chave_da_dps(id).map_err(|e| e.to_string())?;
        let Some(chave) = cliente::interpretar_chave_da_dps(&resposta)? else {
            return Ok(None);
        };
        let mut r = cliente.nfse(&chave).map_or_else(
            |_| rejeitada(Vec::new()),
            |resp| cliente::interpretar_nfse(&resp),
        );
        if r.situacao != SituacaoNfse::Autorizada {
            // A nota existe; o XML vem na próxima consulta.
            r = RetornoNfse {
                situacao: SituacaoNfse::Autorizada,
                numero: None,
                chave: None,
                codigo_verificacao: None,
                xml: None,
                mensagens: Vec::new(),
            };
        }
        r.chave.get_or_insert(chave);
        Ok(Some(r))
    }

    /// Procura a nota de uma DPS (a recuperação de uma emissão sem resposta). Não encontrada
    /// volta `Rejeitada` com a mensagem 404 (pode reenviar).
    ///
    /// # Errors
    /// O certificado não abre.
    pub fn consultar_por_dps(
        &self,
        prestador: &Emitente,
        serie: u32,
        numero: u64,
    ) -> Resultado<RetornoNfse> {
        let id = id_dps(
            &prestador.endereco.codigo_municipio,
            &prestador.cnpj.sem_mascara(),
            serie,
            numero,
        );
        match self.nota_da_dps(&id) {
            Ok(Some(r)) => Ok(r),
            Ok(None) => Ok(rejeitada(vec![
                "404 — a DPS não virou NFS-e na Sefin Nacional".into(),
            ])),
            Err(m) => Ok(RetornoNfse {
                situacao: SituacaoNfse::Erro,
                numero: None,
                chave: None,
                codigo_verificacao: None,
                xml: None,
                mensagens: vec![m],
            }),
        }
    }

    /// Cancela (evento 101101).
    ///
    /// # Errors
    /// O certificado não abre, ou a Sefin não respondeu.
    pub fn cancelar(&self, prestador: &Emitente, c: &CancelamentoNfse) -> Resultado<RetornoNfse> {
        let (chave, cliente) = self.abrir()?;
        if c.chave.len() != 50 {
            return Ok(rejeitada(vec![
                "a NFS-e não tem a chave de acesso de 50 dígitos guardada".into(),
            ]));
        }
        let xml = cancelamento_assinado(
            c,
            &prestador.cnpj.sem_mascara(),
            self.ambiente,
            emissao_agora(),
            chave,
        )
        .map_err(|e| Erro::de_dominio(&e))?;
        let resposta = cliente
            .evento(&c.chave, &xml)
            .map_err(|e| Erro::de_dominio(&e))?;
        Ok(cliente::interpretar_cancelamento(&resposta))
    }
}

/// A hora do `dhEmi`/`dhEvento`: um minuto antes de agora — a Sefin recusa documento "do futuro",
/// e o relógio da máquina pode estar adiantado —, sem voltar para o dia anterior (a competência é
/// a data de hoje e não pode passar da emissão).
fn emissao_agora() -> Instante {
    recuar_um_minuto(Instante::agora())
}

fn recuar_um_minuto(agora: Instante) -> Instante {
    let antes = Instante::de_micros(agora.em_micros() - 60_000_000);
    let fuso = cardeal_kernel::Fuso::BRASILIA;
    if antes.data(fuso) == agora.data(fuso) {
        antes
    } else {
        agora
    }
}

#[cfg(test)]
mod testes;
