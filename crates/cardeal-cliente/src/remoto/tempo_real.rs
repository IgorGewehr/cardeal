//! O tempo real do lado do cliente, **sem I/O**: o que um evento do servidor quer dizer
//! ([`AvisoTempoReal`]) e um leitor de `text/event-stream` ([`LeitorSse`]) para quem não tem
//! um `EventSource` (o desktop). O navegador usa o dele e só traduz com
//! [`AvisoTempoReal::de_evento`].

use cardeal_protocol::{
    modulos_alterados, EVENTO_MUDOU, EVENTO_RECARREGAR, EVENTO_SESSAO_ENCERRADA,
};

/// O que a tela faz com um evento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AvisoTempoReal {
    /// Estes módulos mudaram: recarregue o que estiver na tela deles.
    Mudou(Vec<String>),
    /// O fio se perdeu: recarregue tudo o que está na tela.
    Recarregar,
    /// A sessão caiu: volte ao login.
    SessaoEncerrada,
}

impl AvisoTempoReal {
    /// Traduz um evento (nome + `data`); `None` para o que não é aviso (comentário, batimento,
    /// evento desconhecido de um servidor mais novo).
    #[must_use]
    pub fn de_evento(nome: &str, data: &str) -> Option<Self> {
        match nome {
            EVENTO_MUDOU => Some(Self::Mudou(
                modulos_alterados(data).map(str::to_owned).collect(),
            )),
            EVENTO_RECARREGAR => Some(Self::Recarregar),
            EVENTO_SESSAO_ENCERRADA => Some(Self::SessaoEncerrada),
            _ => None,
        }
    }

    /// Se a tela do `modulo` precisa recarregar.
    #[must_use]
    pub fn afeta(&self, modulo: &str) -> bool {
        match self {
            Self::Mudou(m) => m.iter().any(|x| x == modulo),
            Self::Recarregar => true,
            Self::SessaoEncerrada => false,
        }
    }
}

/// Lê `text/event-stream` em pedaços de qualquer tamanho (uma linha pode chegar partida, até
/// no meio de um caractere) e guarda o último `id` — o `Last-Event-ID` da reconexão.
#[derive(Debug, Default)]
pub struct LeitorSse {
    pendente: Vec<u8>,
    evento: String,
    data: String,
    ultimo_id: Option<u64>,
}

impl LeitorSse {
    /// Um leitor que continua de `ultimo_id` (se a conexão anterior já tinha um).
    #[must_use]
    pub fn continuando(ultimo_id: Option<u64>) -> Self {
        Self {
            ultimo_id,
            ..Self::default()
        }
    }

    /// O ponto para reconectar.
    #[must_use]
    pub const fn ultimo_id(&self) -> Option<u64> {
        self.ultimo_id
    }

    /// Consome bytes; devolve os avisos completos que eles fecharam.
    pub fn alimentar(&mut self, bytes: &[u8]) -> Vec<AvisoTempoReal> {
        self.pendente.extend_from_slice(bytes);
        let mut avisos = Vec::new();
        // `\n` nunca aparece dentro de um caractere UTF-8: cortar nele é seguro.
        while let Some(fim) = self.pendente.iter().position(|&b| b == b'\n') {
            let linha: Vec<u8> = self.pendente.drain(..=fim).collect();
            let linha = String::from_utf8_lossy(&linha);
            let linha = linha.trim_end_matches(['\n', '\r']);
            if linha.is_empty() {
                let evento = std::mem::take(&mut self.evento);
                let data = std::mem::take(&mut self.data);
                if !evento.is_empty() {
                    avisos.extend(AvisoTempoReal::de_evento(&evento, &data));
                }
                continue;
            }
            if linha.starts_with(':') {
                continue;
            }
            let (campo, valor) = linha.split_once(':').unwrap_or((linha, ""));
            let valor = valor.strip_prefix(' ').unwrap_or(valor);
            match campo {
                "event" => valor.clone_into(&mut self.evento),
                "data" => {
                    if !self.data.is_empty() {
                        self.data.push('\n');
                    }
                    self.data.push_str(valor);
                }
                "id" => {
                    if let Ok(n) = valor.parse() {
                        self.ultimo_id = Some(n);
                    }
                }
                _ => {}
            }
        }
        avisos
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn le_eventos_partidos_em_qualquer_ponto_e_guarda_o_ultimo_id() {
        let fluxo = ": pronto\nid: 7\n\n: \n\nid: 9\nevent: mudou\ndata: clientes,os\n\nevent: recarregar\nid: 30\ndata: 1\n\nevent: novo_do_futuro\ndata: x\n\n";
        // Byte a byte: o pior caso de fragmentação.
        let mut l = LeitorSse::default();
        let mut avisos = Vec::new();
        for b in fluxo.as_bytes() {
            avisos.extend(l.alimentar(std::slice::from_ref(b)));
        }
        assert_eq!(
            avisos,
            vec![
                AvisoTempoReal::Mudou(vec!["clientes".into(), "os".into()]),
                AvisoTempoReal::Recarregar
            ]
        );
        assert_eq!(l.ultimo_id(), Some(30));
        assert!(avisos[0].afeta("os") && !avisos[0].afeta("financeiro"));
        assert!(avisos[1].afeta("qualquer"));
    }

    #[test]
    fn crlf_e_utf8_partido() {
        let mut l = LeitorSse::continuando(Some(3));
        let bytes = "event: mudou\r\ndata: ação\r\n\r\n".as_bytes();
        let (a, b) = bytes.split_at(20); // no meio do "ç"
        assert!(l.alimentar(a).is_empty());
        assert_eq!(
            l.alimentar(b),
            vec![AvisoTempoReal::Mudou(vec!["ação".into()])]
        );
        assert_eq!(l.ultimo_id(), Some(3));
    }
}
