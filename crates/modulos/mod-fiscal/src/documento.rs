//! O documento fiscal como agregado próprio (`docs/10` §3): o estado, o resumo para as telas e a
//! **decisão** do que fazer com cada resposta da SEFAZ — a parte que não pode errar, por isso
//! pura e testada caso a caso.
//!
//! ```text
//! Pendente ──emitir──▶ Autorizada | Rejeitada | Denegada
//!    │  ▲                  (resposta sem resultado: Incerta)
//!    │  └──── 217 "não consta" ◀── Incerta ──consultar──▶ Autorizada | Cancelada | Denegada
//!    └── API fora do ar: continua Pendente, tenta de novo mais tarde (30 s → 15 min)
//! Rejeitada ──corrigir e reenviar──▶ Pendente (mesmo número: a rejeição não o consome)
//! ```
//!
//! **Nunca reenviar às cegas:** o `sefaz-api` sorteia o código numérico a cada emissão, então
//! reenviar o mesmo número gera outra chave — se a primeira foi autorizada e a resposta se
//! perdeu, a SEFAZ rejeita por duplicidade (539). Por isso a resposta incerta vira `Incerta` e
//! é **consultada pela chave** antes de qualquer nova emissão; e a própria 539 traz a chave da
//! nota que já existe, que passa a ser consultada.

use cardeal_fiscal::{AmbienteFiscal, ModeloFiscal, RetornoSefaz, SituacaoRetorno};
use cardeal_kernel::{Dinheiro, Id, Instante};
use serde::{Deserialize, Serialize};

use crate::explicacao::explicar;

/// O estado de um documento fiscal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EstadoDocumento {
    /// Na fila para transmitir.
    Pendente,
    /// Enviado, mas o resultado não chegou — consultar pela chave antes de reenviar.
    Incerta,
    /// Autorizada.
    Autorizada,
    /// Rejeitada (pela SEFAZ ou antes, por um problema nos dados) — corrigir e reenviar.
    Rejeitada,
    /// Uso denegado — o número foi consumido.
    Denegada,
    /// Cancelada.
    Cancelada,
    /// O número foi inutilizado (a nota nunca saiu).
    Inutilizada,
}

/// Um pedido sobre um documento, que a fila leva à SEFAZ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PedidoDocumento {
    /// Cancelar a nota autorizada.
    Cancelar,
    /// Inutilizar o número de uma nota que nunca foi autorizada.
    Inutilizar,
}

impl PedidoDocumento {
    /// O texto guardado na base.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::Cancelar => "cancelar",
            Self::Inutilizar => "inutilizar",
        }
    }

    /// O contrário de [`Self::como_str`].
    #[must_use]
    pub fn de_str(s: &str) -> Option<Self> {
        match s {
            "cancelar" => Some(Self::Cancelar),
            "inutilizar" => Some(Self::Inutilizar),
            _ => None,
        }
    }

    /// Para a tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Cancelar => "cancelamento na fila",
            Self::Inutilizar => "inutilização na fila",
        }
    }
}

impl EstadoDocumento {
    /// O texto guardado na base.
    #[must_use]
    pub const fn como_str(self) -> &'static str {
        match self {
            Self::Pendente => "Pendente",
            Self::Incerta => "Incerta",
            Self::Autorizada => "Autorizada",
            Self::Rejeitada => "Rejeitada",
            Self::Denegada => "Denegada",
            Self::Cancelada => "Cancelada",
            Self::Inutilizada => "Inutilizada",
        }
    }

    /// O contrário de [`Self::como_str`].
    #[must_use]
    pub fn de_str(s: &str) -> Option<Self> {
        Some(match s {
            "Pendente" => Self::Pendente,
            "Incerta" => Self::Incerta,
            "Autorizada" => Self::Autorizada,
            "Rejeitada" => Self::Rejeitada,
            "Denegada" => Self::Denegada,
            "Cancelada" => Self::Cancelada,
            "Inutilizada" => Self::Inutilizada,
            _ => return None,
        })
    }

    /// Para a tela.
    #[must_use]
    pub const fn rotulo(self) -> &'static str {
        match self {
            Self::Pendente => "Na fila",
            Self::Incerta => "Conferindo",
            Self::Autorizada => "Autorizada",
            Self::Rejeitada => "Rejeitada",
            Self::Denegada => "Denegada",
            Self::Cancelada => "Cancelada",
            Self::Inutilizada => "Inutilizada",
        }
    }
}

/// Um documento fiscal, para listas e detalhes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentoResumo {
    /// Identidade.
    pub id: Id,
    /// O modelo.
    pub modelo: ModeloFiscal,
    /// A série.
    pub serie: u32,
    /// O número.
    pub numero: u64,
    /// O ambiente em que foi (ou vai ser) emitido.
    pub ambiente: AmbienteFiscal,
    /// O estado.
    pub estado: EstadoDocumento,
    /// O total da nota.
    pub total: Dinheiro,
    /// A chave de acesso.
    pub chave: Option<String>,
    /// O protocolo de autorização.
    pub protocolo: Option<String>,
    /// O último código da SEFAZ (ou do provedor).
    pub codigo: Option<String>,
    /// A última mensagem, já explicada.
    pub motivo: Option<String>,
    /// O que originou o documento (o cupom do PDV).
    pub origem: Id,
    /// Quantas tentativas de transmissão.
    pub tentativas: u32,
    /// Quando foi criado.
    pub criado_em: Instante,
    /// Quando foi autorizado.
    pub autorizado_em: Option<Instante>,
    /// Um cancelamento ou inutilização esperando a fila.
    pub pedido: Option<PedidoDocumento>,
    /// A venda foi desfeita (estorno no PDV): a fila leva a nota ao cancelamento ou à
    /// inutilização, conforme o estado em que ela estiver.
    pub anulada: bool,
}

impl DocumentoResumo {
    /// A venda foi desfeita, mas a nota ainda vale na SEFAZ e nada mais está em curso para
    /// mudar isso — o cancelamento foi recusado (fora do prazo, em geral). Pede uma NFC-e de
    /// devolução ou a ajuda do contador.
    #[must_use]
    pub fn anulacao_pendente(&self) -> bool {
        self.anulada && self.estado == EstadoDocumento::Autorizada && self.pedido.is_none()
    }
}

/// O pedido que a anulação gera para um documento no estado `estado` — `None` quando não há
/// o que pedir agora: a incerta e a pendente já tentada esperam a fila resolver (emitir ou
/// conferir) e só então recebem o pedido; a denegada, a cancelada e a inutilizada já acabaram.
/// A pendente que **nunca** foi transmitida não saiu desta máquina: inutiliza-se o número.
#[must_use]
pub const fn pedido_da_anulacao(
    estado: EstadoDocumento,
    tentativas: u32,
) -> Option<PedidoDocumento> {
    match estado {
        EstadoDocumento::Autorizada => Some(PedidoDocumento::Cancelar),
        EstadoDocumento::Rejeitada => Some(PedidoDocumento::Inutilizar),
        EstadoDocumento::Pendente if tentativas == 0 => Some(PedidoDocumento::Inutilizar),
        _ => None,
    }
}

/// O resumo da fila fiscal — a faixa de estado do PDV (`docs/10` §4).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumoFiscal {
    /// Na fila.
    pub pendentes: u32,
    /// Conferindo.
    pub incertas: u32,
    /// Rejeitadas (exigem correção).
    pub rejeitadas: u32,
    /// Autorizadas hoje.
    pub autorizadas_hoje: u32,
    /// Desde quando há documento esperando (o mais antigo na fila ou conferindo).
    pub esperando_desde: Option<Instante>,
}

impl ResumoFiscal {
    /// A frase da faixa de estado.
    #[must_use]
    pub fn frase(&self) -> String {
        if self.rejeitadas > 0 {
            format!("{} nota(s) rejeitada(s) exigem correção", self.rejeitadas)
        } else if self.pendentes + self.incertas > 0 {
            format!(
                "{} nota(s) na fila — transmitindo",
                self.pendentes + self.incertas
            )
        } else {
            "Fiscal em dia".to_string()
        }
    }
}

/// O que aconteceu numa tentativa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desfecho {
    /// A API respondeu (autorizando, rejeitando, ou sem resultado).
    Resposta(RetornoSefaz),
    /// A API não respondeu: tentar de novo depois.
    Indisponivel(String),
    /// A API (ou a instalação) recusou o pedido: precisa de correção.
    Recusado(String),
}

/// O que gravar depois de uma tentativa.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mudanca {
    /// O novo estado.
    pub estado: EstadoDocumento,
    /// A chave (`None` = apagar; só muda quando [`Self::trocar_chave`]).
    pub chave: Option<String>,
    /// Se a chave deve ser gravada (`chave`) ou mantida.
    pub trocar_chave: bool,
    /// O protocolo, quando autorizado.
    pub protocolo: Option<String>,
    /// O XML autorizado, quando veio.
    pub xml: Option<String>,
    /// O código para a tela.
    pub codigo: Option<String>,
    /// A mensagem para a tela.
    pub motivo: Option<String>,
    /// Quanto esperar até a próxima tentativa (segundos); 0 = já.
    pub espera_segundos: i64,
    /// O pedido (cancelar/inutilizar) foi resolvido — aceito ou recusado.
    pub limpar_pedido: bool,
    /// O protocolo do evento (cancelamento/inutilização) aceito.
    pub protocolo_evento: Option<String>,
}

/// Depois de tantas tentativas com erro do emissor (não de rede), a nota sai da fila como
/// rejeitada, com o motivo — um erro que se repete não se resolve sozinho.
pub const TENTATIVAS_COM_ERRO: u32 = 6;

/// A espera antes da próxima tentativa: 30 s, 1, 2, 4, 8 min... até 15 min.
#[must_use]
pub fn espera(tentativas: u32) -> i64 {
    (30_i64 << tentativas.min(5)).min(15 * 60)
}

fn chave_no_motivo(motivo: &str) -> Option<String> {
    let i = motivo.find("chNFe:")? + "chNFe:".len();
    let digitos: String = motivo[i..]
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(char::is_ascii_digit)
        .collect();
    (digitos.len() == 44).then_some(digitos)
}

/// Decide o que fazer com o desfecho de uma tentativa sobre um documento `estado`
/// (`Pendente` emite; `Incerta` consulta).
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn decidir(estado: EstadoDocumento, tentativas: u32, desfecho: Desfecho) -> Mudanca {
    let base = |estado| Mudanca {
        estado,
        chave: None,
        trocar_chave: false,
        protocolo: None,
        xml: None,
        codigo: None,
        motivo: None,
        espera_segundos: 0,
        limpar_pedido: false,
        protocolo_evento: None,
    };
    let r = match desfecho {
        Desfecho::Indisponivel(m) => {
            return Mudanca {
                motivo: Some(m),
                espera_segundos: espera(tentativas),
                ..base(estado)
            }
        }
        Desfecho::Recusado(m) => {
            return Mudanca {
                motivo: Some(m),
                ..base(EstadoDocumento::Rejeitada)
            }
        }
        Desfecho::Resposta(r) => r,
    };
    let motivo = Some(explicar(&r.codigo, &r.motivo));
    let codigo = Some(r.codigo.clone());
    let incerta = |chave: Option<String>, espera_segundos| Mudanca {
        chave,
        trocar_chave: true,
        codigo: codigo.clone(),
        motivo: motivo.clone(),
        espera_segundos,
        ..base(EstadoDocumento::Incerta)
    };
    let final_ = |estado| Mudanca {
        chave: r.chave.clone(),
        trocar_chave: r.chave.is_some(),
        codigo: codigo.clone(),
        motivo: motivo.clone(),
        ..base(estado)
    };
    // SEFAZ paralisada (108/109): como API fora do ar.
    if matches!(r.codigo.as_str(), "108" | "109") {
        return Mudanca {
            codigo,
            motivo,
            espera_segundos: espera(tentativas),
            ..base(estado)
        };
    }
    if estado == EstadoDocumento::Incerta {
        return match r.situacao {
            _ if matches!(r.codigo.as_str(), "101" | "151" | "155") => {
                final_(EstadoDocumento::Cancelada)
            }
            SituacaoRetorno::Autorizado => Mudanca {
                protocolo: r.protocolo.clone(),
                xml: r.xml.clone(),
                ..final_(EstadoDocumento::Autorizada)
            },
            SituacaoRetorno::Rejeitado if r.codigo == "217" => Mudanca {
                // Não consta: a emissão anterior não chegou. Emite de novo, já.
                chave: None,
                trocar_chave: true,
                codigo,
                motivo,
                ..base(EstadoDocumento::Pendente)
            },
            SituacaoRetorno::Denegado => final_(EstadoDocumento::Denegada),
            SituacaoRetorno::Processando | SituacaoRetorno::Erro => Mudanca {
                codigo,
                motivo,
                espera_segundos: espera(tentativas),
                ..base(EstadoDocumento::Incerta)
            },
            SituacaoRetorno::Rejeitado | SituacaoRetorno::Cancelado => {
                final_(EstadoDocumento::Rejeitada)
            }
        };
    }
    match r.situacao {
        SituacaoRetorno::Autorizado => Mudanca {
            protocolo: r.protocolo.clone(),
            xml: r.xml.clone(),
            ..final_(EstadoDocumento::Autorizada)
        },
        // Duplicidade: o número já tem nota na SEFAZ — a da chave que veio no motivo (539) ou
        // a nossa (204). Conferir antes de qualquer coisa.
        SituacaoRetorno::Rejeitado if r.codigo == "539" => match chave_no_motivo(&r.motivo) {
            Some(chave) => incerta(Some(chave), 0),
            None => final_(EstadoDocumento::Rejeitada),
        },
        SituacaoRetorno::Rejeitado if r.codigo == "204" && r.chave.is_some() => {
            incerta(r.chave.clone(), 0)
        }
        SituacaoRetorno::Rejeitado | SituacaoRetorno::Cancelado => {
            final_(EstadoDocumento::Rejeitada)
        }
        SituacaoRetorno::Denegado => final_(EstadoDocumento::Denegada),
        SituacaoRetorno::Erro if tentativas + 1 >= TENTATIVAS_COM_ERRO => Mudanca {
            motivo: Some(format!(
                "o emissor falhou {} vezes seguidas — {}",
                tentativas + 1,
                explicar(&r.codigo, &r.motivo)
            )),
            ..final_(EstadoDocumento::Rejeitada)
        },
        SituacaoRetorno::Processando | SituacaoRetorno::Erro => match r.chave.clone() {
            Some(chave) => incerta(Some(chave), espera(0)),
            None if r.situacao == SituacaoRetorno::Processando => Mudanca {
                codigo,
                motivo,
                espera_segundos: espera(tentativas),
                ..base(EstadoDocumento::Pendente)
            },
            None => final_(EstadoDocumento::Rejeitada),
        },
    }
}

/// Decide o que fazer com o desfecho de um pedido (cancelar uma autorizada, inutilizar o
/// número de uma rejeitada). Aceito, o documento muda de estado; recusado, volta ao que era,
/// sem o pedido e com o motivo; sem resposta, o pedido espera a próxima rodada.
#[must_use]
pub fn decidir_evento(
    pedido: PedidoDocumento,
    estado: EstadoDocumento,
    tentativas: u32,
    desfecho: Desfecho,
) -> Mudanca {
    let base = Mudanca {
        estado,
        chave: None,
        trocar_chave: false,
        protocolo: None,
        xml: None,
        codigo: None,
        motivo: None,
        espera_segundos: 0,
        limpar_pedido: false,
        protocolo_evento: None,
    };
    let (aceito, ja_feito, nome) = match pedido {
        PedidoDocumento::Cancelar => (
            EstadoDocumento::Cancelada,
            &["573", "218"][..],
            "O cancelamento",
        ),
        PedidoDocumento::Inutilizar => (
            EstadoDocumento::Inutilizada,
            &["563", "206", "256"][..],
            "A inutilização",
        ),
    };
    let r = match desfecho {
        Desfecho::Indisponivel(m) => {
            return Mudanca {
                motivo: Some(m),
                espera_segundos: espera(tentativas),
                ..base
            }
        }
        Desfecho::Recusado(m) => {
            return Mudanca {
                motivo: Some(format!("{nome} não foi aceito: {m}")),
                limpar_pedido: true,
                ..base
            }
        }
        Desfecho::Resposta(r) => r,
    };
    let codigo = Some(r.codigo.clone());
    match r.situacao {
        SituacaoRetorno::Autorizado => Mudanca {
            estado: aceito,
            codigo,
            motivo: Some(explicar(&r.codigo, &r.motivo)),
            limpar_pedido: true,
            protocolo_evento: r.protocolo.clone().filter(|p| !p.is_empty()),
            ..base
        },
        SituacaoRetorno::Rejeitado if ja_feito.contains(&r.codigo.as_str()) => Mudanca {
            estado: aceito,
            codigo,
            motivo: Some(explicar(&r.codigo, &r.motivo)),
            limpar_pedido: true,
            ..base
        },
        SituacaoRetorno::Rejeitado | SituacaoRetorno::Denegado | SituacaoRetorno::Cancelado => {
            Mudanca {
                codigo,
                motivo: Some(format!(
                    "{nome} não foi aceito — {}",
                    explicar(&r.codigo, &r.motivo)
                )),
                limpar_pedido: true,
                ..base
            }
        }
        SituacaoRetorno::Processando | SituacaoRetorno::Erro => Mudanca {
            codigo,
            motivo: Some(explicar(&r.codigo, &r.motivo)),
            espera_segundos: espera(tentativas),
            ..base
        },
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const CHAVE: &str = "43261011222333000181650010000000421100000001";

    fn r(situacao: SituacaoRetorno, codigo: &str, chave: Option<&str>) -> Desfecho {
        Desfecho::Resposta(RetornoSefaz {
            situacao,
            codigo: codigo.into(),
            motivo: format!("motivo {codigo}"),
            chave: chave.map(Into::into),
            protocolo: Some("143".into()),
            recebido_em: None,
            xml: Some("<nfeProc/>".into()),
            erros: vec![],
        })
    }

    use EstadoDocumento::{Autorizada, Cancelada, Denegada, Incerta, Pendente, Rejeitada};
    use SituacaoRetorno::{Autorizado, Cancelado, Denegado, Erro, Processando, Rejeitado};

    #[test]
    fn emissao_autoriza_rejeita_e_denega() {
        let m = decidir(Pendente, 0, r(Autorizado, "100", Some(CHAVE)));
        assert_eq!(m.estado, Autorizada);
        assert_eq!(m.chave.as_deref(), Some(CHAVE));
        assert_eq!(m.xml.as_deref(), Some("<nfeProc/>"));
        assert_eq!(
            decidir(Pendente, 0, r(Rejeitado, "778", None)).estado,
            Rejeitada
        );
        assert_eq!(
            decidir(Pendente, 0, r(Denegado, "302", Some(CHAVE))).estado,
            Denegada
        );
        assert_eq!(
            decidir(Pendente, 0, r(Cancelado, "101", None)).estado,
            Rejeitada
        );
    }

    #[test]
    fn resposta_sem_resultado_vira_incerta_e_nunca_reemite_as_cegas() {
        let m = decidir(Pendente, 0, r(Processando, "TIMEOUT", Some(CHAVE)));
        assert_eq!((m.estado, m.chave.as_deref()), (Incerta, Some(CHAVE)));
        assert_eq!(m.espera_segundos, 30);
        assert_eq!(
            decidir(Pendente, 0, r(Erro, "ERR", Some(CHAVE))).estado,
            Incerta
        );
        // Erro que se repete: sai da fila como rejeitada (não gira para sempre).
        let cansou = decidir(
            Pendente,
            TENTATIVAS_COM_ERRO - 1,
            r(Erro, "ERR", Some(CHAVE)),
        );
        assert_eq!(cansou.estado, Rejeitada);
        assert!(cansou.motivo.unwrap().contains("6 vezes"));
        // Erro sem chave: falhou antes de enviar — precisa de atenção.
        assert_eq!(decidir(Pendente, 0, r(Erro, "ERR", None)).estado, Rejeitada);
        // Processando sem chave: continua na fila.
        assert_eq!(
            decidir(Pendente, 2, r(Processando, "105", None)).estado,
            Pendente
        );
    }

    #[test]
    fn duplicidade_passa_a_conferir_a_chave_que_ja_existe() {
        let desfecho = Desfecho::Resposta(RetornoSefaz {
            situacao: Rejeitado,
            codigo: "539".into(),
            motivo: format!("Rejeição: Duplicidade de NF-e com diferença na Chave de Acesso [chNFe: {CHAVE}][nRec:1]"),
            chave: Some("9".repeat(44)),
            protocolo: None,
            recebido_em: None,
            xml: None,
            erros: vec![],
        });
        let m = decidir(Pendente, 0, desfecho);
        assert_eq!(m.estado, Incerta);
        assert_eq!(m.chave.as_deref(), Some(CHAVE));
        assert_eq!(m.espera_segundos, 0);
        // 539 sem a chave no motivo: não há o que conferir.
        assert_eq!(
            decidir(Pendente, 0, r(Rejeitado, "539", None)).estado,
            Rejeitada
        );
        assert_eq!(
            decidir(Pendente, 0, r(Rejeitado, "204", Some(CHAVE))).estado,
            Incerta
        );
    }

    #[test]
    fn consulta_resolve_a_incerta() {
        let m = decidir(Incerta, 1, r(Autorizado, "100", Some(CHAVE)));
        assert_eq!(m.estado, Autorizada);
        assert_eq!(m.protocolo.as_deref(), Some("143"));
        let nao_consta = decidir(Incerta, 1, r(Rejeitado, "217", Some(CHAVE)));
        assert_eq!(nao_consta.estado, Pendente);
        assert!(nao_consta.trocar_chave && nao_consta.chave.is_none());
        assert_eq!(nao_consta.espera_segundos, 0);
        // O sefaz-api devolve "autorizado" para o 101 da consulta: é cancelada.
        assert_eq!(
            decidir(Incerta, 1, r(Autorizado, "101", Some(CHAVE))).estado,
            Cancelada
        );
        let ainda = decidir(Incerta, 3, r(Erro, "ERR", None));
        assert_eq!((ainda.estado, ainda.espera_segundos), (Incerta, 240));
        assert_eq!(
            decidir(Incerta, 1, r(Denegado, "301", Some(CHAVE))).estado,
            Denegada
        );
    }

    #[test]
    fn fora_do_ar_espera_e_recusa_pede_correcao() {
        let m = decidir(Pendente, 4, Desfecho::Indisponivel("sem internet".into()));
        assert_eq!((m.estado, m.espera_segundos), (Pendente, 480));
        assert_eq!(espera(10), 900);
        let m = decidir(Incerta, 0, Desfecho::Indisponivel("x".into()));
        assert_eq!(m.estado, Incerta);
        let m = decidir(
            Pendente,
            0,
            Desfecho::Recusado("CNPJ do certificado".into()),
        );
        assert_eq!(m.estado, Rejeitada);
        // SEFAZ paralisada: como fora do ar.
        let m = decidir(Pendente, 1, r(Rejeitado, "108", None));
        assert_eq!((m.estado, m.espera_segundos), (Pendente, 60));
    }

    #[test]
    fn cancelamento_e_inutilizacao() {
        use PedidoDocumento::{Cancelar, Inutilizar};
        let m = decidir_evento(Cancelar, Autorizada, 0, r(Autorizado, "135", Some(CHAVE)));
        assert_eq!((m.estado, m.limpar_pedido), (Cancelada, true));
        assert_eq!(m.protocolo_evento.as_deref(), Some("143"));
        // Já cancelada antes (duplicidade de evento): está cancelada.
        assert_eq!(
            decidir_evento(Cancelar, Autorizada, 0, r(Rejeitado, "573", None)).estado,
            Cancelada
        );
        // Fora do prazo: continua autorizada, sem o pedido, com o motivo.
        let prazo = decidir_evento(Cancelar, Autorizada, 0, r(Rejeitado, "501", None));
        assert_eq!((prazo.estado, prazo.limpar_pedido), (Autorizada, true));
        assert!(prazo
            .motivo
            .unwrap()
            .starts_with("O cancelamento não foi aceito"));
        // Sem resposta: o pedido espera.
        let fora = decidir_evento(Cancelar, Autorizada, 2, Desfecho::Indisponivel("x".into()));
        assert_eq!(
            (fora.estado, fora.limpar_pedido, fora.espera_segundos),
            (Autorizada, false, 120)
        );

        let m = decidir_evento(Inutilizar, Rejeitada, 0, r(Autorizado, "102", None));
        assert_eq!(
            (m.estado, m.limpar_pedido),
            (EstadoDocumento::Inutilizada, true)
        );
        assert_eq!(
            decidir_evento(Inutilizar, Rejeitada, 0, r(Rejeitado, "563", None)).estado,
            EstadoDocumento::Inutilizada
        );
        let recusada = decidir_evento(Inutilizar, Rejeitada, 0, r(Rejeitado, "241", None));
        assert_eq!((recusada.estado, recusada.limpar_pedido), (Rejeitada, true));
    }

    #[test]
    fn a_anulacao_pede_conforme_o_estado() {
        use PedidoDocumento::{Cancelar, Inutilizar};
        assert_eq!(pedido_da_anulacao(Autorizada, 3), Some(Cancelar));
        assert_eq!(pedido_da_anulacao(Rejeitada, 1), Some(Inutilizar));
        assert_eq!(pedido_da_anulacao(Pendente, 0), Some(Inutilizar));
        // Já tentada: pode ter chegado à SEFAZ sem resposta — a fila emite (a duplicidade
        // revela a chave) e só então cancela.
        assert_eq!(pedido_da_anulacao(Pendente, 1), None);
        assert_eq!(pedido_da_anulacao(Incerta, 0), None);
        assert_eq!(pedido_da_anulacao(Denegada, 0), None);
        assert_eq!(pedido_da_anulacao(Cancelada, 0), None);
    }

    #[test]
    fn frase_da_faixa() {
        let mut r = ResumoFiscal::default();
        assert_eq!(r.frase(), "Fiscal em dia");
        r.pendentes = 2;
        r.incertas = 1;
        assert!(r.frase().starts_with("3 nota(s) na fila"));
        r.rejeitadas = 1;
        assert!(r.frase().contains("rejeitada"));
    }
}
